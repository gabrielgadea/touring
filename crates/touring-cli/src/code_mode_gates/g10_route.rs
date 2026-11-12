//! G10ROTA (30.4.97) — which route of `touring run` EXECUTES the program a gate is about to name.
//!
//! Real case (analise-78, 09/10/2026 22:39 BRT, transcript `05166acc-…`, lines 5786-5798): the 5th
//! serial `python-inline` WROTE `RETOMADA.md` (`Path.write_text`). G10 named
//! `touring run --lang python --code …`; the executor denies the write under the Sandboxed profile (X6
//! `file-write`) and the named route failed. The one that worked was
//! `TOURING_TRUSTED_OK=1 touring run --allow-forbidden …`, valid only inside the workspace because
//! Landlock grants write access only to it.
//!
//! D8 — the deny has no predicate of its own. The EXECUTOR decides whether a route runs:
//! [`touring_ceg::gateway::run_admission`] (the same composition `touring run` applies) and
//! [`touring_ceg::gateway::sandbox_executor::sandbox_can_write`] (the write roots that
//! Landlock grants). This module only translates their verdict into text.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;
use touring_ceg::gateway::run_admission::{RunRoute, run_route};
use touring_ceg::gateway::sandbox_executor::sandbox_can_write;

/// The route that executes a program, with the write destination already checked against Landlock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Route {
    /// `touring run` as is: the Sandboxed profile admits the program.
    Sandboxed,
    /// Sandboxed refuses, Trusted admits: `TOURING_TRUSTED_OK=1 touring run --allow-forbidden`.
    Trusted {
        /// The phrase from the executor that refused under Sandboxed.
        sandbox_denial: String,
    },
    /// The program writes to `dest`, outside what Landlock grants: no route writes there.
    NoWritableDest {
        /// The resolved destination (absolute, no `..`).
        dest: PathBuf,
    },
    /// The program (or the script file itself) needs a path the sandbox CANNOT READ — the Landlock
    /// read roots are /usr, /etc, ~/projects, ~/.cache, ~/.claude… and not the rest of `$HOME`
    /// (`~/claude-artefatos` included). Sandboxed and Trusted share that ruleset, so neither runs it.
    OutOfReach {
        /// The first existing path the sandbox cannot reach.
        path: PathBuf,
    },
    /// The executor refuses the program under BOTH profiles (network, destructive pattern…).
    Unrunnable {
        /// The refusal from the Trusted profile.
        reason: String,
    },
}

/// The route of the `code` program (`lang`: `python`, `bash`…) from the project `project_root`.
///
/// Retro 11/11/2026 (d): a route the executor admits is still no route when the program needs a
/// path the Landlock does not let the sandbox READ — `touring run` would fail with "Permission
/// denied", and the deny that named it taught failing. The reach question goes to the same table
/// the sandbox is built from ([`unreachable_beyond_workspace`]), never to a list of its own.
pub(super) fn route_for(project_root: &Path, lang: &str, code: &str) -> Route {
    let out_of_reach = || {
        super::unreachable_beyond_workspace(project_root, code)
            .map(|path| Route::OutOfReach { path })
    };
    match run_route(lang, code, project_root) {
        RunRoute::Sandboxed => out_of_reach().unwrap_or(Route::Sandboxed),
        RunRoute::Trusted { sandbox_denial } => {
            match unwritable_destination(project_root, lang, code) {
                Some(dest) => Route::NoWritableDest { dest },
                None => out_of_reach().unwrap_or(Route::Trusted { sandbox_denial }),
            }
        }
        RunRoute::Unrunnable { reason } => Route::Unrunnable { reason },
    }
}

/// The script FILE itself, when it sits where the sandbox cannot read it (outside the project and
/// outside the Landlock read roots — `~/claude-artefatos` is the case that was measured).
pub(super) fn script_out_of_reach(project_root: &Path, script: &Path) -> Option<Route> {
    let unreachable = script.exists()
        && !script.starts_with(project_root)
        && !touring_ceg::gateway::sandbox_executor::sandbox_can_reach(script);
    unreachable.then(|| Route::OutOfReach {
        path: script.to_path_buf(),
    })
}

impl Route {
    /// Does any `touring run` execute this program?
    pub(super) fn runs(&self) -> bool {
        matches!(self, Self::Sandboxed | Self::Trusted { .. })
    }

    /// The environment prefix of the command: elevation is PER COMMAND, never an exported state.
    pub(super) fn env_prefix(&self) -> &'static str {
        match self {
            Self::Trusted { .. } => "TOURING_TRUSTED_OK=1 ",
            _ => "",
        }
    }

    /// The `touring run` flag that selects the profile.
    pub(super) fn run_flags(&self) -> &'static str {
        match self {
            Self::Trusted { .. } => "--allow-forbidden ",
            _ => "",
        }
    }

    /// The phrase that explains why the route differs from plain `touring run` — empty under Sandboxed.
    pub(super) fn explanation(&self, project_root: &Path) -> String {
        let root = project_root.display();
        match self {
            Self::Sandboxed => String::new(),
            Self::Trusted { sandbox_denial } => format!(
                " Este corpo ESCREVE: o executor o nega no perfil Sandboxed ({sandbox_denial}), \
                 então `touring run` sem `--allow-forbidden` falharia. A rota que executa é a \
                 Trusted, decidida POR COMANDO (TOURING_TRUSTED_OK=1 no próprio comando) e válida \
                 só dentro do workspace {root}: o Landlock concede escrita apenas a ele e aos \
                 temporários do run."
            ),
            Self::NoWritableDest { dest } => format!(
                " Este corpo escreve em `{}`, fora do que o sandbox pode escrever (workspace \
                 {root}, ~/.cache, ~/.cargo e o tmp privado do run): nenhuma rota de `touring run` \
                 escreve ali — a Trusted também passa pelo Landlock. Alternativa: aponte o \
                 destino para dentro do workspace.",
                dest.display()
            ),
            Self::OutOfReach { path } => format!(
                " Este corpo precisa de `{}`, fora do que o sandbox enxerga: o Landlock só concede \
                 leitura a /usr, /etc, ~/projects, ~/.cache, ~/.claude e poucos outros caminhos (não \
                 a ~/claude-artefatos nem ao resto do $HOME), e as rotas Sandboxed e Trusted passam \
                 pelo mesmo Landlock: nenhuma rota de `touring run` o executa. Alternativa: ponha \
                 o arquivo dentro do workspace.",
                path.display()
            ),
            Self::Unrunnable { reason } => format!(
                " O executor nega este corpo nos DOIS perfis (Sandboxed e Trusted): {reason}. \
                 Nenhuma rota de `touring run` o executa."
            ),
        }
    }
}

// ── the write destination ────────────────────────────────────────────────────

/// The python marks that write to ONE path. A subset of `PY_INLINE_WRITER_MARKS` (the test
/// `as_marcas_de_destino_sao_subconjunto_das_de_escritor` prevents drift): the wide list includes
/// network/subprocess/DB marks, whose literals are NOT write destinations; and `shutil.`/`os.rename`
/// carry two paths (source read, destination written), which a lexicon cannot tell apart.
pub(super) const PY_PATH_WRITE_MARKS: &[&str] = &[
    ".write(",
    ".writelines(",
    "write_text(",
    "write_bytes(",
    ".unlink(",
    ".touch(",
    ".chmod(",
    ".rmdir(",
    "os.remove",
    "os.rmdir",
    "os.removedirs",
    "os.mkdir",
    "os.makedirs",
    "os.truncate",
];

fn literal_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"["']([^"'\n]+)["']"#).expect("literal regex compila"))
}

fn binding_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // The assignment ENDS at the literal (or at the `)` of `Path(...)`): `t = Path("x").read_text()`
        // binds the CONTENT read, not the path — and `write_text(t)` does not write to "x".
        Regex::new(
            r#"^\s*([A-Za-z_]\w*)\s*=\s*(?:(?:pathlib\.)?Path\(\s*)?["']([^"'\n]+)["']\s*\)?\s*(?:#.*)?$"#,
        )
        .expect("binding regex compila")
    })
}

fn ident_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[A-Za-z_]\w*").expect("ident regex compila"))
}

/// A literal that CAN leave the workspace: absolute, `~/…` or with `..`. A plain relative name
/// (`saida.md`) writes to the cwd — inside the workspace by definition.
fn may_leave_workspace(lit: &str) -> bool {
    lit.starts_with('/') || lit.starts_with("~/") || lit.contains("../") || lit.ends_with("/..")
}

/// The path literals the python body WRITES: the literals of the line that carries a write mark,
/// plus the literals bound (`p = Path("…")`, `p = "…"`) to the names that appear in it.
/// A lexical heuristic — it does not resolve `Path.home() / …`, f-strings or `os.path.join`; a
/// destination it does not see is one the executor still refuses with a message that teaches.
pub(super) fn python_write_destinations(body: &str) -> Vec<String> {
    let mut bound: HashMap<String, String> = HashMap::new();
    let mut out: Vec<String> = Vec::new();
    for line in body.lines() {
        if let Some(c) = binding_re().captures(line) {
            bound.insert(c[1].to_string(), c[2].to_string());
        }
        let writes = PY_PATH_WRITE_MARKS.iter().any(|m| line.contains(m))
            || super::py_open_mutating_mode(line);
        if !writes {
            continue;
        }
        let from_literals = literal_re().captures_iter(line).map(|c| c[1].to_string());
        let from_bindings = ident_re()
            .find_iter(line)
            .filter_map(|m| bound.get(m.as_str()).cloned());
        for lit in from_literals.chain(from_bindings) {
            if may_leave_workspace(&lit) && !out.contains(&lit) {
                out.push(lit);
            }
        }
    }
    out
}

fn resolve_literal(project_root: &Path, lit: &str) -> PathBuf {
    let raw = match (lit.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(lit),
    };
    // No `..` normalization: `sandbox_can_write` resolves the path (lexical + symlinks) before
    // judging, and the destination quoted in the deny comes out as the body wrote it.
    if raw.is_absolute() {
        raw
    } else {
        project_root.join(raw)
    }
}

/// The first write destination that Landlock does NOT grant (python only: it is where the body is inspectable).
fn unwritable_destination(project_root: &Path, lang: &str, code: &str) -> Option<PathBuf> {
    if !matches!(lang.to_ascii_lowercase().as_str(), "python" | "py") {
        return None;
    }
    python_write_destinations(code)
        .iter()
        .map(|lit| resolve_literal(project_root, lit))
        .find(|dest| !sandbox_can_write(project_root, dest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_marcas_de_destino_sao_subconjunto_das_de_escritor() {
        for m in PY_PATH_WRITE_MARKS {
            assert!(
                super::super::PY_INLINE_WRITER_MARKS.contains(m),
                "`{m}` está na lista de destinos mas não na de escritores — a deriva que a regra D8 proíbe"
            );
        }
    }

    /// The real case: the path comes from a binding (`p = pathlib.Path("…")`) and the `write_text` is
    /// on another line — and it is RELATIVE, hence inside the workspace: no destination to watch.
    #[test]
    fn o_caso_real_nao_tem_destino_fora_do_workspace() {
        let body = "import pathlib\np = pathlib.Path(\"docs/plans/2026-10-06-geopolitica-logistica-eua/RETOMADA.md\")\nt = p.read_text(encoding=\"utf-8\")\nvelho = \"> Estado 10/10 ~00:30:\"\np.write_text(t.replace(velho, \"> Estado 09/10 22:38\"), encoding=\"utf-8\")";
        assert!(python_write_destinations(body).is_empty());
    }

    #[test]
    fn destino_absoluto_vinculado_e_visto_na_linha_do_write() {
        let body = "import pathlib\nalvo = pathlib.Path(\"/etc/touring-g97/saida.md\")\nalvo.write_text(\"x\")";
        assert_eq!(
            python_write_destinations(body),
            vec!["/etc/touring-g97/saida.md".to_string()]
        );
    }

    #[test]
    fn destino_literal_direto_no_open_de_escrita() {
        let body = "with open(\"/var/tmp/g97/x.txt\", \"w\") as f:\n    f.write(\"x\")";
        assert_eq!(
            python_write_destinations(body),
            vec!["/var/tmp/g97/x.txt".to_string()]
        );
    }

    /// Control: an absolute path that is only READ is not a destination — the Trusted route stays promised.
    #[test]
    fn caminho_absoluto_apenas_lido_nao_e_destino() {
        let body = "import pathlib\nt = pathlib.Path(\"/etc/hostname\").read_text()\npathlib.Path(\"saida.md\").write_text(t)";
        assert!(python_write_destinations(body).is_empty());
    }

    #[test]
    fn home_e_pai_contam_como_saida_possivel() {
        let body = "p = \"~/relatorio.md\"\nopen(p, \"w\").write(\"x\")\nq = \"../fora/y.md\"\nopen(q, \"w\").write(\"y\")";
        assert_eq!(
            python_write_destinations(body),
            vec!["~/relatorio.md".to_string(), "../fora/y.md".to_string()]
        );
    }

    // ── Retro 11/11/2026 (d): o que o sandbox NÃO ENXERGA também não tem rota ────────────────
    //
    // `~/claude-artefatos` (a pasta que o `tmp_write_guard` manda usar) fica fora das raízes de
    // leitura do Landlock; o G10 write→run nomeava `touring run --file ~/claude-artefatos/x.py`
    // mesmo assim — a rota que o gate ensinava falhava com "Permission denied".

    /// Um diretório sob `$HOME` que o sandbox não lê, com um arquivo dentro. `None` quando o
    /// ambiente não deixa montá-lo (sem `$HOME`, ou `TOURING_SANDBOX_READ_WIDE` ligado).
    fn scratch_fora_do_alcance() -> Option<(tempfile::TempDir, PathBuf)> {
        let home = std::env::var_os("HOME")?;
        let dir = tempfile::Builder::new()
            .prefix(".retro-g10-")
            .tempdir_in(home)
            .ok()?;
        let file = dir.path().join("entrada.txt");
        std::fs::write(&file, "x").ok()?;
        (!touring_ceg::gateway::sandbox_executor::sandbox_can_reach(&file)).then_some((dir, file))
    }

    fn corpo_que_le(path: &Path) -> String {
        format!(
            "import pathlib\nprint(pathlib.Path(\"{}\").read_text())\n",
            path.display()
        )
    }

    #[test]
    fn corpo_que_le_fora_do_alcance_do_sandbox_nao_tem_rota() {
        let Some((_keep, fora)) = scratch_fora_do_alcance() else {
            return;
        };
        let ws = tempfile::tempdir().expect("workspace");
        let rota = route_for(ws.path(), "python", &corpo_que_le(&fora));
        assert_eq!(rota, Route::OutOfReach { path: fora.clone() });
        assert!(!rota.runs(), "nenhuma rota de `touring run` o executa");
        let por_que = rota.explanation(ws.path());
        assert!(por_que.contains(&fora.display().to_string()), "{por_que}");
        assert!(por_que.contains("nenhuma rota"), "{por_que}");
        assert!(
            !por_que.contains("TOURING_TRUSTED_OK"),
            "Trusted não resolve: {por_que}"
        );
    }

    /// Controles: o que o sandbox LÊ, ou o próprio workspace, segue com a rota de sempre.
    #[test]
    fn controles_o_que_o_sandbox_le_e_o_workspace_seguem_sandboxed() {
        let ws = tempfile::tempdir().expect("workspace");
        // dentro do workspace
        let dentro = ws.path().join("dados.txt");
        std::fs::write(&dentro, "x").expect("file");
        assert_eq!(
            route_for(ws.path(), "python", &corpo_que_le(&dentro)),
            Route::Sandboxed
        );
        // numa raiz de leitura do sandbox (/etc)
        let etc = Path::new("/etc/passwd");
        if etc.exists() {
            assert_eq!(
                route_for(ws.path(), "python", &corpo_que_le(etc)),
                Route::Sandboxed
            );
        }
        // um workspace que ele mesmo está fora das raízes enumeradas: o workspace é sempre
        // concedido ao run, então os arquivos DENTRO dele não são "fora do alcance".
        if let Some((keep, file)) = scratch_fora_do_alcance() {
            assert_eq!(
                route_for(keep.path(), "python", &corpo_que_le(&file)),
                Route::Sandboxed
            );
        }
    }
}
