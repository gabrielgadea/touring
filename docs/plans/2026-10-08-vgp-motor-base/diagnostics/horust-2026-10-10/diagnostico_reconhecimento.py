"""HORUST — por que a janela nova nao tem NENHUM candidato? Mede o reconhecedor de `cargo` do gerador: quantos comandos Bash que contem
`cargo check|test|build|clippy|nextest` o gerador reconhece (`analisar_cargo`), por semana, e como sao os que ele NAO reconhece (o
prefixo antes do `cargo`). So leitura dos transcripts (todos os de analise e touring).

Uso (nativo): python3 -I -B diagnostico_reconhecimento.py <saida.json>
"""

import collections
import json
import re
import sys
from datetime import datetime
from pathlib import Path

B = Path("/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base")
sys.path.insert(0, str(B / "scripts"))
import corpus_rust_natural as crn  # noqa: E402
import replay  # noqa: E402

TEM_CARGO = re.compile(r"\bcargo\s+(?:\+\S+\s+)?(?:check|test|build|clippy|nextest)\b")


def semana(ts: str) -> str:
    d = datetime.fromisoformat(ts.replace("Z", "+00:00"))
    return f"{d.isocalendar().year}-W{d.isocalendar().week:02d}"


def prefixo(cmd: str) -> str:
    m = TEM_CARGO.search(cmd)
    antes = cmd[: m.start()].strip().replace("\n", " ") if m else ""
    antes = re.sub(r"/home/gabrielgadea\S*", "<path>", antes)
    antes = re.sub(r"\b\d+\b", "N", antes)
    return antes[-70:] or "(nada)"


def main() -> int:
    por_semana: dict[str, collections.Counter] = collections.defaultdict(collections.Counter)
    nao_reconhecidos: collections.Counter = collections.Counter()
    vistos: set[str] = set()
    for proj in crn.PROJETOS:
        for arq in sorted((replay.PROJECTS / f"-home-gabrielgadea-projects-{proj}").rglob("*.jsonl")):
            for linha in arq.read_text(errors="ignore").splitlines():
                if '"Bash"' not in linha or "cargo" not in linha:
                    continue
                try:
                    rec = json.loads(linha)
                except ValueError:
                    continue
                for b in replay._contents(rec):
                    if b.get("type") != "tool_use" or b.get("name") != "Bash" or b.get("id") in vistos:
                        continue
                    cmd = str((b.get("input") or {}).get("command", ""))
                    if not TEM_CARGO.search(cmd):
                        continue
                    vistos.add(b.get("id"))
                    ok = bool(crn.analisar_cargo(cmd))
                    por_semana[semana(rec["timestamp"])]["reconhecido" if ok else "NAO reconhecido"] += 1
                    if not ok:
                        nao_reconhecidos[prefixo(cmd)] += 1
    saida = {"por_semana": {k: dict(v) for k, v in sorted(por_semana.items())}, "nao_reconhecidos_por_prefixo (top 25)": dict(nao_reconhecidos.most_common(25))}
    Path(sys.argv[1]).write_text(json.dumps(saida, ensure_ascii=False, indent=1), encoding="utf-8")
    for k, v in sorted(por_semana.items()):
        print(k, dict(v))
    print(json.dumps(saida["nao_reconhecidos_por_prefixo (top 25)"], ensure_ascii=False, indent=1))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
