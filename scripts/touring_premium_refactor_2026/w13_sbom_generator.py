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

import hashlib
import json
import logging
import re
import uuid
from pathlib import Path

import script_common as sc

_SCRIPT = "w13_sbom_generator"

LOGGER = logging.getLogger(__name__)
_ROOT, _DATA_DIR, _CRATES_DIR = sc.ROOT, sc.DATA_DIR, sc.CRATES_DIR
_LOCKFILE = _ROOT / "Cargo.lock"

# Cargo.lock format: [[package]] sections with name, version, source
LOCK_PACKAGE_RE = re.compile(
    r'\[\[package\]\]\nname = "([^"]+)"\nversion = "([^"]+)"'
    r'(?:\nsource = "([^"]+)")?',
    re.MULTILINE,
)


def build_parser() -> sc.ArgumentParser:
    """CLI parser."""
    p = sc.new_parser(_SCRIPT, __doc__)
    p.add_argument("--lockfile", type=Path, default=_LOCKFILE)
    p.add_argument("--emit-spdx", action="store_true",
                   help="Also emit SPDX 2.3 sidecar.")
    return sc.with_output_dir_and_verbose(p, _DATA_DIR)


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
            "timestamp": sc.now_iso(),
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
            "created": sc.now_iso(),
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


def run(args: sc.Namespace) -> dict:
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
        "timestamp": sc.now_iso(),
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


def _run_and_report(args: sc.Namespace) -> int:
    """Run w13_sbom_generator, print its summary; the return value is the exit code."""
    result = run(args)
    sc.emit_json({
        "status": result["status"],
        "totals": result.get("totals", {}),
        "sha256": result.get("sha256"),
        "cyclonedx_path": result.get("cyclonedx_path"),
        "spdx_path": result.get("spdx_path"),
    })
    return 0 if result["status"] == "OK" else 1


def main() -> int:
    """Entry point of w13_sbom_generator."""
    return sc.main_frame(build_parser, _run_and_report, __name__)


if __name__ == "__main__":
    raise SystemExit(main())
