//! VGP layer 2 — the project's own type checker (`pyright-langserver`) as a persistent verifier.
//!
//! Layer 1 (native, tree-sitter + index) answers "does this name exist in that module?" without any
//! dependency. Layer 2 asks the checker the project already ships in its venv, **never installing
//! it**: when the venv has no `pyright-langserver` the layer simply is not there and says why.
//! A langserver (not the CLI) because the proposed content enters memory through `didOpen` — the
//! disk is never touched — imports resolve against the real project, and the process start-up is
//! paid once instead of ~1 s per edit.
//!
//! The reference implementation is the measured Python probe `sonda_camada2_pyright.py` (V3 spec,
//! 09/10/2026); [`negacao`], [`escopo`] and [`severidade`] are its ports and keep its test cases.
//! The output type is neutral ([`L2Resultado`]); converting it to the engine's own `Finding` is the
//! consumer's job, which keeps `touring-code` free of a dependency on this crate.
//!
//! ## What denies and what only warns
//!
//! Only high-precision diagnostics become findings ([`negacao`]): an undefined name, a keyword the
//! callee does not take, an attribute missing from a *module*, a name missing from an import, a
//! member missing from a *concrete* class. Receivers pyright could not infer (`X | None`,
//! `dict[Unknown, Unknown]`) are not findings at all. Whether a finding blocks is the one pure
//! function [`severidade`]: code that runs *now* (inline, `/tmp`) denies; a project file denies only
//! when the missing target is external (stdlib/third-party *module*), because an unfinished edit of
//! the project's own code is exactly what a half-written file looks like (measured: 10/259 negative
//! entries carried a real finding, all intermediate states). A missing member of a builtin *value
//! type* (`int.add`, `str.get`) is `forma` and only ever warns: it is a question about the shape of
//! the data, which the checker's inference cannot answer. So is a `dinamico` one — an undefined name
//! under `from <m> import *`, or a dunder attribute such as `__wrapped__` — because the name space is
//! unknowable statically.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde_json::{Map, Value, json};
use thiserror::Error;

pub use crate::achado::{
    Balde, Escopo, L2Achado, L2Resultado, Severidade, Verdict, file_uri, severidade,
};
use crate::client::ClientError;
#[cfg(test)]
use crate::client::LspClient;
use crate::lsp_nucleo::{
    EscolhaDeModo, Inicio, Modo, Normalizador, NucleoLsp, Pedido, Perfil, inline_path_ext,
};
use crate::servidor::{Edicao, Sessao, Verificado};

/// Per-`check` budget: the V3 target is p95 ≤ 1 s per edit, and exceeding it is `unknown`, never allow.
const DEFAULT_BUDGET: Duration = Duration::from_millis(1000);
/// The cold start (node + project analysis set-up) is paid once, at `discover`.
const STARTUP_BUDGET: Duration = Duration::from_secs(15);
const LANGUAGE_ID: &str = "python";
/// Language servers looked up, in order of preference.
const SERVERS: [&str; 2] = ["pyright-langserver", "basedpyright-langserver"];
/// Venv directory names looked up, in order of preference.
const VENV_DIRS: [&str; 2] = [".venv", "venv"];

const NEGA_MODULO: [&str; 2] = [
    "is not a known attribute of module",
    "is unknown import symbol",
];
const UNKNOWN_IMPORT: &str = "is unknown import symbol";
const MODULE_ATTRIBUTE: &str = "is not a known attribute of module";
const OF_MODULE: &str = "of module \"";
const ACCESS_PREFIX: &str = "Cannot access attribute \"";
const FOR_CLASS: &str = "\" for class \"";
/// Receivers pyright could not infer: an attribute error on them is incomplete inference, not a fact.
const INFERENCIA_INCOMPLETA: [&str; 2] = ["None", "Unknown"];

/// Names of the builtin *types* of CPython 3.12 (`dir(builtins)` filtered by `isinstance(_, type)`),
/// sorted for `binary_search`. A missing attribute on one of these is about a stdlib type: external.
const BUILTIN_CLASSES: [&str; 96] = [
    "ArithmeticError",
    "AssertionError",
    "AttributeError",
    "BaseException",
    "BaseExceptionGroup",
    "BlockingIOError",
    "BrokenPipeError",
    "BufferError",
    "BytesWarning",
    "ChildProcessError",
    "ConnectionAbortedError",
    "ConnectionError",
    "ConnectionRefusedError",
    "ConnectionResetError",
    "DeprecationWarning",
    "EOFError",
    "EncodingWarning",
    "EnvironmentError",
    "Exception",
    "ExceptionGroup",
    "FileExistsError",
    "FileNotFoundError",
    "FloatingPointError",
    "FutureWarning",
    "GeneratorExit",
    "IOError",
    "ImportError",
    "ImportWarning",
    "IndentationError",
    "IndexError",
    "InterruptedError",
    "IsADirectoryError",
    "KeyError",
    "KeyboardInterrupt",
    "LookupError",
    "MemoryError",
    "ModuleNotFoundError",
    "NameError",
    "NotADirectoryError",
    "NotImplementedError",
    "OSError",
    "OverflowError",
    "PendingDeprecationWarning",
    "PermissionError",
    "ProcessLookupError",
    "RecursionError",
    "ReferenceError",
    "ResourceWarning",
    "RuntimeError",
    "RuntimeWarning",
    "StopAsyncIteration",
    "StopIteration",
    "SyntaxError",
    "SyntaxWarning",
    "SystemError",
    "SystemExit",
    "TabError",
    "TimeoutError",
    "TypeError",
    "UnboundLocalError",
    "UnicodeDecodeError",
    "UnicodeEncodeError",
    "UnicodeError",
    "UnicodeTranslateError",
    "UnicodeWarning",
    "UserWarning",
    "ValueError",
    "Warning",
    "ZeroDivisionError",
    "__loader__",
    "bool",
    "bytearray",
    "bytes",
    "classmethod",
    "complex",
    "dict",
    "enumerate",
    "filter",
    "float",
    "frozenset",
    "int",
    "list",
    "map",
    "memoryview",
    "object",
    "property",
    "range",
    "reversed",
    "set",
    "slice",
    "staticmethod",
    "str",
    "super",
    "tuple",
    "type",
    "zip",
];

/// Why a session could not be started — the layer then does not run (fail-open), with this reason.
#[derive(Debug, Error)]
pub enum PyrightError {
    /// The project venv has no language server. Touring never installs one.
    #[error(
        "no pyright-langserver in the venv of {raiz} (searched: {procurados}); Touring never installs it"
    )]
    NaoEncontrado {
        /// The project root that was searched.
        raiz: String,
        /// The relative paths that were looked up.
        procurados: String,
    },
    /// The server was found but did not start or complete `initialize`.
    #[error("pyright-langserver failed to start: {0}")]
    Inicio(#[from] ClientError),
}

/// A language server found in a project venv.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerBinary {
    /// The `*-langserver` executable.
    pub path: PathBuf,
    /// The venv's interpreter (`<venv>/bin/python`), told to the server as `pythonPath`.
    pub python: PathBuf,
}

// ── discovery ──────────────────────────────────────────────────────────────

/// Looks for a language server in the project venv: `pyright-langserver` in `.venv/` then `venv/`,
/// then `basedpyright-langserver` in the same order. Pure lookup — nothing is installed or run.
pub fn locate_server(root: &Path) -> Option<ServerBinary> {
    for server in SERVERS {
        for venv in VENV_DIRS {
            let bin_dir = root.join(venv).join("bin");
            let path = bin_dir.join(server);
            if path.is_file() {
                return Some(ServerBinary {
                    python: bin_dir.join("python"),
                    path,
                });
            }
        }
    }
    None
}

fn searched() -> String {
    let mut all = Vec::new();
    for server in SERVERS {
        for venv in VENV_DIRS {
            all.push(format!("{venv}/bin/{server}"));
        }
    }
    all.join(", ")
}

// ── diagnostic → finding (ports of the measured probe) ─────────────────────

fn regra(d: &Value) -> &str {
    d.get("code").and_then(Value::as_str).unwrap_or("")
}

fn mensagem(d: &Value) -> &str {
    d.get("message").and_then(Value::as_str).unwrap_or("")
}

fn linha0(d: &Value) -> Option<u64> {
    d.pointer("/range/start/line").and_then(Value::as_u64)
}

/// The first non-empty `"quoted"` run — the regex `"([^"]+)"`.
fn first_quoted(s: &str) -> Option<&str> {
    let mut rest = s;
    while let Some(open) = rest.find('"') {
        let after = &rest[open + 1..];
        let close = after.find('"')?;
        if close > 0 {
            return Some(&after[..close]);
        }
        rest = after; // `""`: the regex retries from the second quote
    }
    None
}

/// The non-empty quoted run right after `prefix` — the regex `<prefix>([^"]+)"`.
fn quoted_after<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = &s[s.find(prefix)? + prefix.len()..];
    let end = rest.find('"')?;
    (end > 0).then(|| &rest[..end])
}

/// The class of `Cannot access attribute "x" for class "C"`.
fn classe_concreta(msg: &str) -> Option<&str> {
    msg.match_indices(ACCESS_PREFIX)
        .find_map(|(i, prefix)| class_after_attribute(&msg[i + prefix.len()..]))
}

fn class_after_attribute(rest: &str) -> Option<&str> {
    let attr_end = rest.find('"')?;
    if attr_end == 0 {
        return None;
    }
    let tail = rest[attr_end..].strip_prefix(FOR_CLASS)?;
    let class_end = tail.find('"')?;
    (class_end > 0).then(|| &tail[..class_end])
}

/// The receiver class a member diagnostic names — `Cannot access attribute "x" for class "C"` →
/// `C` (possibly qualified: `os.stat_result`). `None` when the message names no class, or a
/// non-simple one (`X | None`, `dict[…]`: an incomplete inference never identifies a receiver).
///
/// The class the merge with layer 1 compares ([`Escopo::Tipo`] findings carry it so that two layers
/// naming the SAME closed type corroborate each other). It is the one parser of the message: the same
/// [`classe_concreta`] that decides the scope of the finding.
pub fn classe_receptora(mensagem: &str) -> Option<String> {
    classe_concreta(mensagem)
        .filter(|c| is_simple_name(c))
        .map(str::to_string)
}

/// A plain dotted identifier — `^[A-Za-z_][A-Za-z0-9_.]*$`. `X | None`, `dict[…]`, `type[Foo]` fail.
fn is_simple_name(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

/// The name a high-precision diagnostic denies; `None` when it only warrants a warning or nothing.
///
/// Denied forms: `reportUndefinedVariable`; `reportCallIssue` starting `No parameter named`;
/// `reportAttributeAccessIssue` on a module or an import symbol, or on a **concrete** class (a plain
/// identifier that is not `None`/`Unknown`). A union or generic receiver is incomplete inference.
pub fn negacao(d: &Value) -> Option<String> {
    let msg = mensagem(d);
    let nome = first_quoted(msg)?;
    let nega = match regra(d) {
        "reportUndefinedVariable" => true,
        "reportCallIssue" => msg.starts_with("No parameter named"),
        "reportAttributeAccessIssue" => nega_atributo(msg),
        _ => false,
    };
    nega.then(|| nome.to_string())
}

/// Attribute of a module/import always denies; of a class only when the class is concrete.
fn nega_atributo(msg: &str) -> bool {
    NEGA_MODULO.iter().any(|t| msg.contains(t))
        || classe_concreta(msg)
            .is_some_and(|c| is_simple_name(c) && !INFERENCIA_INCOMPLETA.contains(&c))
}

/// The vocabulary of layer 1 (`import` | `atributo` | `membro` | `kwarg` | `nome`) for a diagnostic.
pub fn classe_do_achado(d: &Value) -> &'static str {
    let msg = mensagem(d);
    match regra(d) {
        "reportUndefinedVariable" => "nome",
        "reportCallIssue" => "kwarg",
        _ if msg.contains(UNKNOWN_IMPORT) => "import",
        _ if msg.contains(MODULE_ATTRIBUTE) => "atributo",
        _ => "membro",
    }
}

/// Whose symbol is missing. When it cannot be told, [`Escopo::Projeto`]: the side that warns.
/// A member of a builtin value type (the last `.` segment of the class is a builtin type) is
/// [`Escopo::Forma`], not external. An undefined name under `from <m> import *`, or a dunder
/// attribute, is [`Escopo::Dinamico`].
///
/// `texto` is the checked content (to read the `from <module> import …` line of an unknown import
/// symbol), `raiz` the project root and `origem` the checked file (anchor of relative imports).
pub fn escopo(d: &Value, texto: &str, raiz: &Path, origem: &Path) -> Escopo {
    if regra(d) == "reportUndefinedVariable" {
        // `from <m> import *` makes the name space unknowable: the checker cannot say the name is missing
        return if has_star_import(texto) {
            Escopo::Dinamico
        } else {
            Escopo::Proprio
        };
    }
    let msg = mensagem(d);
    if let Some(modulo) = modulo_citado(d, msg, texto) {
        return if modulo_do_projeto(&modulo, raiz, origem) {
            Escopo::Projeto
        } else {
            Escopo::Externo
        };
    }
    escopo_de_membro(msg)
}

/// Scope of `Cannot access attribute "x" for class "C"`. The order matters: a dunder attribute is
/// `dinamico` even on a builtin class, and only then is a builtin class `forma`. Any other concrete
/// class is `tipo` (P0B (b)): the message is about the receiver's INFERRED type, so the member may
/// still exist on the object that arrives at run time — only a module's missing attribute is an
/// existence claim. A message that names no concrete class keeps the conservative `projeto`.
fn escopo_de_membro(msg: &str) -> Escopo {
    let Some(classe) = classe_concreta(msg) else {
        return Escopo::Projeto;
    };
    if first_quoted(msg).is_some_and(is_dunder) {
        Escopo::Dinamico
    } else if is_builtin_class(classe) {
        Escopo::Forma
    } else {
        Escopo::Tipo
    }
}

/// P0B (b): a PRIVATE (`_x`) or DUNDER (`__x__`) member of an EXTERNAL module is not something the
/// checker can call missing. Stubs omit private members (`ssl._ssl._test_decode_cert` exists at run
/// time) and packages serve dunders lazily through PEP 562 `__getattr__`
/// (`jsonschema.__version__`) or the import system (`folium.__file__`). Layer 1 reads the real source:
/// when it confirms the absence its own finding stands in the merge; without that confirmation the
/// extra layer abstains. The module's own public names keep denying (`os.pathx`, `json.loadz`), and a
/// project module is not external (layer 1 reads it too, but nothing here is "third party").
fn membro_nao_publico_de_modulo_externo(d: &Value, nome: &str, escopo: Escopo) -> bool {
    let msg = mensagem(d);
    escopo == Escopo::Externo
        && nome.starts_with('_')
        && (msg.contains(MODULE_ATTRIBUTE) || msg.contains(UNKNOWN_IMPORT))
}

/// `^__\w+__$`: a protocol attribute (`__wrapped__` of `functools.wraps`, `__self__`, …) that a
/// decorator, descriptor or C extension creates at run time and the static checker does not model.
fn is_dunder(name: &str) -> bool {
    name.len() >= 5
        && name.starts_with("__")
        && name.ends_with("__")
        && name[2..name.len() - 2].chars().all(is_word_char)
}

/// Does any line read `from <module> import *` (the regex `^\s*from\s+\S+\s+import\s+\*`, per line)?
fn has_star_import(texto: &str) -> bool {
    texto.lines().any(is_star_import_line)
}

fn is_star_import_line(line: &str) -> bool {
    let Some(rest) = line.trim_start().strip_prefix("from") else {
        return false;
    };
    skip_required_space(rest)
        .and_then(skip_token)
        .and_then(skip_required_space)
        .and_then(|after_module| after_module.strip_prefix("import"))
        .and_then(skip_required_space)
        .is_some_and(|after_import| after_import.starts_with('*'))
}

/// `s` without its leading non-whitespace run (`\S+`); `None` when the run is empty.
fn skip_token(s: &str) -> Option<&str> {
    let end = s.find(char::is_whitespace).unwrap_or(s.len());
    (end > 0).then(|| &s[end..])
}

fn is_builtin_class(classe: &str) -> bool {
    let last = classe.rsplit('.').next().unwrap_or(classe);
    BUILTIN_CLASSES.binary_search(&last).is_ok()
}

/// The module a diagnostic points at: `of module "m"`, or — for an unknown import symbol, whose
/// message names no module — the `from <module> import` of the diagnosed line.
fn modulo_citado(d: &Value, msg: &str, texto: &str) -> Option<String> {
    if let Some(modulo) = quoted_after(msg, OF_MODULE) {
        return Some(modulo.to_string());
    }
    if !msg.contains(UNKNOWN_IMPORT) {
        return None;
    }
    let line = texto.lines().nth(usize::try_from(linha0(d)?).ok()?)?;
    modulo_do_from(line)
}

/// `from <module> import …` → `<module>` (leading dots of a relative import kept).
fn modulo_do_from(line: &str) -> Option<String> {
    let rest = skip_required_space(line.trim_start().strip_prefix("from")?)?;
    let module_len = rest
        .find(|c: char| !(c == '.' || is_word_char(c)))
        .unwrap_or(rest.len());
    let (module, tail) = rest.split_at(module_len);
    let after_import = skip_required_space(tail)?.strip_prefix("import")?;
    let word_boundary = after_import.chars().next().is_none_or(|c| !is_word_char(c));
    word_boundary.then(|| module.to_string())
}

/// `s` without its leading whitespace, or `None` when there was none (`fromage = 1` is not an import).
fn skip_required_space(s: &str) -> Option<&str> {
    let trimmed = s.trim_start();
    (trimmed.len() < s.len()).then_some(trimmed)
}

fn is_word_char(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// Does the module live under the project root (absolute, or relative to the checked file)?
///
/// Decided by a file actually existing — `<module>.py` or `<module>/__init__.py` — and being inside
/// the root once symlinks are resolved. The first base where the module is found decides.
pub fn modulo_do_projeto(modulo: &str, raiz: &Path, origem: &Path) -> bool {
    let bare = modulo.trim_start_matches('.');
    let nivel = modulo.len() - bare.len();
    let partes: Vec<&str> = bare.split('.').filter(|p| !p.is_empty()).collect();
    let Ok(raiz_real) = raiz.canonicalize() else {
        return false;
    };
    bases(nivel, raiz, origem)
        .iter()
        .find_map(|base| modulo_em(base, &partes, &raiz_real))
        .unwrap_or(false)
}

/// Directories a module path is resolved against: the root and the file's directory for an absolute
/// import; the directory `nivel - 1` levels above the file for a relative one.
fn bases(nivel: usize, raiz: &Path, origem: &Path) -> Vec<PathBuf> {
    let pai = origem.parent().unwrap_or_else(|| Path::new("."));
    if nivel == 0 {
        return vec![raiz.to_path_buf(), pai.to_path_buf()];
    }
    let climbed = (1..nivel).fold(pai.to_path_buf(), |dir, _| dir.join(".."));
    climbed.canonicalize().into_iter().collect()
}

/// `Some(inside_root)` when the module exists under `base`, `None` when it does not.
fn modulo_em(base: &Path, partes: &[&str], raiz_real: &Path) -> Option<bool> {
    let found = arquivo_modulo(base, partes)?;
    Some(
        found
            .canonicalize()
            .is_ok_and(|real| real.starts_with(raiz_real)),
    )
}

/// The real Python source of a module, never its stub: `<module>.py` or `<module>/__init__.py`.
fn arquivo_modulo(base: &Path, partes: &[&str]) -> Option<PathBuf> {
    let alvo = partes.iter().fold(base.to_path_buf(), |dir, p| dir.join(p));
    [alvo.with_extension("py"), alvo.join("__init__.py")]
        .into_iter()
        .find(|candidate| candidate.is_file())
}

/// P0B2: pyright's stub can lag the runtime module in two ways that are not safe to deny. A package
/// may expose a submodule only after another import (`urllib.error` via `urllib.request`), and a
/// dynamic module may define a name omitted by the stub (`PIL.Image.LANCZOS`). Abstain only on
/// positive filesystem/source evidence; an actually missing public attribute keeps denying.
fn atributo_de_modulo_incerto(d: &Value, nome: &str, raiz: &Path, origem: &Path) -> bool {
    let msg = mensagem(d);
    if regra(d) != "reportAttributeAccessIssue" || !msg.contains(MODULE_ATTRIBUTE) {
        return false;
    }
    if nome.is_empty() || !nome.chars().all(is_word_char) {
        return false;
    }
    let Some(modulo) = modulo_citado(d, msg, "") else {
        return false;
    };
    let partes: Vec<&str> = modulo.split('.').filter(|p| !p.is_empty()).collect();
    if partes.is_empty() {
        return false;
    }
    let raizes = raizes_de_busca(raiz, origem);
    evidencia_de_modulo(&raizes, &partes, nome)
}

fn evidencia_de_modulo(raizes: &[PathBuf], partes: &[&str], nome: &str) -> bool {
    let Some(base) = raizes
        .iter()
        .find(|base| arquivo_modulo(base, partes).is_some())
    else {
        return false;
    };
    let mut submodulo = partes.to_vec();
    submodulo.push(nome);
    if arquivo_modulo(base, &submodulo).is_some() {
        return true;
    }
    arquivo_modulo(base, partes)
        .and_then(|fonte| std::fs::read_to_string(fonte).ok())
        .is_some_and(|fonte| fonte_define_nome(&fonte, nome))
}

/// Import roots of the selected venv, its matching base interpreter, and the project itself. The
/// venv's `lib/pythonX.Y` fixes the base version, so another Python installed under the same prefix
/// cannot supply the evidence.
fn raizes_de_busca(raiz: &Path, origem: &Path) -> Vec<PathBuf> {
    let mut raizes = bases(0, raiz, origem);
    if let Some(venv) = venv_ativo(raiz) {
        let mut locais = Vec::new();
        let mut versoes = Vec::new();
        for lib in [venv.join("lib"), venv.join("lib64")] {
            acrescenta_raizes_python(&mut locais, &lib, &mut versoes);
        }
        if let Some((prefixo, pacotes_sistema)) = configuracao_python(&venv) {
            let mut globais = Vec::new();
            versoes.sort();
            versoes.dedup();
            for versao in versoes {
                for lib in [prefixo.join("lib"), prefixo.join("lib64")] {
                    let dir = lib.join(&versao);
                    raizes.push(dir.clone());
                    if pacotes_sistema {
                        globais.push(dir.join("site-packages"));
                        globais.push(dir.join("dist-packages"));
                    }
                }
            }
            // CPython: base stdlib → venv site-packages → system site-packages.
            raizes.extend(locais);
            raizes.extend(globais);
        } else {
            raizes.extend(locais);
        }
    }
    raizes
}

/// The venv selected by [`locate_server`], or the first conventional one in test/client sessions.
fn venv_ativo(raiz: &Path) -> Option<PathBuf> {
    if let Some(server) = locate_server(raiz)
        && let Some(venv) = server.python.parent().and_then(|bin| bin.parent())
    {
        return Some(venv.to_path_buf());
    }
    VENV_DIRS
        .iter()
        .map(|nome| raiz.join(nome))
        .find(|candidato| candidato.is_dir())
}

fn configuracao_python(venv: &Path) -> Option<(PathBuf, bool)> {
    let cfg = std::fs::read_to_string(venv.join("pyvenv.cfg")).ok()?;
    let mut home = None;
    let mut pacotes_sistema = false;
    for linha in cfg.lines() {
        let Some((chave, valor)) = linha.split_once('=') else {
            continue;
        };
        match chave.trim() {
            "home" => home = Some(PathBuf::from(valor.trim())),
            "include-system-site-packages" => {
                pacotes_sistema = valor.trim().eq_ignore_ascii_case("true");
            }
            _ => {}
        }
    }
    let prefixo = home?.parent()?.to_path_buf();
    Some((prefixo, pacotes_sistema))
}

fn acrescenta_raizes_python(raizes: &mut Vec<PathBuf>, lib: &Path, versoes: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(lib) else {
        return;
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .and_then(|nome| nome.to_str())
                    .is_some_and(|nome| nome.starts_with("python"))
        })
        .collect();
    dirs.sort();
    for dir in dirs {
        if let Some(versao) = dir.file_name().and_then(|nome| nome.to_str()) {
            versoes.push(versao.to_string());
        }
        raizes.push(dir.clone());
        raizes.push(dir.join("site-packages"));
        raizes.push(dir.join("dist-packages"));
    }
}

/// Direct top-level evidence, or dynamic module publication together with the name as a real
/// identifier. The second condition prevents an unrelated `setattr` loop from hiding a typo.
fn fonte_define_nome(fonte: &str, nome: &str) -> bool {
    let codigo = codigo_sem_comentarios(fonte);
    let definicao_direta = codigo.lines().any(|linha| linha_define_nome(linha, nome));
    definicao_direta || (publicacao_dinamica(&codigo) && contem_identificador(&codigo, nome))
}

/// Source outside comments and triple-quoted strings, preserving ordinary strings for recognizing
/// `sys.modules[__name__]`. Single-line strings are handled separately by `contem_identificador`.
fn codigo_sem_comentarios(fonte: &str) -> String {
    let mut em_multilinha: Option<&str> = None;
    let mut limpo = String::with_capacity(fonte.len());
    for linha in fonte.lines() {
        let comentario = linha.split('#').next().unwrap_or("");
        let Some(codigo) = codigo_apos_fecho(comentario, &mut em_multilinha) else {
            limpo.push('\n');
            continue;
        };
        if let Some((inicio, delimitador)) = delimitador_multilinha(codigo) {
            limpo.push_str(&codigo[..inicio]);
            let depois = &codigo[inicio + delimitador.len()..];
            if let Some(fim) = ocorrencia_nao_escapada(depois, delimitador) {
                limpo.push_str(&depois[fim + delimitador.len()..]);
            } else {
                em_multilinha = Some(delimitador);
            }
        } else {
            limpo.push_str(codigo);
        }
        limpo.push('\n');
    }
    limpo
}

fn publicacao_dinamica(codigo: &str) -> bool {
    let setattr_modulo = codigo.contains("setattr(sys.modules[__name__]")
        || (codigo.contains("module = sys.modules[__name__]")
            && codigo.contains("setattr(module,"));
    let atualiza_namespace =
        codigo.contains("globals().update(") || codigo.contains("vars().update(");
    let atribuicao_globals = codigo.match_indices("globals()[").any(|(inicio, _)| {
        let resto = &codigo[inicio + "globals()[".len()..];
        resto.find(']').is_some_and(|fim| {
            let depois = resto[fim + 1..].trim_start();
            depois.starts_with('=') && !depois.starts_with("==")
        })
    });
    setattr_modulo || atualiza_namespace || atribuicao_globals
}

/// Identifier occurrence outside ordinary quoted strings, with Python identifier boundaries.
fn contem_identificador(codigo: &str, nome: &str) -> bool {
    if nome.is_empty() {
        return false;
    }
    let mut em_string: Option<char> = None;
    let mut escapado = false;
    let mut inicio = 0;
    while inicio < codigo.len() {
        if consome_string(codigo, &mut inicio, &mut em_string, &mut escapado) {
            continue;
        }
        if codigo[inicio..].starts_with(nome) {
            let antes = codigo[..inicio].chars().next_back();
            let depois = codigo[inicio + nome.len()..].chars().next();
            if antes.is_none_or(|c| !is_word_char(c)) && depois.is_none_or(|c| !is_word_char(c)) {
                return true;
            }
        }
        let c = codigo[inicio..].chars().next().unwrap_or('\0');
        inicio += c.len_utf8();
    }
    false
}

/// Consume one character inside a quoted string, or the quote that opens it. `true` means the
/// caller should skip identifier recognition at this position.
fn consome_string(
    codigo: &str,
    inicio: &mut usize,
    em_string: &mut Option<char>,
    escapado: &mut bool,
) -> bool {
    let c = codigo[*inicio..].chars().next().unwrap_or('\0');
    if let Some(delimitador) = *em_string {
        if *escapado {
            *escapado = false;
        } else if c == '\\' {
            *escapado = true;
        } else if c == delimitador {
            *em_string = None;
        }
        *inicio += c.len_utf8();
        return true;
    }
    if c == '"' || c == '\'' {
        *em_string = Some(c);
        *inicio += c.len_utf8();
        return true;
    }
    false
}

fn codigo_apos_fecho<'a>(codigo: &'a str, em_multilinha: &mut Option<&str>) -> Option<&'a str> {
    let Some(delimitador) = *em_multilinha else {
        return Some(codigo);
    };
    let fim = ocorrencia_nao_escapada(codigo, delimitador)?;
    *em_multilinha = None;
    Some(&codigo[fim + delimitador.len()..])
}

fn ocorrencia_nao_escapada(codigo: &str, delimitador: &str) -> Option<usize> {
    let mut inicio = 0;
    while let Some(pos) = codigo[inicio..].find(delimitador) {
        let encontrado = inicio + pos;
        let barras = codigo.as_bytes()[..encontrado]
            .iter()
            .rev()
            .take_while(|byte| **byte == b'\\')
            .count();
        if barras % 2 == 0 {
            return Some(encontrado);
        }
        inicio = encontrado + delimitador.len();
    }
    None
}

fn delimitador_multilinha(codigo: &str) -> Option<(usize, &'static str)> {
    match (
        ocorrencia_nao_escapada(codigo, "\"\"\""),
        ocorrencia_nao_escapada(codigo, "'''"),
    ) {
        (Some(duplas), Some(simples)) if duplas <= simples => Some((duplas, "\"\"\"")),
        (Some(_), Some(simples)) => Some((simples, "'''")),
        (Some(duplas), None) => Some((duplas, "\"\"\"")),
        (None, Some(simples)) => Some((simples, "'''")),
        (None, None) => None,
    }
}

fn linha_define_nome(codigo: &str, nome: &str) -> bool {
    atribuicao_de_nome(codigo, nome) || getattr_de_modulo(codigo)
}

fn atribuicao_de_nome(codigo: &str, nome: &str) -> bool {
    let Some(resto) = codigo.strip_prefix(nome) else {
        return false;
    };
    let resto = resto.trim_start();
    resto
        .strip_prefix('=')
        .is_some_and(|tail| !tail.starts_with('='))
        || (resto.starts_with(':')
            && resto
                .split_once('=')
                .is_some_and(|(_, tail)| !tail.starts_with('=')))
}

fn getattr_de_modulo(codigo: &str) -> bool {
    let Some(resto) = codigo.strip_prefix("def") else {
        return false;
    };
    let Some(resto) = skip_required_space(resto) else {
        return false;
    };
    resto.strip_prefix("__getattr__").is_some_and(|tail| {
        tail.chars()
            .next()
            .is_none_or(|c| c == '(' || c.is_whitespace())
    })
}

/// The virtual file an inline program (`<string>`, `<stdin>`) is opened as: `<raiz>/.vgp_inline_<hash>.py`.
/// It is **never written to disk** — it exists only as a `didOpen` document, so relative imports and
/// the project's import roots resolve as they would for a script placed there. The hash keys it by
/// `(path, content)`, stable within a process (all it needs to be).
pub fn inline_path(raiz: &Path, path: &str, content: &str) -> PathBuf {
    inline_path_ext(raiz, path, content, "py")
}

// ── the session ────────────────────────────────────────────────────────────

/// How the shared LSP check sees pyright: push diagnostics, no readiness report, `.py` inline files.
static PERFIL: Perfil = Perfil {
    nome: "pyright",
    extensao_inline: "py",
    exige_quiescencia: false,
    language_id: |_| LANGUAGE_ID,
};

/// Turns pyright's diagnostics into findings with the measured ports of this module.
struct PyrightNorm;

impl Normalizador for PyrightNorm {
    fn achados(
        &self,
        diags: &[Value],
        pedido: &Pedido<'_>,
        raiz: &Path,
    ) -> Result<Vec<L2Achado>, String> {
        Ok(achados_de(diags, pedido, raiz))
    }

    /// pyright's baseline keys come straight from [`negacao`] (no scope lookup on the disk text).
    fn chaves(
        &self,
        diags: &[Value],
        _pedido: &Pedido<'_>,
        _raiz: &Path,
    ) -> HashSet<(String, String)> {
        diags
            .iter()
            .filter_map(|d| negacao(d).map(|nome| (regra(d).to_string(), nome)))
            .collect()
    }
}

/// A live `pyright-langserver` bound to one project root. Cheap to call (tens of ms warm), so the
/// owner keeps it for the lifetime of the hook daemon or CLI run; dropping it shuts the server down.
/// It is the pyright instance of the shared LSP check ([`crate::lsp_nucleo`]), always in push mode:
/// the measured behaviour of pyright 1.1.408 does not depend on what a newer version advertises.
pub struct PyrightSession {
    nucleo: NucleoLsp,
}

impl PyrightSession {
    /// Finds the project's language server ([`locate_server`]), starts it with `cwd = root` and
    /// completes `initialize`. The error is the *reason the layer is absent*, ready to be shown.
    pub fn discover(project_root: &Path) -> Result<Self, PyrightError> {
        let root = project_root
            .canonicalize()
            .or_else(|_| std::path::absolute(project_root))
            .unwrap_or_else(|_| project_root.to_path_buf());
        let server = locate_server(&root).ok_or_else(|| PyrightError::NaoEncontrado {
            raiz: root.display().to_string(),
            procurados: searched(),
        })?;
        let mut command = Command::new(&server.path);
        command.arg("--stdio").current_dir(&root);
        let inicio = Inicio {
            comando: command,
            sections: sections(&root, &server.python),
            raiz_lsp: &root,
            capabilities: capabilities(),
            init_options: Value::Null,
            startup: STARTUP_BUDGET,
            modo: EscolhaDeModo::Forcado(Modo::Push),
        };
        let nucleo = NucleoLsp::iniciar(inicio, root.clone(), DEFAULT_BUDGET, &PERFIL)?;
        Ok(Self { nucleo })
    }

    /// Wraps an already-initialized client (the seam tests use to inject a scripted server).
    #[cfg(test)]
    pub(crate) fn from_client(client: LspClient, root: PathBuf, budget: Duration) -> Self {
        Self {
            nucleo: NucleoLsp::de_cliente(client, root, budget, Modo::Push, &PERFIL),
        }
    }

    /// Replaces the per-check budget (default 1000 ms).
    pub fn with_budget(mut self, budget: Duration) -> Self {
        self.nucleo = self.nucleo.com_budget(budget);
        self
    }

    /// The project root the server was started on.
    pub fn root(&self) -> &Path {
        self.nucleo.root()
    }

    /// Checks `content` as the file `path` (`<string>`/`<stdin>` for an inline program).
    ///
    /// * `disk` — the file's current content: findings that already exist there (same rule and
    ///   name) are not the edit's fault and are removed (the delta *proposed − disk*).
    /// * `overlays` — `(path, content)` of other modules whose in-memory version must be what
    ///   imports resolve against (e.g. earlier unsaved edits of the same session); opened before the
    ///   check and closed after it.
    ///
    /// The whole call — overlays, the check and the baseline — shares one budget. When it is
    /// exceeded, or the server is gone, the result is [`Verdict::Unknown`] with the reason.
    pub fn check(
        &mut self,
        path: &str,
        content: &str,
        disk: Option<&str>,
        overlays: &[(String, String)],
    ) -> L2Resultado {
        self.nucleo
            .verificar(
                &PyrightNorm,
                &Edicao {
                    path,
                    content,
                    disk,
                    overlays,
                },
            )
            .resultado
    }
}

impl Sessao for PyrightSession {
    fn verificar(&mut self, edicao: &Edicao<'_>, prazo: Option<Duration>) -> Verificado {
        let resultado = match prazo {
            None => self.nucleo.verificar(&PyrightNorm, edicao),
            Some(prazo) => self.nucleo.verificar_ate(&PyrightNorm, edicao, prazo),
        };
        Verificado::pelo_veredito(resultado.resultado)
    }
}

fn achados_de(diags: &[Value], pedido: &Pedido<'_>, raiz: &Path) -> Vec<L2Achado> {
    diags
        .iter()
        .filter_map(|d| {
            let nome = negacao(d)?;
            let escopo = escopo(d, pedido.content, raiz, &pedido.origem);
            if membro_nao_publico_de_modulo_externo(d, &nome, escopo) {
                return None;
            }
            let escopo = if escopo == Escopo::Externo
                && atributo_de_modulo_incerto(d, &nome, raiz, &pedido.origem)
            {
                Escopo::Dinamico
            } else {
                escopo
            };
            Some(L2Achado {
                classe: classe_do_achado(d).to_string(),
                nome,
                linha: linha0(d).map_or(0, |l| {
                    u32::try_from(l.saturating_add(1)).unwrap_or(u32::MAX)
                }),
                escopo,
                severidade: severidade(pedido.balde, escopo),
                regra: regra(d).to_string(),
                mensagem: mensagem(d).to_string(),
            })
        })
        .collect()
}

/// What the server asks through `workspace/configuration` (spec §2.3): which interpreter, and the
/// analysis settings that make imports resolve against the project and keep the server cheap.
fn sections(root: &Path, python: &Path) -> Map<String, Value> {
    let mut sections = Map::new();
    sections.insert(
        "python".into(),
        json!({"pythonPath": python.to_string_lossy()}),
    );
    sections.insert(
        "python.analysis".into(),
        json!({
            "typeCheckingMode": "standard",
            "diagnosticMode": "openFilesOnly",
            "autoSearchPaths": true,
            "extraPaths": [root.to_string_lossy()],
        }),
    );
    sections
}

/// Client capabilities pyright needs: versioned diagnostics (the match key) and configuration pulls.
fn capabilities() -> Value {
    json!({
        "textDocument": {"publishDiagnostics": {"versionSupport": true}},
        "workspace": {"configuration": true},
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::LspClient;
    use serde_json::{Map, json};
    use std::fs;
    use std::process::Command;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    // ── helpers ────────────────────────────────────────────────────────────

    /// A unique scratch tree removed on drop (no external dep, panic-safe).
    struct TempTree {
        root: PathBuf,
    }

    impl TempTree {
        fn new(tag: &str) -> Self {
            static N: AtomicUsize = AtomicUsize::new(0);
            let root = std::env::temp_dir().join(format!(
                "touring_pyright_{tag}_{}_{}",
                std::process::id(),
                N.fetch_add(1, Ordering::SeqCst)
            ));
            fs::create_dir_all(&root).unwrap();
            Self {
                root: root.canonicalize().unwrap(),
            }
        }

        fn file(&self, rel: &str, body: &str) -> PathBuf {
            let path = self.root.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, body).unwrap();
            path
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn diag(code: &str, message: &str) -> Value {
        json!({"code": code, "message": message,
            "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}}})
    }

    fn attr(message: &str) -> Value {
        diag("reportAttributeAccessIssue", message)
    }

    // ── negacao (the selftest case + the spec §3 table) ────────────────────

    #[test]
    fn negacao_names_the_high_precision_forms() {
        let cases = [
            (diag("reportUndefinedVariable", "\"x\" is not defined"), "x"),
            (diag("reportCallIssue", "No parameter named \"kw\""), "kw"),
            (
                attr("\"combending\" is not a known attribute of module \"unicodedata\""),
                "combending",
            ),
            (attr("\"nome\" is unknown import symbol"), "nome"),
            (
                attr("Cannot access attribute \"get\" for class \"str\""),
                "get",
            ),
            (
                attr("Cannot access attribute \"total\" for class \"app.Recibo\""),
                "total",
            ),
        ];
        for (d, esperado) in cases {
            assert_eq!(negacao(&d).as_deref(), Some(esperado), "{d}");
        }
    }

    #[test]
    fn negacao_union_does_not_deny() {
        // selftest: "negação: união não nega"
        assert_eq!(
            negacao(&attr(
                "Cannot access attribute \"a\" for class \"X | None\""
            )),
            None
        );
    }

    #[test]
    fn negacao_only_warns_on_incomplete_inference_and_other_rules() {
        let cases = [
            attr("Cannot access attribute \"a\" for class \"None\""),
            attr("Cannot access attribute \"a\" for class \"Unknown\""),
            attr("Cannot access attribute \"a\" for class \"dict[Unknown, Unknown]\""),
            attr("Cannot access attribute \"a\" for class \"type[Foo]\""),
            // Pinned measured gaps (pyright 1.1.408): a literal receiver is not a plain class name,
            // and an overloaded callee never says `No parameter named`.
            attr(
                "Cannot access attribute \"a\" for class \"Literal['x']\"\n  Attribute \"a\" is unknown",
            ),
            diag(
                "reportCallIssue",
                "No overloads for \"print\" match the provided arguments\n  Argument types: (Literal[1])",
            ),
            diag("reportCallIssue", "Expected 1 positional argument"),
            diag("reportMissingImports", "Import \"x\" could not be resolved"),
            diag("reportUndefinedVariable", "is not defined, but unquoted"),
            diag("reportArgumentType", "Argument of type \"str\""),
        ];
        for d in cases {
            assert_eq!(negacao(&d), None, "{d}");
        }
    }

    // ── escopo ─────────────────────────────────────────────────────────────

    #[test]
    fn escopo_own_name() {
        // selftest: "escopo: nome próprio"
        let tree = TempTree::new("esc_own");
        let d = diag("reportUndefinedVariable", "\"x\" is not defined");
        assert_eq!(
            escopo(&d, "", &tree.root, &tree.root.join("a.py")),
            Escopo::Proprio
        );
    }

    #[test]
    fn escopo_external_module() {
        // selftest: "escopo: módulo externo"
        let tree = TempTree::new("esc_ext");
        let d = attr("\"combending\" is not a known attribute of module \"unicodedata\"");
        assert_eq!(
            escopo(&d, "", &tree.root, &tree.root.join("a.py")),
            Escopo::Externo
        );
    }

    #[test]
    fn escopo_builtin_class_member_is_forma_even_when_dotted() {
        // selftest: "escopo: membro de classe builtin = forma"
        let tree = TempTree::new("esc_builtin");
        let origem = tree.root.join("a.py");
        let builtins = [
            "int",
            "str",
            "list",
            "dict",
            "tuple",
            "set",
            "frozenset",
            "bytes",
            "float",
            "bool",
            "object",
            "type",
            "ValueError",
            "builtins.str",
            "typing.builtins.list",
        ];
        for classe in builtins {
            let d = attr(&format!(
                "Cannot access attribute \"get\" for class \"{classe}\""
            ));
            assert_eq!(
                escopo(&d, "", &tree.root, &origem),
                Escopo::Forma,
                "{classe}"
            );
        }
    }

    #[test]
    fn escopo_dunder_attribute_is_dinamico_before_the_builtin_test() {
        let tree = TempTree::new("esc_dunder");
        let origem = tree.root.join("a.py");
        let cases = [
            ("__wrapped__", "FixtureFunction"),
            ("__self__", "str"), // a builtin class, but the dunder test comes first
            ("__x_y__", "list"),
            ("_____", "Recibo"), // `^__\w+__$` accepts the 5-underscore minimum
        ];
        for (nome, classe) in cases {
            let d = attr(&format!(
                "Cannot access attribute \"{nome}\" for class \"{classe}\""
            ));
            assert_eq!(
                escopo(&d, "", &tree.root, &origem),
                Escopo::Dinamico,
                "{nome} on {classe}"
            );
        }
    }

    #[test]
    fn escopo_names_that_are_not_dunders_keep_their_class_scope() {
        let tree = TempTree::new("esc_not_dunder");
        let origem = tree.root.join("a.py");
        for nome in [
            "__wrapped",
            "wrapped__",
            "_wrapped_",
            "____",
            "__",
            "wrapped",
        ] {
            for (classe, esperado) in [("Recibo", Escopo::Tipo), ("str", Escopo::Forma)] {
                let d = attr(&format!(
                    "Cannot access attribute \"{nome}\" for class \"{classe}\""
                ));
                assert_eq!(
                    escopo(&d, "", &tree.root, &origem),
                    esperado,
                    "{nome} on {classe}"
                );
            }
        }
    }

    #[test]
    fn escopo_undefined_name_under_import_star_is_dinamico_and_without_it_proprio() {
        let tree = TempTree::new("esc_star");
        let origem = tree.root.join("a.py");
        let d = diag("reportUndefinedVariable", "\"HTML\" is not defined");
        let dynamic = [
            "from harness import *\nprint(HTML)\n",
            "import os\n    from a.b import *\nprint(HTML)\n",
            "\n\n  from . import *\nprint(HTML)\n",
            "from .x   import   *  # all\nprint(HTML)\n",
            "print(HTML)\nfrom harness import *\n", // the star import may come after the use
        ];
        for text in dynamic {
            assert_eq!(
                escopo(&d, text, &tree.root, &origem),
                Escopo::Dinamico,
                "{text:?}"
            );
        }
        // controls: the very same undefined name is the program's own without a star import
        let own = [
            "from harness import x\nprint(HTML)\n",
            "print(HTML)\n",
            "# from harness import *\nprint(HTML)\n",
            "x = 1; from m import *\nprint(HTML)\n",
            "fromage import *\nprint(HTML)\n",
            "",
        ];
        for text in own {
            assert_eq!(
                escopo(&d, text, &tree.root, &origem),
                Escopo::Proprio,
                "{text:?}"
            );
        }
    }

    #[test]
    fn escopo_forma_needs_the_last_segment_to_be_a_builtin_type() {
        let tree = TempTree::new("esc_forma_neg");
        let origem = tree.root.join("a.py");
        for classe in ["Recibo", "app.Recibo", "app.str_like", "strx", "Str"] {
            let d = attr(&format!(
                "Cannot access attribute \"get\" for class \"{classe}\""
            ));
            assert_eq!(
                escopo(&d, "", &tree.root, &origem),
                Escopo::Tipo,
                "{classe}"
            );
        }
        // a module attribute never becomes forma, even when the module is named like a builtin
        let d = attr("\"x\" is not a known attribute of module \"str\"");
        assert_eq!(escopo(&d, "", &tree.root, &origem), Escopo::Externo);
    }

    #[test]
    fn escopo_concrete_class_member_is_tipo_a_static_type_never_an_existence_claim() {
        // P0B (b): `Cannot access attribute "x" for class "C"` is what the checker INFERRED about the
        // receiver (`ast.AST` has no `lineno`; `Assign.value: expr` has no `args`); the object may be a
        // subclass that has it, so the member's existence is not decided here
        let tree = TempTree::new("esc_cls");
        let d = attr("Cannot access attribute \"total\" for class \"Recibo\"");
        assert_eq!(
            escopo(&d, "", &tree.root, &tree.root.join("a.py")),
            Escopo::Tipo
        );
    }

    #[test]
    fn escopo_project_module_module_file_and_package() {
        // selftest: "escopo: módulo do projeto" (+ a package via __init__.py)
        let tree = TempTree::new("esc_proj");
        tree.file("scripts/__init__.py", "");
        tree.file("scripts/juiz_motor_vgp.py", "");
        tree.file("scripts/pacote/__init__.py", "");
        let origem = tree.root.join("a.py");
        for modulo in ["scripts.juiz_motor_vgp", "scripts.pacote", "scripts"] {
            let d = attr(&format!(
                "\"x\" is not a known attribute of module \"{modulo}\""
            ));
            assert_eq!(
                escopo(&d, "", &tree.root, &origem),
                Escopo::Projeto,
                "{modulo}"
            );
        }
    }

    #[test]
    fn builtin_class_table_is_sorted_for_binary_search_and_has_the_core_types() {
        assert!(
            BUILTIN_CLASSES.windows(2).all(|w| w[0] < w[1]),
            "table must be strictly sorted"
        );
        for core in [
            "str", "list", "dict", "int", "float", "tuple", "bytes", "set", "bool", "object",
        ] {
            assert!(BUILTIN_CLASSES.binary_search(&core).is_ok(), "{core}");
        }
        assert!(BUILTIN_CLASSES.binary_search(&"Recibo").is_err());
    }

    #[test]
    fn escopo_unknown_import_symbol_reads_the_module_from_the_from_line() {
        let tree = TempTree::new("esc_from");
        tree.file("scripts/mod.py", "");
        tree.file("pkg/vizinho.py", "");
        tree.file("outro.py", "");
        let origem = tree.file("pkg/a.py", "");
        let d = |line: &str| {
            let mut v = attr("\"q\" is unknown import symbol");
            v["range"]["start"]["line"] = json!(1);
            (v, format!("import os\n{line}\n"))
        };
        let cases = [
            ("from scripts.mod import q", Escopo::Projeto),
            ("  from scripts.mod import q", Escopo::Projeto),
            ("from os import q", Escopo::Externo),
            ("from .vizinho import q", Escopo::Projeto),
            ("from ..outro import q", Escopo::Projeto),
            ("from .nao_existe import q", Escopo::Externo),
        ];
        for (line, esperado) in cases {
            let (v, texto) = d(line);
            assert_eq!(escopo(&v, &texto, &tree.root, &origem), esperado, "{line}");
        }
    }

    #[test]
    fn escopo_unknown_import_symbol_without_a_line_falls_back_to_project() {
        let tree = TempTree::new("esc_noline");
        let d = attr("\"q\" is unknown import symbol"); // line 0 of an empty text
        assert_eq!(
            escopo(&d, "", &tree.root, &tree.root.join("a.py")),
            Escopo::Projeto
        );
    }

    #[test]
    fn modulo_found_outside_the_root_is_not_a_project_module() {
        let tree = TempTree::new("esc_outside");
        tree.file("ext.py", "");
        let raiz = tree.root.join("proj");
        let origem = tree.file("proj/sub/a.py", "");
        // `from ...ext import x` from proj/sub/a.py climbs to the tree root, outside `proj`.
        assert!(!modulo_do_projeto("...ext", &raiz, &origem));
        // positive control: the same climbing machinery does find a module that IS inside the root
        assert!(modulo_do_projeto(".a", &raiz, &origem));
    }

    // ── severidade: policy P4 (P3 + dinamico), every cell ─────────────────

    #[test]
    fn severidade_policy_p4_every_cell() {
        use Balde::{Arquivo, Inline, Tmp};
        use Escopo::{Dinamico, Externo, Forma, Projeto, Proprio, Tipo};
        use Severidade::{Deny, Warn};
        // 3 baskets x 6 scopes = 18 cells. `forma` (shape of the data), `tipo` (a static type is not an
        // existence claim) and `dinamico` (name space or attribute unknowable statically) are never
        // denied, whatever the basket.
        let cells = [
            (Inline, Proprio, Deny),
            (Inline, Projeto, Deny),
            (Inline, Externo, Deny),
            (Inline, Forma, Warn),
            (Inline, Tipo, Warn),
            (Inline, Dinamico, Warn),
            (Tmp, Proprio, Deny),
            (Tmp, Projeto, Deny),
            (Tmp, Externo, Deny),
            (Tmp, Forma, Warn),
            (Tmp, Tipo, Warn),
            (Tmp, Dinamico, Warn),
            (Arquivo, Proprio, Warn),
            (Arquivo, Projeto, Warn),
            (Arquivo, Externo, Deny),
            (Arquivo, Forma, Warn),
            (Arquivo, Tipo, Warn),
            (Arquivo, Dinamico, Warn),
        ];
        assert_eq!(cells.len(), 18);
        for (balde, escopo, esperado) in cells {
            assert_eq!(
                severidade(balde, escopo),
                esperado,
                "{balde:?} x {escopo:?}"
            );
        }
    }

    #[test]
    fn balde_classification() {
        let raiz = Path::new("/home/u/proj");
        let cases = [
            ("<string>", Balde::Inline),
            ("<stdin>", Balde::Inline),
            ("/tmp/x.py", Balde::Tmp),
            ("/tmp/claude-1000/scratch/y.py", Balde::Tmp),
            ("/home/u/proj/a.py", Balde::Arquivo),
            ("rel/a.py", Balde::Arquivo),
            ("/tmpx/a.py", Balde::Arquivo),
            ("/etc/a.py", Balde::Arquivo),
        ];
        for (path, esperado) in cases {
            assert_eq!(Balde::from_path(path, raiz), esperado, "{path}");
        }
        // a project that itself lives under /tmp keeps its files in the project bucket
        assert_eq!(
            Balde::from_path("/tmp/proj/a.py", Path::new("/tmp/proj")),
            Balde::Arquivo
        );
    }

    /// One finding per scope, each with the severity the policy gives it in `balde`.
    fn achados_do_balde(balde: Balde) -> Vec<L2Achado> {
        [
            Escopo::Proprio,
            Escopo::Projeto,
            Escopo::Externo,
            Escopo::Forma,
            Escopo::Dinamico,
        ]
        .into_iter()
        .map(|escopo| L2Achado {
            classe: "nome".into(),
            nome: format!("{escopo:?}"),
            linha: 1,
            escopo,
            severidade: severidade(balde, escopo),
            regra: "reportUndefinedVariable".into(),
            mensagem: "m".into(),
        })
        .collect()
    }

    fn severidades(achados: &[L2Achado]) -> Vec<(Escopo, Severidade)> {
        achados.iter().map(|a| (a.escopo, a.severidade)).collect()
    }

    #[test]
    fn rebaixar_tmp_na_escrita_trata_o_fragmento_como_arquivo_do_projeto() {
        use Escopo::{Dinamico, Externo, Forma, Projeto, Proprio};
        use Severidade::{Deny, Warn};
        let raiz = Path::new("/home/u/proj");
        let mut tmp = achados_do_balde(Balde::Tmp);
        assert_eq!(
            severidades(&tmp),
            [
                (Proprio, Deny),
                (Projeto, Deny),
                (Externo, Deny),
                (Forma, Warn),
                (Dinamico, Warn)
            ],
            "controle: na execução o /tmp nega tudo que não é forma/dinâmico"
        );
        crate::achado::rebaixar_tmp_na_escrita(&mut tmp, "/tmp/frag.py", raiz);
        assert_eq!(
            severidades(&tmp),
            [
                (Proprio, Warn),
                (Projeto, Warn),
                (Externo, Deny),
                (Forma, Warn),
                (Dinamico, Warn)
            ],
            "escrita em /tmp: só o alvo EXTERNO (nenhuma edição o cria) segue negado"
        );
        // o resto é intocado: inline roda agora, arquivo do projeto já era julgado como arquivo
        for (path, balde) in [
            ("<string>", Balde::Inline),
            ("<stdin>", Balde::Inline),
            ("/home/u/proj/a.py", Balde::Arquivo),
            ("/etc/a.py", Balde::Arquivo),
        ] {
            let mut achados = achados_do_balde(balde);
            let antes = severidades(&achados);
            crate::achado::rebaixar_tmp_na_escrita(&mut achados, path, raiz);
            assert_eq!(severidades(&achados), antes, "{path}");
        }
        // um projeto que mora sob /tmp mantém o dele como arquivo (Balde::from_path)
        let mut achados = achados_do_balde(Balde::Arquivo);
        let antes = severidades(&achados);
        crate::achado::rebaixar_tmp_na_escrita(
            &mut achados,
            "/tmp/proj/a.py",
            Path::new("/tmp/proj"),
        );
        assert_eq!(severidades(&achados), antes);
    }

    #[test]
    fn classe_do_achado_matches_the_layer_one_vocabulary() {
        let cases = [
            (
                diag("reportUndefinedVariable", "\"x\" is not defined"),
                "nome",
            ),
            (diag("reportCallIssue", "No parameter named \"k\""), "kwarg"),
            (attr("\"q\" is unknown import symbol"), "import"),
            (
                attr("\"q\" is not a known attribute of module \"os\""),
                "atributo",
            ),
            (
                attr("Cannot access attribute \"q\" for class \"str\""),
                "membro",
            ),
        ];
        for (d, esperado) in cases {
            assert_eq!(classe_do_achado(&d), esperado, "{d}");
        }
    }

    // ── uri / inline path / discovery ──────────────────────────────────────

    #[test]
    fn file_uri_percent_encodes_like_python_as_uri() {
        assert_eq!(
            file_uri(Path::new("/home/u/p/a.py")),
            "file:///home/u/p/a.py"
        );
        assert_eq!(
            file_uri(Path::new("/home/u/a b.py")),
            "file:///home/u/a%20b.py"
        );
        assert_eq!(
            file_uri(Path::new("/tmp/ação.py")),
            "file:///tmp/a%C3%A7%C3%A3o.py"
        );
        assert_eq!(
            file_uri(Path::new("/p/.venv/bin/x-y_z~1.py")),
            "file:///p/.venv/bin/x-y_z~1.py"
        );
    }

    #[test]
    fn inline_path_is_virtual_deterministic_and_content_keyed() {
        let raiz = Path::new("/proj");
        let a = inline_path(raiz, "<string>", "x = 1\n");
        assert_eq!(a, inline_path(raiz, "<string>", "x = 1\n"));
        assert_ne!(a, inline_path(raiz, "<string>", "x = 2\n"));
        assert_ne!(a, inline_path(raiz, "<stdin>", "x = 1\n"));
        assert_eq!(a.parent(), Some(raiz));
        let name = a.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.starts_with(".vgp_inline_") && name.ends_with(".py"),
            "{name}"
        );
        assert!(!a.exists());
    }

    #[test]
    fn locate_server_prefers_pyright_then_venv_then_basedpyright_and_never_installs() {
        let tree = TempTree::new("locate");
        assert_eq!(locate_server(&tree.root), None);

        tree.file("venv/bin/basedpyright-langserver", "");
        let found = locate_server(&tree.root).unwrap();
        assert_eq!(
            found.path,
            tree.root.join("venv/bin/basedpyright-langserver")
        );
        assert_eq!(found.python, tree.root.join("venv/bin/python"));

        tree.file("venv/bin/pyright-langserver", "");
        assert_eq!(
            locate_server(&tree.root).unwrap().path,
            tree.root.join("venv/bin/pyright-langserver")
        );

        tree.file(".venv/bin/pyright-langserver", "");
        let first = locate_server(&tree.root).unwrap();
        assert_eq!(first.path, tree.root.join(".venv/bin/pyright-langserver"));
        assert_eq!(first.python, tree.root.join(".venv/bin/python"));
    }

    #[test]
    fn discover_without_a_venv_names_what_it_searched() {
        let tree = TempTree::new("discover_none");
        let Err(err) = PyrightSession::discover(&tree.root) else {
            panic!("discover must fail without a venv");
        };
        let motivo = err.to_string();
        assert!(motivo.contains(".venv/bin/pyright-langserver"), "{motivo}");
        assert!(
            motivo.contains("venv/bin/basedpyright-langserver"),
            "{motivo}"
        );
        assert!(matches!(err, PyrightError::NaoEncontrado { .. }));
    }

    // ── check() against a scripted pyright-like server ─────────────────────

    /// A scripted stand-in for pyright: diagnostics come from magic comment lines in the text
    /// (`# undef N`, `# modattr N mod`, `# strattr N`, `# membro N C`, `# uniao N`, `# probe` lists the OTHER open
    /// documents, `# silencio` publishes nothing). `didClose` publishes the null-version cleanup,
    /// exactly like pyright 1.1.408.
    const FAKE_PYRIGHT: &str = r##"
import sys, json, re, os
rin, out = sys.stdin.buffer, sys.stdout.buffer
docs = {}

def read():
    n = 0
    while True:
        line = rin.readline()
        if not line:
            sys.exit(0)
        line = line.strip()
        if not line:
            break
        if line.lower().startswith(b"content-length:"):
            n = int(line.split(b":")[1])
    return json.loads(rin.read(n))

def send(o):
    b = json.dumps(o).encode()
    out.write(b"Content-Length: %d\r\n\r\n" % len(b) + b)
    out.flush()

def pub(uri, ver, diags):
    send({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics",
          "params": {"uri": uri, "version": ver, "diagnostics": diags}})

def diags_for(uri, text):
    if "# silencio" in text:
        return None
    res = []
    for i, line in enumerate(text.split("\n")):
        rng = {"start": {"line": i, "character": 0}, "end": {"line": i, "character": 1}}
        def add(code, msg):
            res.append({"code": code, "message": msg, "range": rng})
        m = re.match(r"# undef (\S+)", line)
        if m:
            add("reportUndefinedVariable", '"%s" is not defined' % m.group(1))
        m = re.match(r"# modattr (\S+) (\S+)", line)
        if m:
            add("reportAttributeAccessIssue", '"%s" is not a known attribute of module "%s"' % m.groups())
        m = re.match(r"# strattr (\S+)", line)
        if m:
            add("reportAttributeAccessIssue", 'Cannot access attribute "%s" for class "str"' % m.group(1))
        m = re.match(r"# membro (\S+) (\S+)", line)
        if m:
            add("reportAttributeAccessIssue", 'Cannot access attribute "%s" for class "%s"' % m.groups())
        m = re.match(r"# uniao (\S+)", line)
        if m:
            add("reportAttributeAccessIssue", 'Cannot access attribute "%s" for class "X | None"' % m.group(1))
        m = re.match(r"from (\S+) import (\S+)\s+# unk", line)
        if m:
            add("reportAttributeAccessIssue", '"%s" is unknown import symbol' % m.group(2))
        if line.startswith("# probe"):
            names = "|".join(sorted(os.path.basename(u) for u in docs if u != uri))
            add("reportUndefinedVariable", '"%s" is not defined' % names)
    return res

while True:
    m = read()
    meth = m.get("method")
    if meth == "initialize":
        send({"jsonrpc": "2.0", "id": m["id"], "result": {"capabilities": {}}})
    elif meth == "shutdown":
        send({"jsonrpc": "2.0", "id": m["id"], "result": None})
    elif meth == "exit":
        sys.exit(0)
    elif meth == "textDocument/didOpen":
        td = m["params"]["textDocument"]
        docs[td["uri"]] = td["text"]
        d = diags_for(td["uri"], td["text"])
        if d is not None:
            pub(td["uri"], td["version"], d)
    elif meth == "textDocument/didClose":
        uri = m["params"]["textDocument"]["uri"]
        docs.pop(uri, None)
        pub(uri, None, [])
"##;

    fn session(budget_ms: u64) -> (TempTree, PyrightSession) {
        let tree = TempTree::new("session");
        let mut cmd = Command::new("python3");
        cmd.arg("-c").arg(FAKE_PYRIGHT);
        let mut client = LspClient::spawn(cmd, Map::new())
            .expect("python3 must be on PATH to run the fake pyright");
        client
            .initialize(
                &file_uri(&tree.root),
                "proj",
                json!({}),
                Instant::now() + Duration::from_secs(10),
            )
            .expect("fake initialize");
        let s = PyrightSession::from_client(
            client,
            tree.root.clone(),
            Duration::from_millis(budget_ms),
        );
        (tree, s)
    }

    fn nomes(r: &L2Resultado) -> Vec<&str> {
        r.achados.iter().map(|a| a.nome.as_str()).collect()
    }

    #[test]
    fn check_inline_denies_and_never_writes_the_virtual_file() {
        let (tree, mut s) = session(5000);
        let r = s.check("<string>", "# undef fantasma\n", None, &[]);
        assert_eq!(r.verdict, Verdict::Deny, "{r:?}");
        assert_eq!(nomes(&r), ["fantasma"]);
        let a = &r.achados[0];
        assert_eq!(
            (a.classe.as_str(), a.linha, a.escopo, a.severidade),
            ("nome", 1, Escopo::Proprio, Severidade::Deny)
        );
        assert_eq!(a.regra, "reportUndefinedVariable");
        assert!(a.mensagem.contains("fantasma"));
        assert_eq!(r.motivo, None);
        let leftovers: Vec<_> = fs::read_dir(&tree.root).unwrap().collect();
        assert!(
            leftovers.is_empty(),
            "inline check must never touch the disk: {leftovers:?}"
        );
    }

    #[test]
    fn check_project_file_policy_p2_warn_for_own_deny_for_external() {
        let (tree, mut s) = session(5000);
        let path = tree.root.join("app.py");
        let path = path.to_str().unwrap();
        let r = s.check(path, "# undef fantasma\n", None, &[]);
        assert_eq!(r.verdict, Verdict::Warn, "{r:?}");
        assert_eq!(r.achados[0].severidade, Severidade::Warn);

        let r = s.check(path, "# undef fantasma\n# modattr nada os\n", None, &[]);
        assert_eq!(r.verdict, Verdict::Deny, "{r:?}");
        let by_name = |n: &str| r.achados.iter().find(|a| a.nome == n).unwrap();
        assert_eq!(by_name("fantasma").severidade, Severidade::Warn);
        assert_eq!(by_name("nada").severidade, Severidade::Deny);
        assert_eq!(by_name("nada").escopo, Escopo::Externo);
        assert_eq!(by_name("nada").linha, 2);
    }

    #[test]
    fn check_builtin_member_is_forma_and_only_warns_in_every_bucket() {
        let (tree, mut s) = session(5000);
        let in_project = tree.root.join("app.py");
        let paths = [
            "<string>".to_string(),
            "/tmp/nao_existe_vgp_l2.py".to_string(),
            in_project.to_string_lossy().into_owned(),
        ];
        for path in paths {
            let r = s.check(&path, "# strattr get\n", None, &[]);
            assert_eq!(r.verdict, Verdict::Warn, "{path}: {r:?}");
            let a = &r.achados[0];
            assert_eq!(
                (a.classe.as_str(), a.escopo, a.severidade),
                ("membro", Escopo::Forma, Severidade::Warn),
                "{path}"
            );
        }
        // …while a missing attribute of a MODULE still denies inline: forma is not "all builtin"
        let r = s.check("<string>", "# strattr get\n# modattr nada os\n", None, &[]);
        assert_eq!(r.verdict, Verdict::Deny, "{r:?}");
    }

    #[test]
    fn check_dynamic_scope_only_warns_in_every_bucket() {
        let (tree, mut s) = session(5000);
        let in_project = tree.root.join("app.py");
        let paths = [
            "<string>".to_string(),
            "/tmp/nao_existe_vgp_l2.py".to_string(),
            in_project.to_string_lossy().into_owned(),
        ];
        let texts = [
            "from harness import *\n# undef HTML\n",
            "# membro __wrapped__ FixtureFunction\n",
        ];
        for path in &paths {
            for text in texts {
                let r = s.check(path, text, None, &[]);
                assert_eq!(r.verdict, Verdict::Warn, "{path} {text:?}: {r:?}");
                let a = &r.achados[0];
                assert_eq!(
                    (a.escopo, a.severidade),
                    (Escopo::Dinamico, Severidade::Warn),
                    "{path} {text:?}"
                );
            }
        }
        // controls: the same name WITHOUT `import *` is the program's own and denies inline…
        let r = s.check(
            "<string>",
            "from harness import x\n# undef HTML\n",
            None,
            &[],
        );
        assert_eq!(
            (r.verdict, r.achados[0].escopo),
            (Verdict::Deny, Escopo::Proprio)
        );
        // …and a non-dunder member of the same unknown class is a static TYPE claim (P0B (b)): it
        // warns where it used to deny, because the receiver's inferred class is not proof of absence
        let r = s.check("<string>", "# membro total FixtureFunction\n", None, &[]);
        assert_eq!(
            (r.verdict, r.achados[0].escopo),
            (Verdict::Warn, Escopo::Tipo)
        );
    }

    #[test]
    fn check_tmp_path_denies_like_inline() {
        let (_tree, mut s) = session(5000);
        let r = s.check("/tmp/nao_existe_vgp_l2.py", "# undef fantasma\n", None, &[]);
        assert_eq!(r.verdict, Verdict::Deny, "{r:?}");
    }

    #[test]
    fn check_clean_and_non_negating_diagnostics_allow() {
        let (_tree, mut s) = session(5000);
        let r = s.check("<string>", "x = 1\n", None, &[]);
        assert_eq!(
            (r.verdict, r.achados.len(), r.motivo.clone()),
            (Verdict::Allow, 0, None)
        );
        let r = s.check("<string>", "# uniao a\n", None, &[]);
        assert_eq!(
            r.verdict,
            Verdict::Allow,
            "union receivers only warn upstream: {r:?}"
        );
        assert!(r.achados.is_empty());
    }

    #[test]
    fn check_reports_only_the_delta_against_disk() {
        let (_tree, mut s) = session(5000);
        let proposto = "# undef velho\n# undef novo\n# modattr m os\n";
        let disco = "# undef velho\n";
        let r = s.check("<string>", proposto, Some(disco), &[]);
        let mut got = nomes(&r);
        got.sort_unstable();
        assert_eq!(got, ["m", "novo"], "{r:?}");
        // identical content on disk -> nothing is the edit's fault
        let r = s.check("<string>", proposto, Some(proposto), &[]);
        assert_eq!(r.verdict, Verdict::Allow, "{r:?}");
    }

    #[test]
    fn check_opens_overlays_before_and_closes_them_after() {
        let (_tree, mut s) = session(5000);
        let overlays = vec![
            ("/proj/mod_a.py".to_string(), "A = 1\n".to_string()),
            ("/proj/mod_b.py".to_string(), "B = 2\n".to_string()),
        ];
        let r = s.check("<string>", "# probe\n", None, &overlays);
        assert_eq!(
            nomes(&r),
            ["mod_a.py|mod_b.py"],
            "overlays must be open during the check: {r:?}"
        );
        let r = s.check("<string>", "# probe\n", None, &[]);
        assert!(
            r.achados.is_empty(),
            "overlays must be closed afterwards: {r:?}"
        );
    }

    #[test]
    fn check_same_uri_twice_returns_the_second_content_not_the_cleanup() {
        let (tree, mut s) = session(5000);
        let path = tree.root.join("re.py");
        let path = path.to_str().unwrap();
        let r1 = s.check(path, "# undef um\n", None, &[]);
        let r2 = s.check(path, "# undef dois\n", None, &[]);
        let r3 = s.check(path, "x = 1\n", None, &[]);
        let r4 = s.check(path, "# undef tres\n", None, &[]);
        assert_eq!(nomes(&r1), ["um"]);
        assert_eq!(
            nomes(&r2),
            ["dois"],
            "2nd result must be the 2nd content's, not the stale cleanup"
        );
        assert_eq!(r3.verdict, Verdict::Allow);
        assert_eq!(nomes(&r4), ["tres"]);
    }

    #[test]
    fn check_deadline_exceeded_is_unknown_with_a_reason_never_allow() {
        let (_tree, mut s) = session(400);
        let t0 = Instant::now();
        let r = s.check("<string>", "# silencio\n# undef x\n", None, &[]);
        let waited = t0.elapsed();
        assert_eq!(r.verdict, Verdict::Unknown, "{r:?}");
        assert!(r.achados.is_empty());
        let motivo = r.motivo.expect("unknown must carry its reason");
        assert!(motivo.contains("400 ms"), "{motivo}");
        assert!(
            waited >= Duration::from_millis(350) && waited < Duration::from_millis(2000),
            "{waited:?}"
        );
        assert!(r.elapsed_ms >= 350, "{}", r.elapsed_ms);
        // the session is still usable afterwards
        let r = s.check("<string>", "# undef depois\n", None, &[]);
        assert_eq!(nomes(&r), ["depois"]);
    }

    #[test]
    fn check_budget_covers_the_whole_call_including_the_disk_baseline() {
        let (_tree, mut s) = session(400);
        let r = s.check("<string>", "# undef x\n", Some("# silencio\n"), &[]);
        assert_eq!(
            r.verdict,
            Verdict::Unknown,
            "a baseline that cannot be measured is not allow: {r:?}"
        );
        assert!(r.motivo.is_some());
    }

    #[test]
    fn result_serializes_with_lowercase_verdict_strings() {
        let r = L2Resultado {
            verdict: Verdict::Unknown,
            motivo: Some("m".into()),
            achados: vec![],
            elapsed_ms: 3,
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["verdict"], "unknown");
        let a = L2Achado {
            classe: "nome".into(),
            nome: "x".into(),
            linha: 1,
            escopo: Escopo::Proprio,
            severidade: Severidade::Deny,
            regra: "reportUndefinedVariable".into(),
            mensagem: "m".into(),
        };
        let v = serde_json::to_value(&a).unwrap();
        assert_eq!(
            (v["escopo"].as_str(), v["severidade"].as_str()),
            (Some("proprio"), Some("deny"))
        );
        let forma = L2Achado {
            escopo: Escopo::Forma,
            severidade: Severidade::Warn,
            ..a
        };
        let v = serde_json::to_value(&forma).unwrap();
        assert_eq!(
            (v["escopo"].as_str(), v["severidade"].as_str()),
            (Some("forma"), Some("warn"))
        );
        let dinamico = L2Achado {
            escopo: Escopo::Dinamico,
            ..forma
        };
        let v = serde_json::to_value(&dinamico).unwrap();
        assert_eq!(v["escopo"].as_str(), Some("dinamico"));
        let tipo = L2Achado {
            escopo: Escopo::Tipo,
            ..dinamico
        };
        let v = serde_json::to_value(&tipo).unwrap();
        assert_eq!(v["escopo"].as_str(), Some("tipo"));
    }

    // ── P0B-d: a classe receptora que a fusão com a camada 1 compara ───────────

    #[test]
    fn p0bd_classe_receptora_le_a_classe_do_diagnostico_de_membro() {
        let msg = |classe: &str| {
            format!(
                "Cannot access attribute \"st_m_size\" for class \"{classe}\"\n  Attribute \"st_m_size\" is unknown"
            )
        };
        // d94f8090d150, and the qualified spelling the checker may use
        assert_eq!(
            classe_receptora(&msg("stat_result")).as_deref(),
            Some("stat_result")
        );
        assert_eq!(
            classe_receptora(&msg("os.stat_result")).as_deref(),
            Some("os.stat_result")
        );
        // 0893b1f5187f: the class is read as it is — whether `str` is closed is layer 1's call
        assert_eq!(classe_receptora(&msg("str")).as_deref(), Some("str"));
        // an incomplete inference never identifies a receiver
        for incompleta in ["stat_result | None", "dict[str, int]", "type[Foo]"] {
            assert_eq!(classe_receptora(&msg(incompleta)), None, "{incompleta}");
        }
        // not a member diagnostic: nothing to compare
        for outra in [
            "\"pathx\" is not a known attribute of module \"os\"",
            "No parameter named \"zz\"",
            "\"zz\" is not defined",
            "",
        ] {
            assert_eq!(classe_receptora(outra), None, "{outra:?}");
        }
    }

    // ── P0B (b): a camada 2 só nega inexistência DEFINITIVA ────────────────────
    //
    // As formas são as dos falsos de MOTOR do P0M (diagnostics/p0m/DIAGNOSTICO.md §2), com o mesmo
    // formato de diagnóstico do pyright:
    //   eb2d7b7ba5b9  `no.lineno`            → Cannot access attribute "lineno" for class "AST"
    //   d85ca025881a  `n.value.args`         → Cannot access attribute "args" for class "expr"
    //   b4aeefe5820e  `ssl._ssl._test_decode_cert` → "_test_decode_cert" is not a known attribute of module "ssl._ssl"
    //   f01b4181af57  `jsonschema.__version__`      → "__version__" is not a known attribute of module "jsonschema"
    //   636f9a7adec1  `folium.__file__`             → "__file__" is not a known attribute of module "folium"

    fn tres_baldes(tree: &TempTree) -> [String; 3] {
        [
            "<string>".to_string(),
            "/tmp/nao_existe_vgp_l2.py".to_string(),
            tree.root.join("app.py").to_string_lossy().into_owned(),
        ]
    }

    #[test]
    fn p0b_b_membro_de_objeto_que_nao_e_modulo_so_avisa_em_todo_balde() {
        let (tree, mut s) = session(5000);
        for path in tres_baldes(&tree) {
            for (texto, nome) in [
                ("# membro lineno AST\n", "lineno"),
                ("# membro args expr\n", "args"),
                ("# membro total Recibo\n", "total"),
            ] {
                let r = s.check(&path, texto, None, &[]);
                assert_eq!(r.verdict, Verdict::Warn, "{path} {texto:?}: {r:?}");
                let a = &r.achados[0];
                assert_eq!(
                    (a.classe.as_str(), a.nome.as_str(), a.escopo, a.severidade),
                    ("membro", nome, Escopo::Tipo, Severidade::Warn),
                    "{path} {texto:?}"
                );
            }
        }
        // controle: o atributo inexistente de um MÓDULO segue negando onde o código roda agora
        let r = s.check("<string>", "# modattr nada os\n", None, &[]);
        assert_eq!(r.verdict, Verdict::Deny, "{r:?}");
    }

    #[test]
    fn p0b_b_modulo_externo_membro_privado_ou_dunder_abstem_em_todo_balde() {
        let (tree, mut s) = session(5000);
        for path in tres_baldes(&tree) {
            for texto in [
                "# modattr _test_decode_cert ssl._ssl\n",
                "# modattr __version__ jsonschema\n",
                "# modattr __file__ folium\n",
                "# modattr _privado os\n",
            ] {
                let r = s.check(&path, texto, None, &[]);
                assert_eq!(r.verdict, Verdict::Allow, "{path} {texto:?}: {r:?}");
                assert!(r.achados.is_empty(), "{path} {texto:?}: {r:?}");
            }
        }
    }

    #[test]
    fn p0b_b_import_de_simbolo_privado_ou_dunder_de_modulo_externo_abstem() {
        let (tree, mut s) = session(5000);
        for path in tres_baldes(&tree) {
            for texto in [
                "from jsonschema import __version__  # unk\n",
                "from ssl import _ssl  # unk\n",
            ] {
                let r = s.check(&path, texto, None, &[]);
                assert_eq!(r.verdict, Verdict::Allow, "{path} {texto:?}: {r:?}");
            }
        }
        // controle: um símbolo PÚBLICO inexistente de módulo externo segue negando
        let r = s.check("<string>", "from os import nada  # unk\n", None, &[]);
        assert_eq!(r.verdict, Verdict::Deny, "{r:?}");
        assert_eq!(
            (r.achados[0].classe.as_str(), r.achados[0].escopo),
            ("import", Escopo::Externo)
        );
    }

    #[test]
    fn p0b_b_controles_o_publico_de_modulo_externo_e_o_privado_de_modulo_do_projeto_seguem_negando()
    {
        let (tree, mut s) = session(5000);
        tree.file("pkg/__init__.py", "");
        // `os.pathx` / `json.loadz`: públicos, módulo externo — negam em todo balde (arquivo incluso)
        for path in tres_baldes(&tree) {
            let r = s.check(
                &path,
                "# modattr pathx os\n# modattr loadz json\n",
                None,
                &[],
            );
            assert_eq!(r.verdict, Verdict::Deny, "{path}: {r:?}");
            assert_eq!(nomes(&r), ["pathx", "loadz"], "{path}");
            assert!(r.achados.iter().all(|a| a.escopo == Escopo::Externo));
        }
        // o privado de módulo do PROJETO não é abstenção da camada 2 (a abstenção é de terceiros)
        let r = s.check("<string>", "# modattr _ajuda pkg\n", None, &[]);
        assert_eq!(r.verdict, Verdict::Deny, "{r:?}");
        assert_eq!(r.achados[0].escopo, Escopo::Projeto);
    }

    fn venv_com_base(tree: &TempTree) {
        tree.file(".venv/lib/python3.12/site-packages/.keep", "");
        let home = tree.root.join("base/bin");
        tree.file(".venv/pyvenv.cfg", &format!("home = {}\n", home.display()));
    }

    fn stdlib(tree: &TempTree, rel: &str, body: &str) {
        tree.file(&format!("base/lib/python3.12/{rel}"), body);
    }

    #[test]
    fn p0b2_arquivo_urllib_error_submodulo_carregado_abstem() {
        let (tree, mut s) = session(5000);
        venv_com_base(&tree);
        stdlib(&tree, "urllib/__init__.py", "");
        stdlib(
            &tree,
            "urllib/error.py",
            "class URLError(OSError):\n    pass\n",
        );
        let path = tree.file("ddc0a56fb49f.py", "");
        let r = s.check(
            &path.to_string_lossy(),
            "import urllib.request\n# modattr error urllib\n",
            None,
            &[],
        );
        assert_eq!(r.verdict, Verdict::Warn, "{r:?}");
        assert_eq!(r.achados.len(), 1, "{r:?}");
        assert_eq!(r.achados[0].classe, "atributo");
        assert_eq!(r.achados[0].escopo, Escopo::Dinamico);
        assert_eq!(r.achados[0].severidade, Severidade::Warn);
    }

    #[test]
    fn p0b2_arquivo_urllib_error_pacote_carregado_abstem() {
        let (tree, mut s) = session(5000);
        venv_com_base(&tree);
        stdlib(&tree, "urllib/__init__.py", "");
        stdlib(
            &tree,
            "urllib/error/__init__.py",
            "class HTTPError(Exception):\n    pass\n",
        );
        let path = tree.file("056b54167b0b.py", "");
        let r = s.check(
            &path.to_string_lossy(),
            "import urllib.request\n# modattr error urllib\n",
            None,
            &[],
        );
        assert_eq!(r.verdict, Verdict::Warn, "{r:?}");
        assert_eq!(r.achados.len(), 1, "{r:?}");
        assert_eq!(r.achados[0].classe, "atributo");
        assert_eq!(r.achados[0].escopo, Escopo::Dinamico);
        assert_eq!(r.achados[0].severidade, Severidade::Warn);
    }

    #[test]
    fn p0b2_inline_pil_image_lanczos_dinamico_abstem() {
        let (tree, mut s) = session(5000);
        tree.file(
            ".venv/lib/python3.12/site-packages/PIL/Image.py",
            r##"class Resampling:
    LANCZOS = 1

module = sys.modules[__name__]
for enum in (Resampling,):
    for item in enum:
        setattr(module, item.name, item.value)
"##,
        );
        let r = s.check(
            "<string>",
            "from PIL import Image\n# modattr LANCZOS PIL.Image\n",
            None,
            &[],
        );
        assert_eq!(r.verdict, Verdict::Warn, "{r:?}");
        assert_eq!(r.achados.len(), 1, "{r:?}");
        assert_eq!(r.achados[0].classe, "atributo");
        assert_eq!(r.achados[0].escopo, Escopo::Dinamico);
        assert_eq!(r.achados[0].severidade, Severidade::Warn);
    }

    #[test]
    fn p0b2_modulo_dinamico_sem_o_nome_continua_negando() {
        let (tree, mut s) = session(5000);
        tree.file(
            ".venv/lib/python3.12/site-packages/PIL/Image.py",
            r##"class Resampling:
    LANCZOS = 1

module = sys.modules[__name__]
for item in Resampling:
    setattr(module, item.name, item.value)
"##,
        );
        let r = s.check("<string>", "# modattr FOOBAR PIL.Image\n", None, &[]);
        assert_eq!(r.verdict, Verdict::Deny, "{r:?}");
    }

    #[test]
    fn p0b2_nome_so_em_comentario_ou_docstring_continua_negando() {
        let (tree, mut s) = session(5000);
        tree.file(
            ".venv/lib/python3.12/site-packages/PIL/Image.py",
            r##"# FOOBAR = 4
"""
FOOBAR = 4
"""
module = sys.modules[__name__]
setattr(module, "outro", 1)
"##,
        );
        let r = s.check("<string>", "# modattr FOOBAR PIL.Image\n", None, &[]);
        assert_eq!(r.verdict, Verdict::Deny, "{r:?}");
    }

    #[test]
    #[ignore = "needs the analise venv"]
    fn p0b2_pillow_real_lanczos_abstem_e_foobar_nega() {
        let imagem = Path::new(
            "/home/gabrielgadea/projects/analise/.venv/lib/python3.12/site-packages/PIL/Image.py",
        );
        if !imagem.is_file() {
            eprintln!("Pillow real ausente: {}", imagem.display());
            return;
        }
        let Some(raiz) = imagem.parent() else {
            eprintln!("Pillow sem diretório pai: {}", imagem.display());
            return;
        };
        let Some(raiz) = raiz.parent() else {
            eprintln!("Pillow sem site-packages: {}", imagem.display());
            return;
        };
        let fonte = fs::read_to_string(imagem).expect("Image.py real deve ser legível");
        let codigo = codigo_sem_comentarios(&fonte);
        assert!(
            codigo.contains("module = sys.modules[__name__]"),
            "a sanitização perdeu a ligação do módulo"
        );
        assert!(
            publicacao_dinamica(&codigo),
            "a publicação dinâmica real não foi reconhecida"
        );
        assert!(
            contem_identificador(&codigo, "LANCZOS"),
            "LANCZOS não foi reconhecido como identificador no código real"
        );
        assert!(
            arquivo_modulo(raiz, &["PIL", "Image"]).is_some(),
            "o resolvedor não encontrou Image.py"
        );
        assert!(
            fonte_define_nome(&fonte, "LANCZOS"),
            "o predicado completo não reconheceu LANCZOS"
        );
        let raizes = vec![raiz.to_path_buf()];
        assert!(
            evidencia_de_modulo(&raizes, &["PIL", "Image"], "LANCZOS"),
            "LANCZOS deve ser encontrado no Image.py real"
        );
        assert!(
            !evidencia_de_modulo(&raizes, &["PIL", "Image"], "FOOBAR"),
            "FOOBAR não pode ser inferido da publicação dinâmica"
        );
        println!("Pillow real verificado: {}", imagem.display());
    }

    #[test]
    fn p0b2_modulo_com_getattr_abstem() {
        let (tree, mut s) = session(5000);
        tree.file(
            ".venv/lib/python3.12/site-packages/dinamico.py",
            "def __getattr__(nome):\n    return 42\n",
        );
        let r = s.check(
            "<string>",
            "import dinamico\n# modattr qualquer dinamico\n",
            None,
            &[],
        );
        assert_eq!(r.verdict, Verdict::Warn, "{r:?}");
        assert_eq!(r.achados[0].classe, "atributo");
        assert_eq!(r.achados[0].escopo, Escopo::Dinamico);
    }

    #[test]
    fn p0b2_atributo_realmente_inexistente_continua_negando() {
        let (tree, mut s) = session(5000);
        venv_com_base(&tree);
        stdlib(&tree, "urllib/__init__.py", "");
        stdlib(&tree, "urllib/request.py", "def urlopen():\n    pass\n");
        let r = s.check(
            "<string>",
            "import urllib.request\n# modattr nao_existe urllib.request\n",
            None,
            &[],
        );
        assert_eq!(r.verdict, Verdict::Deny, "{r:?}");
        assert_eq!(nomes(&r), ["nao_existe"], "{r:?}");
    }

    #[test]
    fn p0b2_unknown_import_symbol_continua_negando() {
        let (tree, mut s) = session(5000);
        venv_com_base(&tree);
        stdlib(&tree, "urllib/__init__.py", "");
        stdlib(&tree, "urllib/request.py", "def urlopen():\n    pass\n");
        let r = s.check(
            "<string>",
            "from urllib.request import nao_existe  # unk\n",
            None,
            &[],
        );
        assert_eq!(r.verdict, Verdict::Deny, "{r:?}");
        assert_eq!(r.achados[0].classe, "import");
        assert_eq!(nomes(&r), ["nao_existe"], "{r:?}");
    }

    #[test]
    fn p0b2_privado_com_getattr_segue_sem_achado() {
        let (tree, mut s) = session(5000);
        tree.file(
            ".venv/lib/python3.12/site-packages/pkg/__init__.py",
            "def __getattr__(nome):\n    return 42\n",
        );
        let r = s.check("<string>", "# modattr _impl pkg\n", None, &[]);
        assert_eq!(r.verdict, Verdict::Allow, "{r:?}");
        assert!(r.achados.is_empty(), "{r:?}");
    }

    #[test]
    fn p0b2_pacote_global_fora_do_venv_nao_abstem() {
        let (tree, mut s) = session(5000);
        venv_com_base(&tree);
        tree.file(
            "base/lib/python3.12/site-packages/globalpkg.py",
            "nao_existe = 1\n",
        );
        let r = s.check("<string>", "# modattr nao_existe globalpkg\n", None, &[]);
        assert_eq!(r.verdict, Verdict::Deny, "{r:?}");
    }

    #[test]
    fn p0b2_primeira_ocorrencia_do_modulo_decide() {
        let (tree, mut s) = session(5000);
        venv_com_base(&tree);
        tree.file(
            ".venv/lib/python3.12/site-packages/sombra.py",
            "nao_existe = 1\n",
        );
        stdlib(&tree, "sombra.py", "");
        let r = s.check("<string>", "# modattr nao_existe sombra\n", None, &[]);
        assert_eq!(r.verdict, Verdict::Deny, "{r:?}");
    }

    #[test]
    fn p0b2_venv_precede_pacotes_globais_quando_habilitados() {
        let (tree, mut s) = session(5000);
        venv_com_base(&tree);
        let home = tree.root.join("base/bin");
        tree.file(
            ".venv/pyvenv.cfg",
            &format!(
                "home = {}\ninclude-system-site-packages = true\n",
                home.display()
            ),
        );
        tree.file(".venv/lib/python3.12/site-packages/sombra.py", "");
        tree.file(
            "base/lib/python3.12/site-packages/sombra.py",
            "nao_existe = 1\n",
        );
        let r = s.check("<string>", "# modattr nao_existe sombra\n", None, &[]);
        assert_eq!(r.verdict, Verdict::Deny, "{r:?}");
    }

    #[test]
    fn p0b2_fonte_ignora_comparacao_e_docstring() {
        assert!(!fonte_define_nome("len == 4\n", "len"));
        assert!(!fonte_define_nome("len: int\n", "len"));
        assert!(!fonte_define_nome(
            "\"\"\"\nlen = 4\ndef __getattr__(nome):\n\"\"\"\n",
            "len"
        ));
        assert!(!fonte_define_nome(
            r##""""
\"""
len = 4
"""
"##,
            "len"
        ));
        assert!(fonte_define_nome("len = 4\n", "len"));
    }
}
