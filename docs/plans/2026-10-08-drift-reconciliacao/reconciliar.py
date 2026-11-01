#!/usr/bin/env python3
"""Reconciliação do delta de drift 31/08 → 08/10/2026 (561 → 669 sinais).

Delegação: analise-6b, aprovada por Gabriel em 08/10/2026.
Juiz de aceite: exit codes A1-A5 descritos no bundle.

Definições:
  S_old := conjunto de sinais (categoria, item) do snapshot congelado
           docs/plans/2026-08-26-documentacao-touring/knowledge/scan-2026-08-31.json
           (régua de 31/08, materializada — o scanner não mudou desde então:
           único commit de scripts/drift_semantic_scan.py é ec2e5928).
  S_now := conjunto de sinais do scanner atual no estado atual.
  delta := S_now - S_old (sinais novos a reconciliar, 1 disposição cada).
  gone  := S_old - S_now (resolvidos por terceiros entre 31/08 e hoje; informativo).

CONTROLE POSITIVO (A3): as contagens recomputadas das LISTAS do snapshot
devem ser exatamente C1=36, C3=11, C4=279, C5=151, C6=84 (total 561) — o
número que scripts/test_drift_coverage.py fixa. Um zero aqui seria
indistinguível de "olhei no lugar errado" sem esta prova.

Uso:
  python3 reconciliar.py snapshot-check            # controle positivo do baseline
  python3 reconciliar.py delta [--scan-json PATH]  # recomputa o delta (roda o scan se sem --scan-json)
  python3 reconciliar.py verify [--scan-json PATH] # delta coberto pelo ledger.jsonl, 1 disposição cada
"""
import json
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
BUNDLE = Path(__file__).resolve().parent
SNAPSHOT = REPO / "docs/plans/2026-08-26-documentacao-touring/knowledge/scan-2026-08-31.json"
SCANNER = REPO / "scripts" / "drift_semantic_scan.py"
LEDGER = BUNDLE / "ledger.jsonl"

# Contagens congeladas em 31/08 (scripts/test_drift_coverage.py).
BASELINE = {"C1": 36, "C3": 11, "C4": 279, "C5": 151, "C6": 84}
BASELINE_TOTAL = 561
DISPOSICOES = ("a", "b", "c")  # a=corrigido na fonte · b=falso positivo do scanner · c=registro datado legítimo


def signal_set(scan: dict) -> dict[str, set[str]]:
    """Extrai {(categoria, item)} de um JSON de scan (snapshot ou atual).

    Identidade estável do sinal: o ITEM distinto (versão, comando, crate,
    env var), não a ocorrência — é assim que o scanner conta (listas de
    distintos) e é estável entre commits mesmo com linhas mudando.
    """
    cats = scan["categories"]
    return {
        "C1": set(cats["C1-versions"]["stale_versions"]),
        "C3": set(cats["C3-fused-crates-cited"]["fused_crates_with_citations"].keys()),
        "C4": set(cats["C4-cli-cited"]["potential_stale"]),
        "C5": set(cats["C5-crates-in-code-examples"]["stale"]),
        "C6": set(cats["C6-envvars-cited"]["stale"]),
    }


def load_current_scan(scan_json: str | None) -> dict:
    """Scan atual: de um JSON salvo (--scan-json) ou rodando o scanner vivo."""
    if scan_json:
        return json.loads(Path(scan_json).read_text(encoding="utf-8"))
    r = subprocess.run(
        [sys.executable, str(SCANNER), "--quiet"],
        capture_output=True, text=True, cwd=REPO, timeout=300,
    )
    if r.returncode != 0:
        print(f"ERRO: scanner exit {r.returncode}: {r.stderr[-300:]}", file=sys.stderr)
        sys.exit(2)
    return json.loads(r.stdout)


def cmd_snapshot_check() -> int:
    """CONTROLE POSITIVO: o snapshot reproduz 561 = 36+11+279+151+84."""
    snap = json.loads(SNAPSHOT.read_text(encoding="utf-8"))
    s = signal_set(snap)
    errors = []
    total = 0
    print("snapshot:", SNAPSHOT.name, "| scan_timestamp:", snap.get("scan_timestamp_utc"))
    for cat, expected in BASELINE.items():
        got = len(s[cat])
        total += got
        if got != expected:
            errors.append(f"{cat}: snapshot={got} esperado={expected}")
    if total != BASELINE_TOTAL:
        errors.append(f"total: snapshot={total} esperado={BASELINE_TOTAL}")
    for e in errors:
        print(f"  FAIL {e}")
    if not errors:
        print(f"  OK — snapshot reproduz {BASELINE_TOTAL} = "
              + "+".join(str(BASELINE[c]) for c in BASELINE))
    return 1 if errors else 0


def compute_delta(scan_json: str | None) -> tuple[dict, dict, dict]:
    """Retorna (s_now, delta, gone) como {cat: sorted list}."""
    snap = json.loads(SNAPSHOT.read_text(encoding="utf-8"))
    s_old = signal_set(snap)
    s_now = signal_set(load_current_scan(scan_json))
    delta = {c: sorted(s_now[c] - s_old[c]) for c in s_now}
    gone = {c: sorted(s_old[c] - s_now[c]) for c in s_now}
    return s_now, delta, gone


def cmd_delta(scan_json: str | None) -> int:
    _, delta, gone = compute_delta(scan_json)
    n_delta = sum(len(v) for v in delta.values())
    n_gone = sum(len(v) for v in gone.values())
    print(f"delta (S_now - S_old): {n_delta} sinais novos")
    for cat in BASELINE:
        print(f"  {cat}: +{len(delta[cat])} novos · -{len(gone[cat])} resolvidos")
    print(f"gone (S_old - S_now): {n_gone} sinais resolvidos por terceiros")
    out = {"delta": delta, "gone": gone}
    (BUNDLE / "delta.json").write_text(json.dumps(out, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"gravado: {BUNDLE / 'delta.json'}")
    return 0


def _validate_row(row: dict) -> list[str]:
    """Valida UMA linha do ledger contra um sinal do delta."""
    errors = []
    key = f"{row.get('categoria')} {row.get('item')}"
    if row.get("disposicao") not in DISPOSICOES:
        errors.append(f"DISPOSIÇÃO INVÁLIDA: {key} -> {row.get('disposicao')!r}")
    if row.get("disposicao") == "c" and len(row.get("motivo", "")) < 30:
        errors.append(f"MOTIVO <30 chars em (c): {key}")
    if row.get("disposicao") == "a" and not row.get("mudanca"):
        errors.append(f"(a) sem campo 'mudanca' (o que mudou na fonte): {key}")
    if row.get("disposicao") == "b" and not row.get("predicado"):
        errors.append(f"(b) sem campo 'predicado' (qual correção do scanner): {key}")
    return errors


def _uniao_delta(delta_ref: dict, delta_vivo: dict) -> dict[str, set[str]]:
    """União por categoria: o congelado (lote original) ∪ o vivo (scan atual − snapshot)."""
    return {cat: set(delta_ref.get(cat, [])) | set(delta_vivo.get(cat, [])) for cat in BASELINE}


def _cobertura_exata(uniao: dict[str, set[str]], ledger: list[dict]) -> list[str]:
    """Todo sinal da união tem EXATAMENTE 1 linha; nenhuma linha é órfã da união."""
    errors = []
    by_key: dict[tuple[str, str], int] = {}
    for row in ledger:
        key = (str(row.get("categoria") or ""), str(row.get("item") or ""))
        by_key[key] = by_key.get(key, 0) + 1
    for cat, items in uniao.items():
        for item in items:
            n = by_key.get((cat, item), 0)
            if n == 0:
                errors.append(f"SEM DISPOSIÇÃO: {cat} {item}")
            elif n > 1:
                errors.append(f"DISPOSIÇÃO DUPLA ({n}×): {cat} {item}")
    for row in ledger:
        cat, item = str(row.get("categoria") or ""), str(row.get("item") or "")
        if item not in uniao.get(cat, set()):
            errors.append(f"LINHA ÓRFÃ (sinal não está no delta): {cat} {item}")
    return errors


def _contagens_fecham(uniao: dict[str, set[str]], ledger: list[dict]) -> list[str]:
    """Contagem do ledger por categoria == tamanho da união por categoria."""
    errors = []
    for cat in BASELINE:
        n_ledger = sum(1 for r in ledger if r.get("categoria") == cat)
        if n_ledger != len(uniao[cat]):
            errors.append(f"contagem {cat}: ledger={n_ledger} != delta(união)={len(uniao[cat])}")
    return errors


def _coverage_errors(delta_ref: dict, delta_vivo: dict, ledger: list[dict]) -> list[str]:
    """Cobertura sobre a UNIÃO (congelado ∪ vivo): disposições (a)/(b) removem o
    sinal do scan vivo — o delta vivo não os contém, mas o ledger DEVE registrá-los;
    e sinais novos pós-lote (trabalho concorrente) entram pelo vivo."""
    uniao = _uniao_delta(delta_ref, delta_vivo)
    return _cobertura_exata(uniao, ledger) + _contagens_fecham(uniao, ledger)


def _consistency_errors(ledger: list[dict], s_now: dict[str, set[str]]) -> list[str]:
    """A disposição declarada confere com o scan ATUAL (régua final):

    (b) ou (a) ⇒ o item NÃO pode mais aparecer como stale (o predicado o
        eliminou / a fonte foi corrigida);
    (c) ⇒ o item DEVE continuar aparecendo (permanece contado, declarado).
    """
    errors = []
    for row in ledger:
        cat, item = str(row.get("categoria")), str(row.get("item"))
        present = item in s_now.get(cat, set())
        disp = row.get("disposicao")
        if disp in ("a", "b") and present:
            errors.append(f"({disp}) mas AINDA STALE no scan atual: {cat} {item}")
        if disp == "c" and not present:
            errors.append(f"(c) mas SUMIU do scan atual (deveria permanecer contado): {cat} {item}")
    return errors


def cmd_verify(scan_json: str | None) -> int:
    """Verificação completa: snapshot 561 + ledger cobre o DELTA VIVO + disposição×scan atual.

    Delta vivo = (scan atual) − (snapshot 31/08), recomputado a cada run — é o que
    torna este verificador permanente: qualquer sinal novo (ex.: trabalho concorrente
    de outra sessão) exige linha no ledger, não só os 111 do lote original
    (delta.json fica como registro do lote de 08/10 manhã).
    """
    if not LEDGER.is_file():
        print(f"ERRO: ledger ausente: {LEDGER}", file=sys.stderr)
        return 2
    rc_snap = cmd_snapshot_check()
    _, delta, _ = compute_delta(scan_json)
    delta_ref = json.loads((BUNDLE / "delta.json").read_text(encoding="utf-8"))["delta"]
    s_now = signal_set(load_current_scan(scan_json)) if scan_json else signal_set(load_current_scan(None))
    ledger = [json.loads(line) for line in LEDGER.read_text(encoding="utf-8").splitlines() if line.strip()]
    errors = _coverage_errors(delta_ref, delta, ledger)
    for row in ledger:
        errors.extend(_validate_row(row))
    errors.extend(_consistency_errors(ledger, s_now))
    n_delta = sum(len(v) for v in delta.values())
    n_now = sum(len(v) for v in s_now.values())
    print(f"delta vivo (scan atual − snapshot 31/08): {n_delta} sinais · ledger: {len(ledger)} linhas")
    print(f"scan atual (régua final): {n_now} sinais = "
          + "+".join(f"{c}={len(s_now[c])}" for c in BASELINE))
    for e in errors:
        print(f"  FAIL {e}")
    if rc_snap != 0:
        errors.append("snapshot-check falhou")
    if not errors:
        print("VERIFY OK — snapshot 561 reproduzido; cada sinal do delta vivo tem exatamente 1 "
              "disposição válida; disposições conferem com o scan atual")
    return 1 if errors else 0


def main() -> int:
    args = sys.argv[1:]
    if not args or args[0] not in ("snapshot-check", "delta", "verify"):
        print(__doc__)
        return 2
    scan_json = None
    if "--scan-json" in args:
        i = args.index("--scan-json")
        scan_json = args[i + 1]
    return {"snapshot-check": cmd_snapshot_check,
            "delta": lambda: cmd_delta(scan_json),
            "verify": lambda: cmd_verify(scan_json)}[args[0]]()


if __name__ == "__main__":
    sys.exit(main())
