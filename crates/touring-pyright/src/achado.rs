//! The neutral result of a layer-2 check — what every verifier (pyright, rust-analyzer, tsserver, deno)
//! reduces its diagnostics to, plus the one severity policy that decides what blocks.
//!
//! Kept free of any server specifics so that converting it to the engine's own `Finding` stays the
//! consumer's job (which is what keeps `touring-code` free of a dependency on this crate). The policy
//! is the "P4" of the V3 spec (§3) generalised by VL0 §5: code that runs *now* denies, a project
//! file denies only for an *external* target, a shape or dynamic question only ever warns.

use std::path::Path;

use serde::{Deserialize, Serialize};

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
    /// A member of an object that is NOT a module (`Cannot access attribute "lineno" for class "AST"`):
    /// what the checker said is a claim about the receiver's INFERRED type, never about existence — the
    /// object may be a subclass that has the member (`ast.AST` has no `lineno`, its subclasses do;
    /// `Assign.value: expr` has no `args`, the `Call` it holds does). Only a module's missing attribute
    /// is an existence claim (P0B (b)).
    Tipo,
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

/// The severity policy P4 (V3 spec §3: P2 + `forma` + `dinamico`, and P0B's `tipo`), one pure function
/// over the 3 baskets x 6 scopes:
///
/// * [`Escopo::Forma`] → **warn**, in every basket: a member of a builtin value type asks about the
///   shape of the data, which static inference cannot settle;
/// * [`Escopo::Tipo`] → **warn**, in every basket: a member of any other non-module object is a static
///   type claim, not an existence claim;
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
        (_, Escopo::Forma | Escopo::Tipo | Escopo::Dinamico) => Severidade::Warn,
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

/// The write-time reading of a layer-2 result (FRAG, 10/10/2026): a file being WRITTEN under
/// `/tmp` is a fragment the next edit may still complete (a snippet to paste), so its findings
/// are judged as a project file's ([`Balde::Arquivo`]) — [`Escopo::Externo`] still denies (nothing
/// the project does creates it), every other scope warns. The same file about to be EXECUTED keeps
/// the verdict [`severidade`] gave it (`Balde::Tmp` denies): the consumer simply does not call
/// this. Inline programs and project files are untouched; so is a `/tmp` path that is the
/// project's own (`Balde::from_path` already calls it `Arquivo`).
///
/// Layer 1 has the same rule in `touring-code` (`vgp::Contexto`); the two stay apart because this
/// crate carries no `touring-*` dependency, so each consumer applies the one that matches.
pub fn rebaixar_tmp_na_escrita(achados: &mut [L2Achado], path: &str, raiz: &Path) {
    if Balde::from_path(path, raiz) != Balde::Tmp {
        return;
    }
    for achado in achados.iter_mut() {
        achado.severidade = severidade(Balde::Arquivo, achado.escopo);
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
