//! Zero-copy IPC payload templates for the touring daemon protocol.
//!
//! # Why
//!
//! The current hook↔daemon protocol uses newline-delimited JSON over a Unix
//! socket. Every request/response pair pays for two `serde_json` passes plus
//! UTF-8 validation. For large payloads (CallGraph, WiringAudit) this dominates
//! the hook budget.
//!
//! `rkyv` offers `Archive` types that are memory-mappable: the receiver casts
//! the raw byte slice to `&ArchivedIpcRequest` without allocating — zero-copy
//! field access at native Rust speeds.
//!
//! # Safety
//!
//! [`IpcRequest`] and [`IpcResponse`] derive `#[archive(check_bytes)]`, which
//! generates a `CheckBytes` impl. Callers MUST use [`rkyv::check_archived_root`]
//! (exposed via [`crate::check_archived_root`]) to validate bytes from untrusted
//! sources — a hostile or corrupted payload would otherwise produce undefined
//! behavior on field access. See the `ipc_roundtrip` tests for the canonical
//! usage pattern.
//!
//! # Migration status (2026-04-14)
//!
//! Migration is **complete and default-on**:
//! 1. ✅ Magic header (`b"RKYV"`) + u32 LE length prefix framing — implemented
//!    here in [`frame_request`] / [`frame_response`] / [`unframe`].
//! 2. ✅ Daemon `handle_connection_async` peek-byte dispatch in
//!    `touring-hooks::daemon` routes `R` → rkyv, `{` → JSON.
//! 3. ✅ CLI `send_daemon_request` emits rkyv frames; `parse_daemon_response`
//!    parses dual-path on response.
//! 4. ✅ Feature `rkyv-ipc` is in DEFAULT features of `touring-hooks` and
//!    `touring-server` since 2026-04-14 — every standard build has it on.
//! 5. ✅ Hot-disable runtime via `TOURING_RKYV_IPC=0` env var (no rebuild).
//! 6. JSON path is preserved for backward compatibility with older clients.

use rkyv::{Archive, Deserialize, Serialize};

/// Wire-level request sent from the CLI to the daemon.
///
/// Mirrors the JSON shape `{"hook": ..., "payload": ..., "project_root": ...}`
/// currently produced by `touring-server::cli::send_daemon_request`.
///
/// # Fields
///
/// * `hook` — the hook name, e.g. `"cli-ast-meta"` or `"cli-index-find"`.
/// * `payload` — caller-supplied bytes (opaque to the envelope). Hooks with
///   typed payloads should nest a typed `Archive` struct and serialize it
///   into these bytes; this keeps the envelope type stable across hook
///   additions.
/// * `project_root` — absolute path of the project the call targets.
#[derive(Archive, Serialize, Deserialize, Debug, Clone, PartialEq)]
#[archive(compare(PartialEq))]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
pub struct IpcRequest {
    /// Hook name routing the request, e.g. `"cli-ast-meta"` or `"cli-index-find"`.
    pub hook: String,
    /// JSON-encoded payload bytes. Kept as raw bytes (not a typed archive)
    /// so the envelope stays stable as hooks evolve their payload schemas.
    /// The daemon decodes these bytes with `serde_json` inside the hook
    /// handler — the win from rkyv is envelope-side, not payload-side.
    pub payload: Vec<u8>,
    /// Absolute path of the project the call targets.
    pub project_root: String,
    /// Session ID for per-session request tracking. Empty string = None,
    /// preserving `#[derive(Archive)]` compatibility (rkyv 0.7 handles
    /// `Option<String>` but the branching adds archive tag bytes — the
    /// sentinel encoding is cheaper for the hot path).
    pub session_id: String,
    /// Request priority (0-255) for weighted scheduling.
    pub priority: u8,
}

/// Wire-level response returned by the daemon.
///
/// Mirrors the JSON shape `{"output": ..., "success": bool}` emitted by
/// `touring-server::cli::DaemonResponse`.
#[derive(Archive, Serialize, Deserialize, Debug, Clone, PartialEq)]
#[archive(compare(PartialEq))]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
pub struct IpcResponse {
    /// Serialized handler output returned to the caller.
    pub output: String,
    /// Whether the hook handler completed successfully.
    pub success: bool,
}

/// Four-byte magic header that distinguishes an rkyv-framed payload from
/// legacy JSON on the Unix socket.
///
/// `{` (the JSON opening brace) = `0x7B` never collides with `RKYV` because
/// neither is a subsequence of the other. A future daemon reader can
/// dispatch on the first byte: `'{'` → JSON path, `'R'` → rkyv framing.
pub const IPC_MAGIC: [u8; 4] = *b"RKYV";

/// Length of the framing prefix: magic (4) + payload length (u32 LE, 4).
pub const IPC_FRAME_HEADER_LEN: usize = 8;

/// Serialize an rkyv-archivable value into an `AlignedVec` with the IPC
/// magic + length framing (`RKYV[4]` + `len[4]` + body).
///
/// Shared by [`frame_request`] and [`frame_response`] to avoid duplicate
/// framing logic — both functions are type-specific entry points that
/// delegate here.
///
/// # Errors
///
/// Returns [`FrameError::Serialize`] if the rkyv serializer fails, or
/// [`FrameError::PayloadTooLarge`] if the body exceeds `u32::MAX`.
fn frame_ipc_bytes<T>(value: &T) -> Result<rkyv::AlignedVec, FrameError>
where
    T: rkyv::Serialize<rkyv::ser::serializers::AllocSerializer<256>>,
{
    let body = rkyv::to_bytes::<_, 256>(value).map_err(|e| FrameError::Serialize(e.to_string()))?;
    let len: u32 = body
        .len()
        .try_into()
        .map_err(|_| FrameError::PayloadTooLarge(body.len()))?;
    let mut out = rkyv::AlignedVec::with_capacity(IPC_FRAME_HEADER_LEN + body.len());
    out.extend_from_slice(&IPC_MAGIC);
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

/// Serialize an [`IpcRequest`] into an `AlignedVec` with magic + length framing.
///
/// Returns the full wire bytes, ready to write to the socket. The payload
/// length prefix is little-endian u32 immediately after the magic, allowing
/// the receiver to bound its read before invoking `check_archived_root`.
///
/// # Errors
///
/// Returns a [`FrameError`] if serialization fails or the payload is too large.
pub fn frame_request(req: &IpcRequest) -> Result<rkyv::AlignedVec, FrameError> {
    frame_ipc_bytes(req)
}

/// Serialize an [`IpcResponse`] into an `AlignedVec` with magic + length framing.
///
/// Symmetric to [`frame_request`].
///
/// # Errors
///
/// Returns a [`FrameError`] if serialization fails or the payload is too large.
pub fn frame_response(resp: &IpcResponse) -> Result<rkyv::AlignedVec, FrameError> {
    frame_ipc_bytes(resp)
}

/// Strip the framing header and return the archive body slice.
///
/// Performs three validations:
/// 1. Total length ≥ [`IPC_FRAME_HEADER_LEN`].
/// 2. First four bytes equal [`IPC_MAGIC`] (distinguishes from legacy JSON).
/// 3. Declared payload length matches remaining bytes.
///
/// # Errors
///
/// Returns [`FrameError::BadMagic`], [`FrameError::Truncated`], or
/// [`FrameError::LengthMismatch`] on failure. Callers SHOULD fall back to
/// JSON parsing on `BadMagic` so legacy hooks keep working.
pub fn unframe(bytes: &[u8]) -> Result<&[u8], FrameError> {
    if bytes.len() < IPC_FRAME_HEADER_LEN {
        return Err(FrameError::Truncated(bytes.len()));
    }
    let magic: &[u8; 4] = bytes[..4]
        .try_into()
        .expect("slice length checked above; infallible");
    if magic != &IPC_MAGIC {
        return Err(FrameError::BadMagic(*magic));
    }
    let len_bytes: [u8; 4] = bytes[4..8]
        .try_into()
        .expect("slice length checked above; infallible");
    let declared = u32::from_le_bytes(len_bytes) as usize;
    let body = &bytes[IPC_FRAME_HEADER_LEN..];
    if body.len() != declared {
        return Err(FrameError::LengthMismatch {
            declared,
            actual: body.len(),
        });
    }
    Ok(body)
}

/// Errors produced while framing/unframing an rkyv IPC payload.
///
/// Opaque string in `Serialize` keeps this enum `Clone + PartialEq` for tests;
/// the inner rkyv error type varies with feature gates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    /// Serializer exhausted scratch space or hit an archive error.
    Serialize(String),
    /// Payload body exceeds `u32::MAX` bytes (4 GiB — in practice a bug).
    PayloadTooLarge(usize),
    /// Fewer than [`IPC_FRAME_HEADER_LEN`] bytes were provided.
    Truncated(usize),
    /// First four bytes did not match [`IPC_MAGIC`]. Caller should fall back
    /// to JSON parsing if interop with legacy hooks is required.
    BadMagic([u8; 4]),
    /// Length prefix disagreed with the actual body size.
    LengthMismatch {
        /// Byte count claimed by the length prefix.
        declared: usize,
        /// Byte count actually present in the body slice.
        actual: usize,
    },
}

impl core::fmt::Display for FrameError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Serialize(msg) => write!(f, "rkyv serialize failed: {msg}"),
            Self::PayloadTooLarge(n) => write!(f, "payload {n} bytes exceeds u32::MAX"),
            Self::Truncated(n) => write!(f, "frame truncated: got {n} bytes, need at least 8"),
            Self::BadMagic(m) => write!(f, "bad magic bytes {m:?}; expected RKYV"),
            Self::LengthMismatch { declared, actual } => write!(
                f,
                "length mismatch: header says {declared} bytes, body is {actual}"
            ),
        }
    }
}

impl std::error::Error for FrameError {}
