//! VGP motor — layer 1: deny invented API in Python BEFORE the edit, pure and daemonless.
//!
//! One [`check`] call per proposed file: static scope analysis (`nome`), from-import resolution
//! (`import`), project/venv module attributes (`atributo`), concrete-class instance members
//! (`membro`/`atributo`) and keyword arguments (`kwarg`). Fail-open by construction: a parse
//! error or any unresolvable link yields `unknown`/`warn`, never `deny` — the corpus ruler is
//! falso deny ≤ 1 % over 300 clean files (V1 spec, 2026-10-08-vgp-motor-base).
//!
//! Delta mode: with `disk` content, a finding is denied only when it is NOT already present in
//! the disk content — a pre-edit hook denies what the edit INTRODUCES, never what pre-existed.

pub mod checks;
pub mod modules;
pub mod scope;
pub mod sombra;
pub mod stdlib_model;

use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

/// One deny/warn finding — the JSON shape the judge and the hooks read.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Finding {
    /// Check family: `nome` | `atributo` | `import` | `kwarg` | `membro`.
    pub classe: String,
    /// The provably-missing name.
    pub nome: String,
    /// 1-based line in the proposed content.
    pub linha: usize,
    /// `deny` | `warn` (P2: deny only in /tmp+inline or on external targets).
    pub severidade: String,
    /// The teaching refusal — why, plus the closest real name when one exists.
    pub sugestao: String,
    /// Real names that do exist (member set / module top names, capped).
    pub reais: Vec<String>,
}

impl Finding {
    /// Build a finding with owned strings (call sites stay one-liners).
    pub fn new(
        classe: &str,
        nome: &str,
        linha: usize,
        severidade: &str,
        sugestao: String,
        reais: Vec<String>,
    ) -> Self {
        Self {
            classe: classe.to_string(),
            nome: nome.to_string(),
            linha,
            severidade: severidade.to_string(),
            sugestao,
            reais,
        }
    }
}

/// Everything a check needs; the CLI handler resolves files into strings first.
pub struct CheckInput {
    /// Virtual path of the proposed file; `<string>`/`<stdin>` means inline code.
    pub path: String,
    /// Proposed content (the post-edit state the motor judges).
    pub content: String,
    /// Project root the module names resolve against.
    pub project_root: PathBuf,
    /// Disk content for delta mode (deny only what the edit introduces).
    pub disk: Option<String>,
    /// Overlay path → content: module files at the entry's instant, laid over today's disk.
    pub overlays: HashMap<String, String>,
    /// `python` (checks live) · `typescript`/`rust` (warn-only, no corpus yet).
    pub lang: String,
}

/// The JSON the CLI prints and the judge reads — always present, even on failure.
#[derive(Debug, Serialize)]
pub struct CheckOutput {
    /// `deny` | `warn` | `allow` | `unknown` (parse failure — fail-open).
    pub verdict: String,
    /// Motor layer (1 = static, pure, daemonless).
    pub layer: u8,
    /// Language the check ran against.
    pub lang: String,
    /// Wall time of the motor itself (the hook's budget is p95 ≤ 150 ms).
    pub elapsed_ms: f64,
    /// Findings ordered by discovery; deny first is not guaranteed.
    pub findings: Vec<Finding>,
    /// Camada 2 status when `--camadas 1,2` was asked and pyright was unavailable —
    /// `"unknown: <motivo>"` (the L1 findings still stand). Absent when L2 ran or was not asked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camada2: Option<String>,
}

/// The one public entry. `verdict`: deny (≥1 deny finding) · warn (≥1 warn finding, no deny) ·
/// allow · unknown (parse failure or an unresolvable precondition — fail-open, never deny).
pub fn check(input: &CheckInput) -> CheckOutput {
    let t0 = Instant::now();
    let (verdict, findings) = if input.lang == "python" {
        checks::check_python(input)
    } else {
        // TS/Rust: warn-only territory (no corpus to measure precision — spec V1 §"O QUE NEGA")
        ("allow".to_string(), Vec::new())
    };
    CheckOutput {
        verdict,
        layer: 1,
        lang: input.lang.clone(),
        elapsed_ms: t0.elapsed().as_secs_f64() * 1000.0,
        findings,
        camada2: None,
    }
}

/// Is this virtual path an inline program (`python3 -c` / heredoc / `touring run --code`)?
pub fn is_inline(path: &str) -> bool {
    path == "<string>" || path == "<stdin>"
}
