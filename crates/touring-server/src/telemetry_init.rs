//! Centralized `tracing` subscriber setup for the `touring` binary.
//!
//! Feature gates compose additively — the base `fmt` layer always runs,
//! `console`, `otlp`, and `file-logs` layers are stacked only when the
//! calling binary is running in **daemon mode** (`touring serve`). CLI
//! subcommands (`touring doctor`, `touring status`, etc.) skip the heavy
//! layers to avoid contending for the tokio-console port with the daemon
//! and to avoid corrupting daily-rotated log files by racing multiple
//! processes on the same file. Calling `init` more than once is a
//! no-op after the first call succeeds (enforced by `try_init`).
//!
//! # Design invariants
//!
//! 1. **stderr-only base layer**: `stdout` is reserved for hook output and
//!    JSON-RPC framing. Violating this corrupts MCP transport.
//! 2. **Graceful degradation**: if an exporter cannot initialize (e.g.
//!    OTLP collector unreachable at boot, or tokio-console port already
//!    bound by the running daemon), we log a warning and continue with
//!    the remaining layers. Never panic during telemetry init.
//! 3. **Feature composability**: all optional layers use
//!    `tracing_subscriber::Layer` so they stack cleanly via `.with(...)`.
//! 4. **Single-owner resources**: `console-subscriber` binds 127.0.0.1:6669
//!    via tonic — only the daemon installs it. `file-logs` writes to a
//!    single rotated file — only the daemon writes. `otlp` batch exporter
//!    opens a long-lived gRPC connection — only the daemon opens one.

use touring_foundation::run_outcome::raw_output_requested;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Registry};

/// Which binary persona is initializing the subscriber.
///
/// The binary is dual-purpose: a long-lived MCP server (daemon) and a
/// short-lived CLI dispatcher. They have conflicting resource needs:
///
/// | Resource | Daemon | CLI |
/// |---|---|---|
/// | tokio-console port 6669 | owns | must not touch |
/// | daily-rotated log file | writes | must not race-append |
/// | OTLP batch exporter gRPC | persistent | would leak sockets |
/// | stderr `fmt` layer | yes | yes |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// `touring serve` — long-lived MCP server process. Installs all
    /// feature-gated observability layers that the build enabled.
    Daemon,
    /// `touring <subcommand>` — short-lived CLI invocation. Installs only
    /// the stderr `fmt` layer. Optional layers that would contend with the
    /// daemon for port or file ownership are suppressed regardless of
    /// feature flags.
    Cli,
    /// `touring run --raw` (or `TOURING_RUN_RAW=1`) — a short-lived CLI invocation whose stdout/stderr belong to
    /// the PROGRAM it runs (RAWRUN, 11/11/2026): same as [`Mode::Cli`], minus the
    /// once-per-process CEG enforcement banner, which would otherwise be the first line
    /// of every rewritten Bash call the model reads. An explicit `RUST_LOG` still wins.
    CliRawOutput,
}

/// The stderr filter every CLI invocation starts from when `RUST_LOG` is unset.
const CLI_DEFAULT_DIRECTIVES: &str = "info,hyper=warn,tonic=warn";

/// Target of the once-per-process sandbox banner (`touring-ceg`, `limits.rs`).
const CEG_ENFORCEMENT_TARGET: &str = "ceg::enforcement";

/// The default directives for a CLI persona: the raw-output persona silences the
/// enforcement banner, nothing else (a WARN or ERROR still reaches stderr).
fn cli_default_directives(mode: Mode) -> String {
    match mode {
        Mode::CliRawOutput => format!("{CLI_DEFAULT_DIRECTIVES},{CEG_ENFORCEMENT_TARGET}=off"),
        Mode::Cli | Mode::Daemon => CLI_DEFAULT_DIRECTIVES.to_string(),
    }
}

/// Whether this invocation answers like the shell command a gate replaced: a `touring run`
/// that asks for raw output. The RULE is `touring_foundation::run_outcome::
/// raw_output_requested` — the very function `cli::run` asks — fed with the flags found in
/// argv. `subcommand` is the one the dispatcher already resolved past any leading global
/// flag; a flag counts only as a whole argument (inside a `--code` body it is part of one
/// longer argument).
fn is_raw_run(subcommand: &str, args: &[String], env_value: Option<&str>) -> bool {
    let has = |flag: &str| args.iter().any(|a| a == flag);
    subcommand == "run"
        && raw_output_requested(has("--raw"), has("--brief") || has("--stream"), env_value)
}

/// The persona of this process, from the subcommand the dispatcher resolved, the raw argv
/// and the value of `RAW_RUN_ENV`: `serve` is the daemon, a raw `run` hands the streams to
/// its program, and everything else is the plain short-lived CLI.
#[must_use]
pub fn mode_for(subcommand: &str, args: &[String], raw_env: Option<&str>) -> Mode {
    if subcommand == "serve" {
        Mode::Daemon
    } else if is_raw_run(subcommand, args, raw_env) {
        Mode::CliRawOutput
    } else {
        Mode::Cli
    }
}

/// Initialize the global `tracing` subscriber for the `touring` binary.
///
/// # Mode
///
/// - `Mode::Daemon` — full observability stack (fmt + console + otlp +
///   file-logs, whichever features are compiled in). `console` is
///   additionally protected by a pre-bind probe on port 6669: if another
///   process already owns the port (e.g. a previous daemon instance that
///   did not shut down cleanly), the layer is skipped with a warning
///   rather than panicking.
/// - `Mode::Cli` — stderr-only. No port binds, no file races.
///
/// Returns `Ok(())` if at least the base layer initialized. Errors only
/// when both the base layer and filter setup fail, which we have not
/// observed in practice.
pub fn init(mode: Mode) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match mode {
        Mode::Daemon => init_daemon(),
        Mode::Cli | Mode::CliRawOutput => init_cli(mode),
    }
}

/// CLI-mode subscriber: stderr `fmt` + `EnvFilter` only.
fn init_cli(mode: Mode) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(cli_default_directives(mode)));
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(true);
    Registry::default()
        .with(filter)
        .with(fmt_layer)
        .try_init()?;
    tracing::debug!(target: "touring::telemetry", "subscriber initialized (cli mode)");
    Ok(())
}

/// Daemon-mode subscriber: full feature-gated stack.
///
/// When `console` feature is on, probes port 6669 before installing the
/// subscriber. On collision, degrades to the console-less path and keeps
/// the daemon serving. Protects against leftover daemon instances that
/// still own the port.
fn init_daemon() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,hyper=warn,tonic=warn"));
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(true);
    let registry = Registry::default().with(filter).with(fmt_layer);

    #[cfg(feature = "console")]
    {
        if probe_console_port() {
            finish_daemon_with_console(registry)
        } else {
            finish_daemon_no_console(registry)
        }
    }

    #[cfg(not(feature = "console"))]
    finish_daemon_no_console(registry)
}

/// Probe 127.0.0.1:6669. Returns true when free; prints a notice on conflict.
#[cfg(feature = "console")]
fn probe_console_port() -> bool {
    match std::net::TcpListener::bind("127.0.0.1:6669") {
        Ok(l) => {
            drop(l);
            true
        }
        Err(e) => {
            eprintln!("[telemetry] tokio-console disabled: port 6669 not available ({e})");
            false
        }
    }
}

#[cfg(feature = "console")]
fn finish_daemon_with_console<S>(
    registry: S,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    S: tracing::Subscriber + Send + Sync + 'static,
    for<'span> S: tracing_subscriber::registry::LookupSpan<'span>,
{
    let registry = registry.with(console_subscriber::spawn());

    #[cfg(feature = "otlp")]
    let registry = registry.with(build_otel_layer());

    #[cfg(feature = "file-logs")]
    let registry = registry.with(build_file_layer());

    #[cfg(feature = "tracy")]
    let registry = registry.with(build_tracy_layer());

    registry.try_init()?;
    tracing::info!(target: "touring::telemetry", "subscriber initialized (daemon, console on)");
    Ok(())
}

fn finish_daemon_no_console<S>(registry: S) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    S: tracing::Subscriber + Send + Sync + 'static,
    for<'span> S: tracing_subscriber::registry::LookupSpan<'span>,
{
    #[cfg(feature = "otlp")]
    let registry = registry.with(build_otel_layer());

    #[cfg(feature = "file-logs")]
    let registry = registry.with(build_file_layer());

    #[cfg(feature = "tracy")]
    let registry = registry.with(build_tracy_layer());

    registry.try_init()?;
    tracing::info!(target: "touring::telemetry", "subscriber initialized (daemon, console off)");
    Ok(())
}

/// Days of rotated `touring.log.<date>` files kept on disk.
#[cfg(feature = "file-logs")]
const LOG_FILES_KEPT: usize = 7;

/// Directory of the rotated log files: `TOURING_LOG_DIR`, else
/// `~/.claude/touring/logs`, else the system temp dir.
///
/// The default used to be `/tmp`, a tmpfs here: a watcher logging every
/// dropped event filled it with 25 GB in a day, and every writer on the
/// machine started leaving zero-byte files (cross-audit 14/09/2026, R2-1).
#[cfg(feature = "file-logs")]
fn log_dir(env_dir: Option<String>, home: Option<std::path::PathBuf>) -> std::path::PathBuf {
    match (env_dir.filter(|d| !d.is_empty()), home) {
        (Some(dir), _) => std::path::PathBuf::from(dir),
        (None, Some(home)) => home.join(".claude").join("touring").join("logs"),
        (None, None) => std::env::temp_dir(),
    }
}

#[cfg(feature = "file-logs")]
fn build_file_layer<S>() -> Option<impl tracing_subscriber::Layer<S>>
where
    S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
{
    use std::sync::OnceLock;
    use tracing_appender::non_blocking::WorkerGuard;
    use tracing_appender::rolling::{RollingFileAppender, Rotation};

    // Stash the WorkerGuard so it lives for the process. Dropping the
    // guard would flush + close the appender; we want the opposite —
    // keep flushing on every record until the process exits.
    static GUARD: OnceLock<WorkerGuard> = OnceLock::new();

    let dir = log_dir(
        std::env::var("TOURING_LOG_DIR").ok(),
        std::env::var_os("HOME").map(std::path::PathBuf::from),
    );
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!(
            "[telemetry] file logs disabled: cannot create {}: {e}",
            dir.display()
        );
        return None;
    }
    let appender = match RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("touring.log")
        .max_log_files(LOG_FILES_KEPT)
        .build(&dir)
    {
        Ok(appender) => appender,
        Err(e) => {
            eprintln!("[telemetry] file logs disabled: {e}");
            return None;
        }
    };
    let (non_blocking, guard) = tracing_appender::non_blocking(appender);
    let _ = GUARD.set(guard);

    Some(
        tracing_subscriber::fmt::layer()
            .with_writer(non_blocking)
            // ANSI codes leak into rotated files and confuse log aggregators
            // (Loki, Datadog, ELK) — strip them on the file path only.
            .with_ansi(false)
            .with_target(true),
    )
}

#[cfg(feature = "otlp")]
fn build_otel_layer<S>()
-> Option<tracing_opentelemetry::OpenTelemetryLayer<S, opentelemetry_sdk::trace::Tracer>>
where
    S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
{
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_otlp::WithExportConfig;

    let service_name = std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "touring".to_string());

    // Export only to a collector someone configured. Defaulting to
    // localhost:4317 with nothing listening made the batch processor log an
    // ExportError on every flush (cross-audit 14/09/2026, R2-16).
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .ok()
        .filter(|e| !e.is_empty())?;
    let exporter = match opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()
    {
        Ok(exporter) => exporter,
        Err(e) => {
            eprintln!("[telemetry] OTLP export disabled: {e}");
            return None;
        }
    };

    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .build();

    // Obtain a tracer scoped to our service and hand it to the tracing layer.
    // We intentionally skip `opentelemetry::global::set_tracer_provider` — in
    // OTel 0.31 the global setter has tighter trait bounds than the SDK
    // provider satisfies, and we do not need global-provider access because
    // all span creation flows through the tracing framework anyway.
    let tracer = provider.tracer(service_name);

    Some(tracing_opentelemetry::layer().with_tracer(tracer))
}

/// Build a `TracyLayer` that forwards span durations to a running Tracy
/// server via the Tracy protocol.
///
/// Safe to install without a Tracy server listening — the Tracy client
/// buffers locally until a server connects (or the buffer is dropped
/// on process exit). Meant for local dev profiling: attach the Tracy
/// GUI to visualize frame-by-frame timelines of hook handler durations,
/// template rendering, and VGP symbol resolution.
#[cfg(feature = "tracy")]
fn build_tracy_layer() -> tracing_tracy::TracyLayer<tracing_tracy::DefaultConfig> {
    tracing_tracy::TracyLayer::default()
}

/// Call once at shutdown to drain buffered spans before the process exits.
/// No-op when `otlp` feature is disabled.
pub fn shutdown() {
    // OTel 0.31 drains spans on Drop of the SdkTracerProvider held inside the
    // tracing layer. Nothing to do here unless we switch back to the global
    // provider model.
}

#[cfg(all(test, feature = "file-logs"))]
mod log_dir_tests {
    use super::log_dir;
    use std::path::PathBuf;

    #[test]
    fn logs_go_to_the_home_state_dir_never_to_the_tmpfs() {
        let home = PathBuf::from("/home/someone");
        assert_eq!(
            log_dir(None, Some(home.clone())),
            home.join(".claude/touring/logs")
        );
        assert_eq!(
            log_dir(Some(String::new()), Some(home.clone())),
            home.join(".claude/touring/logs"),
            "an empty TOURING_LOG_DIR is unset, not the current directory"
        );
        assert_eq!(
            log_dir(Some("/var/log/touring".into()), Some(home)),
            PathBuf::from("/var/log/touring")
        );
        assert_eq!(log_dir(None, None), std::env::temp_dir());
    }
}

#[cfg(all(test, feature = "dhat-heap"))]
mod dhat_tests {
    // When dhat-heap feature is active, the DHAT allocator is installed
    // in main.rs. Smoke tests verify allocation path still functions.
    #[test]
    fn allocator_smoke() {
        // The heap allocation IS the point — DHAT must record a real Vec
        // alloc, so the collect is intentional. Reading the whole buffer
        // back (sum) forces the optimizer to keep the allocation live.
        let v: Vec<u64> = (0..1024).collect();
        assert_eq!(v.len(), 1024);
        assert_eq!(v.iter().sum::<u64>(), (0..1024).sum::<u64>());
    }
}

/// RAWRUN (11/11/2026) — `touring run --raw` hands both streams to its program, so the
/// CEG enforcement banner (once per process, `ceg::enforcement`) must not reach stderr.
#[cfg(test)]
mod raw_run_tests {
    use super::{Mode, cli_default_directives, mode_for};
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::layer::SubscriberExt;

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn the_persona_follows_the_subcommand_and_the_whole_argument() {
        let raw = argv(&["touring", "run", "--lang", "bash", "--code", "ls", "--raw"]);
        assert_eq!(mode_for("run", &raw, None), Mode::CliRawOutput);
        let plain = argv(&["touring", "run", "--lang", "bash", "--code", "ls"]);
        assert_eq!(mode_for("run", &plain, None), Mode::Cli);
        // `--raw` inside a body is one longer argument, never the flag.
        let inside = argv(&["touring", "run", "--lang", "bash", "--code", "grep --raw x"]);
        assert_eq!(mode_for("run", &inside, None), Mode::Cli);
        // Another subcommand with a `--raw` of its own keeps the plain CLI persona.
        assert_eq!(mode_for("memory", &raw, None), Mode::Cli);
        assert_eq!(mode_for("serve", &raw, None), Mode::Daemon);
    }

    /// The gates' marker is the variable: same rule as `cli::run::wants_raw`.
    #[test]
    fn the_variable_makes_a_run_raw_unless_the_call_chose_a_presentation() {
        let plain = argv(&["touring", "run", "--lang", "bash", "--code", "ls"]);
        assert_eq!(mode_for("run", &plain, Some("1")), Mode::CliRawOutput);
        assert_eq!(mode_for("run", &plain, Some("0")), Mode::Cli);
        assert_eq!(mode_for("run", &plain, Some("")), Mode::Cli);
        let brief = argv(&[
            "touring", "run", "--lang", "bash", "--code", "ls", "--brief",
        ]);
        assert_eq!(mode_for("run", &brief, Some("1")), Mode::Cli);
        // Another subcommand never reads the variable.
        assert_eq!(mode_for("status", &plain, Some("1")), Mode::Cli);
    }

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Capture {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if let Ok(mut inner) = self.0.lock() {
                inner.extend_from_slice(buf);
            }
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// What a subscriber built from the persona's directives writes for the banner and for
    /// an unrelated WARN.
    fn written_under(mode: Mode) -> String {
        let capture = Capture::default();
        let sink = capture.clone();
        let subscriber = tracing_subscriber::Registry::default()
            .with(EnvFilter::new(cli_default_directives(mode)))
            .with(
                tracing_subscriber::fmt::layer()
                    .with_writer(move || sink.clone())
                    .with_ansi(false)
                    .with_target(true),
            );
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(target: "ceg::enforcement", "banner-do-sandbox");
            tracing::warn!(target: "touring::other", "aviso-real");
        });
        let bytes = capture.0.lock().map(|b| b.clone()).unwrap_or_default();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    #[test]
    fn the_raw_persona_silences_the_banner_and_only_the_banner() {
        let raw = written_under(Mode::CliRawOutput);
        assert!(!raw.contains("banner-do-sandbox"), "{raw}");
        assert!(
            raw.contains("aviso-real"),
            "a real WARN still reaches stderr: {raw}"
        );
        let plain = written_under(Mode::Cli);
        assert!(
            plain.contains("banner-do-sandbox"),
            "the plain CLI is unchanged: {plain}"
        );
        assert!(plain.contains("aviso-real"), "{plain}");
    }
}
