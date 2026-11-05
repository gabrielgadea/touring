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
pub(crate) fn portfolio_index() -> Option<std::sync::Arc<touring_foundation::portfolio::store::PortfolioIndex>>
{
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


/// Remédio 1:1 do inline: o MESMO corpo em `touring run --lang python --code`.
/// Sem fusão — a razão histórica de excluir o heredoc da rajada era o R9
/// esmagar multi-linha; com remédio próprio ela caiu (aperto 29/08).
#[cfg(test)]
pub(super) fn python_inline_remedy(cmd: &str) -> String {
    python_inline_remedy_via(cmd, &g10_route::Route::Sandboxed)
}

/// O remédio 1:1 do inline sobre a ROTA que executa o corpo (G10ROTA, 30.4.97): a Sandboxed é o
/// `touring run` simples; a Trusted leva a elevação POR COMANDO (`TOURING_TRUSTED_OK=1` no próprio
/// comando) e a flag `--allow-forbidden`. Para a Sandboxed a saída é byte-idêntica à de sempre.
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


/// A rota que executa o corpo do python-inline negado pelo G10: a pergunta é do EXECUTOR
/// (`g10_route`), não do gate. Fora de `python-inline` o remédio é o R9 em bash — sem corpo
/// python a inspecionar, a rota é a de sempre.
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


/// A abertura do remédio do G10: a de sempre na rota Sandboxed; nas demais, a frase que explica
/// por que a rota difere — ANTES do comando, para que quem copia o comando já tenha lido o porquê.
pub(super) fn g10_lead(rota: &g10_route::Route, project_root: &Path) -> String {
    let explicacao = rota.explanation(project_root);
    if explicacao.is_empty() {
        " — a rajada desenrolada JÁ é o programa; rode-o de uma vez:".to_string()
    } else {
        format!(" — a rajada desenrolada JÁ é o programa.{explicacao} Rode-o de uma vez:")
    }
}


/// O deny do G10 quando NENHUMA rota de `touring run` executa o corpo: diz isso, cita o destino ou
/// a recusa do executor, e nomeia a saída que funciona (o bypass por-comando, nativo) como a
/// linha pronta do PRÓPRIO comando negado. Não há comando de `touring run` para colar — nomear um
/// que falha foi o defeito.
pub(super) fn g10_unrunnable_deny(
    n: u32,
    class: &str,
    rota: &g10_route::Route,
    project_root: &Path,
    cmd: &str,
) -> String {
    format!(
        "[G10 exec-burst] {n}ª chamada seriada do executor `{class}` (0 `touring run` na janela \
         de {EXEC_BURST_WINDOW_SECS}s).{} {}",
        rota.explanation(project_root),
        bypass_next_step(cmd)
    )
}


/// A rajada de python-inline fundida: os CORPOS na ordem, um programa. O
/// contrato de orçamento é o do R9 — inteiro ou fora, jamais truncado; o
/// segundo elemento conta os omitidos.
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


/// A rota do script `path` (já em disco): lê o CONTEÚDO — é ele que o X6 julga. Ilegível, vazio ou
/// grande demais para o gate inspecionar → a rota de sempre (sem evidência não há o que mudar).
pub(super) fn g10_script_route(project_root: &Path, lang: &str, path: &str) -> g10_route::Route {
    const TETO: u64 = 256 * 1024;
    let p = Path::new(path);
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        project_root.join(p)
    };
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


/// S4 — o programa R9 (exec-agregado): as N chamadas REAIS, verbatim e na
/// mesma ordem, cada uma seguida do veredito `[ok]`/`[FALHOU N]` (a execução
/// continua nas falhas), RESUMO final — a íntegra fica no spill quando estoura.
///
/// Em BASH, não python (30/08/2026): o template anterior era `--lang python`
/// com `import subprocess` — a capability que o X6 negava sob Sandboxed, e o
/// deny do G10 emitia uma rota que o gate seguinte barrava (medido pela peer
/// `analise-a2`). No mesmo dia Gabriel estendeu o waiver subprocess-only a
/// toda linguagem, então o template python voltaria a executar — mas bash
/// segue sendo a forma certa AQUI: os comandos viajam verbatim, 1:1, sem a
/// camada de reescrita em subprocess que o modelo teria de conferir. Rede e
/// padrões destrutivos seguem hard-deny em toda lang. `--timeout-ms` viaja
/// explícito (E4): N ferramentas reais não cabem no default de 30s.
/// Os comandos de uma rajada que JÁ rodaram, como linhas de comentário (1ª linha, até
/// 120 chars). Fonte única do S212 para o G10 e o G1: repeti-los repetia o efeito
/// (uma escrita de memória, um commit) — eles são contexto, nunca passo.
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
            continue; // inteiro ou fora, jamais truncado (mesma regra do R9 antigo)
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


/// S6 (26/08) — intent de prior-art derivado da rajada real: a classe do
/// executor + os alvos recorrentes (BM25 precisa dos termos do TRABALHO,
/// não da forma da rajada). Máx 6 termos — intent longo dilui o ranking.
pub(super) fn exec_burst_intent(class: &str, cmds: &[String]) -> String {
    /// Componente de trabalho (dir ou pedaço de stem): ≥2 chars, não-dígito,
    /// dedup. Tokens de caminho INTEIROS não entram — `tests/test_1.py`
    /// diluiria o intent em termos que nenhum propósito contém (medido vivo:
    /// 6 termos → required_matches 3 → 2 matches → o portfolio calado com o
    /// artefato certo na prateleira).
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


/// S6 — o prior art mais próximo da rajada, instanciado: quando o portfólio
/// tem um programa que JÁ funcionou para este intent, a rota vem escrita E
/// vem de um artefato real (a diferença para o esqueleto R9, que é genérico
/// por construção). Fail-open em todas as direções: sem índice, sem entrada,
/// sem entry_point, ou entry_point com placeholder não-derivável → `None` e
/// o deny sai só com o R9 (o comportamento anterior, honesto).
pub(super) fn portfolio_remedy_for_burst(class: &str, cmds: &[String]) -> Option<String> {
    let index = portfolio_index()?;
    portfolio_remedy_for_burst_on(class, cmds, &index)
}


/// O corpo, com o índice por parâmetro — testes carregam do scratch via
/// `store::load_from`, sem `set_var` (ver `GateOverrides`).
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
        return None; // placeholder não-derivável: apresentar seria fabricar
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



// ── W3 (plano code-mode-total) — gates de MODO: G3 e G7 + telemetria G4/G5 ───


/// The Read window (1-based `offset`, `limit`) that covers the first occurrence of an Edit's
/// `old_string` with a little context — the lines the model must see to write the Edit right.
/// `None` when the file is unreadable, large, short, or does not contain `old_string` (the
/// reason then names the plain `Read`). Measured before the 27/10/2026 deploy: 589 of 607 Edits
/// after a PARTIAL Read succeeded, against 67 of 70 after a full one, so a window is a valid
/// Read for the harness.
pub(super) fn g3_read_window(project_root: &Path, fp: &str, old_string: &str) -> Option<(usize, usize)> {
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
pub(super) fn g3_advisory_text(project_root: &Path, fp: &str, n: u32, tool_input: &Value) -> String {
    let leitura = g3_read_call(project_root, fp, tool_input);
    format!(
        "[G3 edit-sem-read] Edit de {fp} sem Read recente (advisory {n}/1 — o 2º seguido nega). \
         Próximo passo: `{leitura}` antes do Edit — só a ferramenta Read do arquivo exato conta \
         (cat/sed/grep via Bash não); o Read zera o contador."
    )
}


/// The G3 deny. Measured before the 27/10/2026 deploy: in 37 of 40 denials the session had
/// already looked at the file through Bash only, and 9 of the 40 were answered with another
/// unread Edit — so the reason says what counts as a Read and what stays forbidden meanwhile.
pub(super) fn g3_deny_reason(project_root: &Path, fp: &str, n: u32, tool_input: &Value) -> String {
    let leitura = g3_read_call(project_root, fp, tool_input);
    format!(
        "[G3 edit-sem-read] {n}º Edit sem Read recente nesta sessão — os 11 erros \
         `edit_string_not_found` medidos nascem exatamente aqui. Próximo passo, exatamente: \
         `{leitura}`, e só então o Edit. Só a ferramenta Read do arquivo exato conta: \
         cat/sed/grep via Bash e o Read de OUTRO arquivo não registram {fp}. Proibido até lá: \
         Edit de {fp} ou de qualquer outro arquivo ainda não lido."
    )
}


/// V0e3 (09/10/2026) — o esqueleto R1 pelo CAMINHO REAL quando existe (medido
/// pela 6b: o deny citava `<r1_varredura_agregado.py>`, um placeholder, e o
/// advisory `r1_varredura_agregado.py`, um relativo inexistente — a recusa
/// apontava para um arquivo que não existia, D8). Espelha o `_r1_route()` do
/// loop_stop_guard.py (a parte Python, feita na mesma noite): sem o arquivo,
/// só a consulta à memória — nunca um placeholder.
pub(super) fn r1_skeleton_path() -> Option<std::path::PathBuf> {
    let p = std::env::var_os("HOME").map(std::path::PathBuf::from)?.join(
        "projects/touring/docs/plans/2026-08-24-code-mode-antecipa/evidence/repertorio/r1_varredura_agregado.py",
    );
    p.is_file().then_some(p)
}


/// A rota R1 para o alvo `fp`: o esqueleto real instanciado quando existe, a
/// query de memória quando não.
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


/// A rota de edição que o deny do G7 nomeia (D8: a frase só sai quando é verdadeira). O deny só
/// dispara com o arquivo inalterado desde a leitura registrada (`g7_refresh_if_changed` zera a
/// contagem quando ele muda), e o ledger do G3 foi alimentado por esta mesma leitura: o `Edit`
/// direto passa no G3 e o harness o aceita. Sem carimbo (arquivo não mensurável) a afirmação
/// «não mudou» não tem lastro — a frase não sai.
pub(super) fn g7_edit_route_note(fp: &str, measured: bool) -> String {
    if !measured {
        return String::new();
    }
    format!(
        " Para EDITAR {fp}: Edit direto — o arquivo não mudou desde a leitura registrada, então o \
         G3 e o harness a aceitam; se ele mudar, a releitura é liberada."
    )
}


/// Funde a rajada acumulada num corpo de programa executável.
///
/// Devolve `(corpo, omitidos)`. A regra é **comando inteiro ou nenhum**: um
/// comando que não cabe no orçamento é DESCARTADO, jamais truncado.
///
/// Origem (2026-08-25, observado ao vivo nesta função): cada comando entrava
/// com `cmd.chars().take(240)`, então uma rajada de greps longos produzia um
/// programa cortado no meio de um caminho (`crates/tou'`) — sintaticamente
/// plausível, com aspas equilibradas, e que **não roda**. Um comando truncado
/// é pior que um ausente: o ausente aparece na contagem que o remédio declara;
/// o truncado se disfarça de programa completo. A emenda do Gabriel (25/08) é
/// exatamente esta — a injeção tem de entregar o snippet que SUBSTITUI as N
/// chamadas, e um snippet que falha não substitui nada.
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


/// O orçamento de parede da chamada que a reescrita do G1 substitui (09/10/2026).
/// A reescrita herdava os 30 s padrão do `touring run`; a chamada original tinha o
/// do Bash — o `timeout` dela, ou 120 s — e morria aos 30 s (`exit_code -2`, medido
/// num laço de espera de 600 s). Em background o Bash não tem teto: vale o máximo
/// do `touring run`. A folga deixa o envelope voltar antes do teto externo.
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


/// O `updatedInput` de uma reescrita: a chamada original com SÓ o `command`
/// trocado (09/10/2026). O `updatedInput` SUBSTITUI o input — medido ao vivo: duas
/// chamadas com `run_in_background: true` e `timeout: 600000` reescritas pelo G2
/// voltaram "did not complete within its 120s timeout and was moved to the
/// background". Usado pelas três reescritas (G1, G2, G8).
pub(super) fn rewritten_input(tool_input: &Value, command: String) -> Value {
    let mut novo = tool_input.as_object().cloned().unwrap_or_default();
    novo.insert("command".to_string(), Value::String(command));
    Value::Object(novo)
}


/// Declara o orçamento de parede num programa `touring run --lang bash …` já
/// montado (o do G8 sai de [`loop_rewrite_candidate`], que não conhece a chamada).
pub(super) fn with_wall_budget(programa: String, wall_ms: u64) -> String {
    match programa.strip_prefix("touring run --lang bash ") {
        Some(resto) => format!("touring run --lang bash --timeout-ms {wall_ms} {resto}"),
        None => programa,
    }
}


/// O advisory do G1 quando a precisão viva o autodemove (S-2.3): só `additionalContext`,
/// e o comando segue NATIVO e intacto. O texto antigo («a rajada acumulada já é o
/// programa») soava como ordem de rodar o programa — repetindo o comando que acabara de
/// rodar. Agora o programa é declarado o FORMATO da próxima rajada, nunca um passo
/// pendente desta.
pub(super) fn g1_advisory_text(count: u32, class: &str, programa: &str) -> String {
    format!(
        "[G1 rajada — AUTODEMOVIDO a advisory: precisão viva abaixo de \
         {G1_DEMOTE_FLOOR}] {count}ª inspeção `{class}` desde o último disparo. \
         ESTE comando roda NATIVO e intacto — não o repita. O programa abaixo é \
         o FORMATO para a próxima rajada: junte as inspeções seguintes num só \
         `touring run`:\n  {programa}"
    )
}


/// A reescrita do G1 (V0e1). O `permissionDecisionReason` de um `allow` vai só para o
/// log de debug (code.claude.com/docs/en/hooks, "For "allow" and "defer", written to
/// the debug log only") — nem o modelo nem o usuário o veem. O modelo, que recebe a
/// saída do sandbox no lugar da do seu comando, precisa do `additionalContext` (que só
/// o `defer` ignora; teto de 10.000 chars) — sem ele a saída parecia "de outra
/// chamada" (medido 09/10).
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




/// N6 (2026-09-23) — o programa fundido do deny encontra a escada: quando um
/// bloco parecido JÁ existe, a razão o entrega (afordância por construção, D8),
/// e a rajada negada vira descoberta em vez de só reescrita. A régua media o
/// contrário: `ladder_enrolled` 390 × `ladder_reused` 10 — descoberta falha
/// no momento exato da ação. Read-only; `None` em qualquer erro (testável com
/// um banco em memória).
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


/// O wrapper de I/O: abre o memory.db do projeto somente-leitura e devolve ""
/// em qualquer falha — um deny jamais falha POR CAUSA da dica.
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

