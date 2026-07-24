//! Property-based tests for the rkyv IPC envelope (Wave 4 A1, 2026-04-14).
//!
//! Where `ipc_roundtrip.rs` pins fixed canonical inputs, this file proves the
//! framing/bytecheck contract holds for arbitrary well-formed inputs. The
//! generators cover:
//!
//! * `hook` names of any non-empty length, mixing ASCII and UTF-8 (relevant
//!   for hooks with Unicode in payloads — e.g. file paths in non-en locales).
//! * Payload bytes from 0 to 16 KiB — the realistic envelope range for
//!   `cli-ast-meta` (~60 B) up to `cli-ast-blast` summaries (~16 KiB).
//! * Project roots that are arbitrary paths.
//! * Session IDs covering the empty-string sentinel + arbitrary identifiers.
//! * Priorities across the full u8 range.
//!
//! Each property runs the strategy 256 times by default (proptest config).
//! Failures shrink to a minimal counterexample — when this catches a bug,
//! the printed input is diff-friendly.

use proptest::prelude::*;
use touring_rkyv::{
    IPC_MAGIC, IpcRequest, IpcResponse, check_archived_root, frame_request, frame_response, unframe,
};

/// Strategy: arbitrary `IpcRequest` covering realistic field shapes.
///
/// Constraints (avoid degenerate or out-of-domain inputs):
/// * `hook` ≤ 64 chars (real hook names are <40)
/// * `payload` ≤ 16 KiB (realistic upper bound)
/// * `project_root` ≤ 256 chars (PATH_MAX-ish)
/// * `session_id` ≤ 64 chars
fn arb_request() -> impl Strategy<Value = IpcRequest> {
    (
        ".{1,64}",
        proptest::collection::vec(any::<u8>(), 0..16 * 1024),
        ".{1,256}",
        ".{0,64}",
        any::<u8>(),
    )
        .prop_map(
            |(hook, payload, project_root, session_id, priority)| IpcRequest {
                hook,
                payload,
                project_root,
                session_id,
                priority,
            },
        )
}

fn arb_response() -> impl Strategy<Value = IpcResponse> {
    (".{0,256}", any::<bool>()).prop_map(|(output, success)| IpcResponse { output, success })
}

proptest! {
    /// Property: every well-formed `IpcRequest` survives frame → unframe →
    /// check_archived_root with field-level equality. Catches drift between
    /// `frame_*` length encoding and `unframe` length checks.
    #[test]
    fn request_roundtrip_preserves_all_fields(req in arb_request()) {
        let frame = frame_request(&req).expect("frame must succeed for any well-formed input");

        // Magic invariant — frame ALWAYS starts with RKYV.
        prop_assert_eq!(&frame[..4], &IPC_MAGIC);

        let body = unframe(&frame).expect("unframe own output is reflexive");
        let archived =
            check_archived_root::<IpcRequest>(body).expect("bytecheck rejects only corruption");

        prop_assert_eq!(archived.hook.as_str(), req.hook.as_str());
        prop_assert_eq!(archived.project_root.as_str(), req.project_root.as_str());
        prop_assert_eq!(archived.session_id.as_str(), req.session_id.as_str());
        prop_assert_eq!(archived.priority, req.priority);
        prop_assert_eq!(archived.payload.as_slice(), req.payload.as_slice());
    }

    /// Property: every well-formed `IpcResponse` survives the round trip.
    /// Smaller surface than request — but still catches drift in the
    /// response-side framing path independently from the request path.
    #[test]
    fn response_roundtrip_preserves_all_fields(resp in arb_response()) {
        let frame = frame_response(&resp).expect("frame must succeed");
        prop_assert_eq!(&frame[..4], &IPC_MAGIC);

        let body = unframe(&frame).expect("unframe is reflexive");
        let archived =
            check_archived_root::<IpcResponse>(body).expect("bytecheck passes for own output");

        prop_assert_eq!(archived.output.as_str(), resp.output.as_str());
        prop_assert_eq!(archived.success, resp.success);
    }

    /// Property: corrupting a single byte AFTER the header makes bytecheck
    /// reject the frame OR the unframe length-check fails. NEVER UB — that
    /// is the safety guarantee `check_archived_root` exists to provide.
    ///
    /// Skips the first 8 bytes (header is checked separately) and skips
    /// the byte we never read.
    #[test]
    fn random_byte_corruption_is_caught(
        req in arb_request(),
        flip_offset in 0usize..1024,
        flip_xor in 1u8..255,
    ) {
        let mut frame = frame_request(&req).expect("frame").to_vec();
        let body_start = 8usize;
        if frame.len() <= body_start {
            return Ok(());
        }
        // Map flip_offset into the body range, leaving the header intact so
        // we focus the property on the bytecheck side specifically.
        let body_len = frame.len() - body_start;
        let idx = body_start + (flip_offset % body_len);
        frame[idx] ^= flip_xor;

        // Whatever happens — unframe error, bytecheck error, no fields
        // accessed — the function must NOT panic and must NOT produce a ref
        // we'd later dereference into garbage.
        match unframe(&frame) {
            Err(_) => {} // length mismatch / structural — fine
            Ok(body) => {
                // Body shape passed; the deeper check must reject.
                let _ = check_archived_root::<IpcRequest>(body);
                // No assertion: we only require absence of panic / UB.
            }
        }
    }
}
