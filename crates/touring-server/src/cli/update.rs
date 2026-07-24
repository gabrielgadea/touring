//! `touring update` — W12.3 / Pln2 F3 (2026-07-24) — per-project update
//! propagation: THE missing nucleus of the productization arc.
//!
//! "A cada atualização, rodar a atualização em cada projeto individualmente":
//! this command reads the project's requested pin (`touring.toml [toolchain]
//! channel`) and resolved state (`.touring/toolchain.lock`), selects the target
//! toolchain under `~/.touring/toolchains/`, re-links `.touring/bin/`, records
//! the transition in the lockfile (deterministic `--rollback`), and restarts
//! the per-project daemon on the NEW binary when one is running.
//!
//! ```text
//! touring update                       # re-resolve the active channel, re-link
//! touring update vB                    # switch this project to toolchain vB
//! touring update --channel vB          # same, explicit flag form
//! touring update --rollback            # return to the lockfile's `previous`
//! touring update --project <root>      # target a project other than cwd
//! touring update --all-projects        # iterate the ProjectRegistry
//! touring update --dry-run             # print the plan, touch nothing
//! touring update --no-restart          # skip the per-project daemon restart
//! ```
//!
//! State discipline (rustup requested-vs-resolved): `touring.toml` is the
//! human's file and is never rewritten here; `.touring/toolchain.lock` is the
//! machine's file and is the only thing this command mutates besides the
//! symlinks. Lock is written BEFORE re-linking: a crash mid-update leaves a
//! lock that says the intent and a re-run converges (relink is idempotent).

use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};

use super::project_toolchain::{
    DEV_CHANNEL, PROJECT_BINARIES, ToolchainLock, relink_bins_inner, resolve_active_channel,
};

/// Parsed args for `touring update`.
#[derive(Debug, Default, Clone)]
pub struct UpdateArgs {
    /// Target channel (`--channel X` or bare positional). `None` = re-resolve.
    pub channel: Option<String>,
    /// `--rollback`: return to the lockfile's `previous` channel.
    pub rollback: bool,
    /// `--project <root>`: explicit project root (default: cwd walk-up).
    pub project: Option<PathBuf>,
    /// `--all-projects`: iterate every ProjectRegistry entry with a `.touring/`.
    pub all_projects: bool,
    /// `--dry-run`: print the plan without touching anything.
    pub dry_run: bool,
    /// `--no-restart`: skip the per-project daemon restart step.
    pub no_restart: bool,
    /// `--json` / `-j`: machine-readable report.
    pub json: bool,
    /// `--help` / `-h`.
    pub help: bool,
}

impl UpdateArgs {
    /// Parse `touring update ...` argv (manual style — crate CLI convention).
    pub fn parse(args: &[String]) -> Self {
        let mut out = Self::default();
        let mut i = 2; // skip binary + "update"
        while i < args.len() {
            let a = args[i].as_str();
            match a {
                "--rollback" => out.rollback = true,
                "--all-projects" => out.all_projects = true,
                "--dry-run" => out.dry_run = true,
                "--no-restart" => out.no_restart = true,
                "--json" | "-j" => out.json = true,
                "--help" | "-h" => out.help = true,
                "--channel" => {
                    if let Some(v) = args.get(i + 1) {
                        out.channel = Some(v.clone());
                        i += 1;
                    }
                }
                other if other.starts_with("--channel=") => {
                    out.channel = Some(other.trim_start_matches("--channel=").to_string());
                }
                "--project" => {
                    if let Some(v) = args.get(i + 1) {
                        out.project = Some(PathBuf::from(v));
                        i += 1;
                    }
                }
                other if other.starts_with("--project=") => {
                    out.project = Some(PathBuf::from(other.trim_start_matches("--project=")));
                }
                other if !other.starts_with('-') && out.channel.is_none() => {
                    out.channel = Some(other.to_string());
                }
                _ => { /* unknown flag — fail-open, crate convention */ }
            }
            i += 1;
        }
        out
    }
}

/// Per-project outcome of one update.
#[derive(Debug)]
pub struct UpdateReport {
    /// Project root that was updated.
    pub root: PathBuf,
    /// Channel active before this run (`None` = was unpinned).
    pub from: Option<String>,
    /// Channel active after this run.
    pub to: String,
    /// Re-link notes (one per core binary).
    pub relinked: Vec<String>,
    /// What happened to the per-project daemon.
    pub daemon: String,
}

const USAGE: &str = "touring update — Propagate a toolchain update to a project (W12.3 / Pln2 F3)

USAGE:
    touring update [<channel>] [--channel <ch>] [--rollback]
                   [--project <root>] [--all-projects]
                   [--dry-run] [--no-restart] [--json]

BEHAVIOR:
    Resolves the target toolchain (explicit <channel> > lockfile active >
    touring.toml [toolchain] channel), re-links <project>/.touring/bin/ to
    ~/.touring/toolchains/<channel>/bin (channel `dev` = ~/.local/bin), records
    the transition in .touring/toolchain.lock (previous kept for --rollback),
    and restarts the per-project daemon on the new binary when one is running.
    touring.toml is never rewritten — the lockfile is the machine's state.";

/// CLI dispatch entry. Called from `cli::command_table`.
pub fn run(args: &[String]) -> Result<()> {
    let parsed = UpdateArgs::parse(args);
    if parsed.help {
        println!("{USAGE}");
        return Ok(());
    }

    let home = std::env::var("HOME").unwrap_or_default();
    let touring_home = std::env::var("TOURING_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(&home).join(".touring"));
    let dev_bin_dir = Path::new(&home).join(".local").join("bin");

    let roots = resolve_project_roots(&parsed)?;
    let mut reports = Vec::new();
    let mut failures = Vec::new();
    for root in roots {
        match update_project(&root, &parsed, &touring_home, &dev_bin_dir) {
            Ok(report) => reports.push(report),
            Err(e) => failures.push(format!("{}: {e}", root.display())),
        }
    }

    if parsed.json {
        let out = serde_json::json!({
            "updated": reports.iter().map(|r| serde_json::json!({
                "root": r.root.display().to_string(),
                "from": r.from,
                "to": r.to,
                "relinked": r.relinked,
                "daemon": r.daemon,
            })).collect::<Vec<_>>(),
            "failures": failures,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        for r in &reports {
            println!(
                "touring update: {} — {} -> {} (daemon: {})",
                r.root.display(),
                r.from.as_deref().unwrap_or("<unpinned>"),
                r.to,
                r.daemon
            );
            for note in &r.relinked {
                println!("  {note}");
            }
        }
        for f in &failures {
            eprintln!("touring update: FAILED {f}");
        }
    }

    if !failures.is_empty() {
        return Err(anyhow!("{} project(s) failed to update", failures.len()));
    }
    Ok(())
}

/// Resolve which project roots this invocation targets.
///
/// `--all-projects` iterates the ProjectRegistry (only entries that actually
/// have a `.touring/`); `--project` targets one explicit root; the default is
/// the cwd's project via `.touring` walk-up — erroring loud when there is no
/// project (a silent global fallback here would "update" nothing).
fn resolve_project_roots(args: &UpdateArgs) -> Result<Vec<PathBuf>> {
    if args.all_projects {
        let mut registry = crate::projects::ProjectRegistry::with_default_path();
        registry.load()?;
        let roots: Vec<PathBuf> = registry
            .entries()
            .filter(|e| e.path.join(".touring").is_dir())
            .map(|e| e.path.clone())
            .collect();
        if roots.is_empty() {
            return Err(anyhow!(
                "--all-projects: no registered project has a .touring/ (run `touring init-project` in each)"
            ));
        }
        return Ok(roots);
    }
    if let Some(root) = &args.project {
        return Ok(vec![root.clone()]);
    }
    let cwd = std::env::current_dir()?;
    walk_up_project_root(&cwd).map(|r| vec![r]).ok_or_else(|| {
        anyhow!(
            "no .touring/ found walking up from {} — pass --project <root> or run `touring init-project`",
            cwd.display()
        )
    })
}

/// Walk up from `start` to the first dir containing `.touring/`.
fn walk_up_project_root(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        if dir.join(".touring").is_dir() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Update ONE project: resolve target → verify installed → lock → re-link →
/// daemon restart. The core of F3.
fn update_project(
    root: &Path,
    args: &UpdateArgs,
    touring_home: &Path,
    dev_bin_dir: &Path,
) -> Result<UpdateReport> {
    let dot = root.join(".touring");
    if !dot.is_dir() {
        return Err(anyhow!(
            "{} has no .touring/ — run `touring init-project` first",
            root.display()
        ));
    }

    let current = resolve_active_channel(&dot);
    let prior_lock = ToolchainLock::read(&dot);

    // 1. Select the target channel.
    let target = if args.rollback {
        prior_lock
            .as_ref()
            .and_then(|l| l.previous.clone())
            .ok_or_else(|| {
                anyhow!("nothing to roll back to — no `previous` recorded in .touring/toolchain.lock")
            })?
    } else {
        args.channel
            .clone()
            .or_else(|| current.clone())
            .unwrap_or_else(|| DEV_CHANNEL.to_string())
    };

    // 2. Verify the target toolchain is actually installed (dev = ~/.local/bin).
    if target != DEV_CHANNEL {
        let tc = touring_home.join("toolchains").join(&target);
        if !tc.is_dir() {
            return Err(anyhow!(
                "toolchain {target} is not installed (expected {}). Run `touring toolchain install` first.",
                tc.display()
            ));
        }
    }

    // 3. Dry-run: report the plan, touch nothing.
    if args.dry_run {
        return Ok(UpdateReport {
            root: root.to_path_buf(),
            from: current.clone(),
            to: target.clone(),
            relinked: vec![format!(
                "[dry-run] would re-link {:?} to channel {target} and write toolchain.lock",
                PROJECT_BINARIES
            )],
            daemon: "[dry-run] untouched".into(),
        });
    }

    // 4. Record the transition BEFORE re-linking (crash-safe intent; relink is
    //    idempotent so a re-run converges). Same-channel re-resolve keeps the
    //    old `previous` — never `previous == active`.
    let previous = match (&current, &target) {
        (Some(cur), tgt) if cur != tgt => Some(cur.clone()),
        _ => prior_lock.as_ref().and_then(|l| l.previous.clone()),
    };
    let reason = if args.rollback {
        "update --rollback".to_string()
    } else {
        match &args.channel {
            Some(c) => format!("update --channel {c}"),
            None => "update (re-resolve)".to_string(),
        }
    };
    ToolchainLock {
        active: target.clone(),
        previous,
        updated_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        reason,
    }
    .write(&dot)?;

    // 5. Re-link `.touring/bin/` against the (now-locked) active channel.
    let relinked = relink_bins_inner(&dot, touring_home, dev_bin_dir);

    // 6. Restart the per-project daemon on the NEW binary — only when this
    //    project actually runs one (socket present). REGRA #19: the canonical
    //    daemon-ctl restart flow, never a raw kill.
    let sock = dot.join("daemon.sock");
    let daemon = if args.no_restart {
        "skipped (--no-restart)".to_string()
    } else if !sock.exists() {
        "not running (no daemon.sock)".to_string()
    } else {
        let project_daemon = dot.join("bin").join("touring-daemon");
        let bin_override = project_daemon.exists().then_some(project_daemon.as_path());
        super::daemon_ctl::restart_socket_with_bin(false, &sock, bin_override)?;
        format!("restarted on {}", sock.display())
    };

    Ok(UpdateReport {
        root: root.to_path_buf(),
        from: current,
        to: target,
        relinked,
        daemon,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args_for(extras: &[&str]) -> Vec<String> {
        let mut v = vec!["touring".to_string(), "update".to_string()];
        v.extend(extras.iter().map(|s| s.to_string()));
        v
    }

    fn fake_bin(dir: &Path, name: &str) {
        std::fs::create_dir_all(dir).expect("mkdir");
        let p = dir.join(name);
        std::fs::write(&p, "#!/bin/sh\nexit 0\n").expect("write");
        let mut perm = std::fs::metadata(&p).expect("meta").permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perm, 0o755);
        std::fs::set_permissions(&p, perm).expect("chmod");
    }

    fn make_toolchain(th: &Path, version: &str) {
        for b in PROJECT_BINARIES {
            fake_bin(&th.join("toolchains").join(version).join("bin"), b);
        }
    }

    fn make_project(root: &Path, channel: &str) -> PathBuf {
        let dot = root.join(".touring");
        std::fs::create_dir_all(dot.join("bin")).expect("mkdir");
        std::fs::write(
            dot.join("touring.toml"),
            format!("[toolchain]\nchannel = \"{channel}\"\n"),
        )
        .expect("write toml");
        dot
    }

    fn no_restart_args() -> UpdateArgs {
        UpdateArgs {
            no_restart: true,
            ..Default::default()
        }
    }

    #[test]
    fn parse_positional_channel_and_flags() {
        let p = UpdateArgs::parse(&args_for(&["vB", "--dry-run", "--no-restart", "-j"]));
        assert_eq!(p.channel.as_deref(), Some("vB"));
        assert!(p.dry_run && p.no_restart && p.json);
        let p = UpdateArgs::parse(&args_for(&["--channel", "vC", "--rollback"]));
        assert_eq!(p.channel.as_deref(), Some("vC"));
        assert!(p.rollback);
        let p = UpdateArgs::parse(&args_for(&["--project=/tmp/x", "--all-projects"]));
        assert_eq!(p.project, Some(PathBuf::from("/tmp/x")));
        assert!(p.all_projects);
    }

    #[test]
    fn update_switches_channel_and_records_lock() {
        let tmp = tempfile::tempdir().expect("tmp");
        let th = tmp.path().join("th");
        make_toolchain(&th, "vA");
        make_toolchain(&th, "vB");
        let proj = tmp.path().join("proj");
        let dot = make_project(&proj, "vA");
        let dev = tmp.path().join("no-dev");

        // First: plain resolve links vA (the pin).
        let r = update_project(&proj, &no_restart_args(), &th, &dev).expect("update pin");
        assert_eq!(r.to, "vA");
        // Switch: vB becomes active, vA becomes previous.
        let mut args = no_restart_args();
        args.channel = Some("vB".into());
        let r = update_project(&proj, &args, &th, &dev).expect("update vB");
        assert_eq!(r.from.as_deref(), Some("vA"));
        assert_eq!(r.to, "vB");
        let lock = ToolchainLock::read(&dot).expect("lock");
        assert_eq!(lock.active, "vB");
        assert_eq!(lock.previous.as_deref(), Some("vA"));
        let link = std::fs::read_link(dot.join("bin/touring")).expect("link");
        assert!(link.display().to_string().contains("/vB/"), "{link:?}");
    }

    #[test]
    fn rollback_restores_previous_deterministically() {
        let tmp = tempfile::tempdir().expect("tmp");
        let th = tmp.path().join("th");
        make_toolchain(&th, "vA");
        make_toolchain(&th, "vB");
        let proj = tmp.path().join("proj");
        let dot = make_project(&proj, "vA");
        let dev = tmp.path().join("no-dev");

        let mut args = no_restart_args();
        args.channel = Some("vB".into());
        update_project(&proj, &args, &th, &dev).expect("to vB");

        let mut rb = no_restart_args();
        rb.rollback = true;
        let r = update_project(&proj, &rb, &th, &dev).expect("rollback");
        assert_eq!(r.to, "vA");
        let lock = ToolchainLock::read(&dot).expect("lock");
        assert_eq!(lock.active, "vA");
        assert_eq!(lock.previous.as_deref(), Some("vB"), "swap for re-rollback");
        let link = std::fs::read_link(dot.join("bin/touring")).expect("link");
        assert!(link.display().to_string().contains("/vA/"), "{link:?}");
    }

    #[test]
    fn rollback_without_lock_fails_loud() {
        let tmp = tempfile::tempdir().expect("tmp");
        let proj = tmp.path().join("proj");
        make_project(&proj, "vA");
        let mut rb = no_restart_args();
        rb.rollback = true;
        let err = update_project(&proj, &rb, &tmp.path().join("th"), &tmp.path().join("dev"))
            .expect_err("must fail");
        assert!(format!("{err}").contains("nothing to roll back"), "{err}");
    }

    #[test]
    fn update_refuses_uninstalled_toolchain() {
        let tmp = tempfile::tempdir().expect("tmp");
        let th = tmp.path().join("th");
        make_toolchain(&th, "vA");
        let proj = tmp.path().join("proj");
        make_project(&proj, "vA");
        let mut args = no_restart_args();
        args.channel = Some("v-missing".into());
        let err = update_project(&proj, &args, &th, &tmp.path().join("dev"))
            .expect_err("must fail");
        assert!(format!("{err}").contains("not installed"), "{err}");
    }

    #[test]
    fn same_channel_reresolve_keeps_old_previous() {
        // `update` (no args) after a switch must NOT clobber previous with
        // active — rollback stays meaningful across re-resolves.
        let tmp = tempfile::tempdir().expect("tmp");
        let th = tmp.path().join("th");
        make_toolchain(&th, "vA");
        make_toolchain(&th, "vB");
        let proj = tmp.path().join("proj");
        let dot = make_project(&proj, "vA");
        let dev = tmp.path().join("no-dev");

        let mut args = no_restart_args();
        args.channel = Some("vB".into());
        update_project(&proj, &args, &th, &dev).expect("to vB");
        update_project(&proj, &no_restart_args(), &th, &dev).expect("re-resolve");
        let lock = ToolchainLock::read(&dot).expect("lock");
        assert_eq!(lock.active, "vB");
        assert_eq!(
            lock.previous.as_deref(),
            Some("vA"),
            "re-resolve must not set previous = active"
        );
    }

    #[test]
    fn dry_run_touches_nothing() {
        let tmp = tempfile::tempdir().expect("tmp");
        let th = tmp.path().join("th");
        make_toolchain(&th, "vB");
        let proj = tmp.path().join("proj");
        let dot = make_project(&proj, "vB");
        let mut args = no_restart_args();
        args.dry_run = true;
        let r = update_project(&proj, &args, &th, &tmp.path().join("dev")).expect("dry");
        assert_eq!(r.to, "vB");
        assert!(!dot.join("toolchain.lock").exists(), "dry-run must not write");
        assert_eq!(
            std::fs::read_dir(dot.join("bin")).expect("dir").count(),
            0,
            "dry-run must not link"
        );
    }

    #[test]
    fn missing_project_fails_loud() {
        let tmp = tempfile::tempdir().expect("tmp");
        let err = update_project(
            &tmp.path().join("nope"),
            &no_restart_args(),
            &tmp.path().join("th"),
            &tmp.path().join("dev"),
        )
        .expect_err("must fail");
        assert!(format!("{err}").contains("init-project"), "{err}");
    }

    #[test]
    fn walk_up_finds_dot_touring() {
        let tmp = tempfile::tempdir().expect("tmp");
        let root = tmp.path().join("proj");
        std::fs::create_dir_all(root.join(".touring")).expect("mkdir");
        let deep = root.join("a/b/c");
        std::fs::create_dir_all(&deep).expect("mkdir deep");
        assert_eq!(walk_up_project_root(&deep), Some(root));
        assert_eq!(walk_up_project_root(&tmp.path().join("elsewhere")), None);
    }
}
