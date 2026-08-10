//! Saga coordination messages for distributed 2PC via rkyv zero-copy IPC.
//!
//! All messages get a `CheckBytes` impl from the derive (automatic in rkyv 0.8
//! with the `bytecheck` feature; the 0.7 `#[archive(check_bytes)]` opt-in is
//! gone) — byte validation is O(1) and prevents UB on corrupted payloads. Use
//! `frame_saga`/`unframe_saga` for wire-format framing (`SAGA[4]` magic + u32
//! LE length prefix).
//!
//! # Protocol
//!
//! ```text
//! Coordinator                                   Agent
//!    │                                            │
//!    │  ── Register ──────────────────────────►  │  (once, at startup)
//!    │  ◄── { registered: true } ───────────────│
//!    │                                            │
//!    │  [for each step in saga]                  │
//!    │  ── Prepare { tx_id, step_id } ───────► │  (2PC phase 1)
//!    │  ◄── Vote { commit: bool } ─────────────│
//!    │                                            │
//!    │  [if ALL votes = true]                    │
//!    │  ── Commit { tx_id } ──────────────────► │  (2PC phase 2)
//!    │  ◄── Delta { delta_bytes } ─────────────│  (after commit)
//!    │                                            │
//!    │  [if ANY vote = false]                    │
//!    │  ── Rollback { tx_id } ────────────────► │  (all agents)
//! ```

/// Magic bytes identifying saga protocol frames on the socket.
///
/// Distinto do magic do IPC de hooks e preserva o despacho por primeiro byte
/// (`'S'`). O último byte carrega a **versão do fio**, pelo mesmo motivo do
/// [`crate::ipc::IPC_MAGIC`]: **v2** = corpo produzido pelo rkyv 0.8, layout
/// incompatível com o v1.
pub const SAGA_MAGIC: [u8; 4] = *b"SAG2";

/// Magic da versão 1 do fio saga — corpo produzido pelo rkyv 0.7.
///
/// Mantido para **diagnóstico** da janela de rollout (ver [`crate::ipc::IPC_MAGIC_V1`]).
pub const SAGA_MAGIC_V1: [u8; 4] = *b"SAGA";

/// Total framing overhead: magic (4) + length (4) = 8 bytes.
pub const SAGA_FRAME_LEN: usize = 8;

// Defeito upstream do rkyv_derive 0.8.18 (verificado na fonte): para ENUMS,
// `generate_archived_variants` emite `#[doc]` em cada campo do tipo arquivado,
// mas `generate_resolver` NÃO — e campos de variante de enum são sempre tão
// visíveis quanto o enum, então o `SagaMessageResolver` gerado nasce público e
// sem doc, colidindo com o `#![deny(missing_docs)]` do crate. (Structs escapam
// porque lá o resolver copia a `vis` do campo-fonte e sai privado.) Nenhum knob
// do derive alcança o resolver — `attr(...)` só vai ao archived e `resolver`
// apenas renomeia.
//
// A saída é de VISIBILIDADE, não de supressão: encapsular o enum aqui deixa o
// resolver inalcançável de fora do crate, então o lint corretamente para de
// exigir doc dele — enquanto `SagaMessage`, reexportado abaixo, continua sob o
// `deny`. Zero `allow`, zero perda de cobertura.
mod message {
    use rkyv::{Archive, Deserialize, Serialize};

    /// All saga coordination messages.
    ///
    /// Byte sizes (archived, approximate):
    ///   Register: ~50B | Prepare: ~80B | Vote: ~60B
    ///   Commit: ~20B  | Rollback: ~50B | Delta: variable (body + overhead)
    #[derive(Archive, Serialize, Deserialize, Debug, Clone)]
    #[rkyv(derive(Debug))]
    pub enum SagaMessage {
        /// Agent → Coordinator: register intent to participate.
        /// Idempotent — re-registration with same agent_id updates step_count.
        Register {
            /// Unique identifier of the registering agent.
            agent_id: String,
            /// Number of saga steps the agent expects to participate in.
            step_count: u32,
        },

        /// Coordinator → Agent: ask agent to evaluate whether it can commit.
        /// `action` is an opaque JSON string scoped to the agent's domain.
        Prepare {
            /// Identifier of the saga transaction being prepared.
            transaction_id: String,
            /// Identifier of the step within the transaction.
            step_id: String,
            /// Opaque JSON action payload scoped to the agent's domain.
            action: String,
        },

        /// Agent → Coordinator: vote on the transaction.
        Vote {
            /// Identifier of the saga transaction being voted on.
            transaction_id: String,
            /// Identifier of the step the vote applies to.
            step_id: String,
            /// `true` if the agent can commit, `false` to abort.
            commit: bool,
            /// Human-readable explanation for the vote (especially on abort).
            reason: String,
        },

        /// Coordinator → All Agents: all voted yes; agent should commit atomically.
        Commit {
            /// Identifier of the saga transaction to commit.
            transaction_id: String,
        },

        /// Coordinator → All Agents: at least one voted no; full rollback required.
        Rollback {
            /// Identifier of the saga transaction to roll back.
            transaction_id: String,
            /// Human-readable reason the rollback was triggered.
            reason: String,
        },

        /// Agent → Coordinator: CRDT delta for state reconciliation post-commit.
        /// `delta_bytes` is already an rkyv-serialized CrdtDelta — coordinator
        /// stores as-is and can merge later without deserializing immediately.
        Delta {
            /// Identifier of the committed saga transaction.
            transaction_id: String,
            /// Identifier of the step producing the delta.
            step_id: String,
            /// rkyv-serialized `CrdtDelta` bytes for state reconciliation.
            delta_bytes: Vec<u8>,
        },
    }
}

pub use message::SagaMessage;

impl SagaMessage {
    /// Soma dos bytes crus que a mensagem carrega.
    ///
    /// Base do teto anti-panic ([`crate::MAX_ARCHIVE_BYTES`]): o arquivo rkyv é
    /// sempre ≥ os dados que carrega, então esta soma é um piso do tamanho
    /// arquivado e serve para recusar cedo o que faria o rkyv entrar em pânico.
    fn raw_bytes(&self) -> usize {
        match self {
            Self::Register { agent_id, .. } => agent_id.len(),
            Self::Prepare {
                transaction_id,
                step_id,
                action,
            } => transaction_id.len() + step_id.len() + action.len(),
            Self::Vote {
                transaction_id,
                step_id,
                reason,
                ..
            } => transaction_id.len() + step_id.len() + reason.len(),
            Self::Commit { transaction_id } => transaction_id.len(),
            Self::Rollback {
                transaction_id,
                reason,
            } => transaction_id.len() + reason.len(),
            Self::Delta {
                transaction_id,
                step_id,
                delta_bytes,
            } => transaction_id.len() + step_id.len() + delta_bytes.len(),
        }
    }
}

// ── Error taxonomy ──────────────────────────────────────────────────────────

/// Errors produced by the saga framing and coordination logic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SagaError {
    /// Serialization of the message body failed (rkyv error).
    Serialize(String),
    /// Payload exceeds what an rkyv archive can address
    /// ([`crate::MAX_ARCHIVE_BYTES`], 2 GiB) or what the u32 length prefix can
    /// declare — always a bug if triggered.
    PayloadTooLarge(usize),
    /// Frame shorter than SAGA_FRAME_LEN (8 bytes).
    Truncated(usize),
    /// First 4 bytes != SAGA_MAGIC.
    BadMagic([u8; 4]),
    /// Length prefix mismatch: header declared N bytes, body has M.
    LengthMismatch {
        /// Byte count claimed by the length prefix.
        declared: usize,
        /// Byte count actually present in the body slice.
        actual: usize,
    },
    /// Agent attempted to register with an id already registered.
    AlreadyRegistered(String),
    /// Operation referenced an unknown transaction id.
    UnknownTransaction(String),
    /// Operation attempted an invalid state transition.
    InvalidStateTransition {
        /// Name of the state the saga was in when the transition was rejected.
        from: &'static str,
        /// Human-readable detail of why the transition was invalid.
        msg: String,
    },
    /// Commit attempted but not all participants had voted prepare.
    NotAllPrepared(String),
    /// Delta submitted for a transaction that was not committed.
    NotCommitted(String),
    /// Agent attempted to send delta but was not registered.
    AgentNotRegistered(String),
}

impl core::fmt::Display for SagaError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Serialize(s) => write!(f, "saga serialize failed: {s}"),
            Self::PayloadTooLarge(n) => {
                write!(f, "payload {n} bytes exceeds the serializable maximum")
            }
            Self::Truncated(n) => write!(f, "saga frame truncated: got {n}, need 8"),
            Self::BadMagic(m) if *m == SAGA_MAGIC_V1 => write!(
                f,
                "peer is speaking the v1 saga wire format (pre-rkyv-0.8 build); \
                 rebuild and restart it — daemon and clients must share one build"
            ),
            Self::BadMagic(m) => write!(f, "bad magic {m:?}; expected {SAGA_MAGIC:?}"),
            Self::LengthMismatch { declared, actual } => {
                write!(f, "saga length mismatch: header={declared}, body={actual}")
            }
            Self::AlreadyRegistered(id) => write!(f, "agent '{id}' already registered"),
            Self::UnknownTransaction(tx) => write!(f, "unknown transaction '{tx}'"),
            Self::InvalidStateTransition { from, msg } => {
                write!(f, "invalid transition from {from}: {msg}")
            }
            Self::NotAllPrepared(tx) => write!(f, "commit on '{tx}' but not all prepared"),
            Self::NotCommitted(tx) => write!(f, "delta on '{tx}' but not committed"),
            Self::AgentNotRegistered(id) => write!(f, "agent '{id}' not registered"),
        }
    }
}

impl std::error::Error for SagaError {}

/// Frame a saga message into wire bytes (`SAGA[4]` + `len[4]` + body).
pub fn frame_saga(msg: &SagaMessage) -> Result<crate::AlignedVec, SagaError> {
    // Recusa ANTES de serializar: acima deste teto o rkyv 0.8 entra em pânico em
    // vez de devolver erro (ver `crate::MAX_ARCHIVE_BYTES`), e um daemon não pode
    // cair por causa do tamanho de uma mensagem que chegou pelo socket.
    let raw = msg.raw_bytes();
    if raw >= crate::MAX_ARCHIVE_BYTES {
        return Err(SagaError::PayloadTooLarge(raw));
    }
    let body = crate::to_bytes::<SagaMessage, 256>(msg)
        .map_err(|e| SagaError::Serialize(e.to_string()))?;
    let len: u32 = body
        .len()
        .try_into()
        .map_err(|_| SagaError::PayloadTooLarge(body.len()))?;
    let mut out = crate::AlignedVec::with_capacity(SAGA_FRAME_LEN + body.len());
    out.extend_from_slice(&SAGA_MAGIC);
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

/// Validate and unframe a saga wire frame, returning the body slice.
///
/// Errors on: truncated frame, bad magic, length mismatch.
/// Does NOT deserialize — caller decides whether to zero-copy (check_bytes)
/// or fully deserialize based on use case.
pub fn unframe_saga(bytes: &[u8]) -> Result<&[u8], SagaError> {
    if bytes.len() < SAGA_FRAME_LEN {
        return Err(SagaError::Truncated(bytes.len()));
    }
    let magic: [u8; 4] = bytes[..4].try_into().expect("infallible");
    if magic != SAGA_MAGIC {
        return Err(SagaError::BadMagic(magic));
    }
    let len_bytes: [u8; 4] = bytes[4..8].try_into().expect("infallible");
    let declared = u32::from_le_bytes(len_bytes) as usize;
    let body = &bytes[SAGA_FRAME_LEN..];
    if body.len() != declared {
        return Err(SagaError::LengthMismatch {
            declared,
            actual: body.len(),
        });
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_unframe_register() {
        let msg = SagaMessage::Register {
            agent_id: "agent-1".into(),
            step_count: 4,
        };
        let framed = frame_saga(&msg).unwrap();
        assert!(framed.starts_with(&SAGA_MAGIC));
        let body = unframe_saga(&framed).unwrap();
        let parsed: SagaMessage = crate::from_bytes(body).unwrap();
        match parsed {
            SagaMessage::Register {
                agent_id,
                step_count,
            } => {
                assert_eq!(agent_id, "agent-1");
                assert_eq!(step_count, 4);
            }
            _ => panic!("expected Register variant"),
        }
    }

    #[test]
    fn test_frame_unframe_prepare() {
        let msg = SagaMessage::Prepare {
            transaction_id: "tx-abc".into(),
            step_id: "step-1".into(),
            action: r#"{"type":"edit"}"#.into(),
        };
        let framed = frame_saga(&msg).unwrap();
        let body = unframe_saga(&framed).unwrap();
        let parsed: SagaMessage = crate::from_bytes(body).unwrap();
        match parsed {
            SagaMessage::Prepare {
                transaction_id,
                step_id,
                action,
            } => {
                assert_eq!(transaction_id, "tx-abc");
                assert_eq!(step_id, "step-1");
                assert_eq!(action, r#"{"type":"edit"}"#);
            }
            _ => panic!("expected Prepare variant"),
        }
    }

    #[test]
    fn test_frame_unframe_vote_yes() {
        let msg = SagaMessage::Vote {
            transaction_id: "tx-abc".into(),
            step_id: "step-1".into(),
            commit: true,
            reason: "ok".into(),
        };
        let framed = frame_saga(&msg).unwrap();
        let body = unframe_saga(&framed).unwrap();
        let parsed: SagaMessage = crate::from_bytes(body).unwrap();
        match parsed {
            SagaMessage::Vote { commit, .. } => assert!(commit),
            _ => panic!("expected Vote variant"),
        }
    }

    #[test]
    fn test_frame_unframe_vote_no() {
        let msg = SagaMessage::Vote {
            transaction_id: "tx-abc".into(),
            step_id: "step-1".into(),
            commit: false,
            reason: "file not found".into(),
        };
        let framed = frame_saga(&msg).unwrap();
        let body = unframe_saga(&framed).unwrap();
        let parsed: SagaMessage = crate::from_bytes(body).unwrap();
        match parsed {
            SagaMessage::Vote { commit, reason, .. } => {
                assert!(!commit);
                assert_eq!(reason, "file not found");
            }
            _ => panic!("expected Vote variant"),
        }
    }

    #[test]
    fn test_frame_unframe_commit() {
        let msg = SagaMessage::Commit {
            transaction_id: "tx-abc".into(),
        };
        let framed = frame_saga(&msg).unwrap();
        let body = unframe_saga(&framed).unwrap();
        let parsed: SagaMessage = crate::from_bytes(body).unwrap();
        match parsed {
            SagaMessage::Commit { transaction_id } => assert_eq!(transaction_id, "tx-abc"),
            _ => panic!("expected Commit variant"),
        }
    }

    #[test]
    fn test_frame_unframe_rollback() {
        let msg = SagaMessage::Rollback {
            transaction_id: "tx-abc".into(),
            reason: "step-2 voted no".into(),
        };
        let framed = frame_saga(&msg).unwrap();
        let body = unframe_saga(&framed).unwrap();
        let parsed: SagaMessage = crate::from_bytes(body).unwrap();
        match parsed {
            SagaMessage::Rollback { reason, .. } => assert_eq!(reason, "step-2 voted no"),
            _ => panic!("expected Rollback variant"),
        }
    }

    #[test]
    fn test_frame_unframe_delta() {
        let delta = vec![1u8, 2, 3, 4];
        let msg = SagaMessage::Delta {
            transaction_id: "tx-abc".into(),
            step_id: "step-1".into(),
            delta_bytes: delta.clone(),
        };
        let framed = frame_saga(&msg).unwrap();
        let body = unframe_saga(&framed).unwrap();
        let parsed: SagaMessage = crate::from_bytes(body).unwrap();
        match parsed {
            SagaMessage::Delta { delta_bytes, .. } => assert_eq!(delta_bytes, delta),
            _ => panic!("expected Delta variant"),
        }
    }

    // ── Error cases ────────────────────────────────────────────────────────

    #[test]
    fn test_unframe_truncated() {
        let result = unframe_saga(b"SAG"); // only 3 bytes
        assert!(matches!(result, Err(SagaError::Truncated(3))));
    }

    #[test]
    fn test_unframe_bad_magic() {
        let result = unframe_saga(b"RKYV\x00\x00\x00\x00");
        assert!(matches!(result, Err(SagaError::BadMagic(_))));
    }

    /// Espelha `ipc::tests::legacy_v1_frame_is_diagnosed_by_name_...` — os dois
    /// caminhos de fio precisam diagnosticar igual. Assimetria entre irmãos é
    /// exatamente o padrão de bug que este módulo já pagou uma vez (o
    /// duplo-unframe do handler saga no daemon).
    #[test]
    fn legacy_v1_saga_frame_is_diagnosed_by_name() {
        let mut legacy = SAGA_MAGIC_V1.to_vec();
        legacy.extend_from_slice(&0u32.to_le_bytes());

        let err = unframe_saga(&legacy).expect_err("v1 saga magic must be rejected");
        assert!(matches!(err, SagaError::BadMagic(m) if m == SAGA_MAGIC_V1));

        let msg = err.to_string();
        assert!(msg.contains("v1"), "must name the wire version: {msg}");
        assert!(
            msg.contains("rebuild"),
            "must say what to do about it: {msg}"
        );
    }

    #[test]
    fn test_unframe_length_mismatch() {
        // SAGA magic + u32 declaring 100 bytes + only 3 body bytes
        let mut bytes = SAGA_MAGIC.to_vec();
        bytes.extend_from_slice(&100u32.to_le_bytes());
        bytes.extend_from_slice(&[1u8, 2, 3]);
        let result = unframe_saga(&bytes);
        assert!(matches!(
            result,
            Err(SagaError::LengthMismatch {
                declared: 100,
                actual: 3
            })
        ));
    }

    #[test]
    fn test_payload_too_large() {
        // Constrói EXATAMENTE no teto documentado. Isto não é zelo teórico: acima
        // de `MAX_ARCHIVE_BYTES` o rkyv 0.8 derruba o processo (o `RelPtr::emplace`
        // de Vec/String fixa `rancor::Panic`) em vez de devolver erro, então o
        // guarda tem de recusar ANTES de serializar. O 0.7 errava graciosamente
        // aqui; a versão nova só erra graciosamente porque o guarda existe.
        //
        // Usar o teto real (2 GiB) em vez do antigo `u32::MAX + 1` também corta
        // pela metade a memória que o teste aloca.
        let big_msg = SagaMessage::Delta {
            transaction_id: "x".repeat(crate::MAX_ARCHIVE_BYTES),
            step_id: String::new(),
            delta_bytes: Vec::new(),
        };
        let result = frame_saga(&big_msg);
        assert!(matches!(result, Err(SagaError::PayloadTooLarge(_))));
    }

    #[test]
    fn raw_bytes_counts_every_variable_length_field() {
        // Guarda barato contra o modo de falha real do `raw_bytes`: esquecer um
        // campo faria o teto subestimar o payload e devolver o panic de volta.
        assert_eq!(
            SagaMessage::Delta {
                transaction_id: "abc".into(), // 3
                step_id: "de".into(),         // 2
                delta_bytes: vec![0u8; 7],    // 7
            }
            .raw_bytes(),
            12
        );
        assert_eq!(
            SagaMessage::Vote {
                transaction_id: "abcd".into(), // 4
                step_id: "ef".into(),          // 2
                commit: true,
                reason: "xyz".into(), // 3
            }
            .raw_bytes(),
            9
        );
    }
}
