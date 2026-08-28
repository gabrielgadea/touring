//! Best practices gate — diretrizes E/A/M de elaboração de código (28/08/2026).
//!
//! Era um stub puro que delegava tudo ao CI externo. Hoje faz dois checks
//! REAIS, nos moldes do guard D8 (a declaração e o executor saem da mesma
//! fonte — texto que promete o que nada impõe é a origem da divergência):
//!
//! 1. **DECLARAÇÃO** — o catálogo E/A/M (ESCOLHER E1-E6 · ACERTAR A1-A14 ·
//!    MEDIR M1-M3) precisa estar declarado onde o agente lê: no manual
//!    canônico do workspace (`docs/code-mode.md`) ou na rule global
//!    (`~/.claude/rules/code-elaboration-directives.md`). Ausente dos dois →
//!    Warn: as diretrizes caíram da superfície.
//! 2. **MEDIÇÃO (M1)** — aderência viva do `~/.claude/touring/run_journal.jsonl`
//!    (`successfulExecuteCalls/total`, o CME do TanStack): taxa < 0.8 com
//!    amostra ≥ 20 runs → Warn. Régua completa: `touring kpi -j` →
//!    `code_mode_adherence`.
//!
//! O CI externo (`clippy::pedantic` + `cargo semver-checks`) segue nomeado no
//! payload — este gate soma, não substitui.

use std::path::{Path, PathBuf};

use crate::change::Change;
use crate::gate::{Gate, GateId, GateOutcome, GateSeverity};

/// Marcador que prova o catálogo completo (A14 é a diretriz mais recente:
/// cap de 3 tentativas no retry com autocorreção).
const CATALOG_MARKER: &str = "A14";
/// Título da seção no manual canônico.
const DOC_SECTION: &str = "Diretrizes de elaboração de código";
/// Amostra mínima para a régua M1 opinar (abaixo disso, fail-open).
const MIN_RUNS: u64 = 20;
/// Piso de aderência (M1) — abaixo com amostra suficiente, Warn.
const ADHERENCE_FLOOR: f64 = 0.8;

/// Gate das diretrizes E/A/M + ponte para o CI externo de best practices.
pub struct BestPracticesGate;

/// Onde o catálogo E/A/M está declarado, se em algum lugar: primeiro o manual
/// do workspace, depois a rule global. Puro sobre os caminhos para os testes
/// não dependerem de env.
fn declaration_site(root: &Path, home: Option<&Path>) -> Option<PathBuf> {
    let doc = root.join("docs/code-mode.md");
    if let Ok(t) = std::fs::read_to_string(&doc)
        && t.contains(DOC_SECTION)
        && t.contains(CATALOG_MARKER)
    {
        return Some(doc);
    }
    let rule = home?.join(".claude/rules/code-elaboration-directives.md");
    let t = std::fs::read_to_string(&rule).ok()?;
    (t.contains(CATALOG_MARKER)).then_some(rule)
}

/// `(runs_total, runs_ok)` do journal — a fração é a régua M1. Linha ilegível
/// é pulada (o journal é append-only de processos que morrem no meio).
fn adherence_counts(journal: &str) -> (u64, u64) {
    let mut total = 0u64;
    let mut ok = 0u64;
    for line in journal.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        total += 1;
        if v.get("exit_code").and_then(serde_json::Value::as_i64) == Some(0) {
            ok += 1;
        }
    }
    (total, ok)
}

impl Gate for BestPracticesGate {
    fn id(&self) -> GateId {
        GateId::BestPractices
    }
    fn severity(&self) -> GateSeverity {
        GateSeverity::Warn
    }
    fn check(&self, change: &Change) -> GateOutcome {
        let root = change
            .workspace_root
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let declared = declaration_site(&root, home.as_deref());

        let journal = home
            .as_ref()
            .map(|h| h.join(".claude/touring/run_journal.jsonl"))
            .and_then(|p| std::fs::read_to_string(p).ok());
        let (runs, ok) = journal.as_deref().map(adherence_counts).unwrap_or((0, 0));
        let rate = (runs > 0).then(|| ok as f64 / runs as f64);

        let payload = serde_json::json!({
            "directives": {
                "E_escolher": "E1-E6 (rota code vs tool: custo declarado, colapso no executor, transport completo)",
                "A_acertar": "A1-A14 (SDK plana, JSON tipado, erros estruturados, output-limit explícito, \
                              concorrência declarada, prompt byte-estável, retry máx 3, fail-loud, escada de trust)",
                "M_medir": "M1-M3 (régua touring kpi code_mode_adherence, modelo × apresentação, contenção no substrato)",
                "canonical": "docs/code-mode.md §Diretrizes de elaboração de código",
            },
            "declared_at": declared.as_ref().map(|p| p.display().to_string()),
            "adherence_m1": { "runs_total": runs, "runs_ok": ok, "success_rate": rate },
            "external": "clippy::pedantic lints + cargo semver-checks (ci.yml:gates, advisory)",
        });

        let outcome = if declared.is_none() {
            GateOutcome::warn(
                GateId::BestPractices,
                GateSeverity::Warn,
                "diretrizes E/A/M de elaboração de código não declaradas — reponha a seção em \
                 docs/code-mode.md ou a rule ~/.claude/rules/code-elaboration-directives.md",
            )
        } else if let Some(r) = rate
            && runs >= MIN_RUNS
            && r < ADHERENCE_FLOOR
        {
            GateOutcome::warn(
                GateId::BestPractices,
                GateSeverity::Warn,
                format!(
                    "aderência code-mode (M1) em {r:.2} < {ADHERENCE_FLOOR} sobre {runs} runs — \
                     inspecione `touring kpi -j` → code_mode_adherence.by_failure_kind"
                ),
            )
        } else {
            GateOutcome::pass(GateId::BestPractices, GateSeverity::Warn)
        };
        outcome.with_payload(payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declaration_site_finds_the_workspace_doc_first() {
        let dir = std::env::temp_dir().join(format!("tq-bp-{}", std::process::id()));
        let docs = dir.join("docs");
        std::fs::create_dir_all(&docs).expect("tempdir");
        std::fs::write(
            docs.join("code-mode.md"),
            "## Diretrizes de elaboração de código (E/A/M)\nA10/A14 retry máx 3.",
        )
        .expect("write doc");
        let hit = declaration_site(&dir, None).expect("doc declara o catálogo");
        assert!(hit.ends_with("docs/code-mode.md"));
        // sem o marcador A14, a seção velha (pré-catálogo) NÃO conta
        std::fs::write(docs.join("code-mode.md"), "## Diretrizes de elaboração de código\n")
            .expect("rewrite");
        assert!(declaration_site(&dir, None).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn adherence_counts_reads_exit_codes_and_skips_garbage() {
        let journal = concat!(
            "{\"exit_code\":0,\"language\":\"python\"}\n",
            "lixo-nao-json\n",
            "{\"exit_code\":1,\"language\":\"bash\"}\n",
            "{\"exit_code\":0}\n",
        );
        assert_eq!(adherence_counts(journal), (3, 2));
    }

    #[test]
    fn gate_carries_the_catalog_in_the_payload() {
        let out = BestPracticesGate.check(&Change::default());
        assert_eq!(out.gate_id, GateId::BestPractices);
        let dir = out.payload.get("directives").expect("catálogo no payload");
        for family in ["E_escolher", "A_acertar", "M_medir"] {
            assert!(dir.get(family).is_some(), "família {family} declarada");
        }
        assert!(
            dir.get("A_acertar")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|s| s.contains("A1-A14")),
            "o catálogo A inclui o cap A14"
        );
    }
}
