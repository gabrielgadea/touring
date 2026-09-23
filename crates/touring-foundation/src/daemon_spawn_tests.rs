//! Decision 2-A (14/09/2026): the daemon launcher. The route decision is pure;
//! the fallback is proven with a launcher that fails before `exec`; the scope
//! route is proven live wherever a systemd user manager answers.

use super::*;
use std::path::Path;

fn sleep_bin() -> PathBuf {
    find_on_path("sleep").expect("`sleep` on PATH")
}

/// Uma relaxação POR-COMANDO não pode virar chave de máquina.
///
/// Medido em 22/09/2026: o daemon foi reiniciado de um shell que carregava
/// `TOURING_CODE_MODE=native` (o relaxamento documentado para UM comando). O
/// daemon herdou a var, e como a resolução da apresentação lê a env do processo
/// que decide, TODO gate de code mode ficou desligado na máquina — em silêncio,
/// com `doctor` verde e a prova comportamental reprovando 2 de 40 asserções sem
/// dizer por quê. O spawn é o único lugar onde isso se corta para todos os
/// sítios; a intenção deliberada tem porta própria.
#[test]
fn a_per_command_relaxation_never_reaches_the_daemon() {
    let mut cmd = Command::new(sleep_bin());
    cmd.env("TOURING_CODE_MODE", "native");
    cmd.env("TOURING_CODE_ONLY", "1");
    cmd.env("TOURING_DAEMON_SOCKET", "/tmp/x.sock");
    scrub_per_command_relaxations(&mut cmd, None);

    let envs: Vec<_> = cmd.get_envs().collect();
    for var in PER_COMMAND_RELAXATIONS {
        assert!(
            envs.iter()
                .any(|(k, v)| *k == std::ffi::OsStr::new(var) && v.is_none()),
            "{var} devia ser REMOVIDA do ambiente do daemon, e não apenas ignorada"
        );
    }
    assert!(
        envs.iter().any(|(k, v)| *k == std::ffi::OsStr::new("TOURING_DAEMON_SOCKET")
            && *v == Some(std::ffi::OsStr::new("/tmp/x.sock"))),
        "o resto do ambiente segue intacto"
    );
}

/// A porta da intenção deliberada: quem QUER um daemon em outro modo o diz por
/// uma var própria, que o spawn traduz. Sem ela não há como pedir, e alguém
/// voltaria a exportar a var por-comando — o defeito de novo.
#[test]
fn the_deliberate_daemon_mode_survives_the_scrub() {
    let mut cmd = Command::new(sleep_bin());
    cmd.env("TOURING_CODE_MODE", "native");
    scrub_per_command_relaxations(&mut cmd, Some(" both "));

    let envs: Vec<_> = cmd.get_envs().collect();
    assert!(
        envs.iter().any(|(k, v)| *k == std::ffi::OsStr::new("TOURING_CODE_MODE")
            && *v == Some(std::ffi::OsStr::new("both"))),
        "o pedido deliberado chega ao daemon como a própria var de apresentação, sem espaços"
    );

    // Um pedido vazio não é pedido: continua removida, nunca `TOURING_CODE_MODE=`.
    let mut vazio = Command::new(sleep_bin());
    vazio.env("TOURING_CODE_MODE", "native");
    scrub_per_command_relaxations(&mut vazio, Some("   "));
    assert!(
        vazio
            .get_envs()
            .any(|(k, v)| k == std::ffi::OsStr::new("TOURING_CODE_MODE") && v.is_none()),
        "pedido em branco deixa a var removida"
    );
}

/// Guard cruzado: as DUAS rotas de spawn chamam a limpeza. Um predicado
/// implementado e não ligado numa das rotas é invisível — a rota esquecida é
/// justamente a que roda quando o systemd não responde.
#[test]
fn both_spawn_routes_scrub_before_spawning() {
    let src = include_str!("daemon_spawn.rs");
    let body = src
        .split_once("fn spawn_with_launcher")
        .expect("spawn_with_launcher exists")
        .1;
    assert_eq!(
        body.matches("scrub_per_command_relaxations(&mut cmd").count(),
        2,
        "a rota do scope e a direta precisam AMBAS limpar o ambiente"
    );
    // A limpeza vem depois de `configure`: quem configura não pode reintroduzir a var.
    for route in body.split("detach_session(&mut cmd)").take(2) {
        let configure = route.rfind("configure(&mut cmd)");
        let scrub = route.rfind("scrub_per_command_relaxations(&mut cmd");
        assert!(
            matches!((configure, scrub), (Some(c), Some(s)) if s > c),
            "a limpeza precisa rodar DEPOIS de configure em cada rota"
        );
    }
}

#[test]
fn the_scope_route_needs_the_manager_the_launcher_and_no_opt_out() {
    let run = Some(PathBuf::from("/usr/bin/systemd-run"));
    let socket = Path::new("/run/user/1000/systemd/private");
    assert_eq!(
        scope_launcher(None, run.clone(), Some(socket)),
        Some(PathBuf::from("/usr/bin/systemd-run"))
    );
    assert_eq!(scope_launcher(Some("1"), run.clone(), Some(socket)), run);
    for off in ["0", "false", "off"] {
        assert_eq!(
            scope_launcher(Some(off), run.clone(), Some(socket)),
            None,
            "{off}"
        );
    }
    assert_eq!(scope_launcher(None, run, None), None, "no user manager");
    assert_eq!(
        scope_launcher(None, None, Some(socket)),
        None,
        "no systemd-run"
    );
}

#[test]
fn the_scope_launch_execs_the_daemon_after_a_double_dash() {
    let args = scope_args(
        Path::new("/opt/bin/touring-daemon"),
        "touring-daemon /tmp/x.sock",
    );
    let args: Vec<String> = args
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        args,
        [
            "--user",
            "--scope",
            "--quiet",
            "--collect",
            "--expand-environment=no",
            "--description=touring-daemon /tmp/x.sock",
            "--",
            "/opt/bin/touring-daemon",
        ]
    );
}

#[test]
fn comm_is_the_file_name_cut_at_fifteen_bytes() {
    assert_eq!(comm_of(Path::new("/usr/bin/systemd-run")), "systemd-run");
    assert_eq!(
        comm_of(Path::new("/x/touring-daemon-strace")),
        "touring-daemon-"
    );
}

/// A launcher that exits unsuccessfully before `exec` (no user bus, a refused
/// scope) must never leave the caller without a daemon: the direct route runs.
#[test]
fn a_launcher_that_fails_before_exec_falls_back_to_the_direct_route() {
    let failing = find_on_path("false").expect("`false` on PATH");
    let mut spawned = spawn_with_launcher(
        Some(&failing),
        &sleep_bin(),
        "probe",
        SCOPE_EXEC_WAIT,
        &|cmd| {
            cmd.arg("30");
        },
    )
    .expect("direct spawn");
    let comm =
        std::fs::read_to_string(format!("/proc/{}/comm", spawned.child.id())).unwrap_or_default();
    let _ = spawned.child.kill();
    let _ = spawned.child.wait();
    assert_eq!(spawned.route, SpawnRoute::Direct);
    assert_eq!(
        comm.trim_end(),
        "sleep",
        "the fallback runs the daemon itself"
    );
}

/// Cross-audit 14/09/2026 (C8): a launcher that never reaches `exec` holds the
/// caller only for the bound it was given — a hook's quarter second, not the
/// operator's 2 s — and is then taken as launched, never raced by a second daemon.
#[test]
fn a_slow_launcher_holds_the_caller_only_for_its_bound() {
    let dir = tempfile::tempdir().expect("tempdir");
    let launcher = dir.path().join("slowlaunch");
    // A shell loop keeps the process named after the launcher (an `exec` would
    // rename it at once and the bound would never be exercised).
    std::fs::write(&launcher, "#!/bin/sh\nwhile :; do sleep 1; done\n").expect("script");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let started = Instant::now();
    let mut spawned = spawn_with_launcher(
        Some(&launcher),
        &sleep_bin(),
        "probe",
        Duration::from_millis(200),
        &|_cmd| {},
    )
    .expect("spawn");
    let elapsed = started.elapsed();
    let comm =
        std::fs::read_to_string(format!("/proc/{}/comm", spawned.child.id())).unwrap_or_default();
    let _ = spawned.child.kill();
    let _ = spawned.child.wait();
    assert_eq!(
        comm.trim_end(),
        "slowlaunch",
        "the launcher never reached exec"
    );
    assert!(
        elapsed >= Duration::from_millis(200),
        "the bound was waited: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_millis(1500),
        "held the caller for {elapsed:?}"
    );
    assert_eq!(
        spawned.route,
        SpawnRoute::UserScope,
        "taken as launched, not doubled"
    );
}

/// Live, wherever a systemd user manager answers: the daemon ends up in a
/// transient scope — not in the cgroup of the process that launched it, which is
/// the cgroup a oneshot unit kills when it ends. Without a manager the test says
/// so instead of passing silently on the direct route.
#[test]
fn with_a_user_manager_the_daemon_owns_a_transient_scope() {
    let Some(launcher) = scope_launcher(
        None,
        find_on_path("systemd-run"),
        user_manager_socket().as_deref(),
    ) else {
        eprintln!(
            "SKIP: no systemd user manager reachable — the scope route is not exercised here"
        );
        return;
    };
    let mut spawned = spawn_with_launcher(
        Some(&launcher),
        &sleep_bin(),
        "touring daemon_spawn test",
        SCOPE_EXEC_WAIT,
        &|cmd| {
            cmd.arg("30");
        },
    )
    .expect("spawn");
    let pid = spawned.child.id();
    // The launcher's exec is awaited by the spawn; the daemon's cgroup is final.
    let cgroup = std::fs::read_to_string(format!("/proc/{pid}/cgroup")).unwrap_or_default();
    let own = std::fs::read_to_string("/proc/self/cgroup").unwrap_or_default();
    let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
    let _ = spawned.child.kill();
    let _ = spawned.child.wait();
    assert_eq!(spawned.route, SpawnRoute::UserScope, "cgroup: {cgroup}");
    assert_eq!(
        comm.trim_end(),
        "sleep",
        "the scope process became the daemon"
    );
    assert!(
        cgroup.trim_end().ends_with(".scope") && cgroup.contains("/run-"),
        "a transient scope of its own: {cgroup}"
    );
    assert_ne!(cgroup, own, "not the launching process's cgroup");
}
