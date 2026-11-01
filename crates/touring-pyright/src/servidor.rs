//! The set of layer-2 verifiers and how a project is matched to one.
//!
//! A [`Servidor`] is a language's own checker as the *project* ships it: `pyright-langserver` from the
//! venv, `rust-analyzer` of the toolchain the project pins, `tsserver` from the project's
//! `node_modules/typescript`, `deno lsp` for a Deno project. Touring never installs any of them (VL0
//! §6): discovery is a pure lookup that either names a binary or says why there is none, and an absent
//! verifier is an `unknown` with that reason while layer 1 stays standing.
//!
//! The pool ([`crate::pool`]) is generic over this enum: it keeps one warm [`Sessao`] per
//! `(project root, server, sub-key)` and never looks inside it.

use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use crate::achado::L2Resultado;

/// The layer-2 verifiers the pool can keep warm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Servidor {
    /// `pyright-langserver` (LSP) from the project's venv.
    Pyright,
    /// `rust-analyzer` (LSP) of the toolchain the project resolves — native diagnostics only.
    RustAnalyzer,
    /// `tsserver` (its own protocol, not LSP) of the project's `node_modules/typescript`.
    Tsserver,
    /// `deno lsp` (LSP), for a project that declares itself Deno (`deno.json`).
    DenoLsp,
}

impl Servidor {
    /// Every verifier, in a stable order.
    pub const TODOS: [Servidor; 4] = [
        Servidor::Pyright,
        Servidor::RustAnalyzer,
        Servidor::Tsserver,
        Servidor::DenoLsp,
    ];

    /// The name used in reasons, in logs and in `TOURING_VGP_L2_OFF`.
    pub fn nome(self) -> &'static str {
        match self {
            Self::Pyright => "pyright",
            Self::RustAnalyzer => "rust-analyzer",
            Self::Tsserver => "tsserver",
            Self::DenoLsp => "deno",
        }
    }

    /// Inverse of [`Servidor::nome`] (case-insensitive).
    pub fn do_nome(nome: &str) -> Option<Self> {
        Self::TODOS
            .into_iter()
            .find(|s| s.nome().eq_ignore_ascii_case(nome.trim()))
    }

    /// The verifier for a layer-1 `lang` (VL0 §2) of the file `caminho` in the project at `raiz`.
    ///
    /// `typescript`/`tsx`/`javascript` go to `deno lsp` when the nearest project marker above the file
    /// is a `deno.json[c]` and to `tsserver` otherwise; `html`/`css` (and anything unknown) have no
    /// layer 2 — `None`, and the caller keeps layer 1 alone.
    pub fn para_lang(lang: &str, raiz: &Path, caminho: &str) -> Option<Self> {
        match lang {
            "python" => Some(Self::Pyright),
            "rust" => Some(Self::RustAnalyzer),
            "typescript" | "tsx" | "javascript" => Some(if projeto_deno(raiz, caminho) {
                Self::DenoLsp
            } else {
                Self::Tsserver
            }),
            _ => None,
        }
    }

    /// The part of a registry key beyond `(project root, verifier)`: where one project can hold several
    /// independent installs, which one an edit belongs to (the package whose `node_modules` provides the
    /// `tsserver`, the Cargo workspace root, the Deno workspace root). Pure filesystem lookups; empty
    /// when there is none — the spawner then fails with the reason, and a verifier that appears later
    /// is a different key, so its absence is not remembered.
    pub(crate) fn subchave(self, raiz: &Path, arquivo: &str) -> PathBuf {
        match self {
            Self::Pyright => PathBuf::new(),
            Self::RustAnalyzer => crate::rust_analyzer::subchave(raiz, arquivo),
            Self::Tsserver => crate::tsserver::subchave(raiz, arquivo),
            Self::DenoLsp => crate::deno::subchave(raiz, arquivo),
        }
    }

    /// The cheap, pure part of discovery (filesystem lookups, `PATH`): is there anything to start for this
    /// edit? `Err` = why the layer is absent. Answered before a background warm-up is launched.
    pub(crate) fn pre_checar(self, alvo: &Alvo<'_>, amb: &Ambiente) -> Result<(), String> {
        match self {
            Self::Pyright => Ok(()),
            Self::RustAnalyzer => crate::rust_analyzer::pre_checar(alvo, amb),
            Self::Tsserver => crate::tsserver::localizar(alvo.raiz, alvo.arquivo, amb).map(|_| ()),
            Self::DenoLsp => crate::deno::localizar(alvo.raiz, alvo.arquivo, amb).map(|_| ()),
        }
    }

    /// Discovers this verifier in the project and starts it. `Err` = why the layer is absent.
    pub(crate) fn abrir(self, alvo: &Alvo<'_>, amb: &Ambiente) -> Result<Box<dyn Sessao>, String> {
        match self {
            Self::Pyright => crate::pyright::PyrightSession::discover(alvo.raiz)
                .map(|sessao| Box::new(sessao) as Box<dyn Sessao>)
                .map_err(|e| e.to_string()),
            Self::RustAnalyzer => crate::rust_analyzer::abrir(alvo, amb),
            Self::Tsserver => crate::tsserver::abrir(alvo, amb),
            Self::DenoLsp => crate::deno::abrir(alvo, amb),
        }
    }

    /// Does a session of this verifier take long enough to start that it must not run on the hook's
    /// clock? Pyright cold-starts in ~0.4 s (kept synchronous, as it always was); the others load a
    /// workspace or a program and warm up in the background.
    pub(crate) fn arranque(self) -> Arranque {
        match self {
            Self::Pyright => Arranque::Sincrono,
            Self::RustAnalyzer | Self::Tsserver | Self::DenoLsp => Arranque::SegundoPlano,
        }
    }
}

/// Where a session's start-up runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Arranque {
    /// Inside the first check, under the slot lock (cheap servers).
    Sincrono,
    /// On a detached thread; checks answer `unknown: warming up` until the session is ready.
    SegundoPlano,
}

/// One edit to check (everything but the project root).
pub(crate) struct Edicao<'a> {
    /// Project-relative path, or `<string>`/`<stdin>` for an inline program.
    pub(crate) path: &'a str,
    /// The proposed content.
    pub(crate) content: &'a str,
    /// The file's current content on disk, for the proposed − disk delta.
    pub(crate) disk: Option<&'a str>,
    /// `(path, content)` of other modules whose in-memory version imports must resolve against.
    pub(crate) overlays: &'a [(String, String)],
}

/// What a check produced and whether the session that produced it is still worth keeping.
pub(crate) struct Verificado {
    /// The outcome handed to the hook.
    pub(crate) resultado: L2Resultado,
    /// `false` when the session could not be trusted any more (dead server, deadline overrun): the
    /// pool drops it and the next use recreates it. `true` for an `unknown` that is about the *file*
    /// (rust-analyzer: not part of any crate; tsserver: outside every `tsconfig.json`) — throwing a
    /// warm workspace away for that would be paying minutes for nothing.
    pub(crate) saudavel: bool,
}

impl Verificado {
    /// A result whose health follows its verdict: any `unknown` condemns the session (the pyright rule).
    pub(crate) fn pelo_veredito(resultado: L2Resultado) -> Self {
        let saudavel = resultado.verdict != crate::achado::Verdict::Unknown;
        Self {
            resultado,
            saudavel,
        }
    }
}

/// A live verifier bound to one project. The pool owns it behind a lock, one check at a time.
pub(crate) trait Sessao: Send {
    /// Checks one edit within `prazo` (`None` = the session's own per-check budget). Never panics;
    /// whatever prevents a measurement is an `unknown` with the reason.
    fn verificar(&mut self, edicao: &Edicao<'_>, prazo: Option<Duration>) -> Verificado;

    /// Spends a long budget getting ready (workspace load, first program build) on the first edit;
    /// the result of that check is discarded. `Err(reason)` = the session is not usable.
    fn aquecer(&mut self, edicao: &Edicao<'_>, prazo: Duration) -> Result<(), String> {
        let verificado = self.verificar(edicao, Some(prazo));
        if verificado.saudavel {
            Ok(())
        } else {
            Err(verificado
                .resultado
                .motivo
                .unwrap_or_else(|| "a sessão não ficou utilizável".to_string()))
        }
    }
}

/// What the pool hands a spawner: which verifier, for which project, because of which file.
pub(crate) struct Alvo<'a> {
    pub(crate) servidor: Servidor,
    pub(crate) raiz: &'a Path,
    /// Project-relative path of the edit that asked for the session (where a verifier is searched from).
    pub(crate) arquivo: &'a str,
}

// ── process environment seen by discovery ─────────────────────────────────

/// The `PATH` directories discovery may use to find the *runtime* of a verifier (`node`, `rustup`,
/// `deno`) — never the verifier of a project itself, which is looked up inside the project. Injected so
/// the tests can prove that a "global" binary on `PATH` is not picked in place of the project's.
#[derive(Debug, Clone)]
pub(crate) struct Ambiente {
    path: Vec<PathBuf>,
}

impl Ambiente {
    /// The `PATH` of this process.
    pub(crate) fn do_processo() -> Self {
        Self {
            path: std::env::var_os("PATH")
                .map(|p| std::env::split_paths(&p).collect())
                .unwrap_or_default(),
        }
    }

    /// An environment whose `PATH` is exactly `dirs`.
    #[cfg(test)]
    pub(crate) fn com_path(dirs: Vec<PathBuf>) -> Self {
        Self { path: dirs }
    }

    /// The first executable regular file called `nome` on `PATH`.
    pub(crate) fn achar(&self, nome: &str) -> Option<PathBuf> {
        self.path
            .iter()
            .map(|dir| dir.join(nome))
            .find(|candidato| e_executavel(candidato))
    }
}

#[cfg(unix)]
fn e_executavel(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.is_file()
        && path
            .metadata()
            .is_ok_and(|meta| meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn e_executavel(path: &Path) -> bool {
    path.is_file()
}

// ── project markers ───────────────────────────────────────────────────────

/// `path` with `.` and `..` resolved lexically (no filesystem access, no symlink resolution).
fn limpar(path: &Path) -> PathBuf {
    let mut limpo = PathBuf::new();
    for componente in path.components() {
        match componente {
            Component::ParentDir => {
                limpo.pop();
            }
            Component::CurDir => {}
            outro => limpo.push(outro.as_os_str()),
        }
    }
    limpo
}

/// The directories from the one holding `caminho` up to `raiz`, inclusive, nearest first. An inline
/// `<string>`, an absolute path outside the project and a `..` that climbs out of it all start (and
/// stop) at `raiz` itself: discovery never reads above the project root.
pub(crate) fn dirs_ate_a_raiz(raiz: &Path, caminho: &str) -> Vec<PathBuf> {
    let inicio = if caminho.starts_with('<') {
        raiz.to_path_buf()
    } else {
        limpar(&raiz.join(caminho))
            .parent()
            .map_or_else(|| raiz.to_path_buf(), Path::to_path_buf)
    };
    if !inicio.starts_with(raiz) {
        return vec![raiz.to_path_buf()];
    }
    inicio
        .ancestors()
        .take_while(|dir| dir.starts_with(raiz))
        .map(Path::to_path_buf)
        .collect()
}

/// Is the nearest project marker above the file a Deno one? `deno.json[c]` wins over `package.json`/
/// `tsconfig.json` in the same directory (Deno 2 projects keep both); a closer Node marker wins over
/// a farther `deno.json`; no marker at all is not Deno.
pub(crate) fn projeto_deno(raiz: &Path, caminho: &str) -> bool {
    for dir in dirs_ate_a_raiz(raiz, caminho) {
        if ["deno.json", "deno.jsonc"]
            .iter()
            .any(|marker| dir.join(marker).is_file())
        {
            return true;
        }
        if ["package.json", "tsconfig.json", "jsconfig.json"]
            .iter()
            .any(|marker| dir.join(marker).is_file())
        {
            return false;
        }
    }
    false
}

/// Does `path` end in one of `extensoes` (without the dot, ASCII case-insensitive)?
pub(crate) fn tem_extensao(path: &str, extensoes: &[&str]) -> bool {
    Path::new(path)
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|ext| extensoes.iter().any(|e| e.eq_ignore_ascii_case(ext)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::Arvore;
    use std::fs;

    #[test]
    fn names_round_trip_and_are_unique() {
        for s in Servidor::TODOS {
            assert_eq!(Servidor::do_nome(s.nome()), Some(s));
        }
        assert_eq!(
            Servidor::do_nome(" Rust-Analyzer "),
            Some(Servidor::RustAnalyzer)
        );
        assert_eq!(Servidor::do_nome("pyrite"), None);
        let nomes: std::collections::HashSet<_> =
            Servidor::TODOS.iter().map(|s| s.nome()).collect();
        assert_eq!(nomes.len(), Servidor::TODOS.len());
    }

    #[test]
    fn lang_table_maps_every_layer_one_language_or_none() {
        let t = Arvore::nova("lang");
        let cases = [
            ("python", Some(Servidor::Pyright)),
            ("rust", Some(Servidor::RustAnalyzer)),
            ("typescript", Some(Servidor::Tsserver)),
            ("tsx", Some(Servidor::Tsserver)),
            ("javascript", Some(Servidor::Tsserver)),
            ("html", None),
            ("css", None),
            ("cobol", None),
        ];
        for (lang, esperado) in cases {
            assert_eq!(
                Servidor::para_lang(lang, &t.raiz, "a/b"),
                esperado,
                "{lang}"
            );
        }
    }

    #[test]
    fn a_deno_json_above_the_file_routes_typescript_to_deno() {
        let t = Arvore::nova("deno");
        t.arquivo("deno.json", "{}");
        t.arquivo("src/a.ts", "");
        for lang in ["typescript", "tsx", "javascript"] {
            assert_eq!(
                Servidor::para_lang(lang, &t.raiz, "src/a.ts"),
                Some(Servidor::DenoLsp),
                "{lang}"
            );
        }
        // python and rust are not affected by a deno.json
        assert_eq!(
            Servidor::para_lang("rust", &t.raiz, "src/a.rs"),
            Some(Servidor::RustAnalyzer)
        );
    }

    #[test]
    fn the_nearest_marker_decides_between_deno_and_node() {
        let t = Arvore::nova("near");
        t.arquivo("deno.json", "{}");
        t.arquivo("web/package.json", "{}");
        t.arquivo("web/src/a.ts", "");
        t.arquivo("edge/src/b.ts", "");
        t.arquivo("both/deno.json", "{}");
        t.arquivo("both/package.json", "{}");
        assert!(
            !projeto_deno(&t.raiz, "web/src/a.ts"),
            "package.json is closer"
        );
        assert!(
            projeto_deno(&t.raiz, "edge/src/b.ts"),
            "only the root deno.json above"
        );
        assert!(
            projeto_deno(&t.raiz, "both/x.ts"),
            "deno.json wins in the same directory"
        );
        assert!(
            projeto_deno(&t.raiz, "<string>"),
            "inline starts at the root"
        );
    }

    #[test]
    fn no_marker_at_all_is_not_deno() {
        let t = Arvore::nova("nomark");
        assert!(!projeto_deno(&t.raiz, "x/y.ts"));
        assert_eq!(
            Servidor::para_lang("typescript", &t.raiz, "x/y.ts"),
            Some(Servidor::Tsserver)
        );
    }

    #[test]
    fn dirs_walk_from_the_file_up_to_the_root_and_never_past_it() {
        let t = Arvore::nova("walk");
        let dirs = dirs_ate_a_raiz(&t.raiz, "a/b/c.ts");
        assert_eq!(
            dirs,
            vec![t.raiz.join("a/b"), t.raiz.join("a"), t.raiz.clone()]
        );
        assert_eq!(dirs_ate_a_raiz(&t.raiz, "<stdin>"), vec![t.raiz.clone()]);
        // a path that escapes the root — with `..` or by being absolute elsewhere — is clamped to the
        // root itself, never above it
        assert_eq!(
            dirs_ate_a_raiz(&t.raiz, "../fora/x.ts"),
            vec![t.raiz.clone()]
        );
        assert_eq!(
            dirs_ate_a_raiz(&t.raiz, "a/../../fora/x.ts"),
            vec![t.raiz.clone()]
        );
        assert_eq!(
            dirs_ate_a_raiz(&t.raiz, "/etc/passwd"),
            vec![t.raiz.clone()]
        );
        // `..` that stays inside is resolved
        assert_eq!(
            dirs_ate_a_raiz(&t.raiz, "a/b/../c.ts"),
            vec![t.raiz.join("a"), t.raiz.clone()]
        );
    }

    #[cfg(unix)]
    #[test]
    fn ambiente_finds_only_executable_files_in_order() {
        use std::os::unix::fs::PermissionsExt;
        let t = Arvore::nova("amb");
        let a = t.arquivo("a/tool", "#!/bin/sh\n");
        let b = t.arquivo("b/tool", "#!/bin/sh\n");
        fs::set_permissions(&b, fs::Permissions::from_mode(0o755)).unwrap();
        fs::set_permissions(&a, fs::Permissions::from_mode(0o644)).unwrap();
        let amb = Ambiente::com_path(vec![t.raiz.join("a"), t.raiz.join("b")]);
        assert_eq!(
            amb.achar("tool"),
            Some(b),
            "the non-executable `a/tool` is skipped"
        );
        assert_eq!(amb.achar("outro"), None);
        assert_eq!(Ambiente::com_path(Vec::new()).achar("tool"), None);
    }

    #[test]
    fn extension_matching_is_case_insensitive_and_needs_a_dot() {
        assert!(tem_extensao("a/B.TSX", &["tsx"]));
        assert!(tem_extensao("x.mjs", &["js", "mjs"]));
        assert!(!tem_extensao("tsx", &["tsx"]));
        assert!(!tem_extensao("a.ts.bak", &["ts"]));
    }

    #[test]
    fn only_pyright_starts_synchronously() {
        for s in Servidor::TODOS {
            let esperado = if s == Servidor::Pyright {
                Arranque::Sincrono
            } else {
                Arranque::SegundoPlano
            };
            assert_eq!(s.arranque(), esperado, "{s:?}");
        }
    }
}
