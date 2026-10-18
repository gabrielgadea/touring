"""Prova do corpus_rust_sanidade_v2 contra o juiz de VERDADE.

O que a coordenadora pediu: sha do entradas.jsonl novo, as contagens, e a prova do teste contra o
juiz de verdade, com chamadas > 0 e positivos de simbolo > 0 — E o nome do binario usado.

Este script mede e imprime, sem mocks e sem tocar no corpus_rust/ vivo (que o vrs-porta esta
usando).
"""
import hashlib
import json
import subprocess
import sys
from collections import Counter
from pathlib import Path

CORPUS = Path.home() / ".claude/touring/vgp/corpus_rust_sanidade_v2"
JUIZ = Path(
    "/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base"
    "/scripts/juiz_motor_vgp.py"
)
# O binario CONGELADO. A coordenadora avisou que o 30.4.96 implantado ainda responde stub em Rust,
# entao o recall 0/3 e esperado com ele; o que precisa ficar provado aqui e o DENOMINADOR.
MOTOR = Path.home() / ".claude/touring/vgp/binarios/f55ec4b5/touring"


def main() -> int:
    arq = CORPUS / "entradas.jsonl"
    if not arq.is_file():
        print(f"FALHA: {arq} nao existe")
        return 2
    b = arq.read_bytes()
    print("=== corpus_rust_sanidade_v2 ===")
    print(f"  entradas.jsonl sha256 = {hashlib.sha256(b).hexdigest()}")
    rows = [json.loads(x) for x in b.decode().splitlines() if x.strip()]
    print(f"  entradas = {len(rows)}")
    print(f"  rotulo   = {dict(Counter(r.get('rotulo') for r in rows))}")
    print(f"  classe   = {dict(Counter(r.get('classe') for r in rows))}")
    ctx = Counter(type(r.get('contexto')).__name__ for r in rows)
    print(f"  contexto = {dict(ctx)}  (o juiz ITERA este campo; lista e o contrato)")
    pos = [r for r in rows if r.get("rotulo") == "positivo" and str(r.get("reproduz")).lower() == "true"]
    print(f"  positivos que o juiz conta = {len(pos)}")
    for r in pos:
        print(f"      {r.get('classe'):10s} nome={r.get('nome_esperado')}")
        print(f"         mensagem: {str(r.get('mensagem'))[:110]}")

    print()
    print("=== juiz de verdade ===")
    v = MOTOR if MOTOR.is_file() else None
    if v is None:
        print(f"  (motor congelado ausente em {MOTOR}; uso o do PATH)")
    ver = subprocess.run(
        [str(MOTOR), "--version"], capture_output=True, text=True, timeout=60
    )
    linha_ver = (ver.stderr or ver.stdout).strip().splitlines()
    print(f"  BINARIO USADO = {MOTOR}")
    print(f"  versao        = {linha_ver[0] if linha_ver else '(nao respondeu)'}")
    print(f"  sha256[:8]    = {hashlib.sha256(MOTOR.read_bytes()).hexdigest()[:8]}")

    out = Path("/tmp/juiz-rust-v2.json")
    cmd = [sys.executable, str(JUIZ), "--corpus", str(CORPUS), "--json", str(out)]
    if v is not None:
        cmd += ["--touring", str(MOTOR)]
    j = subprocess.run(cmd, capture_output=True, text=True, timeout=3600)
    txt = (j.stdout or "") + (j.stderr or "")
    if "Traceback" in txt:
        print("  Traceback no juiz:")
        for ln in txt.splitlines()[-8:]:
            print("   ", ln[:120])
        return 1
    if not out.is_file():
        print(f"  juiz nao escreveu veredito: {txt[-300:]}")
        return 1
    r = json.loads(out.read_text())
    print(f"  chamadas           = {r.get('chamadas')}")
    print(f"  positivos_simbolo  = {r.get('positivos_simbolo')}")
    print(f"  nao_classificado   = {len(r.get('nao_classificado', []))}")
    print(f"  negativos          = {r.get('negativos')}")
    print(f"  falso_deny_por_balde = {r.get('falso_deny_por_balde')}")
    print(f"  pendencias         = {r.get('pendencias')}")
    ps = r.get("positivos_simbolo", {}).get("arquivo", [0, 0])
    print()
    ok = (r.get("chamadas", 0) > 0) and (ps[1] > 0)
    print("VEREDITO DA PROVA:", "denominador EXISTE (chamadas>0 e positivos>0)" if ok
          else "denominador AUSENTE — o defeito NAO foi corrigido")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())