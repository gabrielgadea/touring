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
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use thiserror::Error;

use crate::client::{ClientError, LspClient};

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

// ── public types ───────────────────────────────────────────────────────────

/// Where the checked code lives — the axis of the severity policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Balde {
    /// A program passed on the command line or stdin (`<string>`, `<stdin>`): it runs right now.
    Inline,
    /// A file under `/tmp` (outside the project): a one-off script that runs right now.
    Tmp,
    /// A file of the project: it may be a half-written intermediate state.
    Arquivo,
}

/// Whose symbol is missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Escopo {
    /// A name of the program itself (undefined variable).
    Proprio,
    /// A module, class or callee that lives under the project root. Also the answer when unsure.
    Projeto,
    /// A stdlib/third-party module.
    Externo,
    /// A member of a builtin *value type* (`int.add`, `str.get`): the shape of the data, not a
    /// missing symbol. pyright infers e.g. `list[int | set]` and accuses `int.add` in a program that ran.
    Forma,
    /// Unknowable statically: an undefined name in a text with `from <m> import *`, or a dunder
    /// attribute (`__wrapped__`, `__self__`) that a decorator, descriptor or C extension creates at run time.
    Dinamico,
}

/// Whether a finding blocks the edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severidade {
    /// Blocks.
    Deny,
    /// Reported, does not block.
    Warn,
}

/// The verdict of one check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    /// At least one finding denies.
    Deny,
    /// Findings exist but none denies.
    Warn,
    /// The checker looked and found nothing attributable to the edit.
    Allow,
    /// The checker could not look (deadline, dead server): never an implicit allow.
    Unknown,
}

/// One finding attributable to the edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct L2Achado {
    /// Layer-1 vocabulary: `import` | `atributo` | `membro` | `kwarg` | `nome`.
    pub classe: String,
    /// The invented name (first quoted token of the diagnostic).
    pub nome: String,
    /// 1-based line.
    pub linha: u32,
    /// Whose symbol is missing.
    pub escopo: Escopo,
    /// Whether this finding blocks.
    pub severidade: Severidade,
    /// The pyright rule (`reportAttributeAccessIssue`, …).
    pub regra: String,
    /// The diagnostic message, verbatim.
    pub mensagem: String,
}

/// The outcome of [`PyrightSession::check`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct L2Resultado {
    /// Aggregate verdict.
    pub verdict: Verdict,
    /// Why the verdict is [`Verdict::Unknown`]; `None` otherwise.
    pub motivo: Option<String>,
    /// Findings attributable to the edit (proposed − disk).
    pub achados: Vec<L2Achado>,
    /// Wall time of the whole check.
    pub elapsed_ms: u64,
}

impl L2Resultado {
    /// A result that says "could not measure", with the reason.
    pub fn unknown(motivo: impl Into<String>, elapsed_ms: u64) -> Self {
        Self {
            verdict: Verdict::Unknown,
            motivo: Some(motivo.into()),
            achados: Vec::new(),
            elapsed_ms,
        }
    }

    /// Aggregates findings: any deny → deny; otherwise any finding → warn; otherwise allow.
    pub fn from_achados(achados: Vec<L2Achado>, elapsed_ms: u64) -> Self {
        let verdict = if achados.iter().any(|a| a.severidade == Severidade::Deny) {
            Verdict::Deny
        } else if achados.is_empty() {
            Verdict::Allow
        } else {
            Verdict::Warn
        };
        Self {
            verdict,
            motivo: None,
            achados,
            elapsed_ms,
        }
    }
}

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
/// `dinamico` even on a builtin class, and only then is a builtin class `forma`.
fn escopo_de_membro(msg: &str) -> Escopo {
    let Some(classe) = classe_concreta(msg) else {
        return Escopo::Projeto;
    };
    if first_quoted(msg).is_some_and(is_dunder) {
        Escopo::Dinamico
    } else if is_builtin_class(classe) {
        Escopo::Forma
    } else {
        Escopo::Projeto
    }
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
    let alvo = partes.iter().fold(base.to_path_buf(), |dir, p| dir.join(p));
    let found = [alvo.with_extension("py"), alvo.join("__init__.py")]
        .into_iter()
        .find(|candidate| candidate.is_file())?;
    Some(
        found
            .canonicalize()
            .is_ok_and(|real| real.starts_with(raiz_real)),
    )
}

/// The severity policy P4 (V3 spec §3: P2 + `forma` + `dinamico`), one pure function over the 3 baskets
/// x 5 scopes:
///
/// * [`Escopo::Forma`] → **warn**, in every basket: a member of a builtin value type asks about the
///   shape of the data, which static inference cannot settle;
/// * [`Escopo::Dinamico`] → **warn**, in every basket: under `from <m> import *` or for a dunder
///   attribute the name space is unknowable statically;
/// * code that runs now ([`Balde::Inline`], [`Balde::Tmp`]) → deny for every other scope;
/// * a project file ([`Balde::Arquivo`]) → deny only for [`Escopo::Externo`], warn otherwise.
///
/// Measured on the V0d corpus (09/10/2026): P2 denied 7/7 `/tmp` and 16/28 inline invented symbols
/// with 1/259 (0.39%) false deny; "deny everything" produced 10/259, all intermediate states. The 2
/// inline false denies and the single file false deny of P2 were all `forma` (pyright inferring
/// `list[int | set]` and accusing `int.add` in a program that had run). On the final corpus (instant
/// ruler) P3 still had false denies of 1/100 (`/tmp`) and 2/300 (inline); `dinamico` brings them to
/// 0/100 and 1/300 (the remaining one is a corpus defect) at zero cost in deny recall.
pub fn severidade(balde: Balde, escopo: Escopo) -> Severidade {
    match (balde, escopo) {
        (_, Escopo::Forma | Escopo::Dinamico) => Severidade::Warn,
        (Balde::Arquivo, Escopo::Externo) => Severidade::Deny,
        (Balde::Arquivo, _) => Severidade::Warn,
        (Balde::Inline | Balde::Tmp, _) => Severidade::Deny,
    }
}

impl Balde {
    /// Classifies a path: `<…>` is inline; a path inside the project root is a project file even
    /// when the root itself lives under `/tmp`; otherwise `/tmp` (or the OS temp dir) is [`Balde::Tmp`].
    pub fn from_path(path: &str, raiz: &Path) -> Self {
        if path.starts_with('<') {
            return Self::Inline;
        }
        let full = raiz.join(path);
        if full.starts_with(raiz) {
            return Self::Arquivo;
        }
        if full.starts_with("/tmp") || full.starts_with(std::env::temp_dir()) {
            Self::Tmp
        } else {
            Self::Arquivo
        }
    }
}

// ── uris ───────────────────────────────────────────────────────────────────

/// `file://` URI of an absolute path, percent-encoding every byte but `A-Za-z0-9-._~/` — the same
/// set Python's `Path.as_uri()` leaves alone.
pub fn file_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    for &byte in path.as_os_str().as_encoded_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                uri.push(char::from(byte));
            }
            _ => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    uri
}

/// The virtual file an inline program (`<string>`, `<stdin>`) is opened as: `<raiz>/.vgp_inline_<hash>.py`.
/// It is **never written to disk** — it exists only as a `didOpen` document, so relative imports and
/// the project's import roots resolve as they would for a script placed there. The hash keys it by
/// `(path, content)`, stable within a process (all it needs to be).
pub fn inline_path(raiz: &Path, path: &str, content: &str) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    content.hash(&mut hasher);
    raiz.join(format!(".vgp_inline_{:016x}.py", hasher.finish()))
}

// ── the session ────────────────────────────────────────────────────────────

/// What a check works on: the target, the proposed content, the optional disk baseline, overlays.
struct Pedido<'a> {
    uri: String,
    origem: PathBuf,
    balde: Balde,
    content: &'a str,
    disk: Option<&'a str>,
    overlays: &'a [(String, String)],
    deadline: Instant,
}

/// A live `pyright-langserver` bound to one project root. Cheap to call (tens of ms warm), so the
/// owner keeps it for the lifetime of the hook daemon or CLI run; dropping it shuts the server down.
pub struct PyrightSession {
    client: LspClient,
    root: PathBuf,
    budget: Duration,
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
        let mut client = LspClient::spawn(command, sections(&root, &server.python))?;
        let name = root
            .file_name()
            .map_or_else(|| "project".into(), |n| n.to_string_lossy().into_owned());
        client.initialize(
            &file_uri(&root),
            &name,
            capabilities(),
            Instant::now() + STARTUP_BUDGET,
        )?;
        Ok(Self::from_client(client, root, DEFAULT_BUDGET))
    }

    /// Wraps an already-initialized client (the seam tests use to inject a scripted server).
    pub(crate) fn from_client(client: LspClient, root: PathBuf, budget: Duration) -> Self {
        Self {
            client,
            root,
            budget,
        }
    }

    /// Replaces the per-check budget (default 1000 ms).
    pub fn with_budget(mut self, budget: Duration) -> Self {
        self.budget = budget;
        self
    }

    /// The project root the server was started on.
    pub fn root(&self) -> &Path {
        &self.root
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
        let started = Instant::now();
        let origem = if path.starts_with('<') {
            inline_path(&self.root, path, content)
        } else {
            self.root.join(path)
        };
        let pedido = Pedido {
            uri: file_uri(&origem),
            origem,
            balde: Balde::from_path(path, &self.root),
            content,
            disk,
            overlays,
            deadline: started + self.budget,
        };
        let mut opened = Vec::new();
        let outcome = self.run_check(&pedido, &mut opened);
        for uri in &opened {
            let _ = self.client.close(uri);
        }
        let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        match outcome {
            Ok(achados) => L2Resultado::from_achados(achados, elapsed_ms),
            Err(e) => L2Resultado::unknown(self.motivo(&e), elapsed_ms),
        }
    }

    fn run_check(
        &mut self,
        pedido: &Pedido<'_>,
        opened: &mut Vec<String>,
    ) -> Result<Vec<L2Achado>, ClientError> {
        self.open_overlays(pedido.overlays, opened)?;
        let diags = self.diagnose(&pedido.uri, pedido.content, pedido.deadline)?;
        let mut achados = achados_de(&diags, pedido, &self.root);
        if let Some(on_disk) = pedido.disk {
            let ja = self.pre_existing(pedido, on_disk)?;
            achados.retain(|a| !ja.contains(&(a.regra.clone(), a.nome.clone())));
        }
        Ok(achados)
    }

    fn open_overlays(
        &mut self,
        overlays: &[(String, String)],
        opened: &mut Vec<String>,
    ) -> Result<(), ClientError> {
        for (path, text) in overlays {
            let uri = file_uri(&self.root.join(path));
            self.client.open(&uri, LANGUAGE_ID, text)?;
            opened.push(uri);
        }
        Ok(())
    }

    /// `(rule, name)` pairs the file already denies on disk — the "before" of the delta.
    fn pre_existing(
        &mut self,
        pedido: &Pedido<'_>,
        on_disk: &str,
    ) -> Result<HashSet<(String, String)>, ClientError> {
        let before = self.diagnose(&pedido.uri, on_disk, pedido.deadline)?;
        Ok(before
            .iter()
            .filter_map(|d| negacao(d).map(|nome| (regra(d).to_string(), nome)))
            .collect())
    }

    /// One `didOpen → publishDiagnostics(this version) → didClose` round-trip. The document is
    /// closed even when the wait fails, so a timeout never leaves a stale open document behind.
    fn diagnose(
        &mut self,
        uri: &str,
        text: &str,
        deadline: Instant,
    ) -> Result<Vec<Value>, ClientError> {
        let version = self.client.open(uri, LANGUAGE_ID, text)?;
        let published = self.client.wait_publish(uri, version, deadline);
        let _ = self.client.close(uri);
        published
    }

    fn motivo(&self, e: &ClientError) -> String {
        match e {
            ClientError::Timeout(what) => format!(
                "prazo de {} ms estourado esperando {what}",
                self.budget.as_millis()
            ),
            other => format!("camada 2 (pyright) indisponível: {other}"),
        }
    }
}

fn achados_de(diags: &[Value], pedido: &Pedido<'_>, raiz: &Path) -> Vec<L2Achado> {
    diags
        .iter()
        .filter_map(|d| {
            let nome = negacao(d)?;
            let escopo = escopo(d, pedido.content, raiz, &pedido.origem);
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
            for (classe, esperado) in [("Recibo", Escopo::Projeto), ("str", Escopo::Forma)] {
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
                Escopo::Projeto,
                "{classe}"
            );
        }
        // a module attribute never becomes forma, even when the module is named like a builtin
        let d = attr("\"x\" is not a known attribute of module \"str\"");
        assert_eq!(escopo(&d, "", &tree.root, &origem), Escopo::Externo);
    }

    #[test]
    fn escopo_unknown_class_is_project_the_conservative_side() {
        // selftest: "escopo: classe desconhecida = projeto (lado conservador)"
        let tree = TempTree::new("esc_cls");
        let d = attr("Cannot access attribute \"total\" for class \"Recibo\"");
        assert_eq!(
            escopo(&d, "", &tree.root, &tree.root.join("a.py")),
            Escopo::Projeto
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
        use Escopo::{Dinamico, Externo, Forma, Projeto, Proprio};
        use Severidade::{Deny, Warn};
        // 3 baskets x 5 scopes = 15 cells. `forma` (shape of the data) and `dinamico` (name space or
        // attribute unknowable statically) are never denied, whatever the basket.
        let cells = [
            (Inline, Proprio, Deny),
            (Inline, Projeto, Deny),
            (Inline, Externo, Deny),
            (Inline, Forma, Warn),
            (Inline, Dinamico, Warn),
            (Tmp, Proprio, Deny),
            (Tmp, Projeto, Deny),
            (Tmp, Externo, Deny),
            (Tmp, Forma, Warn),
            (Tmp, Dinamico, Warn),
            (Arquivo, Proprio, Warn),
            (Arquivo, Projeto, Warn),
            (Arquivo, Externo, Deny),
            (Arquivo, Forma, Warn),
            (Arquivo, Dinamico, Warn),
        ];
        assert_eq!(cells.len(), 15);
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
        // …and a non-dunder member of the same unknown class stays project scope
        let r = s.check("<string>", "# membro total FixtureFunction\n", None, &[]);
        assert_eq!(
            (r.verdict, r.achados[0].escopo),
            (Verdict::Deny, Escopo::Projeto)
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
    }
}
