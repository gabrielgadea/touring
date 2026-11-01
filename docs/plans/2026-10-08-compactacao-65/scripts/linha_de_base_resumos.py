"""Baseline (and later the 'after') of what compaction summaries keep, using the recorder's own judge.

For every compact_boundary followed by a summary in the analise transcripts (optionally only those
after --since YYYY-MM-DD), judge the segment it compacted: coverage of files changed, DAG ids and
tests run, summary size, and whether it used the old TASK/NEXT format block. Aggregates only.
"""

import argparse
import statistics
import sys
from pathlib import Path

sys.path.insert(0, str(Path.home() / ".claude/skills/loop-engineering/scripts/hooks"))
import compaction_ledger as cl

PROJECT = Path.home() / ".claude/projects/-home-gabrielgadea-projects-analise"


def compactions(recs):
    """Yield (segment, summary_text, boundary) for every boundary that has a summary after it."""
    idx = [i for i, r in enumerate(recs) if cl._is_boundary(r)]
    for k, i in enumerate(idx):
        start = idx[k - 1] + 1 if k else 0
        end = idx[k + 1] if k + 1 < len(idx) else len(recs)
        summary = cl._summary_from_transcript(recs[i:end])
        if summary:
            yield recs[start:i], summary, recs[i]


def ratio(pairs, key):
    kept = sum(p[key][0] for p in pairs)
    total = sum(p[key][1] for p in pairs)
    return f"{kept}/{total} ({kept / total:.0%})" if total else "0/0"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--since", default="2026-09-17")
    args = ap.parse_args()
    rows = []
    for f in sorted(PROJECT.glob("*.jsonl")):
        recs = cl._load(f)
        for seg, summary, boundary in compactions(recs):
            if str(boundary.get("timestamp", ""))[:10] < args.since:
                continue
            files, tests, dags = cl._collect(seg)
            facts = {"files": cl._ordered_unique(files), "dags": cl._ordered_unique(dags),
                     "tests": cl._ordered_unique(tests)}
            gaps = cl.judge(summary, facts)
            rows.append({"chars": len(summary), "fmt": "TASK:" in summary and "NEXT:" in summary,
                         **cl._coverage(facts, gaps)})
    if not rows:
        print("nenhuma compactação com resumo no período — medição vazia, não zero")
        return 1
    print(f"{len(rows)} compactações desde {args.since}")
    print(f"  resumo: p50 {statistics.median(r['chars'] for r in rows):,.0f} chars · "
          f"formato TASK/NEXT em {sum(r['fmt'] for r in rows)}/{len(rows)}")
    for key, label in (("files", "arquivos alterados"), ("dags", "DAG citados"), ("tests", "testes rodados")):
        print(f"  {label:<20} preservados {ratio(rows, key)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
