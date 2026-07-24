//! Stage **X6 CAPABILITY-GATE** of the Code Execution Gateway. Phase **P3.6**
//! of CEG Pln2 (`docs/2026-05-17-ceg-pln2-plan.md`).
//!
//! X5 ran the code once in the sandbox; X6 asks the next question: *if this
//! code runs for real, what authority would it exercise, and does the granted
//! [`CapabilityProfile`] permit it?*
//!
//! Two steps:
//!
//! 1. [`required_capabilities`] derives — lexically — the [`Capability`] set the
//!    code body would exercise: subprocess spawns, file writes, outbound
//!    network connections, environment reads.
//! 2. [`gate_capabilities`] resolves each required capability against the
//!    granted profile — deny-by-default, deny-wins (see [`crate::capability`])
//!    — producing a [`GateReport`].
//!
//! # A lexical, fail-closed heuristic
//!
//! Capability extraction is **lexical**, not semantic — a token scan, the same
//! discipline as X3's [`extract_symbols`](super::vgp_stage::extract_symbols). It
//! deliberately over-detects: when a scope cannot be determined it is recorded
//! at its broadest (an `FsWrite` of `/`, a `Run` of an unknown command), so the
//! gate resolves it against the profile's *default* disposition. Under a
//! deny-by-default profile that fails **closed** — the safe direction for a
//! security gate. A precise, AST-level capability extractor is a future
//! refinement, and would only ever *narrow* a detected capability, never miss
//! one.
//!
//! `FsRead` is deliberately not detected: every built-in profile already grants
//! workspace read, so flagging it would be pure noise. X6 concentrates on the
//! four authority classes a profile actually restricts — file write, network,
//! subprocess, environment.

use super::capture::ExecSurface;
use super::typestate::{Execution, Gated, SandboxTested};
use crate::capability::{
    Capability, CapabilityProfile, CmdScope, Decision, HostScope, KeyScope, PathScope,
};
use serde::{Deserialize, Serialize};

// ── Required-capability extraction ────────────────────────────────────────────

/// A single [`Capability`] the executed code was inferred to require, paired
/// with the lexical `operation` — a command name or source token — that
/// triggered the inference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityNeed {
    /// The authority the code would exercise.
    pub capability: Capability,
    /// The command or source token that revealed the need — kept for the
    /// human-readable gate report and the X7 canonical fix.
    pub operation: String,
}

impl CapabilityNeed {
    /// Pair a [`Capability`] with the `operation` that revealed it.
    #[must_use]
    pub fn new(capability: Capability, operation: impl Into<String>) -> Self {
        Self {
            capability,
            operation: operation.into(),
        }
    }
}

/// Shell metacharacters that separate one command from the next.
const SHELL_SEPARATORS: &[char] = &[';', '|', '&', '\n', '(', ')'];

/// Shell commands that open an outbound network connection.
const NET_COMMANDS: &[&str] = &[
    "curl", "wget", "nc", "ncat", "socat", "ssh", "scp", "sftp", "rsync", "ftp", "telnet",
];

/// Source tokens that signal a subprocess spawn, across the sandbox languages.
///
/// A bare `system(` covers the Python `os.system(...)`, C `system(...)` and
/// Ruby `Kernel.system(...)` call forms, so no language-prefixed alias is
/// needed for the dominant case.
const RUN_TOKENS: &[&str] = &[
    "subprocess",
    "os.popen",
    "os.spawn",
    "child_process",
    "popen(",
    "Popen",
    "system(",
    "spawn(",
    "Runtime.getRuntime",
    "ProcessBuilder",
    "IO.popen",
    "std::process::Command",
];

/// Source tokens that signal an outbound network connection.
const NET_TOKENS: &[&str] = &[
    "socket",
    "urllib",
    "http.client",
    "requests.",
    "httpx",
    "fetch(",
    "XMLHttpRequest",
    "Net::HTTP",
    "net/http",
    "axios",
    "urlopen",
    "aiohttp",
    "reqwest",
    "HttpClient",
];

/// Source tokens that signal a filesystem write.
const WRITE_TOKENS: &[&str] = &[
    ".write(",
    "writeFile",
    "File.write",
    "fs.write",
    "ofstream",
    "O_WRONLY",
    "O_CREAT",
    "Files.write",
    "shutil.copy",
    "shutil.move",
    "os.remove",
    "os.unlink",
    "os.mkdir",
];

/// Source tokens that signal an environment-variable read.
const ENV_TOKENS: &[&str] = &[
    "os.environ",
    "getenv",
    "process.env",
    "ENV[",
    "std::env::var",
    "System.getenv",
];

/// Derive the [`Capability`] set the code body would exercise if run for real.
///
/// Dispatches on the [`ExecSurface`]: a bash surface is tokenised into shell
/// commands; a `ctx_execute` / inferlet surface is scanned for the per-language
/// capability tokens. Each distinct capability appears once, in first-seen
/// order — the result is deterministic.
#[must_use]
pub fn required_capabilities(code: &str, surface: ExecSurface) -> Vec<CapabilityNeed> {
    match surface {
        ExecSurface::BashCommand => bash_capability_needs(code),
        ExecSurface::CtxExecute | ExecSurface::Inferlet => code_capability_needs(code),
        ExecSurface::NonExec => Vec::new(),
    }
}

/// Append `need` only if its [`Capability`] is not already present — keeps the
/// need list deduplicated by capability while preserving first-seen order.
fn push_unique(needs: &mut Vec<CapabilityNeed>, need: CapabilityNeed) {
    if !needs.iter().any(|n| n.capability == need.capability) {
        needs.push(need);
    }
}

/// The first whitespace-token of `segment` that is a command, with any
/// `VAR=value` prefix skipped and any directory prefix stripped (`/bin/rm` →
/// `rm`), so the result lines up with a profile's [`CmdScope`] grants.
fn leading_command(segment: &str) -> Option<&str> {
    segment
        .split_whitespace()
        .find(|tok| !tok.contains('='))
        .map(|tok| tok.rsplit('/').next().unwrap_or(tok))
        .filter(|cmd| !cmd.is_empty())
}

/// `true` when the shell code redirects to, or `tee`s into, a file.
///
/// File-descriptor duplication (`2>&1`, `>&2`) is excluded — it redirects an
/// fd, it does not name a file to write.
fn bash_writes_a_file(code: &str) -> bool {
    if code.contains("tee ") || code.contains("| tee") {
        return true;
    }
    let bytes = code.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b != b'>' {
            continue;
        }
        if bytes.get(i + 1) == Some(&b'&') {
            continue; // `>&` — fd duplication, not a file write.
        }
        let mut j = i + 1;
        while bytes.get(j) == Some(&b'>') {
            j += 1; // `>>` append redirection.
        }
        while bytes.get(j).is_some_and(u8::is_ascii_whitespace) {
            j += 1;
        }
        if bytes.get(j).is_some_and(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, b'/' | b'.' | b'_' | b'~' | b'$' | b'"' | b'\'')
        }) {
            return true;
        }
    }
    false
}

/// Extract the capability needs of a shell command body.
fn bash_capability_needs(code: &str) -> Vec<CapabilityNeed> {
    let mut needs = Vec::new();
    for segment in code.split(SHELL_SEPARATORS) {
        let Some(command) = leading_command(segment) else {
            continue;
        };
        if NET_COMMANDS.contains(&command) {
            push_unique(
                &mut needs,
                CapabilityNeed::new(Capability::Net(HostScope::any()), command),
            );
        }
        push_unique(
            &mut needs,
            CapabilityNeed::new(Capability::Run(CmdScope::new(command)), command),
        );
    }
    if bash_writes_a_file(code) {
        push_unique(
            &mut needs,
            CapabilityNeed::new(Capability::FsWrite(PathScope::new("/")), "redirection"),
        );
    }
    needs
}

/// The first token of `tokens` that appears anywhere in `code`.
fn first_token_in<'a>(code: &str, tokens: &[&'a str]) -> Option<&'a str> {
    tokens.iter().copied().find(|t| code.contains(t))
}

/// Extract the capability needs of an inline code body (`ctx_execute`,
/// inferlet) via the per-class token tables.
fn code_capability_needs(code: &str) -> Vec<CapabilityNeed> {
    let mut needs = Vec::new();
    if let Some(tok) = first_token_in(code, RUN_TOKENS) {
        push_unique(
            &mut needs,
            CapabilityNeed::new(Capability::Run(CmdScope::any()), tok),
        );
    }
    if let Some(tok) = first_token_in(code, NET_TOKENS) {
        push_unique(
            &mut needs,
            CapabilityNeed::new(Capability::Net(HostScope::any()), tok),
        );
    }
    if let Some(tok) = first_token_in(code, WRITE_TOKENS) {
        push_unique(
            &mut needs,
            CapabilityNeed::new(Capability::FsWrite(PathScope::new("/")), tok),
        );
    }
    if let Some(tok) = first_token_in(code, ENV_TOKENS) {
        push_unique(
            &mut needs,
            CapabilityNeed::new(Capability::Env(KeyScope::new("*")), tok),
        );
    }
    needs
}

/// The authority class of a [`Capability`], as a stable label for reports and
/// the X7 canonical fix.
#[must_use]
pub fn capability_class(capability: &Capability) -> &'static str {
    match capability {
        Capability::FsRead(_) => "file-read",
        Capability::FsWrite(_) => "file-write",
        Capability::Net(_) => "network",
        Capability::Run(_) => "subprocess",
        Capability::Env(_) => "environment",
    }
}

/// `true` for the authority classes a profile genuinely restricts — a denied
/// one of these is a hard block at X7. An `Env` / `FsRead` denial is a warning,
/// not a hard block: a generic environment read cannot be told apart from a
/// credential read at this lexical level.
fn is_high_authority(capability: &Capability) -> bool {
    matches!(
        capability,
        Capability::Run(_) | Capability::FsWrite(_) | Capability::Net(_)
    )
}

// ── The gate report ───────────────────────────────────────────────────────────

/// One [`CapabilityNeed`] resolved against the granted [`CapabilityProfile`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatedCapability {
    /// The required authority.
    pub capability: Capability,
    /// The operation that revealed the need.
    pub operation: String,
    /// How the granted profile resolved the request.
    pub decision: Decision,
}

/// The **X6 CAPABILITY-GATE** result: every required capability resolved
/// against the granted profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateReport {
    /// The name of the profile the requests were resolved against.
    pub profile_name: String,
    /// One entry per required capability, in first-seen order.
    pub gated: Vec<GatedCapability>,
}

/// Rank a [`Decision`] by severity — `Allow < Prompt < Deny`.
fn decision_rank(decision: Decision) -> u8 {
    match decision {
        Decision::Allow => 0,
        Decision::Prompt => 1,
        Decision::Deny => 2,
    }
}

impl GateReport {
    /// The most severe [`Decision`] across every gated capability —
    /// `Allow < Prompt < Deny`. An empty report (the code needs no restricted
    /// authority) resolves to [`Decision::Allow`].
    #[must_use]
    pub fn worst_decision(&self) -> Decision {
        self.gated
            .iter()
            .map(|g| g.decision)
            .max_by_key(|&d| decision_rank(d))
            .unwrap_or(Decision::Allow)
    }

    /// `true` when every required capability resolved to [`Decision::Allow`]
    /// (vacuously true for an empty report).
    #[must_use]
    pub fn is_clear(&self) -> bool {
        self.gated.iter().all(|g| g.decision == Decision::Allow)
    }

    /// The first denied high-authority capability — a subprocess, file write or
    /// network connection the profile refused. This is the hard-block signal
    /// the X7 DECISION stage acts on.
    #[must_use]
    pub fn first_blocking_denial(&self) -> Option<&GatedCapability> {
        self.gated
            .iter()
            .find(|g| g.decision == Decision::Deny && is_high_authority(&g.capability))
    }

    /// `true` when at least one high-authority capability was denied.
    #[must_use]
    pub fn has_blocking_denial(&self) -> bool {
        self.first_blocking_denial().is_some()
    }

    /// Every denied capability, regardless of authority class.
    pub fn denied(&self) -> impl Iterator<Item = &GatedCapability> {
        self.gated.iter().filter(|g| g.decision == Decision::Deny)
    }
}

/// Resolve every [`CapabilityNeed`] against `profile`, producing the
/// **X6 CAPABILITY-GATE** [`GateReport`].
#[must_use]
pub fn gate_capabilities(needs: &[CapabilityNeed], profile: &CapabilityProfile) -> GateReport {
    let gated = needs
        .iter()
        .map(|n| GatedCapability {
            capability: n.capability.clone(),
            operation: n.operation.clone(),
            decision: profile.resolve(&n.capability),
        })
        .collect();
    GateReport {
        profile_name: profile.name().to_owned(),
        gated,
    }
}

// ── X6 transition ─────────────────────────────────────────────────────────────

impl Execution<SandboxTested> {
    /// **X6 CAPABILITY-GATE** — derive the code's required capabilities, resolve
    /// them against `profile`, attach the [`GateReport`] to the evidence ledger
    /// and advance to [`Gated`].
    ///
    /// The code body and surface come from the X1
    /// [`Classification`](super::classify::Classification) when one is on the
    /// ledger; absent it (an execution advanced here by bare
    /// [`advance`](Execution::advance)), the raw payload and the tool-name
    /// surface are used instead — so the gate is always sound.
    pub fn capability_gate(mut self, profile: &CapabilityProfile) -> Execution<Gated> {
        let report = {
            let (code, surface) = match self.evidence().classification.as_ref() {
                Some(cls) => (cls.code_body.source.as_str(), cls.surface),
                None => (
                    self.raw().payload.as_str(),
                    ExecSurface::detect(&self.raw().tool),
                ),
            };
            let needs = required_capabilities(code, surface);
            gate_capabilities(&needs, profile)
        };
        self.evidence_mut().gate_report = Some(report);
        self.advance()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::builtins::{sandboxed, trusted};
    use crate::gateway::capture_tool_call;
    use std::path::Path;

    fn ws() -> &'static Path {
        Path::new("/ws")
    }

    // ── required_capabilities — bash surface ──────────────────────────────

    #[test]
    fn required_capabilities_bash_dedups_repeated_command() {
        let needs = required_capabilities("cargo build && cargo test", ExecSurface::BashCommand);
        let runs: Vec<_> = needs
            .iter()
            .filter(|n| matches!(n.capability, Capability::Run(_)))
            .collect();
        assert_eq!(runs.len(), 1, "`cargo` appears twice but needs one Run");
        assert_eq!(runs[0].operation, "cargo");
    }

    #[test]
    fn required_capabilities_bash_distinct_commands() {
        let needs = required_capabilities("git status; rm tmp", ExecSurface::BashCommand);
        let ops: Vec<&str> = needs.iter().map(|n| n.operation.as_str()).collect();
        assert!(ops.contains(&"git"), "got {ops:?}");
        assert!(ops.contains(&"rm"), "got {ops:?}");
    }

    #[test]
    fn required_capabilities_bash_strips_directory_prefix() {
        // `/usr/bin/rm` must reduce to `rm` so it lines up with a CmdScope grant.
        let needs = required_capabilities("/usr/bin/rm tmp", ExecSurface::BashCommand);
        assert!(needs.iter().any(|n| n.operation == "rm"), "{needs:?}");
    }

    #[test]
    fn required_capabilities_bash_redirection_is_fswrite() {
        let needs = required_capabilities("echo hi > out.txt", ExecSurface::BashCommand);
        assert!(
            needs
                .iter()
                .any(|n| matches!(n.capability, Capability::FsWrite(_))),
            "a `>` redirection must require FsWrite: {needs:?}"
        );
    }

    #[test]
    fn required_capabilities_bash_fd_dup_is_not_a_write() {
        // `2>&1` redirects a file descriptor — it does not write a file.
        let needs = required_capabilities("ls -la 2>&1", ExecSurface::BashCommand);
        assert!(
            !needs
                .iter()
                .any(|n| matches!(n.capability, Capability::FsWrite(_))),
            "fd duplication is not a file write: {needs:?}"
        );
    }

    #[test]
    fn required_capabilities_bash_network_command() {
        let needs = required_capabilities("curl https://example.test", ExecSurface::BashCommand);
        assert!(
            needs
                .iter()
                .any(|n| matches!(n.capability, Capability::Net(_))),
            "`curl` must require Net: {needs:?}"
        );
        assert!(
            needs
                .iter()
                .any(|n| matches!(n.capability, Capability::Run(_)))
        );
    }

    // ── required_capabilities — code surface ──────────────────────────────

    #[test]
    fn required_capabilities_code_subprocess() {
        let needs = required_capabilities(
            "import subprocess; subprocess.run(['ls'])",
            ExecSurface::CtxExecute,
        );
        assert!(
            needs
                .iter()
                .any(|n| matches!(n.capability, Capability::Run(_))),
            "{needs:?}"
        );
    }

    #[test]
    fn required_capabilities_code_network() {
        let needs = required_capabilities("requests.get('http://x')", ExecSurface::CtxExecute);
        assert!(
            needs
                .iter()
                .any(|n| matches!(n.capability, Capability::Net(_))),
            "{needs:?}"
        );
    }

    #[test]
    fn required_capabilities_code_write_and_env() {
        let needs = required_capabilities(
            "open('o.txt','w').write(os.environ['HOME'])",
            ExecSurface::CtxExecute,
        );
        assert!(
            needs
                .iter()
                .any(|n| matches!(n.capability, Capability::FsWrite(_)))
        );
        assert!(
            needs
                .iter()
                .any(|n| matches!(n.capability, Capability::Env(_)))
        );
    }

    #[test]
    fn required_capabilities_nonexec_surface_is_empty() {
        assert!(required_capabilities("anything", ExecSurface::NonExec).is_empty());
    }

    #[test]
    fn required_capabilities_pure_code_needs_nothing() {
        // No capability token — a pure computation requires no restricted authority.
        assert!(required_capabilities("total = 2 + 2", ExecSurface::CtxExecute).is_empty());
    }

    // ── gate_capabilities + GateReport ────────────────────────────────────

    #[test]
    fn gate_capabilities_denies_run_under_sandboxed() {
        let needs = vec![CapabilityNeed::new(
            Capability::Run(CmdScope::new("python3")),
            "python3",
        )];
        let report = gate_capabilities(&needs, &sandboxed(ws()));
        assert_eq!(report.profile_name, "Sandboxed");
        assert_eq!(report.gated[0].decision, Decision::Deny);
        assert!(!report.is_clear());
        assert_eq!(report.worst_decision(), Decision::Deny);
    }

    #[test]
    fn gate_capabilities_allows_safe_run_under_trusted() {
        let needs = vec![CapabilityNeed::new(
            Capability::Run(CmdScope::new("cargo")),
            "cargo",
        )];
        let report = gate_capabilities(&needs, &trusted());
        assert_eq!(report.gated[0].decision, Decision::Allow);
        assert!(report.is_clear());
        assert_eq!(report.worst_decision(), Decision::Allow);
    }

    #[test]
    fn empty_gate_report_is_clear_and_allows() {
        let report = gate_capabilities(&[], &sandboxed(ws()));
        assert!(report.is_clear());
        assert_eq!(report.worst_decision(), Decision::Allow);
        assert!(!report.has_blocking_denial());
    }

    #[test]
    fn first_blocking_denial_ignores_low_authority_env() {
        // A denied Run is a hard block; a denied Env is not — a generic env read
        // cannot be told from a credential read at the lexical level.
        let run_denied = gate_capabilities(
            &[CapabilityNeed::new(
                Capability::Run(CmdScope::new("rm")),
                "rm",
            )],
            &sandboxed(ws()),
        );
        assert!(run_denied.has_blocking_denial());

        let env_denied = gate_capabilities(
            &[CapabilityNeed::new(
                Capability::Env(KeyScope::new("*")),
                "environ",
            )],
            &sandboxed(ws()),
        );
        assert_eq!(env_denied.worst_decision(), Decision::Deny);
        assert!(
            !env_denied.has_blocking_denial(),
            "an Env denial alone is not a hard block"
        );
        assert_eq!(env_denied.denied().count(), 1);
    }

    #[test]
    fn capability_class_labels_every_variant() {
        assert_eq!(
            capability_class(&Capability::FsRead(PathScope::new("/"))),
            "file-read"
        );
        assert_eq!(
            capability_class(&Capability::FsWrite(PathScope::new("/"))),
            "file-write"
        );
        assert_eq!(
            capability_class(&Capability::Net(HostScope::any())),
            "network"
        );
        assert_eq!(
            capability_class(&Capability::Run(CmdScope::any())),
            "subprocess"
        );
        assert_eq!(
            capability_class(&Capability::Env(KeyScope::new("X"))),
            "environment"
        );
    }

    #[test]
    fn gate_report_serde_roundtrip() {
        let report = gate_capabilities(
            &[CapabilityNeed::new(
                Capability::Run(CmdScope::new("rm")),
                "rm",
            )],
            &trusted(),
        );
        let json = serde_json::to_string(&report).expect("serialize");
        let back: GateReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(report, back);
    }

    // ── X6 transition ─────────────────────────────────────────────────────

    #[test]
    fn capability_gate_transition_attaches_report_and_advances() {
        // Bare `advance()` to X5 — no classification on the ledger — exercises
        // the raw-payload fallback inside `capability_gate`.
        // ES1 P3 (2026-06-01): X3.5 PROVE inserted between X3 and X4,
        // so 5 advances become 6; ordinals renumber accordingly.
        let sandbox_tested = capture_tool_call("Bash", "cargo build > log.txt", None)
            .expect("Bash is code-bearing")
            .advance() // X1
            .advance() // X2
            .advance() // X3
            .advance() // X3.5 PROVE
            .advance() // X4
            .advance(); // X5
        assert_eq!(sandbox_tested.ordinal(), 6);

        let gated = sandbox_tested.capability_gate(&trusted());
        assert_eq!(gated.ordinal(), 7);
        assert_eq!(gated.stage(), "X6-CAPABILITY-GATE");
        let report = gated
            .evidence()
            .gate_report
            .as_ref()
            .expect("capability_gate must attach a GateReport");
        assert_eq!(report.profile_name, "Trusted");
        // `cargo` (Run) + the `> log.txt` redirection (FsWrite).
        assert!(report.gated.iter().any(|g| g.operation == "cargo"));
        assert!(
            report
                .gated
                .iter()
                .any(|g| matches!(g.capability, Capability::FsWrite(_)))
        );
    }
}
