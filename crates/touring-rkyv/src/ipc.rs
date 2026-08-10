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
//! [`IpcRequest`] and [`IpcResponse`] get a `CheckBytes` impl from the derive.
//! Since rkyv 0.8 that is automatic whenever the `bytecheck` feature is on — the
//! 0.7 opt-in `#[archive(check_bytes)]` no longer exists. Callers MUST use
//! [`crate::check_archived_root`] to validate bytes from untrusted sources — a
//! hostile or corrupted payload would otherwise produce undefined behavior on
//! field access. See the `ipc_roundtrip` tests for the canonical usage pattern.
//!
//! # Migration status (2026-04-14)
//!
//! Migration is **complete and default-on**:
//! 1. ✅ Magic header + u32 LE length prefix framing — implemented here in
//!    [`frame_request`] / [`frame_response`] / [`unframe`]. O magic era `b"RKYV"`
//!    até a migração 0.8; hoje é [`IPC_MAGIC`] (`RKY2`), com [`IPC_MAGIC_V1`]
//!    guardado para diagnóstico.
//! 2. ✅ Daemon `handle_connection_async` peek-byte dispatch in
//!    `touring-hooks::daemon` routes `R` → rkyv, `{` → JSON.
//! 3. ✅ CLI `send_daemon_request` emits rkyv frames; `parse_daemon_response`
//!    parses dual-path on response.
//! 4. ✅ Feature `rkyv-ipc` is in DEFAULT features of `touring-hooks` and
//!    `touring-server` since 2026-04-14 — every standard build has it on.
//! 5. ✅ Hot-disable runtime via `TOURING_RKYV_IPC=0` env var (no rebuild).
//! 6. JSON path is preserved for backward compatibility with older clients.
//!
//! # rkyv 0.8 (2026-08-07, RUSTSEC-2026-0235)
//!
//! O corpo do frame passou a ser produzido pelo rkyv 0.8. A **forma** do envelope
//! é a mesma — `magic[4]` + `len[4]` LE + corpo —, mas o layout INTERNO do corpo
//! mudou e o 0.8 não lê arquivos 0.7.
//!
//! Um peer defasado nunca lê lixo em silêncio: o SPIKE do P0 mediu o 0.8
//! RECUSANDO bytes 0.7 (`invalid UTF-8`), e este módulo só expõe caminhos
//! validados. O que a migração acrescentou foi **precisão**: o magic passou de
//! `RKYV` a [`IPC_MAGIC`] (`RKY2`), então a incompatibilidade é detectada já no
//! frame e [`FrameError::BadMagic`] a NOMEIA — em vez de reaparecer adiante como
//! um erro de validação que parece corrupção.

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
#[rkyv(compare(PartialEq))]
#[rkyv(derive(Debug))]
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
#[rkyv(compare(PartialEq))]
#[rkyv(derive(Debug))]
pub struct IpcResponse {
    /// Serialized handler output returned to the caller.
    pub output: String,
    /// Whether the hook handler completed successfully.
    pub success: bool,
}

/// Four-byte magic header that distinguishes an rkyv-framed payload from
/// legacy JSON on the socket — e, desde a migração 0.8, também carrega a
/// **versão do fio**.
///
/// O daemon despacha pelo PRIMEIRO byte (`'{'` → JSON, `'R'` → rkyv), por isso
/// o dígito de versão fica no ÚLTIMO byte: versionar não pode quebrar o
/// despacho.
///
/// **v2** = corpo produzido pelo rkyv 0.8. O layout interno mudou na migração
/// (RUSTSEC-2026-0235) e não é compatível com o v1. Sem discriminar a versão a
/// falha ainda seria ruidosa — o bytecheck recusa, nunca lê lixo —, mas o erro
/// (`invalid UTF-8`) apontaria para corrupção em vez da causa real. Com o
/// discriminador a falha acontece no frame e **nomeia** o problema.
pub const IPC_MAGIC: [u8; 4] = *b"RKY2";

/// Magic da versão 1 do fio — corpo produzido pelo rkyv 0.7.
///
/// Mantido exclusivamente para **diagnóstico**: recebê-lo prova que o peer roda
/// um binário anterior à migração 0.8, e permite dizer isso em vez de emitir um
/// erro genérico. É o que torna a janela de rollout legível.
pub const IPC_MAGIC_V1: [u8; 4] = *b"RKYV";

/// Length of the framing prefix: magic (4) + payload length (u32 LE, 4).
pub const IPC_FRAME_HEADER_LEN: usize = 8;

/// Soma dos bytes crus que um envelope carrega.
///
/// Base do teto anti-panic ([`crate::MAX_ARCHIVE_BYTES`]): o arquivo rkyv é
/// sempre ≥ os dados que carrega, então esta soma é um piso do tamanho
/// arquivado e permite recusar cedo o que faria o rkyv entrar em pânico.
/// Privado de propósito — é detalhe do framing, não da superfície pública.
trait RawBytes {
    fn raw_bytes(&self) -> usize;
}

impl RawBytes for IpcRequest {
    fn raw_bytes(&self) -> usize {
        self.hook.len() + self.payload.len() + self.project_root.len() + self.session_id.len()
    }
}

impl RawBytes for IpcResponse {
    fn raw_bytes(&self) -> usize {
        self.output.len()
    }
}

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
fn frame_ipc_bytes<T: crate::Serializable + RawBytes>(
    value: &T,
) -> Result<crate::AlignedVec, FrameError> {
    // Recusa ANTES de serializar: acima deste teto o rkyv 0.8 entra em pânico em
    // vez de devolver erro (ver `crate::MAX_ARCHIVE_BYTES`), e o daemon não pode
    // cair por causa do tamanho de um payload que chegou pelo socket.
    let raw = value.raw_bytes();
    if raw >= crate::MAX_ARCHIVE_BYTES {
        return Err(FrameError::PayloadTooLarge(raw));
    }
    let body =
        crate::to_bytes::<T, 256>(value).map_err(|e| FrameError::Serialize(e.to_string()))?;
    let len: u32 = body
        .len()
        .try_into()
        .map_err(|_| FrameError::PayloadTooLarge(body.len()))?;
    let mut out = crate::AlignedVec::with_capacity(IPC_FRAME_HEADER_LEN + body.len());
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
pub fn frame_request(req: &IpcRequest) -> Result<crate::AlignedVec, FrameError> {
    frame_ipc_bytes(req)
}

/// Serialize an [`IpcResponse`] into an `AlignedVec` with magic + length framing.
///
/// Symmetric to [`frame_request`].
///
/// # Errors
///
/// Returns a [`FrameError`] if serialization fails or the payload is too large.
pub fn frame_response(resp: &IpcResponse) -> Result<crate::AlignedVec, FrameError> {
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
    /// Payload exceeds what an rkyv archive can address
    /// ([`crate::MAX_ARCHIVE_BYTES`], 2 GiB) or what the u32 length prefix can
    /// declare — in practice a bug.
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
            Self::PayloadTooLarge(n) => {
                write!(f, "payload {n} bytes exceeds the serializable maximum")
            }
            Self::Truncated(n) => write!(f, "frame truncated: got {n} bytes, need at least 8"),
            Self::BadMagic(m) if *m == IPC_MAGIC_V1 => write!(
                f,
                "peer is speaking the v1 rkyv wire format (pre-0.8 build); \
                 rebuild and restart it — daemon and clients must share one build"
            ),
            Self::BadMagic(m) => write!(f, "bad magic bytes {m:?}; expected {IPC_MAGIC:?}"),
            Self::LengthMismatch { declared, actual } => write!(
                f,
                "length mismatch: header says {declared} bytes, body is {actual}"
            ),
        }
    }
}

impl std::error::Error for FrameError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Guarda barato contra o modo de falha real do teto anti-panic: se
    /// `raw_bytes` esquecer um campo, o payload é subestimado e o panic do rkyv
    /// volta a ser alcançável. Verifica a aritmética sem alocar gigabytes —
    /// o caminho de recusa em si é exercido por `saga_ipc::tests`.
    #[test]
    fn raw_bytes_counts_every_variable_length_field() {
        let req = IpcRequest {
            hook: "cli-ast-meta".into(), // 12
            payload: vec![0u8; 5],       // 5
            project_root: "/tmp".into(), // 4
            session_id: "abc".into(),    // 3
            priority: 0,
        };
        assert_eq!(req.raw_bytes(), 24);

        let resp = IpcResponse {
            output: "hello".into(), // 5
            success: true,
        };
        assert_eq!(resp.raw_bytes(), 5);
    }

    /// O valor de versionar o fio não é apenas RECUSAR — o bytecheck já recusava.
    /// É **nomear a causa**: sem o discriminador, um peer pré-0.8 falhava lá na
    /// validação com `invalid UTF-8`, que parece corrupção e manda o operador
    /// investigar o lado errado. Este teste prova a mensagem, não só o erro.
    #[test]
    fn legacy_v1_frame_is_diagnosed_by_name_not_as_generic_corruption() {
        let mut legacy = IPC_MAGIC_V1.to_vec();
        legacy.extend_from_slice(&0u32.to_le_bytes());

        let err = unframe(&legacy).expect_err("v1 magic must be rejected");
        assert!(matches!(err, FrameError::BadMagic(m) if m == IPC_MAGIC_V1));

        let msg = err.to_string();
        assert!(msg.contains("v1"), "must name the wire version: {msg}");
        assert!(
            msg.contains("rebuild"),
            "must say what to do about it: {msg}"
        );
    }

    /// Contraprova: um magic qualquer NÃO deve ser atribuído ao rollout — senão a
    /// mensagem específica viraria ruído que esconde corrupção de verdade.
    #[test]
    fn unknown_magic_is_not_blamed_on_the_rollout() {
        let mut junk = b"XXXX".to_vec();
        junk.extend_from_slice(&0u32.to_le_bytes());

        let msg = unframe(&junk)
            .expect_err("junk magic must be rejected")
            .to_string();
        assert!(
            !msg.contains("v1"),
            "must not misattribute corruption: {msg}"
        );
    }
}
