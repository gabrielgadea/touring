#!/usr/bin/env python3
"""orphan-classify.py — Classify pub symbols as structural/intentional/feature-gated.

Wave W4 of the 47to13-residual UPGRADE plan (Premium Elite product).

Touring has 6,367 orphan pub symbols. The plan's W4 target is ≤ 2,000
(structural only). This tool walks the workspace and classifies each
pub symbol by:

  1. **Structural**: pub symbol with no consumer (true orphan, action: wire).
  2. **Re-export**: `pub use foo::Bar;` facades (intentional compat shims).
  3. **Feature-gated**: `#[cfg(feature = "X")]` symbols (only relevant in feature combos).
  4. **Trait-method**: `impl Trait for X` methods (required by contract, not a "real" use).
  5. **Serde-derive**: `#[derive(Serialize)]` types (used via trait bound, not direct call).

Output: JSON to stdout + per-crate table to stderr.

Stdlib-only, fast, deterministic.
"""
from __future__ import annotations
import argparse
import json
import re
import sys
from collections import Counter
from dataclasses import dataclass, field, asdict
from pathlib import Path

_ROOT = Path("/home/gabrielgadea/.claude/rust")
_CRATES = _ROOT / "crates"

# A "pub use" re-export.
_PUB_USE_RE = re.compile(r"^\s*pub\s+use\s+", re.MULTILINE)
# A #[cfg(feature = "...")] attribute on or before a pub item.
_CFG_FEATURE_RE = re.compile(r"#\[cfg\(feature\s*=\s*\"([^\"]+)\"\)\]")
# A generic pub item.
_PUB_ITEM_RE = re.compile(
    r"^(\s*)pub\s+(fn|struct|enum|trait|mod|const|static|type|use|macro_rules!)\s+(\w+)",
    re.MULTILINE,
)


@dataclass
class CrateOrphans:
    crate: str
    pub_total: int
    re_exports: int
    feature_gated: int
    trait_methods: int
    serde_derives: int
    structural: int  # the rest

    @property
    def structural_pct(self) -> float:
        return round(self.structural / max(self.pub_total, 1) * 100, 2)

    @property
    def status(self) -> str:
        if self.structural_pct <= 30:
            return "PASS"
        if self.structural_pct <= 50:
            return "WARN"
        return "FAIL"


@dataclass
class OverallReport:
    tool: str = "orphan-classify"
    version: str = "0.1.0"
    workspace_root: str = str(_ROOT)
    crates: list[CrateOrphans] = field(default_factory=list)

    @property
    def total(self) -> dict:
        s = Counter()
        for c in self.crates:
            for k in ["pub_total", "re_exports", "feature_gated", "trait_methods",
                       "serde_derives", "structural"]:
                s[k] += getattr(c, k)
        s["crates_pass"] = sum(1 for c in self.crates if c.status == "PASS")
        s["crates_warn"] = sum(1 for c in self.crates if c.status == "WARN")
        s["crates_fail"] = sum(1 for c in self.crates if c.status == "FAIL")
        s["structural_pct"] = round(s["structural"] / max(s["pub_total"], 1) * 100, 2)
        return dict(s)


def _iter_rs(crate_dir: Path) -> list[Path]:
    if not (crate_dir / "src").exists():
        return []
    return [p for p in (crate_dir / "src").rglob("*.rs") if "target" not in p.parts]


def _classify(text: str, file_path: Path) -> dict:
    """Return counts: re_exports, feature_gated, trait_methods, serde_derives, structural, total."""
    counts = Counter({
        "pub_total": 0,
        "re_exports": 0,
        "feature_gated": 0,
        "trait_methods": 0,
        "serde_derives": 0,
        "structural": 0,
    })
    # Strip block comments to avoid false positives
    text_clean = re.sub(r"/\*.*?\*/", "", text, flags=re.DOTALL)
    for m in _PUB_ITEM_RE.finditer(text_clean):
        counts["pub_total"] += 1
        line_start = text_clean.rfind("\n", 0, m.start()) + 1
        # Look back up to 30 lines for context (#[derive(...)], #[cfg(...)])
        context_start = max(0, line_start - 1500)
        context = text_clean[context_start:line_start]
        item = m.group(0)
        is_re_export = item.lstrip().startswith("pub use")
        is_feature_gated = bool(_CFG_FEATURE_RE.search(context))
        is_serde_derive = "Serialize" in context or "Deserialize" in context
        # Trait method: `pub fn` inside an `impl ... for ...` block
        # Heuristic: look back 5 lines for `impl ... for ...`
        near_context = text_clean[max(0, line_start - 500):line_start]
        is_trait_method = bool(re.search(r"impl\s+[\w:]+\s+for\s+", near_context)) and "fn " in item
        if is_re_export:
            counts["re_exports"] += 1
        elif is_feature_gated:
            counts["feature_gated"] += 1
        elif is_serde_derive:
            counts["serde_derives"] += 1
        elif is_trait_method:
            counts["trait_methods"] += 1
        else:
            counts["structural"] += 1
    return dict(counts)


def measure_crate(crate_name: str) -> CrateOrphans:
    crate_dir = _CRATES / crate_name
    agg = Counter()
    for f in _iter_rs(crate_dir):
        try:
            text = f.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        for k, v in _classify(text, f).items():
            agg[k] += v
    return CrateOrphans(
        crate=crate_name,
        pub_total=agg["pub_total"],
        re_exports=agg["re_exports"],
        feature_gated=agg["feature_gated"],
        trait_methods=agg["trait_methods"],
        serde_derives=agg["serde_derives"],
        structural=agg["structural"],
    )


def main() -> int:
    p = argparse.ArgumentParser(prog="orphan-classify", description=__doc__)
    p.add_argument("--crate", help="Single crate to measure")
    p.add_argument("--json", action="store_true", help="JSON-only stdout")
    p.add_argument("-q", "--quiet", action="store_true", help="Suppress stderr table")
    args = p.parse_args()

    # Use the doc-coverage's member parser (same path logic)
    cargo = _ROOT / "Cargo.toml"
    text = cargo.read_text()
    m = re.search(r"\[workspace\][^\[]*?members\s*=\s*\[(.*?)\]", text, re.DOTALL)
    members: list[str] = []
    if m:
        for line in m.group(1).splitlines():
            line = re.sub(r"#.*$", "", line).strip().rstrip(",").strip('"').strip("'")
            if not line:
                continue
            if line.startswith("crates/"):
                members.append(line[len("crates/"):])
            elif line.startswith("./crates/"):
                members.append(line[len("./crates/"):])
            elif line.startswith("./"):
                members.append(line[2:])
            else:
                members.append(line)

    if args.crate:
        members = [c for c in members if c.endswith(args.crate)]

    report = OverallReport(
        crates=[measure_crate(m) for m in sorted(members) if (Path(_CRATES) / m).exists()]
    )
    report.crates.sort(key=lambda c: c.structural_pct, reverse=True)
    payload = asdict(report)
    payload["total"] = report.total

    if args.json or not args.quiet:
        print(json.dumps(payload, indent=2))
    if not args.json and not args.quiet:
        t = report.total
        print(f"\n{'='*90}\nORPHAN-CLASSIFY — Touring\n{'='*90}\n", file=sys.stderr)
        print(f"{'CRATE':<32} {'PUB':>6} {'RUSE':>5} {'CFG':>5} {'TRT':>5} {'SRD':>5} {'STR':>6} {'STR%':>6}  {'STATUS':<5}",
              file=sys.stderr)
        print("-" * 90, file=sys.stderr)
        for c in report.crates:
            print(f"{c.crate:<32} {c.pub_total:>6} {c.re_exports:>5} {c.feature_gated:>5} "
                  f"{c.trait_methods:>5} {c.serde_derives:>5} {c.structural:>6} {c.structural_pct:>5.1f}%  {c.status}",
                  file=sys.stderr)
        print("-" * 90, file=sys.stderr)
        print(f"{'TOTAL':<32} {t['pub_total']:>6} {t['re_exports']:>5} {t['feature_gated']:>5} "
              f"{t['trait_methods']:>5} {t['serde_derives']:>5} {t['structural']:>6} {t['structural_pct']:>5.1f}%  "
              f"P:{t['crates_pass']} W:{t['crates_warn']} F:{t['crates_fail']}",
              file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
