/// ternlang-ml: Ternary ML inference kernels for RFI-IRFOS Ternary Intelligence Stack
///
/// Provides:
///   - quantize()      — convert f32 weights to balanced ternary (-1, 0, +1)
///   - sparse_matmul() — matmul skipping zero-state weights (the benchmark flagship)
///   - dense_matmul()  — standard ternary matmul for comparison
///   - linear()        — BitNet-style ternary linear layer (sparse by default)
///   - sparsity()      — measure fraction of zero-state elements

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
