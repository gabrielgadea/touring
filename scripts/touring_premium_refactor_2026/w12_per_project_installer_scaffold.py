#!/usr/bin/env python3
"""W12.1 — Scaffold rustup-like per-project Touring installer.

Generates the installer CLI tree that powers `curl install.touring.dev | sh`:
  - $HOME/.touring/toolchains/<channel>/         (binary versions)
  - $HOME/.touring/cache/                        (shared download cache)
  - <project>/.touring/touring.toml              (per-project config)
  - <project>/.touring/bin/                      (symlinks to active toolchain)
  - <project>/.touring/data/                     (project-local memory/index)

Outputs:
  - staging/w12-touring-init/Cargo.toml      (Rust CLI crate)
  - staging/w12-touring-init/src/main.rs     (subcommands: init, set-channel, status)
  - staging/w12-touring-init/install.sh      (curl-pipe installer)
  - staging/w12-templates/touring.toml.j2    (per-project config template)
  - data/w12-installer-manifest.json
"""
from __future__ import annotations

from pathlib import Path

import script_common as sc

_SCRIPT = "w12_per_project_installer_scaffold"

_ROOT, _DATA_DIR, _STAGING_DIR = sc.ROOT, sc.DATA_DIR, sc.STAGING_DIR
_SKELETON_DIR = _STAGING_DIR / "w12-touring-init"
_TEMPLATES_DIR = _STAGING_DIR / "w12-templates"


def build_parser() -> sc.ArgumentParser:
    """CLI parser."""
    p = sc.new_parser(_SCRIPT, __doc__)
    return sc.with_output_dir_and_verbose(p, _DATA_DIR)


def emit_installer_cargo() -> Path:
    """Emit installer crate Cargo.toml."""
    _SKELETON_DIR.mkdir(parents=True, exist_ok=True)
    cargo = _SKELETON_DIR / "Cargo.toml"
    cargo.write_text("\n".join([
        "# touring-init CLI installer (rustup-like)",
        f"# Generated: {sc.now_iso()}",
        "",
        "[package]",
        'name = "touring-init"',
        'version = "0.1.0"',
        'edition = "2021"',
        'license = "MIT OR Apache-2.0"',
        "",
        "[[bin]]",
        'name = "touring-init"',
        'path = "src/main.rs"',
        "",
        "[dependencies]",
        'clap = { version = "4.5", features = ["derive"] }',
        'serde = { version = "1.0", features = ["derive"] }',
        'toml = "0.8"',
        'reqwest = { version = "0.12", features = ["blocking", "rustls-tls"], default-features = false }',
        'sha2 = "0.10"',
        'anyhow = "1.0"',
        'directories = "5.0"',
        "",
    ]), encoding="utf-8")
    return cargo


def emit_installer_main() -> Path:
    """Emit src/main.rs with subcommands."""
    src = _SKELETON_DIR / "src"
    src.mkdir(parents=True, exist_ok=True)
    main_rs = src / "main.rs"
    main_rs.write_text('''//! `touring-init` — rustup-like installer for per-project Touring.
//!
//! Subcommands:
//!   - init       Bootstrap <project>/.touring/ structure
//!   - set-channel Pin <project> to a specific Touring toolchain
//!   - status     Report toolchain versions + per-project config
//!   - update     Pull latest stable channel into ~/.touring/toolchains/

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "touring-init", version, about)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Initialize <project>/.touring/ structure
    Init {
        #[arg(long, default_value = "stable")]
        channel: String,
    },
    /// Pin project to a specific channel/version
    SetChannel { channel: String },
    /// Show toolchain + per-project status
    Status,
    /// Update toolchain cache
    Update,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Init { channel } => init(&channel),
        Cmd::SetChannel { channel } => set_channel(&channel),
        Cmd::Status => status(),
        Cmd::Update => update(),
    }
}

fn init(channel: &str) -> anyhow::Result<()> {
    println!("Initializing project with channel: {channel}");
    // TODO: W12.2 — actual scaffolding logic
    Ok(())
}

fn set_channel(channel: &str) -> anyhow::Result<()> {
    println!("Pinning to channel: {channel}");
    Ok(())
}

fn status() -> anyhow::Result<()> {
    println!("Touring status: (W12.3 placeholder)");
    Ok(())
}

fn update() -> anyhow::Result<()> {
    println!("Updating toolchain cache: (W12.4 placeholder)");
    Ok(())
}
''', encoding="utf-8")
    return main_rs


def emit_install_sh() -> Path:
    """Emit curl-pipe installer script."""
    _SKELETON_DIR.mkdir(parents=True, exist_ok=True)
    sh = _SKELETON_DIR / "install.sh"
    sh.write_text('''#!/usr/bin/env bash
# AUTO-GENERATED — touring-init bootstrap installer
# Usage: curl -fsSL https://install.touring.dev/ | sh

set -euo pipefail

readonly TOURING_HOME="${TOURING_HOME:-$HOME/.touring}"
readonly CHANNEL="${TOURING_CHANNEL:-stable}"
readonly INSTALL_URL_BASE="${TOURING_INSTALL_URL:-https://releases.touring.dev}"

echo "==> Detecting host triple..."
case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) TRIPLE="x86_64-unknown-linux-gnu" ;;
    Linux-aarch64) TRIPLE="aarch64-unknown-linux-gnu" ;;
    Darwin-x86_64) TRIPLE="x86_64-apple-darwin" ;;
    Darwin-arm64) TRIPLE="aarch64-apple-darwin" ;;
    *) echo "Unsupported platform: $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac
echo "    Host: $TRIPLE"

echo "==> Creating $TOURING_HOME structure..."
mkdir -p "$TOURING_HOME"/{toolchains,cache,bin}

echo "==> Downloading channel manifest: $CHANNEL"
# TODO W12.5: fetch + verify signature
# curl -fsSL "$INSTALL_URL_BASE/channels/$CHANNEL/manifest.toml" > "$TOURING_HOME/manifest.toml"

echo "==> touring-init installed successfully."
echo "    Run: touring-init init  (in your project directory)"
''', encoding="utf-8")
    sh.chmod(0o755)
    return sh


def emit_template_toml() -> Path:
    """Emit touring.toml.j2 template."""
    _TEMPLATES_DIR.mkdir(parents=True, exist_ok=True)
    p = _TEMPLATES_DIR / "touring.toml.j2"
    p.write_text('''# AUTO-GENERATED — per-project touring.toml template
# Place at <project>/.touring/touring.toml

[project]
name = "{{ project_name }}"
created = "{{ timestamp }}"

[toolchain]
channel = "{{ channel }}"           # stable | nightly | <version>
auto_update = false                 # opt-in CI cache refresh

[features]
# Per-project feature toggles (commercial tier-* mapping)
tier = "{{ tier }}"                  # free | standard | premium | enterprise

[storage]
backend = "sqlite"
cache_size_mb = 256

[telemetry]
# OFF by default for premium/enterprise; ON for free/standard with opt-out
enabled = {{ telemetry_default }}

[paths]
data_dir = ".touring/data"
cache_dir = ".touring/cache"
log_dir = ".touring/logs"
''', encoding="utf-8")
    return p


def run(args: sc.Namespace) -> dict:
    """Scaffold installer + templates."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    emitted = [
        str(emit_installer_cargo().relative_to(_ROOT)),
        str(emit_installer_main().relative_to(_ROOT)),
        str(emit_install_sh().relative_to(_ROOT)),
        str(emit_template_toml().relative_to(_ROOT)),
    ]
    manifest = {
        "script": "w12_per_project_installer_scaffold",
        "wave": "W12", "subtask_refs": ["W12.1"],
        "timestamp": sc.now_iso(),
        "status": "OK",
        "supported_triples": [
            "x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu",
            "x86_64-apple-darwin", "aarch64-apple-darwin",
        ],
        "subcommands": ["init", "set-channel", "status", "update"],
        "files_emitted": emitted,
    }
    json_path = args.output_dir / "w12-installer-manifest.json"
    return sc.write_json_report(json_path, manifest, _ROOT)


def _run_and_report(args: sc.Namespace) -> int:
    """Run w12_per_project_installer_scaffold, print its summary; the return value is the exit code."""
    result = run(args)
    sc.emit_json({
        "status": result["status"],
        "subcommands": result["subcommands"],
        "files_emitted_count": len(result["files_emitted"]),
        "json_path": result["json_path"],
    })
    return 0


def main() -> int:
    """Entry point of w12_per_project_installer_scaffold."""
    return sc.main_frame(build_parser, _run_and_report, __name__)


if __name__ == "__main__":
    raise SystemExit(main())
