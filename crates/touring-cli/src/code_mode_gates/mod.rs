//! The code-mode gates: the decision to let a tool call run, speak beside it, rewrite it, or
//! deny it — and the memory that decision needs.
//!
//! # The interface
//!
//! ```text
//! evaluate(&GateState, &ToolEvent, &GateConfig) -> Decision { verdict, advisories }
//! ```
//!
//! * **`ToolEvent`** — the call: project root, actor (session or session+agent), tool, input.
//! * **`GateState`** — what the session did recently: every ledger, every window. One store per
//!   daemon ([`GateState::global`]); a test builds its own and shares nothing.
//! * **`GateConfig`** — the environment an operator controls, INJECTED (never read mid-gate):
//!   the kill switch `TOURING_CODE_GATES_DISABLED`, the rewrite switches, the code-mode env
//!   levels. Production builds it with [`GateConfig::from_env`].
//! * **`Decision`** — a typed [`Verdict`] (`Allow` / `Advise` / `Rewrite` / `Deny{gate, reason,
//!   fused_program}`) plus the advisories that ride along. `evaluate` updates the state; the
//!   caller only formats.
//!
//! Everything else is behind the interface: the windows and thresholds ([`spec`]), the
//! precedence that decides which presentation is in force ([`mode`]), the shell-aware
//! classification of a command ([`shell`]), the words and programs a verdict hands over
//! ([`remedy`]), and the A/B arm whose evidence may — only when armed, never over a human
//! declaration — pick the presentation ([`arm`]).
//!
//! # The contract (workspace `CLAUDE.md` §10)
//!
//! Inspection in isolation passes; a burst collapses. After a deny, change the call — the reason
//! names the route back. Precedence: `TOURING_CODE_MODE=<v>` prefix of the command → hook env →
//! `.touring/touring.toml` `[code_mode] mode` → default `both`. Kill switch (human):
//! `TOURING_CODE_GATES_DISABLED=1`. The deny texts are read by the model, by `tool_trio` and by
//! the test rubric: the text and the predicate derive from one source (rule D8).
//!
//! # Proof
//!
//! `tests/` drives a table of event sequences through [`evaluate`] and compares every response,
//! byte for byte, with the corpus recorded before the gates left `cli_suggester`.

mod arm;
mod chain;
mod g10_route;
mod mode;
mod remedy;
mod shell;
pub(crate) mod spec;
mod state;

#[cfg(test)]
mod tests;

use serde_json::Value;
use std::path::Path;

use self::{mode::*, remedy::*, shell::*, spec::*, state::*};

#[cfg(test)]
pub use self::arm::turn_ledger_insert_for_test;
pub(crate) use self::arm::{
    ARM_MIN_SAMPLE, bump_durable_evidence, read_durable_evidence, record_route_offer,
};
pub use self::arm::{RouteOffer, claim_route_reward, take_route_offer};
pub(crate) use self::mode::{CodeModeEnv, CodeModePresentation};
pub(crate) use self::remedy::portfolio_index;
pub(crate) use self::shell::{
    effective_tokens, inline_edit_target, is_inline_blind_edit, scan_class_of,
};
pub(crate) use self::spec::{
    CACHE_MAX_CAPACITY, CODE_MODE_WINDOW_SECS, CONFORMAL_TAU_MAX_PROJECTS, SUGGESTION_TTL_SECS,
};
pub(crate) use self::state::GateState;

// ── The vocabulary ───────────────────────────────────────────────────────────────────────

/// Which gate spoke. The label is the `[G<n> …]` prefix of its reason — the id `tool_trio`
/// reads back from a deny.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Gate {
    G1,
    G2,
    G3,
    G5,
    G6,
    G7,
    G8,
    G9,
    G10,
    G11,
    /// The inspection-burst warning of the `code` presentation (S3).
    S3,
    /// The delegation advisory at the tenth distinct file (M3).
    M3,
    /// The counterfactual-comment advisory (E3).
    E3,
}

/// What the chain decided for one tool call.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Verdict {
    /// No gate acts: the call runs untouched.
    Allow,
    /// The call runs; `context` rides beside it (`additionalContext`).
    Advise { gate: Gate, context: String },
    /// The call is replaced: `updated_input` runs in its place (spike S-8.6). `context`, when
    /// present, is what the MODEL reads — an `allow`'s reason reaches only the debug log.
    Rewrite {
        gate: Gate,
        reason: String,
        context: Option<String>,
        updated_input: Value,
    },
    /// The call does not run. `reason` teaches the route back; `fused_program` is the program
    /// the deny hands over when the gate fused a burst into one.
    Deny {
        gate: Gate,
        reason: String,
        fused_program: Option<String>,
    },
}

impl Verdict {
    pub(crate) fn deny(gate: Gate, reason: String) -> Self {
        Verdict::Deny {
            gate,
            reason,
            fused_program: None,
        }
    }

    pub(crate) fn advise(gate: Gate, context: String) -> Self {
        Verdict::Advise { gate, context }
    }
}

/// A note that rides along with whatever else the chain decides (G5's telemetry, the S3 burst
/// warning). A gate that only advises never ends the chain.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Advisory {
    pub gate: Gate,
    pub text: String,
}

impl Advisory {
    pub(crate) fn new(gate: Gate, text: String) -> Self {
        Self { gate, text }
    }
}

/// The verdict plus the advisories that ride along: a gate that only advises never ends the
/// chain, and its text is never lost — it travels with the deny that wins.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Decision {
    pub verdict: Verdict,
    pub advisories: Vec<Advisory>,
}

/// One tool call, as the hook receives it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ToolEvent<'a> {
    pub project_root: &'a Path,
    /// The actor: the session id, suffixed by the agent id for a subagent. Every window of
    /// every gate belongs to ONE actor.
    pub session: &'a str,
    pub tool_name: &'a str,
    pub tool_input: &'a Value,
}

/// Overrides dos pontos que leem env global, como objeto nomeado (diretriz A2)
/// — produção usa `from_env()`; testes constroem o literal e NUNCA tocam o env
/// do processo (a race medida em 08/10 pela analise-6b: `set_var` num teste vaza
/// para os outros testes em threads paralelas do mesmo binário — 41/150 rodadas
/// no `dedupe_`; é por isso que o Rust 2024 marcou `set_var` como unsafe).
#[derive(Default, Clone)]
pub(crate) struct GateConfig {
    /// TOURING_G2_REWRITE_DISABLED=1 → o G2 nega em vez de reescrever.
    pub g2_rewrite_disabled: bool,
    /// TOURING_G1_REWRITE_DISABLED=1 → o G1 (rajada de inspeção) volta ao deny
    /// clássico em vez de reescrever para o `touring run` (V0e1: o default é
    /// EXECUTAR a rota via updatedInput).
    pub g1_rewrite_disabled: bool,
    /// TOURING_PORTFOLIO_DIR → o portfólio lido de outro dir (testes/scratch).
    pub portfolio_dir: Option<std::path::PathBuf>,
    /// Os níveis «env» + «alias» da resolução do code mode (TOURING_CODE_MODE /
    /// TOURING_CODE_ONLY) — lidos uma vez aqui, nunca no meio do gate.
    pub code_mode_env: CodeModeEnv,
    /// TOURING_CODE_GATES_DISABLED=1 → o kill switch humano desliga TODOS os
    /// gates (29 testes liam isso do ambiente — matriz da 6b, 08/10).
    pub code_gates_disabled: bool,
    /// Precisão viva do G1 INJETADA (`(precisão, volume)`): `None` lê o contador global
    /// (`g1_live_precision()`, o comportamento de produção; `from_env` nunca o preenche);
    /// `Some(x)` fixa o dado — o ramo advisory do G1 só dispara com precisão abaixo do
    /// piso, e o contador global não é controlável por um teste.
    pub g1_precision: Option<Option<(f64, u64)>>,
    /// O esqueleto R1 INJETADO: `None` = produção (procura o arquivo sob `$HOME`);
    /// `Some(x)` fixa o dado. Sem isto o texto do G7 dependia de um arquivo da máquina
    /// do desenvolvedor, e um teste que afirmava o texto reprovava em qualquer outra.
    pub r1_skeleton: Option<Option<std::path::PathBuf>>,
}

impl GateConfig {
    /// O esqueleto R1 em vigor: o injetado, ou o da máquina (produção).
    fn r1_skeleton(&self) -> Option<std::path::PathBuf> {
        match &self.r1_skeleton {
            Some(fixed) => fixed.clone(),
            None => r1_skeleton_path(),
        }
    }

    /// The operator's environment, read ONCE (production).
    pub(crate) fn from_env() -> Self {
        Self {
            g2_rewrite_disabled: std::env::var("TOURING_G2_REWRITE_DISABLED")
                .map(|v| v == "1")
                .unwrap_or(false),
            g1_rewrite_disabled: std::env::var("TOURING_G1_REWRITE_DISABLED")
                .map(|v| v == "1")
                .unwrap_or(false),
            portfolio_dir: std::env::var("TOURING_PORTFOLIO_DIR")
                .ok()
                .map(std::path::PathBuf::from),
            code_mode_env: CodeModeEnv::from_env(),
            code_gates_disabled: code_gates_disabled(),
            g1_precision: None,
            r1_skeleton: None,
        }
    }
}

// ── The actor ────────────────────────────────────────────────────────────────────────────

/// ACTOR key of every per-session window (F2 uptake cache, G1/G3/G7/G10/G11, T3-B).
///
/// S23 (25/09/2026): the actor is `(session_id, agent_id)`. A subagent inherits its
/// parent's `session_id`, so keying by session alone let a subagent's greps and bypasses
/// spend its parent's windows (measured live). A payload without `session_id` gets its own
/// `"anon"` key — never a slot shared with real sessions (was `"default"`).
pub(crate) fn session_key(payload: &Value) -> String {
    let session = payload
        .get("session_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("anon");
    match payload
        .get("agent_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        Some(agent) => format!("{session}\u{2}agent:{agent}"),
        None => session.to_string(),
    }
}

// ── The entry point ──────────────────────────────────────────────────────────────────────

/// Decide one tool call and record it in `state`.
///
/// The chain runs BEFORE the suggester's anti-spam cache — a repeated deny must never be
/// swallowed by it — and returns the MOST SEVERE verdict: no advisory ends the chain, an
/// applicable deny anywhere in it wins, and the advisories ride along.
pub(crate) fn evaluate(state: &GateState, event: &ToolEvent<'_>, cfg: &GateConfig) -> Decision {
    let mut advisories: Vec<Advisory> = Vec::new();
    let verdict = chain::run_chain(state, event, cfg, &mut advisories).unwrap_or(Verdict::Allow);
    Decision {
        verdict,
        advisories,
    }
}
