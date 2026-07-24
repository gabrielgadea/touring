#![no_main]
//! Fuzz harness: rkyv zero-copy deserialization validation.
//!
//! Feeds arbitrary bytes into `check_archived_root::<IpcRequest>`, the
//! `CheckBytes`-backed validator for the wire IPC envelope. This is the
//! security-critical path — untrusted socket bytes are validated here before
//! any field access. The target proves the validator rejects malformed buffers
//! with `Err` and never panics, never reads out of bounds.

use libfuzzer_sys::fuzz_target;
use touring_rkyv::{check_archived_root, IpcRequest};

fuzz_target!(|data: &[u8]| {
    // `check_archived_root` performs full bounds + layout validation. A
    // malformed buffer must yield `Err` — a panic or UB is a real finding.
    let _ = check_archived_root::<IpcRequest>(data);
});
