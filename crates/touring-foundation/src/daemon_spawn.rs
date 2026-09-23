//! Where a spawned daemon lives — one launcher for the Rust spawn sites.
//!
//! Callers: the hook autostart (`touring-hooks`, bounded to 250 ms of launcher
//! wait) and `touring daemon-ctl` (`touring-server`). `scripts/update-touring`
//! mirrors the same route in bash. The convergence judge's private gate daemon
//! is deliberately NOT launched here: it lives only for the judge's cargo
//! clause, which stops it (`loop_converged.py`, cross-audit 14/09/2026, C9).
//!
//! A process inherits its parent's cgroup, and systemd stops a unit by killing
//! every process in the unit's cgroup (`KillMode=control-group`, the default).
//! `setsid(2)` detaches the session and process group but not the cgroup, so a
//! daemon auto-started by a hook that ran inside a oneshot service died when the
//! service ended: `routine-inbox-digest.service` (14/09/2026, SIGTERM at unit end)
//! and the three daemons of release 30.4.45, born in `touring-release45.service`
//! and alive only because that unit had `KillMode=process`.
//!
//! Decision 2-A (14/09/2026): when the systemd user manager is reachable, the
//! daemon is launched through `systemd-run --user --scope`, which moves the
//! launcher into a transient scope of its own and then `exec`s the daemon —
//! same pid, same environment, same working directory, a cgroup no caller
//! owns. Proven before this code existed: from inside a oneshot unit, a
//! `systemd-run --scope` child survived the unit's end while a `setsid` child
//! died. Where the manager is unreachable (no systemd, no user bus, a
//! container) or the scope launch fails before `exec`, the daemon is spawned
//! directly, exactly as before. `TOURING_DAEMON_SCOPE=0|false|off` forces the direct
//! route.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

/// How a daemon was launched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnRoute {
    /// Through `systemd-run --user --scope`: the daemon owns a transient scope.
    UserScope,
    /// Directly, in the caller's cgroup (no user manager, opt-out, or the scope
    /// launch failed before `exec`).
    Direct,
}

/// A launched daemon: the route taken and the pid of the process that is (or
/// becomes, after `exec`) the daemon.
#[derive(Debug)]
pub struct SpawnedDaemon {
    /// The route taken.
    pub route: SpawnRoute,
    /// The child process — the daemon itself on both routes.
    pub child: Child,
}

/// The environment switch that forces [`SpawnRoute::Direct`].
const SCOPE_OPT_OUT_ENV: &str = "TOURING_DAEMON_SCOPE";

/// Variables that relax a gate for ONE command and must never outlive it.
///
/// `TOURING_CODE_MODE=native <cmd>` is the documented way to relax the code-mode
/// gates for a single call, and the presentation is resolved from the environment
/// of whichever process decides — which is the daemon. So a daemon that inherits
/// the variable turns a per-command relaxation into a machine-wide kill switch:
/// measured on 2026-09-22, a restart from a shell carrying it left every
/// code-mode gate off, with `doctor` green and the behavioural proof failing 2 of
/// 40 assertions without naming a cause.
///
/// Deliberate intent has its own door: [`DAEMON_CODE_MODE_ENV`].
const PER_COMMAND_RELAXATIONS: &[&str] = &["TOURING_CODE_MODE", "TOURING_CODE_ONLY"];

/// Ask for a daemon that runs in a specific code-mode presentation on purpose.
///
/// The spawn translates it into `TOURING_CODE_MODE` for the child, after the
/// scrub — so the only way to set the daemon's presentation is to say so.
pub const DAEMON_CODE_MODE_ENV: &str = "TOURING_DAEMON_CODE_MODE";

/// Remove the per-command relaxations from a daemon command, honouring a
/// deliberate request (the caller reads it from [`DAEMON_CODE_MODE_ENV`]).
///
/// Called on every spawn route, after `configure`, so no spawn site can forget
/// it. The request arrives as an argument — like `scope_launcher`'s opt-out — so
/// the decision is pure and provable without mutating the process environment.
fn scrub_per_command_relaxations(cmd: &mut Command, deliberate: Option<&str>) {
    for var in PER_COMMAND_RELAXATIONS {
        cmd.env_remove(var);
    }
    if let Some(mode) = deliberate.map(str::trim).filter(|m| !m.is_empty()) {
        cmd.env("TOURING_CODE_MODE", mode);
    }
}

/// The deliberate daemon presentation asked for in this process's environment.
fn deliberate_daemon_mode() -> Option<String> {
    std::env::var(DAEMON_CODE_MODE_ENV).ok()
}

/// How long a scope launch may take to reach `exec` before it is taken as
/// launched. `systemd-run` answers in tens of milliseconds; a manager that
/// takes longer is still left to finish rather than raced by a second daemon.
const SCOPE_EXEC_WAIT: Duration = Duration::from_secs(2);

/// Launch `daemon_bin` detached from the caller: its own session (`setsid`)
/// and, when the user manager is reachable, its own cgroup.
///
/// `configure` receives the command before each spawn attempt and sets what
/// every spawn site sets — environment, working directory and stdio. It may run
/// twice: once for the scope route and once more for the direct fallback, so
/// it must build fresh stdio handles each time.
///
/// # Errors
///
/// The error of the direct spawn, when that route is reached and fails.
pub fn spawn_daemon_detached(
    daemon_bin: &Path,
    description: &str,
    configure: &dyn Fn(&mut Command),
) -> io::Result<SpawnedDaemon> {
    spawn_daemon_detached_within(daemon_bin, description, SCOPE_EXEC_WAIT, configure)
}

/// [`spawn_daemon_detached`] with the caller's own bound on the scope launcher's
/// `exec`. A hook answers the harness and cannot spend the operator's 2 s on a
/// slow user manager (cross-audit 14/09/2026, C8): past the bound the launch is
/// taken as done, never raced by a second daemon, and a launcher that fails
/// later leaves the next hook to try again.
///
/// # Errors
///
/// The error of the direct spawn, when that route is reached and fails.
pub fn spawn_daemon_detached_within(
    daemon_bin: &Path,
    description: &str,
    exec_wait: Duration,
    configure: &dyn Fn(&mut Command),
) -> io::Result<SpawnedDaemon> {
    let launcher = scope_launcher(
        std::env::var(SCOPE_OPT_OUT_ENV).ok().as_deref(),
        find_on_path("systemd-run"),
        user_manager_socket().as_deref(),
    );
    spawn_with_launcher(
        launcher.as_deref(),
        daemon_bin,
        description,
        exec_wait,
        configure,
    )
}

/// The launcher to use, or `None` for the direct route. Pure, so the decision
/// is testable without a user manager.
#[must_use]
fn scope_launcher(
    opt_out: Option<&str>,
    systemd_run: Option<PathBuf>,
    user_manager_socket: Option<&Path>,
) -> Option<PathBuf> {
    if matches!(opt_out, Some("0" | "false" | "off")) {
        return None;
    }
    user_manager_socket?;
    systemd_run
}

/// The argv of a scope launch of `daemon_bin`.
#[must_use]
fn scope_args(daemon_bin: &Path, description: &str) -> Vec<OsString> {
    vec![
        "--user".into(),
        "--scope".into(),
        "--quiet".into(),
        // A scope whose daemon exited with an error is garbage-collected, not
        // left `failed` in `systemctl --user` forever.
        "--collect".into(),
        // systemd-run expands `$VAR`/`${VAR}` in the argv by default, which would
        // rewrite a daemon path or description that contains one (C6).
        "--expand-environment=no".into(),
        format!("--description={description}").into(),
        "--".into(),
        daemon_bin.as_os_str().to_owned(),
    ]
}

/// [`spawn_daemon_detached`] with the launcher chosen by the caller — the seam
/// the tests use to force the fallback.
fn spawn_with_launcher(
    launcher: Option<&Path>,
    daemon_bin: &Path,
    description: &str,
    exec_wait: Duration,
    configure: &dyn Fn(&mut Command),
) -> io::Result<SpawnedDaemon> {
    if let Some(launcher) = launcher {
        let mut cmd = Command::new(launcher);
        cmd.args(scope_args(daemon_bin, description));
        configure(&mut cmd);
        scrub_per_command_relaxations(&mut cmd, deliberate_daemon_mode().as_deref());
        detach_session(&mut cmd);
        if let Ok(child) = cmd.spawn()
            && let Some(child) = await_exec(child, launcher, exec_wait)
        {
            return Ok(SpawnedDaemon {
                route: SpawnRoute::UserScope,
                child,
            });
        }
    }
    let mut cmd = Command::new(daemon_bin);
    configure(&mut cmd);
    scrub_per_command_relaxations(&mut cmd, deliberate_daemon_mode().as_deref());
    detach_session(&mut cmd);
    Ok(SpawnedDaemon {
        route: SpawnRoute::Direct,
        child: cmd.spawn()?,
    })
}

/// Wait until the launcher has `exec`ed the daemon. `Some(child)` once the
/// process no longer runs the launcher (or the wait ran out); `None` when the
/// launcher exited unsuccessfully without ever reaching `exec` — no scope, no
/// daemon, so the caller spawns directly.
fn await_exec(mut child: Child, launcher: &Path, exec_wait: Duration) -> Option<Child> {
    let launcher_comm = comm_of(launcher);
    let deadline = Instant::now() + exec_wait;
    loop {
        match child.try_wait() {
            // Exited before it was ever seen running the daemon: a successful
            // exit is the daemon's own idempotent one (another daemon holds the
            // socket, and a second launch would exit the same way); an
            // unsuccessful one is the launcher failing — no user bus, a refused
            // scope — and asks for the direct route.
            Ok(Some(status)) => return status.success().then_some(child),
            Ok(None) if !still_launcher(&child, &launcher_comm) => return Some(child),
            Ok(None) if Instant::now() >= deadline => return Some(child),
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(_) => return Some(child),
        }
    }
}

/// Whether the live process still runs the launcher.
fn still_launcher(child: &Child, launcher_comm: &str) -> bool {
    match std::fs::read_to_string(format!("/proc/{}/comm", child.id())) {
        Ok(comm) => comm.trim_end() == launcher_comm,
        // No `/proc` (not Linux): nothing distinguishes the launcher from the
        // daemon, so the process is taken as launched.
        Err(_) => false,
    }
}

/// The kernel's `comm` for an executable: its file name, truncated to 15 bytes.
fn comm_of(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.chars()
        .scan(0usize, |bytes, c| {
            *bytes += c.len_utf8();
            (*bytes <= 15).then_some(c)
        })
        .collect()
}

/// A fresh session and process group, no controlling terminal: the daemon
/// never dies with the terminal or session that spawned it (fix of 01/07/2026,
/// now in one place instead of two mirrored copies).
fn detach_session(cmd: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: `pre_exec` runs in the forked child before `exec`. `setsid(2)`
        // is async-signal-safe, touches no memory, and cannot fail with EPERM
        // there: a freshly forked child is never a process-group leader.
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
}

/// The user manager's private socket, when this process can reach one.
fn user_manager_socket() -> Option<PathBuf> {
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")?;
    let socket = PathBuf::from(runtime_dir).join("systemd").join("private");
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        std::fs::metadata(&socket)
            .ok()
            .filter(|m| m.file_type().is_socket())
            .map(|_| socket)
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// `name` resolved against `PATH`, as `execvp` would.
fn find_on_path(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
#[path = "daemon_spawn_tests.rs"]
mod tests;
