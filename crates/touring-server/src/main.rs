//! Touring — Unified Rust MCP Mega-Server + Neural Hook Accelerator
//!
//! Modes:
//!   - `touring serve` (or no args): MCP server over stdio
//!   - `touring <command>`: CLI subcommand dispatched via command table
//!
//! Exit code contract:
//!   - Hook commands (ErrorPolicy::HookSilent): exit 0 always
//!   - Tool commands (ErrorPolicy::ExitOnError): exit 1 on error
//!   - MCP server: propagates errors via `?`
use tracing::{error, info};

// Re-export lib modules needed by CLI handlers in binary context.
// In the lib context these are at crate root; in the binary context
// we need to explicitly import them since main.rs has its own module tree.
pub use touring_server::agent_diary;
pub use touring_server::cli;
pub use touring_server::memory_store;
pub use touring_server::telemetry_init;

// Heap profiling allocator — installs DHAT as the global allocator when
// the `dhat-heap` feature is active. The production allocator (mimalloc)
// is owned by `touring-core::alloc` behind its `mimalloc-allocator` feature,
// which is toggled off by `--no-default-features` when activating dhat-heap.
#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

// Wave 5 (2026-04-18) — jemalloc production allocator.
//
// Opt in with `--no-default-features --features jemalloc,<others>`. The
// `prod-allocator` (mimalloc) and `dhat-heap` features are mutually
// exclusive with `jemalloc` — the compile_error! guards below catch
// misconfiguration at build time rather than producing the opaque
// "duplicate #[global_allocator]" linker error.
#[cfg(all(feature = "jemalloc", feature = "prod-allocator"))]
compile_error!(
    "`jemalloc` and `prod-allocator` install competing global allocators. \
     Disable one: `--no-default-features --features jemalloc,...`."
);
#[cfg(all(feature = "jemalloc", feature = "dhat-heap"))]
compile_error!("`jemalloc` and `dhat-heap` install competing global allocators.");

#[cfg(all(
    feature = "jemalloc",
    not(feature = "prod-allocator"),
    not(feature = "dhat-heap")
))]
#[global_allocator]
static JEMALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

// ── MCP Server Mode ──────────────────────────────────────────────────

async fn run_serve() -> anyhow::Result<()> {
    info!("Touring MCP Server starting...");

    // D42-S1: Wire CC hooks into daemon startup lifecycle.
    // install_cc_hooks() is idempotent — skips files that already exist with
    // identical content, so this is safe to call on every startup without
    // duplicating what `touring init --cc-setup` already installed.
    use touring_server::cli::init::install_cc_hooks;
    if let Err(e) = install_cc_hooks() {
        error!("Failed to install CC hooks: {}", e);
        // Non-fatal: daemon can still serve MCP without hooks installed.
    }

    let server = touring_server::server::TouringServer::new().map_err(|e| {
        error!("Failed to initialize Touring server: {}", e);
        anyhow::anyhow!("Server init failed: {}", e)
    })?;

    server.spawn_background_tasks();

    info!("Touring server initialized, serving MCP over stdio");

    let transport = rmcp::transport::io::stdio();
    let service = rmcp::ServiceExt::serve(server, transport)
        .await
        .map_err(|e| {
            error!("Failed to start MCP service: {}", e);
            e
        })?;

    service.waiting().await?;

    Ok(())
}

// ── Entry Point ──────────────────────────────────────────────────────

/// Main synchronous entry. Builds the rayon global pool and the tokio
/// multi-thread runtime explicitly so operators can tune thread counts
/// via env vars without recompiling.
///
/// Env var overrides (all optional):
///   - `TOURING_MCP_WORKERS`       — tokio worker threads (default: physical cores)
///   - `TOURING_BLOCKING_WORKERS`  — tokio blocking pool cap (default: 512)
///   - `TOURING_RAYON_THREADS`     — rayon global pool size (default: physical/2)
fn main() -> anyhow::Result<()> {
    // Sprint 3 PC-1 (REGRA #19): set kernel-visible comm BEFORE rayon + tokio
    // runtimes spawn workers. We peek at argv[1] to pick the identity:
    // `touring serve` → MCP bridge → "touring-mcp"; any other subcommand
    // (or none) → ephemeral CLI client → "touring-cli". Both kinds are
    // visible to `ps -o comm` and the cli_handlers daemon-ctl census.
    {
        let early_args: Vec<String> = std::env::args().collect();
        let early_subcmd = early_args.get(1).map(String::as_str).unwrap_or("serve");
        let is_mcp_bridge = early_subcmd == "serve";
        let comm = if is_mcp_bridge {
            "touring-mcp"
        } else {
            "touring-cli"
        };
        touring_hooks::proc_identity::set_process_name(comm);

        // No papel de CLI, restaura o `SIGPIPE` padrão. Rust instala `SIG_IGN`,
        // o que faz `touring … | head -1` entrar em panic com "Broken pipe" —
        // e `panic = "abort"` transforma isso em SIGABRT (exit 134). Foi o que
        // derrubou o `propagate-release.sh` em 03/08/2026. O bridge MCP fica
        // com o `SIG_IGN` de propósito: lá, morrer em silêncio esconderia o
        // erro. Ver `panic_log::restore_default_sigpipe_for_cli`.
        if !is_mcp_bridge {
            touring_hooks::panic_log::restore_default_sigpipe_for_cli();
        }
    }

    // Heap profiling — only instantiated when dhat-heap feature is active.
    // Drop order matters: this guard must outlive main() so that the
    // dhat-heap.json report writes on process exit.
    #[cfg(feature = "dhat-heap")]
    let _dhat_profiler = dhat::Profiler::new_heap();

    // S5 (2026-04-21) — Rayon global pool configured BEFORE the tokio
    // runtime boots. This prevents `rayon::par_iter` calls (used inside
    // pre_edit signal scoring and quality analysis) from cannibalizing
    // tokio worker threads when both share the default global pool.
    // Budget default: half physical cores to leave CPU headroom for
    // tokio workers and blocking pool.
    let rayon_threads = std::env::var("TOURING_RAYON_THREADS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or_else(|| (num_cpus::get_physical() / 2).max(2));
    if let Err(e) = rayon::ThreadPoolBuilder::new()
        .num_threads(rayon_threads)
        .thread_name(|i| format!("touring-rayon-{i}"))
        .build_global()
    {
        eprintln!("rayon global pool init warning: {e}; default pool retained");
    }

    // S2 (2026-04-21) — Explicit tokio multi-thread runtime. Previously
    // used `#[tokio::main]` with all defaults, which defaults worker
    // count to num_cpus (logical = 32 on this host via SMT). For
    // CPU-bound workloads (AST parse, SIMD, MCTS) physical-core count
    // typically yields better throughput because SMT siblings compete
    // for L1/L2 cache.
    let workers = std::env::var("TOURING_MCP_WORKERS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or_else(num_cpus::get_physical);
    let max_blocking = std::env::var("TOURING_BLOCKING_WORKERS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(512);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .max_blocking_threads(max_blocking)
        .thread_name("touring-mcp-worker")
        .thread_stack_size(4 * 1024 * 1024) // 4 MiB — AST recursion headroom
        .enable_all()
        .build()?;

    rt.block_on(async_main())
}

/// Resolve the dispatch subcommand, tolerating global flags that PRECEDE it
/// (`touring --brief <cmd>`, `touring -j status`), not only follow it.
///
/// Delegates to [`cli::common::parse_global_flags`], which strips the globals
/// (`-j`/`--json`/`-v`/`--brief`/`--full`/`--timeout`) and seeds the
/// `BRIEF_OUTPUT` / `DAEMON_READ_TIMEOUT_SECS` process mirrors — so a leading
/// `--brief`/`--timeout` takes effect. The original argv still flows to the
/// handler, which re-parses globals from any position (idempotent). Returns
/// `"serve"` when no positional subcommand remains. (A1 / REGRA #0)
fn resolve_subcommand(args: &[String]) -> String {
    let (_globals, filtered) = cli::common::parse_global_flags(args);
    filtered
        .get(1)
        .map(|s| s.to_string())
        .unwrap_or_else(|| "serve".to_string())
}

/// Modification time of the running executable, as UTC RFC-3339.
///
/// `VERGEN_BUILD_TIMESTAMP` is stamped by `build.rs`, and cargo only re-runs
/// that script when `Cargo.toml` or `build.rs` change. A rebuild driven purely
/// by a source edit therefore leaves `built:` pointing at an *older* build —
/// observed 04/08/2026, when a binary written at 21:55 still reported the
/// 18:36 stamp. This value is read from the binary on disk at every
/// invocation, so it cannot go stale: it is the field to trust when answering
/// "is the binary I am running the one I just built?" — the recurring
/// stale-binary diagnosis that `update-touring` exists to settle.
fn binary_mtime_utc() -> String {
    std::env::current_exe()
        .and_then(|path| path.metadata())
        .and_then(|meta| meta.modified())
        .map(|t| {
            chrono::DateTime::<chrono::Utc>::from(t)
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string()
        })
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Async body of `main()`. Holds the original subcommand dispatch and
/// MCP server bootstrap logic unchanged — only the runtime construction
/// moved out to `main()` to enable explicit tuning.
async fn async_main() -> anyhow::Result<()> {
    // Parse subcommand BEFORE telemetry init so we can pick the right
    // subscriber profile. The daemon (`touring serve`) owns tokio-console
    // port 6669, the OTLP exporter socket, and the rotated log file.
    // Short-lived CLI invocations must not touch any of those — see
    // telemetry_init::Mode for the full rationale.
    let args: Vec<String> = std::env::args().collect();
    // Global flags may precede the subcommand (`touring --brief wiring audit`),
    // not only follow it — resolve the real subcommand past any leading globals.
    let subcommand_owned = resolve_subcommand(&args);
    let subcommand = subcommand_owned.as_str();

    let mode = if subcommand == "serve" {
        telemetry_init::Mode::Daemon
    } else {
        telemetry_init::Mode::Cli
    };

    if let Err(e) = telemetry_init::init(mode) {
        // Don't abort — a broken telemetry init must not prevent the MCP
        // server from serving or the CLI from responding. Fall back to a
        // best-effort stderr path.
        eprintln!("telemetry_init failed: {e}; falling back to stderr fmt");
    }

    // Initialize plugin registry early — all providers must register
    // before any tool handler tries to resolve them.
    touring_foundation::plugin::populate_global_registry();

    // Register embedding providers as plugins (D27 wiring — 2026-05-03)
    // Wrapped via EmbeddingProviderPlugin so FastEmbedProvider implements ProviderPlugin.
    // This enables SearchPipeline::with_registry::<FastEmbedProvider>() to retrieve
    // the provider from global_registry() at runtime.
    #[cfg(feature = "fastembed")]
    {
        use touring_foundation::plugin::embeddings::EmbeddingProviderPlugin;
        use touring_storage::embeddings::{FastEmbedModel, FastEmbedProvider};

        let provider = FastEmbedProvider::with_model(FastEmbedModel::BgeSmall);
        let plugin = EmbeddingProviderPlugin::new(
            provider,
            "default",
            touring_foundation::plugin::PluginFamily::Embeddings,
        );
        touring_foundation::plugin::global_registry().register(Box::new(plugin));
    }

    // ── Built-in commands (not in the table) ────────────────────────

    match subcommand {
        "serve" => return run_serve().await,
        "--version" | "-V" | "version" => {
            // Wave 5 (2026-04-18) — enriched version line. build.rs emits
            // VERGEN_* env vars under the `build-info` feature. When the
            // feature is off (or the provider failed — shallow clone,
            // no git, etc.) each `option_env!` returns None and we print
            // "unknown" rather than failing to compile the match arm.
            let pkg_version = env!("CARGO_PKG_VERSION");
            let git_sha = option_env!("VERGEN_GIT_SHA").unwrap_or("unknown");
            let rustc = option_env!("VERGEN_RUSTC_SEMVER").unwrap_or("unknown");
            let built = option_env!("VERGEN_BUILD_TIMESTAMP").unwrap_or("unknown");
            let features = option_env!("VERGEN_CARGO_FEATURES").unwrap_or("");
            eprintln!("touring {pkg_version}");
            eprintln!("  git:      {git_sha}");
            eprintln!("  rustc:    {rustc}");
            eprintln!("  built:    {built}");
            eprintln!("  binary:   {} (mtime)", binary_mtime_utc());
            if !features.is_empty() {
                eprintln!("  features: {features}");
            }
            return Ok(());
        }
        "--help" | "-h" | "help" => {
            cli::common::print_help(&cli::command_table::command_table());
            return Ok(());
        }
        _ => {} // fall through to table dispatch
    }

    // ── Table-driven dispatch ───────────────────────────────────────

    let table = cli::command_table::command_table();
    if let Some(cmd) = table.iter().find(|c| c.name == subcommand) {
        // A1: heavy-output command families (`wiring`/`viz`/`graph`, whose
        // snapshots reach MBs) default to `--brief` so the LLM context stays
        // lean; `--full` restores the complete output. Small outputs are
        // unaffected (only arrays over the elision threshold collapse to counts).
        cli::common::apply_heavy_brief_default(cmd.name, &args);
        match (cmd.handler)(&args) {
            Ok(()) => {}
            Err(e) => {
                error!("{} failed: {}", cmd.name, e);
                if cmd.error_policy == cli::common::ErrorPolicy::ExitOnError {
                    std::process::exit(1);
                }
                // HookSilent: swallow error, exit 0
            }
        }
    } else {
        error!("Unknown subcommand: {subcommand}. Run 'touring --help' for usage.");
        std::process::exit(1);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{binary_mtime_utc, resolve_subcommand};

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    /// The `binary:` field must be read from the executable on disk, never
    /// baked in at compile time — that is the whole reason it exists next to
    /// `built:`, which cargo can leave stale (04/08/2026: binary written at
    /// 21:55 still reporting the 18:36 build stamp).
    #[test]
    fn binary_mtime_is_read_from_disk_not_a_compile_time_constant() {
        let stamp = binary_mtime_utc();
        assert_ne!(stamp, "unknown", "the test binary exists, so its mtime must resolve");

        let parsed = chrono::DateTime::parse_from_rfc3339(&stamp)
            .unwrap_or_else(|e| panic!("`{stamp}` is not RFC-3339: {e}"));

        // A compile-time constant would drift from the file forever; a disk
        // read tracks it. Pin only what cannot be true of a stale constant:
        // this binary was linked after the crate existed and is not in the
        // future.
        let epoch_2020 = chrono::DateTime::parse_from_rfc3339("2020-01-01T00:00:00Z").unwrap();
        assert!(parsed > epoch_2020, "mtime {stamp} predates the crate");
        assert!(
            parsed <= chrono::Utc::now() + chrono::Duration::days(1),
            "mtime {stamp} is in the future — not a real file timestamp"
        );
    }

    #[test]
    fn leading_brief_resolves_real_subcommand() {
        // REGRA #0: `touring --brief wiring orphans` must dispatch `wiring`,
        // not fail with "Unknown subcommand: --brief".
        assert_eq!(
            resolve_subcommand(&argv(&["touring", "--brief", "wiring", "orphans"])),
            "wiring"
        );
    }

    #[test]
    fn leading_json_verbose_timeout_resolve_subcommand() {
        assert_eq!(
            resolve_subcommand(&argv(&["touring", "-j", "status"])),
            "status"
        );
        assert_eq!(
            resolve_subcommand(&argv(&["touring", "--json", "status"])),
            "status"
        );
        assert_eq!(
            resolve_subcommand(&argv(&["touring", "-v", "status"])),
            "status"
        );
        // `--timeout` consumes its value — the subcommand is past both tokens.
        assert_eq!(
            resolve_subcommand(&argv(&["touring", "--timeout", "5", "status"])),
            "status"
        );
    }

    #[test]
    fn trailing_globals_and_builtins_preserved() {
        // Globals after the subcommand still work (unchanged behaviour).
        assert_eq!(
            resolve_subcommand(&argv(&["touring", "wiring", "orphans", "--brief"])),
            "wiring"
        );
        // Non-global tokens (`--version`/`--help`) are preserved for the builtin match arms.
        assert_eq!(
            resolve_subcommand(&argv(&["touring", "--version"])),
            "--version"
        );
        assert_eq!(resolve_subcommand(&argv(&["touring", "--help"])), "--help");
        // No positional subcommand → daemon (`serve`), same as bare `touring`.
        assert_eq!(resolve_subcommand(&argv(&["touring"])), "serve");
    }
}
