//! MinHash signatures + LSH banding for near-duplicate set detection.
//!
//! Companion to [`super::jaccard`]: MinHash *finds* the candidate pairs that are
//! plausibly similar in near-linear time, and `JaccardComputer` decides. Neither
//! replaces the other — MinHash alone gives an estimate with real variance, so
//! every candidate this module proposes must be confirmed exactly before being
//! reported as a clone.
//!
//! # Why not compare every pair
//!
//! N sets of shingles need N²/2 Jaccard evaluations. For the ~200k windows of a
//! large crate corpus that is 2×10¹⁰ comparisons. LSH banding buckets sets whose
//! signatures agree on any whole band, so only plausible pairs are ever scored.
//!
//! # Determinism (REGRA #17)
//!
//! The permutations come from a fixed splitmix64 sequence over a constant seed —
//! never `rand`, never address- or time-derived. The same input yields the same
//! signature on every machine and every run, which is what makes a clone report
//! diffable across sessions.
//!
//! # Parameters
//!
//! [`SIGNATURE_LEN`] = 32 hashes as [`BANDS`] = 8 bands × [`ROWS`] = 4 rows. The
//! detection probability of a pair at true Jaccard `t` is `1 - (1 - t^4)^8`:
//! ≈ 0.997 at t = 0.85, ≈ 0.40 at t = 0.5. Recall is what matters here because
//! precision is restored exactly by the verification step; a missed pair, by
//! contrast, is invisible.

/// Number of MinHash permutations per signature.
pub const SIGNATURE_LEN: usize = 32;
/// LSH bands. `BANDS * ROWS == SIGNATURE_LEN`.
pub const BANDS: usize = 8;
/// Rows per LSH band.
pub const ROWS: usize = SIGNATURE_LEN / BANDS;

/// Mersenne prime 2³¹−1 — the modulus for the universal hash family.
const MERSENNE: u64 = (1 << 31) - 1;

/// Fixed seed for the permutation coefficients. Changing it changes every
/// signature, so it is part of the module's observable contract.
const SEED: u64 = 0x5EED_C10E_2026_0808;

/// splitmix64 — a fast, well-distributed deterministic sequence.
#[inline]
const fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The `(a, b)` coefficients of `h_i(x) = (a_i·x + b_i) mod (2³¹−1)`.
///
/// `a` is forced non-zero: a zero multiplier collapses the permutation to the
/// constant `b`, silently costing a whole hash's worth of discrimination.
fn coefficients() -> [(u64, u64); SIGNATURE_LEN] {
    let mut state = SEED;
    let mut out = [(0u64, 0u64); SIGNATURE_LEN];
    for slot in out.iter_mut() {
        let a = splitmix64(&mut state) % MERSENNE;
        let b = splitmix64(&mut state) % MERSENNE;
        *slot = (if a == 0 { 1 } else { a }, b);
    }
    out
}

/// A MinHash signature: one minimum per permutation.
pub type Signature = [u32; SIGNATURE_LEN];

/// Computes MinHash signatures with a fixed permutation family.
#[derive(Debug, Clone)]
pub struct MinHasher {
    coeffs: [(u64, u64); SIGNATURE_LEN],
}

impl Default for MinHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl MinHasher {
    /// Builds the hasher. Cheap (32 splitmix64 pairs) but worth hoisting out of
    /// a loop over windows.
    #[must_use]
    pub fn new() -> Self {
        Self {
            coeffs: coefficients(),
        }
    }

    /// Signature of a shingle set.
    ///
    /// An EMPTY set yields all-`u32::MAX`, which is a real signature and will
    /// bucket every empty set together — callers must drop empty sets before
    /// banding rather than rely on this being "no signature".
    #[must_use]
    pub fn signature(&self, shingles: &[u32]) -> Signature {
        let mut sig = [u32::MAX; SIGNATURE_LEN];
        for &s in shingles {
            let x = u64::from(s);
            for (i, &(a, b)) in self.coeffs.iter().enumerate() {
                // (a*x + b) mod (2^31 - 1); a,b,x all < 2^31 so no overflow.
                let h = ((a.wrapping_mul(x).wrapping_add(b)) % MERSENNE) as u32;
                if let Some(slot) = sig.get_mut(i)
                    && h < *slot
                {
                    *slot = h;
                }
            }
        }
        sig
    }

    /// Estimated Jaccard similarity: the fraction of agreeing positions.
    ///
    /// This is an ESTIMATE with standard error ≈ `sqrt(t(1-t)/32)` — about 0.06
    /// at t = 0.85. Use it to rank or filter, never to decide; decide with
    /// [`super::JaccardComputer`] on the underlying sets.
    #[must_use]
    pub fn estimate(a: &Signature, b: &Signature) -> f64 {
        let agree = a.iter().zip(b.iter()).filter(|(x, y)| x == y).count();
        agree as f64 / SIGNATURE_LEN as f64
    }
}

/// The `BANDS` band keys of a signature.
///
/// Two sets share a key iff their signatures agree on that whole band, which is
/// the LSH candidate condition.
#[must_use]
pub fn band_keys(sig: &Signature) -> [u64; BANDS] {
    let mut keys = [0u64; BANDS];
    for (band, key) in keys.iter_mut().enumerate() {
        // FNV-1a over the band's rows — order-sensitive, so band 0's rows can
        // never collide with band 1's identical values.
        let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ (band as u64);
        for row in 0..ROWS {
            let v = sig.get(band * ROWS + row).copied().unwrap_or(u32::MAX);
            h ^= u64::from(v);
            h = h.wrapping_mul(0x0000_0100_0000_01B3);
        }
        *key = h;
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::similarity::{JaccardComputer, traits::JaccardSimilarity};

    fn set(range: std::ops::Range<u32>) -> Vec<u32> {
        range.collect()
    }

    #[test]
    fn identical_sets_have_identical_signatures() {
        let h = MinHasher::new();
        let a = set(0..200);
        assert_eq!(h.signature(&a), h.signature(&a));
        assert!((MinHasher::estimate(&h.signature(&a), &h.signature(&a)) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn signatures_are_stable_across_hasher_instances() {
        // Determinism is the contract: two processes must agree, so two
        // independently constructed hashers must too.
        let a = set(7..97);
        assert_eq!(MinHasher::new().signature(&a), MinHasher::new().signature(&a));
    }

    #[test]
    fn the_estimate_tracks_the_true_jaccard() {
        let h = MinHasher::new();
        let jac = JaccardComputer::new();
        // 0..1000 vs 150..1000 → |∩| = 850, |∪| = 1000 → J = 0.85
        let (a, b) = (set(0..1000), set(150..1000));
        let truth = jac.jaccard(&a, &b);
        let est = MinHasher::estimate(&h.signature(&a), &h.signature(&b));
        assert!((truth - 0.85).abs() < 1e-9, "fixture must be J=0.85, got {truth}");
        // 3 standard errors at 32 permutations ≈ 0.19.
        assert!(
            (est - truth).abs() < 0.19,
            "estimate {est} too far from truth {truth}"
        );
    }

    #[test]
    fn disjoint_sets_rarely_share_a_band() {
        let h = MinHasher::new();
        let (a, b) = (set(0..500), set(10_000..10_500));
        let (ka, kb) = (band_keys(&h.signature(&a)), band_keys(&h.signature(&b)));
        let shared = ka.iter().zip(kb.iter()).filter(|(x, y)| x == y).count();
        assert_eq!(shared, 0, "disjoint sets must not be LSH candidates");
    }

    #[test]
    fn highly_similar_sets_share_at_least_one_band() {
        // The recall property the whole design rests on: at J≈0.97 a pair must
        // survive banding, or the exact verifier never gets to see it.
        let h = MinHasher::new();
        let (a, b) = (set(0..1000), set(30..1000));
        let (ka, kb) = (band_keys(&h.signature(&a)), band_keys(&h.signature(&b)));
        assert!(
            ka.iter().zip(kb.iter()).any(|(x, y)| x == y),
            "a near-identical pair was not bucketed together"
        );
    }

    #[test]
    fn bands_partition_the_signature_exactly() {
        assert_eq!(BANDS * ROWS, SIGNATURE_LEN);
    }

    #[test]
    fn no_permutation_is_degenerate() {
        // A zero multiplier would silently waste a hash slot.
        assert!(coefficients().iter().all(|&(a, _)| a != 0));
    }

    #[test]
    fn an_empty_set_is_the_sentinel_signature() {
        assert_eq!(MinHasher::new().signature(&[]), [u32::MAX; SIGNATURE_LEN]);
    }
}
