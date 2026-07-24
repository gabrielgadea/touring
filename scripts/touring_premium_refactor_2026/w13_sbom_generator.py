#!/usr/bin/env python3
"""W13.x — Generate CycloneDX SBOM for the workspace.

Software Bill of Materials (SBOM) is required for:
  - Premium/Enterprise tier compliance (supply-chain auditability)
  - SLSA-3+ provenance attestations
  - Sigstore signing pipeline (W13.y)

Strategy:
  1. Parse Cargo.lock + Cargo.toml across workspace
  2. Build dependency graph with version pins
  3. Emit CycloneDX 1.5 JSON (and optional SPDX 2.3 sidecar)
  4. Compute hash of SBOM for signature attestation

Outputs:
  - data/w13-sbom-cyclonedx.json
  - data/w13-sbom-spdx.json (with --emit-spdx)
  - data/w13-sbom.sha256
"""
from __future__ import annotations

import argparse
import hashlib
import json
import logging
import re
import sys
import uuid
from datetime import UTC, datetime
from pathlib import Path

LOGGER = logging.getLogger(__name__)
_ROOT = Path(__file__).resolve().parents[2]
_CRATES_DIR = _ROOT / "crates"
_LOCKFILE = _ROOT / "Cargo.lock"
_DATA_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "data"

# Cargo.lock format: [[package]] sections with name, version, source
LOCK_PACKAGE_RE = re.compile(
    r'\[\[package\]\]\nname = "([^"]+)"\nversion = "([^"]+)"'
    r'(?:\nsource = "([^"]+)")?',
    re.MULTILINE,
)


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w13_sbom_generator", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--lockfile", type=Path, default=_LOCKFILE)
    p.add_argument("--emit-spdx", action="store_true",
                   help="Also emit SPDX 2.3 sidecar.")
    p.add_argument("--output-dir", type=Path, default=_DATA_DIR)
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def parse_lockfile(lockfile: Path) -> list[dict]:
    """Parse Cargo.lock + return list of {name, version, source}."""
    if not lockfile.exists():
        LOGGER.error("lockfile not found: %s", lockfile)
        return []
    content = lockfile.read_text(encoding="utf-8", errors="replace")
    packages: list[dict] = []
    for m in LOCK_PACKAGE_RE.finditer(content):
        name, version, source = m.groups()
        packages.append({
            "name": name, "version": version,
            "source": source or "workspace",
            "type": "library" if source else "application",
        })
    return packages


def to_purl(pkg: dict) -> str:
    """Convert package to Package URL (purl) spec."""
    if pkg["source"] == "workspace":
        return f"pkg:cargo/{pkg['name']}@{pkg['version']}?source=local"
    if "crates.io" in (pkg["source"] or ""):
        return f"pkg:cargo/{pkg['name']}@{pkg['version']}"
    if "git" in (pkg["source"] or ""):
        return f"pkg:cargo/{pkg['name']}@{pkg['version']}?vcs_url={pkg['source']}"
    return f"pkg:cargo/{pkg['name']}@{pkg['version']}"


def render_cyclonedx(packages: list[dict], serial: str) -> dict:
    """Render CycloneDX 1.5 JSON document."""
    return {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "serialNumber": f"urn:uuid:{serial}",
        "version": 1,
        "metadata": {
            "timestamp": datetime.now(UTC).isoformat(),
            "tools": [{"vendor": "touring", "name": "w13_sbom_generator",
                        "version": "0.1.0"}],
            "component": {
                "type": "application", "name": "touring",
                "purl": "pkg:cargo/touring",
            },
        },
        "components": [
            {
                "type": pkg["type"],
                "bom-ref": to_purl(pkg),
                "name": pkg["name"],
                "version": pkg["version"],
                "purl": to_purl(pkg),
                "scope": "required",
            }
            for pkg in packages
        ],
    }


def render_spdx(packages: list[dict], serial: str) -> dict:
    """Render SPDX 2.3 JSON document."""
    return {
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "documentNamespace": f"https://touring.dev/sbom/{serial}",
        "name": "touring-sbom",
        "creationInfo": {
            "created": datetime.now(UTC).isoformat(),
            "creators": ["Tool: w13_sbom_generator-0.1.0"],
        },
        "packages": [
            {
                "SPDXID": f"SPDXRef-Package-{pkg['name']}",
                "name": pkg["name"],
                "versionInfo": pkg["version"],
                "downloadLocation": pkg["source"] or "NOASSERTION",
                "filesAnalyzed": False,
                "externalRefs": [{
                    "referenceCategory": "PACKAGE-MANAGER",
                    "referenceType": "purl",
                    "referenceLocator": to_purl(pkg),
                }],
            }
            for pkg in packages
        ],
    }


def run(args: argparse.Namespace) -> dict:
    """Parse lockfile + emit SBOM."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    packages = parse_lockfile(args.lockfile)
    if not packages:
        return {"status": "LOCKFILE_EMPTY_OR_MISSING",
                "lockfile": str(args.lockfile)}
    serial = str(uuid.uuid4())
    cyclonedx = render_cyclonedx(packages, serial)
    cyclonedx_path = args.output_dir / "w13-sbom-cyclonedx.json"
    cyclonedx_json = json.dumps(cyclonedx, indent=2, ensure_ascii=False)
    cyclonedx_path.write_text(cyclonedx_json, encoding="utf-8")
    # Hash
    sha256 = hashlib.sha256(cyclonedx_json.encode("utf-8")).hexdigest()
    (args.output_dir / "w13-sbom.sha256").write_text(
        f"{sha256}  w13-sbom-cyclonedx.json\n", encoding="utf-8")
    spdx_path: str | None = None
    if args.emit_spdx:
        spdx = render_spdx(packages, serial)
        sp = args.output_dir / "w13-sbom-spdx.json"
        sp.write_text(json.dumps(spdx, indent=2, ensure_ascii=False),
                       encoding="utf-8")
        spdx_path = str(sp.relative_to(_ROOT))
    return {
        "script": "w13_sbom_generator",
        "wave": "W13", "subtask_refs": ["W13.x"],
        "timestamp": datetime.now(UTC).isoformat(),
        "status": "OK",
        "totals": {
            "packages": len(packages),
            "workspace_local": sum(1 for p in packages
                                     if p["source"] == "workspace"),
            "external": sum(1 for p in packages if p["source"] != "workspace"),
        },
        "serial_number": serial,
        "sha256": sha256,
        "cyclonedx_path": str(cyclonedx_path.relative_to(_ROOT)),
        "spdx_path": spdx_path,
    }


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
            "status": result["status"],
            "totals": result.get("totals", {}),
            "sha256": result.get("sha256"),
            "cyclonedx_path": result.get("cyclonedx_path"),
            "spdx_path": result.get("spdx_path"),
        }, indent=2, ensure_ascii=False) + "\n")
        return 0 if result["status"] == "OK" else 1
    except KeyboardInterrupt:
        LOGGER.warning("interrupted")
        return 130
    except Exception:  # noqa: BLE001
        LOGGER.exception("error")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
