//! VGP at EXECUTION (FRAG, 10/10/2026) — the deny of a scratch fragment lives here, not in the write.
//!
//! Spec §5.1 lets the motor `deny` a symbol of the PROJECT only where no later edit can make the
//! code right. A `.py` written to `/tmp` is not that place: it may be a snippet to paste (case
//! `9a8eed8a7502` of the P0M, `trecho_g3.py`), and the next edit can complete it — so the write
//! only advises ([`touring_code::vgp::Contexto::Escrita`]). The moment that holds is the one where
//! an interpreter is asked to run the file: `python3 X.py`, `uv run python X.py`, `node X.js`.
//! This module reads that moment off the command and hands the file's CURRENT content to the same
//! motor, asked [`Contexto::Execucao`].
//!
//! Three rules keep it honest:
//!
//! * The predicate is POSITIVE and reads one SIMPLE command at a time, by its words — the lexer
//!   that already judges `rm -rf` ([`shell_pipelines`] / [`command_of`]). `echo python3 X.py`, a
//!   quoted string, a comment and a heredoc body are text, never a run.
//! * Only what the interpreter takes as the MAIN PROGRAM counts: `python3 -m mod X.py`,
//!   `python3 -c '…' X.py` and `python3 - X.py` hand `X.py` to something else as an argument.
//! * Only SCRATCH files are judged — the buckets [`touring_code::vgp::politica::roda_agora`]
//!   defines for [`Contexto::Execucao`] (`/tmp/`, the project's `.cache/`): they are the files the
//!   write stopped denying. A repo script was judged, with the delta against the disk, when it was
//!   written; re-judging it without a baseline at every run would deny what pre-existed, which
//!   the motor never does.
//!
//! Fail-open end to end: an unreadable or oversized file, a missing interpreter word, a motor
//! `unknown` — none of them denies.

use std::path::{Path, PathBuf};

use touring_code::vgp::politica::roda_agora;
use touring_code::vgp::sombra::{Mode as VgpMode, sombra_check_em, vgp_mode};
use touring_code::vgp::{CheckInput, CheckOutput, Contexto, lang_do_caminho};
use touring_hooks_core::pre_tool_validator::{command_of, shell_pipelines};

use super::runtime::{HookResponse, HookRuntime};

/// Largest script the check reads (bytes): above it the file is left alone — fail-open.
const TETO_BYTES: u64 = 1 << 20;

/// Scripts judged per command line: a line that runs more is a loop, not a fragment.
const MAX_SCRIPTS: usize = 4;

/// The hook name the shadow log records for an execution check.
const HOOK: &str = "pre-bash-exec";

/// A file a simple command hands to an interpreter as the main program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScriptExecutado {
    /// Absolute path (a relative word is resolved against the `cd` of the line, else the cwd).
    pub caminho: PathBuf,
    /// The `lang` of VL0 §2 the motor judges it as.
    pub lang: &'static str,
}

/// The verdict of the VGP over the scratch files this command line runs: `Some(response)` only in
/// "ativo" mode (a deny, or the advisories joined); "sombra" logs and "off" does nothing.
pub(crate) fn vgp_de_execucao(
    runtime: &HookRuntime,
    input: &serde_json::Value,
    command: &str,
) -> Option<HookResponse> {
    let mode = vgp_mode(&runtime.project_root);
    if mode == VgpMode::Off {
        return None;
    }
    let cwd = input
        .get("cwd")
        .and_then(serde_json::Value::as_str)
        .filter(|c| !c.is_empty())
        .map_or_else(|| runtime.project_root.clone(), PathBuf::from);
    let sessao = input
        .pointer("/session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let uso = input
        .pointer("/tool_use_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let mut avisos: Vec<String> = Vec::new();
    for script in scripts_executados(command, &cwd) {
        let path = script.caminho.to_string_lossy().into_owned();
        if !roda_agora(&path, &runtime.project_root, Contexto::Execucao) {
            continue;
        }
        let Some(content) = ler_script(&script.caminho) else {
            continue;
        };
        let vgp_in = CheckInput {
            path,
            content,
            project_root: runtime.project_root.clone(),
            disk: None,
            overlays: std::collections::HashMap::new(),
            lang: script.lang.to_string(),
        };
        let out = sombra_check_em(
            HOOK,
            &vgp_in,
            sessao,
            uso,
            mode,
            crate::vgp_l2::extra_em(&vgp_in, Contexto::Execucao),
            Contexto::Execucao,
        );
        if mode != VgpMode::Ativo {
            continue;
        }
        match resposta_do_vgp(&out, &vgp_in.path, true) {
            Some(deny @ HookResponse::Deny { .. }) => return Some(deny),
            Some(HookResponse::Context { context, .. }) => avisos.push(context),
            _ => {}
        }
    }
    (!avisos.is_empty()).then(|| HookResponse::Context {
        context: avisos.join("\n"),
        event_name: Some("PreToolUse".to_string()),
    })
}

/// Two VGP answers about the same command line (the inline program and the executed files): a
/// deny wins, advisories join, nothing stays nothing.
pub(crate) fn combina(a: Option<HookResponse>, b: Option<HookResponse>) -> Option<HookResponse> {
    match (a, b) {
        (Some(deny @ HookResponse::Deny { .. }), _)
        | (_, Some(deny @ HookResponse::Deny { .. })) => Some(deny),
        (
            Some(HookResponse::Context {
                context: x,
                event_name,
            }),
            Some(HookResponse::Context { context: y, .. }),
        ) => Some(HookResponse::Context {
            context: format!("{x}\n{y}"),
            event_name,
        }),
        (a, None) => a,
        (None, b) => b,
        (a, _) => a,
    }
}

/// The hook response of one motor output — `[VGP · deny]` / `[VGP · aviso]` with the first three
/// findings and, for a deny, the motor's teaching suggestion. `path` is shown as given; for an
/// execution the deny also says WHY the file is judged now.
pub(crate) fn resposta_do_vgp(
    out: &CheckOutput,
    path: &str,
    execucao: bool,
) -> Option<HookResponse> {
    let resumo = out
        .findings
        .iter()
        .take(3)
        .map(|f| format!("{} `{}` ({}:{})", f.classe, f.nome, path, f.linha))
        .collect::<Vec<_>>()
        .join(" · ");
    match out.verdict.as_str() {
        "deny" => Some(HookResponse::Deny {
            reason: format!(
                "[VGP · deny] {resumo}{}{}",
                out.findings
                    .first()
                    .map(|f| format!(" — {}", f.sugestao))
                    .unwrap_or_default(),
                if execucao {
                    " — o arquivo é executado agora, e nenhuma edição seguinte conserta esta \
                     execução: complete o trecho (ou corrija o nome) antes de rodar"
                } else {
                    ""
                }
            ),
            context: None,
            event_name: Some("PreToolUse".to_string()),
        }),
        "warn" => Some(HookResponse::Context {
            context: format!("[VGP · aviso] {resumo}"),
            event_name: Some("PreToolUse".to_string()),
        }),
        _ => None,
    }
}

/// The files the simple commands of `command` run as the main program of `python`/`python3`
/// (including `uv run python`) or `node`, in order, deduplicated, capped at [`MAX_SCRIPTS`].
/// A relative word resolves against the last `cd <dir>` of the line, else `cwd`; only a file that
/// EXISTS is returned (a script the same command is about to write is invisible here — fail-open).
pub(crate) fn scripts_executados(command: &str, cwd: &Path) -> Vec<ScriptExecutado> {
    let mut cwd = cwd.to_path_buf();
    let mut achados: Vec<ScriptExecutado> = Vec::new();
    for pipeline in shell_pipelines(&sem_corpos_de_heredoc(command)) {
        for words in &pipeline {
            let Some((verbo, args)) = command_of(words) else {
                continue;
            };
            if base(verbo) == "cd" {
                if let Some(dir) = args.iter().find(|a| !a.starts_with('-')) {
                    cwd = cwd.join(dir);
                }
                continue;
            }
            let Some((lang, palavra)) = script_do_comando(verbo, args) else {
                continue;
            };
            let caminho = cwd.join(palavra);
            let candidato = ScriptExecutado { caminho, lang };
            if candidato.caminho.is_file()
                && lang_do_caminho(&candidato.caminho.to_string_lossy()) == Some(lang)
                && !achados.contains(&candidato)
            {
                achados.push(candidato);
                if achados.len() == MAX_SCRIPTS {
                    return achados;
                }
            }
        }
    }
    achados
}

/// `(lang, script word)` when the simple command `verbo args…` runs a file as the main program.
fn script_do_comando<'a>(verbo: &str, args: &'a [String]) -> Option<(&'static str, &'a str)> {
    match base(verbo) {
        "uv" => {
            let resto = args.strip_prefix(&["run".to_string()])?;
            let i = depois_das_opcoes_do_uv(resto)?;
            let (programa, argumentos) = resto[i..].split_first()?;
            eh_python(base(programa)).then_some(())?;
            python_script(argumentos).map(|s| ("python", s))
        }
        b if eh_python(b) => python_script(args).map(|s| ("python", s)),
        "node" | "nodejs" => node_script(args).map(|s| ("javascript", s)),
        _ => None,
    }
}

/// The last component of a command word (`.venv/bin/python3` → `python3`).
fn base(palavra: &str) -> &str {
    palavra.rsplit('/').next().unwrap_or(palavra)
}

/// Is this command WORD a python interpreter (`python3`, `/usr/bin/python3.12`, `.venv/bin/python`)?
pub(crate) fn palavra_eh_python(palavra: &str) -> bool {
    eh_python(base(palavra))
}

/// `python`, `python3`, `python3.12` — never `python3-config` or `pythonw`.
fn eh_python(base: &str) -> bool {
    base == "python"
        || base == "python3"
        || base
            .strip_prefix("python3.")
            .is_some_and(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
}

/// `uv run`'s options that take a SEPARATE value (`--with requests`, `--python 3.12`): without
/// them the value would be read as the program. An unknown option is taken as a flag — at worst
/// its value is read as the program, which is not a python word, and the line is left alone.
const UV_OPCOES_COM_VALOR: &[&str] = &[
    "--with",
    "--python",
    "-p",
    "--project",
    "--directory",
    "--env-file",
    "--package",
    "--extra",
    "--group",
    "--with-requirements",
    "--with-editable",
    "--index",
    "--index-url",
    "--find-links",
    "-f",
    "--exclude-newer",
    "--config-file",
    "--cache-dir",
];

/// The index of the first word of `uv run`'s arguments that is not one of its options.
fn depois_das_opcoes_do_uv(args: &[String]) -> Option<usize> {
    let mut i = 0;
    while let Some(a) = args.get(i) {
        if !a.starts_with('-') {
            return Some(i);
        }
        i += if UV_OPCOES_COM_VALOR.contains(&a.as_str()) {
            2
        } else {
            1
        };
    }
    None
}

/// What one option word means for the search of `python`'s main program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Opcao {
    /// A flag with no say in it (`-u`, `-B`, `-O`): keep reading.
    Segue,
    /// A flag whose value is the NEXT word (`-W error`, `-X dev`): skip that word too.
    ValorNaPalavraSeguinte,
    /// `-c`/`-m` (the rest is the code or the module's argv), `-` (stdin), `-V`/`-h`/`--version`/
    /// `--help` (print and exit): no file is the main program.
    SemArquivo,
}

/// A short-option cluster read the way CPython reads it: the first of `c`/`m`/`V`/`h`/`?` ends the
/// search; `W`/`X`/`Q` take the rest of the cluster as value, or the next word when it is empty.
fn opcao_curta(cluster: &str) -> Opcao {
    for (i, c) in cluster.char_indices() {
        match c {
            'c' | 'm' | 'V' | 'h' | '?' => return Opcao::SemArquivo,
            'W' | 'X' | 'Q' => {
                return if cluster[i + c.len_utf8()..].is_empty() {
                    Opcao::ValorNaPalavraSeguinte
                } else {
                    Opcao::Segue
                };
            }
            _ => {}
        }
    }
    Opcao::Segue
}

/// The long options that matter: the ones that only print, and the one that takes a value.
fn opcao_longa(opcao: &str) -> Opcao {
    match opcao {
        "--version" | "--help" | "--help-env" | "--help-xoptions" | "--help-all" => {
            Opcao::SemArquivo
        }
        "--check-hash-based-pycs" => Opcao::ValorNaPalavraSeguinte,
        _ => Opcao::Segue,
    }
}

/// The script `python` runs as the main program, read off the way CPython reads its options:
/// `-c` and `-m` (alone or inside a cluster such as `-uc`) hand everything after them to the code
/// or the module, `-W`/`-X` take a value, `-` is stdin, `--` ends the options. `None` when no
/// `.py` file is the main program.
fn python_script(args: &[String]) -> Option<&str> {
    let mut it = args.iter().map(String::as_str);
    while let Some(a) = it.next() {
        if a == "--" {
            return it.next().filter(|s| termina_em(s, &["py"]));
        }
        let Some(opcao) = a.strip_prefix('-') else {
            return termina_em(a, &["py"]).then_some(a);
        };
        let efeito = if opcao.is_empty() {
            Opcao::SemArquivo
        } else if opcao.starts_with('-') {
            opcao_longa(a)
        } else {
            opcao_curta(opcao)
        };
        match efeito {
            Opcao::SemArquivo => return None,
            Opcao::ValorNaPalavraSeguinte => {
                it.next();
            }
            Opcao::Segue => {}
        }
    }
    None
}

/// The script `node` runs as the main program. `-e`/`-p`/`--eval`/`--print` (inline), `-c`/
/// `--check`, `--test` and `-i` run no file; `-r`/`--require`/`--import`/`--loader` take a value.
fn node_script(args: &[String]) -> Option<&str> {
    const SEM_ARQUIVO: &[&str] = &[
        "-e",
        "-p",
        "-pe",
        "-c",
        "-i",
        "--eval",
        "--print",
        "--check",
        "--test",
        "--interactive",
        "--help",
        "--version",
        "-h",
        "-v",
    ];
    const COM_VALOR: &[&str] = &[
        "-r",
        "--require",
        "--import",
        "--loader",
        "--experimental-loader",
        "--conditions",
        "-C",
        "--env-file",
        "--input-type",
    ];
    let mut it = args.iter().map(String::as_str);
    while let Some(a) = it.next() {
        if a == "--" {
            return it.next().filter(|s| termina_em(s, &["js", "mjs", "cjs"]));
        }
        let nome = a.split('=').next().unwrap_or(a);
        if SEM_ARQUIVO.contains(&nome) {
            return None;
        }
        if a.starts_with('-') {
            if COM_VALOR.contains(&a) {
                it.next();
            }
            continue;
        }
        return termina_em(a, &["js", "mjs", "cjs"]).then_some(a);
    }
    None
}

/// Does the word end in `.ext` for one of `exts`?
fn termina_em(palavra: &str, exts: &[&str]) -> bool {
    Path::new(palavra)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| exts.contains(&e))
}

/// The text of the script, unless it is unreadable, not UTF-8 or above [`TETO_BYTES`].
fn ler_script(caminho: &Path) -> Option<String> {
    let meta = std::fs::metadata(caminho).ok()?;
    if meta.len() > TETO_BYTES {
        return None;
    }
    std::fs::read_to_string(caminho).ok()
}

/// `command` without the BODIES of its heredocs (from the line after `<<TAG` to the line `TAG`):
/// a body is data fed to a program, not a command of the line. An unterminated heredoc swallows the
/// rest, as the shell does.
fn sem_corpos_de_heredoc(command: &str) -> String {
    let mut mantidas: Vec<&str> = Vec::new();
    let mut ate: Option<String> = None;
    for linha in command.lines() {
        if let Some(tag) = &ate {
            if linha.trim() == tag.as_str() {
                ate = None;
            }
            continue;
        }
        mantidas.push(linha);
        if let Some(pos) = linha.find("<<")
            && !linha[pos..].starts_with("<<<")
        {
            let tag = linha[pos + 2..]
                .trim_start_matches('-')
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_matches(['\'', '"']);
            if !tag.is_empty() {
                ate = Some(tag.to_string());
            }
        }
    }
    mantidas.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Scripts that exist: `/tmp/<d>/a.py`, `b.py`, `c.js`, `n.txt`.
    fn scratch() -> TempDir {
        let d = TempDir::new_in("/tmp").expect("scratch");
        for (nome, texto) in [("a.py", "x = 1\n"), ("b.py", "y = 2\n"), ("c.js", "1;\n")] {
            std::fs::write(d.path().join(nome), texto).expect("escreve");
        }
        std::fs::write(d.path().join("n.txt"), "nota\n").expect("escreve");
        d
    }

    fn alvo(d: &TempDir, comando: &str) -> Vec<(String, &'static str)> {
        scripts_executados(comando, Path::new("/"))
            .into_iter()
            .map(|s| {
                (
                    s.caminho.strip_prefix(d.path()).map_or_else(
                        |_| s.caminho.display().to_string(),
                        |r| r.display().to_string(),
                    ),
                    s.lang,
                )
            })
            .collect()
    }

    fn ex(d: &TempDir, resto: &str) -> String {
        resto.replace('@', &d.path().display().to_string())
    }

    #[test]
    fn python_e_uv_run_python_executam_o_arquivo() {
        let d = scratch();
        for cmd in [
            "python @/a.py",
            "python3 @/a.py arg --flag",
            "python3.12 @/a.py",
            "/usr/bin/python3 @/a.py",
            ".venv/bin/python @/a.py",
            "python3 -u -B @/a.py",
            "python3 -W error @/a.py",
            "python3 -Werror @/a.py",
            "python3 -X dev @/a.py",
            "python3 -- @/a.py",
            "uv run python @/a.py",
            "uv run python3 -u @/a.py",
            "uv run --with requests --python 3.12 python @/a.py",
            "FOO=1 python3 @/a.py",
            "env FOO=1 python3 @/a.py",
            "timeout 5 python3 @/a.py",
            "nohup python3 @/a.py",
            "cd / && python3 @/a.py | head",
            "python3 @/a.py 2>&1",
        ] {
            assert_eq!(
                alvo(&d, &ex(&d, cmd)),
                [("a.py".to_string(), "python")],
                "{cmd}"
            );
        }
    }

    #[test]
    fn node_executa_js_mjs_cjs() {
        let d = scratch();
        std::fs::write(d.path().join("m.mjs"), "1;\n").unwrap();
        std::fs::write(d.path().join("k.cjs"), "1;\n").unwrap();
        for (cmd, esperado) in [
            ("node @/c.js", "c.js"),
            ("node --no-warnings @/m.mjs", "m.mjs"),
            ("node -r ./setup @/k.cjs", "k.cjs"),
            ("node --require ./setup --import ./x @/c.js", "c.js"),
            ("nodejs @/c.js", "c.js"),
        ] {
            assert_eq!(
                alvo(&d, &ex(&d, cmd)),
                [(esperado.to_string(), "javascript")],
                "{cmd}"
            );
        }
    }

    #[test]
    fn modulo_codigo_inline_stdin_e_outros_interpretadores_ficam_fora() {
        let d = scratch();
        for cmd in [
            "python3 -m pytest @/a.py",
            "python3 -m @/a.py",
            "python3 -mpytest @/a.py",
            "python3 -uc 'print(1)' @/a.py",
            "python3 -c 'print(1)' @/a.py",
            "python3 -cprint @/a.py",
            "python3 - @/a.py",
            "python3 --version @/a.py",
            "python3 @/n.txt",
            "python3 @/nao_existe.py",
            "uv run python -m pytest @/a.py",
            "uv run pytest @/a.py",
            "uv pip install @/a.py",
            "uv run",
            "python3-config @/a.py",
            "pythonw @/a.py",
            "bash @/a.py",
            "ruby @/a.py",
            "cat @/a.py",
            "node -e 'x' @/c.js",
            "node -p 1 @/c.js",
            "node --check @/c.js",
            "node --test @/c.js",
            "node @/a.py",
            "python3 @/c.js",
        ] {
            assert!(alvo(&d, &ex(&d, cmd)).is_empty(), "{cmd}");
        }
    }

    #[test]
    fn so_o_comando_simples_conta_nunca_o_texto_da_linha() {
        let d = scratch();
        for cmd in [
            "echo python3 @/a.py",
            "echo \"python3 @/a.py\"",
            "echo 'python3 @/a.py'",
            "grep -n python3 @/a.py",
            "cat @/a.py # python3 @/a.py",
            "# python3 @/a.py",
            "cat > @/s.sh <<'EOF'\npython3 @/a.py\nEOF",
            "cat > @/s.sh <<-EOF\n\tpython3 @/a.py\n\tEOF",
            "git commit -m \"python3 @/a.py\"",
        ] {
            assert!(alvo(&d, &ex(&d, cmd)).is_empty(), "{cmd:?}");
        }
        // o controle: o mesmo comando FORA do texto é execução
        assert_eq!(alvo(&d, &ex(&d, "echo ok; python3 @/a.py")).len(), 1);
        assert_eq!(
            alvo(&d, &ex(&d, "cat > @/s.sh <<'EOF'\nx\nEOF\npython3 @/a.py")).len(),
            1,
            "depois do heredoc a linha volta a ser comando"
        );
        assert_eq!(
            alvo(
                &d,
                &ex(&d, "cat <<-EOF > @/s.sh\n\tx\n\tEOF\npython3 @/a.py")
            )
            .len(),
            1,
            "`<<-` aceita o marcador recuado, e a linha seguinte volta a ser comando"
        );
    }

    #[test]
    fn varios_scripts_na_linha_sem_repeticao_e_com_teto() {
        let d = scratch();
        let dois = alvo(
            &d,
            &ex(&d, "python3 @/a.py && python3 @/b.py ; python3 @/a.py"),
        );
        assert_eq!(
            dois,
            [
                ("a.py".to_string(), "python"),
                ("b.py".to_string(), "python")
            ]
        );
        let muitos = (0..9)
            .map(|i| {
                std::fs::write(d.path().join(format!("s{i}.py")), "z = 0\n").unwrap();
                format!("python3 {}/s{i}.py", d.path().display())
            })
            .collect::<Vec<_>>()
            .join("; ");
        // literal, não a constante: o teste que lê o limiar do código não o fixa
        assert_eq!(scripts_executados(&muitos, Path::new("/")).len(), 4);
        assert_eq!(MAX_SCRIPTS, 4);
    }

    #[test]
    fn caminho_relativo_resolve_pelo_ultimo_cd_da_linha_senao_pelo_cwd() {
        let d = scratch();
        let dir = d.path();
        let por_cwd = scripts_executados("python3 a.py", dir);
        assert_eq!(por_cwd.len(), 1);
        assert_eq!(por_cwd[0].caminho, dir.join("a.py"));
        let por_cd = scripts_executados(
            &format!("cd {} && python3 b.py", dir.display()),
            Path::new("/"),
        );
        assert_eq!(por_cd.len(), 1);
        assert_eq!(por_cd[0].caminho, dir.join("b.py"));
        assert!(scripts_executados("python3 a.py", Path::new("/")).is_empty());
    }

    #[test]
    fn heredoc_sem_corpo_e_herestring_e_marcador_entre_aspas() {
        let s = sem_corpos_de_heredoc("cat <<'EOF' > f\nA\nB\nEOF\nls");
        assert_eq!(s, "cat <<'EOF' > f\nls");
        // `<<<` é herestring: não abre corpo
        assert_eq!(sem_corpos_de_heredoc("cat <<< x\nls"), "cat <<< x\nls");
        // sem terminador, o corpo engole o resto (como no shell)
        assert_eq!(sem_corpos_de_heredoc("cat <<EOF\nA\nB"), "cat <<EOF");
        assert_eq!(sem_corpos_de_heredoc("ls\npwd"), "ls\npwd");
    }

    #[test]
    fn combina_deny_vence_aviso_avisos_se_juntam() {
        let aviso = |t: &str| HookResponse::Context {
            context: t.to_string(),
            event_name: Some("PreToolUse".into()),
        };
        let deny = || HookResponse::Deny {
            reason: "d".into(),
            context: None,
            event_name: Some("PreToolUse".into()),
        };
        assert!(matches!(
            combina(Some(aviso("a")), Some(deny())),
            Some(HookResponse::Deny { .. })
        ));
        assert!(matches!(
            combina(Some(deny()), Some(aviso("a"))),
            Some(HookResponse::Deny { .. })
        ));
        assert!(matches!(
            combina(None, Some(deny())),
            Some(HookResponse::Deny { .. })
        ));
        match combina(Some(aviso("a")), Some(aviso("b"))) {
            Some(HookResponse::Context { context, .. }) => assert_eq!(context, "a\nb"),
            outra => panic!("{outra:?}"),
        }
        assert!(matches!(
            combina(Some(aviso("a")), None),
            Some(HookResponse::Context { .. })
        ));
        assert!(matches!(
            combina(None, Some(aviso("b"))),
            Some(HookResponse::Context { .. })
        ));
        assert!(combina(None, None).is_none());
    }

    #[test]
    fn arquivo_acima_do_teto_ou_nao_utf8_fica_de_fora() {
        let d = scratch();
        let grande = d.path().join("grande.py");
        std::fs::write(&grande, vec![b'#'; (TETO_BYTES + 1) as usize]).unwrap();
        assert!(ler_script(&grande).is_none());
        let latin1 = d.path().join("latin1.py");
        std::fs::write(&latin1, b"# caf\xe9\n").unwrap();
        assert!(ler_script(&latin1).is_none());
        assert_eq!(
            ler_script(&d.path().join("a.py")).as_deref(),
            Some("x = 1\n")
        );
    }
}
