//! Which presentation is in force for THIS call, and the two human switches that sit beside it.
//!
//! Precedence, outermost first: the `TOURING_CODE_MODE=<v>` PREFIX of the command → the hook
//! process' `TOURING_CODE_MODE` → the `TOURING_CODE_ONLY=1` alias → `[code_mode] mode` of the
//! project's `.touring/touring.toml` → the armed A/B policy → the default (`both`).

use std::path::Path;

use super::arm::{arm_counts_durable, code_mode_arm_armed, resolve_with_policy};
use super::spec::GATE_BYPASS_TOKEN;

/// `true` when the command IS the sanctioned route (a single program in the sandbox).
pub(super) fn is_sanctioned_route(cmd: &str) -> bool {
    // S24: a path-qualified binary (`.touring/bin/touring run`, the per-project one) is the
    // same route — it was never credited, so a project's own binary looked like a bypass.
    cmd.split(|c: char| c.is_whitespace() || ";|&(".contains(c))
        .collect::<Vec<_>>()
        .windows(2)
        .any(|w| (w[0] == "touring" || w[0].ends_with("/touring")) && w[1] == "run")
}

/// Global kill switch (human; the DAEMON's env — requires `touring daemon-ctl
/// restart` to take effect, like `TOURING_PILLAR_INDUCTION_ARMED`).
/// The pure decision of the HUMAN kill switch — injectable: the hermeticity matrix of
/// analise-6b (08/10) measured 29 red tests with
/// `TOURING_CODE_GATES_DISABLED=1` in the environment — they read the process's switch.
pub(super) fn code_gates_disabled_from(v: Option<&str>) -> bool {
    v == Some("1")
}

pub(super) fn code_gates_disabled() -> bool {
    code_gates_disabled_from(std::env::var("TOURING_CODE_GATES_DISABLED").ok().as_deref())
}

/// The form in which the tools reach the model IN THIS scope.
///
/// The canonical definition lives in [`touring_foundation::code_mode`] because TWO
/// executors enforce it — this hook (`PreToolUse`) and the MCP handshake
/// (`touring-server::server::apply_curation`, S1 TRANSPORT). Duplicating the type and
/// the parser would recreate the failure this workspace has already paid for twice: sites of the
/// same rule that diverge (2026-08-23) and a declaration that promises what the
/// executor does not apply (finding D8, 2026-08-25).
pub(crate) use touring_foundation::code_mode::CodeModePresentation;

// **Calibration REVISED by S3 (27/08/2026) — the fixed list is out, the burst
// predicate is in.** The table below is the one from 25/08 (1,312 calls), kept
// because it is what motivated the revision, with the 27/08 data beside it
// (115 transcripts, 300s window, `scripts/s3_burst_distribution.py`):
//
// | class | collapsed? | calls | % isolated | % volume in burst ≥2 |
// |---|---|---|---|---|
// | `grep`  | yes  | 770 | 12.6% | 87.4% |
// | `sed-n` | NO   | 408 | 18.6% | 81.4% |
// | `ls`    | NO   | 338 | 28.7% | 71.3% |
// | `cat`   | yes  | 316 | 26.6% | 73.4% |
// | `wc`    | NO   |  34 | 67.6% | 32.4% |
// | `find`  | yes  |  32 | 56.2% | 43.8% (zero bursts ≥3) |
//
// The fixed list was wrong in both directions at once: it denied `find`, which is
// mostly isolated and did not produce ONE burst ≥3 in 115 transcripts, and
// let `sed-n` and `ls` pass, which together add up to 746 calls with ~76% of the
// volume in bursts. A class name does not predict fan-out; what predicts it is what the
// class is doing IN THIS window — and a counter answers that, a list does not.
//
// The per-class levels are exactly the design that DeepSeek DEFERRED (*"its
// design depends on evidence about how models split usage under `both`"*). The
// evidence they lacked is this table: we ran under instrumented `both`,
// and then measured again to correct our own first reading.

/// Resolves the presentation: **command prefix → hook env → alias → project → default**.
///
/// Two measured truths (25/08) live in this order:
///
/// 1. **Exporting in the Bash tool's shell does NOT reach the hook.** The hook process
///    and the shell that executes the command are SIBLINGS spawned by Claude Code —
///    `export TOURING_CODE_MODE=…` in one is invisible to the other. The env that this
///    function reads with `std::env::var` is that of the hook process (that of the daemon that
///    spawned it), not the operator's "session" one.
/// 2. **The route that gets through is the command line.** That is how
///    `TOURING_GATE_OK=1 <cmd>` already works: the hook reads the `VAR=value` prefix
///    from the command itself. That is why the outermost level is the prefix — useful
///    above all to RELAX per command (`TOURING_CODE_MODE=native grep …`),
///    symmetric to the bypass token.
///
/// `TOURING_CODE_ONLY=1` still counts as an alias of `Code` (compatibility
/// with the S-8.1 pilot), now with the scope and calibration it lacked — it
/// denied EVERY class, `ls` included.
///
/// The «hook env» and «alias» levels arrive INJECTED (`CodeModeEnv`), never
/// read from the process in the middle of the resolution: the hermeticity matrix of
/// analise-6b (08/10) measured 15 red tests under `TOURING_CODE_MODE=native`
/// in the environment — they read `Native` from the environment instead of the case under test (the env
/// wins over the scratch project's `touring.toml`, by documented precedence).
#[derive(Default, Clone)]
pub(crate) struct CodeModeEnv {
    /// Value of `TOURING_CODE_MODE` (raw — parsing is part of the behavior).
    pub mode: Option<String>,
    /// Value of `TOURING_CODE_ONLY` (the legacy alias of `Code`).
    pub code_only_alias: Option<String>,
    /// `TOURING_CODE_MODE_ARM_ARMED` (the A/B arm policy only acts ARMED). Read once
    /// here, never in the middle of the resolution: the pure predicate `resolve_with_policy` already received
    /// the datum, but whoever delivered it read the process env.
    pub arm_armed: bool,
}

impl CodeModeEnv {
    pub(crate) fn from_env() -> Self {
        Self {
            mode: std::env::var("TOURING_CODE_MODE").ok(),
            code_only_alias: std::env::var("TOURING_CODE_ONLY").ok(),
            arm_armed: code_mode_arm_armed(),
        }
    }

    /// The pure decision of the env+alias levels — testable without touching the process.
    fn resolve(&self) -> Option<CodeModePresentation> {
        // parse() is the canonical parser (trim + unquote): a value in quotes
        // also resolves.
        if let Some(m) = self.mode.as_deref().and_then(CodeModePresentation::parse) {
            return Some(m);
        }
        if self.code_only_alias.as_deref() == Some("1") {
            return Some(CodeModePresentation::Code);
        }
        None
    }
}

pub(super) fn resolve_presentation(
    project_root: &Path,
    cmd: &str,
    env: &CodeModeEnv,
) -> CodeModePresentation {
    for token in cmd.split_whitespace() {
        let Some((nome, valor)) = token.split_once('=') else {
            break;
        };
        if !nome
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        {
            break;
        }
        if nome == "TOURING_CODE_MODE" {
            match CodeModePresentation::parse(valor) {
                Some(m) => return m,
                None => break, // invalid value does not silence the following levels
            }
        }
    }
    if let Some(m) = env.resolve() {
        return m;
    }
    // The human declaration ALWAYS wins: prefix, env, alias and `touring.toml` have already
    // decided above. The policy only fills the space the human left
    // open — overriding an explicit declaration would not be learning, it would be
    // disobeying.
    resolve_with_policy(
        project_presentation(project_root),
        env.arm_armed,
        arm_counts_durable(project_root),
    )
}

/// Reads `[code_mode] mode` from `<root>/.touring/touring.toml`.
///
/// Delegates to the canonical parser of `touring-foundation` — the same one the MCP
/// handshake consults, so that the declared and the enforced presentation can never
/// diverge. Re-exported in the module scope because the tests consult it
/// directly.
pub(crate) use touring_foundation::code_mode::project_presentation;

/// S24 — the bypass token counts only where the SHELL reads it (a command prefix), never
/// inside a heredoc body or a quoted string: a test file whose text mentions the token was
/// charged as a bypass and denied by G11 on 25/09.
pub(super) fn has_bypass_token(cmd: &str) -> bool {
    touring_ceg::gateway::gate::mask_shell_data(cmd).contains(GATE_BYPASS_TOKEN)
}
