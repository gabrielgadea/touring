//! What a command IS: the shell-aware classification the gates decide on.
//!
//! Wrappers (`time`, `sudo`, `env`…) are transparent, `cd` and sequences are walked step by
//! step, redirects are read after the CEG shell mask (quoted text and heredoc bodies are data),
//! and every classifier fails toward "not an inspection" — the loose direction.

use std::path::Path;
use std::sync::OnceLock;

use super::*;

/// True iff `command` starts an atomic content/file search that `ctx_execute`
/// could fold into one pass.
/// `VAR=valor` de prefixo (inclusive o bypass `TOURING_GATE_OK=1`): a classe é
/// do comando executado, não do ambiente que o precede. Predicado idêntico ao
/// que `scan_class_of` sempre usou — extraído para os 5 sítios dividirem.
pub(super) fn is_env_assignment(t: &str) -> bool {
    t.contains('=')
        && t.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
}


/// Wrapper de execução reconhecido pelo resolvedor: flags que tomam valor +
/// quantos posicionais o PRÓPRIO wrapper consome (a duração do `timeout`, a
/// máscara do `taskset`, a prioridade do `chrt`).
pub(super) struct WrapperSpec {
    value_flags: &'static [&'static str],
    positionals: usize,
}


/// S1 (26/08/2026) — os classificadores liam o 1º token: `time grep`,
/// `nice -n 5 find`, `sudo cat`, `env F=1 rg` escapavam de TODOS os gates e
/// nudges ao mesmo tempo (a cobertura nascia zero onde a classe nasce).
pub(super) fn wrapper_spec(verb: &str) -> Option<WrapperSpec> {
    Some(match verb {
        "env" => WrapperSpec {
            value_flags: &["-u", "--unset", "-C", "--chdir", "-S", "--split-string"],
            positionals: 0,
        },
        "time" => WrapperSpec {
            value_flags: &["-o", "--output", "-f", "--format"],
            positionals: 0,
        },
        "nice" => WrapperSpec {
            value_flags: &["-n", "--adjustment"],
            positionals: 0,
        },
        "ionice" => WrapperSpec {
            value_flags: &["-c", "-n", "-p", "-P"],
            positionals: 0,
        },
        "sudo" => WrapperSpec {
            value_flags: &[
                "-u", "-g", "-h", "-p", "-C", "-r", "-t", "-T", "-U", "-D", "-R", "--user",
                "--group", "--host", "--prompt", "--role", "--type", "--chdir", "--chroot",
            ],
            positionals: 0,
        },
        "command" | "builtin" => WrapperSpec {
            value_flags: &[],
            positionals: 0,
        },
        "timeout" => WrapperSpec {
            value_flags: &["-s", "--signal", "-k", "--kill-after"],
            positionals: 1,
        },
        "stdbuf" => WrapperSpec {
            value_flags: &["-i", "-o", "-e"],
            positionals: 0,
        },
        "taskset" => WrapperSpec {
            value_flags: &["-c", "--cpu-list"],
            positionals: 1,
        },
        "chrt" => WrapperSpec {
            value_flags: &[],
            positionals: 1,
        },
        _ => return None,
    })
}


/// Consome o wrapper em `toks[i]` (verbo + flags + posicionais dele) e devolve
/// o índice seguinte; `i` inalterado quando `toks[i]` não é wrapper. Flag
/// idêntica à da tabela consome o valor no token seguinte (`-n 5`); a forma
/// colada (`-n5`) e a longa com `=` (`--signal=KILL`) consomem um só token.
pub(super) fn skip_wrapper(toks: &[&str], i: usize) -> usize {
    let Some(spec) = toks.get(i).and_then(|v| wrapper_spec(v)) else {
        return i;
    };
    let mut j = i + 1;
    let mut positionals = spec.positionals;
    while j < toks.len() {
        let t = toks[j];
        if is_env_assignment(t) {
            j += 1; // `sudo FOO=1 cmd` / as atribuições do próprio `env`
            continue;
        }
        if t.starts_with('-') && t != "-" {
            j += if spec.value_flags.contains(&t) { 2 } else { 1 };
            continue;
        }
        if positionals > 0 {
            positionals -= 1;
            j += 1;
            continue;
        }
        break; // o verbo real
    }
    j
}


/// Tokens a partir do VERBO REAL: prefixos `VAR=valor` e wrappers de execução
/// (`env`/`time`/`nice`/`sudo`/`command`/`builtin`/`timeout`/`stdbuf`/
/// `ionice`/`taskset`/`chrt`, encadeados em qualquer ordem) removidos. Vazio
/// quando o comando é só invólucro (`env` puro imprime o ambiente). Um único
/// resolvedor para todos os classificadores — consertar um sítio só mascarou
/// o defeito uma vez (memória `definer-module-cinco-sitios`); não duas.
pub(super) fn resolved_tokens<'a, 'b>(toks: &'b [&'a str]) -> &'b [&'a str] {
    let mut i = 0;
    while i < toks.len() {
        if is_env_assignment(toks[i]) {
            i += 1;
            continue;
        }
        let j = skip_wrapper(toks, i);
        if j == i {
            break;
        }
        i = j;
    }
    &toks[i..]
}


/// Segmentos de comando separados por operadores de sequência (`&&`, `||`,
/// `;`, nova linha). Ingênuo quanto a aspas (`grep 'a;b'` corta dentro da
/// aspa) — seguro para CLASSIFICAÇÃO: o verbo do segmento não muda com o
/// corte, que é o único uso (S1, 26/08).
pub(super) fn command_segments(cmd: &str) -> Vec<&str> {
    cmd.split(['\n', ';'])
        .flat_map(|part| part.split("&&"))
        .flat_map(|part| part.split("||"))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}


/// Tokens efetivos: segmenta por operadores de sequência, pula segmentos
/// `cd <dir>` (prefixo de navegação — `cd /x && grep foo` é `grep`), e
/// resolve wrappers no primeiro segmento que não é só navegação. Tudo `cd`
/// (ou nada) devolve vazio. É o resolved_tokens + a lacuna que a estratégia
/// S1 nomeou (`cd <dir> (&&|;\n)`) e a primeira entrega deixou passar.
pub(crate) fn effective_tokens(cmd: &str) -> Vec<&str> {
    for seg in command_segments(cmd) {
        let toks: Vec<&str> = seg.split_whitespace().collect();
        let rest = resolved_tokens(&toks);
        match rest.first() {
            Some(&"cd") => continue, // prefixo de navegação — o próximo decide
            // Segmento que resolve para NADA (`P=x` sozinho, `env` puro): o
            // próximo segmento decide. Sem este braço, `P=x\npython3 f.py`
            // devolvia vazio e TODA classificação morria no assignment —
            // provado ao vivo (29/08): 12 execuções python atrás de
            // `VAR=...\n` e zero denies do G10 no turno de 60 do `analise`.
            None => continue,
            _ => return rest.to_vec(),
        }
    }
    Vec::new()
}


/// The file an inline `sed -i`/`awk -i inplace`/`perl -pi` edit targets — the last
/// path-like token (skips flags and the substitution script). Lets the Edit tool /
/// ast-meta nudge carry the real path instead of `<file>`. `None` when not derivable.
/// N3a (26/08/2026) — escrita cega inline: `sed -i`/`sed --in-place`/
/// `awk -i inplace`/`perl -pi` EM POSIÇÃO DE COMANDO (início, `|`, `;`,
/// `&&`/`&`, `$(`, nova linha). A âncora em posição de comando separa a
/// invocação do texto — `echo "rode sed -i aqui"` é prosa, não escrita.
/// Lacuna conhecida (documentada, fora do escopo N3a): `find -exec sed -i`
/// e `xargs sed -i` não estão em posição de comando.
pub(crate) fn is_inline_blind_edit(cmd: &str) -> bool {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(
            r"(?:^|[|;&\n]|\$\()\s*(?:sed\s+-[a-zA-Z]*i(?:\.\S*)?(?:\s|$)|sed\s+--in-place\b|awk\s+-i\s+inplace\b|perl\s+-pi\b)",
        )
        .expect("G9 regex compila")
    })
    .is_match(cmd)
}


/// S4 (26/08) — classe do executor homogêneo (R4): `python3`/
/// `.venv/bin/python3`/`pytest` & cia — a rajada desenrolada que nenhum gate
/// nomeava (75 chamadas seriadas na sessão dc87e4e0, 5,8% adoção).
/// `python3 -c` fica de fora (o advisory CEG é o dono do inline). Mutação
/// marcada fica de fora por construção (P2.3): redirect de shell,
/// instaladores, git, rm — o marcador é a calibração, não prova de pureza
/// (a limitação é documentada, como no G8).
/// A visão do comando que o filtro de mutação inspeciona: corpos de heredoc
/// removidos (são DADO alimentando stdin, não comando) e redirects inofensivos
/// de FD/descarte neutralizados (`2>&1`, `2>/dev/null`, `>/dev/null`, …) — eles
/// não gravam arquivo nenhum. O `>` que sobrar é redirect real (escrita), e o
/// comando continua fora da rajada exec, como os testes exigem. Furo provado
/// ao vivo (29/08): as execuções python do turno de 60 do `analise` carregavam
/// `2>&1`/`2>/dev/null` e o `contains(">")` sobre o blob inteiro anulava a
/// classe — o G10 ficou mudo o turno todo ("verificador usa menos que o
/// extrator", mais uma encarnação).
pub(super) fn mutation_scan_view(cmd: &str) -> String {
    // 1) corta corpos de heredoc: da linha após `<<MARKER` até a linha MARKER.
    let mut kept: Vec<&str> = Vec::new();
    let mut skip_until: Option<String> = None;
    for line in cmd.lines() {
        if let Some(marker) = &skip_until {
            if line.trim() == marker.as_str() {
                skip_until = None;
            }
            continue;
        }
        kept.push(line);
        if let Some(pos) = line.find("<<")
            && !line[pos..].starts_with("<<<")
        {
            let raw = line[pos + 2..].trim_start_matches('-').trim();
            let marker = raw
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_matches(|c| c == '\'' || c == '"');
            if !marker.is_empty() {
                skip_until = Some(marker.to_string());
            }
        }
    }
    let mut s = kept.join("\n");
    // 2) neutraliza redirects que não gravam nada (ordem: os prefixados por FD
    // antes do genérico, senão o replace parcial deixa o dígito para trás).
    for inofensivo in [
        "2>&1",
        "1>&2",
        "2>/dev/null",
        "2> /dev/null",
        "&>/dev/null",
        "&> /dev/null",
        ">/dev/null",
        "> /dev/null",
    ] {
        s = s.replace(inofensivo, " ");
    }
    s
}


pub(super) fn exec_class_of(cmd: &str) -> Option<&'static str> {
    const MUTATING: &[&str] = &[
        ">",
        "| tee",
        "pip install",
        "setup.py install",
        "rm ",
        "mv ",
        "cp ",
        "git ",
        "kill",
        "chmod",
        "chown",
        "curl",
        "wget",
        "ssh",
        "docker",
        "systemctl",
        "touch ",
        "mkdir",
    ];
    let scan = mutation_scan_view(cmd);
    if MUTATING.iter().any(|m| scan.contains(m)) {
        return None;
    }
    let rest = effective_tokens(cmd);
    let verb = rest.first()?;
    let base = verb.rsplit('/').next()?;
    if base == "python" || base == "python3" || base.starts_with("python3.") {
        // Inline (`-c` / `- <<EOF`) entrou na rajada em 29/08 (aperto de
        // Gabriel) como classe PRÓPRIA: a exclusão histórica existia porque o
        // R9 esmagaria heredoc multi-linha no remédio — o python-inline tem
        // remédio 1:1 (o corpo verbatim em `touring run --lang python`), então
        // a razão caiu. Os 5 heredocs do turno de 60 do analise estavam fora.
        if matches!(rest.get(1), Some(&"-c") | Some(&"-")) {
            return Some("python-inline");
        }
        return Some("python");
    }
    if base.starts_with("pytest") {
        return Some("pytest");
    }
    None
}


pub(super) fn is_script_path(p: &str) -> bool {
    let p = p.trim_matches(|c| c == '\'' || c == '"');
    p.ends_with(".py") || p.ends_with(".sh")
}


pub(super) fn clean_script_path(p: &str) -> String {
    p.trim_matches(|c| c == '\'' || c == '"').to_string()
}


/// O path de script (.py/.sh) que este comando ESCREVE via `cat >`/`tee`.
/// Varre a VIEW (corpos de heredoc removidos): um `cat > y.py` dentro do
/// corpo é dado, não escrita.
pub(super) fn script_write_target(cmd: &str) -> Option<String> {
    let view = mutation_scan_view(cmd);
    for seg in command_segments(&view) {
        let toks: Vec<&str> = seg.split_whitespace().collect();
        let rest = resolved_tokens(&toks);
        match rest.first() {
            Some(&"cat") => {
                let mut it = rest[1..].iter().copied();
                while let Some(t) = it.next() {
                    if t == ">" || t == ">>" {
                        if let Some(p) = it.next()
                            && is_script_path(p)
                        {
                            return Some(clean_script_path(p));
                        }
                    } else if let Some(p) = t.strip_prefix(">>").or_else(|| t.strip_prefix('>'))
                        && !p.is_empty()
                        && is_script_path(p)
                    {
                        return Some(clean_script_path(p));
                    }
                }
            }
            Some(&"tee") => {
                if let Some(p) = rest[1..].iter().find(|t| !t.starts_with('-'))
                    && is_script_path(p)
                {
                    return Some(clean_script_path(p));
                }
            }
            _ => {}
        }
    }
    None
}


/// Os paths de script que este comando EXECUTA diretamente (python/bash/sh),
/// em QUALQUER segmento — o padrão real do `analise` (24 dos 60) escrevia E
/// executava no MESMO tool_use multi-linha (`cat > x.py <<EOF…EOF` +
/// `python3 x.py`), e um detector que só olha o primeiro verbo efetivo vê o
/// `cat` e nunca o run. Varre a VIEW (heredoc fora); segmento com redirect
/// real de saída é pulado — o remédio `--file` não reproduziria a escrita.
pub(super) fn script_run_targets(cmd: &str) -> Vec<String> {
    let view = mutation_scan_view(cmd);
    let mut out: Vec<String> = Vec::new();
    for seg in command_segments(&view) {
        if seg.contains('>') {
            continue;
        }
        let toks: Vec<&str> = seg.split_whitespace().collect();
        let rest = resolved_tokens(&toks);
        let Some(verb) = rest.first() else { continue };
        let base = verb.rsplit('/').next().unwrap_or(verb);
        let interpretador = base == "bash"
            || base == "sh"
            || base == "python"
            || base == "python3"
            || base.starts_with("python3.");
        if !interpretador {
            continue;
        }
        if let Some(arg) = rest[1..].iter().find(|t| !t.starts_with('-'))
            && is_script_path(arg)
            && !out.iter().any(|p| p == &clean_script_path(arg))
        {
            out.push(clean_script_path(arg));
        }
    }
    out
}


/// O corpo do programa inline (`python3 -c '<code>'` ou `python3 - <<'M'`),
/// para o remédio 1:1 da classe `python-inline`.
pub(super) fn python_inline_body(cmd: &str) -> Option<String> {
    // heredoc: corpo entre a linha pós-`<<MARKER` e a linha MARKER
    if let Some(pos) = cmd.find("<<")
        && !cmd[pos..].starts_with("<<<")
    {
        let first_line_end = pos + cmd[pos..].find('\n')?;
        let marker = cmd[pos + 2..first_line_end]
            .trim_start_matches('-')
            .split_whitespace()
            .next()?
            .trim_matches(|c| c == '\'' || c == '"')
            .to_string();
        let body: Vec<&str> = cmd[first_line_end + 1..]
            .lines()
            .take_while(|l| l.trim() != marker)
            .collect();
        return Some(body.join("\n"));
    }
    // -c: o literal após a flag (heurística: o par de aspas externo)
    let idx = cmd.find(" -c ")?;
    let rest = cmd[idx + 4..].trim();
    let quote = rest.chars().next().filter(|c| *c == '\'' || *c == '"')?;
    let inner = &rest[1..];
    let end = inner.rfind(quote)?;
    Some(inner[..end].to_string())
}


/// A classe do python-inline READ-ONLY dentro da rajada de inspeção (aperto
/// 29/08, ordem de Gabriel: *"read-only cai na rajada de inspeção"*). Distinta
/// no ledger — a chave é por classe — e fora de `CODE_MODE_COLLAPSED_CLASSES`,
/// que documenta as classes que `scan_class_of` emite; esta vem do CORPO do
/// programa, não do nome do comando.
pub(super) const PY_INLINE_INSPECT_CLASS: &str = "python-inline";


/// Indícios de que um corpo python NÃO é leitura pura — deny-list
/// conservadora: escrita em FS, modos mutantes de `open`, subprocesso, rede,
/// import/exec dinâmico e DB (abrir SQLite muda estado de lock mesmo lendo).
/// Falso "escritor" (a marca aparece numa string inocente) só devolve o
/// comando ao G10, mais frouxo — a direção segura; o inverso rotearia um
/// escritor para o deny de inspeção.
pub(super) const PY_INLINE_WRITER_MARKS: &[&str] = &[
    ".write(",
    ".writelines(",
    "write_text(",
    "write_bytes(",
    ".unlink(",
    ".touch(",
    ".chmod(",
    ".rename(",
    ".rmdir(",
    "os.remove",
    "os.rename",
    "os.replace",
    "os.rmdir",
    "os.removedirs",
    "os.mkdir",
    "os.makedirs",
    "os.symlink",
    "os.link",
    "os.truncate",
    "os.putenv",
    "shutil.",
    "tempfile.",
    "mode='w",
    "mode=\"w",
    "mode='a",
    "mode=\"a",
    "mode='x",
    "mode=\"x",
    "subprocess",
    "os.system",
    "os.popen",
    "os.exec",
    "os.spawn",
    "pty.",
    "socket",
    "urllib",
    "requests",
    "http.client",
    "httpx",
    "ftplib",
    "smtplib",
    "exec(",
    "eval(",
    "__import__",
    "importlib",
    "sqlite3",
    "dbm.",
    "shelve",
];


/// `Some(PY_INLINE_INSPECT_CLASS)` quando o comando é um python-inline cujo
/// corpo é leitura pura — e portanto INSPEÇÃO, sujeita à rajada 2ª/300s. Um
/// corpo inextraível ou com qualquer marca de escrita fica de fora (G10 é o
/// backstop, 5ª/600s).
pub(super) fn python_inline_readonly_class(cmd: &str) -> Option<&'static str> {
    if exec_class_of(cmd) != Some("python-inline") {
        return None;
    }
    let body = python_inline_body(cmd)?;
    if body.is_empty()
        || PY_INLINE_WRITER_MARKS.iter().any(|m| body.contains(m))
        || py_open_mutating_mode(&body)
    {
        return None;
    }
    Some(PY_INLINE_INSPECT_CLASS)
}


/// `open(...)` com 2º argumento LITERAL contendo `w`/`a`/`x`/`+` — o modo
/// mutante. Um `open(f)`/`open(f, encoding=…)` é leitura e passa; a marca por
/// aspas soltas (`'w'`) foi tentada e mordeu o próprio teste (`re.findall(
/// "x", …)` classificava escritor). Falso positivo residual (uma vírgula de
/// outra chamada seguida de literal com w/a/x) só devolve o comando ao G10.
pub(super) fn py_open_mutating_mode(body: &str) -> bool {
    let mut rest = body;
    while let Some(i) = rest.find("open(") {
        let after = &rest[i + 5..];
        if let Some(c) = after.find(',') {
            let tail = after[c + 1..].trim_start();
            if let Some(q) = tail.chars().next().filter(|ch| *ch == '\'' || *ch == '"') {
                let inner = &tail[1..];
                if let Some(end) = inner.find(q)
                    && inner[..end].contains(['w', 'a', 'x', '+'])
                {
                    return true;
                }
            }
        }
        rest = after;
    }
    false
}


/// As classes de inspeção que o ledger da rajada acompanha — TODAS as que
/// [`scan_class_of`] reconhece.
///
/// A lista fixa que existia aqui (`grep`/`cat`/`find`) errava nos DOIS sentidos,
/// e a mesma medição mostra as duas pontas: `find` é 56,2% isolada e não tem
/// UMA rajada ≥3 em 115 transcripts — negá-la por classe era fricção pura; já
/// `sed-n` (408 chamadas, 81,4% do volume em rajada) e `ls` (338, 71,3%)
/// passavam sempre, e eram o maior volume fan-out não capturado. O predicado de
/// rajada dispensa a lista: ele discrimina pelo que a classe FAZ nesta janela,
/// não pelo nome dela.
pub(super) const CODE_MODE_COLLAPSED_CLASSES: &[&str] = &["grep", "cat", "find", "ls", "wc", "sed-n"];


pub(crate) fn inline_edit_target(command: &str) -> Option<String> {
    command
        .split_whitespace()
        .map(|t| t.trim_matches(|c| c == '"' || c == '\''))
        .filter(|t| !t.starts_with('-'))
        .rev()
        .find(|t| t.contains('/') || (t.contains('.') && !t.contains('*')))
        .map(std::string::ToString::to_string)
}


/// W1 S-1.2 — comandos de estado vivo: chamadas idênticas legitimamente
/// retornam resultados diferentes (saúde, daemon, working tree, contadores).
/// Allowlist EXPLÍCITA, não heurística; ampliar só com dado (S-7.2).
pub(super) const G6_LIVE_STATE_ALLOWLIST: &[&str] = &[
    "touring doctor",
    "touring status",
    "touring daemon-ctl",
    "touring gate-metrics",
    "touring kpi",
    "git status",
    "git diff",
];


pub(super) fn g6_allowlisted(cmd: &str) -> bool {
    let trimmed = cmd.trim_start();
    G6_LIVE_STATE_ALLOWLIST
        .iter()
        .any(|p| trimmed.starts_with(p))
}


/// S213 — whether `cmd` APPENDS to a file (`>>` or `tee -a`/`--append`), read after the CEG
/// shell mask so a `>>` inside quotes or a heredoc body is data, not a redirect.
pub(super) fn appends_to_a_file(cmd: &str) -> bool {
    let masked = touring_ceg::gateway::gate::mask_shell_data(cmd);
    masked.contains(">>") || masked.contains("tee -a") || masked.contains("tee --append")
}


/// W2 S-2.1 — classe da inspeção atômica (a taxonomia da simulação). `None`
/// para comandos que não são inspeção — a rajada só conta o que inspeciona.
pub(crate) fn scan_class_of(cmd: &str) -> Option<&'static str> {
    // Verbo REAL pós-segmentos/prefixos-cd/wrappers (S1, 26/08): `time grep`
    // é `grep`, `cd /x && cat f` é `cat` — antes o 1º token decidia, e
    // `time`/`nice`/`sudo`/`env`/`cd` anulavam a classe para os 4 gates que
    // consomem este classificador de uma vez só.
    // S213: an APPENDING command is never inspection — the burst remedy replays the
    // commands it fuses, and an append replayed duplicates what it wrote (observed live:
    // `cat t.rs >> src/x.rs && cargo test`). An overwrite redirect stays inspection (P2.3):
    // replaying it is idempotent. Read through the CEG shell mask — quoted/heredoc is data.
    if appends_to_a_file(cmd) {
        return None;
    }
    // S213b (26/09/2026): a SEQUENCE is inspection only if every step inspects — the
    // class came from the first real verb alone, so `grep x f; cargo clippy …` was
    // `grep` and the remedy replayed a 20-minute clippy (observed live); a `touring
    // memory store` in its place would have been a replayed write. Navigation,
    // assignments and `echo`-style separators are neutral; the class is the first
    // reading step's.
    let mut class = None;
    for seg in command_segments(cmd) {
        let toks: Vec<&str> = seg.split_whitespace().collect();
        let rest = resolved_tokens(&toks);
        match rest.first() {
            None => {}
            Some(verb) if NEUTRAL_SEQUENCE_STEPS.contains(verb) => {}
            Some(_) => {
                let step = step_class(rest)?;
                class.get_or_insert(step);
            }
        }
    }
    class
}


/// Steps that neither inspect nor do work — they never break an inspection sequence.
pub(super) const NEUTRAL_SEQUENCE_STEPS: &[&str] = &["cd", "echo", "printf", "pwd", "true"];


/// The inspection class of ONE step (its wrapper-resolved tokens), or `None` when the
/// step does something other than inspect.
pub(super) fn step_class(rest: &[&str]) -> Option<&'static str> {
    let first = *rest.first()?;
    match first {
        "grep" | "rg" => Some("grep"),
        // P2.3 (calibração 25/08, 1.312 chamadas reais): `cat > f`/`cat >> f`
        // (heredoc de escrita) é T4 — escrita — não inspeção. O prefixo sozinho
        // negava 27 escritas (20,6% do que a matriz pegaria) sob o modo `code`,
        // que nega na 1ª chamada. Se o primeiro operando não-flag já é um
        // redirect, não há arquivo de leitura: não é classe `cat`.
        "cat" | "head" | "tail" | "less" => {
            // O primeiro operando REAL decide: flags (`-n`) — e o argumento
            // das que tomam valor (`-n 5`, `-c +3`) — são transparentes. Se o
            // que sobra já começa com `>`, não há arquivo de leitura.
            let mut it = rest[1..].iter().copied().peekable();
            let mut primeiro_operando: Option<&str> = None;
            while let Some(t) = it.next() {
                if t == "-n" || t == "-c" {
                    it.next(); // o argumento da flag, não um operando
                    continue;
                }
                if t.starts_with('-') {
                    continue;
                }
                primeiro_operando = Some(t);
                break;
            }
            match primeiro_operando {
                Some(op) if op.starts_with('>') => None,
                _ => Some("cat"),
            }
        }
        "find" | "fd" => Some("find"),
        "ls" => Some("ls"),
        "wc" => Some("wc"),
        // `-n` do sed, não do invólucro: `nice -n 5 sed 5p f` NÃO é sed-n
        // (com o verbo resolvido, o `contains` no comando inteiro herdaria o
        // `-n` do `nice` — o bug latente que a resolução expõe).
        "sed" if rest[1..].contains(&"-n") => Some("sed-n"),
        _ => None,
    }
}


/// O carimbo de `fp` agora; `None` quando o arquivo não existe ou não se deixa medir (o G7 então
/// segue contando como sempre contou — sem medição não há o que afirmar).
pub(super) fn file_stamp(project_root: &Path, fp: &str) -> Option<FileStamp> {
    let path = Path::new(fp);
    let meta = if path.is_absolute() {
        std::fs::metadata(path)
    } else {
        std::fs::metadata(project_root.join(path))
    }
    .ok()?;
    let mtime_ns = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some(FileStamp {
        mtime_ns,
        len: meta.len(),
    })
}


/// V0e3 (09/10/2026) — os fragmentos de path de um comando: quebra em todo
/// char que não é de path e fica com os fragmentos que contêm `/`. O reset do
/// G7 nunca funcionava no caso real porque o `trim_matches` das pontas cobria
/// só `'\"[]{},` — um path citado DENTRO de código (`Path("/…")`, o run
/// 1791491632099 da 6b) não casava a chave gravada e a leitura seguinte era
/// negada com a promessa falsa na mensagem (D8).
pub(super) fn path_fragments(cmd: &str) -> impl Iterator<Item = &str> {
    cmd.split(|c: char| !(c.is_alphanumeric() || matches!(c, '.' | '/' | '_' | '-' | '~')))
        .filter(|frag| frag.len() > 1 && frag.contains('/'))
}


/// W6 S-6.3 — linha ADICIONADA de comentário com modal contrafactual e nenhum
/// run_id na vizinhança: a afirmação nunca exercitada (o pior erro de 24/08).
pub(super) fn counterfactual_comment(added: &str) -> bool {
    const MODAIS: &[&str] = &["seria ", "quebraria", "impediria", "faria com que"];
    added.lines().any(|l| {
        let t = l.trim_start();
        let comentario = t.starts_with("//") || t.starts_with('#') || t.starts_with('*');
        comentario && MODAIS.iter().any(|m| t.contains(m)) && !t.contains("run-")
    })
}


/// W2 S-2.1 — o gate de rajada. `Some(resposta)` curto-circuita; `None` deixa
/// o fluxo (inclusive o advisory legado da 3ª busca) seguir.
/// Fração de tokens que dois comandos compartilham (Jaccard sobre o multiconjunto
/// de tokens), em `[0.0, 1.0]`.
///
/// Calibração proposta por Gabriel (26/08/2026): *"se o comando repetir pelo
/// menos 50% do comando anterior"*. Preenche um buraco real entre os gates —
/// o G6 pega só o byte-idêntico, o G7 só a re-inspeção do MESMO arquivo, e o G1
/// só ao acumular N da mesma classe. `grep X a.rs` seguido de `grep X b.rs` é a
/// assinatura canônica do fan-out serial e não era pego por nenhum deles.
///
/// Jaccard sobre tokens, e não prefixo comum: `sed -n 1,20p f` vs
/// `sed -n 40,60p f` compartilham quase tudo mas divergem cedo no texto, e um
/// prefixo os julgaria distintos. A ordem também não deve importar — o que
/// interessa é quanto do trabalho se repete.
pub(crate) fn command_similarity(a: &str, b: &str) -> f64 {
    let toks = |c: &str| -> std::collections::BTreeSet<String> {
        c.split_whitespace().map(str::to_string).collect()
    };
    let (ta, tb) = (toks(a), toks(b));
    if ta.is_empty() || tb.is_empty() {
        return 0.0;
    }
    let inter = ta.intersection(&tb).count() as f64;
    let uniao = ta.union(&tb).count() as f64;
    inter / uniao
}


/// G8 — o laço de inspeção pura vira UMA varredura no sandbox.
///
/// Devolve o comando reescrito, ou `None` quando o laço não é candidato. Puro
/// (sem I/O) para ser exercitável em unit test — o predicado que decide precisa
/// ser mais fácil de testar do que de contornar.
///
/// Entra: `for`/`while` cujo corpo só inspeciona (grep/cat/head/tail/ls/find/
/// echo/wc/awk/sed/jq/stat/basename). Fica de fora, por construção, tudo que
/// tem efeito (kill, git, cargo, rm, mv, install, deploy), redireciona para
/// arquivo, usa heredoc (o corpo é dado, não programa) ou já é code mode.
pub(crate) fn loop_rewrite_candidate(cmd: &str) -> Option<String> {
    if std::env::var("TOURING_G8_REWRITE_DISABLED").map(|v| v == "1") == Ok(true) {
        return None;
    }
    let trimmed = cmd.trim();
    // Já é code mode / master CLI — nada a converter.
    if trimmed.contains("touring run") || trimmed.contains("touring exec") {
        return None;
    }
    // Heredoc: o corpo é DADO (mesma razão do G2), e o quoting não sobrevive.
    if trimmed.contains("<<") {
        return None;
    }
    // Aspas simples desbalanceadas quebrariam o `--code '...'`.
    if !trimmed.matches('\'').count().is_multiple_of(2) {
        return None;
    }
    // Precisa de um laço de verdade — `for x in` / `while `.
    let has_loop = trimmed.split(['\n', ';', '|', '&']).any(|part| {
        let p = part.trim_start();
        p.starts_with("for ") && p.contains(" in ") || p.starts_with("while ")
    });
    if !has_loop {
        return None;
    }
    // Qualquer verbo com efeito colateral tira o comando do escopo.
    const EFFECTFUL: &[&str] = &[
        "kill",
        "pkill",
        "git ",
        "cargo",
        "rm ",
        "mv ",
        "cp ",
        "install",
        "update-touring",
        "npm",
        "make ",
        "chmod",
        "chown",
        "mkdir",
        "touch ",
        "tee ",
        "curl",
        "wget",
        "ssh",
        "docker",
        "systemctl",
        "sudo",
        "python3 -c",
        "python3 -m",
        "pip",
        ">>",
        "> /",
    ];
    if EFFECTFUL.iter().any(|verb| trimmed.contains(verb)) {
        return None;
    }
    // Limite de tamanho: acima disso o comando reescrito polui mais do que ajuda.
    if trimmed.len() > 1200 {
        return None;
    }
    let escaped = trimmed.replace('\'', r"'\''");
    Some(format!("touring run --lang bash --code '{escaped}'"))
}


/// V0e5 (09/10/2026) — entries of ANOTHER process's `/proc/<pid>/` that the sandbox
/// cannot read while a native shell can: guarded by the kernel's ptrace access mode
/// (proc(5)), not by landlock — `/proc` itself is a read root. Measured on 30.4.94
/// against a live daemon, sandbox × native; listed only where the two DIVERGE
/// (`stack`, `syscall`, `pagemap`, `mem` fail on both sides, so they say nothing;
/// `cmdline`, `status`, `stat`, `comm`, `limits` and the `fd/` listing read on both).
pub(super) const PTRACE_GUARDED_PROC_ENTRIES: &[&str] = &[
    "environ",
    "maps",
    "smaps",
    "smaps_rollup",
    "auxv",
    "cwd",
    "exe",
    "root",
    "io",
    "map_files",
];

/// Directories whose LISTING reads in the sandbox but whose entries (`fd/3`) do not.
pub(super) const PTRACE_GUARDED_PROC_DIR_ENTRIES: &[&str] = &["fd", "fdinfo"];


/// The `/proc/<pid>/<entry>` token, when `<pid>` is another process (a literal, `$p`,
/// `${pid}` or a glob — anything but `self`/`thread-self`) and `<entry>` is guarded.
pub(super) fn ptrace_guarded_proc_entry(tok: &str) -> Option<std::path::PathBuf> {
    let (pid, tail) = tok.strip_prefix("/proc/")?.split_once('/')?;
    if pid == "self" || pid == "thread-self" {
        return None;
    }
    let mut parts = tail.split('/');
    let entry = parts.next().unwrap_or("");
    let below = parts.next().is_some_and(|p| !p.is_empty());
    let guarded = PTRACE_GUARDED_PROC_ENTRIES.contains(&entry)
        || (below && PTRACE_GUARDED_PROC_DIR_ENTRIES.contains(&entry));
    guarded.then(|| std::path::PathBuf::from(tok))
}


/// S24 (plan 2026-09-25) — the first path `cmd` names that a sandboxed program cannot
/// reach, judged by the CEG's own landlock table
/// ([`touring_ceg::gateway::sandbox_executor::sandbox_can_reach`] — one predicate, D8).
///
/// A command naming such a path is never rewritten into `touring run` nor denied toward
/// it (G1/G8/G10/T3-B): the route would not execute. Measured 25/09: the gate rewrote a
/// `.touring/bin/touring decompose …` loop into a run that failed with "Permission
/// denied" and still reported `success: true`.
pub(super) fn unreachable_in_sandbox(project_root: &Path, cmd: &str) -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    for raw in cmd.split(|c: char| c.is_whitespace() || ";|&()<>`".contains(c)) {
        let tok = raw.trim_matches(|c: char| "'\"{}[],".contains(c));
        let tok = tok.rsplit_once('=').map_or(tok, |(_, v)| v);
        if let Some(guarded) = ptrace_guarded_proc_entry(tok) {
            return Some(guarded);
        }
        if tok.is_empty() || tok.contains("://") || !(tok.contains('/') || tok.starts_with('~')) {
            continue;
        }
        let expanded = match (
            tok.strip_prefix("~/")
                .or_else(|| tok.strip_prefix("$HOME/")),
            &home,
        ) {
            (Some(rest), Some(h)) => h.join(rest),
            _ => std::path::PathBuf::from(tok),
        };
        let path = if expanded.is_absolute() {
            expanded
        } else {
            project_root.join(expanded)
        };
        // Only an EXISTING path is evidence: a missing one fails inside or outside the
        // sandbox alike, so it says nothing about capability (keeps `/x/a.rs` fixtures fusible).
        if path.exists() && !touring_ceg::gateway::sandbox_executor::sandbox_can_reach(&path) {
            return Some(path);
        }
    }
    None
}

