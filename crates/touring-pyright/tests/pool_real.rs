//! VGP layer-2 pool against the REAL `pyright-langserver` of the `analise` project.
//!
//! `#[ignore]`: it needs the project venv's pyright. Run explicitly:
//! `cargo test -p touring-pyright --test pool_real -- --ignored`.
//! It lives in its own test binary on purpose: it counts the child processes of the *test process*,
//! which any other test spawning a language server in the same binary would pollute.
//! `VGP_L2_PROJECT` overrides the project root (default `~/projects/analise`).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use touring_pyright::pool;
use touring_pyright::pyright::Verdict;

fn project_root() -> PathBuf {
    if let Some(custom) = std::env::var_os("VGP_L2_PROJECT") {
        return PathBuf::from(custom);
    }
    let home = std::env::var_os("HOME").expect("HOME must be set");
    PathBuf::from(home).join("projects/analise")
}

/// Parent pid of `pid`: field 4 of `/proc/<pid>/stat` (the `comm` field may hold spaces and
/// parentheses, so parsing starts after the LAST `)`).
fn parent_of(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after_comm = &stat[stat.rfind(')')? + 1..];
    after_comm.split_whitespace().nth(1)?.parse().ok()
}

/// Direct children of this test process (the pyright wrapper is one; node is its child).
fn my_children() -> Vec<u32> {
    let me = std::process::id();
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let mut pids: Vec<u32> = dir
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse::<u32>().ok())
        .filter(|pid| parent_of(*pid) == Some(me))
        .collect();
    pids.sort_unstable();
    pids
}

fn p95(sorted_ms: &[u128]) -> u128 {
    sorted_ms[(sorted_ms.len() * 95 / 100).min(sorted_ms.len() - 1)]
}

#[test]
#[ignore = "needs the real pyright-langserver of the analise project"]
fn twenty_checks_on_one_root_share_one_server_and_the_warm_ones_are_fast() {
    pool::habilitar_neste_processo();
    let root = project_root();
    assert!(
        my_children().is_empty(),
        "fixture: the test process must start without children: {:?}",
        my_children()
    );

    // Check #0 is the cold one (spawn + initialize + first analysis, ~650 ms here, budget 1000 ms). On a
    // busy machine it can overrun and the pool, by design, drops that session and answers `unknown`;
    // so the warm-up may take a few attempts. Reuse is then proven on the 19 checks that follow.
    let cold = |i: usize| {
        let code = format!("import os\nprint(os.caminho_inventado)\n# edicao {i}\n");
        let started = Instant::now();
        let r = pool::checar(&root, "<string>", &code, None, &[]);
        (r, started.elapsed().as_millis())
    };
    let mut attempts = 0;
    let (first, cold_ms) = loop {
        attempts += 1;
        let (r, ms) = cold(0);
        if r.verdict != Verdict::Unknown || attempts == 3 {
            break (r, ms);
        }
        println!(
            "cold attempt {attempts} timed out ({ms} ms): {:?}",
            r.motivo
        );
    };
    assert_eq!(first.verdict, Verdict::Deny, "warm-up: {first:#?}");
    println!(
        "cold check (spawn + initialize + first analysis): {cold_ms} ms in {attempts} attempt(s)"
    );

    // sessions dropped by a timed-out attempt are closed on a background thread: let them leave
    let mut after_first = my_children();
    for _ in 0..100 {
        if after_first.len() == 1 {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
        after_first = my_children();
    }

    let mut warm_ms = Vec::new();
    for i in 1..20 {
        let code = format!("import os\nprint(os.caminho_inventado)\n# edicao {i}\n");
        let started = Instant::now();
        let r = pool::checar(&root, "<string>", &code, None, &[]);
        warm_ms.push(started.elapsed().as_millis());
        assert_eq!(r.verdict, Verdict::Deny, "check #{i}: {r:#?}");
    }

    let after_all = my_children();
    assert_eq!(
        after_first.len(),
        1,
        "after the warm-up exactly one server is alive: {after_first:?}"
    );
    assert_eq!(
        after_all, after_first,
        "19 more checks must reuse that very process (same pid, no new spawn)"
    );

    warm_ms.sort_unstable();
    let p95_ms = p95(&warm_ms);
    println!(
        "warm wall-clock ms (sorted, n={}): {warm_ms:?} p95={p95_ms}",
        warm_ms.len()
    );
    assert!(
        p95_ms < 150,
        "p95 of the 19 warm checks was {p95_ms} ms (limit 150)"
    );

    // idleness: far in the future the session is closed and the process is really gone
    let reaped = pool::reap_idle(Instant::now() + Duration::from_secs(10_000));
    assert_eq!(reaped, 1);
    let mut gone = false;
    for _ in 0..100 {
        if my_children().is_empty() {
            gone = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        gone,
        "reaped session left a process behind: {:?}",
        my_children()
    );
}
