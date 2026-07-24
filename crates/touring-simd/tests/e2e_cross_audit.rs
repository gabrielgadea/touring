//! E2E Cross-Audit: Verifies touring-simd v0.2.0 fulfills its documented purpose.
//!
//! This is NOT a unit test. It verifies:
//! 1. **Interface contracts**: public API returns correct types with correct semantics
//! 2. **Integration**: components work together (simd_utils → similarity → topk → cortex)
//! 3. **Invariants**: SIMD backend detection, precision guarantees, edge cases
//! 4. **Purpose alignment**: each module does what its documentation claims
//!
//! Run: `cargo test --test e2e_cross_audit --features "ann,quantization"`

use touring_simd::financial;
use touring_simd::simd_utils::*;
use touring_simd::similarity::*;
use touring_simd::statistics::*;

// ============================================================
// AUDIT 1: SIMD Foundation — pulp dispatch works end-to-end
// ============================================================

#[test]
fn audit_simd_backend_detection() {
    // Contract: backend_name() returns a non-empty string identifying the ISA
    let name = backend_name();
    assert!(!name.is_empty(), "Backend name must not be empty");

    // On x86_64 CI/dev machines, should be AVX2 or better
    #[cfg(target_arch = "x86_64")]
    {
        let valid = ["AVX-512", "AVX2+FMA", "AVX2", "SSE4.1", "scalar"];
        assert!(valid.contains(&name), "Unknown backend: {name}");
    }
}

#[test]
fn audit_dot_product_mathematical_correctness() {
    // Contract: dot(a,b) = Σ(a[i]*b[i]) with SIMD precision
    let a: Vec<f32> = (0..1536).map(|i| (i as f32 * 0.001).sin()).collect();
    let b: Vec<f32> = (0..1536).map(|i| (i as f32 * 0.002).cos()).collect();

    let simd_result = simd_dot_f32(&a, &b);
    let portable_result = portable_dot_f32(&a, &b);

    // Both dispatch paths must agree (they both use pulp now)
    assert!(
        (simd_result - portable_result).abs() < 1e-4,
        "simd_dot_f32 ({simd_result}) != portable_dot_f32 ({portable_result})"
    );

    // Verify against naive scalar computation
    let naive: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
    assert!(
        (simd_result - naive).abs() < 1e-2,
        "SIMD result ({simd_result}) diverges from naive ({naive})"
    );
}

#[test]
fn audit_dot_product_edge_cases() {
    // Empty vectors
    assert_eq!(simd_dot_f32(&[], &[]), 0.0, "Empty dot must be 0");
    assert_eq!(portable_dot_f64(&[], &[]), 0.0, "Empty f64 dot must be 0");

    // Single element
    assert!((simd_dot_f32(&[3.0], &[4.0]) - 12.0).abs() < 1e-6);

    // Odd lengths (not divisible by SIMD width)
    for len in [1, 3, 7, 13, 15, 17, 31, 33] {
        let a: Vec<f32> = (0..len).map(|i| i as f32).collect();
        let b: Vec<f32> = (0..len).map(|i| (len - i) as f32).collect();
        let result = simd_dot_f32(&a, &b);
        let expected: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
        assert!(
            (result - expected).abs() < 1e-3,
            "Dot product wrong for len={len}: got {result}, expected {expected}"
        );
    }
}

#[test]
fn audit_element_wise_ops_roundtrip() {
    // Contract: add(a, b) - b == a (within precision)
    let a = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];
    let b = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9];
    let mut sum = vec![0.0; 9];
    let mut diff = vec![0.0; 9];

    simd_add_f32(&a, &b, &mut sum);
    simd_sub_f32(&sum, &b, &mut diff);

    for (i, (&original, &restored)) in a.iter().zip(&diff).enumerate() {
        assert!(
            (original - restored).abs() < 1e-5,
            "Roundtrip failed at [{i}]: {original} != {restored}"
        );
    }
}

#[test]
fn audit_norm_and_scale_invariant() {
    // Contract: ||scale(a, c)|| == |c| * ||a||
    let a = vec![3.0, 4.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    let mut scaled = vec![0.0; 8];
    simd_scale_f32(&a, 2.5, &mut scaled);

    let norm_a = simd_norm_f32(&a);
    let norm_scaled = simd_norm_f32(&scaled);

    assert!(
        (norm_scaled - 2.5 * norm_a).abs() < 1e-5,
        "Scale invariant violated: ||2.5a|| = {norm_scaled}, 2.5*||a|| = {}",
        2.5 * norm_a
    );
}

// ============================================================
// AUDIT 2: Similarity — Cosine single-pass correctness
// ============================================================

#[test]
fn audit_cosine_single_pass_vs_naive() {
    // Contract: single-pass cosine must match 3-pass naive computation
    let computer = CosineComputer::new();

    let a: Vec<f32> = (0..1536).map(|i| (i as f32 * 0.001).sin()).collect();
    let b: Vec<f32> = (0..1536).map(|i| (i as f32 * 0.002).cos()).collect();

    let single_pass = computer.cosine(&a, &b);

    // Naive 3-pass in f64 (ground truth)
    let dot: f64 = a.iter().zip(&b).map(|(&x, &y)| x as f64 * y as f64).sum();
    let na: f64 = a
        .iter()
        .map(|&x| (x as f64) * (x as f64))
        .sum::<f64>()
        .sqrt();
    let nb: f64 = b
        .iter()
        .map(|&x| (x as f64) * (x as f64))
        .sum::<f64>()
        .sqrt();
    let naive = dot / (na * nb);

    assert!(
        (single_pass - naive).abs() < 1e-5,
        "Single-pass ({single_pass}) diverges from f64 naive ({naive})"
    );
}

#[test]
fn audit_cosine_mathematical_properties() {
    let computer = CosineComputer::new();

    // Property 1: cos(a, a) == 1.0 (self-similarity)
    let a = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
    assert!(
        (computer.cosine(&a, &a) - 1.0).abs() < 1e-6,
        "Self-similarity must be 1.0"
    );

    // Property 2: cos(a, -a) == -1.0 (opposite)
    let neg_a: Vec<f32> = a.iter().map(|x| -x).collect();
    assert!(
        (computer.cosine(&a, &neg_a) - (-1.0)).abs() < 1e-6,
        "Opposite must be -1.0"
    );

    // Property 3: cos(a, zero) == 0.0
    let zero = vec![0.0; 8];
    assert_eq!(
        computer.cosine(&a, &zero),
        0.0,
        "Zero vector cosine must be 0"
    );

    // Property 4: cos(a, b) == cos(b, a) (symmetry)
    let b = vec![8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0];
    let ab = computer.cosine(&a, &b);
    let ba = computer.cosine(&b, &a);
    assert!(
        (ab - ba).abs() < 1e-10,
        "Symmetry violated: cos(a,b)={ab} != cos(b,a)={ba}"
    );

    // Property 5: cos is in [-1, 1]
    assert!((-1.0..=1.0).contains(&ab), "Cosine out of range: {ab}");

    // Property 6: dimension mismatch returns 0
    assert_eq!(
        computer.cosine(&[1.0, 2.0], &[1.0]),
        0.0,
        "Dimension mismatch must return 0"
    );
}

#[test]
fn audit_cosine_mixed_precision_high_dim() {
    // Contract: f64 mixed-precision accumulation prevents catastrophic cancellation
    // in high-dimensional vectors where f32 accumulation fails
    let computer = CosineComputer::new();

    // Large uniform vectors with tiny perturbation — stresses floating point
    let dim = 4096;
    let a: Vec<f32> = (0..dim).map(|_| 1.0f32).collect();
    let mut b = a.clone();
    b[0] = 1.001; // tiny perturbation

    let result = computer.cosine(&a, &b);

    // Must be very close to 1.0 (FMA precision may push slightly above 1.0)
    assert!(
        (result - 1.0).abs() < 1e-4,
        "High-dim cosine precision failed: {result} (expected ~1.0)"
    );
}

// ============================================================
// AUDIT 3: Distance Metrics — SIMD vs scalar agreement
// ============================================================

#[test]
fn audit_distance_metrics_consistency() {
    let a = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
    let b = vec![8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0];

    // Euclidean: sqrt(sum((a-b)^2))
    let euc = distance::euclidean(&a, &b);
    let euc_expected: f32 = a
        .iter()
        .zip(&b)
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt();
    assert!(
        (euc - euc_expected).abs() < 1e-4,
        "Euclidean: {euc} vs expected {euc_expected}"
    );

    // Manhattan: sum(|a-b|) — now SIMD-accelerated
    let man = distance::manhattan(&a, &b);
    let man_expected: f32 = a.iter().zip(&b).map(|(x, y)| (x - y).abs()).sum();
    assert!(
        (man - man_expected).abs() < 1e-4,
        "Manhattan: {man} vs expected {man_expected}"
    );

    // Pearson: correlation coefficient
    let pearson = distance::pearson_correlation(&a, &b);
    assert!(
        (pearson - (-1.0)).abs() < 1e-6,
        "Perfectly inversely correlated vectors should have pearson = -1.0, got {pearson}"
    );

    // Dimension mismatch returns sentinel
    assert!(distance::euclidean(&[1.0], &[1.0, 2.0]).is_infinite());
    assert!(distance::manhattan(&[1.0], &[1.0, 2.0]).is_infinite());
}

// ============================================================
// AUDIT 4: TopK — Integration with cosine
// ============================================================

#[test]
fn audit_topk_finds_exact_match() {
    let searcher = TopKSearcher::new(5);

    let query = vec![1.0, 0.0, 0.0];
    let candidates = vec![
        vec![0.0, 1.0, 0.0],  // orthogonal
        vec![-1.0, 0.0, 0.0], // opposite
        vec![1.0, 0.0, 0.0],  // IDENTICAL
        vec![0.5, 0.5, 0.0],  // partial match
        vec![0.9, 0.1, 0.0],  // close match
    ];

    let results = searcher.top_k(&query, &candidates, 3);

    assert_eq!(results.len(), 3);
    // Identical vector must be top-1
    assert_eq!(results[0].index, 2, "Identical vector must be top result");
    assert!(
        (results[0].score - 1.0).abs() < 1e-6,
        "Identical score must be 1.0"
    );
    // Results must be sorted descending
    for w in results.windows(2) {
        assert!(w[0].score >= w[1].score, "TopK not sorted");
    }
}

#[test]
fn audit_topk_batch_correctness() {
    let searcher = TopKSearcher::new(2);

    let queries = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
    let candidates = vec![vec![1.0, 0.0], vec![0.0, 1.0], vec![0.5, 0.5]];

    let results = searcher.top_k_batch(&queries, &candidates, 2);
    assert_eq!(results.len(), 2, "One result set per query");

    // Query [1,0] should find [1,0] as top-1
    assert_eq!(results[0][0].index, 0);
    // Query [0,1] should find [0,1] as top-1
    assert_eq!(results[1][0].index, 1);
}

// ============================================================
// AUDIT 5: Similarity Traits — Slice and Vec implementations
// ============================================================

#[test]
fn audit_similarity_trait_slice_and_vec() {
    let computer = CosineComputer::new();
    let a = vec![1.0f32, 0.0, 0.0];
    let b = vec![1.0f32, 0.0, 0.0];

    // Similarity<[f32]> — new slice-based API
    let slice_result: f64 = <CosineComputer as Similarity<[f32]>>::similarity(&computer, &a, &b);
    assert!(
        (slice_result - 1.0).abs() < 1e-6,
        "Slice-based Similarity trait failed"
    );

    // Similarity<Vec<f32>> — backward-compatible API
    let vec_result: f64 = <CosineComputer as Similarity<Vec<f32>>>::similarity(&computer, &a, &b);
    assert!(
        (vec_result - 1.0).abs() < 1e-6,
        "Vec-based Similarity trait failed"
    );

    // Both must agree
    assert_eq!(slice_result, vec_result, "Slice and Vec traits must agree");
}

#[test]
fn audit_jaccard_trait_slice_and_vec() {
    let computer = JaccardComputer::new();
    let a = vec![1u32, 2, 3, 4, 5];
    let b = vec![1u32, 2, 3, 4, 5];

    let slice_result: f64 = <JaccardComputer as Similarity<[u32]>>::similarity(&computer, &a, &b);
    let vec_result: f64 = <JaccardComputer as Similarity<Vec<u32>>>::similarity(&computer, &a, &b);

    assert_eq!(slice_result, 1.0, "Identical sets must have Jaccard 1.0");
    assert_eq!(slice_result, vec_result, "Slice and Vec traits must agree");
}

// ============================================================
// AUDIT 6: Matrix Operations — Flat vs Legacy agreement
// ============================================================

#[test]
fn audit_matrix_flat_vs_legacy() {
    let rows = 3;
    let cols = 4;

    // Legacy format
    let legacy = vec![
        vec![1.0, 2.0, 3.0, 4.0],
        vec![5.0, 6.0, 7.0, 8.0],
        vec![9.0, 10.0, 11.0, 12.0],
    ];

    // Flat format
    let flat_data: Vec<f32> = legacy.iter().flat_map(|r| r.iter().copied()).collect();
    let flat = matrix::MatrixView::new(&flat_data, rows, cols);

    let v = vec![1.0, 0.0, 0.0, 0.0];

    // Legacy result
    let mut legacy_out = vec![0.0; 3];
    matrix::mat_vec_mul(&legacy, &v, &mut legacy_out);

    // Flat result
    let mut flat_out = vec![0.0; 3];
    matrix::mat_vec_mul_flat(flat, &v, &mut flat_out);

    // Must agree
    for (i, (l, f)) in legacy_out.iter().zip(&flat_out).enumerate() {
        assert!((l - f).abs() < 1e-6, "Row {i}: legacy={l} != flat={f}");
    }
}

#[test]
fn audit_softmax_properties() {
    // Softmax output must: sum to 1, all positive, preserve ordering
    let x = vec![1.0, 3.0, 2.0, 0.5, 4.0];
    let result = matrix::softmax(&x);

    let sum: f32 = result.iter().sum();
    assert!(
        (sum - 1.0).abs() < 1e-5,
        "Softmax sum must be 1.0, got {sum}"
    );
    assert!(
        result.iter().all(|&v| v > 0.0),
        "Softmax values must all be positive"
    );

    // Largest input → largest output
    let max_idx = x
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .unwrap()
        .0;
    let max_out_idx = result
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .unwrap()
        .0;
    assert_eq!(max_idx, max_out_idx, "Softmax must preserve ordering");
}

#[test]
fn audit_relu_properties() {
    let x = vec![-5.0, -1.0, 0.0, 1.0, 5.0, -0.001, 100.0, -100.0, 0.001];
    let result = matrix::relu(&x);

    for (i, (&input, &output)) in x.iter().zip(&result).enumerate() {
        if input > 0.0 {
            assert_eq!(output, input, "ReLU must preserve positive [{i}]");
        } else {
            assert_eq!(output, 0.0, "ReLU must zero negative [{i}]");
        }
    }
}

// ============================================================
// AUDIT 7: Statistics — Mathematical invariants
// ============================================================

#[test]
fn audit_wilson_score_monotonicity() {
    // More successes with same total → higher score
    let ranker = WilsonRanker::default();
    let scores: Vec<f64> = (0..=10).map(|s| ranker.wilson_score(s, 10, 0.95)).collect();

    for w in scores.windows(2) {
        assert!(
            w[1] >= w[0],
            "Wilson score not monotonic: {} < {}",
            w[1],
            w[0]
        );
    }
}

#[test]
fn audit_ks_statistic_properties() {
    let detector = DriftDetector::new();

    // KS(a, a) == 0 (identical distributions)
    let a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
    assert!(
        detector.ks_statistic(&a, &a) < 1e-10,
        "KS of identical must be 0"
    );

    // KS(a, b) in [0, 1]
    let b = vec![6.0, 7.0, 8.0, 9.0, 10.0];
    let ks = detector.ks_statistic(&a, &b);
    assert!((0.0..=1.0).contains(&ks), "KS out of range: {ks}");

    // Completely disjoint → KS = 1.0
    assert!(
        (ks - 1.0).abs() < 1e-10,
        "Disjoint samples should have KS=1.0, got {ks}"
    );
}

#[test]
fn audit_bayesian_fusion_properties() {
    use touring_simd::statistics::reconciliation::bayesian_fusion;

    // High confidence dominates
    let (val, conf) = bayesian_fusion(&[(100.0, 0.99), (0.0, 0.01)]);
    assert!(val > 95.0, "High confidence must dominate: {val}");
    assert!(conf > 0.99, "Combined conf must exceed max: {conf}");

    // Equal confidence → arithmetic mean
    let (val, _) = bayesian_fusion(&[(10.0, 0.5), (20.0, 0.5)]);
    assert!(
        (val - 15.0).abs() < 1e-10,
        "Equal conf must give mean: {val}"
    );
}

// ============================================================
// AUDIT 8: Financial — NPV/IRR mathematical properties
// ============================================================

#[test]
fn audit_npv_irr_consistency() {
    // IRR is the rate at which NPV == 0
    let cfs = [-1000.0, 400.0, 400.0, 400.0];
    if let Some(irr) = financial::irr(&cfs, 1e-8, 200) {
        let npv_at_irr = financial::npv(&cfs, irr);
        assert!(
            npv_at_irr.abs() < 1e-4,
            "NPV at IRR must be ~0, got {npv_at_irr}"
        );
    }

    // NPV at 0% = sum of cash flows
    let npv_zero = financial::npv(&cfs, 0.0);
    let sum: f64 = cfs.iter().sum();
    assert!(
        (npv_zero - sum).abs() < 1e-10,
        "NPV at 0% must equal sum: {npv_zero} vs {sum}"
    );
}

// ============================================================
// AUDIT 9: Integration — Full pipeline cosine → topk → cortex
// ============================================================

#[cfg(feature = "cortex-integration")]
#[test]
fn audit_full_pipeline_cortex_integration() {
    // Simulate the full cortex embedding pipeline:
    // 1. Normalize vectors
    // 2. Compute similarity via cortex module
    // 3. Find top-K via cortex module
    // 4. Verify results are consistent

    let mut embeddings: Vec<Vec<f32>> = vec![
        vec![1.0, 0.0, 0.0, 0.0],
        vec![0.0, 1.0, 0.0, 0.0],
        vec![0.9, 0.1, 0.0, 0.0],
        vec![0.0, 0.0, 1.0, 0.0],
        vec![0.8, 0.2, 0.0, 0.0],
    ];

    // Normalize in place
    for emb in &mut embeddings {
        let norm = simd_norm_f32(emb);
        if norm > 0.0 {
            for v in emb.iter_mut() {
                *v /= norm;
            }
        }
    }

    let query = &embeddings[0]; // normalized [1,0,0,0]

    // Cortex batch similarity
    let sims = touring_simd::cortex::embedding_batch_similarity(query, &embeddings);
    assert_eq!(sims.len(), 5);
    assert!((sims[0] - 1.0).abs() < 1e-5, "Self-similarity must be 1.0");

    // TopK via cortex
    let top_results = touring_simd::cortex::embedding_top_k(query, &embeddings, 3);
    assert_eq!(top_results.len(), 3);
    assert_eq!(top_results[0].index, 0, "Self must be top-1");

    // Top-3 should include the most similar vectors (indices 0, 2, 4)
    let top_indices: Vec<usize> = top_results.iter().map(|r| r.index).collect();
    assert!(
        top_indices.contains(&0) && top_indices.contains(&2),
        "Top-3 should include self (0) and most similar (2), got: {top_indices:?}"
    );
}

// ============================================================
// AUDIT 10: Learning module integration
// ============================================================

#[cfg(feature = "learning-integration")]
#[test]
fn audit_learning_knn_integration() {
    let query = vec![1.0, 0.0, 0.0];
    let candidates = vec![
        vec![1.0, 0.0, 0.0],
        vec![0.0, 1.0, 0.0],
        vec![0.9, 0.1, 0.0],
    ];

    // KNN via learning module
    let results = touring_simd::learning::simd_knn_search(&query, &candidates, 2);
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].index, 0, "KNN must find exact match");

    // Batch dot products
    let dots = touring_simd::learning::batch_dot_products(&query, &candidates);
    assert_eq!(dots.len(), 3);
    assert!((dots[0] - 1.0).abs() < 1e-6, "Dot with self must be 1.0");
    assert!(
        (dots[1] - 0.0).abs() < 1e-6,
        "Dot with orthogonal must be 0"
    );
}

// ============================================================
// AUDIT 11: Reduce operations
// ============================================================

#[test]
fn audit_reduce_operations_correctness() {
    let data: Vec<f32> = (1..=100).map(|i| i as f32).collect();

    // Sum: 1 + 2 + ... + 100 = 5050
    let sum = reduce_sum_f32(&data);
    assert!((sum - 5050.0).abs() < 1e-2, "reduce_sum: {sum} != 5050");

    // Max
    assert_eq!(reduce_max_f32(&data), 100.0);

    // Min
    assert_eq!(reduce_min_f32(&data), 1.0);

    // Argmax / argmin
    assert_eq!(argmax_f32(&data), Some(99));
    assert_eq!(argmin_f32(&data), Some(0));

    // f64 sum
    let data_f64: Vec<f64> = data.iter().map(|&x| x as f64).collect();
    let sum_f64 = reduce_sum_f64(&data_f64);
    assert!((sum_f64 - 5050.0).abs() < 1e-10);
}

// ============================================================
// AUDIT 12: Buffer pool thread-local reuse
// ============================================================

#[test]
fn audit_buffer_pool_reuse() {
    use touring_simd::buffer_pool::with_f32_buffer;

    // First call
    let result1 = with_f32_buffer(100, |buf| {
        buf[0] = 42.0;
        buf.len()
    });
    assert_eq!(result1, 100);

    // Second call reuses memory (no allocation visible but works)
    let result2 = with_f32_buffer(50, |buf| buf.len());
    assert_eq!(result2, 50);

    // Large call
    let result3 = with_f32_buffer(10000, |buf| {
        assert_eq!(buf.len(), 10000);
        buf.iter().sum::<f32>()
    });
    // Buffer is reused — may contain stale data from prior calls, which is expected
    // The pool's purpose is to avoid allocation, not to guarantee zeroed data
    assert!(result3.is_finite());
}

// ============================================================
// AUDIT 13: Quantization — Scalar quantizer roundtrip
// ============================================================

#[test]
fn audit_scalar_quantization_accuracy() {
    use touring_simd::quantization::ScalarQuantizer;

    // Real-world scenario: embedding values in [-1, 1]
    let original: Vec<f32> = (0..128).map(|i| (i as f32 / 64.0) - 1.0).collect();

    let quantizer = ScalarQuantizer::fit(&original);
    let quantized = quantizer.quantize(&original);
    let restored = quantizer.dequantize(&quantized);

    // Max error should be small (within 1/255 of range)
    let max_error: f32 = original
        .iter()
        .zip(&restored)
        .map(|(o, r)| (o - r).abs())
        .fold(0.0f32, f32::max);

    let range = quantizer.max - quantizer.min;
    let theoretical_max_error = range / 255.0;

    assert!(
        max_error <= theoretical_max_error + 1e-6,
        "Quantization error ({max_error}) exceeds theoretical max ({theoretical_max_error})"
    );
}

#[test]
fn audit_quantized_dot_approximation() {
    use touring_simd::quantization::ScalarQuantizer;

    let a = vec![0.1, 0.5, 0.9, 0.3, 0.7, 0.2, 0.8, 0.4];
    let b = vec![0.9, 0.5, 0.1, 0.7, 0.3, 0.8, 0.2, 0.6];

    let exact_dot: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();

    let mut all = a.clone();
    all.extend_from_slice(&b);
    let q = ScalarQuantizer::fit(&all);
    let qa = q.quantize(&a);
    let qb = q.quantize(&b);
    let approx_dot = q.dot_quantized(&qa, &qb);

    let relative_error = ((exact_dot - approx_dot) / exact_dot).abs();
    assert!(
        relative_error < 0.2,
        "Quantized dot error too high: exact={exact_dot}, approx={approx_dot}, error={relative_error}"
    );
}
