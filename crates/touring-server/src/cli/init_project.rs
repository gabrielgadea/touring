//! `touring init-project` — W12.1 (2026-05-23) — per-project scaffolder.
//!
//! Creates the rustup-pattern per-project tree under the current working
//! directory:
//!
//! ```text
//! .touring/
//! ├── touring.toml      # per-project config (project layer in W12.4 loader)
//! ├── data/             # per-project DBs (symbols.db, memory.db, graph.db)
//! ├── bin/              # per-project binary slot (resolved by hook walk-up shim)
//! └── hooks/            # project-local hook overrides (optional)
//! ```
//!
//! This is DIFFERENT from the existing `touring init` subcommand which manages
//! TOML *profile presets* under `~/.claude/touring/profiles/`. W12.1 spec calls
//! for a per-project rustup-like layout — both subcommands coexist (REGRA #0).
//!
//! Flags:
//! - `--force` — overwrite an existing `.touring/` tree (default: refuse)
//! - `--bare`  — skip generating the default `touring.toml` body (useful when
//!   the caller will write the file via another tool)
//!
//! Usage:
//! ```text
//! cd ~/projects/myproject
//! touring init-project              # creates .touring/{touring.toml,data,bin,hooks}
//! touring init-project --force      # overwrites existing tree
//! ```

use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};

/// Parsed command-line arguments for `touring init-project`.
#[derive(Debug, Default, Clone)]
pub struct InitProjectArgs {
    /// Overwrite an existing `.touring/` tree.
    pub force: bool,
    /// Skip generating the default `touring.toml` body.
    pub bare: bool,
    /// Project root (default: current working directory).
    pub project_root: Option<PathBuf>,
}

impl InitProjectArgs {
    /// Parse `args` slice (full argv including binary + subcommand).
    pub fn parse(args: &[String]) -> Self {
        let mut out = Self::default();
        // Skip args[0] (binary) and args[1] (subcommand "init-project").
        for a in args.iter().skip(2) {
            match a.as_str() {
                "--force" | "-f" => out.force = true,
                "--bare" => out.bare = true,
                other if other.starts_with("--root=") => {
                    let p = other.trim_start_matches("--root=");
                    out.project_root = Some(PathBuf::from(p));
                }
                _ => { /* ignore unknown — fail-open on extra flags */ }
            }
        }
        out
    }
}

/// Default `touring.toml` body written when `--bare` is not passed. Mirrors
/// the rustup `rust-toolchain.toml` shape (channel + components + targets).
///
/// The `channel` pin is derived from `CARGO_PKG_VERSION` at compile time, so a
/// scaffolded project pins the toolchain that actually created it. It used to
/// be the literal `"30.3.0"`, which meant every workspace version bump silently
/// began minting projects pinned to the PREVIOUS version — the drift was found
/// while bumping to 30.3.1 (04/08/2026). `concat!` + `env!` keep it a `const`.
const DEFAULT_TOURING_TOML: &str = concat!(
    "# Per-project touring configuration.\n\
# Read by `TouringConfig::detect_layered()` as the Project layer (highest precedence\n\
# below env-var overrides). Edit any field to override the User/System layers.\n\
\n\
# Cache + watcher tuning (sensible defaults for a fresh project)\n\
cache_size = 10000\n\
watcher_debounce_ms = 100\n\
max_file_size = 5242880\n\
debug = false\n\
\n\
# Embedding service (override per project to point to a remote GPU)\n\
# gpu_service_url = \"http://localhost:8200\"\n\
# embedding_dim = 384\n\
# auto_embed = true\n\
\n\
# JSONL watcher (set false in CI / batch-only environments)\n\
jsonl_watch_enabled = true\n\
jsonl_poll_interval_s = 30\n\
\n\
# Evolution engine (set false to disable periodic analysis)\n\
evolution_enabled = true\n\
evolution_interval_s = 300\n\
\n\
# Toolchain pin (rustup-like, Productization Fase 0): which installed touring\n\
# toolchain this project uses (`~/.touring/toolchains/<channel>`). Read by\n\
# `TouringConfig::detect_layered()`; consumed by `touring update` / `.touring/bin`.\n\
[toolchain]\n\
channel = \"",
    env!("CARGO_PKG_VERSION"),
    "\"\n\
\n\
# W12.5 — opt in to a dedicated per-project daemon (default OFF: without this\n\
# block the project shares the global daemon). When true, every touring client\n\
# inside this project resolves `<project>/.touring/daemon.sock` and the first\n\
# call autostarts the daemon there.\n\
# [daemon]\n\
# per_project = true\n"
);

/// Subdirectories created under `.touring/` (in addition to `touring.toml`).
const SUBDIRS: &[&str] = &["data", "bin", "hooks"];

/// CLI dispatch entry. Called from `cli::common::command_table`.
pub fn run(args: &[String]) -> Result<()> {
    let parsed = InitProjectArgs::parse(args);
    let root = parsed
        .project_root
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    init_project_in(&root, &parsed)?;
    println!(
        "touring: initialized .touring/ under {} (force={}, bare={})",
        root.display(),
        parsed.force,
        parsed.bare
    );
    Ok(())
}

/// Core scaffolder — testable variant accepting an explicit root path.
///
/// Idempotency: with `force = false`, refuses to touch an existing `.touring/`
/// directory (returns `Err`). With `force = true`, removes the old tree first.
pub fn init_project_in(root: &Path, args: &InitProjectArgs) -> Result<()> {
    let dot_touring = root.join(".touring");

    if dot_touring.exists() {
        if !args.force {
            return Err(anyhow!(
                ".touring/ already exists at {} — pass --force to overwrite",
                dot_touring.display()
            ));
        }
        std::fs::remove_dir_all(&dot_touring).map_err(|e| {
            anyhow!(
                "failed to remove existing .touring/ at {}: {e}",
                dot_touring.display()
            )
        })?;
    }

    std::fs::create_dir_all(&dot_touring)
        .map_err(|e| anyhow!("create_dir_all {}: {e}", dot_touring.display()))?;

    for sub in SUBDIRS {
        let p = dot_touring.join(sub);
        std::fs::create_dir_all(&p).map_err(|e| anyhow!("create_dir_all {}: {e}", p.display()))?;
    }

    if !args.bare {
        let toml_path = dot_touring.join("touring.toml");
        std::fs::write(&toml_path, DEFAULT_TOURING_TOML)
            .map_err(|e| anyhow!("write {}: {e}", toml_path.display()))?;
    }

    // F2 (2.1, 2026-07-24): populate `.touring/bin/` — the dir was scaffolded
    // empty since W12.1, so no project ever actually used its own toolchain.
    // F3: the channel/lock/bin state machine lives in `project_toolchain`
    // (single home for the paired concepts — F-NEW-1 lesson).
    for note in super::project_toolchain::relink_bins(&dot_touring) {
        eprintln!("touring init-project: {note}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args_for(extras: &[&str]) -> Vec<String> {
        let mut v = vec!["touring".to_string(), "init-project".to_string()];
        v.extend(extras.iter().map(|s| s.to_string()));
        v
    }

    #[test]
    fn parse_default_flags() {
        let p = InitProjectArgs::parse(&args_for(&[]));
        assert!(!p.force);
        assert!(!p.bare);
        assert!(p.project_root.is_none());
    }

    #[test]
    fn parse_force_short_and_long() {
        assert!(InitProjectArgs::parse(&args_for(&["--force"])).force);
        assert!(InitProjectArgs::parse(&args_for(&["-f"])).force);
    }

    #[test]
    fn parse_bare_and_root() {
        let p = InitProjectArgs::parse(&args_for(&["--bare", "--root=/tmp/x"]));
        assert!(p.bare);
        assert_eq!(p.project_root, Some(PathBuf::from("/tmp/x")));
    }

    // Bin-population behavior (F2 2.1) moved WITH its implementation to
    // `cli::project_toolchain` (F3 single-home refactor) — tests live there.

    #[test]
    fn init_creates_full_tree() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let args = InitProjectArgs::default();
        init_project_in(tmp.path(), &args).expect("init_project_in");
        let dot = tmp.path().join(".touring");
        assert!(dot.is_dir());
        assert!(dot.join("touring.toml").is_file());
        assert!(dot.join("data").is_dir());
        assert!(dot.join("bin").is_dir());
        assert!(dot.join("hooks").is_dir());
        let body = std::fs::read_to_string(dot.join("touring.toml")).unwrap();
        assert!(body.contains("cache_size"));
        assert!(body.contains("evolution_enabled"));
    }

    #[test]
    fn init_bare_skips_toml_body() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let args = InitProjectArgs {
            bare: true,
            ..Default::default()
        };
        init_project_in(tmp.path(), &args).expect("init_project_in bare");
        let dot = tmp.path().join(".touring");
        assert!(dot.is_dir());
        assert!(!dot.join("touring.toml").exists());
        assert!(dot.join("data").is_dir());
    }

    #[test]
    fn init_refuses_existing_without_force() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let args = InitProjectArgs::default();
        init_project_in(tmp.path(), &args).expect("first init");
        let err = init_project_in(tmp.path(), &args).expect_err("second init must fail");
        let msg = format!("{err}");
        assert!(msg.contains(".touring/ already exists"), "got: {msg}");
    }

    #[test]
    fn init_force_overwrites_existing() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let args = InitProjectArgs::default();
        init_project_in(tmp.path(), &args).expect("first init");
        // Drop a sentinel file under .touring/ — force should wipe it
        let sentinel = tmp.path().join(".touring").join("sentinel.txt");
        std::fs::write(&sentinel, b"stale").unwrap();
        assert!(sentinel.exists());

        let force_args = InitProjectArgs {
            force: true,
            ..Default::default()
        };
        init_project_in(tmp.path(), &force_args).expect("force overwrite");
        assert!(!sentinel.exists(), "sentinel must be wiped by --force");
        assert!(tmp.path().join(".touring").join("touring.toml").is_file());
    }

    #[test]
    fn init_root_flag_targets_explicit_dir() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let nested = tmp.path().join("nested/sub");
        std::fs::create_dir_all(&nested).unwrap();
        let args = InitProjectArgs {
            project_root: Some(nested.clone()),
            ..Default::default()
        };
        init_project_in(&nested, &args).expect("init nested");
        assert!(nested.join(".touring").is_dir());
        // Outer tmp must be untouched
        assert!(!tmp.path().join(".touring").exists());
    }

    /// O canal do scaffold acompanha a versão que o gerou — nunca uma literal.
    ///
    /// Antes de 04/08/2026 o `channel` era o literal "30.3.0": todo bump de
    /// versão passava, em silêncio, a cunhar projetos pinados na versão
    /// ANTERIOR. Este teste falha se alguém reintroduzir a literal.
    #[test]
    fn scaffold_pins_the_version_that_generated_it() {
        let esperado = format!("channel = \"{}\"", env!("CARGO_PKG_VERSION"));
        assert!(
            DEFAULT_TOURING_TOML.contains(&esperado),
            "o touring.toml gerado deve pinar {esperado:?} — a versão viva, \
             não uma literal congelada. Conteúdo: {DEFAULT_TOURING_TOML}"
        );
    }
}
