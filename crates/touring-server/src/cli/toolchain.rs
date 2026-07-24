//! `touring toolchain` — W12.2 (2026-05-23) — `~/.touring/` toolchain manager.
//!
//! Rustup-pattern user-level toolchain root. Manages the per-user installation
//! tree at `$TOURING_HOME` (default `~/.touring/`):
//!
//! ```text
//! ~/.touring/
//! ├── default              # text file: <version> selected as default
//! ├── config.toml          # User-layer config (W12.4 User layer in detect_layered)
//! └── toolchains/
//!     └── <version>/       # e.g. 0.30.0/
//!         ├── bin/         # binaries: touring, touring-hook, touring-daemon
//!         ├── lib/         # shared libs (placeholder for future)
//!         ├── share/       # docs, generators, plugins
//!         └── meta.toml    # { version, installed_at, source }
//! ```
//!
//! Subcommands:
//! - `touring toolchain init`             — Scaffold `~/.touring/` if absent
//! - `touring toolchain list`             — List installed versions
//! - `touring toolchain default <ver>`    — Write `<ver>` to `~/.touring/default`
//!
//! Toolchain installation (`touring toolchain install <ver>`) is W12.3 / W12.8
//! (requires download + signature verification). This module is the FOUNDATION
//! that the install subcommand will populate.

use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};

/// Resolved `~/.touring/` root, honoring `TOURING_HOME` override.
pub fn toolchain_home() -> PathBuf {
    std::env::var("TOURING_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".touring")
        })
}

/// Parsed args for `touring toolchain ...`.
#[derive(Debug, Clone)]
pub enum ToolchainCmd {
    /// `touring toolchain init [--force]`
    Init {
        /// `--force`/`-f`: remove and re-scaffold the root if it already exists.
        force: bool,
    },
    /// `touring toolchain list`
    List,
    /// `touring toolchain default <version>`
    Default {
        /// Version string to write to `~/.touring/default`.
        version: String,
    },
    /// `touring toolchain install {--from-tarball <path> | --from-source <dir> | --from-url <url>} <version>`
    /// — installs a toolchain under `~/.touring/toolchains/<version>/` from a
    /// local tarball, a built canonical-source workspace (`<dir>/target/release`),
    /// or a URL (F3; sigstore verification arrives with the release server, F5).
    Install {
        /// Version name to install under `~/.touring/toolchains/`.
        version: String,
        /// Where the toolchain content comes from (exactly one).
        source: InstallSource,
        /// `--force`/`-f`: overwrite an already-installed version.
        force: bool,
    },
    /// `touring toolchain remove <version>` — removes the toolchain dir.
    /// Refuses to remove the currently-active default (safety check).
    Remove {
        /// Version name to remove from `~/.touring/toolchains/`.
        version: String,
    },
    /// Unknown / missing subcommand → print help and exit 1
    Help,
}

/// Content source for `toolchain install` (F3: exactly one per invocation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallSource {
    /// `--from-tarball <path>` — local `.tar.gz` (W12.3 simplified original).
    Tarball(PathBuf),
    /// `--from-source <dir>` — a built canonical-source workspace: binaries
    /// are copied from `<dir>/target/release/` (the dev→toolchain bridge the
    /// per-project pilot needs).
    SourceDir(PathBuf),
    /// `--from-url <url>` — download to a temp tarball then extract. Signature
    /// verification (sigstore) arrives with the release server (Fase 5).
    Url(String),
}

impl ToolchainCmd {
    /// Parses `touring toolchain <sub> ...` argv into a `ToolchainCmd`, dispatching
    /// on `args[2]` for `init`, `list`, `default`, `install`, and `remove`/`uninstall`.
    /// Missing required arguments or unknown subcommands resolve to `ToolchainCmd::Help`.
    pub fn parse(args: &[String]) -> Self {
        // args = ["touring", "toolchain", <sub>, ...]
        let sub = args.get(2).map(|s| s.as_str()).unwrap_or("");
        match sub {
            "init" => {
                let force = args.iter().skip(3).any(|a| a == "--force" || a == "-f");
                Self::Init { force }
            }
            "list" => Self::List,
            "default" => match args.get(3) {
                Some(v) if !v.is_empty() => Self::Default { version: v.clone() },
                _ => Self::Help, // missing version
            },
            "install" => {
                // touring toolchain install {--from-tarball <p> | --from-source <d> | --from-url <u>} <version> [--force]
                let mut sources: Vec<InstallSource> = Vec::new();
                let mut version: Option<String> = None;
                let mut force = false;
                let mut i = 3;
                while i < args.len() {
                    let a = &args[i];
                    match a.as_str() {
                        "--from-tarball" | "--from-source" | "--from-url" => {
                            if let Some(v) = args.get(i + 1) {
                                sources.push(match a.as_str() {
                                    "--from-tarball" => InstallSource::Tarball(PathBuf::from(v)),
                                    "--from-source" => InstallSource::SourceDir(PathBuf::from(v)),
                                    _ => InstallSource::Url(v.clone()),
                                });
                                i += 1;
                            }
                        }
                        other if other.starts_with("--from-tarball=") => {
                            sources.push(InstallSource::Tarball(PathBuf::from(
                                other.trim_start_matches("--from-tarball="),
                            )));
                        }
                        other if other.starts_with("--from-source=") => {
                            sources.push(InstallSource::SourceDir(PathBuf::from(
                                other.trim_start_matches("--from-source="),
                            )));
                        }
                        other if other.starts_with("--from-url=") => {
                            sources.push(InstallSource::Url(
                                other.trim_start_matches("--from-url=").to_string(),
                            ));
                        }
                        "--force" | "-f" => force = true,
                        other if !other.starts_with("--") && version.is_none() => {
                            version = Some(other.to_string());
                        }
                        _ => { /* ignore */ }
                    }
                    i += 1;
                }
                // Exactly one source — zero is underspecified, two is ambiguous.
                match (sources.len(), version) {
                    (1, Some(v)) if !v.is_empty() => Self::Install {
                        version: v,
                        source: sources.remove(0),
                        force,
                    },
                    _ => Self::Help,
                }
            }
            "remove" | "uninstall" => match args.get(3) {
                Some(v) if !v.is_empty() => Self::Remove { version: v.clone() },
                _ => Self::Help,
            },
            _ => Self::Help,
        }
    }
}

/// Default `~/.touring/config.toml` body — User layer in `detect_layered()`.
const DEFAULT_USER_CONFIG: &str = "# User-level touring configuration (~/.touring/config.toml)\n\
# Read by TouringConfig::detect_layered() as the User layer (lower precedence\n\
# than .touring/touring.toml Project layer, higher than /etc/touring/config.toml\n\
# System layer).\n\
\n\
# Defaults match TouringConfig::default(); override per user preference.\n\
# cache_size = 10000\n\
# embedding_dim = 384\n\
# auto_embed = true\n";

/// Files/dirs created by `init`.
const TOOLCHAINS_DIR: &str = "toolchains";
const DEFAULT_FILE: &str = "default";
const CONFIG_FILE: &str = "config.toml";

/// CLI dispatch entry. Called from `cli::common::command_table`.
pub fn run(args: &[String]) -> Result<()> {
    let cmd = ToolchainCmd::parse(args);
    let home = toolchain_home();
    match cmd {
        ToolchainCmd::Init { force } => {
            init_toolchain_root(&home, force)?;
            println!(
                "touring toolchain: initialized {} (force={})",
                home.display(),
                force
            );
        }
        ToolchainCmd::List => {
            let versions = list_installed_toolchains(&home)?;
            if versions.is_empty() {
                println!(
                    "touring toolchain: no toolchains installed under {}",
                    home.display()
                );
                println!("Run `touring toolchain init` to scaffold the root.");
            } else {
                let default = current_default(&home).ok().flatten();
                for v in &versions {
                    let marker = if Some(v.as_str()) == default.as_deref() {
                        " (default)"
                    } else {
                        ""
                    };
                    println!("{}{}", v, marker);
                }
            }
        }
        ToolchainCmd::Default { version } => {
            set_default(&home, &version)?;
            println!("touring toolchain: default → {}", version);
        }
        ToolchainCmd::Install {
            version,
            source,
            force,
        } => {
            let described = match &source {
                InstallSource::Tarball(p) => {
                    install_toolchain_from_tarball(&home, &version, p, force)?;
                    format!("tarball {}", p.display())
                }
                InstallSource::SourceDir(d) => {
                    install_toolchain_from_source(&home, &version, d, force)?;
                    format!("source {}", d.display())
                }
                InstallSource::Url(u) => {
                    install_toolchain_from_url(&home, &version, u, force)?;
                    format!("url {u}")
                }
            };
            println!(
                "touring toolchain: installed {} (from {}, force={})",
                version, described, force
            );
        }
        ToolchainCmd::Remove { version } => {
            remove_toolchain(&home, &version)?;
            println!("touring toolchain: removed {}", version);
        }
        ToolchainCmd::Help => {
            println!("touring toolchain — Manage ~/.touring/ user-level toolchain root");
            println!();
            println!("USAGE:");
            println!("    touring toolchain init [--force]");
            println!("    touring toolchain list");
            println!("    touring toolchain default <version>");
            println!("    touring toolchain install --from-tarball <path> <version> [--force]");
            println!("    touring toolchain install --from-source <workspace> <version> [--force]");
            println!("    touring toolchain install --from-url <url> <version> [--force]");
            println!("    touring toolchain remove <version>");
            return Err(anyhow!("missing or unknown subcommand"));
        }
    }
    Ok(())
}

/// W12.3 simplified — install a toolchain from a local .tar.gz tarball.
///
/// Extracts `tarball` into `home/toolchains/<version>/` via the system `tar`
/// binary. Refuses to overwrite an existing toolchain version unless `force`
/// is true. The toolchain root must exist (run `touring toolchain init` first).
///
/// Full W12.3 (download from URL + sigstore verification) is W12.8 territory.
/// This function is the local-only foundation that the future installer wraps.
pub fn install_toolchain_from_tarball(
    home: &Path,
    version: &str,
    tarball: &Path,
    force: bool,
) -> Result<()> {
    let label = format!("local-tarball:{}", tarball.display());
    install_toolchain_from_tarball_labeled(home, version, tarball, force, &label)
}

/// [`install_toolchain_from_tarball`] with an explicit `meta.toml` source
/// label — lets `--from-url` record the URL, not its temp-file detour.
fn install_toolchain_from_tarball_labeled(
    home: &Path,
    version: &str,
    tarball: &Path,
    force: bool,
    source_label: &str,
) -> Result<()> {
    if version.trim().is_empty() {
        return Err(anyhow!("version cannot be empty"));
    }
    if !tarball.is_file() {
        return Err(anyhow!("tarball not found: {}", tarball.display()));
    }
    let dest = prepare_toolchain_dest(home, version, force)?;

    // Use system `tar` — avoids adding tar/flate2 to workspace deps. Tier-1
    // for W12 is Linux + macOS, both ship with `tar -xzf` POSIX-compliant.
    let status = std::process::Command::new("tar")
        .arg("-xzf")
        .arg(tarball)
        .arg("-C")
        .arg(&dest)
        .status()
        .map_err(|e| anyhow!("spawn tar: {e}"))?;
    if !status.success() {
        // Clean up partial extraction on failure
        let _ = std::fs::remove_dir_all(&dest);
        return Err(anyhow!(
            "tar -xzf {} -C {} failed with status {}",
            tarball.display(),
            dest.display(),
            status
        ));
    }

    // Sanity check: extracted at least one entry
    let entries = std::fs::read_dir(&dest)
        .map_err(|e| anyhow!("read_dir {}: {e}", dest.display()))?
        .count();
    if entries == 0 {
        let _ = std::fs::remove_dir_all(&dest);
        return Err(anyhow!("tarball extracted but produced no entries"));
    }

    write_toolchain_meta(&dest, version, source_label)
}

/// Optional binaries `--from-source` also picks up when the workspace built
/// them (curated: the dev dir holds unrelated binaries, so no wildcard scan).
const OPTIONAL_SOURCE_BINARIES: &[&str] = &["touring-quality"];

/// F3 (3.3) — install a toolchain from a BUILT canonical-source workspace:
/// copies `<source_dir>/target/release/{touring,touring-hook,touring-daemon}`
/// (+ known optionals) into `~/.touring/toolchains/<version>/bin/`.
///
/// Copies, not symlinks: an installed toolchain is an immutable versioned
/// snapshot — the next `cargo build` in the source must NOT silently mutate
/// it (that's the dev channel's job). All core binaries must exist; a partial
/// toolchain would break every project pinned to it.
pub fn install_toolchain_from_source(
    home: &Path,
    version: &str,
    source_dir: &Path,
    force: bool,
) -> Result<()> {
    if version.trim().is_empty() {
        return Err(anyhow!("version cannot be empty"));
    }
    let release = source_dir.join("target").join("release");
    if !release.is_dir() {
        return Err(anyhow!(
            "{} has no target/release/ — build the workspace first (cargo build --release)",
            source_dir.display()
        ));
    }
    let core = super::project_toolchain::PROJECT_BINARIES;
    let missing: Vec<&str> = core
        .iter()
        .filter(|b| !release.join(b).is_file())
        .copied()
        .collect();
    if !missing.is_empty() {
        return Err(anyhow!(
            "core binaries missing from {}: {} — a partial toolchain would break pinned projects",
            release.display(),
            missing.join(", ")
        ));
    }
    let dest = prepare_toolchain_dest(home, version, force)?;
    let bin = dest.join("bin");
    std::fs::create_dir_all(&bin).map_err(|e| anyhow!("create_dir_all {}: {e}", bin.display()))?;
    let optionals = OPTIONAL_SOURCE_BINARIES
        .iter()
        .filter(|b| release.join(b).is_file());
    for name in core.iter().chain(optionals) {
        let from = release.join(name);
        let to = bin.join(name);
        // fs::copy preserves the executable bit on Unix.
        std::fs::copy(&from, &to)
            .map_err(|e| anyhow!("copy {} -> {}: {e}", from.display(), to.display()))?;
    }
    write_toolchain_meta(&dest, version, &format!("local-source:{}", source_dir.display()))
}

/// F3 (3.3) — install a toolchain from a URL: download to a temp tarball via
/// the system `curl`, then extract. Signature verification (sigstore) arrives
/// with the release server (Fase 5) — until then the URL itself is the trust
/// anchor, recorded in `meta.toml`.
pub fn install_toolchain_from_url(
    home: &Path,
    version: &str,
    url: &str,
    force: bool,
) -> Result<()> {
    if version.trim().is_empty() {
        return Err(anyhow!("version cannot be empty"));
    }
    let tmp = std::env::temp_dir().join(format!(
        "touring-toolchain-{version}-{}.tar.gz",
        std::process::id()
    ));
    let status = std::process::Command::new("curl")
        .args(["-fsSL", "-o"])
        .arg(&tmp)
        .arg(url)
        .status()
        .map_err(|e| anyhow!("spawn curl: {e}"))?;
    if !status.success() {
        let _ = std::fs::remove_file(&tmp);
        return Err(anyhow!("curl -fsSL {url} failed with status {status}"));
    }
    let result = install_toolchain_from_tarball_labeled(
        home,
        version,
        &tmp,
        force,
        &format!("url:{url}"),
    );
    let _ = std::fs::remove_file(&tmp);
    result
}

/// Shared install plumbing: validate the root, resolve the destination dir,
/// honor `--force` overwrite semantics.
fn prepare_toolchain_dest(home: &Path, version: &str, force: bool) -> Result<PathBuf> {
    let toolchains_dir = home.join(TOOLCHAINS_DIR);
    if !toolchains_dir.is_dir() {
        return Err(anyhow!(
            "{} does not exist. Run `touring toolchain init` first.",
            toolchains_dir.display()
        ));
    }
    let dest = toolchains_dir.join(version);
    if dest.exists() {
        if !force {
            return Err(anyhow!(
                "toolchain {} already installed (pass --force to overwrite)",
                version
            ));
        }
        std::fs::remove_dir_all(&dest)
            .map_err(|e| anyhow!("remove existing {}: {e}", dest.display()))?;
    }
    std::fs::create_dir_all(&dest)
        .map_err(|e| anyhow!("create_dir_all {}: {e}", dest.display()))?;
    Ok(dest)
}

/// Write `meta.toml` at the toolchain root for future inspection.
fn write_toolchain_meta(dest: &Path, version: &str, source_label: &str) -> Result<()> {
    let meta = format!(
        "version = \"{}\"\ninstalled_at = {}\nsource = \"{}\"\n",
        version,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        source_label,
    );
    std::fs::write(dest.join("meta.toml"), meta).map_err(|e| anyhow!("write meta.toml: {e}"))
}

/// Remove an installed toolchain. Refuses to remove the currently-active
/// default (safety check — user must `touring toolchain default <other>`
/// first, or rerun this with the default-clearing follow-up).
pub fn remove_toolchain(home: &Path, version: &str) -> Result<()> {
    if version.trim().is_empty() {
        return Err(anyhow!("version cannot be empty"));
    }
    let toolchain = home.join(TOOLCHAINS_DIR).join(version);
    if !toolchain.exists() {
        return Err(anyhow!(
            "toolchain {} not installed (nothing to remove)",
            version
        ));
    }
    // Safety: refuse to remove the active default
    let default = current_default(home)?;
    if default.as_deref() == Some(version) {
        return Err(anyhow!(
            "refuse to remove the currently-active default toolchain {}. \
             Run `touring toolchain default <other>` first.",
            version
        ));
    }
    std::fs::remove_dir_all(&toolchain)
        .map_err(|e| anyhow!("remove_dir_all {}: {e}", toolchain.display()))?;
    Ok(())
}

/// Scaffold the toolchain root at `home`.
///
/// Creates `home/{toolchains/, config.toml}`. If `home` already exists and
/// `force=false`, returns an error. If `force=true`, removes `home` first.
pub fn init_toolchain_root(home: &Path, force: bool) -> Result<()> {
    if home.exists() {
        if !force {
            return Err(anyhow!(
                "{} already exists — pass --force to overwrite",
                home.display()
            ));
        }
        std::fs::remove_dir_all(home)
            .map_err(|e| anyhow!("remove_dir_all {}: {e}", home.display()))?;
    }
    std::fs::create_dir_all(home.join(TOOLCHAINS_DIR)).map_err(|e| {
        anyhow!(
            "create_dir_all {}: {e}",
            home.join(TOOLCHAINS_DIR).display()
        )
    })?;
    let cfg = home.join(CONFIG_FILE);
    std::fs::write(&cfg, DEFAULT_USER_CONFIG)
        .map_err(|e| anyhow!("write {}: {e}", cfg.display()))?;
    Ok(())
}

/// List installed toolchains by enumerating subdirs of `home/toolchains/`.
///
/// Each subdir is treated as a version name. Returns sorted list (lexicographic).
/// Missing `home/toolchains/` returns `Ok(empty)`.
pub fn list_installed_toolchains(home: &Path) -> Result<Vec<String>> {
    let dir = home.join(TOOLCHAINS_DIR);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut versions: Vec<String> = std::fs::read_dir(&dir)
        .map_err(|e| anyhow!("read_dir {}: {e}", dir.display()))?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if entry.file_type().ok()?.is_dir() {
                entry.file_name().into_string().ok()
            } else {
                None
            }
        })
        .collect();
    versions.sort();
    Ok(versions)
}

/// Read the current default version, if any. `None` if `default` file is
/// absent or empty.
pub fn current_default(home: &Path) -> Result<Option<String>> {
    let path = home.join(DEFAULT_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let content =
        std::fs::read_to_string(&path).map_err(|e| anyhow!("read {}: {e}", path.display()))?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed.to_string()))
    }
}

/// Set the default version (writes `home/default`).
///
/// Refuses if `version` does NOT correspond to an installed toolchain (i.e.,
/// `home/toolchains/<version>/` doesn't exist). This catches typos before they
/// cause the shim to silently fall through to the global fallback.
pub fn set_default(home: &Path, version: &str) -> Result<()> {
    if version.trim().is_empty() {
        return Err(anyhow!("version cannot be empty"));
    }
    let installed = home.join(TOOLCHAINS_DIR).join(version);
    if !installed.is_dir() {
        return Err(anyhow!(
            "toolchain {} is not installed (expected {}). Run `touring toolchain list` to see available.",
            version,
            installed.display()
        ));
    }
    std::fs::write(home.join(DEFAULT_FILE), format!("{}\n", version))
        .map_err(|e| anyhow!("write default: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(extras: &[&str]) -> Vec<String> {
        let mut v = vec!["touring".to_string(), "toolchain".to_string()];
        v.extend(extras.iter().map(|s| s.to_string()));
        v
    }

    #[test]
    fn parse_init_force() {
        assert!(matches!(
            ToolchainCmd::parse(&args(&["init", "--force"])),
            ToolchainCmd::Init { force: true }
        ));
        assert!(matches!(
            ToolchainCmd::parse(&args(&["init"])),
            ToolchainCmd::Init { force: false }
        ));
    }

    #[test]
    fn parse_list() {
        assert!(matches!(
            ToolchainCmd::parse(&args(&["list"])),
            ToolchainCmd::List
        ));
    }

    #[test]
    fn parse_default_with_version() {
        if let ToolchainCmd::Default { version } =
            ToolchainCmd::parse(&args(&["default", "0.30.0"]))
        {
            assert_eq!(version, "0.30.0");
        } else {
            panic!("expected ToolchainCmd::Default");
        }
    }

    #[test]
    fn parse_default_missing_version_falls_to_help() {
        assert!(matches!(
            ToolchainCmd::parse(&args(&["default"])),
            ToolchainCmd::Help
        ));
    }

    #[test]
    fn parse_unknown_subcommand_is_help() {
        assert!(matches!(
            ToolchainCmd::parse(&args(&["fizzbuzz"])),
            ToolchainCmd::Help
        ));
        assert!(matches!(
            ToolchainCmd::parse(&args(&[])),
            ToolchainCmd::Help
        ));
    }

    #[test]
    fn init_creates_root_with_config_and_toolchains_dir() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let home = tmp.path().join("dot-touring");
        init_toolchain_root(&home, false).expect("init");
        assert!(home.is_dir());
        assert!(home.join("toolchains").is_dir());
        let cfg = home.join("config.toml");
        assert!(cfg.is_file());
        let body = std::fs::read_to_string(cfg).unwrap();
        assert!(body.contains("User-level touring configuration"));
    }

    #[test]
    fn init_refuses_existing_without_force() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let home = tmp.path().join("dot-touring");
        init_toolchain_root(&home, false).expect("first");
        let err = init_toolchain_root(&home, false).expect_err("second must fail");
        assert!(format!("{err}").contains("already exists"));
    }

    #[test]
    fn init_force_overwrites_existing() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let home = tmp.path().join("dot-touring");
        init_toolchain_root(&home, false).expect("first");
        let sentinel = home.join("sentinel.txt");
        std::fs::write(&sentinel, b"stale").unwrap();
        assert!(sentinel.exists());
        init_toolchain_root(&home, true).expect("force");
        assert!(!sentinel.exists());
        assert!(home.join("config.toml").is_file());
    }

    #[test]
    fn list_empty_for_fresh_init() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let home = tmp.path().join("dot-touring");
        init_toolchain_root(&home, false).expect("init");
        let v = list_installed_toolchains(&home).expect("list");
        assert!(v.is_empty(), "expected empty, got {:?}", v);
    }

    #[test]
    fn list_returns_subdirs_sorted() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let home = tmp.path().join("dot-touring");
        init_toolchain_root(&home, false).expect("init");
        for v in &["0.31.0", "0.29.0", "0.30.0"] {
            std::fs::create_dir_all(home.join("toolchains").join(v).join("bin")).unwrap();
        }
        let got = list_installed_toolchains(&home).expect("list");
        assert_eq!(got, vec!["0.29.0", "0.30.0", "0.31.0"]);
    }

    #[test]
    fn list_returns_empty_when_toolchains_dir_missing() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        // Don't init — directory doesn't exist
        let home = tmp.path().join("does-not-exist");
        let v = list_installed_toolchains(&home).expect("list");
        assert!(v.is_empty());
    }

    #[test]
    fn current_default_none_initially() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let home = tmp.path().join("dot-touring");
        init_toolchain_root(&home, false).expect("init");
        let d = current_default(&home).expect("read default");
        assert!(d.is_none());
    }

    #[test]
    fn set_default_writes_file_after_install() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let home = tmp.path().join("dot-touring");
        init_toolchain_root(&home, false).expect("init");
        // Simulate an installed toolchain
        std::fs::create_dir_all(home.join("toolchains").join("0.30.0").join("bin")).unwrap();
        set_default(&home, "0.30.0").expect("set default");
        let d = current_default(&home).expect("read default").expect("Some");
        assert_eq!(d, "0.30.0");
    }

    #[test]
    fn set_default_refuses_uninstalled_version() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let home = tmp.path().join("dot-touring");
        init_toolchain_root(&home, false).expect("init");
        let err = set_default(&home, "0.99.99").expect_err("must fail — not installed");
        assert!(format!("{err}").contains("not installed"));
    }

    #[test]
    fn set_default_refuses_empty_version() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let home = tmp.path().join("dot-touring");
        init_toolchain_root(&home, false).expect("init");
        let err = set_default(&home, "  ").expect_err("must fail — empty");
        assert!(format!("{err}").contains("empty"));
    }

    #[test]
    fn toolchain_home_honors_env_override() {
        let prev = std::env::var("TOURING_HOME").ok();
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("TOURING_HOME", "/custom/path") };
        assert_eq!(toolchain_home(), PathBuf::from("/custom/path"));
        match prev {
            // TODO: Audit that the environment access only happens in single-threaded code.
            Some(v) => unsafe { std::env::set_var("TOURING_HOME", v) },
            // TODO: Audit that the environment access only happens in single-threaded code.
            None => unsafe { std::env::remove_var("TOURING_HOME") },
        }
    }

    // ── W12.3 simplified — install / remove tests ─────────────────────────

    fn make_sample_tarball() -> tempfile::NamedTempFile {
        // Build a small .tar.gz on the fly via the system `tar` binary.
        let src = tempfile::tempdir().expect("src tmpdir");
        std::fs::create_dir_all(src.path().join("bin")).unwrap();
        std::fs::write(
            src.path().join("bin/touring"),
            b"#!/bin/sh\necho fake-touring",
        )
        .unwrap();
        std::fs::write(src.path().join("share.txt"), b"shared resource").unwrap();
        let tarball = tempfile::Builder::new()
            .prefix("touring-tc-")
            .suffix(".tar.gz")
            .tempfile()
            .expect("tarball NamedTempFile");
        let status = std::process::Command::new("tar")
            .arg("-czf")
            .arg(tarball.path())
            .arg("-C")
            .arg(src.path())
            .arg(".")
            .status()
            .expect("spawn tar -czf");
        assert!(status.success(), "tar -czf failed: {status:?}");
        tarball
    }

    #[test]
    fn parse_install_with_version_and_tarball() {
        let tar = "/tmp/touring-0.30.0.tar.gz";
        let v = "0.30.0";
        // touring toolchain install --from-tarball /tmp/touring-0.30.0.tar.gz 0.30.0
        match ToolchainCmd::parse(&args(&["install", "--from-tarball", tar, v])) {
            ToolchainCmd::Install {
                version,
                source,
                force,
            } => {
                assert_eq!(version, v);
                assert_eq!(source, InstallSource::Tarball(PathBuf::from(tar)));
                assert!(!force);
            }
            other => panic!("expected Install, got {:?}", other),
        }
    }

    #[test]
    fn parse_install_with_equals_form_and_force() {
        let tar = "/tmp/t.tgz";
        let v = "0.30.0";
        match ToolchainCmd::parse(&args(&[
            "install",
            &format!("--from-tarball={tar}"),
            v,
            "--force",
        ])) {
            ToolchainCmd::Install {
                version,
                source,
                force,
            } => {
                assert_eq!(version, v);
                assert_eq!(source, InstallSource::Tarball(PathBuf::from(tar)));
                assert!(force);
            }
            other => panic!("expected Install, got {:?}", other),
        }
    }

    #[test]
    fn parse_install_from_source_and_url() {
        match ToolchainCmd::parse(&args(&["install", "--from-source", "/src/ws", "vdev"])) {
            ToolchainCmd::Install { version, source, .. } => {
                assert_eq!(version, "vdev");
                assert_eq!(source, InstallSource::SourceDir(PathBuf::from("/src/ws")));
            }
            other => panic!("expected Install, got {:?}", other),
        }
        match ToolchainCmd::parse(&args(&[
            "install",
            "--from-url=https://example.com/t.tar.gz",
            "v1",
        ])) {
            ToolchainCmd::Install { version, source, .. } => {
                assert_eq!(version, "v1");
                assert_eq!(
                    source,
                    InstallSource::Url("https://example.com/t.tar.gz".into())
                );
            }
            other => panic!("expected Install, got {:?}", other),
        }
    }

    #[test]
    fn parse_install_missing_tarball_falls_to_help() {
        assert!(matches!(
            ToolchainCmd::parse(&args(&["install", "0.30.0"])),
            ToolchainCmd::Help
        ));
    }

    #[test]
    fn parse_install_two_sources_is_ambiguous_help() {
        assert!(matches!(
            ToolchainCmd::parse(&args(&[
                "install",
                "--from-tarball",
                "/t.tgz",
                "--from-source",
                "/src",
                "v1"
            ])),
            ToolchainCmd::Help
        ));
    }

    #[test]
    fn parse_remove_and_uninstall_alias() {
        match ToolchainCmd::parse(&args(&["remove", "0.30.0"])) {
            ToolchainCmd::Remove { version } => assert_eq!(version, "0.30.0"),
            other => panic!("expected Remove, got {:?}", other),
        }
        match ToolchainCmd::parse(&args(&["uninstall", "0.30.0"])) {
            ToolchainCmd::Remove { version } => assert_eq!(version, "0.30.0"),
            other => panic!("expected Remove (uninstall alias), got {:?}", other),
        }
    }

    #[test]
    fn install_extracts_tarball_into_toolchain_dir() {
        let tmp = tempfile::tempdir().expect("home tmpdir");
        let home = tmp.path().join("dot-touring");
        init_toolchain_root(&home, false).expect("init");
        let tarball = make_sample_tarball();

        install_toolchain_from_tarball(&home, "0.30.0", tarball.path(), false).expect("install");

        let extracted = home.join("toolchains").join("0.30.0");
        assert!(extracted.is_dir());
        assert!(extracted.join("bin/touring").is_file());
        assert!(extracted.join("share.txt").is_file());
        assert!(extracted.join("meta.toml").is_file());
        let meta = std::fs::read_to_string(extracted.join("meta.toml")).unwrap();
        assert!(meta.contains("version = \"0.30.0\""));
        assert!(meta.contains("source = \"local-tarball:"));

        // Sanity: list now sees this version
        let listed = list_installed_toolchains(&home).expect("list");
        assert!(listed.contains(&"0.30.0".to_string()));
    }

    #[test]
    fn install_refuses_existing_without_force() {
        let tmp = tempfile::tempdir().expect("home tmpdir");
        let home = tmp.path().join("dot-touring");
        init_toolchain_root(&home, false).expect("init");
        let tarball = make_sample_tarball();
        install_toolchain_from_tarball(&home, "0.30.0", tarball.path(), false).expect("first");

        let err = install_toolchain_from_tarball(&home, "0.30.0", tarball.path(), false)
            .expect_err("second must fail without force");
        assert!(format!("{err}").contains("already installed"));
    }

    #[test]
    fn install_force_overwrites_existing() {
        let tmp = tempfile::tempdir().expect("home tmpdir");
        let home = tmp.path().join("dot-touring");
        init_toolchain_root(&home, false).expect("init");
        let tarball = make_sample_tarball();
        install_toolchain_from_tarball(&home, "0.30.0", tarball.path(), false).expect("first");
        // Drop a sentinel inside — force wipe should remove it
        let sentinel = home.join("toolchains/0.30.0/sentinel.txt");
        std::fs::write(&sentinel, b"stale").unwrap();
        assert!(sentinel.exists());

        install_toolchain_from_tarball(&home, "0.30.0", tarball.path(), true).expect("force");
        assert!(!sentinel.exists());
        assert!(home.join("toolchains/0.30.0/bin/touring").is_file());
    }

    #[test]
    fn install_errors_when_root_missing() {
        let tmp = tempfile::tempdir().expect("home tmpdir");
        // Don't init — toolchains/ doesn't exist
        let home = tmp.path().join("dot-touring");
        let tarball = make_sample_tarball();
        let err = install_toolchain_from_tarball(&home, "0.30.0", tarball.path(), false)
            .expect_err("must fail — no root");
        let msg = format!("{err}");
        assert!(
            msg.contains("does not exist") || msg.contains("Run `touring toolchain init`"),
            "got: {msg}"
        );
    }

    #[test]
    fn install_errors_when_tarball_missing() {
        let tmp = tempfile::tempdir().expect("home tmpdir");
        let home = tmp.path().join("dot-touring");
        init_toolchain_root(&home, false).expect("init");
        let err = install_toolchain_from_tarball(
            &home,
            "0.30.0",
            std::path::Path::new("/tmp/nonexistent-touring-tarball-xyz.tar.gz"),
            false,
        )
        .expect_err("must fail — no tarball");
        assert!(format!("{err}").contains("tarball not found"));
    }

    #[test]
    fn remove_extracts_toolchain_dir() {
        let tmp = tempfile::tempdir().expect("home tmpdir");
        let home = tmp.path().join("dot-touring");
        init_toolchain_root(&home, false).expect("init");
        let tarball = make_sample_tarball();
        install_toolchain_from_tarball(&home, "0.30.0", tarball.path(), false).expect("install");
        assert!(home.join("toolchains/0.30.0").is_dir());

        remove_toolchain(&home, "0.30.0").expect("remove");
        assert!(!home.join("toolchains/0.30.0").exists());
    }

    #[test]
    fn remove_refuses_active_default() {
        let tmp = tempfile::tempdir().expect("home tmpdir");
        let home = tmp.path().join("dot-touring");
        init_toolchain_root(&home, false).expect("init");
        let tarball = make_sample_tarball();
        install_toolchain_from_tarball(&home, "0.30.0", tarball.path(), false).expect("install");
        set_default(&home, "0.30.0").expect("set default");

        let err = remove_toolchain(&home, "0.30.0").expect_err("must refuse: active default");
        let msg = format!("{err}");
        assert!(msg.contains("currently-active default"), "got: {msg}");
        // Toolchain still there
        assert!(home.join("toolchains/0.30.0").is_dir());
    }

    #[test]
    fn remove_errors_on_uninstalled() {
        let tmp = tempfile::tempdir().expect("home tmpdir");
        let home = tmp.path().join("dot-touring");
        init_toolchain_root(&home, false).expect("init");
        let err = remove_toolchain(&home, "0.99.99").expect_err("must fail: not installed");
        assert!(format!("{err}").contains("not installed"));
    }

    // ── F3 (3.3) — install from source / url ──────────────────────────────

    fn make_fake_source(core_only: bool) -> tempfile::TempDir {
        let src = tempfile::tempdir().expect("src tmpdir");
        let release = src.path().join("target/release");
        std::fs::create_dir_all(&release).unwrap();
        let mut names = vec!["touring", "touring-hook"];
        if !core_only {
            names.push("touring-daemon");
            names.push("touring-quality");
        }
        for name in names {
            let p = release.join(name);
            std::fs::write(&p, format!("#!/bin/sh\necho src-{name}\n")).unwrap();
            let mut perm = std::fs::metadata(&p).unwrap().permissions();
            std::os::unix::fs::PermissionsExt::set_mode(&mut perm, 0o755);
            std::fs::set_permissions(&p, perm).unwrap();
        }
        src
    }

    #[test]
    fn install_from_source_copies_core_and_known_optionals() {
        let tmp = tempfile::tempdir().expect("home tmpdir");
        let home = tmp.path().join("dot-touring");
        init_toolchain_root(&home, false).expect("init");
        let src = make_fake_source(false);

        install_toolchain_from_source(&home, "vdev", src.path(), false).expect("install");

        let bin = home.join("toolchains/vdev/bin");
        for name in ["touring", "touring-hook", "touring-daemon", "touring-quality"] {
            assert!(bin.join(name).is_file(), "missing {name}");
        }
        // Copies, not symlinks — an installed toolchain is an immutable snapshot.
        assert!(!bin.join("touring").is_symlink());
        // Executable bit preserved by fs::copy.
        let mode = std::os::unix::fs::PermissionsExt::mode(
            &std::fs::metadata(bin.join("touring")).unwrap().permissions(),
        );
        assert_ne!(mode & 0o111, 0, "executable bit must survive the copy");
        let meta = std::fs::read_to_string(home.join("toolchains/vdev/meta.toml")).unwrap();
        assert!(meta.contains("local-source:"), "meta: {meta}");
    }

    #[test]
    fn install_from_source_missing_core_binary_fails_loud() {
        let tmp = tempfile::tempdir().expect("home tmpdir");
        let home = tmp.path().join("dot-touring");
        init_toolchain_root(&home, false).expect("init");
        let src = make_fake_source(true); // no touring-daemon

        let err = install_toolchain_from_source(&home, "vdev", src.path(), false)
            .expect_err("partial toolchain must be refused");
        let msg = format!("{err}");
        assert!(msg.contains("touring-daemon"), "must name the missing bin: {msg}");
        assert!(
            !home.join("toolchains/vdev").exists() || !home.join("toolchains/vdev/bin/touring").exists(),
            "no partial install may be left behind as complete"
        );
    }

    #[test]
    fn install_from_url_file_scheme_roundtrip() {
        // file:// exercises the full curl→tarball→extract path without a
        // network dependency (deterministic in CI).
        let tmp = tempfile::tempdir().expect("home tmpdir");
        let home = tmp.path().join("dot-touring");
        init_toolchain_root(&home, false).expect("init");
        let tarball = make_sample_tarball();
        let url = format!("file://{}", tarball.path().display());

        install_toolchain_from_url(&home, "vurl", &url, false).expect("install from url");

        assert!(home.join("toolchains/vurl/bin/touring").is_file());
        let meta = std::fs::read_to_string(home.join("toolchains/vurl/meta.toml")).unwrap();
        assert!(meta.contains(&format!("url:{url}")), "meta: {meta}");
    }
}
