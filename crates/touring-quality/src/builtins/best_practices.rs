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

/// Tabela canônica dos 15 sinais reativos definidos em
/// `docs/plans/2026-08-31-complementacao-hooks/specs/H1-H15.md`.
///
/// Cada entrada: (símbolo canônico para busca em fonte, descrição curta).
/// A busca é case-insensitive substring match no source — Rust permite `pub
/// fn <symbol>` em qualquer crate, então procuramos só o identificador.
const HOOKS_COMPLEMENT_SIGNALS: &[(&str, &str)] = &[
    ("measure_quality_snapshot", "H6 quality delta"),
    ("blast_radius_signal", "H3 dependents"),
    ("detect_cwes", "H6 scan vulnerabilities"),
    ("extract_pub_symbols", "H4 pub_api_diff"),
    ("enrich_with_cognitive", "H5 gotchas"),
    ("similar_symbol_signal_for_path", "H5 gotcha match"),
    ("wilson_adjusted_score", "H5 gotchas ranker"),
    ("assemble_scored_context", "H5 assemble scored"),
    ("normalize_scores", "H5 normalize"),
    ("apply_relevance_cutoff", "H5 relevance cutoff"),
    ("rank_gotchas_by_relevance", "H5 gotcha rank"),
    ("try_send_symbol", "H10 tantivy_stream"),
    ("is_active", "H10 tantivy_stream active"),
    ("spawn_stream_actor", "H10 tantivy_stream actor"),
    ("reindex_file", "H11 reindex"),
];

/// Collect text content of all .rs/.py files under `root/src/`, skipping
/// target/.git/node_modules. Returns concatenated text + per-symbol hit set.
fn collect_wired_signals(root: &Path) -> std::collections::BTreeSet<&'static str> {
    let mut hits = std::collections::BTreeSet::new();
    let src = root.join("src");
    if !src.exists() {
        return hits;
    }
    let queue = std::cell::RefCell::new(vec![src]);
    while let Some(dir) = {
        let mut q = queue.borrow_mut();
        q.pop()
    } {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if path.is_dir() {
                if matches!(name, "target" | ".git" | "node_modules" | "dist") {
                    continue;
                }
                queue.borrow_mut().push(path);
            } else if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                if ext != "rs" && ext != "py" {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else { continue };
                for (sym, _desc) in HOOKS_COMPLEMENT_SIGNALS {
                    if text.contains(sym) {
                        hits.insert(*sym);
                    }
                }
            }
        }
    }
    hits
}

/// Counts how many of the canonical 15 complementacao-hooks symbols appear
/// anywhere in the workspace's `src/` tree. Each symbol counted at most once.
fn hooks_complement_wired(root: &Path) -> (usize, usize) {
    let total = HOOKS_COMPLEMENT_SIGNALS.len();
    let hits = collect_wired_signals(root);
    (hits.len().min(total), total)
}

/// F5 S4 (2026-09-01) — counts distinct canonical SDK hooks with at least
/// one recorded invocation in the post-tool-use mirror. Mirrors live in
/// `~/.claude/touring/sdk_signal_mirror.jsonl` (append-only JSONL sink
/// written by F3/F4 — see `crates/touring-code/src/sdk_signal_mirror.rs`).
///
/// Returns `(used, total)` where `total` is the canonical 8-hook surface
/// from `crates/touring-code/src/sdk.rs::HookName::ALL`. The mirror file
/// may not exist yet (deployment race), in which case we return `(0, 8)`
/// and the gate is `pass` for signal_use — fail-open at the data-source
/// level, not at the gate level.
fn signal_use_counts(mirror_path: &Path) -> (usize, usize) {
    const TOTAL_HOOKS: usize = 8; // mirrors `HookName::ALL.len()`
    let Ok(text) = std::fs::read_to_string(mirror_path) else {
        return (0, TOTAL_HOOKS);
    };
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for line in text.lines() {
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if let Some(name) = entry.get("hook_name").and_then(|v| v.as_str()) {
            seen.insert(name.to_string());
        }
    }
    (seen.len().min(TOTAL_HOOKS), TOTAL_HOOKS)
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

        // 3ª regra: hooks_complement_use — quantos dos 15 sinais reativos da
        // complementacao-hooks estão wirados no source do workspace. Piso 12/15
        // = 80%. Abaixo → Warn. Fail-open se `src/` não existir.
        let (hc_wired, hc_total) = hooks_complement_wired(&root);
        let hc_payload = serde_json::json!({
            "wired": hc_wired,
            "total": hc_total,
            "ratio": hc_wired as f64 / hc_total.max(1) as f64,
        });

        // 4ª regra (F5 S4, 2026-09-01) — signal_use counts distinct canonical
        // SDK hooks invoked at runtime, read from the post-tool-use sink
        // (~/.claude/touring/sdk_signal_mirror.jsonl). Piso 6/8 = 75%.
        // Abaixo → Warn-severo. Severity stays Warn (não fail-closed) por
        // design do BestPracticesGate; promoção a Block requer decisão Gabriel.
        let mirror_path = home
            .as_ref()
            .map(|h| h.join(".claude/touring/sdk_signal_mirror.jsonl"))
            .unwrap_or_else(|| PathBuf::from(".claude/touring/sdk_signal_mirror.jsonl"));
        let (su_used, su_total) = signal_use_counts(&mirror_path);
        let su_payload = serde_json::json!({
            "used": su_used,
            "total": su_total,
            "ratio": su_used as f64 / su_total.max(1) as f64,
            "mirror": mirror_path.display().to_string(),
        });

        let mut outcome = if declared.is_none() {
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

        // Hierarquia: o WARN mais severo vence (declared missing > adherence low >
        // hooks_complement low > pass). Se outcome atual já é WARN por motivos
        // mais sérios, mantemos. Caso contrário, avaliamos hc_use.
        let hc_warn_threshold = (hc_total * 80) / 100; // 12/15 = 80%
        if hc_wired < hc_warn_threshold && hc_total > 0 && !matches!(outcome.status, crate::gate::GateStatus::Warn) {
            outcome = GateOutcome::warn(
                GateId::BestPractices,
                GateSeverity::Warn,
                format!(
                    "hooks_complement_use {hc_wired}/{hc_total} < 80% — wirings faltando para os \
                     15 sinais reativos da complementacao-hooks (H1-H15)"
                ),
            );
        }

        // F5 4ª regra — signal_use threshold. 6/8 = 75%; abaixo disso, o
        // operador está chamando menos da metade do SDK surface em produção.
        let su_warn_threshold = (su_total * 75) / 100; // 6/8 = 75%
        if su_used < su_warn_threshold
            && su_total > 0
            && !matches!(outcome.status, crate::gate::GateStatus::Warn)
        {
            outcome = GateOutcome::warn(
                GateId::BestPractices,
                GateSeverity::Warn,
                format!(
                    "signal_use {su_used}/{su_total} < 75% — menos da metade do SDK surface \
                     apareceu no mirror post-tool-use; inspecione `~/.claude/touring/sdk_signal_mirror.jsonl`"
                ),
            );
        }

        let mut merged = payload;
        if let serde_json::Value::Object(ref mut m) = merged {
            m.insert("hooks_complement_use".to_string(), hc_payload);
            m.insert("signal_use".to_string(), su_payload);
        }
        outcome.with_payload(merged)
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

    // F5 — signal_use counts distinct canonical SDK hooks in the mirror.
    #[test]
    fn signal_use_counts_missing_file_returns_zero() {
        let p = std::env::temp_dir().join(format!(
            "tq-bp-signal-nonexistent-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let (used, total) = signal_use_counts(&p);
        assert_eq!(used, 0);
        assert_eq!(total, 8);
    }

    #[test]
    fn signal_use_counts_distinct_hooks_from_mirror() {
        let p = std::env::temp_dir().join(format!(
            "tq-bp-signal-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let body = concat!(
            "{\"hook_name\":\"ast_meta\",\"duration_ms\":10,\"success\":true}\n",
            "{\"hook_name\":\"ast_meta\",\"duration_ms\":12,\"success\":true}\n", // dup, counts once
            "{\"hook_name\":\"memory_recall\",\"duration_ms\":40,\"success\":true}\n",
            "lixo\n", // skipped
            "{\"hook_name\":\"parallel\",\"duration_ms\":100,\"success\":false}\n",
        );
        std::fs::write(&p, body).expect("write mirror");
        let (used, total) = signal_use_counts(&p);
        assert_eq!(used, 3); // ast_meta + memory_recall + parallel (dedup)
        assert_eq!(total, 8);
        std::fs::remove_file(&p).ok();
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
