//! Command-to-state hashing for Pensieve integration (E15).
//!
//! FNV-1a hashing of bash command tokens into `Vec<u64>` state sequences for
//! [`touring_intelligence::reasoning::Pensieve`]. Structurally similar commands produce
//! overlapping state sequences for cosine-similarity lookup.

/// FNV-1a hash of a byte slice → u64.
///
/// Deterministic, fast, and produces good distribution for short strings
/// (command tokens are typically <30 bytes).
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325; // FNV offset basis
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3); // FNV prime
    }
    h
}

/// Convert a bash command string into a sequence of u64 state hashes.
///
/// Each whitespace-delimited token is hashed independently via FNV-1a.
/// This means `cargo test` and `cargo build` share the first state (`cargo`),
/// allowing the Pensieve to detect partial similarity.
///
/// Empty commands produce an empty vector.
///
/// # Examples
///
/// ```ignore
/// let states = command_to_states("cargo test --lib");
/// assert_eq!(states.len(), 3);
/// // First state is always the hash of "cargo"
/// ```
pub fn command_to_states(command: &str) -> Vec<u64> {
    command
        .split_whitespace()
        .map(|token| fnv1a(token.as_bytes()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_command_produces_empty_states() {
        assert!(command_to_states("").is_empty());
        assert!(command_to_states("   ").is_empty());
    }

    #[test]
    fn single_token_produces_single_state() {
        let states = command_to_states("ls");
        assert_eq!(states.len(), 1);
        assert_ne!(states[0], 0, "hash should be non-zero");
    }

    #[test]
    fn deterministic_hashing() {
        let a = command_to_states("cargo test --lib");
        let b = command_to_states("cargo test --lib");
        assert_eq!(a, b, "same command should produce same states");
    }

    #[test]
    fn different_commands_differ() {
        let a = command_to_states("cargo test");
        let b = command_to_states("cargo build");
        assert_eq!(a.len(), 2);
        assert_eq!(b.len(), 2);
        // First token ("cargo") should be identical
        assert_eq!(a[0], b[0], "shared prefix should produce same first state");
        // Second token differs
        assert_ne!(
            a[1], b[1],
            "different tokens should produce different states"
        );
    }

    #[test]
    fn whitespace_normalization() {
        let a = command_to_states("cargo  test   --lib");
        let b = command_to_states("cargo test --lib");
        assert_eq!(a, b, "split_whitespace normalizes multiple spaces");
    }

    #[test]
    fn long_command_produces_many_states() {
        let cmd = "sudo timeout 30 cargo test --workspace --exclude touring-python -- --nocapture";
        let states = command_to_states(cmd);
        assert_eq!(states.len(), 10);
    }

    #[test]
    fn fnv1a_basic_properties() {
        // Different inputs produce different hashes (probabilistic but reliable for short strings)
        let h1 = fnv1a(b"cargo");
        let h2 = fnv1a(b"test");
        let h3 = fnv1a(b"cargo");
        assert_ne!(h1, h2);
        assert_eq!(h1, h3, "same input = same hash");
    }
}
