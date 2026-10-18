//! The ONE precedence rule of a PreToolUse chain (30.4.97, ordem dos gates).
//!
//! Invariant (coordenadora, 10/10/2026): every applicable deny beats every advisory, whatever
//! the order the gates were registered in. A gate that only ADVISES must not end the chain —
//! it deposits its text, the chain keeps evaluating, and the most severe verdict wins; the
//! advisory rides along (inside a deny's reason, inside any other verdict's context) and is
//! never lost.
//!
//! The chains speak two dialects — the `cli-suggest` gates return the wire JSON, the
//! `pre-bash`/`pre-edit`/`pre-write` handlers return [`HookResponse`] — and a rule written
//! twice diverges. This module is the single home: [`HookResponse::with_advisories`] and
//! [`merge_pretool_json`] are two adapters over the same two text helpers
//! ([`deny_reason_with`], [`context_with`]), and the test table below pins that both adapters
//! produce the same wire output for every verdict.

use crate::hook_runtime::HookResponse;
use serde_json::Value;

/// Marker that precedes the advisories carried inside a deny's `permissionDecisionReason`.
pub const ALSO_NON_BLOCKING: &str = "[também, sem bloquear]";

/// The event every PreToolUse verdict answers as.
const PRE_TOOL_USE: &str = "PreToolUse";

/// A deny's reason with the advisories that rode along. The deny stays a deny.
#[must_use]
pub fn deny_reason_with(reason: &str, avisos: &[String]) -> String {
    if avisos.is_empty() {
        return reason.to_string();
    }
    format!("{reason}\n\n{ALSO_NON_BLOCKING} {}", avisos.join("\n"))
}

/// The `additionalContext` of a non-deny verdict: the advisories first, the verdict's own
/// context after.
#[must_use]
pub fn context_with(avisos: &[String], context: Option<&str>) -> String {
    match context.filter(|c| !c.is_empty()) {
        Some(c) if !avisos.is_empty() => format!("{}\n{c}", avisos.join("\n")),
        Some(c) => c.to_string(),
        None => avisos.join("\n"),
    }
}

impl HookResponse {
    /// This verdict with the deposited `avisos` attached — deny > rewrite/advisory > nothing.
    ///
    /// * `Allow` has nothing to be more severe than: the advisories become the context.
    /// * `Deny` / `Block` / `Halt` keep their decision; the advisories enter the reason.
    /// * `Context` / `ContextWithUpdatedInput` keep their payload; the advisories lead the
    ///   context.
    ///
    /// An empty `avisos` returns `self` untouched.
    #[must_use]
    pub fn with_advisories(self, avisos: Vec<String>) -> HookResponse {
        if avisos.is_empty() {
            return self;
        }
        match self {
            HookResponse::Allow => HookResponse::Context {
                context: context_with(&avisos, None),
                event_name: Some(PRE_TOOL_USE.to_string()),
            },
            HookResponse::Context {
                context,
                event_name,
            } => HookResponse::Context {
                context: context_with(&avisos, Some(&context)),
                event_name,
            },
            HookResponse::Deny {
                reason,
                context,
                event_name,
            } => HookResponse::Deny {
                reason: deny_reason_with(&reason, &avisos),
                context,
                event_name,
            },
            HookResponse::Block {
                reason,
                context,
                event_name,
            } => HookResponse::Block {
                reason: deny_reason_with(&reason, &avisos),
                context,
                event_name,
            },
            HookResponse::Halt { reason } => HookResponse::Halt {
                reason: deny_reason_with(&reason, &avisos),
            },
            HookResponse::ContextWithUpdatedInput {
                context,
                event_name,
                updated_input,
            } => HookResponse::ContextWithUpdatedInput {
                context: context_with(&avisos, Some(&context)),
                event_name,
                updated_input,
            },
        }
    }
}

/// The wire form of [`HookResponse::with_advisories`] for the chains that answer in JSON.
///
/// `veredito` is the chain's own answer (`None`, `{}` or an object without
/// `hookSpecificOutput` all mean "no verdict"); `avisos` the texts deposited along the way.
/// Same rule: a deny keeps `permissionDecision: "deny"` and carries the advisories in its
/// reason; a rewrite or an advisory carries them in `additionalContext`; no verdict becomes a
/// plain advisory. An unparseable `veredito` is returned as it came (the chain's own bytes).
#[must_use]
pub fn merge_pretool_json(veredito: Option<String>, avisos: Vec<String>) -> Option<String> {
    if avisos.is_empty() {
        return veredito;
    }
    let sem_veredito = || {
        Some(
            serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": PRE_TOOL_USE,
                    "additionalContext": context_with(&avisos, None),
                }
            })
            .to_string(),
        )
    };
    let Some(bruto) = veredito else {
        return sem_veredito();
    };
    let Ok(mut v) = serde_json::from_str::<Value>(&bruto) else {
        return Some(bruto);
    };
    let Some(saida) = v
        .get_mut("hookSpecificOutput")
        .and_then(Value::as_object_mut)
    else {
        return if v.as_object().is_some_and(serde_json::Map::is_empty) {
            sem_veredito()
        } else {
            Some(bruto)
        };
    };
    if saida.get("permissionDecision").and_then(Value::as_str) == Some("deny") {
        let razao = saida
            .get("permissionDecisionReason")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let razao = deny_reason_with(razao, &avisos);
        saida.insert("permissionDecisionReason".into(), Value::String(razao));
    } else {
        let contexto = context_with(
            &avisos,
            saida.get("additionalContext").and_then(Value::as_str),
        );
        saida.insert("additionalContext".into(), Value::String(contexto));
    }
    Some(v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn avisos() -> Vec<String> {
        vec!["[A1 aviso] primeiro".into(), "[A2 aviso] segundo".into()]
    }

    fn variantes() -> Vec<(&'static str, HookResponse)> {
        vec![
            ("allow", HookResponse::Allow),
            (
                "context",
                HookResponse::Context {
                    context: "contexto do veredito".into(),
                    event_name: Some("PreToolUse".into()),
                },
            ),
            (
                "deny",
                HookResponse::Deny {
                    reason: "razao do deny".into(),
                    context: None,
                    event_name: Some("PreToolUse".into()),
                },
            ),
            (
                "deny-com-contexto",
                HookResponse::Deny {
                    reason: "razao do deny".into(),
                    context: Some("contexto junto do deny".into()),
                    event_name: Some("PreToolUse".into()),
                },
            ),
            (
                "reescrita",
                HookResponse::ContextWithUpdatedInput {
                    context: "contexto da reescrita".into(),
                    event_name: Some("PreToolUse".into()),
                    updated_input: serde_json::json!({"command": "ls"}),
                },
            ),
        ]
    }

    /// `hookEventName` é carimbado por um estado global do processo (`set_invoking_event`) que
    /// outro teste pode ligar; o que esta tabela compara é a regra, não o carimbo.
    fn sem_carimbo(json: &str) -> Value {
        let mut v: Value = serde_json::from_str(json).expect("json");
        if let Some(o) = v
            .get_mut("hookSpecificOutput")
            .and_then(Value::as_object_mut)
        {
            o.remove("hookEventName");
        }
        v
    }

    /// A TABELA DE PRECEDÊNCIA: os dois adaptadores (enum e JSON) dão a MESMA saída de fio para
    /// todo veredito — é o que impede o `cli-suggest` e os handlers de divergirem.
    #[test]
    fn os_dois_adaptadores_pintam_a_mesma_tabela_de_precedencia() {
        for (nome, v) in variantes() {
            let pelo_enum = v.clone().with_advisories(avisos()).to_json();
            let pelo_json = merge_pretool_json(Some(v.to_json()), avisos()).expect("veredito");
            assert_eq!(
                sem_carimbo(&pelo_enum),
                sem_carimbo(&pelo_json),
                "{nome}: enum e JSON divergem\n  enum: {pelo_enum}\n  json: {pelo_json}"
            );
        }
    }

    #[test]
    fn deny_vence_e_leva_o_aviso_na_razao() {
        let d = HookResponse::Deny {
            reason: "bloqueado".into(),
            context: None,
            event_name: None,
        };
        match d.with_advisories(avisos()) {
            HookResponse::Deny { reason, .. } => {
                assert!(reason.starts_with("bloqueado"), "{reason}");
                assert!(reason.contains(ALSO_NON_BLOCKING), "{reason}");
                assert!(reason.contains("[A1 aviso] primeiro"), "{reason}");
                assert!(reason.contains("[A2 aviso] segundo"), "{reason}");
            }
            outro => panic!("o deny segue deny: {outro:?}"),
        }
    }

    #[test]
    fn sem_veredito_o_aviso_vira_contexto_e_o_json_idem() {
        match HookResponse::Allow.with_advisories(avisos()) {
            HookResponse::Context { context, .. } => {
                assert_eq!(context, "[A1 aviso] primeiro\n[A2 aviso] segundo");
            }
            outro => panic!("Allow + avisos = Context: {outro:?}"),
        }
        for vazio in [None, Some("{}".to_string())] {
            let j = merge_pretool_json(vazio, avisos()).expect("advisory");
            let v: Value = serde_json::from_str(&j).expect("json");
            assert_eq!(
                v.pointer("/hookSpecificOutput/additionalContext")
                    .and_then(Value::as_str),
                Some("[A1 aviso] primeiro\n[A2 aviso] segundo")
            );
            assert!(
                v.pointer("/hookSpecificOutput/permissionDecision")
                    .is_none()
            );
        }
    }

    #[test]
    fn sem_avisos_o_veredito_passa_intocado() {
        for (nome, v) in variantes() {
            assert_eq!(v.clone().with_advisories(Vec::new()), v, "{nome}");
            let j = v.to_json();
            assert_eq!(
                merge_pretool_json(Some(j.clone()), Vec::new()),
                Some(j),
                "{nome}"
            );
        }
        assert_eq!(merge_pretool_json(None, Vec::new()), None);
    }

    #[test]
    fn a_reescrita_mantem_o_updated_input() {
        let r = HookResponse::ContextWithUpdatedInput {
            context: "c".into(),
            event_name: None,
            updated_input: serde_json::json!({"command": "ls"}),
        };
        match r.with_advisories(avisos()) {
            HookResponse::ContextWithUpdatedInput {
                context,
                updated_input,
                ..
            } => {
                assert!(context.starts_with("[A1 aviso] primeiro"), "{context}");
                assert!(context.ends_with("\nc"), "{context}");
                assert_eq!(updated_input, serde_json::json!({"command": "ls"}));
            }
            outro => panic!("a reescrita segue reescrita: {outro:?}"),
        }
    }

    #[test]
    fn json_nao_parseavel_volta_como_veio() {
        assert_eq!(
            merge_pretool_json(Some("não é json".into()), avisos()),
            Some("não é json".to_string())
        );
    }
}
