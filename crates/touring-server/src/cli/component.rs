//! `touring component` — W12.3 / Pln2 F3 (2026-07-24) — optional per-project
//! components on top of the core toolchain binaries.
//!
//! A *component* is any extra binary a toolchain offers beyond the core three
//! (`touring`, `touring-hook`, `touring-daemon`) — e.g. `touring-quality`.
//! Components are linked into `<project>/.touring/bin/` from the project's
//! ACTIVE channel (same lock > pin resolution as `touring update`, single
//! source of truth in `cli::project_toolchain`).
//!
//! ```text
//! touring component list   [--project <root>] [--json]
//! touring component add    <name> [--project <root>]
//! touring component remove <name> [--project <root>]
//! ```
//!
//! Core binaries are listed but never removable here (potentialize, never
//! reduce — REGRA #0): their lifecycle belongs to `touring update`.

use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};

use super::project_toolchain::{
    DEV_CHANNEL, PROJECT_BINARIES, resolve_active_channel, resolve_binary_target,
};

/// Parsed args for `touring component ...`.
#[derive(Debug, Clone)]
pub enum ComponentCmd {
    /// `component list`
    List {
        /// Explicit project root (default: cwd walk-up).
        project: Option<PathBuf>,
        /// Machine-readable output.
        json: bool,
    },
    /// `component add <name>`
    Add {
        /// Component (binary) name to link.
        name: String,
        /// Explicit project root (default: cwd walk-up).
        project: Option<PathBuf>,
    },
    /// `component remove <name>`
    Remove {
        /// Component (binary) name to unlink.
        name: String,
        /// Explicit project root (default: cwd walk-up).
        project: Option<PathBuf>,
    },
    /// `--help` or missing/unknown subcommand.
    Help {
        /// True when help was explicitly requested (exit 0) vs a parse
        /// failure (exit 1).
        requested: bool,
    },
}

impl ComponentCmd {
    /// Parse `touring component <sub> ...` argv (manual style — crate CLI
    /// convention).
    pub fn parse(args: &[String]) -> Self {
        if args.iter().skip(2).any(|a| a == "--help" || a == "-h") {
            return Self::Help { requested: true };
        }
        let sub = args.get(2).map(|s| s.as_str()).unwrap_or("");
        let project = parse_project_flag(args);
        let json = args.iter().skip(3).any(|a| a == "--json" || a == "-j");
        let name = args.iter().skip(3).find(|a| !a.starts_with('-')).cloned();
        match (sub, name) {
            ("list", _) => Self::List { project, json },
            ("add", Some(name)) => Self::Add { name, project },
            ("remove", Some(name)) => Self::Remove { name, project },
            _ => Self::Help { requested: false },
        }
    }
}

/// Extract `--project <root>` / `--project=<root>` from argv.
fn parse_project_flag(args: &[String]) -> Option<PathBuf> {
    let mut i = 3;
    while i < args.len() {
        let a = args[i].as_str();
        if a == "--project" {
            return args.get(i + 1).map(PathBuf::from);
        }
        if let Some(rest) = a.strip_prefix("--project=") {
            return Some(PathBuf::from(rest));
        }
        i += 1;
    }
    None
}

const USAGE: &str = "touring component — Manage optional per-project components (W12.3 / Pln2 F3)

USAGE:
    touring component list   [--project <root>] [--json]
    touring component add    <name> [--project <root>]
    touring component remove <name> [--project <root>]

BEHAVIOR:
    Components are extra binaries offered by the project's ACTIVE toolchain
    channel (lock > touring.toml pin) beyond the core three. `add` links
    .touring/bin/<name> from the active channel; `remove` unlinks it. Core
    binaries (touring, touring-hook, touring-daemon) are managed by
    `touring update` and are never removable here.";

/// CLI dispatch entry. Called from `cli::command_table`.
pub fn run(args: &[String]) -> Result<()> {
    let cmd = ComponentCmd::parse(args);
    let home = std::env::var("HOME").unwrap_or_default();
    let touring_home = std::env::var("TOURING_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(&home).join(".touring"));
    let dev_bin_dir = Path::new(&home).join(".local").join("bin");

    match cmd {
        ComponentCmd::List { project, json } => {
            let dot = resolve_dot_touring(project.as_deref())?;
            let rows = list_components(&dot, &touring_home, &dev_bin_dir);
            if json {
                let out = serde_json::json!({
                    "project": dot.parent().map(|p| p.display().to_string()),
                    "active_channel": resolve_active_channel(&dot),
                    "components": rows.iter().map(|r| serde_json::json!({
                        "name": r.name, "core": r.core, "status": r.status,
                    })).collect::<Vec<_>>(),
                });
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                let channel = resolve_active_channel(&dot);
                println!(
                    "components of {} (active channel: {})",
                    dot.display(),
                    channel.as_deref().unwrap_or("<unpinned/dev>")
                );
                for r in rows {
                    let kind = if r.core { "core" } else { "optional" };
                    println!("  {:<24} {:<8} {}", r.name, kind, r.status);
                }
            }
            Ok(())
        }
        ComponentCmd::Add { name, project } => {
            let dot = resolve_dot_touring(project.as_deref())?;
            add_component(&dot, &name, &touring_home, &dev_bin_dir)?;
            println!("touring component: added {name}");
            Ok(())
        }
        ComponentCmd::Remove { name, project } => {
            let dot = resolve_dot_touring(project.as_deref())?;
            remove_component(&dot, &name)?;
            println!("touring component: removed {name}");
            Ok(())
        }
        ComponentCmd::Help { requested } => {
            println!("{USAGE}");
            if requested {
                Ok(())
            } else {
                Err(anyhow!("missing or unknown subcommand"))
            }
        }
    }
}

/// One row of `component list`.
#[derive(Debug)]
pub struct ComponentRow {
    /// Binary name.
    pub name: String,
    /// True for the core three (never removable here).
    pub core: bool,
    /// `linked -> <target>` | `available (not linked)` | `missing`.
    pub status: String,
}

/// Resolve the project's `.touring/` dir from an explicit root or cwd walk-up.
fn resolve_dot_touring(project: Option<&Path>) -> Result<PathBuf> {
    let root = match project {
        Some(r) => r.to_path_buf(),
        None => {
            let cwd = std::env::current_dir()?;
            walk_up(&cwd).ok_or_else(|| {
                anyhow!(
                    "no .touring/ found walking up from {} — pass --project <root> or run `touring init-project`",
                    cwd.display()
                )
            })?
        }
    };
    let dot = root.join(".touring");
    if !dot.is_dir() {
        return Err(anyhow!(
            "{} has no .touring/ — run `touring init-project` first",
            root.display()
        ));
    }
    Ok(dot)
}

fn walk_up(start: &Path) -> Option<PathBuf> {
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

/// Enumerate core + optional components with their per-project status.
///
/// Optional discovery: every non-core binary in the active toolchain's `bin/`
/// plus `touring-*`-named binaries from the dev channel dir (curated by
/// prefix — the dev dir holds unrelated user binaries too).
pub(crate) fn list_components(
    dot_touring: &Path,
    touring_home: &Path,
    dev_bin_dir: &Path,
) -> Vec<ComponentRow> {
    let channel = resolve_active_channel(dot_touring);
    let mut optional: Vec<String> = Vec::new();
    if let Some(c) = channel.as_deref().filter(|c| *c != DEV_CHANNEL) {
        let tc_bin = touring_home.join("toolchains").join(c).join("bin");
        if let Ok(entries) = std::fs::read_dir(&tc_bin) {
            for e in entries.flatten() {
                if let Ok(name) = e.file_name().into_string()
                    && !PROJECT_BINARIES.contains(&name.as_str())
                {
                    optional.push(name);
                }
            }
        }
    }
    if let Ok(entries) = std::fs::read_dir(dev_bin_dir) {
        for e in entries.flatten() {
            if let Ok(name) = e.file_name().into_string()
                && name.starts_with("touring")
                && !PROJECT_BINARIES.contains(&name.as_str())
                && !optional.contains(&name)
            {
                optional.push(name);
            }
        }
    }
    optional.sort();

    PROJECT_BINARIES
        .iter()
        .map(|n| (n.to_string(), true))
        .chain(optional.into_iter().map(|n| (n, false)))
        .map(|(name, core)| {
            let link = dot_touring.join("bin").join(&name);
            let status = match std::fs::read_link(&link) {
                Ok(target) => format!("linked -> {}", target.display()),
                Err(_) if link.exists() => "present (not a symlink)".to_string(),
                Err(_) => {
                    let available =
                        resolve_binary_target(&name, channel.as_deref(), touring_home, dev_bin_dir)
                            .is_some();
                    if available {
                        "available (not linked)".to_string()
                    } else {
                        "missing".to_string()
                    }
                }
            };
            ComponentRow { name, core, status }
        })
        .collect()
}

/// Link component `name` into `.touring/bin/` from the active channel.
pub(crate) fn add_component(
    dot_touring: &Path,
    name: &str,
    touring_home: &Path,
    dev_bin_dir: &Path,
) -> Result<()> {
    if name.trim().is_empty() {
        return Err(anyhow!("component name cannot be empty"));
    }
    if PROJECT_BINARIES.contains(&name) {
        return Err(anyhow!(
            "{name} is a core binary — managed by `touring update`, not `component add`"
        ));
    }
    let channel = resolve_active_channel(dot_touring);
    let target = resolve_binary_target(name, channel.as_deref(), touring_home, dev_bin_dir)
        .ok_or_else(|| {
            anyhow!(
                "component {name} not found in toolchain {:?} nor dev channel {} — is it installed?",
                channel.as_deref().unwrap_or("<unpinned>"),
                dev_bin_dir.display()
            )
        })?;
    let bin_dir = dot_touring.join("bin");
    std::fs::create_dir_all(&bin_dir)
        .map_err(|e| anyhow!("create_dir_all {}: {e}", bin_dir.display()))?;
    let link = bin_dir.join(name);
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(&target, &link)
        .map_err(|e| anyhow!("symlink {} -> {}: {e}", link.display(), target.display()))?;
    Ok(())
}

/// Unlink component `name` from `.touring/bin/`. Refuses core binaries.
pub(crate) fn remove_component(dot_touring: &Path, name: &str) -> Result<()> {
    if PROJECT_BINARIES.contains(&name) {
        return Err(anyhow!(
            "{name} is a core binary — refuse to remove (its lifecycle belongs to `touring update`)"
        ));
    }
    let link = dot_touring.join("bin").join(name);
    if !link.is_symlink() && !link.exists() {
        return Err(anyhow!("component {name} is not linked in this project"));
    }
    std::fs::remove_file(&link).map_err(|e| anyhow!("remove {}: {e}", link.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args_for(extras: &[&str]) -> Vec<String> {
        let mut v = vec!["touring".to_string(), "component".to_string()];
        v.extend(extras.iter().map(|s| s.to_string()));
        v
    }

    fn fake_bin(dir: &Path, name: &str) -> PathBuf {
        std::fs::create_dir_all(dir).expect("mkdir");
        let p = dir.join(name);
        std::fs::write(&p, "#!/bin/sh\nexit 0\n").expect("write");
        let mut perm = std::fs::metadata(&p).expect("meta").permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perm, 0o755);
        std::fs::set_permissions(&p, perm).expect("chmod");
        p
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

    #[test]
    fn parse_subcommands() {
        assert!(matches!(
            ComponentCmd::parse(&args_for(&["list", "--json"])),
            ComponentCmd::List { json: true, .. }
        ));
        match ComponentCmd::parse(&args_for(&[
            "add",
            "touring-quality",
            "--project",
            "/tmp/x",
        ])) {
            ComponentCmd::Add { name, project } => {
                assert_eq!(name, "touring-quality");
                assert_eq!(project, Some(PathBuf::from("/tmp/x")));
            }
            other => panic!("expected Add, got {other:?}"),
        }
        assert!(matches!(
            ComponentCmd::parse(&args_for(&["remove", "x"])),
            ComponentCmd::Remove { .. }
        ));
        assert!(matches!(
            ComponentCmd::parse(&args_for(&["--help"])),
            ComponentCmd::Help { requested: true }
        ));
        assert!(matches!(
            ComponentCmd::parse(&args_for(&["add"])),
            ComponentCmd::Help { requested: false }
        ));
        assert!(matches!(
            ComponentCmd::parse(&args_for(&[])),
            ComponentCmd::Help { requested: false }
        ));
    }

    #[test]
    fn list_shows_core_and_toolchain_optionals() {
        let tmp = tempfile::tempdir().expect("tmp");
        let th = tmp.path().join("th");
        let tc_bin = th.join("toolchains/vA/bin");
        for b in PROJECT_BINARIES {
            fake_bin(&tc_bin, b);
        }
        fake_bin(&tc_bin, "touring-quality");
        let dot = make_project(&tmp.path().join("proj"), "vA");

        let rows = list_components(&dot, &th, &tmp.path().join("no-dev"));
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"touring"), "{names:?}");
        assert!(names.contains(&"touring-quality"), "{names:?}");
        let quality = rows
            .iter()
            .find(|r| r.name == "touring-quality")
            .expect("row");
        assert!(!quality.core);
        assert_eq!(quality.status, "available (not linked)");
    }

    #[test]
    fn add_links_and_remove_unlinks_optional() {
        let tmp = tempfile::tempdir().expect("tmp");
        let th = tmp.path().join("th");
        let target = fake_bin(&th.join("toolchains/vA/bin"), "touring-quality");
        let dot = make_project(&tmp.path().join("proj"), "vA");

        add_component(&dot, "touring-quality", &th, &tmp.path().join("no-dev")).expect("add");
        assert_eq!(
            std::fs::read_link(dot.join("bin/touring-quality")).expect("link"),
            target
        );

        remove_component(&dot, "touring-quality").expect("remove");
        assert!(!dot.join("bin/touring-quality").exists());
    }

    #[test]
    fn add_unknown_component_fails_loud() {
        let tmp = tempfile::tempdir().expect("tmp");
        let dot = make_project(&tmp.path().join("proj"), "vA");
        let err = add_component(
            &dot,
            "touring-fizzbuzz",
            &tmp.path().join("no-th"),
            &tmp.path().join("no-dev"),
        )
        .expect_err("must fail");
        assert!(format!("{err}").contains("not found"), "{err}");
    }

    #[test]
    fn core_binaries_are_refused_by_add_and_remove() {
        let tmp = tempfile::tempdir().expect("tmp");
        let dot = make_project(&tmp.path().join("proj"), "vA");
        for core in PROJECT_BINARIES {
            let err = add_component(&dot, core, &tmp.path().join("th"), &tmp.path().join("dev"))
                .expect_err("add core must fail");
            assert!(format!("{err}").contains("core binary"), "{err}");
            let err = remove_component(&dot, core).expect_err("remove core must fail");
            assert!(format!("{err}").contains("core binary"), "{err}");
        }
    }

    #[test]
    fn remove_unlinked_component_fails_loud() {
        let tmp = tempfile::tempdir().expect("tmp");
        let dot = make_project(&tmp.path().join("proj"), "vA");
        let err = remove_component(&dot, "touring-quality").expect_err("must fail");
        assert!(format!("{err}").contains("not linked"), "{err}");
    }
}
