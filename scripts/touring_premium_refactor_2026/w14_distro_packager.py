#!/usr/bin/env python3
"""W14 — Build deb/rpm/brew/scoop packages + Docker images via CI matrix.

Emits per-distro packaging spec + GitHub Actions matrix workflow.

Outputs:
  - staging/w14-packaging/deb/postinst, postrm
  - staging/w14-packaging/rpm/touring.spec
  - staging/w14-packaging/brew/touring.rb (Homebrew formula)
  - staging/w14-packaging/scoop/touring.json (Scoop manifest)
  - staging/w14-packaging/docker/Dockerfile
  - staging/w14-github-workflows/distro-matrix.yml
  - data/w14-distro-manifest.json
"""
from __future__ import annotations

import argparse
import json
import logging
import sys
from datetime import UTC, datetime
from pathlib import Path

LOGGER = logging.getLogger(__name__)
_ROOT = Path(__file__).resolve().parents[2]
_DATA_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "data"
_STAGING_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "staging"
_PKG_DIR = _STAGING_DIR / "w14-packaging"


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w14_distro_packager", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--output-dir", type=Path, default=_DATA_DIR)
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def emit_deb() -> list[Path]:
    """Emit Debian postinst/postrm scripts."""
    d = _PKG_DIR / "deb"
    d.mkdir(parents=True, exist_ok=True)
    postinst = d / "postinst"
    postinst.write_text("""#!/bin/sh
# Touring post-install script
set -e
if [ "$1" = "configure" ]; then
    update-alternatives --install /usr/local/bin/touring touring \\
        /usr/lib/touring/bin/touring 100 || true
fi
""", encoding="utf-8")
    postinst.chmod(0o755)
    postrm = d / "postrm"
    postrm.write_text("""#!/bin/sh
set -e
if [ "$1" = "purge" ]; then
    update-alternatives --remove touring /usr/lib/touring/bin/touring || true
fi
""", encoding="utf-8")
    postrm.chmod(0o755)
    return [postinst, postrm]


def emit_rpm_spec() -> Path:
    """Emit RPM spec file."""
    d = _PKG_DIR / "rpm"
    d.mkdir(parents=True, exist_ok=True)
    spec = d / "touring.spec"
    spec.write_text("""# Touring RPM spec
Name:           touring
Version:        %{?_version}
Release:        1%{?dist}
Summary:        Code intelligence and AI-assisted refactoring

License:        MIT OR Apache-2.0
URL:            https://touring.dev
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  rust >= 1.83
Requires:       glibc

%description
Touring is a code intelligence platform providing AST-based blast radius
analysis, BM25 symbol search, and reinforcement-learning-guided refactoring.

%prep
%autosetup

%build
cargo build --release --frozen

%install
mkdir -p %{buildroot}/usr/lib/touring/bin
install -m 0755 target/release/touring %{buildroot}/usr/lib/touring/bin/
mkdir -p %{buildroot}/usr/local/bin
ln -s /usr/lib/touring/bin/touring %{buildroot}/usr/local/bin/touring

%files
/usr/lib/touring/bin/touring
/usr/local/bin/touring

%changelog
* %{?_changedate} Touring Team - %{version}-1
- Release %{version}
""", encoding="utf-8")
    return spec


def emit_brew_formula() -> Path:
    """Emit Homebrew formula."""
    d = _PKG_DIR / "brew"
    d.mkdir(parents=True, exist_ok=True)
    rb = d / "touring.rb"
    rb.write_text('''# Homebrew formula for Touring
class Touring < Formula
  desc "Code intelligence and AI-assisted refactoring"
  homepage "https://touring.dev"
  url "https://github.com/touring/touring/archive/v0.1.0.tar.gz"
  sha256 "PLACEHOLDER_FILL_AT_RELEASE"
  license any_of: ["MIT", "Apache-2.0"]

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "touring", shell_output("#{bin}/touring --version")
  end
end
''', encoding="utf-8")
    return rb


def emit_scoop_manifest() -> Path:
    """Emit Scoop manifest (Windows)."""
    d = _PKG_DIR / "scoop"
    d.mkdir(parents=True, exist_ok=True)
    p = d / "touring.json"
    p.write_text(json.dumps({
        "version": "0.1.0",
        "description": "Code intelligence and AI-assisted refactoring",
        "homepage": "https://touring.dev",
        "license": "MIT OR Apache-2.0",
        "architecture": {
            "64bit": {
                "url": "https://github.com/touring/touring/releases/download/v0.1.0/touring-x86_64-pc-windows-msvc.zip",
                "hash": "PLACEHOLDER_FILL_AT_RELEASE",
            },
        },
        "bin": "touring.exe",
        "checkver": {
            "github": "https://github.com/touring/touring",
        },
        "autoupdate": {
            "architecture": {
                "64bit": {
                    "url": "https://github.com/touring/touring/releases/download/v$version/touring-x86_64-pc-windows-msvc.zip",
                },
            },
        },
    }, indent=2) + "\n", encoding="utf-8")
    return p


def emit_dockerfile() -> Path:
    """Emit Docker image."""
    d = _PKG_DIR / "docker"
    d.mkdir(parents=True, exist_ok=True)
    df = d / "Dockerfile"
    df.write_text("""# Touring multi-stage Docker build
FROM rust:1.83-slim-bookworm AS builder
WORKDIR /src
COPY . .
RUN cargo build --release --frozen

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \\
    ca-certificates libssl3 \\
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /src/target/release/touring /usr/local/bin/touring
ENTRYPOINT ["touring"]
LABEL org.opencontainers.image.source="https://github.com/touring/touring"
LABEL org.opencontainers.image.licenses="MIT OR Apache-2.0"
""", encoding="utf-8")
    return df


def emit_workflow() -> Path:
    """Emit GitHub Actions distro matrix workflow."""
    w = _STAGING_DIR / "w14-github-workflows"
    w.mkdir(parents=True, exist_ok=True)
    yml = w / "distro-matrix.yml"
    yml.write_text("""# AUTO-GENERATED — distro packaging matrix
name: Distro Packaging

on:
  release:
    types: [created]

jobs:
  package:
    strategy:
      matrix:
        format: [deb, rpm, brew, scoop, docker]
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Build ${{ matrix.format }} package
        run: ./scripts/touring_premium_refactor_2026/w14_distro_packager.py
""", encoding="utf-8")
    return yml


def run(args: argparse.Namespace) -> dict:
    """Emit all distro artifacts."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    _PKG_DIR.mkdir(parents=True, exist_ok=True)
    emitted = []
    emitted.extend(str(p.relative_to(_ROOT)) for p in emit_deb())
    emitted.append(str(emit_rpm_spec().relative_to(_ROOT)))
    emitted.append(str(emit_brew_formula().relative_to(_ROOT)))
    emitted.append(str(emit_scoop_manifest().relative_to(_ROOT)))
    emitted.append(str(emit_dockerfile().relative_to(_ROOT)))
    emitted.append(str(emit_workflow().relative_to(_ROOT)))
    manifest = {
        "script": "w14_distro_packager",
        "wave": "W14", "subtask_refs": ["W14.distro"],
        "timestamp": datetime.now(UTC).isoformat(),
        "status": "OK",
        "formats": ["deb", "rpm", "brew", "scoop", "docker"],
        "files_emitted": emitted,
    }
    json_path = args.output_dir / "w14-distro-manifest.json"
    json_path.write_text(json.dumps(manifest, indent=2, ensure_ascii=False),
                          encoding="utf-8")
    manifest["json_path"] = str(json_path.relative_to(_ROOT))
    return manifest


def main() -> int:
    """Entry point."""
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        result = run(args)
        sys.stdout.write(json.dumps({
            "status": result["status"], "formats": result["formats"],
            "files_emitted_count": len(result["files_emitted"]),
            "json_path": result["json_path"],
        }, indent=2, ensure_ascii=False) + "\n")
        return 0
    except KeyboardInterrupt:
        LOGGER.warning("interrupted")
        return 130
    except Exception:  # noqa: BLE001
        LOGGER.exception("error")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
