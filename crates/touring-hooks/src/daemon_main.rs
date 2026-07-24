//! Entry point for the `touring-daemon` binary.
//!
//! This is a thin wrapper around `daemon::run_daemon_async()` with signal handling
//! and tracing initialization. Kept minimal — all logic lives in `daemon.rs`.
//!
//! Signal handling: SIGINT/SIGTERM are handled inside the tokio runtime via
//! `tokio::signal`, which allows calling the async `graceful_shutdown()` to flush
//! WAL, LinUCB, and CRDT state before exiting. The previous C-level `signal()`
//! handler bypassed `graceful_shutdown()` entirely, risking data loss.

fn main() {
    // Sprint 4.5 (Wave touring-process-hygiene 2026-05-23): install panic
    // forensics hook FIRST — before tracing init, before set_process_name,
    // before tokio runtime spawns any worker. Captures every panic in any
    // thread to ~/.claude/touring/daemon-crash.jsonl (overridable via
    // TOURING_CRASH_LOG_PATH env var, which update-touring sets at spawn).
    // Workspace profile uses `panic = "abort"` so the process still
    // terminates immediately after the hook returns — no behavior change,
    // only diagnostics.
    touring_hooks::panic_log::install_hook();

    // Sprint 3 PC-1 (REGRA #19): set kernel-visible comm so `ps -o comm`,
    // `pgrep`, and the cli_handlers daemon detector can distinguish this
    // singleton from hook handlers / MCP bridges / CLI clients that share
    // the same exe path. Must run BEFORE the tokio runtime spawns workers.
    touring_hooks::proc_identity::set_process_name("touring-daemon");

    // Initialize tracing — same pattern as touring-hook
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_env("TOURING_LOG"))
        .init();

    // Build a multi-threaded tokio runtime for the async daemon.
    // Signal handling is done inside the runtime (run_daemon_async spawns
    // tokio::signal tasks), so no C-level signal handlers are needed.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    match rt.block_on(touring_hooks::daemon::run_daemon_async()) {
        Ok(()) => {
            // Sprint 4.5 (2026-05-23): loud marker on the rt.block_on Ok arm
            // — daemon should never normally exit through this path (the
            // accept loop is `loop {}` infinite + `#[allow(unreachable_code)]`
            // on the trailing Ok(()) at the bottom of run_daemon_async).
            // The legitimate Ok exit is the AlreadyAlive race (logged
            // separately by daemon.rs). Reaching here without AlreadyAlive
            // would indicate the accept loop terminated unexpectedly.
            eprintln!(
                "[touring-daemon] rt.block_on returned Ok — exit(0). Check prior stderr for `AlreadyAlive` (expected) vs unreached accept-loop exit (UNEXPECTED)"
            );
            std::process::exit(0)
        }
        Err(e) => {
            eprintln!("[touring-daemon] fatal: rt.block_on returned Err — exit(1): {e}");
            std::process::exit(1);
        }
    }
}
