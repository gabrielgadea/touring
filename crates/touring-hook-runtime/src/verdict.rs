//! The ONE precedence rule of a PreToolUse chain (30.4.97, ordem dos gates).
//!
//! Invariant (coordenadora, 10/10/2026): every applicable deny beats every advisory, whatever
//! the order the gates were registered in. A gate that only ADVISES must not end the chain —
//! it deposits its text, the chain keeps evaluating, and the most severe verdict wins; the
//! advisory rides along (inside a deny's reason, inside any other verdict's context) and is
//! never lost.
//!
//! The chains speak two dialects — the `cli-suggest` gates return the wire JSON, the
//! `pre-bash`/`pre-edit`/`pre-write` handlers return [`HookResponse`](touring_hooks_core::hook_response::HookResponse) — and a rule written
//! twice diverges. This module is the single home: [`HookResponse::with_advisories`](touring_hooks_core::hook_response::HookResponse::with_advisories) and
//! [`merge_pretool_json`] are two adapters over the same two text helpers
//! ([`deny_reason_with`], [`context_with`]), and the test table below pins that both adapters
//! produce the same wire output for every verdict.

#[cfg(test)]
use crate::hook_runtime::HookResponse;
use serde_json::Value;
use std::path::Path;
use touring_foundation::injection_policy::{self, InjectionLevel};

/// The event every PreToolUse verdict answers as.
const PRE_TOOL_USE: &str = "PreToolUse";

// I-3 (31/10/2026): `HookResponse` is defined in `touring-hooks-core`, and an inherent
// impl (`with_advisories`, `with_advisories_at`) can only live in the defining crate —
// they moved there together with the two text helpers and the marker, so the JSON
// adapter below and the typed adapter keep ONE rule. Re-exported here: the old paths
// (`verdict::context_with`, `verdict::deny_reason_with`, `verdict::ALSO_NON_BLOCKING`)
// still resolve.
pub use touring_hooks_core::hook_response::{ALSO_NON_BLOCKING, context_with, deny_reason_with};

/// The wire form of [`HookResponse::with_advisories`](touring_hooks_core::hook_response::HookResponse::with_advisories) for the chains that answer in JSON.
///
/// `veredito` is the chain's own answer (`None`, `{}` or an object without
/// `hookSpecificOutput` all mean "no verdict"); `avisos` the texts deposited along the way.
/// Same rule: a deny keeps `permissionDecision: "deny"` and carries the advisories in its
/// reason; a rewrite or an advisory carries them in `additionalContext`; no verdict becomes a
/// plain advisory. An unparseable `veredito` is returned as it came (the chain's own bytes).
#[must_use]
pub fn merge_pretool_json(veredito: Option<String>, avisos: Vec<String>) -> Option<String> {
    let (level, sink) = injection_policy::current();
    merge_pretool_json_at(level, sink.as_deref(), veredito, avisos)
}

/// [`merge_pretool_json`] at an explicit injection `level` — the same advisory
/// filter as [`HookResponse::with_advisories_at`](touring_hooks_core::hook_response::HookResponse::with_advisories_at), so the two adapters still
/// answer alike.
#[must_use]
pub fn merge_pretool_json_at(
    level: InjectionLevel,
    sink: Option<&Path>,
    veredito: Option<String>,
    avisos: Vec<String>,
) -> Option<String> {
    let avisos =
        injection_policy::retain_allowed(level, injection_policy::SITE_VERDICT, avisos, sink);
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
            (
                "resultado-reescrito",
                HookResponse::ContextWithUpdatedToolOutput {
                    context: "contexto do resultado".into(),
                    event_name: Some("PostToolUse".into()),
                    updated_tool_output: serde_json::json!({"stdout": "s", "stderr": "", "interrupted": false}),
                },
            ),
            (
                "resultado-reescrito-sem-contexto",
                HookResponse::ContextWithUpdatedToolOutput {
                    context: String::new(),
                    event_name: Some("PostToolUse".into()),
                    updated_tool_output: serde_json::json!({"stdout": "s", "stderr": "", "interrupted": false}),
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

    // ── S-2 (27/10/2026): a política de injeção na montagem do veredito ──────

    const SERMON: &str =
        "[TOURING SUGGEST · code-mode-scan · conf=0.95]\n  MUST    touring run --lang bash";
    const FACT: &str = "[CODE MODE · rajada] 2a inspecao grep em 300s";

    fn deny() -> HookResponse {
        HookResponse::Deny {
            reason: "razao do deny".into(),
            context: None,
            event_name: Some("PreToolUse".into()),
        }
    }

    fn journal(path: &Path) -> Vec<Value> {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).expect("cada linha do diário é JSON"))
            .collect()
    }

    /// `no-advisory`: o aviso de sermão não é colado — a razão do deny e o
    /// veredito saem exatamente como o gate os produziu —, o aviso de fato
    /// (o deny de outro gate) continua colado, e o corte vai para o diário.
    #[test]
    fn test_no_advisory_drops_sermon_keeps_decision() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sink = touring_foundation::injection_journal::journal_path_in(dir.path());

        let so_sermao =
            deny().with_advisories_at(InjectionLevel::NoAdvisory, Some(&sink), vec![SERMON.into()]);
        assert_eq!(
            so_sermao,
            deny(),
            "sem aviso que sobreviva, o deny é o do gate"
        );

        let misto = deny().with_advisories_at(
            InjectionLevel::NoAdvisory,
            Some(&sink),
            vec![SERMON.into(), FACT.into()],
        );
        match &misto {
            HookResponse::Deny { reason, .. } => {
                assert!(reason.starts_with("razao do deny"), "{reason}");
                assert!(reason.contains(FACT), "o aviso de fato viaja: {reason}");
                assert!(
                    !reason.contains("TOURING SUGGEST"),
                    "o sermão não: {reason}"
                );
            }
            outro => panic!("o deny segue deny: {outro:?}"),
        }
        let j: Value = serde_json::from_str(&misto.to_json()).expect("json");
        assert_eq!(
            j.pointer("/hookSpecificOutput/permissionDecision"),
            Some(&serde_json::json!("deny"))
        );

        let lines = journal(&sink);
        assert_eq!(lines.len(), 2, "um corte por montagem que cortou");
        assert_eq!(lines[0]["site"], serde_json::json!("verdict"));
        assert_eq!(
            lines[0]["cuts"][0]["family"],
            serde_json::json!("touring-suggest")
        );
        assert_eq!(
            lines[0]["cuts"][0]["bytes"],
            serde_json::json!(SERMON.len())
        );
    }

    /// `off`: nenhum aviso é colado em lugar nenhum — e um `Allow` com avisos
    /// continua `Allow` (nada de contexto vazio no lugar do silêncio).
    #[test]
    fn test_off_attaches_no_advisory_to_any_verdict() {
        for (nome, v) in variantes() {
            let com = v
                .clone()
                .with_advisories_at(InjectionLevel::Off, None, avisos());
            assert_eq!(com, v, "{nome}: off não cola aviso");
        }
        assert_eq!(
            merge_pretool_json_at(InjectionLevel::Off, None, None, avisos()),
            None
        );
    }

    /// `Full` é o comportamento de sempre: o adaptador explícito e o do
    /// ambiente (sem env) devolvem a mesma coisa, e os dois adaptadores (enum e
    /// JSON) seguem pintando a mesma tabela em todos os níveis.
    #[test]
    fn test_levels_keep_both_adapters_in_step() {
        for level in [
            InjectionLevel::Full,
            InjectionLevel::NoAdvisory,
            InjectionLevel::Off,
        ] {
            let mistos = || vec![SERMON.to_string(), FACT.to_string()];
            for (nome, v) in variantes() {
                let pelo_enum = v
                    .clone()
                    .with_advisories_at(level, None, mistos())
                    .to_json();
                let pelo_json = merge_pretool_json_at(level, None, Some(v.to_json()), mistos())
                    .unwrap_or_else(|| v.to_json());
                assert_eq!(
                    sem_carimbo(&pelo_enum),
                    sem_carimbo(&pelo_json),
                    "{nome} @ {level:?}: enum e JSON divergem"
                );
            }
        }
        for (nome, v) in variantes() {
            assert_eq!(
                v.clone()
                    .with_advisories_at(InjectionLevel::Full, None, avisos()),
                v.clone().with_advisories(avisos()),
                "{nome}: Full == o caminho do ambiente sem env"
            );
        }
    }
}
