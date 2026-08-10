//! `touring portfolio` — prior-art discovery keyed by purpose.
//!
//! ```text
//! touring portfolio "<intento>"                  # consultar o prior-art
//! touring portfolio refresh [--root <path>]...   # (re)minerar o corpus
//! touring portfolio status                       # cobertura do índice
//! touring portfolio verdict "<intento>" --choice <v> --why "<razão>" [--artifact <id>] [--reward <f>]
//! touring portfolio inspect <arquivo>            # o que o minerador extrai deste arquivo
//! touring portfolio history                      # vereditos registrados
//! ```
//!
//! The query surface deliberately prints three sections, never a bare ranked
//! list: prior art *with evidence*, the gaps that prior art leaves, and the
//! external lens to consult. See [`crate::portfolio`] for why.

use anyhow::{Context, Result};

use crate::portfolio::{
    PortfolioAnswer, Verdict,
    feedback::{self, VerdictRecord},
    miner, query,
    store::{self, PortfolioIndex},
};

/// Default number of prior-art candidates shown.
const DEFAULT_TOP_K: usize = 5;

/// Read the value following `flag` in `args`, if present.
fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

/// Every `--flag value` pair for a repeatable flag.
fn flag_values(args: &[String], flag: &str) -> Vec<String> {
    args.iter()
        .enumerate()
        .filter(|(_, a)| a.as_str() == flag)
        .filter_map(|(i, _)| args.get(i + 1).cloned())
        .collect()
}

/// Positional words: everything after the subcommand that is neither a flag nor
/// a flag's value.
fn positional(args: &[String], skip: usize) -> String {
    let flags_with_values = ["--root", "--choice", "--why", "--artifact", "--reward", "--top"];
    let mut out: Vec<&str> = Vec::new();
    let mut i = skip;
    while i < args.len() {
        let a = args[i].as_str();
        if flags_with_values.contains(&a) {
            i += 2;
            continue;
        }
        if a.starts_with('-') {
            i += 1;
            continue;
        }
        out.push(a);
        i += 1;
    }
    out.join(" ")
}

/// Entry point for the `portfolio` subcommand.
///
/// `args` is the full process argv. Always returns `Ok` for the query path —
/// discovery is advisory and must never fail a caller's pipeline; `refresh` and
/// `verdict` do surface IO errors, because a silent failure there would rot the
/// index without anyone noticing.
///
/// # Errors
/// Propagates IO failures from `refresh` and `verdict`.
pub fn run(args: &[String]) -> Result<()> {
    let json = args.iter().any(|a| a == "-j" || a == "--json");
    match args.get(2).map(String::as_str) {
        Some("refresh") => cmd_refresh(args, json),
        Some("status") => cmd_status(json),
        Some("verdict") => cmd_verdict(args, json),
        Some("inspect") => cmd_inspect(args, json),
        Some("history") => cmd_history(json),
        _ => cmd_query(args, json),
    }
}

/// (Re)mine the corpus and materialize the index.
fn cmd_refresh(args: &[String], json: bool) -> Result<()> {
    let custom = flag_values(args, "--root");
    let roots: Vec<std::path::PathBuf> = if custom.is_empty() {
        miner::default_roots()
    } else {
        custom.into_iter().map(std::path::PathBuf::from).collect()
    };

    let mut entries = miner::mine(&roots);
    let dir = store::index_dir();
    // Layer the recorded verdicts back on as evidence (the pheromone).
    let recorded = feedback::history(&dir).unwrap_or_default();
    feedback::apply_history(&mut entries, &recorded);

    let index = PortfolioIndex {
        version: store::INDEX_VERSION,
        built_at: store::now_stamp(),
        roots: roots.iter().map(|r| miner::display_path(r)).collect(),
        entries,
    };
    let path = store::save_to(&dir, &index).context("saving portfolio index")?;

    if json {
        println!(
            "{}",
            serde_json::json!({
                "indexed": index.entries.len(),
                "roots": index.roots,
                "path": path.display().to_string(),
                "verdicts_applied": recorded.len(),
            })
        );
    } else {
        println!("portfólio indexado: {} artefatos", index.entries.len());
        for r in &index.roots {
            println!("  raiz  {r}");
        }
        println!("  arquivo {}", path.display());
        if !recorded.is_empty() {
            println!("  vereditos reaplicados: {}", recorded.len());
        }
    }
    Ok(())
}

/// Report index coverage, so an empty or stale portfolio is visible.
fn cmd_status(json: bool) -> Result<()> {
    let index = store::load().unwrap_or_else(|_| PortfolioIndex::empty());
    let mut by_kind: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    let mut inherited = 0usize;
    for e in &index.entries {
        *by_kind.entry(e.kind.tag()).or_default() += 1;
        if e.purpose_inherited {
            inherited += 1;
        }
    }
    if json {
        println!(
            "{}",
            serde_json::json!({
                "entries": index.entries.len(),
                "by_kind": by_kind,
                "purpose_inherited": inherited,
                "roots": index.roots,
                "built_at": index.built_at,
                "path": store::index_path().display().to_string(),
            })
        );
        return Ok(());
    }
    if index.is_empty() {
        println!("portfólio vazio — rode: touring portfolio refresh");
        return Ok(());
    }
    println!("portfólio: {} artefatos ({})", index.entries.len(), index.built_at);
    for (kind, n) in &by_kind {
        println!("  {kind:<8} {n}");
    }
    println!("  propósito herdado do bundle: {inherited}");
    for r in &index.roots {
        println!("  raiz  {r}");
    }
    Ok(())
}

/// Record a verdict about an intent (the compounding loop).
fn cmd_verdict(args: &[String], json: bool) -> Result<()> {
    let intent = positional(args, 3);
    let choice = flag_value(args, "--choice").unwrap_or_default();
    let why = flag_value(args, "--why").unwrap_or_default();

    let Some(verdict) = Verdict::parse(choice) else {
        eprintln!(
            "usage: touring portfolio verdict \"<intento>\" --choice <{}> --why \"<razão>\" [--artifact <id>] [--reward <0..1>]",
            Verdict::all().map(Verdict::tag).join("|")
        );
        return Ok(());
    };
    if intent.trim().is_empty() || why.trim().is_empty() {
        eprintln!("um veredito exige o intento e o --why (a razão é o que vira evidência)");
        return Ok(());
    }

    let rec = VerdictRecord {
        intent: intent.clone(),
        artifact_id: flag_value(args, "--artifact").map(str::to_string),
        verdict,
        rationale: why.to_string(),
        reward: flag_value(args, "--reward").and_then(|r| r.parse::<f64>().ok()),
        at: feedback::now_stamp(),
    };
    let dir = store::index_dir();
    feedback::record(&dir, &rec).context("recording verdict")?;
    let (memory_ok, reward_ok) = mirror_verdict(&rec);

    if json {
        println!(
            "{}",
            serde_json::json!({
                "record": rec,
                "mirrored_to_memory": memory_ok,
                "reward_emitted": reward_ok,
            })
        );
    } else {
        println!("veredito registrado: {} — {}", verdict.tag(), intent);
        println!(
            "  memória institucional: {} · reward RL: {}",
            if memory_ok { "ok" } else { "indisponível (log local é canônico)" },
            if reward_ok {
                "emitido"
            } else if rec.reward.is_some() {
                "indisponível"
            } else {
                "sem --reward"
            }
        );
    }
    Ok(())
}

/// Query the portfolio for an intent.
fn cmd_query(args: &[String], json: bool) -> Result<()> {
    let intent = positional(args, 2);
    if intent.trim().is_empty() {
        eprintln!("usage: touring portfolio [-j] \"<intento em linguagem natural>\"");
        eprintln!("  e.g. touring portfolio \"gerar um PDF profissional\"");
        eprintln!("  outros: refresh | status | verdict | inspect <arquivo> | history");
        return Ok(());
    }
    let top_k = flag_value(args, "--top")
        .and_then(|t| t.parse::<usize>().ok())
        .unwrap_or(DEFAULT_TOP_K);

    let index = store::load().unwrap_or_else(|_| PortfolioIndex::empty());
    // Semantic re-rank only when the human armed it (it may fetch a model).
    let scorer = crate::portfolio::semantic::FastEmbedSimilarity::if_armed();
    let ans = query::answer_with_scorer(
        &index,
        &intent,
        top_k,
        scorer.as_ref().map(|s| s as &dyn touring_foundation::portfolio::SemanticScorer),
    );

    if json {
        println!("{}", serde_json::to_string(&ans).unwrap_or_default());
        return Ok(());
    }
    print_answer(&ans);
    Ok(())
}

/// Render the three-section contract for a human reader.
fn print_answer(ans: &PortfolioAnswer) {
    println!("intento: {}", ans.intent);
    println!("corpus : {} artefatos indexados", ans.corpus_size);

    println!("\n── prior art ──");
    if ans.prior_art.is_empty() {
        println!("  (nenhum candidato acima do piso de ruído)");
    }
    for (i, h) in ans.prior_art.iter().enumerate() {
        let inherited = if h.entry.purpose_inherited { "  [propósito herdado do bundle]" } else { "" };
        println!(
            "  {}. {} [{}·{}] (score {:.2}){inherited}",
            i + 1,
            h.entry.name,
            h.entry.kind.tag(),
            h.entry.language,
            h.score
        );
        println!("     {}", h.entry.purpose);
        println!("     {} · {}", h.entry.display_path, h.entry.provenance);
        if let Some(ep) = &h.entry.entry_point {
            println!("     invocar: {ep}");
        }
        println!("     evidência: {}", h.entry.evidence.summary());
    }

    println!("\n── lacunas (o que o prior-art NÃO cobre) ──");
    if ans.gaps.is_empty() {
        println!("  (nenhuma lacuna detectada nos termos consultados)");
    }
    for g in &ans.gaps {
        println!("  · {g}");
    }

    println!("\n── lentes externas ──");
    for l in &ans.external {
        println!("  · [{}] {} — {}", l.source, l.subject, l.question);
    }

    println!("\n── veredito exigido ──");
    println!("  escolha um: {}", ans.verdict_required.join(" | "));
    println!(
        "  registre:   touring portfolio verdict \"{}\" --choice <escolha> --why \"<razão>\"",
        ans.intent
    );
}

/// Slug used as the memory key for an intent — stable, so a later verdict on
/// the same intent updates the same institutional record rather than piling up.
fn intent_slug(intent: &str) -> String {
    let s: String = intent
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    s.split('-')
        .filter(|p| !p.is_empty())
        .take(8)
        .collect::<Vec<_>>()
        .join("-")
}

/// Mirror a verdict into the institutional record, best-effort.
///
/// The append-only log in the portfolio directory stays canonical: the
/// portfolio is GLOBAL while `touring memory` is per-project, so the log is the
/// only store with the right scope. The mirror exists because the loop's
/// institutional lens reads `touring memory recall` — a verdict invisible there
/// is invisible to the OUTER protocol.
///
/// Returns `(memory_ok, reward_ok)`. A daemon that is down or a project without
/// an initialized memory DB simply yields `false`; the verdict is already
/// durable on disk by the time this runs.
fn mirror_verdict(rec: &VerdictRecord) -> (bool, bool) {
    let payload = serde_json::json!({
        "key": format!("portfolio-verdict:{}", intent_slug(&rec.intent)),
        "value": serde_json::json!({
            "intent": rec.intent,
            "verdict": rec.verdict.tag(),
            "artifact_id": rec.artifact_id,
            "rationale": rec.rationale,
            "at": rec.at,
        })
        .to_string(),
        "tier": "semantic",
        "entry_type": "decision",
        "reward": rec.reward,
        "outcome_context": rec.intent,
        "importance": serde_json::Value::Null,
        "pinned": false,
        "supersedes": serde_json::Value::Null,
    });
    let memory_ok = crate::daemon_client::daemon_query("cli-memory-store", payload).is_ok();
    let reward_ok = rec.reward.is_some_and(|r| {
        crate::daemon_client::daemon_query(
            "cli-learning-reward",
            serde_json::json!({
                "tool_name": "portfolio",
                "reward": r,
                "context": rec.intent,
            }),
        )
        .is_ok()
    });
    (memory_ok, reward_ok)
}

/// Show exactly what the miner extracts from one file.
///
/// The debugging surface the portfolio was missing: when an artifact does not
/// appear in a query, this answers whether it was mined at all, which prose was
/// taken, and which symbols cleared the floors.
fn cmd_inspect(args: &[String], json: bool) -> Result<()> {
    let target = positional(args, 3);
    if target.trim().is_empty() {
        eprintln!("usage: touring portfolio inspect [-j] <arquivo>");
        return Ok(());
    }
    let path = std::path::Path::new(target.trim());
    let Ok(content) = std::fs::read_to_string(path) else {
        eprintln!("não consegui ler {}", path.display());
        return Ok(());
    };
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    // Which extractor applies, and what each one yields — including the
    // fallbacks, so a `None` is legible as "this source had nothing".
    let artifact = match ext {
        "py" => miner::python_docstring(&content)
            .map(|p| ("docstring de módulo", p))
            .or_else(|| miner::argparse_description(&content).map(|p| ("argparse description", p))),
        "rs" => miner::rust_module_doc(&content).map(|p| ("cabeçalho //!", p)),
        "sh" => miner::shell_header(&content).map(|p| ("comentário inicial", p)),
        "md" => miner::markdown_frontmatter(&content).map(|(_, d)| ("frontmatter YAML", d)),
        "toml" => miner::adw_description(&content).map(|(_, d)| ("[adw] description", d)),
        _ => None,
    };
    let symbols: Vec<(String, String)> = match ext {
        "py" => miner::python_symbols(&content),
        "rs" => miner::rust_symbols(&content),
        _ => Vec::new(),
    }
    .into_iter()
    .map(|s| (s.name, s.purpose))
    .collect();

    if json {
        println!(
            "{}",
            serde_json::json!({
                "path": miner::display_path(path),
                "language": ext,
                "artifact_purpose": artifact.as_ref().map(|(_, p)| p),
                "purpose_source": artifact.as_ref().map(|(s, _)| s),
                "symbols": symbols.iter().map(|(n, p)| serde_json::json!({"name": n, "purpose": p}))
                    .collect::<Vec<_>>(),
            })
        );
        return Ok(());
    }

    println!("arquivo: {}", miner::display_path(path));
    match &artifact {
        Some((source, purpose)) => {
            println!("propósito do artefato [{source}]:\n  {purpose}");
        }
        None => println!(
            "propósito do artefato: NENHUM extraído — só entra no índice por herança de bundle"
        ),
    }
    if symbols.is_empty() {
        println!("símbolos documentados: nenhum acima do piso de prosa");
    } else {
        println!("símbolos documentados ({}):", symbols.len());
        for (name, purpose) in &symbols {
            println!("  {name} — {purpose}");
        }
    }
    Ok(())
}

/// List the recorded verdicts and the latest decision per artifact.
fn cmd_history(json: bool) -> Result<()> {
    let dir = store::index_dir();
    let records = feedback::history(&dir).unwrap_or_default();
    let latest = feedback::latest_by_artifact(&records);

    if json {
        println!(
            "{}",
            serde_json::json!({
                "records": records.len(),
                "artifacts_with_verdict": latest.len(),
                "history": records,
            })
        );
        return Ok(());
    }
    if records.is_empty() {
        println!("nenhum veredito registrado ainda");
        return Ok(());
    }
    println!("{} veredito(s); {} artefato(s) com decisão", records.len(), latest.len());
    for r in &records {
        let reward = r.reward.map_or_else(|| "—".to_string(), |v| format!("{v:.2}"));
        println!(
            "  [{}] {} · reward {reward}\n      intento: {}\n      porquê:  {}",
            r.verdict.tag(),
            r.artifact_id.as_deref().unwrap_or("(sem artefato)"),
            r.intent,
            r.rationale
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(parts: &[&str]) -> Vec<String> {
        std::iter::once("touring".to_string())
            .chain(parts.iter().map(|s| (*s).to_string()))
            .collect()
    }

    #[test]
    fn positional_skips_flags_and_their_values() {
        let a = argv(&["portfolio", "gerar", "um", "mapa", "--top", "3", "-j"]);
        assert_eq!(positional(&a, 2), "gerar um mapa");
    }

    #[test]
    fn positional_for_verdict_skips_the_subcommand_and_its_flags() {
        let a = argv(&[
            "portfolio", "verdict", "gerar", "PDF", "--choice", "reuse", "--why", "serve",
        ]);
        assert_eq!(positional(&a, 3), "gerar PDF");
    }

    #[test]
    fn flag_value_reads_the_following_token() {
        let a = argv(&["portfolio", "x", "--choice", "supersede"]);
        assert_eq!(flag_value(&a, "--choice"), Some("supersede"));
        assert_eq!(flag_value(&a, "--missing"), None);
    }

    #[test]
    fn flag_value_at_end_without_a_value_is_none_not_a_panic() {
        let a = argv(&["portfolio", "x", "--choice"]);
        assert_eq!(flag_value(&a, "--choice"), None);
    }

    #[test]
    fn repeatable_root_flag_collects_every_occurrence() {
        let a = argv(&["portfolio", "refresh", "--root", "/a", "--root", "/b"]);
        assert_eq!(flag_values(&a, "--root"), vec!["/a".to_string(), "/b".to_string()]);
    }

    #[test]
    fn empty_intent_is_usage_not_an_error() {
        assert!(run(&argv(&["portfolio"])).is_ok());
    }

    #[test]
    fn verdict_without_rationale_is_refused_but_not_fatal() {
        // The rationale is what becomes evidence; a verdict without it is noise.
        let a = argv(&["portfolio", "verdict", "algo", "--choice", "reuse"]);
        assert!(run(&a).is_ok(), "must not fail the pipeline");
    }

    #[test]
    fn unknown_verdict_choice_prints_usage_and_succeeds() {
        let a = argv(&["portfolio", "verdict", "algo", "--choice", "talvez", "--why", "x"]);
        assert!(run(&a).is_ok());
    }
}
