//! R3 — Master CLI commands: thin wrappers over the Touring skill's Layer-3
//! Python scripts (code-mode *without* MCP).
//!
//! Each of `touring scout/read/health/guard/map/blast` exposes one of the most
//! common multi-call workflows as a single memorable subcommand. The command
//! forwards verbatim to the already invariant-hardened script (R2: `--brief`
//! density default, fail-soft Chain-7 fallback, `touring-quality` 50-dim
//! correctness) and propagates its stdout/stderr/exit code unchanged. See
//! `docs/2026-06-27-coupling-codemode-cli-and-master-commands.md` §4 (R3).
//!
//! Path resolution (in order):
//!   1. `$TOURING_SKILL_SCRIPTS` — explicit override (must be a directory)
//!   2. `$HOME/.claude/skills/Touring/scripts` — default install location
//!
//! The interpreter defaults to `python3`, overridable via `$TOURING_PYTHON`.
//!
//! Why forward instead of reimplement: the scripts are the canonical Layer-3
//! leverage (the skill's "code analyses, the model synthesises" idiom). A native
//! Rust reimplementation would duplicate the R2-corrected logic and drift from
//! it — so the master command is a *channel*, not a fork.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Default location of the Touring skill's Layer-3 scripts, relative to `$HOME`.
const DEFAULT_SCRIPTS_REL: &str = ".claude/skills/Touring/scripts";

/// Map a master command name to its backing Layer-3 script file name.
///
/// This is the single source of truth for the command → script mapping; both the
/// per-command handlers and the R6 quality gate consult it, so a typo surfaces in
/// one place and is covered by [`tests::script_for_maps_every_master_command`].
pub fn script_for(command: &str) -> Option<&'static str> {
    match command {
        "scout" => Some("discover_symbol.py"),
        "read" => Some("read_file.py"),
        "health" => Some("diagnose_health.py"),
        "guard" => Some("pre_edit_gate.py"),
        "map" => Some("discover_workspace.py"),
        "blast" => Some("analyze_blast.py"),
        "investigate" => Some("investigate.py"),
        "explore" => Some("explore_until_dry.py"),
        "adw" => Some("adw.py"),
        "factory" => Some("factory.py"),
        _ => None,
    }
}

/// Every master command name, in registry order. Used by the R6 gate to score the
/// whole surface and by tests to assert the mapping stays complete.
pub const MASTER_COMMANDS: &[&str] = &[
    "scout",
    "read",
    "health",
    "guard",
    "map",
    "blast",
    "investigate",
    "explore",
    "adw",
    "factory",
];

/// Pure resolver for the skill-scripts directory, taking the two environment
/// inputs explicitly so it is unit-testable without mutating process-global env.
fn resolve_dir(override_env: Option<&str>, home: Option<&str>) -> Result<PathBuf> {
    if let Some(dir) = override_env {
        let p = PathBuf::from(dir);
        if p.is_dir() {
            return Ok(p);
        }
        bail!("TOURING_SKILL_SCRIPTS={dir:?} is not a directory");
    }
    let home = home.context("$HOME is not set; cannot locate the touring skill scripts")?;
    let p = Path::new(home).join(DEFAULT_SCRIPTS_REL);
    if p.is_dir() {
        return Ok(p);
    }
    bail!(
        "touring skill scripts not found at {} — set $TOURING_SKILL_SCRIPTS to the skill scripts dir",
        p.display()
    );
}

/// Resolve the directory holding the skill's Layer-3 scripts from the live env.
fn skill_scripts_dir() -> Result<PathBuf> {
    resolve_dir(
        std::env::var("TOURING_SKILL_SCRIPTS").ok().as_deref(),
        std::env::var("HOME").ok().as_deref(),
    )
}

/// Resolve a single script file inside the skill-scripts dir, erroring with an
/// actionable message when it is missing.
fn resolve_script(file_name: &str) -> Result<PathBuf> {
    let p = skill_scripts_dir()?.join(file_name);
    if !p.is_file() {
        bail!("master-command script not found: {}", p.display());
    }
    Ok(p)
}

/// Forward a master command to its backing Layer-3 script.
///
/// `args` is the full process argv (`args[0]` = binary, `args[1]` = command name);
/// everything from `args[2]` on is passed verbatim, so the script's own `--json` /
/// `--brief` / `--timeout` flags keep working (they reach the script because
/// `main.rs` dispatches the *unfiltered* argv). stdin/stdout/stderr are inherited
/// and the script's exit code is propagated as the CLI exit code, so callers (and
/// code-mode orchestration) see a faithful success/failure signal.
fn forward(args: &[String], script_file: &str) -> Result<()> {
    let script = resolve_script(script_file)?;
    let python = std::env::var("TOURING_PYTHON").unwrap_or_else(|_| "python3".to_string());
    let forwarded = args.get(2..).unwrap_or(&[]);
    let status = Command::new(&python)
        .arg(&script)
        .args(forwarded)
        .status()
        .with_context(|| format!("spawning `{python} {}`", script.display()))?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

/// Look up the command's script via [`script_for`] and forward to it.
fn forward_command(args: &[String], command: &str) -> Result<()> {
    let script_file = script_for(command).ok_or_else(|| {
        anyhow::anyhow!("no Layer-3 script mapped for master command {command:?}")
    })?;
    forward(args, script_file)
}

/// `touring scout <symbol>` — symbol forensics: index find + ast find + wiring
/// impact + memory + gotcha + polyglot homonimia. Backed by `discover_symbol.py`.
pub fn scout(args: &[String]) -> Result<()> {
    forward_command(args, "scout")
}

/// `touring read <file>` — read-comprehend one-shot: ast meta + blast + tdg +
/// touring-quality + rust-semantic. Backed by `read_file.py`.
pub fn read(args: &[String]) -> Result<()> {
    forward_command(args, "read")
}

/// `touring health` — traffic-light health gate: doctor + status + gate-metrics +
/// learning + drift (exit 0/1/2). Backed by `diagnose_health.py`.
pub fn health(args: &[String]) -> Result<()> {
    forward_command(args, "health")
}

/// `touring guard <file>` — pre-edit GO/CAUTION/NO_GO gate: blast + tdg + gotcha +
/// memory + pre-edit score. Backed by `pre_edit_gate.py`.
pub fn guard(args: &[String]) -> Result<()> {
    forward_command(args, "guard")
}

/// `touring map [dir]` — workspace structure map: workspace-info + per-crate sweep
/// + wiring chains. Backed by `discover_workspace.py`.
pub fn map(args: &[String]) -> Result<()> {
    forward_command(args, "map")
}

/// `touring blast <files...>` — multi-file blast risk: blast + wiring impact +
/// cross-feature + cycles (LOW/MEDIUM/HIGH/CRITICAL). Backed by `analyze_blast.py`.
pub fn blast(args: &[String]) -> Result<()> {
    forward_command(args, "blast")
}

/// `touring investigate <topic>` — R5: search + index + wiring chains + memory →
/// a topic map. Backed by `investigate.py`.
pub fn investigate(args: &[String]) -> Result<()> {
    forward_command(args, "investigate")
}

/// `touring explore <topic>` — F1/ADW: loop-until-dry multi-lens exploration with
/// a persistent ledger, coverage matrix (CCE v2), open-question gating and an
/// epistemically honest convergence verdict (exit 0 converged / 1 continue /
/// 3 degraded). Backed by `explore_until_dry.py`.
pub fn explore(args: &[String]) -> Result<()> {
    forward_command(args, "explore")
}

/// `touring adw <list|lint|run|test|from-template>` — F0/ADW: durable declarative
/// agent workflows. Typed nodes (code/agent/gate/loop/human), fsync'd journal with
/// `--resume-run` replay, {summary, omitted_bytes, full_ref} results store, Class-D
/// narrative-vs-verdict detection and budget-verify lint. Backed by `adw.py`.
pub fn adw(args: &[String]) -> Result<()> {
    forward_command(args, "adw")
}

/// `touring factory <route|start|stats>` — F4/ADW: the factory router. Routes a
/// ticket to a library ADW deterministically (keyword families + CILA), reserving
/// the LLM router for ambiguous tickets; `start` launches `touring adw run` and
/// feeds the outcome back into the router's RL arm. Backed by `factory.py`.
pub fn factory(args: &[String]) -> Result<()> {
    forward_command(args, "factory")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn script_for_maps_every_master_command() {
        for cmd in MASTER_COMMANDS {
            assert!(
                script_for(cmd).is_some(),
                "master command {cmd:?} has no backing script"
            );
        }
    }
    #[test]
    fn script_for_unknown_is_none() {
        assert!(script_for("definitely-not-a-command").is_none());
        assert!(script_for("").is_none());
    }
    #[test]
    fn mapped_scripts_are_python_files() {
        for cmd in MASTER_COMMANDS {
            let s = script_for(cmd).expect("mapped");
            assert!(s.ends_with(".py"), "{cmd}: {s} is not a .py file");
        }
    }
    #[test]
    fn resolve_dir_honors_existing_override() {
        let tmp = std::env::temp_dir();
        let got = resolve_dir(Some(tmp.to_str().expect("utf8")), None).expect("temp dir resolves");
        assert_eq!(got, tmp);
    }
    #[test]
    fn resolve_dir_rejects_nonexistent_override() {
        let err = resolve_dir(Some("/nonexistent/touring/scripts/xyz"), None)
            .expect_err("missing override dir must error");
        assert!(err.to_string().contains("not a directory"));
    }
    #[test]
    fn resolve_dir_falls_back_to_home() {
        let tmp = std::env::temp_dir();
        let err = resolve_dir(None, Some(tmp.to_str().expect("utf8")))
            .expect_err("default rel path under a bare temp HOME does not exist");
        assert!(err.to_string().contains("touring skill scripts not found"));
    }
    #[test]
    fn resolve_dir_errors_when_home_absent() {
        let err = resolve_dir(None, None).expect_err("no override and no HOME must error");
        assert!(err.to_string().contains("$HOME is not set"));
    }
    #[test]
    fn all_mapped_scripts_exist_when_skill_installed() {
        let Ok(dir) = skill_scripts_dir() else {
            return;
        };
        for cmd in MASTER_COMMANDS {
            let name = script_for(cmd).expect("mapped");
            if name == "investigate.py" && !dir.join(name).is_file() {
                continue;
            }
            assert!(
                dir.join(name).is_file(),
                "{cmd}: backing script {name} missing in {}",
                dir.display()
            );
        }
    }
}
