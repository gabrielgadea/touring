//! Zero-copy roundtrip + bytecheck safety tests for the IPC envelope.
//!
//! These tests lock the wire-level contract of [`touring_rkyv::ipc`]:
//!
//! 1. **Roundtrip identity**: frame → unframe → archived_root field access
//!    yields the original values without allocation.
//! 2. **Bytecheck safety**: `check_archived_root` rejects corrupted bytes
//!    (flipped tag, truncated body, bad UTF-8) before field access — the
//!    safety guarantee required for untrusted Unix socket input.
//! 3. **Framing invariants**: magic/length prefix correctly distinguishes
//!    rkyv payloads from legacy JSON.

use touring_rkyv::{
    FrameError, IPC_FRAME_HEADER_LEN, IPC_MAGIC, IpcRequest, IpcResponse, check_archived_root,
    frame_request, frame_response, unframe,
};

/// Canonical small request used across tests — matches the shape of a
/// `cli-ast-meta` call for one file.
fn sample_request() -> IpcRequest {
    IpcRequest {
        hook: "cli-ast-meta".to_string(),
        payload: b"{\"file\":\"src/lib.rs\"}".to_vec(),
        project_root: "/home/gabriel/project".to_string(),
        session_id: "sess-abc".to_string(),
        priority: 100,
    }
}

fn sample_response() -> IpcResponse {
    IpcResponse {
        output: "{\"symbols\":42,\"loc\":317}".to_string(),
        success: true,
    }
}

#[test]
fn frame_request_starts_with_magic() {
    let bytes = frame_request(&sample_request()).expect("serialize");
    assert!(bytes.len() >= IPC_FRAME_HEADER_LEN);
    assert_eq!(&bytes[..4], &IPC_MAGIC);
}

#[test]
fn frame_length_prefix_matches_body() {
    let bytes = frame_request(&sample_request()).expect("serialize");
    let declared = u32::from_le_bytes(bytes[4..8].try_into().expect("header slice")) as usize;
    assert_eq!(declared, bytes.len() - IPC_FRAME_HEADER_LEN);
}

#[test]
fn roundtrip_request_zero_copy_field_access() {
    let original = sample_request();
    let bytes = frame_request(&original).expect("serialize");
    let body = unframe(&bytes).expect("unframe");

    // check_archived_root validates every field's bytes before returning the
    // typed reference — this is the line that prevents UB on hostile input.
    let archived = check_archived_root::<IpcRequest>(body).expect("bytecheck passes");

    // Zero-copy access — no Deserialize call, fields are accessed directly
    // from the original byte buffer.
    assert_eq!(archived.hook.as_str(), original.hook.as_str());
    assert_eq!(
        archived.project_root.as_str(),
        original.project_root.as_str()
    );
    assert_eq!(archived.payload.as_slice(), original.payload.as_slice());
}

#[test]
fn roundtrip_response_zero_copy_field_access() {
    let original = sample_response();
    let bytes = frame_response(&original).expect("serialize");
    let body = unframe(&bytes).expect("unframe");

    let archived = check_archived_root::<IpcResponse>(body).expect("bytecheck passes");
    assert_eq!(archived.output.as_str(), original.output.as_str());
    assert_eq!(archived.success, original.success);
}

#[test]
fn unframe_rejects_legacy_json() {
    // JSON payloads start with `{` — BadMagic lets the caller dispatch
    // to the legacy JSON path.
    let json = b"{\"hook\":\"cli-ast-meta\"}\n__________________________";
    match unframe(json) {
        Err(FrameError::BadMagic(m)) => {
            assert_eq!(m, *b"{\"ho");
        }
        other => panic!("expected BadMagic, got {other:?}"),
    }
}

#[test]
fn unframe_rejects_truncated_frame() {
    let bytes = frame_request(&sample_request()).expect("serialize");
    // Lose everything past the magic — length prefix unreadable.
    let truncated = &bytes[..4];
    match unframe(truncated) {
        Err(FrameError::Truncated(n)) => assert_eq!(n, 4),
        other => panic!("expected Truncated, got {other:?}"),
    }
}

#[test]
fn unframe_rejects_length_mismatch() {
    let mut bytes = frame_request(&sample_request())
        .expect("serialize")
        .to_vec();
    // Inflate declared length by 1 — body won't match.
    let declared = u32::from_le_bytes(bytes[4..8].try_into().expect("header slice"));
    let inflated = (declared + 1).to_le_bytes();
    bytes[4..8].copy_from_slice(&inflated);

    match unframe(&bytes) {
        Err(FrameError::LengthMismatch { declared, actual }) => {
            assert_eq!(declared, actual + 1);
        }
        other => panic!("expected LengthMismatch, got {other:?}"),
    }
}

#[test]
fn bytecheck_rejects_corrupted_body() {
    let bytes = frame_request(&sample_request()).expect("serialize");
    let body = unframe(&bytes).expect("unframe");

    // Flip a byte deep in the archived struct — somewhere past the first
    // 4 bytes so we corrupt the hook string's payload or length.
    let flip_idx = body.len() / 2;
    let mut corrupted = body.to_vec();
    corrupted[flip_idx] = corrupted[flip_idx].wrapping_add(0x55);

    // check_archived_root MUST return Err, not panic and not produce a ref.
    // Exact error type varies with what byte was flipped; we only assert
    // the validation refused to return a reference.
    assert!(
        check_archived_root::<IpcRequest>(&corrupted).is_err(),
        "bytecheck must reject corrupted body — UB risk otherwise"
    );
}

#[test]
fn empty_payload_roundtrips() {
    // Hooks that take no args still need a valid envelope.
    let req = IpcRequest {
        hook: "cli-doctor".to_string(),
        payload: Vec::new(),
        project_root: "/".to_string(),
        session_id: String::new(),
        priority: 0,
    };
    let bytes = frame_request(&req).expect("serialize");
    let body = unframe(&bytes).expect("unframe");
    let archived = check_archived_root::<IpcRequest>(body).expect("bytecheck passes");
    assert_eq!(archived.hook.as_str(), "cli-doctor");
    assert!(archived.payload.is_empty());
}

#[test]
fn large_payload_roundtrips() {
    // Simulate a CallGraph-sized payload (~64 KiB) to confirm the auto-grow
    // serializer path works end-to-end.
    let big = vec![0xABu8; 64 * 1024];
    let req = IpcRequest {
        hook: "cli-ast-blast".to_string(),
        payload: big.clone(),
        project_root: "/workspace".to_string(),
        session_id: String::new(),
        priority: 50,
    };
    let bytes = frame_request(&req).expect("serialize large payload");
    let body = unframe(&bytes).expect("unframe");
    let archived = check_archived_root::<IpcRequest>(body).expect("bytecheck passes");
    assert_eq!(archived.payload.len(), big.len());
    assert_eq!(archived.payload.as_slice(), big.as_slice());
}
