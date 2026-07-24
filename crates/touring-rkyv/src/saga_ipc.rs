//! Saga coordination messages for distributed 2PC via rkyv zero-copy IPC.
//!
//! All messages derive `#[archive(check_bytes)]` — byte validation is O(1)
//! and prevents UB on corrupted payloads. Use `frame_saga`/`unframe_saga`
//! for wire-format framing (`SAGA[4]` magic + u32 LE length prefix).
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

use rkyv::{Archive, Deserialize, Serialize};

/// Magic bytes identifying saga protocol frames on the socket.
/// Distinct from `RKYV[4]` (hook IPC) and preserves first-byte dispatch.
pub const SAGA_MAGIC: [u8; 4] = *b"SAGA";

/// Total framing overhead: magic (4) + length (4) = 8 bytes.
pub const SAGA_FRAME_LEN: usize = 8;

/// All saga coordination messages.
///
/// Byte sizes (archived, approximate):
///   Register: ~50B | Prepare: ~80B | Vote: ~60B
///   Commit: ~20B  | Rollback: ~50B | Delta: variable (body + overhead)
#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
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

// ── Error taxonomy ──────────────────────────────────────────────────────────

/// Errors produced by the saga framing and coordination logic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SagaError {
    /// Serialization of the message body failed (rkyv error).
    Serialize(String),
    /// Payload exceeds u32::MAX (4 GiB — always a bug if triggered).
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
            Self::PayloadTooLarge(n) => write!(f, "payload {n} exceeds u32::MAX"),
            Self::Truncated(n) => write!(f, "saga frame truncated: got {n}, need 8"),
            Self::BadMagic(m) => write!(f, "bad magic {m:?}; expected SAGA"),
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
pub fn frame_saga(msg: &SagaMessage) -> Result<rkyv::AlignedVec, SagaError> {
    let body = rkyv::to_bytes::<_, 256>(msg).map_err(|e| SagaError::Serialize(e.to_string()))?;
    let len: u32 = body
        .len()
        .try_into()
        .map_err(|_| SagaError::PayloadTooLarge(body.len()))?;
    let mut out = rkyv::AlignedVec::with_capacity(SAGA_FRAME_LEN + body.len());
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
        let parsed: SagaMessage = rkyv::from_bytes(body).unwrap();
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
        let parsed: SagaMessage = rkyv::from_bytes(body).unwrap();
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
        let parsed: SagaMessage = rkyv::from_bytes(body).unwrap();
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
        let parsed: SagaMessage = rkyv::from_bytes(body).unwrap();
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
        let parsed: SagaMessage = rkyv::from_bytes(body).unwrap();
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
        let parsed: SagaMessage = rkyv::from_bytes(body).unwrap();
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
        let parsed: SagaMessage = rkyv::from_bytes(body).unwrap();
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
        // A message that exceeds u32::MAX when serialized — this is impractical
        // but we test the error path for completeness.
        let big_msg = SagaMessage::Delta {
            transaction_id: "x".repeat((u32::MAX as usize) + 1),
            step_id: String::new(),
            delta_bytes: Vec::new(),
        };
        let result = frame_saga(&big_msg);
        assert!(matches!(result, Err(SagaError::PayloadTooLarge(_))));
    }
}
