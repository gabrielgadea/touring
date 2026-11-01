//! Entry point for the `touring-daemon` binary.
//!
//! This is a thin wrapper around `daemon::run_daemon_async()` with signal handling
//! and tracing initialization. Kept minimal — all logic lives in `daemon.rs`.
//!
//! Signal handling: SIGINT/SIGTERM are handled inside the tokio runtime via
//! `tokio::signal`, which allows calling the async `graceful_shutdown()` to flush
//! WAL, LinUCB, and CRDT state before exiting. The previous C-level `signal()`
//! handler bypassed `graceful_shutdown()` entirely, risking data loss.

const USAGE: &str = "touring-daemon — the Touring backend (one per socket).\n\
\n\
Takes no arguments: every launcher (`touring_foundation::daemon_spawn`, `update-touring`)\n\
execs it bare. Manage it with `touring daemon-ctl status|restart|stop` (REGRA #19).\n\
\n\
Options:\n  -V, --version  print the version and exit (never starts a daemon)\n  -h, --help     print this help and exit";

/// What the binary does with its arguments, decided BEFORE any side effect.
///
/// D5 (analise-92, 06/10/2026): `main` never read its arguments, so `touring-daemon --version` — a
/// probe — went on to bind and start a daemon, leaving through `AlreadyAlive` with rc 0. A question
/// must never become a spawn (REGRA #19). `None` means "no arguments: run the daemon"; anything
/// else is `(exit code, text)` — 0 goes to stdout, the rest to stderr.
fn early_exit(args: &[String]) -> Option<(i32, String)> {
    let first = args.first()?;
    Some(match first.as_str() {
        "--version" | "-V" => (0, format!("touring-daemon {}", env!("CARGO_PKG_VERSION"))),
        "--help" | "-h" => (0, USAGE.to_string()),
        other => (
            2,
            format!(
                "touring-daemon: unexpected argument '{other}' — the daemon takes none (only --version, \
                 --help). Launch it bare, or manage it with `touring daemon-ctl`."
            ),
        ),
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some((code, text)) = early_exit(&args) {
        if code == 0 {
            println!("{text}");
        } else {
            eprintln!("{text}");
        }
        std::process::exit(code);
    }

    // Sprint 4.5 (Wave touring-process-hygiene 2026-05-23): install panic
    // forensics hook FIRST — before tracing init, before set_process_name,
    // before tokio runtime spawns any worker. Captures every panic in any
    // thread to ~/.claude/touring/daemon-crash.jsonl (overridable via
    // TOURING_CRASH_LOG_PATH env var, which update-touring sets at spawn).
    // Workspace profile uses `panic = "abort"` so the process still
    // terminates immediately after the hook returns — no behavior change,
    // only diagnostics.
    touring_hooks::panic_log::install_hook();

    // VGP camada 2: o pool pyright fica LIGADO neste processo (o daemon é o dono do pyright —
    // o hook standalone efêmero NUNCA chama isto e `checar` devolve unknown sem spawnar).
    touring_pyright::pool::habilitar_neste_processo();

    // Sprint 3 PC-1 (REGRA #19): set kernel-visible comm so `ps -o comm`,
    // `pgrep`, and the cli_handlers daemon detector can distinguish this
    // singleton from hook handlers / MCP bridges / CLI clients that share
    // the same exe path. Must run BEFORE the tokio runtime spawns workers.
    touring_hooks::proc_identity::set_process_name("touring-daemon");

    // A2 (2026-08-08): install the cl100k_base counter into the context-savings
    // ledger. Registration is free — the BPE table builds lazily on the first
    // compression/routing event, so a daemon whose routing subsystem never
    // fires never pays for it. Without this the ledger reports tokens as
    // `not_measured`, which is the truth, rather than bytes/4 dressed up as a
    // token count (the pre-A2 behavior). Disable with TOURING_TOKEN_METER=0.
    touring_hooks::token_meter::install();

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

#[cfg(test)]
mod tests {
    use super::early_exit;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn no_argument_runs_the_daemon() {
        assert_eq!(early_exit(&[]), None);
    }

    #[test]
    fn a_version_probe_answers_and_never_runs_the_daemon() {
        for flag in ["--version", "-V"] {
            let (code, text) = early_exit(&args(&[flag])).expect("a probe is answered, not run");
            assert_eq!(code, 0);
            assert_eq!(
                text,
                format!("touring-daemon {}", env!("CARGO_PKG_VERSION"))
            );
        }
    }

    #[test]
    fn help_answers_and_names_the_manager() {
        let (code, text) = early_exit(&args(&["--help"])).expect("help is answered");
        assert_eq!(code, 0);
        assert!(
            text.contains("daemon-ctl") && text.contains("--version"),
            "{text}"
        );
    }

    #[test]
    fn an_unknown_argument_is_refused_with_the_accepted_ones() {
        let (code, text) =
            early_exit(&args(&["--socket", "/tmp/x.sock"])).expect("refused, not run");
        assert_eq!(code, 2);
        assert!(
            text.contains("'--socket'") && text.contains("--version"),
            "{text}"
        );
    }
}
