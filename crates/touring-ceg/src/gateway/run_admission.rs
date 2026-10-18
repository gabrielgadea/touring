//! The admission predicate of `touring run` — ONE definition (D8).
//!
//! The `touring run` executor drives the user's program through X0..X7 under a capability
//! profile — `Sandboxed` by default, `Trusted` under `--allow-forbidden` — before running it, and
//! refuses what the profile denies. The code-mode gates of `touring-cli` (G10, G1, G8…) route work
//! INTO that executor: their deny messages name `touring run …` as the way out. A gate that names
//! a route the executor then refuses has taught a command that fails (measured 10/10/2026: the G10
//! named `touring run --lang python` for a program that WRITES; the Sandboxed profile denies the
//! write, and the run that worked was `TOURING_TRUSTED_OK=1 touring run --allow-forbidden`).
//!
//! So the question «would the executor admit this program?» has exactly one answer, and it lives
//! here: `touring run` (`touring-server`, `cli/run.rs::gate_run`) and the gates (`touring-cli`,
//! `cli_suggester`) both call [`admission_outcome`] / [`only_subprocess_denials`] /
//! [`blocking_reason`]; the gates add [`run_route`] on top. Two predicates would drift.

use std::path::Path;

use super::decision::{GateDecision, Verdict};
use super::error::GatewayError;
use super::pre_exec::{
    GatewayDeps, GatewayOutcome, deferred_dry_run, neutral_outcome_history, run_gateway,
    soft_pass_symbol,
};
use super::predict::ExecutionOutcomePredictor;
use crate::capability::{CapabilityProfile, builtins};

/// The gateway tool identity for a `--lang` value — mirrors the sandbox adapter's mapping, so the
/// ledger names the real runtime and X6 classifies the program the way the executor will.
#[must_use]
fn tool_for_lang(lang: &str) -> &'static str {
    match lang.to_ascii_lowercase().as_str() {
        "sh" | "bash" | "shell" => "Bash",
        "python" | "py" => "SandboxPython",
        "js" | "javascript" | "node" | "bun" => "SandboxJavaScript",
        "ts" | "typescript" => "SandboxTypeScript",
        "ruby" | "rb" => "SandboxRuby",
        "go" | "golang" => "SandboxGo",
        "rust" | "rs" => "SandboxRust",
        _ => "SandboxPython",
    }
}

/// Drive `code` through X0..X7 under `profile` exactly as the `touring run` executor does: neutral
/// X3/X4 dependencies and a deferred X5 dry-run (the REAL execution happens later, in the sandbox).
///
/// # Errors
///
/// [`GatewayError`] when the entry layer rejects the call (an empty body, or a tool that does not
/// run code) — the pipeline itself cannot fail once entered. Callers fail OPEN on it (the CEG
/// invariant: the gate never bricks the session).
pub fn admission_outcome(
    lang: &str,
    code: &str,
    profile: &CapabilityProfile,
) -> Result<GatewayOutcome, GatewayError> {
    let predictor = ExecutionOutcomePredictor::new();
    let deps = GatewayDeps {
        symbol_exists: &soft_pass_symbol,
        outcome_history: &neutral_outcome_history,
        sandbox_runner: &deferred_dry_run,
        predictor: &predictor,
        profile,
        claim: None,
        claim_context: super::offensive_integration::ClaimContext::default(),
        solver_backend: super::offensive_integration::SolverBackendKind::Stub,
    };
    run_gateway(tool_for_lang(lang), code, None, &deps)
}

/// Whether every capability X6 denied is `subprocess` — the only class the run path waives (every
/// language since 30/08/2026; shell-only before).
///
/// An EMPTY class list means the deny came from somewhere other than X6 (an X2 destructive
/// pattern, a composite below threshold), and those were never shell noise: it returns `false`,
/// so the deny stands. Failing to the hard verdict is the right default — a waiver granted by
/// accident is a hole.
#[must_use]
pub fn only_subprocess_denials(decision: &GateDecision) -> bool {
    // An X2 destructive pattern is never shell noise, even when a `subprocess` denial rides along
    // with it — `rm -rf` denies for BOTH reasons, and keying the waiver on the capability class
    // alone let the destructive half through on the coattails of the harmless one (observed
    // 2026-08-27).
    !decision.static_blocked
        && !decision.denied_classes.is_empty()
        && decision.denied_classes.iter().all(|c| c == "subprocess")
}

/// The reason worth showing when a deny STANDS: the first one whose class is not `subprocess`,
/// because that is the class that kept the waiver from firing — `reasons.first()` named an
/// innocent `sed` while the determinant class was `file-write` (G-B, campo analise-a2 30/08).
/// Falls back to the first reason (or the generic line) when nothing more specific exists.
#[must_use]
pub fn blocking_reason(decision: &GateDecision) -> &str {
    let has_non_subprocess = decision.denied_classes.iter().any(|c| c != "subprocess");
    if has_non_subprocess
        && let Some(r) = decision
            .reasons
            .iter()
            .find(|r| !r.contains("the subprocess capability"))
    {
        return r;
    }
    decision
        .reasons
        .first()
        .map_or("hard block fired", String::as_str)
}

/// The executor's verdict on a program under one profile, with the run path's own waiver applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunAdmission {
    /// The executor runs it (a clean verdict, a subprocess-only advisory, or a gateway error —
    /// fail-open).
    Admitted,
    /// The executor refuses it: a hard deny that no waiver covers.
    Denied {
        /// The capability classes X6 denied (`"file-write"`, `"network"`, …).
        classes: Vec<String>,
        /// The reason that kept the deny standing — what the executor prints.
        reason: String,
    },
}

/// The executor's verdict on `code` under `profile` — the SAME composition `touring run` applies:
/// a `Deny` stands unless every denied class is `subprocess` (the waiver).
#[must_use]
pub fn admission(lang: &str, code: &str, profile: &CapabilityProfile) -> RunAdmission {
    match admission_outcome(lang, code, profile) {
        Ok(outcome)
            if outcome.decision.verdict == Verdict::Deny
                && !only_subprocess_denials(&outcome.decision) =>
        {
            RunAdmission::Denied {
                classes: outcome.decision.denied_classes.clone(),
                reason: blocking_reason(&outcome.decision).to_owned(),
            }
        }
        _ => RunAdmission::Admitted,
    }
}

/// Which `touring run` route executes a program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunRoute {
    /// `touring run` as is: the `Sandboxed` profile admits the program.
    Sandboxed,
    /// The `Sandboxed` profile refuses it (`sandbox_denial`), the `Trusted` profile admits it:
    /// `TOURING_TRUSTED_OK=1 touring run --allow-forbidden`. The kernel (Landlock) still confines
    /// the child to the workspace — see [`super::sandbox_executor::sandbox_can_write`].
    Trusted {
        /// Why the Sandboxed profile refused — the executor's own sentence.
        sandbox_denial: String,
    },
    /// Both profiles refuse it: no `touring run` route executes this program.
    Unrunnable {
        /// The Trusted profile's refusal.
        reason: String,
    },
}

/// The route that executes `code` — asked of the executor's own predicate, Sandboxed first, then
/// Trusted. `workspace` is the project root the `Sandboxed` profile reads from.
#[must_use]
pub fn run_route(lang: &str, code: &str, workspace: &Path) -> RunRoute {
    let RunAdmission::Denied { reason, .. } =
        admission(lang, code, &builtins::sandboxed(workspace))
    else {
        return RunRoute::Sandboxed;
    };
    match admission(lang, code, &builtins::trusted()) {
        RunAdmission::Admitted => RunRoute::Trusted {
            sandbox_denial: reason,
        },
        RunAdmission::Denied { reason, .. } => RunRoute::Unrunnable { reason },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ESCRITOR: &str = "import pathlib\np = pathlib.Path(\"RETOMADA.md\")\np.write_text(p.read_text().replace(\"a\", \"b\"), encoding=\"utf-8\")\nprint(\"ok\")";
    const LEITOR: &str = "import pathlib\nprint(pathlib.Path(\"RETOMADA.md\").read_text())";

    /// O caso real do `analise` (05166acc, linha 5786): `Path.write_text` — o Sandboxed nega
    /// (`file-write`), o Trusted admite.
    #[test]
    fn um_corpo_que_escreve_so_roda_no_perfil_trusted() {
        let ws = Path::new("/tmp/ws");
        assert!(matches!(
            admission("python", ESCRITOR, &builtins::sandboxed(ws)),
            RunAdmission::Denied { .. }
        ));
        assert_eq!(
            admission("python", ESCRITOR, &builtins::trusted()),
            RunAdmission::Admitted
        );
        let RunRoute::Trusted { sandbox_denial } = run_route("python", ESCRITOR, ws) else {
            panic!("um corpo que escreve tem rota Trusted");
        };
        assert!(
            sandbox_denial.contains("write_text"),
            "a razão é a do executor: {sandbox_denial}"
        );
    }

    /// Controle positivo: leitura pura roda no Sandboxed — a rota NÃO pede elevação.
    #[test]
    fn um_corpo_de_leitura_roda_no_sandboxed() {
        assert_eq!(
            run_route("python", LEITOR, Path::new("/tmp/ws")),
            RunRoute::Sandboxed
        );
    }

    /// Rede nega nos DOIS perfis (o Trusted nega `Net`): nenhuma rota — a elevação não é uma
    /// porta dos fundos para o que o executor recusa de verdade.
    #[test]
    fn rede_nao_tem_rota_em_perfil_nenhum() {
        let rede = "import socket\ns = socket.create_connection((\"example.com\", 80))";
        assert!(matches!(
            run_route("python", rede, Path::new("/tmp/ws")),
            RunRoute::Unrunnable { .. }
        ));
    }

    /// O waiver é o do executor: um deny só de `subprocess` NÃO é recusa.
    #[test]
    fn deny_so_de_subprocess_e_admitido_como_no_executor() {
        assert_eq!(
            run_route("bash", "sed -n 1,3p arquivo.txt", Path::new("/tmp/ws")),
            RunRoute::Sandboxed
        );
    }

    /// Corpo vazio é erro da camada de entrada, e o preditor falha ABERTO (invariante do CEG).
    #[test]
    fn corpo_vazio_falha_aberto() {
        assert_eq!(
            admission("python", "   ", &builtins::sandboxed(Path::new("/tmp/ws"))),
            RunAdmission::Admitted
        );
    }

    #[test]
    fn tool_for_lang_cobre_os_aliases_do_executor() {
        for (lang, tool) in [
            ("sh", "Bash"),
            ("bash", "Bash"),
            ("py", "SandboxPython"),
            ("python", "SandboxPython"),
            ("node", "SandboxJavaScript"),
            ("ts", "SandboxTypeScript"),
            ("rb", "SandboxRuby"),
            ("go", "SandboxGo"),
            ("rs", "SandboxRust"),
            ("desconhecida", "SandboxPython"),
        ] {
            assert_eq!(tool_for_lang(lang), tool, "{lang}");
        }
    }
}
