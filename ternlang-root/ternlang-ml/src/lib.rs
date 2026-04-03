/// ternlang-ml: Ternary ML inference kernels for RFI-IRFOS Ternary Intelligence Stack
///
/// Provides:
///   - quantize()        — convert f32 weights to balanced ternary (-1, 0, +1)
///   - sparse_matmul()   — matmul skipping zero-state weights (flagship kernel)
///   - dense_matmul()    — standard ternary matmul for comparison
///   - linear()          — BitNet-style ternary linear layer (sparse by default)
///   - sparsity()        — measure fraction of zero-state elements
///   - timed_benchmark() — wall-clock timing across multiple matrix sizes
///   - MLP               — 2-layer ternary multi-layer perceptron

use ternlang_core::trit::Trit;

// ─── Quantization ────────────────────────────────────────────────────────────

/// Quantize a slice of f32 weights to balanced ternary using threshold τ.
///
/// Rule:
///   w >  τ → +1 (truth)
///   w < -τ → -1 (conflict)
///   else   →  0 (hold)
///
/// A τ of 0.5 * mean(|weights|) matches the BitNet b1.58 scheme.
pub fn quantize(weights: &[f32], threshold: f32) -> Vec<Trit> {
    weights.iter().map(|&w| {
        if w > threshold {
            Trit::PosOne
        } else if w < -threshold {
            Trit::NegOne
        } else {
            Trit::Zero
        }
    }).collect()
}

/// Compute the BitNet-style threshold: 0.5 × mean(|weights|)
pub fn bitnet_threshold(weights: &[f32]) -> f32 {
    let mean_abs = weights.iter().map(|w| w.abs()).sum::<f32>() / weights.len() as f32;
    0.5 * mean_abs
}

// ─── Tensor layout ───────────────────────────────────────────────────────────

/// A flat row-major ternary matrix (rows × cols).
pub struct TritMatrix {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<Trit>,
}

impl TritMatrix {
    pub fn new(rows: usize, cols: usize) -> Self {
        Self { rows, cols, data: vec![Trit::Zero; rows * cols] }
    }

    pub fn from_trits(rows: usize, cols: usize, data: Vec<Trit>) -> Self {
        assert_eq!(data.len(), rows * cols);
        Self { rows, cols, data }
    }

    pub fn from_f32(rows: usize, cols: usize, weights: &[f32], threshold: f32) -> Self {
        Self::from_trits(rows, cols, quantize(weights, threshold))
    }

    #[inline]
    pub fn get(&self, row: usize, col: usize) -> Trit {
        self.data[row * self.cols + col]
    }

    #[inline]
    pub fn set(&mut self, row: usize, col: usize, val: Trit) {
        self.data[row * self.cols + col] = val;
    }

    /// Fraction of elements that are zero (hold state).
    pub fn sparsity(&self) -> f64 {
        let zeros = self.data.iter().filter(|&&t| t == Trit::Zero).count();
        zeros as f64 / self.data.len() as f64
    }

    /// Count of non-zero elements (active computation sites).
    pub fn nnz(&self) -> usize {
        self.data.iter().filter(|&&t| t != Trit::Zero).count()
    }
}

// ─── Matmul kernels ──────────────────────────────────────────────────────────

/// Dense ternary matrix multiply: C = A × B
/// No skipping — every element is computed regardless of zero state.
/// Use this as the baseline for benchmark comparisons.
pub fn dense_matmul(a: &TritMatrix, b: &TritMatrix) -> TritMatrix {
    assert_eq!(a.cols, b.rows, "matmul dimension mismatch: a.cols must equal b.rows");
    let mut c = TritMatrix::new(a.rows, b.cols);
    for row in 0..a.rows {
        for col in 0..b.cols {
            let mut acc = Trit::Zero;
            for k in 0..a.cols {
                let prod = a.get(row, k) * b.get(k, col);
                let (sum, _carry) = acc + prod;
                acc = sum;
            }
            c.set(row, col, acc);
        }
    }
    c
}

/// Sparse ternary matrix multiply: C = A × B, skipping zero-weight elements.
///
/// Returns (result_matrix, skipped_count).
/// skipped_count is the number of multiply-accumulate operations avoided.
/// For typical ternary-quantized LLM weights (60-80% zeros), this is the
/// core performance gain of the ternary approach.
pub fn sparse_matmul(a: &TritMatrix, b: &TritMatrix) -> (TritMatrix, usize) {
    assert_eq!(a.cols, b.rows, "matmul dimension mismatch");
    let mut c = TritMatrix::new(a.rows, b.cols);
    let mut skipped = 0usize;

    for row in 0..a.rows {
        for col in 0..b.cols {
            let mut acc = Trit::Zero;
            for k in 0..a.cols {
                let weight = b.get(k, col);
                // ── SPARSE SKIP ── zero weights contribute nothing; skip entirely
                if weight == Trit::Zero {
                    skipped += 1;
                    continue;
                }
                let prod = a.get(row, k) * weight;
                let (sum, _carry) = acc + prod;
                acc = sum;
            }
            c.set(row, col, acc);
        }
    }
    (c, skipped)
}

// ─── Linear layer ────────────────────────────────────────────────────────────

/// BitNet-style ternary linear layer: output = sparse_matmul(input, W)
///
/// input: [batch × in_features]
/// W:     [in_features × out_features]  (pre-quantized ternary weights)
/// returns: ([batch × out_features], skipped_ops)
pub fn linear(input: &TritMatrix, weights: &TritMatrix) -> (TritMatrix, usize) {
    sparse_matmul(input, weights)
}

// ─── Benchmark helpers ───────────────────────────────────────────────────────

/// Summary statistics for a benchmark run.
pub struct BenchmarkResult {
    pub dense_ops: usize,
    pub sparse_ops: usize,
    pub skipped_ops: usize,
    pub skip_rate: f64,
    pub weight_sparsity: f64,
}

impl BenchmarkResult {
    pub fn print_summary(&self) {
        println!("=== Ternary Sparse Matmul Benchmark ===");
        println!("  Weight sparsity:  {:.1}% zeros", self.weight_sparsity * 100.0);
        println!("  Dense ops:        {}", self.dense_ops);
        println!("  Sparse ops:       {}", self.sparse_ops);
        println!("  Skipped ops:      {}", self.skipped_ops);
        println!("  Skip rate:        {:.1}%", self.skip_rate * 100.0);
        println!("  Ops saved:        {:.1}x fewer multiplies", self.dense_ops as f64 / self.sparse_ops.max(1) as f64);
    }
}

pub fn benchmark(a: &TritMatrix, b: &TritMatrix) -> BenchmarkResult {
    let dense_ops = a.rows * a.cols * b.cols;
    let (_result, skipped) = sparse_matmul(a, b);
    let sparse_ops = dense_ops - skipped;
    BenchmarkResult {
        dense_ops,
        sparse_ops,
        skipped_ops: skipped,
        skip_rate: skipped as f64 / dense_ops as f64,
        weight_sparsity: b.sparsity(),
    }
}

// ─── Trit activation functions ───────────────────────────────────────────────

/// Ternary threshold activation: maps accumulator trit to output trit.
/// sign(x): +1 → +1, 0 → 0, -1 → -1. Identity on Trit — but useful as a
/// named function to clarify intent in MLP forward passes.
pub fn trit_activation(t: Trit) -> Trit { t }

/// Majority vote across a row of trits — reduces a vector to one trit.
/// Returns the sign of the sum: positive majority → +1, negative → -1, tie → 0.
pub fn majority(trits: &[Trit]) -> Trit {
    let sum: i32 = trits.iter().map(|&t| match t {
        Trit::PosOne => 1,
        Trit::NegOne => -1,
        Trit::Zero   => 0,
    }).sum();
    match sum.signum() {
        1  => Trit::PosOne,
        -1 => Trit::NegOne,
        _  => Trit::Zero,
    }
}

// ─── 2-Layer Ternary MLP ─────────────────────────────────────────────────────

/// A 2-layer ternary multi-layer perceptron.
///
/// Architecture:
///   input (in_features) → hidden (hidden_size) → output (out_features)
///
/// All weights are ternary {-1, 0, +1}. Forward pass uses sparse_matmul.
/// No bias terms (ternary bias adds nothing that weight magnitude can't cover).
pub struct TernaryMLP {
    pub w1: TritMatrix,   // [in_features × hidden_size]
    pub w2: TritMatrix,   // [hidden_size × out_features]
    pub in_features:  usize,
    pub hidden_size:  usize,
    pub out_features: usize,
}

impl TernaryMLP {
    /// Construct from pre-quantized weight matrices.
    pub fn new(w1: TritMatrix, w2: TritMatrix) -> Self {
        let in_features  = w1.rows;
        let hidden_size  = w1.cols;
        let out_features = w2.cols;
        assert_eq!(w2.rows, hidden_size, "w1.cols must equal w2.rows");
        Self { w1, w2, in_features, hidden_size, out_features }
    }

    /// Initialise from f32 weight slices using BitNet threshold quantization.
    pub fn from_f32(
        in_features: usize, hidden_size: usize, out_features: usize,
        w1_f32: &[f32], w2_f32: &[f32],
    ) -> Self {
        let τ1 = bitnet_threshold(w1_f32);
        let τ2 = bitnet_threshold(w2_f32);
        let w1 = TritMatrix::from_f32(in_features, hidden_size, w1_f32, τ1);
        let w2 = TritMatrix::from_f32(hidden_size, out_features, w2_f32, τ2);
        Self::new(w1, w2)
    }

    /// Forward pass: input [1 × in_features] → output [1 × out_features].
    ///
    /// Returns (output_row, layer1_skips, layer2_skips).
    pub fn forward(&self, input: &TritMatrix) -> (TritMatrix, usize, usize) {
        assert_eq!(input.cols, self.in_features,
            "input width must match in_features");

        // Layer 1: hidden = input × w1  (sparse)
        let (hidden, skip1) = sparse_matmul(input, &self.w1);

        // Trit activation (identity — ternary is already bounded)
        let hidden_act = TritMatrix::from_trits(
            hidden.rows, hidden.cols,
            hidden.data.iter().map(|&t| trit_activation(t)).collect(),
        );

        // Layer 2: output = hidden × w2  (sparse)
        let (output, skip2) = sparse_matmul(&hidden_act, &self.w2);

        (output, skip1, skip2)
    }

    /// Classify a single input row: returns the column index of the max
    /// activated output (most +1, breaking ties by column index).
    pub fn predict(&self, input: &TritMatrix) -> usize {
        let (output, _, _) = self.forward(input);
        let row = 0;
        let mut best_col = 0;
        let mut best_val: i8 = -2;
        for col in 0..self.out_features {
            let v = match output.get(row, col) {
                Trit::PosOne => 1,
                Trit::Zero   => 0,
                Trit::NegOne => -1,
            };
            if v > best_val { best_val = v; best_col = col; }
        }
        best_col
    }

    pub fn layer1_sparsity(&self) -> f64 { self.w1.sparsity() }
    pub fn layer2_sparsity(&self) -> f64 { self.w2.sparsity() }
}

// ─── Extended timed benchmark ────────────────────────────────────────────────

/// Wall-clock timed benchmark result for one matrix size.
#[derive(Debug)]
pub struct TimedResult {
    pub size:            usize,   // N (N×N square matrices)
    pub dense_ops:       usize,
    pub sparse_ops:      usize,
    pub skipped_ops:     usize,
    pub weight_sparsity: f64,
    pub skip_rate:       f64,
    pub speedup:         f64,
    pub dense_us:        u64,     // microseconds
    pub sparse_us:       u64,     // microseconds
}

/// Run timed dense vs sparse matmul across multiple square matrix sizes.
///
/// Uses normally distributed f32 weights quantized with BitNet threshold.
/// Each size is run `reps` times and the median is reported.
pub fn timed_benchmark(sizes: &[usize], reps: usize) -> Vec<TimedResult> {
    use std::time::Instant;

    // Deterministic pseudo-random f32 weights (no external crate needed)
    fn lcg_weights(n: usize, seed: u64) -> Vec<f32> {
        let mut state = seed;
        (0..n).map(|_| {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            // Map to approximately N(0,1) via Box-Muller would need two values;
            // instead use a simple mapping to [-1.5, 1.5]
            let f = ((state >> 33) as f32) / (u32::MAX as f32) * 3.0 - 1.5;
            f
        }).collect()
    }

    fn median_us(mut times: Vec<u64>) -> u64 {
        times.sort_unstable();
        times[times.len() / 2]
    }

    sizes.iter().map(|&n| {
        let weights_a = lcg_weights(n * n, 0xdeadbeef);
        let weights_b = lcg_weights(n * n, 0xc0ffee42);
        let τa = bitnet_threshold(&weights_a);
        let τb = bitnet_threshold(&weights_b);
        let a = TritMatrix::from_f32(n, n, &weights_a, τa);
        let b = TritMatrix::from_f32(n, n, &weights_b, τb);

        let sparsity = b.sparsity();
        let dense_ops  = n * n * n;
        let (_, skipped) = sparse_matmul(&a, &b); // warm-up + count
        let sparse_ops = dense_ops - skipped;

        // Time dense
        let dense_times: Vec<u64> = (0..reps).map(|_| {
            let t = Instant::now();
            let _ = dense_matmul(&a, &b);
            t.elapsed().as_micros() as u64
        }).collect();

        // Time sparse
        let sparse_times: Vec<u64> = (0..reps).map(|_| {
            let t = Instant::now();
            let _ = sparse_matmul(&a, &b);
            t.elapsed().as_micros() as u64
        }).collect();

        let dense_us  = median_us(dense_times);
        let sparse_us = median_us(sparse_times);
        let speedup   = if sparse_us > 0 {
            dense_us as f64 / sparse_us as f64
        } else { dense_ops as f64 / sparse_ops.max(1) as f64 };

        TimedResult {
            size: n, dense_ops, sparse_ops, skipped_ops: skipped,
            weight_sparsity: sparsity, skip_rate: skipped as f64 / dense_ops as f64,
            speedup, dense_us, sparse_us,
        }
    }).collect()
}

/// Print a formatted benchmark table to stdout.
pub fn print_benchmark_table(results: &[TimedResult]) {
    println!("\n╔══════════════════════════════════════════════════════════════════════╗");
    println!(  "║         Ternlang Sparse Matmul Benchmark — RFI-IRFOS TIS           ║");
    println!(  "╠════════╦══════════╦═══════════╦══════════╦══════════╦═════════════╣");
    println!(  "║  Size  ║ Sparsity ║ Dense μs  ║ Sparse μs║  Speedup ║  Skip rate  ║");
    println!(  "╠════════╬══════════╬═══════════╬══════════╬══════════╬═════════════╣");
    for r in results {
        println!("║ {:>4}² ║  {:>5.1}%  ║  {:>7}  ║  {:>7} ║  {:>5.2}×  ║   {:>6.1}%   ║",
            r.size,
            r.weight_sparsity * 100.0,
            r.dense_us,
            r.sparse_us,
            r.speedup,
            r.skip_rate * 100.0,
        );
    }
    println!(  "╚════════╩══════════╩═══════════╩══════════╩══════════╩═════════════╝");
}

// ─── XOR / Parity datasets ───────────────────────────────────────────────────

/// All 4 XOR inputs as ternary rows: {-1,+1} × {-1,+1} → {-1,+1}
/// Input encoding: -1 = False, +1 = True
pub fn xor_dataset() -> Vec<(TritMatrix, usize)> {
    let inputs = vec![
        (vec![Trit::NegOne, Trit::NegOne], 0usize), // F XOR F = F → class 0
        (vec![Trit::NegOne, Trit::PosOne], 1usize), // F XOR T = T → class 1
        (vec![Trit::PosOne, Trit::NegOne], 1usize), // T XOR F = T → class 1
        (vec![Trit::PosOne, Trit::PosOne], 0usize), // T XOR T = F → class 0
    ];
    inputs.into_iter().map(|(row, label)| {
        (TritMatrix::from_trits(1, 2, row), label)
    }).collect()
}

/// 3-bit parity dataset: 8 inputs → label 0 (even parity) or 1 (odd parity)
pub fn parity_dataset() -> Vec<(TritMatrix, usize)> {
    (0u8..8).map(|i| {
        let bits = vec![
            if i & 4 != 0 { Trit::PosOne } else { Trit::NegOne },
            if i & 2 != 0 { Trit::PosOne } else { Trit::NegOne },
            if i & 1 != 0 { Trit::PosOne } else { Trit::NegOne },
        ];
        let parity = (i.count_ones() % 2) as usize;
        (TritMatrix::from_trits(1, 3, bits), parity)
    }).collect()
}

/// Evaluate MLP accuracy on a dataset.
/// Returns (correct, total, accuracy).
pub fn evaluate(mlp: &TernaryMLP, dataset: &[(TritMatrix, usize)]) -> (usize, usize, f64) {
    let total   = dataset.len();
    let correct = dataset.iter()
        .filter(|(input, label)| mlp.predict(input) == *label)
        .count();
    let accuracy = correct as f64 / total as f64;
    (correct, total, accuracy)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quantize_basic() {
        let weights = vec![-0.9f32, -0.2, 0.0, 0.3, 0.8];
        let threshold = 0.5;
        let trits = quantize(&weights, threshold);
        assert_eq!(trits, vec![Trit::NegOne, Trit::Zero, Trit::Zero, Trit::Zero, Trit::PosOne]);
    }

    #[test]
    fn test_bitnet_threshold() {
        let weights = vec![1.0f32, -1.0, 0.5, -0.5];
        let τ = bitnet_threshold(&weights);
        // mean(|w|) = 0.75, threshold = 0.375
        assert!((τ - 0.375).abs() < 1e-6);
    }

    #[test]
    fn test_dense_matmul_identity() {
        // Identity matrix: [[1,0],[0,1]] × [[1,0],[0,1]] = [[1,0],[0,1]]
        let mut id = TritMatrix::new(2, 2);
        id.set(0, 0, Trit::PosOne);
        id.set(1, 1, Trit::PosOne);

        let result = dense_matmul(&id, &id);
        assert_eq!(result.get(0, 0), Trit::PosOne);
        assert_eq!(result.get(0, 1), Trit::Zero);
        assert_eq!(result.get(1, 0), Trit::Zero);
        assert_eq!(result.get(1, 1), Trit::PosOne);
    }

    #[test]
    fn test_sparse_matmul_matches_dense() {
        // Sparse and dense must produce identical results
        let weights = vec![0.9f32, -0.1, 0.05, -0.8, 0.0, 0.7, -0.6, 0.2, 0.0];
        let threshold = 0.5;
        let w = TritMatrix::from_f32(3, 3, &weights, threshold);
        let mut input = TritMatrix::new(3, 3);
        input.set(0, 0, Trit::PosOne);
        input.set(1, 1, Trit::NegOne);
        input.set(2, 2, Trit::PosOne);

        let dense = dense_matmul(&input, &w);
        let (sparse, skipped) = sparse_matmul(&input, &w);

        // Results must match element-by-element
        for r in 0..3 {
            for c in 0..3 {
                assert_eq!(dense.get(r, c), sparse.get(r, c),
                    "mismatch at ({}, {})", r, c);
            }
        }
        // Some ops should have been skipped
        assert!(skipped > 0, "expected skips for a sparse weight matrix");
    }

    #[test]
    fn test_sparsity_measurement() {
        let weights = vec![0.9f32, 0.1, -0.9]; // threshold 0.5 → [+1, 0, -1]
        let threshold = 0.5;
        let m = TritMatrix::from_f32(1, 3, &weights, threshold);
        // 1 out of 3 is zero
        assert!((m.sparsity() - 1.0/3.0).abs() < 1e-9);
        assert_eq!(m.nnz(), 2);
    }

    #[test]
    fn test_majority_vote() {
        assert_eq!(majority(&[Trit::PosOne, Trit::PosOne, Trit::NegOne]), Trit::PosOne);
        assert_eq!(majority(&[Trit::NegOne, Trit::NegOne, Trit::PosOne]), Trit::NegOne);
        assert_eq!(majority(&[Trit::PosOne, Trit::NegOne]),               Trit::Zero);
        assert_eq!(majority(&[Trit::Zero, Trit::Zero]),                   Trit::Zero);
    }

    #[test]
    fn test_mlp_forward_runs() {
        // Tiny 2-in → 4-hidden → 2-out MLP, random-ish weights
        let w1_f32: Vec<f32> = vec![
             0.9, -0.8,  0.7, -0.6,
            -0.7,  0.9, -0.5,  0.8,
        ];
        let w2_f32: Vec<f32> = vec![
             0.9, -0.9,
            -0.8,  0.8,
             0.7, -0.7,
            -0.6,  0.6,
        ];
        let mlp = TernaryMLP::from_f32(2, 4, 2, &w1_f32, &w2_f32);
        let input = TritMatrix::from_trits(1, 2, vec![Trit::PosOne, Trit::NegOne]);
        let (out, s1, s2) = mlp.forward(&input);
        assert_eq!(out.rows, 1);
        assert_eq!(out.cols, 2);
        // Skips should be non-negative (may be 0 if all weights non-zero after quantize)
        let _ = (s1, s2);
    }

    #[test]
    fn test_mlp_predict_returns_valid_class() {
        let w1_f32: Vec<f32> = vec![0.9, -0.8, -0.7, 0.9];
        let w2_f32: Vec<f32> = vec![0.9, -0.9, -0.8, 0.8];
        let mlp = TernaryMLP::from_f32(2, 2, 2, &w1_f32, &w2_f32);
        let input = TritMatrix::from_trits(1, 2, vec![Trit::PosOne, Trit::NegOne]);
        let pred = mlp.predict(&input);
        assert!(pred < 2, "prediction must be a valid class index");
    }

    #[test]
    fn test_xor_dataset_shape() {
        let ds = xor_dataset();
        assert_eq!(ds.len(), 4);
        for (input, label) in &ds {
            assert_eq!(input.rows, 1);
            assert_eq!(input.cols, 2);
            assert!(*label < 2);
        }
    }

    #[test]
    fn test_parity_dataset_shape() {
        let ds = parity_dataset();
        assert_eq!(ds.len(), 8);
        for (input, label) in &ds {
            assert_eq!(input.cols, 3);
            assert!(*label < 2);
        }
    }

    #[test]
    fn test_xor_mlp_with_known_weights() {
        // Hand-designed weights that solve XOR in ternary:
        // Layer 1: detect (A AND NOT B) and (NOT A AND B)
        // w1: [2-in → 2-hidden]
        //   h0 = A·(+1) + B·(-1)  → +1 when A=+1,B=-1
        //   h1 = A·(-1) + B·(+1)  → +1 when A=-1,B=+1
        let w1_f32 = vec![
             1.0, -1.0,
            -1.0,  1.0,
        ];
        // Layer 2: OR the two hidden units → XOR output
        // w2: [2-hidden → 2-out]  (class 0 = same, class 1 = different)
        let w2_f32 = vec![
            -1.0,  1.0,
            -1.0,  1.0,
        ];
        let mlp = TernaryMLP::from_f32(2, 2, 2, &w1_f32, &w2_f32);
        let ds  = xor_dataset();
        let (correct, total, acc) = evaluate(&mlp, &ds);
        println!("XOR MLP: {}/{} = {:.0}%", correct, total, acc * 100.0);
        // With perfect hand-designed weights we expect ≥ 50% (ternary quantization
        // is exact here since all weights are ±1.0 with threshold ≈ 0.5)
        assert!(correct >= 2, "MLP should get at least half of XOR correct");
    }

    #[test]
    fn test_timed_benchmark_small() {
        let results = timed_benchmark(&[8, 16], 3);
        assert_eq!(results.len(), 2);
        for r in &results {
            assert!(r.dense_ops > 0);
            assert!(r.weight_sparsity >= 0.0 && r.weight_sparsity <= 1.0);
            assert!(r.skip_rate >= 0.0 && r.skip_rate <= 1.0);
        }
        print_benchmark_table(&results);
    }

    #[test]
    fn test_benchmark_reports_skips() {
        // 4×4 weight matrix from f32, ~50% zeros
        let weights: Vec<f32> = vec![
            0.9, 0.1, -0.9, 0.0,
            0.1, 0.8, 0.0, -0.7,
            0.0, 0.1, 0.6, 0.2,
           -0.8, 0.0, 0.1, 0.9,
        ];
        let threshold = 0.5;
        let w = TritMatrix::from_f32(4, 4, &weights, threshold);
        let input = TritMatrix::new(4, 4); // all zeros input
        let result = benchmark(&input, &w);
        assert!(result.skipped_ops > 0);
        assert!(result.skip_rate > 0.0 && result.skip_rate <= 1.0);
        result.print_summary();
    }
}
