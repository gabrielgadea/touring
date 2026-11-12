//! The words of a verdict: the texts a gate speaks and the programs it hands over.
//!
//! Workspace rule D8 lives here: the text and the predicate derive from the same source, so a
//! reason never promises a route the executor would refuse. These strings are read by the
//! model, by `tool_trio`'s gate-id parser and by the test rubric — change one only on purpose.

use std::path::Path;

use serde_json::Value;

use super::*;

/// The portfolio index, cached and invalidated by the file's mtime.
///
/// A plain `OnceLock` was wrong here: the suggester runs in-daemon, the daemon
/// is long-lived, and `touring portfolio refresh` would therefore never be seen
/// until a restart — a portfolio that silently ages is worse than none, because
/// it keeps asserting stale prior art with full confidence. An mtime check is a
/// `stat` (microseconds) and is exact, so no TTL guesswork is needed.
pub(super) type CachedPortfolio = (
    std::sync::Arc<touring_foundation::portfolio::store::PortfolioIndex>,
    Option<std::time::SystemTime>,
);

pub(super) static PORTFOLIO: std::sync::OnceLock<std::sync::Mutex<Option<CachedPortfolio>>> =
    std::sync::OnceLock::new();

/// Last-modified time of the portfolio index file, when it exists.
pub(super) fn portfolio_mtime() -> Option<std::time::SystemTime> {
    std::fs::metadata(touring_foundation::portfolio::store::index_path())
        .ok()?
        .modified()
        .ok()
}

/// The portfolio index, reloaded whenever the file on disk has changed.
pub(crate) fn portfolio_index()
-> Option<std::sync::Arc<touring_foundation::portfolio::store::PortfolioIndex>> {
    let cell = PORTFOLIO.get_or_init(|| std::sync::Mutex::new(None));
    let mtime = portfolio_mtime();
    let mut guard = cell.lock().ok()?;
    if let Some((index, cached_mtime)) = guard.as_ref()
        && *cached_mtime == mtime
    {
        return Some(std::sync::Arc::clone(index));
    }
    let loaded = std::sync::Arc::new(
        touring_foundation::portfolio::store::load()
            .unwrap_or_else(|_| touring_foundation::portfolio::store::PortfolioIndex::empty()),
    );
    *guard = Some((std::sync::Arc::clone(&loaded), mtime));
    Some(loaded)
}

/// 1:1 remedy of the inline: the SAME body in `touring run --lang python --code`.
/// No fusion — the historical reason for excluding the heredoc from the burst was R9
/// crushing multi-line; with its own remedy it fell (tightening 29/08).
#[cfg(test)]
pub(super) fn python_inline_remedy(cmd: &str) -> String {
    python_inline_remedy_via(cmd, &g10_route::Route::Sandboxed)
}

/// The 1:1 remedy of the inline over the ROUTE that executes the body (G10ROTA, 30.4.97): Sandboxed is
/// plain `touring run`; Trusted carries the PER-COMMAND elevation (`TOURING_TRUSTED_OK=1` in the
/// command itself) and the `--allow-forbidden` flag. For Sandboxed the output is byte-identical to what it always was.
pub(super) fn python_inline_remedy_via(cmd: &str, rota: &g10_route::Route) -> String {
    let cabeca = format!(
        "{}touring run {}--lang python --code",
        rota.env_prefix(),
        rota.run_flags()
    );
    match python_inline_body(cmd) {
        Some(b) if !b.is_empty() && b.len() <= 1500 => {
            format!("{cabeca} '{}'", b.replace('\'', "'\\''"))
        }
        _ => format!("{cabeca} '<o corpo do seu heredoc/-c, verbatim>'"),
    }
}

/// The route that executes the body of the python-inline denied by G10: the question belongs to the EXECUTOR
/// (`g10_route`), not to the gate. Outside `python-inline` the remedy is R9 in bash — with no python
/// body to inspect, the route is the usual one.
pub(super) fn g10_inline_route(project_root: &Path, class: &str, cmd: &str) -> g10_route::Route {
    if class != "python-inline" {
        return g10_route::Route::Sandboxed;
    }
    match python_inline_body(cmd) {
        Some(corpo) if !corpo.trim().is_empty() => {
            g10_route::route_for(project_root, "python", &corpo)
        }
        _ => g10_route::Route::Sandboxed,
    }
}

/// The opening of the G10 remedy: the usual one on the Sandboxed route; on the others, the phrase that explains
/// why the route differs — BEFORE the command, so that whoever copies the command has already read the why.
pub(super) fn g10_lead(rota: &g10_route::Route, project_root: &Path) -> String {
    let explicacao = rota.explanation(project_root);
    if explicacao.is_empty() {
        " — a rajada desenrolada JÁ é o programa; rode-o de uma vez:".to_string()
    } else {
        format!(" — a rajada desenrolada JÁ é o programa.{explicacao} Rode-o de uma vez:")
    }
}

/// What follows the header of a G10 deny when NO `touring run` route executes the body: the route
/// that DOES work — the denied command itself under the per-command bypass, native — comes FIRST,
/// as the ready line, and the why after it. Retro 11/11/2026 (d): the why used to lead and the way
/// out came last, behind a paragraph about Landlock — and three guards disagreed (the deny named
/// `touring run`, the sandbox could not see `~/claude-artefatos`, the tmp-write-guard sent the
/// work there), so calls were redone. There is no `touring run` command to paste: naming one that
/// fails was the defect.
pub(super) fn g10_no_route_tail(rota: &g10_route::Route, project_root: &Path, cmd: &str) -> String {
    format!(
        " — nenhuma rota de `touring run` o executa; a que funciona é a nativa, por comando. \
         {}\nPor quê:{}",
        bypass_next_step(cmd),
        rota.explanation(project_root)
    )
}

/// The G10 exec-burst deny when NO `touring run` route executes the body.
pub(super) fn g10_unrunnable_deny(
    n: u32,
    class: &str,
    rota: &g10_route::Route,
    project_root: &Path,
    cmd: &str,
) -> String {
    format!(
        "[G10 exec-burst] {n}ª chamada seriada do executor `{class}` (0 `touring run` na janela \
         de {EXEC_BURST_WINDOW_SECS}s){}",
        g10_no_route_tail(rota, project_root, cmd)
    )
}

/// The fused python-inline burst: the BODIES in order, one program. The
/// budget contract is R9's — whole or out, never truncated; the
/// second element counts the omitted ones.
pub(super) fn python_inline_burst_program(cmds: &[String]) -> (String, usize) {
    const BUDGET: usize = 4000;
    let mut out = String::new();
    let mut omitidos = 0usize;
    for cmd in cmds {
        let Some(body) = python_inline_body(cmd).filter(|b| !b.is_empty()) else {
            omitidos += 1;
            continue;
        };
        let piece = body.replace('\'', "'\\''");
        if out.len() + piece.len() + 40 > BUDGET {
            omitidos += 1;
            continue;
        }
        if !out.is_empty() {
            out.push_str("\n\n# --- proximo comando da rajada ---\n");
        }
        out.push_str(&piece);
    }
    (out, omitidos)
}

/// The route of the script `path` (already on disk): reads the CONTENT — it is what X6 judges. Unreadable, empty or
/// too large for the gate to inspect → the usual route (without evidence there is nothing to change).
pub(super) fn g10_script_route(project_root: &Path, lang: &str, path: &str) -> g10_route::Route {
    const TETO: u64 = 256 * 1024;
    // `~/x.py` is the form the command carries; the sandbox sees the expanded, normalized path.
    let abs = resolve_operand(project_root, path).unwrap_or_else(|| {
        let p = Path::new(path);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            project_root.join(p)
        }
    });
    // Retro 11/11/2026 (d): the script FILE must be readable by the sandbox — `--file` of a path the
    // Landlock does not grant fails with "Permission denied", whatever the program does.
    if let Some(rota) = g10_route::script_out_of_reach(project_root, &abs) {
        return rota;
    }
    if std::fs::metadata(&abs).is_ok_and(|m| m.len() > TETO) {
        return g10_route::Route::Sandboxed;
    }
    match std::fs::read_to_string(&abs) {
        Ok(conteudo) if !conteudo.trim().is_empty() => {
            g10_route::route_for(project_root, lang, &conteudo)
        }
        _ => g10_route::Route::Sandboxed,
    }
}

/// S4 — the R9 program (aggregated exec): the N REAL calls, verbatim and in the
/// same order, each followed by the `[ok]`/`[FAILED N]` verdict (execution
/// continues on failures), final SUMMARY — the full output stays in the spill when it overflows.
///
/// In BASH, not python (30/08/2026): the previous template was `--lang python`
/// with `import subprocess` — the capability that X6 denied under Sandboxed, and the
/// G10 deny emitted a route that the next gate barred (measured by the peer
/// `analise-a2`). On the same day Gabriel extended the subprocess-only waiver to
/// every language, so the python template would run again — but bash
/// remains the right form HERE: the commands travel verbatim, 1:1, without the
/// subprocess rewrite layer that the model would have to check. Network and
/// destructive patterns remain hard-deny in every lang. `--timeout-ms` travels
/// explicitly (E4): N real tools do not fit in the default 30s.
/// The commands of a burst that ALREADY ran, as comment lines (1st line, up to
/// 120 chars). Single source of S212 for G10 and G1: repeating them repeated the effect
/// (a memory write, a commit) — they are context, never a step.
pub(super) fn comentarios_ja_executados(executed: &[String]) -> String {
    let mut corpo = String::new();
    for cmd in executed {
        let first: String = cmd.lines().next().unwrap_or("").chars().take(120).collect();
        corpo.push_str("# já executado (não repetir): ");
        corpo.push_str(&first);
        corpo.push('\n');
    }
    corpo
}

pub(super) fn r9_exec_program(executed: &[String], pending: &str) -> String {
    // S212 (25/09/2026): the commands of the burst that ALREADY ran travel as context
    // comments (first line only) — re-running them repeated their effects (a memory
    // write, a commit). Only the denied command is a step.
    let mut corpo = comentarios_ja_executados(executed);
    corpo.push_str("falhas=0\n");
    let mut ordem = 0usize;
    let cmds = [pending.to_string()];
    for cmd in &cmds {
        let cabe = corpo.chars().count() + cmd.chars().count() + 64 <= G1_BODY_BUDGET;
        if !cabe {
            continue; // whole or out, never truncated (same rule as the old R9)
        }
        ordem += 1;
        corpo.push_str(&format!("echo \"== {ordem}/{} ==\"\n", cmds.len()));
        corpo.push_str(cmd);
        corpo.push_str(&format!(
            " || {{ echo \"[FALHOU {ordem}] exit=$?\"; falhas=$((falhas+1)); }}\n"
        ));
    }
    corpo.push_str(&format!(
        "echo \"RESUMO: $(( {ordem} - falhas ))/{ordem} ok\""
    ));
    format!(
        "touring run --lang bash --timeout-ms 120000 --code '{}'",
        corpo.replace('\'', "'\\''")
    )
}

/// S6 (26/08) — prior-art intent derived from the real burst: the class of the
/// executor + the recurring targets (BM25 needs the terms of the WORK,
/// not the shape of the burst). Max 6 terms — a long intent dilutes the ranking.
pub(super) fn exec_burst_intent(class: &str, cmds: &[String]) -> String {
    /// Work component (dir or stem piece): ≥2 chars, non-digit,
    /// dedup. WHOLE path tokens do not enter — `tests/test_1.py`
    /// would dilute the intent in terms that no purpose contains (measured live:
    /// 6 terms → required_matches 3 → 2 matches → the portfolio mute with the
    /// right artifact on the shelf).
    fn push_componente(t: &str, termos: &mut Vec<String>) {
        let t = t.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '-');
        if t.len() >= 2 && !t.chars().all(|c| c.is_ascii_digit()) && !termos.iter().any(|x| x == t)
        {
            termos.push(t.to_string());
        }
    }
    let mut termos: Vec<String> = vec![class.to_string()];
    'outer: for cmd in cmds.iter().take(8) {
        for tok in effective_tokens(cmd).into_iter().skip(1) {
            let limpo = tok.trim_matches(|c: char| "'\"()[]{},".contains(c));
            if limpo.starts_with('-') {
                continue;
            }
            if limpo.contains('/') || limpo.contains('.') {
                let sem_ext = limpo.split('.').next().unwrap_or(limpo);
                for parte in sem_ext.split('/') {
                    for pedaco in parte.split(['_', '-']) {
                        push_componente(pedaco, &mut termos);
                    }
                }
                if termos.len() >= 6 {
                    break 'outer;
                }
            }
        }
    }
    termos.join(" ")
}

/// S6 — the closest prior art of the burst, instantiated: when the portfolio
/// has a program that ALREADY worked for this intent, the route comes written AND
/// comes from a real artifact (the difference from the R9 skeleton, which is generic
/// by construction). Fail-open in every direction: no index, no entry,
/// no entry_point, or entry_point with a non-derivable placeholder → `None` and
/// the deny comes out with R9 only (the previous behavior, honest).
pub(super) fn portfolio_remedy_for_burst(class: &str, cmds: &[String]) -> Option<String> {
    let index = portfolio_index()?;
    portfolio_remedy_for_burst_on(class, cmds, &index)
}

/// The body, with the index as a parameter — tests load from the scratch via
/// `store::load_from`, without `set_var` (see `GateOverrides`).
pub(super) fn portfolio_remedy_for_burst_on(
    class: &str,
    cmds: &[String],
    index: &touring_foundation::portfolio::store::PortfolioIndex,
) -> Option<String> {
    if index.is_empty() {
        return None;
    }
    let intent = exec_burst_intent(class, cmds);
    let answer = touring_foundation::portfolio::query::answer(index, &intent, 1);
    let top = answer.prior_art.first()?;
    let entry_point = top.entry.entry_point.as_deref()?;
    if entry_point.contains('<') {
        return None; // non-derivable placeholder: presenting it would be fabricating
    }
    Some(format!(
        "\n  prior art (programa que já funcionou para este trabalho): {entry_point} \
         — {} · score {:.2}",
        top.entry.display_path, top.score
    ))
}

/// The bypass line for `cmd`: the per-command token in front of the SAME command, ready to
/// paste. `None` when the command cannot travel on one line (heredoc, multi-line) or is too
/// long to paste without error.
pub(super) fn ready_bypass_line(cmd: &str) -> Option<String> {
    let c = cmd.trim();
    (!c.is_empty() && !c.contains('\n') && c.chars().count() <= READY_LINE_MAX_CHARS)
        .then(|| format!("{GATE_BYPASS_TOKEN} {c}"))
}

/// The closing sentence of a reason whose next step is the bypass itself (a G10 deny with no
/// runnable route): the ready line when the command fits on one line, the rule otherwise.
pub(super) fn bypass_next_step(cmd: &str) -> String {
    match ready_bypass_line(cmd) {
        Some(line) => {
            format!("Rode exatamente (bypass por-comando, contado como bypassed):\n  {line}")
        }
        None => format!(
            "Bypass por-comando: prefixe o comando negado, verbatim, com {GATE_BYPASS_TOKEN} \
             (contado como bypassed)."
        ),
    }
}

/// G6 — the three legitimate next steps after a byte-identical repeat, shared by the advisory
/// and the deny. Measured before the 27/10/2026 deploy: all 24 denials came from one session
/// whose first runs had all succeeded with the same result size and no call in between (the
/// redundant case, not live state), and in 21 the next call carried the same bytes — the model
/// acknowledged the gate in prose and re-emitted the command. The reason therefore hands over
/// the one command that is NOT a blind retry: the bypass line for the case where the state
/// changed outside the Edit/Write epoch this gate watches (build, git, daemon, a background
/// task). The token is per-command and charged to the G11 budget; both facts are stated.
pub(super) fn g6_next_steps(cmd: &str) -> String {
    let bypass = match ready_bypass_line(cmd) {
        Some(line) => format!("rode exatamente:\n  {line}"),
        None => format!("prefixe o comando abaixo, verbatim, com `{GATE_BYPASS_TOKEN} `."),
    };
    format!(
        "Próximo passo, escolha um: (1) o dado já basta — use o resultado acima e siga, sem \
         rodar de novo; (2) precisa de OUTRO recorte — mude o comando de verdade (outro padrão, \
         intervalo ou arquivo), ou reúna os recortes numa única chamada `touring run`; (3) o \
         estado mudou sem passar por Edit/Write (build, git, daemon, processo, log, rede, tarefa \
         em background, outra sessão) — repetir é legítimo, {bypass}\n\
         O bypass é por-comando e conta no G11 (o 2º seguido é negado; um `touring run` \
         recarrega o crédito)."
    )
}

/// The G6 advisory (first repeat): the call still runs, the next identical one is denied.
pub(super) fn g6_advisory_text(cmd: &str) -> String {
    format!(
        "[G6 redundant-exact-call] comando byte-idêntico há <{SUGGESTION_TTL_SECS}s sem nenhuma \
         mutação no meio — o resultado já está no seu contexto. Esta cópia executou; a PRÓXIMA \
         idêntica será negada.\n{}",
        g6_next_steps(cmd)
    )
}

/// The G6 deny (second repeat onwards). The command is echoed only when the bypass line cannot
/// carry it whole (multi-line or over `READY_LINE_MAX_CHARS`).
pub(super) fn g6_deny_reason(cmd: &str) -> String {
    let comando = if ready_bypass_line(cmd).is_some() {
        String::new()
    } else {
        let inicio: String = cmd.chars().take(200).collect();
        format!("\nComando: {inicio}")
    };
    format!(
        "[G6 redundant-exact-call] repetição byte-idêntica em {SUGGESTION_TTL_SECS}s sem mutação \
         no meio — o resultado idêntico já está no seu contexto (retry cego, 5 casos medidos).\n\
         {}\nProibido: reemitir este comando sem mudar nada — a próxima cópia idêntica é negada \
         de novo.{comando}",
        g6_next_steps(cmd)
    )
}

// ── W3 (code-mode-total plan) — MODE gates: G3 and G7 + G4/G5 telemetry ───

/// The Read window (1-based `offset`, `limit`) that covers the first occurrence of an Edit's
/// `old_string` with a little context — the lines the model must see to write the Edit right.
/// `None` when the file is unreadable, large, short, or does not contain `old_string` (the
/// reason then names the plain `Read`). Measured before the 27/10/2026 deploy: 589 of 607 Edits
/// after a PARTIAL Read succeeded, against 67 of 70 after a full one, so a window is a valid
/// Read for the harness.
pub(super) fn g3_read_window(
    project_root: &Path,
    fp: &str,
    old_string: &str,
) -> Option<(usize, usize)> {
    if old_string.is_empty() {
        return None;
    }
    let path = Path::new(fp);
    let full = if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_root.join(path)
    };
    let meta = std::fs::metadata(&full).ok()?;
    if !meta.is_file() || meta.len() > G3_WINDOW_MAX_BYTES {
        return None;
    }
    let text = std::fs::read_to_string(&full).ok()?;
    let total = text.split('\n').count();
    if total < G3_WINDOW_MIN_FILE_LINES {
        return None;
    }
    let at = text.find(old_string)?;
    let first = 1 + text[..at].matches('\n').count();
    let span = 1 + old_string.trim_end_matches('\n').matches('\n').count();
    let offset = first.saturating_sub(G3_WINDOW_CONTEXT_LINES).max(1);
    let limit = (first - offset) + span + G3_WINDOW_CONTEXT_LINES;
    (offset > 1 || limit < total).then_some((offset, limit))
}

/// The exact `Read` call a G3 reason hands over for an Edit of `fp`: the window over the Edit's
/// own `old_string` when it can be derived, the plain file otherwise.
pub(super) fn g3_read_call(project_root: &Path, fp: &str, tool_input: &Value) -> String {
    let old_string = tool_input
        .get("old_string")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match g3_read_window(project_root, fp, old_string) {
        Some((offset, limit)) => format!("Read {fp} offset={offset} limit={limit}"),
        None => format!("Read {fp}"),
    }
}

/// The G3 advisory (first unread Edit): the Edit still runs, the second one in a row is denied.
pub(super) fn g3_advisory_text(
    project_root: &Path,
    fp: &str,
    n: u32,
    tool_input: &Value,
) -> String {
    let leitura = g3_read_call(project_root, fp, tool_input);
    format!(
        "[G3 edit-sem-read] Edit de {fp} sem Read recente (advisory {n}/1 — o 2º seguido nega). \
         Próximo passo: `{leitura}` antes do Edit — ou um grep -n/sed -n/cat/head/tail via Bash \
         sobre este MESMO arquivo (caminho exato) nesta sessão; o Read zera o contador."
    )
}

/// The G3 deny. Measured before the 27/10/2026 deploy: in 37 of 40 denials the session had
/// already looked at the file through Bash only, and 9 of the 40 were answered with another
/// unread Edit — so the reason says what counts as a Read and what stays forbidden meanwhile.
/// Since 11/11/2026 (retro item c) a Bash read of the EXACT file counts too (`note_bash_reads`
/// feeds the same ledger), so the text names both forms — what the reason declares and what the
/// predicate applies derive from the same rule (D8).
pub(super) fn g3_deny_reason(project_root: &Path, fp: &str, n: u32, tool_input: &Value) -> String {
    let leitura = g3_read_call(project_root, fp, tool_input);
    format!(
        "[G3 edit-sem-read] {n}º Edit sem Read recente nesta sessão — os 11 erros \
         `edit_string_not_found` medidos nascem exatamente aqui. Próximo passo, exatamente: \
         `{leitura}`, e só então o Edit. Conta a leitura do arquivo exato — a ferramenta Read, ou \
         grep -n/sed -n/cat/head/tail via Bash sobre ele —; o Read ou o grep de OUTRO arquivo \
         não registra {fp}. Proibido até lá: Edit de {fp} ou de qualquer outro arquivo ainda \
         não lido."
    )
}

/// V0e3 (09/10/2026) — the R1 skeleton by the REAL PATH when it exists (measured
/// by 6b: the deny cited `<r1_varredura_agregado.py>`, a placeholder, and the
/// advisory `r1_varredura_agregado.py`, a nonexistent relative — the refusal
/// pointed to a file that did not exist, D8). Mirrors `_r1_route()` of
/// loop_stop_guard.py (the Python part, done the same night): without the file,
/// only the memory query — never a placeholder.
pub(super) fn r1_skeleton_path() -> Option<std::path::PathBuf> {
    let p = std::env::var_os("HOME").map(std::path::PathBuf::from)?.join(
        "projects/touring/docs/plans/2026-08-24-code-mode-antecipa/evidence/repertorio/r1_varredura_agregado.py",
    );
    p.is_file().then_some(p)
}

/// The R1 route for the target `fp`: the real skeleton instantiated when it exists, the
/// memory query when not.
pub(super) fn r1_route_for(fp: &str, skeleton: Option<&Path>) -> String {
    let query = "`touring memory query \"#kind:snippet #process:code-mode\"`";
    match skeleton {
        Some(p) => format!(
            "touring run --lang python --args '[\"{fp}\"]' --file {} (outros esqueletos: {query})",
            p.display()
        ),
        None => format!("esqueletos de varredura em {query}"),
    }
}

/// The edit route that the G7 deny names (D8: the phrase only comes out when it is true). The deny only
/// fires with the file unchanged since the recorded read (`g7_refresh_if_changed` zeroes the
/// count when it changes), and the G3 ledger was fed by this same read: the direct
/// `Edit` passes G3 and the harness accepts it. Without a stamp (file not measurable) the claim
/// «it did not change» has no backing — the phrase does not come out.
pub(super) fn g7_edit_route_note(fp: &str, measured: bool) -> String {
    if !measured {
        return String::new();
    }
    format!(
        " Para EDITAR {fp}: Edit direto — o arquivo não mudou desde a leitura registrada, então o \
         G3 e o harness a aceitam; se ele mudar, a releitura é liberada."
    )
}

/// Fuses the accumulated burst into an executable program body.
///
/// Returns `(body, omitted)`. The rule is **whole command or none**: a
/// command that does not fit in the budget is DROPPED, never truncated.
///
/// Origin (2026-08-25, observed live in this function): each command entered
/// with `cmd.chars().take(240)`, so a burst of long greps produced a
/// program cut in the middle of a path (`crates/tou'`) — syntactically
/// plausible, with balanced quotes, and that **does not run**. A truncated command
/// is worse than an absent one: the absent one shows up in the count that the remedy declares;
/// the truncated one disguises itself as a complete program. Gabriel's amendment (25/08) is
/// exactly this — the injection has to deliver the snippet that REPLACES the N
/// calls, and a snippet that fails replaces nothing.
pub(super) fn fuse_burst_program(cmds: &[String]) -> (String, usize) {
    let mut corpo = String::new();
    let mut omitidos = 0usize;
    for cmd in cmds {
        let cabe = corpo.chars().count() + cmd.chars().count() + 1 <= G1_BODY_BUDGET;
        if cabe {
            if !corpo.is_empty() {
                corpo.push('\n');
            }
            corpo.push_str(cmd);
        } else {
            omitidos += 1;
        }
    }
    (corpo.replace('\'', "'\\''"), omitidos)
}

/// The wall-clock budget of the call that the G1 rewrite replaces (09/10/2026).
/// The rewrite inherited the default 30 s of `touring run`; the original call had
/// Bash's — its `timeout`, or 120 s — and died at 30 s (`exit_code -2`, measured
/// in a 600 s wait loop). In background Bash has no ceiling: the maximum of
/// `touring run` applies. The slack lets the envelope return before the outer ceiling.
pub(super) fn rewrite_wall_ms(tool_input: &Value) -> u64 {
    const BASH_DEFAULT_MS: u64 = 120_000;
    const RUN_MAX_MS: u64 = 600_000;
    const FOLGA_MS: u64 = 5_000;
    if tool_input.get("run_in_background").and_then(Value::as_bool) == Some(true) {
        return RUN_MAX_MS;
    }
    let externo = tool_input
        .get("timeout")
        .and_then(Value::as_u64)
        .unwrap_or(BASH_DEFAULT_MS);
    externo.saturating_sub(FOLGA_MS).clamp(1_000, RUN_MAX_MS)
}

/// The `updatedInput` of a rewrite: the original call with ONLY the `command`
/// swapped (09/10/2026). The `updatedInput` REPLACES the input — measured live: two
/// calls with `run_in_background: true` and `timeout: 600000` rewritten by G2
/// came back "did not complete within its 120s timeout and was moved to the
/// background". Used by the three rewrites (G1, G2, G8).
pub(super) fn rewritten_input(tool_input: &Value, command: String) -> Value {
    let mut novo = tool_input.as_object().cloned().unwrap_or_default();
    novo.insert(
        "command".to_string(),
        Value::String(crate::hook_rewrite::mark_run_rewrite(&command)),
    );
    Value::Object(novo)
}

/// Declares the wall-clock budget in an already-assembled `touring run --lang bash …` program
/// (the one from G8 comes out of [`loop_rewrite_candidate`], which does not know the call).
pub(super) fn with_wall_budget(programa: String, wall_ms: u64) -> String {
    match programa.strip_prefix("touring run --lang bash ") {
        Some(resto) => format!("touring run --lang bash --timeout-ms {wall_ms} {resto}"),
        None => programa,
    }
}

/// The G1 advisory when live precision demotes it automatically (S-2.3): only `additionalContext`,
/// and the command stays NATIVE and intact. The old text («the accumulated burst is already the
/// program») sounded like an order to run the program — repeating the command that had just
/// run. Now the program is declared the FORMAT of the next burst, never a pending
/// step of this one.
pub(super) fn g1_advisory_text(count: u32, class: &str, programa: &str) -> String {
    format!(
        "[G1 rajada — AUTODEMOVIDO a advisory: precisão viva abaixo de \
         {G1_DEMOTE_FLOOR}] {count}ª inspeção `{class}` desde o último disparo. \
         ESTE comando roda NATIVO e intacto — não o repita. O programa abaixo é \
         o FORMATO para a próxima rajada: junte as inspeções seguintes num só \
         `touring run`:\n  {programa}"
    )
}

/// The G1 rewrite (V0e1). The `permissionDecisionReason` of an `allow` goes only to the
/// debug log (code.claude.com/docs/en/hooks, "For "allow" and "defer", written to
/// the debug log only") — neither the model nor the user sees it. The model, which receives the
/// sandbox output in place of that of its command, needs the `additionalContext` (which only
/// `defer` ignores; ceiling of 10,000 chars) — without it the output looked "like it came from another
/// call" (measured 09/10).
pub(super) fn g1_rewrite(
    motivo: &str,
    hint: &str,
    ja_executados: usize,
    tool_input: &Value,
    programa: String,
) -> Verdict {
    Verdict::Rewrite {
        gate: Gate::G1,
        reason: format!(
            "[G1 rajada-de-inspeção → REESCRITO no sandbox] {motivo}. Só o comando que \
             disparou executa no sandbox; os {ja_executados} já executados vêm como \
             comentário.{hint}"
        ),
        context: Some(format!(
            "[G1 rajada-de-inspeção → REESCRITO no sandbox] {motivo}. Este comando rodou \
             como `touring run` (a saída vem no JSON do sandbox). Os {ja_executados} \
             anterior(es) da rajada já tinham rodado e aparecem só como comentário — NÃO \
             rodaram de novo. Na próxima rajada, junte as inspeções em 1 programa."
        )),
        updated_input: rewritten_input(tool_input, programa),
    }
}

/// N6 (2026-09-23) — the fused program of the deny meets the ladder: when a similar
/// block ALREADY exists, the reason hands it over (affordance by construction, D8),
/// and the denied burst becomes discovery instead of only rewriting. The ruler measured the
/// opposite: `ladder_enrolled` 390 × `ladder_reused` 10 — discovery fails
/// at the exact moment of the action. Read-only; `None` on any error (testable with
/// an in-memory database).
pub(super) fn snippet_hint_line(conn: &rusqlite::Connection, program: &str) -> Option<String> {
    let m = touring_intelligence::rl::memory::snippet_stats::similar_snippets(conn, program, 1)
        .ok()?
        .into_iter()
        .next()?;
    Some(format!(
        "\nJá há um bloco parecido na escada: `{}` (sim {:.0}%, {} exec, {}) — \
         `touring memory recall \"{}\"` cobre sem reescrever.",
        m.entry_key,
        m.similarity * 100.0,
        m.executions,
        m.trust.badge(),
        m.entry_key,
    ))
}

/// The I/O wrapper: opens the project's memory.db read-only and returns ""
/// on any failure — a deny NEVER fails BECAUSE of the hint.
pub(super) fn snippet_hint_for_program(program: &str, root: &Path) -> String {
    let db = touring_foundation::TouringConfig::memory_db_canonical(root);
    if !db.exists() {
        return String::new();
    }
    rusqlite::Connection::open_with_flags(
        &db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()
    .and_then(|conn| snippet_hint_line(&conn, program))
    .unwrap_or_default()
}
