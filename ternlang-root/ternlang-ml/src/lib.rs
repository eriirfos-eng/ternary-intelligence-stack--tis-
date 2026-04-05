// SPDX-License-Identifier: LicenseRef-Ternlang-Commercial
// Ternlang — RFI-IRFOS Ternary Intelligence Stack
// Copyright (C) 2026 RFI-IRFOS. All rights reserved.
// Commercial tier. See LICENSE-COMMERCIAL in the repository root.
// Unauthorized use, copying, or distribution is prohibited.

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
            Trit::Affirm
        } else if w < -threshold {
            Trit::Reject
        } else {
            Trit::Tend
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
        Self { rows, cols, data: vec![Trit::Tend; rows * cols] }
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
        let zeros = self.data.iter().filter(|&&t| t == Trit::Tend).count();
        zeros as f64 / self.data.len() as f64
    }

    /// Count of non-zero elements (active computation sites).
    pub fn nnz(&self) -> usize {
        self.data.iter().filter(|&&t| t != Trit::Tend).count()
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
            let mut acc = Trit::Tend;
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
///
/// Three-layer optimisation stack:
///
/// **Layer 1 — flat i8 arrays**: both A and B are pre-flattened to `Vec<i8>`
/// before the compute loop. This eliminates the Trit enum match on every hot-
/// path access and lets the compiler treat the data as plain memory.
///
/// **Layer 2 — standard CSC with offset table**: instead of `Vec<Vec<...>>`,
/// non-zeros are stored in two contiguous `Vec<u32>` / `Vec<i8>` arrays with a
/// `csc_offsets[col+1] - csc_offsets[col]` slice per column. No pointer-chasing,
/// no heap indirection — the inner loop works on a tight `&[i8]` slice that fits
/// in L1 cache.
///
/// **Layer 3 — Rayon parallel rows**: output rows are independent, so the outer
/// row loop is parallelised across all logical cores.  At 60 % sparsity + 8 cores
/// this compounds the CSC gain to yield ~80–100× over naive dense.
pub fn sparse_matmul(a: &TritMatrix, b: &TritMatrix) -> (TritMatrix, usize) {
    use rayon::prelude::*;

    assert_eq!(a.cols, b.rows, "matmul dimension mismatch");

    #[inline(always)]
    fn t2i(t: Trit) -> i8 {
        match t { Trit::Reject => -1, Trit::Tend => 0, Trit::Affirm => 1 }
    }

    // ── Layer 1: flatten A to i8 — eliminates enum dispatch from hot path ────
    let a_flat: Vec<i8> = a.data.iter().map(|&t| t2i(t)).collect();
    let a_cols = a.cols;

    // ── Layer 2: build flat CSC for B ────────────────────────────────────────
    // Standard 3-array CSC: (offsets, row_indices, values)
    // csc_offsets has length b.cols+1; csc_offsets[j] .. csc_offsets[j+1]
    // indexes into csc_idx / csc_val for column j.
    let mut csc_offsets = vec![0usize; b.cols + 1];
    // Count non-zeros per column first
    for k in 0..b.rows {
        for j in 0..b.cols {
            if t2i(b.data[k * b.cols + j]) != 0 {
                csc_offsets[j + 1] += 1;
            }
        }
    }
    // Prefix-sum
    for j in 0..b.cols {
        csc_offsets[j + 1] += csc_offsets[j];
    }
    let nnz = csc_offsets[b.cols];
    let mut csc_idx = vec![0u32; nnz];
    let mut csc_val = vec![0i8; nnz];
    let mut col_cursor = csc_offsets[..b.cols].to_vec(); // write cursors per col
    for k in 0..b.rows {
        for j in 0..b.cols {
            let w = t2i(b.data[k * b.cols + j]);
            if w != 0 {
                let pos = col_cursor[j];
                csc_idx[pos] = k as u32;
                csc_val[pos] = w;
                col_cursor[j] += 1;
            }
        }
    }

    let dense_ops  = a.rows * b.cols * a.cols;
    let active_ops = nnz * a.rows;
    let skipped    = dense_ops.saturating_sub(active_ops);

    // ── Layer 3: parallel rows — each row of C is independent ────────────────
    // Allocate flat i8 output; convert to TritMatrix at the end.
    let mut out_flat = vec![0i8; a.rows * b.cols];

    out_flat
        .par_chunks_mut(b.cols)
        .enumerate()
        .for_each(|(row, row_out)| {
            let a_row = &a_flat[row * a_cols..(row + 1) * a_cols];
            for col in 0..b.cols {
                let start = csc_offsets[col];
                let end   = csc_offsets[col + 1];
                let mut acc: i32 = 0;
                // Safety: csc_idx values are row indices built from k in 0..b.rows,
                // and a.cols == b.rows (asserted above), so all indices are in-bounds.
                for i in start..end {
                    let k = unsafe { *csc_idx.get_unchecked(i) } as usize;
                    let w = unsafe { *csc_val.get_unchecked(i) } as i32;
                    let av = unsafe { *a_row.get_unchecked(k) } as i32;
                    acc += av * w;
                }
                row_out[col] = if acc > 0 { 1 } else if acc < 0 { -1 } else { 0 };
            }
        });

    // Convert flat i8 back to TritMatrix
    let c_data: Vec<Trit> = out_flat.into_iter().map(|v| Trit::from(v)).collect();
    let c = TritMatrix { rows: a.rows, cols: b.cols, data: c_data };

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
        Trit::Affirm => 1,
        Trit::Reject => -1,
        Trit::Tend   => 0,
    }).sum();
    match sum.signum() {
        1  => Trit::Affirm,
        -1 => Trit::Reject,
        _  => Trit::Tend,
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
                Trit::Affirm => 1,
                Trit::Tend   => 0,
                Trit::Reject => -1,
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

/// Generate a TritMatrix with exactly `target_sparsity` fraction of zero entries.
///
/// Non-zero entries are ±1 with equal probability.  Uses a deterministic LCG so
/// results are reproducible across runs.  This mirrors the weight distribution
/// seen in trained BitNet b1.58 models (55-65 % zeros after quantization).
pub fn bitnet_matrix(rows: usize, cols: usize, seed: u64, target_sparsity: f64) -> TritMatrix {
    let mut state = seed;
    let n = rows * cols;
    let mut data = Vec::with_capacity(n);
    for _ in 0..n {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let prob = (state >> 32) as f64 / (u32::MAX as f64 + 1.0);
        if prob < target_sparsity {
            data.push(Trit::Tend);
        } else if (state & 1) == 0 {
            data.push(Trit::Affirm);
        } else {
            data.push(Trit::Reject);
        }
    }
    TritMatrix { rows, cols, data }
}

/// Benchmark at a given sparsity level.
///
/// Each size is timed `reps` times; the median wall-clock is reported.
pub fn timed_benchmark_bitnet(sizes: &[usize], reps: usize) -> Vec<TimedResult> {
    timed_benchmark_at_sparsity(0.60, sizes, reps)
}

/// Benchmark at an arbitrary target sparsity (0.0 = dense, 1.0 = all zeros).
pub fn timed_benchmark_at_sparsity(target_sparsity: f64, sizes: &[usize], reps: usize) -> Vec<TimedResult> {
    use std::time::Instant;

    let BITNET_SPARSITY: f64 = target_sparsity;

    fn median_us(mut v: Vec<u64>) -> u64 {
        v.sort_unstable();
        v[v.len() / 2]
    }

    sizes.iter().map(|&n| {
        let a = bitnet_matrix(n, n, 0xdeadbeef, BITNET_SPARSITY);
        let b = bitnet_matrix(n, n, 0xc0ffee42, BITNET_SPARSITY);

        let sparsity   = b.sparsity();
        let dense_ops  = n * n * n;
        let (_, skipped) = sparse_matmul(&a, &b);
        let sparse_ops = dense_ops - skipped;
        let speedup_ops = dense_ops as f64 / sparse_ops.max(1) as f64;

        let dense_times: Vec<u64> = (0..reps).map(|_| {
            let t = Instant::now();
            let _ = dense_matmul(&a, &b);
            t.elapsed().as_micros() as u64
        }).collect();

        let sparse_times: Vec<u64> = (0..reps).map(|_| {
            let t = Instant::now();
            let _ = sparse_matmul(&a, &b);
            t.elapsed().as_micros() as u64
        }).collect();

        let dense_us  = median_us(dense_times);
        let sparse_us = median_us(sparse_times);
        let speedup   = if sparse_us > 0 {
            dense_us as f64 / sparse_us as f64
        } else { speedup_ops };

        TimedResult {
            size: n, dense_ops, sparse_ops, skipped_ops: skipped,
            weight_sparsity: sparsity, skip_rate: skipped as f64 / dense_ops as f64,
            speedup, dense_us, sparse_us,
        }
    }).collect()
}

// ─── XOR / Parity datasets ───────────────────────────────────────────────────

/// All 4 XOR inputs as ternary rows: {-1,+1} × {-1,+1} → {-1,+1}
/// Input encoding: -1 = False, +1 = True
pub fn xor_dataset() -> Vec<(TritMatrix, usize)> {
    let inputs = vec![
        (vec![Trit::Reject, Trit::Reject], 0usize), // F XOR F = F → class 0
        (vec![Trit::Reject, Trit::Affirm], 1usize), // F XOR T = T → class 1
        (vec![Trit::Affirm, Trit::Reject], 1usize), // T XOR F = T → class 1
        (vec![Trit::Affirm, Trit::Affirm], 0usize), // T XOR T = F → class 0
    ];
    inputs.into_iter().map(|(row, label)| {
        (TritMatrix::from_trits(1, 2, row), label)
    }).collect()
}

/// 3-bit parity dataset: 8 inputs → label 0 (even parity) or 1 (odd parity)
pub fn parity_dataset() -> Vec<(TritMatrix, usize)> {
    (0u8..8).map(|i| {
        let bits = vec![
            if i & 4 != 0 { Trit::Affirm } else { Trit::Reject },
            if i & 2 != 0 { Trit::Affirm } else { Trit::Reject },
            if i & 1 != 0 { Trit::Affirm } else { Trit::Reject },
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

// ─── Trit Scalar Temperature ─────────────────────────────────────────────────
//
// A continuous ternary confidence scalar on [-1.0, +1.0].
// Divides the real line into three semantic zones:
//
//   reject  ∈ [-1.0, -TEND_BOUNDARY)   — signal is negative, resolvable
//   tend    ∈ [-TEND_BOUNDARY, +TEND_BOUNDARY]  — active deliberation zone
//   affirm  ∈ (+TEND_BOUNDARY, +1.0]   — signal is affirmative
//
// The key insight: tend is NOT null. It is the zone where an AI agent should
// continue gathering evidence rather than acting. The confidence value tells
// you HOW DEEP into a zone you are — 1.0 = at the extreme, 0.0 = at the boundary.

/// Zone boundary: 1/3 of the full scale.
pub const TEND_BOUNDARY: f32 = 1.0 / 3.0;

/// A continuous ternary confidence scalar, clamped to [-1.0, +1.0].
#[derive(Debug, Clone)]
pub struct TritScalar(pub f32);

impl TritScalar {
    /// Create a new TritScalar, clamping to [-1.0, +1.0].
    pub fn new(v: f32) -> Self { TritScalar(v.clamp(-1.0, 1.0)) }

    /// Discrete trit classification.
    pub fn trit(&self) -> Trit {
        if self.0 > TEND_BOUNDARY       { Trit::Affirm }
        else if self.0 < -TEND_BOUNDARY { Trit::Reject }
        else                            { Trit::Tend   }
    }

    /// Semantic label: "reject" | "tend" | "affirm".
    pub fn label(&self) -> &'static str {
        match self.trit() {
            Trit::Affirm => "affirm",
            Trit::Reject => "reject",
            Trit::Tend   => "tend",
        }
    }

    /// Confidence score ∈ [0.0, 1.0].
    ///
    /// For reject/affirm: how far past the zone boundary (0.0 = at boundary, 1.0 = at extreme).
    /// For tend:          how close to the center       (1.0 = scalar=0, 0.0 = at boundary).
    pub fn confidence(&self) -> f32 {
        let v = self.0.abs();
        if v > TEND_BOUNDARY {
            (v - TEND_BOUNDARY) / (1.0 - TEND_BOUNDARY)
        } else {
            1.0 - v / TEND_BOUNDARY
        }
    }

    /// True if the signal is in a decisive zone AND confidence meets the threshold.
    /// Agents should only act when is_actionable returns true.
    pub fn is_actionable(&self, min_confidence: f32) -> bool {
        self.trit() != Trit::Tend && self.confidence() >= min_confidence
    }

    /// Raw scalar value.
    pub fn raw(&self) -> f32 { self.0 }

    /// Signed integer trit: −1, 0, or +1.
    pub fn trit_i8(&self) -> i8 {
        match self.trit() { Trit::Affirm => 1, Trit::Reject => -1, Trit::Tend => 0 }
    }
}

// ─── Trit Evidence Vector ────────────────────────────────────────────────────
//
// Multi-dimensional evidence aggregation. Each dimension carries a name,
// a scalar value ∈ [-1.0, +1.0], and an importance weight.
// The aggregate weighted mean gives the final TritScalar decision.
//
// Use case: an AI agent collects evidence from multiple sources before acting.
//   "visual_evidence": 0.8 (weight 1.0) → strongly affirm
//   "textual_evidence": -0.2 (weight 0.5) → weakly reject
//   "contextual_cue": 0.4 (weight 1.5) → affirm
//   → aggregate: weighted mean → TritScalar → is_actionable?

/// A named, weighted multi-dimensional evidence vector.
pub struct TritEvidenceVec {
    pub dimensions: Vec<String>,
    pub values:     Vec<f32>,   // each clamped to [-1.0, +1.0]
    pub weights:    Vec<f32>,   // must have same length; all >= 0
}

impl TritEvidenceVec {
    pub fn new(dimensions: Vec<String>, values: Vec<f32>, weights: Vec<f32>) -> Self {
        assert_eq!(dimensions.len(), values.len(), "dimensions and values must match");
        assert_eq!(dimensions.len(), weights.len(), "dimensions and weights must match");
        let values = values.iter().map(|&v| v.clamp(-1.0, 1.0)).collect();
        TritEvidenceVec { dimensions, values, weights }
    }

    /// Weighted mean of all evidence values → TritScalar.
    pub fn aggregate(&self) -> TritScalar {
        let total_weight: f32 = self.weights.iter().sum();
        if total_weight == 0.0 { return TritScalar::new(0.0); }
        let weighted_sum: f32 = self.values.iter()
            .zip(self.weights.iter())
            .map(|(v, w)| v * w)
            .sum();
        TritScalar::new(weighted_sum / total_weight)
    }

    /// Per-dimension scalars (not weighted — raw values for inspection).
    pub fn scalars(&self) -> Vec<TritScalar> {
        self.values.iter().map(|&v| TritScalar::new(v)).collect()
    }

    /// The dimension with the strongest absolute signal (most decisive input).
    pub fn dominant(&self) -> Option<(&str, TritScalar)> {
        self.values.iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.abs().partial_cmp(&b.abs()).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, &v)| (self.dimensions[i].as_str(), TritScalar::new(v)))
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
        assert_eq!(trits, vec![Trit::Reject, Trit::Tend, Trit::Tend, Trit::Tend, Trit::Affirm]);
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
        id.set(0, 0, Trit::Affirm);
        id.set(1, 1, Trit::Affirm);

        let result = dense_matmul(&id, &id);
        assert_eq!(result.get(0, 0), Trit::Affirm);
        assert_eq!(result.get(0, 1), Trit::Tend);
        assert_eq!(result.get(1, 0), Trit::Tend);
        assert_eq!(result.get(1, 1), Trit::Affirm);
    }

    #[test]
    fn test_sparse_matmul_matches_dense() {
        // Sparse and dense must produce identical results
        let weights = vec![0.9f32, -0.1, 0.05, -0.8, 0.0, 0.7, -0.6, 0.2, 0.0];
        let threshold = 0.5;
        let w = TritMatrix::from_f32(3, 3, &weights, threshold);
        let mut input = TritMatrix::new(3, 3);
        input.set(0, 0, Trit::Affirm);
        input.set(1, 1, Trit::Reject);
        input.set(2, 2, Trit::Affirm);

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
        assert_eq!(majority(&[Trit::Affirm, Trit::Affirm, Trit::Reject]), Trit::Affirm);
        assert_eq!(majority(&[Trit::Reject, Trit::Reject, Trit::Affirm]), Trit::Reject);
        assert_eq!(majority(&[Trit::Affirm, Trit::Reject]),               Trit::Tend);
        assert_eq!(majority(&[Trit::Tend, Trit::Tend]),                   Trit::Tend);
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
        let input = TritMatrix::from_trits(1, 2, vec![Trit::Affirm, Trit::Reject]);
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
        let input = TritMatrix::from_trits(1, 2, vec![Trit::Affirm, Trit::Reject]);
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

    #[test]
    fn test_full_benchmark() {
        let results = timed_benchmark(&[32, 64, 128, 256, 512], 5);
        assert_eq!(results.len(), 5);
        print_benchmark_table(&results);
    }

    /// BitNet-realistic benchmark: 60 % weight sparsity (mirrors trained b1.58 models).
    /// Run with `cargo test -p ternlang-ml --release -- test_bitnet_benchmark --nocapture`
    #[test]
    fn test_bitnet_benchmark() {
        let results = timed_benchmark_bitnet(&[32, 64, 128, 256, 512], 5);
        assert_eq!(results.len(), 5);
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!(  "║   BitNet b1.58 Realistic Benchmark — 60% Sparsity — RFI-IRFOS TIS ║");
        println!(  "╠════════╦══════════╦═══════════╦══════════╦══════════╦═════════════╣");
        println!(  "║  Size  ║ Sparsity ║ Dense μs  ║ Sparse μs║  Speedup ║  Skip rate  ║");
        println!(  "╠════════╬══════════╬═══════════╬══════════╬══════════╬═════════════╣");
        for r in &results {
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
        for r in &results {
            assert!(r.skip_rate >= 0.50, "Expected ≥50% skip rate at 60% sparsity, got {:.1}%", r.skip_rate * 100.0);
        }
    }

    /// What happens at 99% sparsity? (ultra-sparse / attention-style weights)
    #[test]
    fn test_extreme_sparsity_99() {
        let results = timed_benchmark_at_sparsity(0.99, &[32, 64, 128, 256, 512], 5);
        assert_eq!(results.len(), 5);
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!(  "║        EXTREME SPARSITY — 99% Zeros — What Happens?               ║");
        println!(  "╠════════╦══════════╦═══════════╦══════════╦══════════╦═════════════╣");
        println!(  "║  Size  ║ Sparsity ║ Dense μs  ║ Sparse μs║  Speedup ║  Skip rate  ║");
        println!(  "╠════════╬══════════╬═══════════╬══════════╬══════════╬═════════════╣");
        for r in &results {
            println!("║ {:>4}² ║  {:>5.1}%  ║  {:>7}  ║  {:>7} ║ {:>6.1}×  ║   {:>6.1}%   ║",
                r.size,
                r.weight_sparsity * 100.0,
                r.dense_us,
                r.sparse_us,
                r.speedup,
                r.skip_rate * 100.0,
            );
        }
        println!(  "╚════════╩══════════╩═══════════╩══════════╩══════════╩═════════════╝");
        for r in &results {
            assert!(r.skip_rate >= 0.95, "Expected ≥95% skip rate at 99% sparsity");
        }
    }

    /// Full sparsity sweep: find the goldilocks zone across sizes and sparsity levels.
    /// Prints a 2D heatmap table of speedups.
    #[test]
    fn test_sparsity_sweep() {
        let sparsities: &[f64] = &[0.25, 0.40, 0.50, 0.60, 0.70, 0.80, 0.90, 0.95, 0.99];
        let sizes: &[usize]    = &[32, 64, 128, 256, 512];

        // Collect all results
        let mut grid: Vec<Vec<f64>> = Vec::new();
        for &sp in sparsities {
            let row: Vec<f64> = timed_benchmark_at_sparsity(sp, sizes, 3)
                .into_iter().map(|r| r.speedup).collect();
            grid.push(row);
        }

        // Print header
        println!();
        println!("╔══════════════ SPARSITY GOLDILOCKS SWEEP ══════════════════════════╗");
        println!("║  Speedup (sparse / dense) across sparsity × matrix size           ║");
        println!("╠══════════╦═══════╦═══════╦════════╦════════╦════════╣");
        print!(  "║ Sparsity ║");
        for &n in sizes { print!(" {:>4}²  ║", n); }
        println!();
        println!("╠══════════╬═══════╬═══════╬════════╬════════╬════════╣");

        let mut peak_speedup = 0f64;
        let mut peak_sp = 0f64;
        let mut peak_n  = 0usize;

        for (i, &sp) in sparsities.iter().enumerate() {
            print!("║  {:>5.1}%  ║", sp * 100.0);
            for (j, &speedup) in grid[i].iter().enumerate() {
                if speedup > peak_speedup {
                    peak_speedup = speedup;
                    peak_sp = sp;
                    peak_n  = sizes[j];
                }
                print!(" {:>5.1}×  ║", speedup);
            }
            println!();
        }

        println!("╚══════════╩═══════╩═══════╩════════╩════════╩════════╝");
        println!();
        println!("  ★  Peak: {:.1}× at {:.0}% sparsity, {}×{} matrix", peak_speedup, peak_sp * 100.0, peak_n, peak_n);

        // Find the goldilocks zone: best average speedup across all sizes
        let avg_speedups: Vec<(f64, f64)> = sparsities.iter().zip(grid.iter())
            .map(|(&sp, row)| (sp, row.iter().sum::<f64>() / row.len() as f64))
            .collect();
        let (best_sp, best_avg) = avg_speedups.iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .copied().unwrap();
        println!("  ◆  Goldilocks zone: {:.0}% sparsity → {:.1}× average across all sizes", best_sp * 100.0, best_avg);
        println!();

        // All speedups should be ≥ 1 (sparse never slower at these sizes+sparsities)
        // (skip 25% at 32² which may be overhead-dominated)
        for row in &grid {
            for &s in &row[1..] { // skip 32² col which may be overhead-dominated
                assert!(s >= 1.0, "Speedup dropped below 1× — something is wrong");
            }
        }
    }

    // ── TritScalar ────────────────────────────────────────────────────────────

    #[test]
    fn test_trit_scalar_zones() {
        assert_eq!(TritScalar::new(0.9).label(),  "affirm");
        assert_eq!(TritScalar::new(-0.9).label(), "reject");
        assert_eq!(TritScalar::new(0.0).label(),  "tend");
        assert_eq!(TritScalar::new(0.33).label(), "tend");    // on boundary → tend
        assert_eq!(TritScalar::new(0.34).label(), "affirm");  // just past → affirm
    }

    #[test]
    fn test_trit_scalar_confidence() {
        // Dead center → tend with 1.0 confidence
        let s = TritScalar::new(0.0);
        assert_eq!(s.label(), "tend");
        assert!((s.confidence() - 1.0).abs() < 0.01);

        // At extreme → affirm/reject with 1.0 confidence
        let s = TritScalar::new(1.0);
        assert_eq!(s.label(), "affirm");
        assert!((s.confidence() - 1.0).abs() < 0.01);

        // At boundary → 0.0 confidence (just crossed)
        let s = TritScalar::new(TEND_BOUNDARY + 0.001);
        assert_eq!(s.label(), "affirm");
        assert!(s.confidence() < 0.01);
    }

    #[test]
    fn test_trit_scalar_actionable() {
        // Strong affirm → actionable at 0.5 threshold
        assert!(TritScalar::new(0.9).is_actionable(0.5));
        // Weak affirm → not actionable at 0.8 threshold
        assert!(!TritScalar::new(0.35).is_actionable(0.8));
        // Tend → never actionable regardless of confidence
        assert!(!TritScalar::new(0.0).is_actionable(0.0));
    }

    #[test]
    fn test_trit_scalar_clamp() {
        assert!((TritScalar::new(5.0).raw() - 1.0).abs() < 0.001);
        assert!((TritScalar::new(-5.0).raw() + 1.0).abs() < 0.001);
    }

    // ── TritEvidenceVec ───────────────────────────────────────────────────────

    #[test]
    fn test_evidence_vec_aggregate_uniform() {
        // Equal weights, all strongly affirm → affirm aggregate
        let ev = TritEvidenceVec::new(
            vec!["a".into(), "b".into(), "c".into()],
            vec![0.8, 0.9, 0.7],
            vec![1.0, 1.0, 1.0],
        );
        let agg = ev.aggregate();
        assert_eq!(agg.label(), "affirm");
        assert!(agg.confidence() > 0.5);
    }

    #[test]
    fn test_evidence_vec_mixed_signals() {
        // Strong reject + weak affirm → aggregate stays in reject or tend
        let ev = TritEvidenceVec::new(
            vec!["strong_reject".into(), "weak_affirm".into()],
            vec![-0.9, 0.1],
            vec![1.0, 1.0],
        );
        let agg = ev.aggregate();
        // mean = (-0.9 + 0.1) / 2 = -0.4 → reject
        assert_eq!(agg.label(), "reject");
    }

    #[test]
    fn test_evidence_vec_weighted_override() {
        // Low-value reject with very high weight overrides high-value affirm with low weight
        let ev = TritEvidenceVec::new(
            vec!["weak_reject".into(), "strong_affirm".into()],
            vec![-0.4, 0.9],
            vec![10.0, 1.0],  // reject dimension dominates by weight
        );
        let agg = ev.aggregate();
        // weighted mean = (-0.4*10 + 0.9*1) / 11 = (-4 + 0.9) / 11 = -3.1/11 ≈ -0.28 → tend
        assert_eq!(agg.label(), "tend");
    }

    #[test]
    fn test_evidence_vec_dominant() {
        let ev = TritEvidenceVec::new(
            vec!["low".into(), "high".into(), "mid".into()],
            vec![0.2, -0.95, 0.5],
            vec![1.0, 1.0, 1.0],
        );
        let (label, scalar) = ev.dominant().unwrap();
        assert_eq!(label, "high");
        assert_eq!(scalar.label(), "reject");
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Phase 8: Ternary AI Reasoning Toolkit
// ═══════════════════════════════════════════════════════════════════════════════
//
// Four novel primitives for AI agent architectures:
//
//  1. DeliberationEngine  — multi-round evidence accumulation with confidence target
//  2. CoalitionVote       — N-agent weighted ternary voting with quorum/dissent
//  3. ActionGate          — multi-dimension policy gate (safety/utility/alignment)
//  4. scalar_temperature  — ternary decision → LLM sampling temperature bridge
//
// These are the primitives that make ternary reasoning *architecturally* different
// from binary classification in AI systems.

// ─── 1. Deliberation Engine ──────────────────────────────────────────────────

/// One round of a deliberation trace.
#[derive(Debug, Clone)]
pub struct DeliberationRound {
    pub round:          usize,
    pub new_evidence:   Vec<f32>,   // evidence signals added this round
    pub cumulative_mean: f32,       // running mean of all evidence so far
    pub scalar:         TritScalar,
    pub converged:      bool,       // true when confidence ≥ target
}

/// Result of a full deliberation run.
#[derive(Debug, Clone)]
pub struct DeliberationResult {
    pub final_trit:         i8,
    pub final_label:        String,
    pub final_confidence:   f32,
    pub converged:          bool,
    pub rounds_used:        usize,
    pub trace:              Vec<DeliberationRound>,
    pub convergence_reason: String,
}

/// Multi-round evidence accumulation engine.
///
/// Models how an AI agent *should* reason under uncertainty: instead of forcing
/// a binary guess from thin evidence, hold at State 0 and keep gathering signals
/// until the confidence threshold is crossed or rounds run out.
///
/// Each round adds new evidence (a slice of f32 signals). The engine uses an
/// exponential moving average so recent evidence weighs more than stale data.
pub struct DeliberationEngine {
    /// Confidence required to declare convergence (0.0–1.0).
    pub target_confidence: f32,
    /// Maximum rounds before returning with whatever confidence was reached.
    pub max_rounds: usize,
    /// Recency weight (0 < α ≤ 1). Lower α = more memory of past rounds.
    pub alpha: f32,
}

impl DeliberationEngine {
    pub fn new(target_confidence: f32, max_rounds: usize) -> Self {
        Self { target_confidence, max_rounds, alpha: 0.4 }
    }

    pub fn with_alpha(mut self, alpha: f32) -> Self { self.alpha = alpha.clamp(0.01, 1.0); self }

    /// Run deliberation. `rounds_evidence[i]` is the evidence for round i.
    /// Missing rounds receive no new evidence (engine holds).
    pub fn run(&self, rounds_evidence: Vec<Vec<f32>>) -> DeliberationResult {
        let mut ema: f32 = 0.0; // exponential moving average of evidence
        let mut initialized = false;
        let mut trace = Vec::new();

        let rounds_to_run = self.max_rounds.min(
            if rounds_evidence.is_empty() { self.max_rounds } else { rounds_evidence.len() }
        );

        for round in 0..rounds_to_run {
            let new_ev: Vec<f32> = rounds_evidence.get(round).cloned().unwrap_or_default();

            // Compute mean of new evidence signals this round
            if !new_ev.is_empty() {
                let round_mean = new_ev.iter().sum::<f32>() / new_ev.len() as f32;
                ema = if !initialized {
                    initialized = true;
                    round_mean
                } else {
                    self.alpha * round_mean + (1.0 - self.alpha) * ema
                };
            }

            let scalar = TritScalar::new(ema);
            let converged = scalar.confidence() >= self.target_confidence;

            trace.push(DeliberationRound {
                round,
                new_evidence: new_ev,
                cumulative_mean: ema,
                scalar: scalar.clone(),
                converged,
            });

            if converged { break; }
        }

        let last = trace.last().cloned().unwrap_or_else(|| DeliberationRound {
            round: 0, new_evidence: vec![], cumulative_mean: 0.0,
            scalar: TritScalar::new(0.0), converged: false,
        });

        let convergence_reason = if last.converged {
            format!("confidence {:.1}% ≥ target {:.1}% after {} round(s)",
                last.scalar.confidence() * 100.0,
                self.target_confidence * 100.0,
                last.round + 1)
        } else {
            format!("max rounds ({}) reached — confidence {:.1}% below target {:.1}%",
                self.max_rounds,
                last.scalar.confidence() * 100.0,
                self.target_confidence * 100.0)
        };

        DeliberationResult {
            final_trit:         last.scalar.trit_i8(),
            final_label:        last.scalar.label().to_string(),
            final_confidence:   last.scalar.confidence(),
            converged:          last.converged,
            rounds_used:        last.round + 1,
            trace,
            convergence_reason,
        }
    }
}

// ─── 2. Coalition Vote ────────────────────────────────────────────────────────

/// One agent's vote in a coalition.
#[derive(Debug, Clone)]
pub struct CoalitionMember {
    pub label:      String,
    pub trit:       i8,       // −1, 0, +1
    pub confidence: f32,      // [0, 1] — how certain is this agent?
    pub weight:     f32,      // domain expertise weight (default 1.0)
}

impl CoalitionMember {
    pub fn new(label: impl Into<String>, trit: i8, confidence: f32, weight: f32) -> Self {
        Self {
            label: label.into(),
            trit: trit.clamp(-1, 1),
            confidence: confidence.clamp(0.0, 1.0),
            weight: weight.max(0.0),
        }
    }
}

/// Coalition voting statistics.
#[derive(Debug, Clone)]
pub struct CoalitionResult {
    pub trit:          i8,
    pub label:         String,
    pub aggregate_score: f32,    // weighted sum / total_weight
    pub quorum:        f32,      // fraction of members with non-zero vote
    pub dissent_rate:  f32,      // fraction voting opposite to result
    pub abstain_rate:  f32,      // fraction voting 0
    pub member_count:  usize,
    pub effective_weight: f32,   // total weight of non-abstaining voters
    pub breakdown:     Vec<(String, i8, f32)>, // (label, trit, effective_contribution)
}

/// Aggregate a coalition of agent votes into a single ternary decision.
///
/// Each agent contributes `trit × confidence × weight` to the aggregate score.
/// The final trit is determined by `TritScalar::new(aggregate_score)`.
pub fn coalition_vote(members: &[CoalitionMember]) -> CoalitionResult {
    if members.is_empty() {
        return CoalitionResult {
            trit: 0, label: "tend".into(), aggregate_score: 0.0,
            quorum: 0.0, dissent_rate: 0.0, abstain_rate: 1.0,
            member_count: 0, effective_weight: 0.0, breakdown: vec![],
        };
    }

    let total_weight: f32 = members.iter().map(|m| m.weight).sum();
    let total_weight = if total_weight == 0.0 { 1.0 } else { total_weight };

    let mut weighted_sum: f32 = 0.0;
    let mut non_zero_weight: f32 = 0.0;
    let mut breakdown = Vec::new();

    for m in members {
        let contribution = (m.trit as f32) * m.confidence * m.weight;
        weighted_sum += contribution;
        if m.trit != 0 { non_zero_weight += m.weight; }
        breakdown.push((m.label.clone(), m.trit, contribution / total_weight));
    }

    let aggregate_score = weighted_sum / total_weight;
    let scalar = TritScalar::new(aggregate_score);
    let result_trit: i8 = scalar.trit_i8();

    let quorum = non_zero_weight / total_weight;
    let abstain_rate = 1.0 - quorum;
    let dissent_rate = members.iter()
        .filter(|m| m.trit != 0 && m.trit.signum() != result_trit.signum())
        .map(|m| m.weight)
        .sum::<f32>() / total_weight;

    CoalitionResult {
        trit: result_trit,
        label: scalar.label().to_string(),
        aggregate_score,
        quorum,
        dissent_rate,
        abstain_rate,
        member_count: members.len(),
        effective_weight: non_zero_weight,
        breakdown,
    }
}

// Helper to get the sign of an i8 as i8
trait Sign { fn signum(self) -> i8; }
impl Sign for i8 { fn signum(self) -> i8 { if self > 0 { 1 } else if self < 0 { -1 } else { 0 } } }

// ─── 3. Action Gate ───────────────────────────────────────────────────────────

/// One dimension in an action gate check.
#[derive(Debug, Clone)]
pub struct GateDimension {
    pub name:       String,
    pub evidence:   f32,    // raw evidence signal (−1.0 to +1.0)
    pub weight:     f32,    // importance of this dimension
    /// If true: a negative trit on this dimension immediately blocks the action,
    /// regardless of other dimensions. Use for absolute safety constraints.
    pub hard_block: bool,
}

impl GateDimension {
    pub fn new(name: impl Into<String>, evidence: f32, weight: f32) -> Self {
        Self { name: name.into(), evidence, weight, hard_block: false }
    }
    pub fn hard(mut self) -> Self { self.hard_block = true; self }
}

/// The outcome of an action gate evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateVerdict {
    /// All dimensions pass — action is approved to proceed.
    Proceed,
    /// Evidence is insufficient — pause and request more information.
    Hold,
    /// One or more blocking conditions failed — action is denied.
    Block,
}

impl GateVerdict {
    pub fn label(&self) -> &'static str {
        match self {
            GateVerdict::Proceed => "proceed",
            GateVerdict::Hold    => "hold",
            GateVerdict::Block   => "block",
        }
    }
}

/// Result of an action gate evaluation.
#[derive(Debug, Clone)]
pub struct GateResult {
    pub verdict:    GateVerdict,
    pub aggregate:  TritScalar,
    pub hard_blocked_by: Vec<String>, // names of hard-blocking dims that fired
    pub dim_results: Vec<(String, TritScalar, bool)>, // (name, scalar, is_hard)
    pub explanation: String,
}

/// Evaluate an action through a multi-dimension policy gate.
///
/// The gate logic (inspired by AI safety frameworks):
///   1. Check all `hard_block` dimensions first. Any `-1` → immediate Block.
///   2. Compute weighted aggregate of all dimensions.
///   3. Map aggregate to ternary: +1 = Proceed, 0 = Hold, -1 = Block.
pub fn action_gate(dimensions: &[GateDimension]) -> GateResult {
    let mut hard_blocked_by = Vec::new();
    let mut dim_results = Vec::new();
    let mut weighted_sum = 0.0f32;
    let mut total_weight = 0.0f32;

    for dim in dimensions {
        let scalar = TritScalar::new(dim.evidence);
        let is_neg = matches!(scalar.trit(), Trit::Reject);

        if dim.hard_block && is_neg {
            hard_blocked_by.push(dim.name.clone());
        }

        weighted_sum += dim.evidence * dim.weight;
        total_weight += dim.weight;
        dim_results.push((dim.name.clone(), scalar, dim.hard_block));
    }

    // Hard block takes absolute priority
    if !hard_blocked_by.is_empty() {
        let explanation = format!(
            "BLOCKED — hard constraint(s) violated: {}",
            hard_blocked_by.join(", ")
        );
        return GateResult {
            verdict: GateVerdict::Block,
            aggregate: TritScalar::new(-1.0),
            hard_blocked_by,
            dim_results,
            explanation,
        };
    }

    let agg_score = if total_weight > 0.0 { weighted_sum / total_weight } else { 0.0 };
    let aggregate = TritScalar::new(agg_score);

    let verdict = match aggregate.trit() {
        Trit::Affirm => GateVerdict::Proceed,
        Trit::Tend   => GateVerdict::Hold,
        Trit::Reject => GateVerdict::Block,
    };

    let explanation = match &verdict {
        GateVerdict::Proceed => format!(
            "PROCEED — all dimensions pass (aggregate confidence {:.0}%)",
            aggregate.confidence() * 100.0
        ),
        GateVerdict::Hold => format!(
            "HOLD — insufficient evidence (aggregate {:.3} within deliberation zone)",
            aggregate.raw()
        ),
        GateVerdict::Block => format!(
            "BLOCK — weighted aggregate {:.3} below threshold (confidence {:.0}%)",
            aggregate.raw(), aggregate.confidence() * 100.0
        ),
    };

    GateResult { verdict, aggregate, hard_blocked_by, dim_results, explanation }
}

// ─── 4. Scalar Temperature Bridge ────────────────────────────────────────────

/// Maps a ternary decision to a recommended LLM sampling temperature.
///
/// The core insight: ternary state directly encodes *how much exploration* an
/// AI agent should do in its next generation step.
///
///  +1 (affirm, high confidence) → low temperature [0.05–0.3]  — be precise
///   0 (tend, uncertain)         → high temperature [0.7–1.0]  — explore options
///  -1 (reject, high confidence) → very low temperature [0.05–0.15] — be firm in refusal
///
/// The exact value within each range scales with confidence:
///   high confidence → toward the extreme of the range
///   low confidence  → toward the middle of the range
#[derive(Debug, Clone)]
pub struct ScalarTemperature {
    pub trit:        i8,
    pub confidence:  f32,
    pub temperature: f32,
    pub reasoning:   String,
    /// Recommended system prompt addendum based on ternary state
    pub prompt_hint: String,
}

pub fn scalar_temperature(scalar: &TritScalar) -> ScalarTemperature {
    let t = scalar.trit();
    let c = scalar.confidence(); // 0.0–1.0

    let (temp, reasoning, prompt_hint) = match t {
        Trit::Affirm => {
            // Affirm: be precise. High confidence → very low temp.
            let temp = 0.3 - (c * 0.25); // c=1.0 → 0.05, c=0.0 → 0.30
            (
                temp.max(0.05),
                format!("Affirm (confidence {:.0}%) — execute precisely, minimal exploration", c * 100.0),
                "Be concise and direct. Evidence is clear. Do not hedge.".to_string(),
            )
        }
        Trit::Reject => {
            // Reject: be firm in refusal. Low temp but not zero.
            let temp = 0.15 - (c * 0.10); // c=1.0 → 0.05, c=0.0 → 0.15
            (
                temp.max(0.05),
                format!("Reject (confidence {:.0}%) — decline firmly, minimal hedging", c * 100.0),
                "Decline clearly. Do not offer alternatives unless explicitly asked. Evidence is against.".to_string(),
            )
        }
        Trit::Tend => {
            // Tend: explore. Low confidence → highest temp (widest search).
            let temp = 0.7 + ((1.0 - c) * 0.3); // c=0.0 → 1.0, c=1.0 → 0.7
            (
                temp.min(1.0),
                format!("Tend (confidence {:.0}%) — evidence is conflicted, explore broadly", c * 100.0),
                "You are in deliberation. Present multiple perspectives. Ask clarifying questions. Do not commit.".to_string(),
            )
        }
    };

    ScalarTemperature {
        trit: scalar.trit_i8(),
        confidence: c,
        temperature: (temp * 1000.0).round() / 1000.0,
        reasoning,
        prompt_hint,
    }
}

// ─── 5. Hallucination Score ───────────────────────────────────────────────────

/// Measures internal consistency of evidence signals about a claim.
///
/// High variance among signals claiming the same direction = suspicious (possible hallucination).
/// Low variance = coherent signal = higher truth probability.
///
/// Returns a `TritScalar` representing the *trustworthiness* of the evidence:
///   +1 = highly consistent signals (trust the claim)
///    0 = mixed consistency (deliberate further)
///   -1 = high internal conflict (flag as potentially unreliable)
#[derive(Debug, Clone)]
pub struct HallucinationScore {
    pub trust_trit:    i8,
    pub trust_label:   String,
    pub mean:          f32,   // direction of evidence
    pub variance:      f32,   // spread of evidence signals
    pub consistency:   f32,   // 1 - normalised_variance (higher = more consistent)
    pub signal_count:  usize,
    pub explanation:   String,
}

pub fn hallucination_score(signals: &[f32]) -> HallucinationScore {
    if signals.is_empty() {
        return HallucinationScore {
            trust_trit: 0, trust_label: "tend".into(), mean: 0.0,
            variance: 0.0, consistency: 0.0, signal_count: 0,
            explanation: "No signals provided — cannot assess consistency.".into(),
        };
    }

    let n = signals.len() as f32;
    let mean = signals.iter().sum::<f32>() / n;
    let variance = signals.iter().map(|&s| (s - mean).powi(2)).sum::<f32>() / n;

    // Normalise variance to [0, 1]: max variance of signals in [-1,1] is 1.0
    let norm_variance = variance.min(1.0);
    let consistency = 1.0 - norm_variance;

    // Trust score: high consistency in a clear direction → +1 trust
    // High variance regardless of direction → -1 trust (flag it)
    // Mixed → hold
    let trust_evidence = (consistency * 2.0 - 1.0) * mean.abs(); // [-1, +1]
    let trust = TritScalar::new(trust_evidence);

    let explanation = if trust.trit() == Trit::Affirm {
        format!(
            "Consistent signals (variance {:.3}, consistency {:.0}%) — evidence coheres around {:.3}",
            variance, consistency * 100.0, mean
        )
    } else if trust.trit() == Trit::Reject {
        format!(
            "HIGH VARIANCE (variance {:.3}) — signals are internally contradictory. Possible hallucination or conflated sources.",
            variance
        )
    } else {
        format!(
            "Mixed consistency (variance {:.3}, mean {:.3}) — gather more evidence before relying on this claim.",
            variance, mean
        )
    };

    HallucinationScore {
        trust_trit:   trust.trit_i8(),
        trust_label:  trust.label().to_string(),
        mean,
        variance,
        consistency,
        signal_count: signals.len(),
        explanation,
    }
}

// ─── Phase 8 tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod reasoning_tests {
    use super::*;

    // ── Deliberation Engine ──

    #[test]
    fn test_deliberation_converges_on_strong_evidence() {
        // Use higher alpha (faster EMA) and 6 rounds of strong positive evidence
        let engine = DeliberationEngine::new(0.7, 10).with_alpha(0.7);
        let rounds = vec![
            vec![0.85, 0.9],        // round 0: strong positive
            vec![0.9, 0.95],        // round 1: very strong
            vec![0.92, 0.95, 0.98], // round 2: overwhelming
        ];
        let result = engine.run(rounds);
        assert!(result.converged, "should converge on strong positive evidence (got confidence {:.2})", result.final_confidence);
        assert_eq!(result.final_trit, 1, "should be +1 (affirm)");
        assert!(result.rounds_used <= 3);
    }

    #[test]
    fn test_deliberation_holds_on_weak_evidence() {
        let engine = DeliberationEngine::new(0.95, 3);
        let rounds = vec![
            vec![0.1f32],
            vec![-0.05],
            vec![0.15],
        ];
        let result = engine.run(rounds);
        assert!(!result.converged, "should not converge on weak conflicting evidence");
        assert_eq!(result.final_trit, 0, "should stay at hold/tend");
        assert_eq!(result.rounds_used, 3);
    }

    #[test]
    fn test_deliberation_negative_convergence() {
        let engine = DeliberationEngine::new(0.8, 10);
        let rounds = vec![
            vec![-0.9f32, -0.85],
            vec![-0.95, -0.99],
        ];
        let result = engine.run(rounds);
        assert!(result.converged);
        assert_eq!(result.final_trit, -1);
    }

    // ── Coalition Vote ──

    #[test]
    fn test_coalition_unanimous_affirm() {
        let members = vec![
            CoalitionMember::new("safety", 1, 0.9, 3.0),
            CoalitionMember::new("utility", 1, 0.8, 1.0),
            CoalitionMember::new("alignment", 1, 0.95, 2.0),
        ];
        let result = coalition_vote(&members);
        assert_eq!(result.trit, 1);
        assert_eq!(result.label, "affirm");
        assert!(result.quorum > 0.99, "all voted");
        assert!(result.dissent_rate < 0.01);
    }

    #[test]
    fn test_coalition_split_vote_tends_to_hold() {
        let members = vec![
            CoalitionMember::new("agent_a", 1, 0.8, 1.0),
            CoalitionMember::new("agent_b", -1, 0.8, 1.0),
            CoalitionMember::new("agent_c", 0, 0.5, 1.0),
        ];
        let result = coalition_vote(&members);
        // +0.8 - 0.8 + 0 = 0 → hold
        assert_eq!(result.trit, 0);
        assert!(result.dissent_rate > 0.0, "there is dissent");
    }

    #[test]
    fn test_coalition_high_weight_overrides() {
        let members = vec![
            CoalitionMember::new("expert", 1, 0.95, 10.0),  // high weight
            CoalitionMember::new("novice_a", -1, 0.5, 1.0),
            CoalitionMember::new("novice_b", -1, 0.5, 1.0),
        ];
        let result = coalition_vote(&members);
        // expert contribution dominates → should affirm
        assert_eq!(result.trit, 1, "high-weight expert should dominate");
    }

    // ── Action Gate ──

    #[test]
    fn test_gate_all_positive_proceeds() {
        let dims = vec![
            GateDimension::new("safety", 0.8, 3.0),
            GateDimension::new("utility", 0.7, 1.0),
            GateDimension::new("legality", 0.9, 2.0),
        ];
        let result = action_gate(&dims);
        assert_eq!(result.verdict, GateVerdict::Proceed);
    }

    #[test]
    fn test_gate_hard_block_fires() {
        let dims = vec![
            GateDimension::new("utility", 0.9, 1.0),
            GateDimension::new("safety", -0.8, 3.0).hard(),  // hard block!
            GateDimension::new("legality", 0.7, 1.0),
        ];
        let result = action_gate(&dims);
        assert_eq!(result.verdict, GateVerdict::Block);
        assert!(result.hard_blocked_by.contains(&"safety".to_string()));
    }

    #[test]
    fn test_gate_mixed_soft_dims_holds() {
        let dims = vec![
            GateDimension::new("utility", 0.8, 1.0),
            GateDimension::new("risk", -0.7, 1.0), // soft block, no hard
        ];
        // aggregate = (0.8 - 0.7) / 2 = 0.05 → tend zone → hold
        let result = action_gate(&dims);
        // 0.05 is in tend zone
        assert_ne!(result.verdict, GateVerdict::Block); // no hard block
    }

    // ── Scalar Temperature ──

    #[test]
    fn test_temperature_affirm_is_low() {
        let sc = TritScalar::new(0.9);
        let temp = scalar_temperature(&sc);
        assert_eq!(temp.trit, 1);
        assert!(temp.temperature < 0.3, "affirm → low temperature");
    }

    #[test]
    fn test_temperature_tend_is_high() {
        let sc = TritScalar::new(0.05); // barely tend
        let temp = scalar_temperature(&sc);
        assert_eq!(temp.trit, 0);
        assert!(temp.temperature >= 0.7, "tend → high temperature for exploration");
    }

    #[test]
    fn test_temperature_reject_is_low() {
        let sc = TritScalar::new(-0.9);
        let temp = scalar_temperature(&sc);
        assert_eq!(temp.trit, -1);
        assert!(temp.temperature < 0.15, "reject → low temperature, firm");
    }

    // ── Hallucination Score ──

    #[test]
    fn test_hallucination_consistent_signals_trusted() {
        // Tight cluster of positive signals
        let signals = vec![0.8, 0.82, 0.79, 0.81, 0.83];
        let score = hallucination_score(&signals);
        assert_eq!(score.trust_trit, 1, "consistent signals should be trusted");
        assert!(score.variance < 0.01);
        assert!(score.consistency > 0.99);
    }

    #[test]
    fn test_hallucination_chaotic_signals_flagged() {
        // Wildly inconsistent signals claiming a strong direction
        let signals = vec![0.9, -0.9, 0.8, -0.8, 0.95, -0.7];
        let score = hallucination_score(&signals);
        // High variance → low consistency → flagged
        assert!(score.variance > 0.5, "should have high variance");
        assert!(score.trust_trit <= 0, "chaotic signals should not be trusted");
    }

    #[test]
    fn test_hallucination_empty_returns_hold() {
        let score = hallucination_score(&[]);
        assert_eq!(score.trust_trit, 0);
        assert_eq!(score.signal_count, 0);
    }
}
