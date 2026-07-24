//! Run-Length Encoding (RLE) for CRDT delta field compression.
//!
//! Compresses sequences into `(count: u32, value)` pairs.
//! Targets: `Vec<CrdtNodeId>`, `Vec<(CrdtNodeId, CrdtNodeId)>`, and similar.
//!
//! RLE saves space only when `run_length >= 2`. For isolated elements,
//! the 4-byte count overhead exceeds the savings.

use std::io::Write;

/// Encodes a slice of `u64` values into RLE bytes.
pub fn encode_u64(input: &[u64]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len() * 12);
    let mut run_start = 0;

    while run_start < input.len() {
        let mut run_end = run_start + 1;
        while run_end < input.len() && input.get(run_end) == input.get(run_start) {
            run_end += 1;
        }

        let run_len: u32 = (run_end - run_start) as u32;
        let _ = output.write_all(&run_len.to_le_bytes());
        if let Some(val) = input.get(run_start) {
            let _ = output.write_all(&val.to_le_bytes());
        }
        run_start = run_end;
    }

    output
}

/// Decodes RLE bytes back into a `Vec<u64>`.
pub fn decode_u64(input: &[u8]) -> Option<Vec<u64>> {
    if input.len() % 12 != 0 {
        return None;
    }

    let pairs = input.len() / 12;
    let mut output = Vec::with_capacity(pairs * 2);
    let mut offset = 0;

    for _ in 0..pairs {
        let count_bytes: [u8; 4] = input.get(offset..offset + 4)?.try_into().ok()?;
        let value_bytes: [u8; 8] = input.get(offset + 4..offset + 12)?.try_into().ok()?;
        let count = u32::from_le_bytes(count_bytes);
        let value = u64::from_le_bytes(value_bytes);
        offset += 12;

        let count_usize: usize = count.try_into().ok()?;
        output.resize(output.len() + count_usize, value);
    }

    Some(output)
}

/// Encodes a slice of `(u64, u64)` pairs (e.g., edge endpoint pairs).
pub fn encode_u64_pair(input: &[(u64, u64)]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len() * 20);
    let mut run_start = 0;

    while run_start < input.len() {
        let mut run_end = run_start + 1;
        while run_end < input.len() && input.get(run_end) == input.get(run_start) {
            run_end += 1;
        }

        let run_len: u32 = (run_end - run_start) as u32;
        let _ = output.write_all(&run_len.to_le_bytes());
        if let Some(val) = input.get(run_start) {
            let _ = output.write_all(&val.0.to_le_bytes());
            let _ = output.write_all(&val.1.to_le_bytes());
        }
        run_start = run_end;
    }

    output
}

/// Decodes RLE bytes back into `Vec<(u64, u64)>`.
pub fn decode_u64_pair(input: &[u8]) -> Option<Vec<(u64, u64)>> {
    if input.len() % 20 != 0 {
        return None;
    }

    let pairs = input.len() / 20;
    let mut output = Vec::with_capacity(pairs * 2);
    let mut offset = 0;

    for _ in 0..pairs {
        let count_bytes: [u8; 4] = input.get(offset..offset + 4)?.try_into().ok()?;
        let a_bytes: [u8; 8] = input.get(offset + 4..offset + 12)?.try_into().ok()?;
        let b_bytes: [u8; 8] = input.get(offset + 12..offset + 20)?.try_into().ok()?;
        let count = u32::from_le_bytes(count_bytes);
        let a = u64::from_le_bytes(a_bytes);
        let b = u64::from_le_bytes(b_bytes);
        offset += 20;

        let count_usize: usize = count.try_into().ok()?;
        output.resize(output.len() + count_usize, (a, b));
    }

    Some(output)
}

/// Encodes a slice of strings (e.g., source_ids).
/// Strings are length-prefixed UTF-8 bytes.
pub fn encode_str(input: &[String]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut run_start = 0;

    while run_start < input.len() {
        let mut run_end = run_start + 1;
        while run_end < input.len() && input.get(run_end) == input.get(run_start) {
            run_end += 1;
        }

        let run_len: u32 = (run_end - run_start) as u32;
        let _ = output.write_all(&run_len.to_le_bytes());

        // Length-prefix the string bytes
        if let Some(s) = input.get(run_start) {
            let bytes = s.as_bytes();
            let len_bytes: u32 = bytes.len() as u32;
            let _ = output.write_all(&len_bytes.to_le_bytes());
            let _ = output.write_all(bytes);
        }

        run_start = run_end;
    }

    output
}

/// Decodes RLE bytes back into `Vec<String>`.
pub fn decode_str(input: &[u8]) -> Option<Vec<String>> {
    let mut output = Vec::new();
    let mut offset = 0;

    while offset < input.len() {
        // Read count
        let count_bytes: [u8; 4] = input.get(offset..offset + 4)?.try_into().ok()?;
        let count = u32::from_le_bytes(count_bytes);
        offset += 4;

        // Read string length
        let len_bytes: [u8; 4] = input.get(offset..offset + 4)?.try_into().ok()?;
        let str_len = u32::from_le_bytes(len_bytes) as usize;
        offset += 4;

        // Read string bytes
        let str_bytes = input.get(offset..offset + str_len)?;
        let s = String::from_utf8(str_bytes.to_vec()).ok()?;
        offset += str_len;

        let count_usize: usize = count.try_into().ok()?;
        output.resize(output.len() + count_usize, s);
    }

    Some(output)
}

/// Returns the RLE compression ratio for u64 slices.
pub fn compression_ratio_u64(input: &[u64]) -> f64 {
    if input.is_empty() {
        return 1.0;
    }
    let encoded_len = encode_u64(input).len();
    if encoded_len == 0 {
        return 1.0;
    }
    (input.len() as f64 * 8.0) / (encoded_len as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── u64 tests ─────────────────────────────────────────────

    #[test]
    fn test_rle_u64_empty() {
        let input: Vec<u64> = vec![];
        let encoded = encode_u64(&input);
        assert!(encoded.is_empty());
        assert_eq!(decode_u64(&encoded), Some(vec![]));
    }

    #[test]
    fn test_rle_u64_single() {
        let input = vec![42u64];
        let encoded = encode_u64(&input);
        assert_eq!(encoded.len(), 12); // 4 + 8
        assert_eq!(decode_u64(&encoded), Some(vec![42]));
    }

    #[test]
    fn test_rle_u64_run_of_3() {
        let input = vec![10u64, 10, 10];
        let encoded = encode_u64(&input);
        assert_eq!(encoded.len(), 12); // 1 run: 4 + 8
        assert_eq!(decode_u64(&encoded), Some(vec![10, 10, 10]));
    }

    #[test]
    fn test_rle_u64_mixed() {
        let input = vec![1u64, 1, 2, 2, 2, 3];
        let encoded = encode_u64(&input);
        // 3 runs × 12 bytes = 36
        assert_eq!(encoded.len(), 36);
        assert_eq!(decode_u64(&encoded), Some(vec![1, 1, 2, 2, 2, 3]));
    }

    #[test]
    fn test_rle_u64_large_run() {
        let input: Vec<u64> = vec![99; 1000];
        let encoded = encode_u64(&input);
        assert_eq!(encoded.len(), 12); // 4 + 8
        assert_eq!(decode_u64(&encoded), Some(vec![99; 1000]));
    }

    #[test]
    fn test_rle_u64_invalid_len() {
        let garbage: Vec<u8> = vec![1, 2, 3, 4, 5];
        assert!(decode_u64(&garbage).is_none());
    }

    #[test]
    fn test_rle_u64_compression_ratio() {
        let long_run: Vec<u64> = vec![7; 100];
        let ratio = compression_ratio_u64(&long_run);
        // 800 bytes raw (100*8) → 12 bytes encoded (1 run) = ratio ~66.67
        assert!((ratio - (800.0 / 12.0)).abs() < 0.1);

        let isolated: Vec<u64> = (0..5).collect();
        let ratio2 = compression_ratio_u64(&isolated);
        // 40 bytes raw (5*8) → 60 bytes encoded (5 runs*12) = ratio ~0.67
        assert!(ratio2 < 1.0);
    }

    #[test]
    fn test_rle_u64_roundtrip_stress() {
        let mut rng = std::cell::Cell::new(0xdeadbeefu64);
        let next_rand = |rng: &std::cell::Cell<u64>| -> u64 {
            let val = rng.get().wrapping_mul(1103515245).wrapping_add(12345);
            rng.set(val);
            val >> 16
        };

        for _ in 0..50 {
            let len = (next_rand(&mut rng) % 300) as usize;
            let mut input: Vec<u64> = Vec::with_capacity(len);
            let mut prev = next_rand(&mut rng);
            input.push(prev);

            for _ in 1..len {
                // 35% chance of same (creates runs)
                let curr = if next_rand(&mut rng) % 100 < 35 {
                    prev
                } else {
                    next_rand(&mut rng)
                };
                input.push(curr);
                prev = curr;
            }

            let encoded = encode_u64(&input);
            let decoded = decode_u64(&encoded).expect("decode must succeed");
            assert_eq!(decoded, input);
        }
    }

    // ── u64 pair tests ────────────────────────────────────────

    #[test]
    fn test_rle_u64_pair_empty() {
        let input: Vec<(u64, u64)> = vec![];
        let encoded = encode_u64_pair(&input);
        assert!(encoded.is_empty());
        assert_eq!(decode_u64_pair(&encoded), Some(vec![]));
    }

    #[test]
    fn test_rle_u64_pair_single() {
        let input = vec![(1u64, 2u64)];
        let encoded = encode_u64_pair(&input);
        assert_eq!(encoded.len(), 20); // 4 + 8 + 8
        assert_eq!(decode_u64_pair(&encoded), Some(vec![(1, 2)]));
    }

    #[test]
    fn test_rle_u64_pair_run() {
        let input = vec![(5u64, 10u64), (5, 10), (5, 10), (5, 10), (99, 1)];
        let encoded = encode_u64_pair(&input);
        let decoded = decode_u64_pair(&encoded).expect("decode must succeed");
        assert_eq!(decoded, input);
    }

    #[test]
    fn test_rle_u64_pair_invalid_len() {
        let garbage: Vec<u8> = vec![0, 0, 0, 0, 1, 2, 3];
        assert!(decode_u64_pair(&garbage).is_none());
    }

    // ── String tests ───────────────────────────────────────────

    #[test]
    fn test_rle_str_empty() {
        let input: Vec<String> = vec![];
        let encoded = encode_str(&input);
        assert!(encoded.is_empty());
        assert_eq!(decode_str(&encoded), Some(vec![]));
    }

    #[test]
    fn test_rle_str_single() {
        let input = vec![String::from("agent-1")];
        let encoded = encode_str(&input);
        let decoded = decode_str(&encoded).expect("decode must succeed");
        assert_eq!(decoded, input);
    }

    #[test]
    fn test_rle_str_run() {
        let input = vec![
            String::from("same"),
            String::from("same"),
            String::from("same"),
            String::from("diff"),
        ];
        let encoded = encode_str(&input);
        let decoded = decode_str(&encoded).expect("decode must succeed");
        assert_eq!(decoded, input);
    }

    #[test]
    fn test_rle_str_empty_string() {
        let input = vec![String::from(""), String::from(""), String::from("a")];
        let encoded = encode_str(&input);
        let decoded = decode_str(&encoded).expect("decode must succeed");
        assert_eq!(decoded, input);
    }
}
