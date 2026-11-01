"""Mutantes do reconhecedor de cargo estendido (HORUST): cada um e aplicado a uma COPIA de `estado_rust_no_instante.py` (o original nunca e
escrito; sha conferido antes e depois) e `test_estado_rust_no_instante.py` (via $RECONHECEDOR_MODULO) TEM de reprovar. O controle
(copia sem mutacao) tem de passar. Mutante que nao termina (m11) reprova por TIMEOUT.

  r1..r11: envoltorios (hw-slot, env -u, timeout, nice, taskset, VAR=), bash -c, separadores, cd interno, instalar().

Uso: python3 -I mutantes_estado_rust.py <estado_rust_no_instante.py> <test_estado_rust_no_instante.py> <saida.json>
Exit 0 so se todos morreram E o controle passou E o original nao mudou.
"""

import hashlib
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

ESTADO, TESTE, SAIDA = Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve(), Path(sys.argv[3])

MUTANTES = (
    ("r1", "hw-slot nao e envoltorio", '    if b == "hw-slot":\n', "    if False:\n"),
    ("r2", "env -u consome 1 token so", 'if tok[j] in ("-u", "-C", "-S"):\n                j += 2', 'if tok[j] in ("-u", "-C", "-S"):\n                j += 1'),
    ("r3", "bash -c nao abre o script", "            return transformar(tok[k + 1])\n", "            return stmt\n"),
    ("r4", "os separadores somem", '_desembrulhar_stmt(seg) + sep for seg', '_desembrulhar_stmt(seg) + "" for seg'),
    ("r5", "fica sempre com o transformado", "    return b if len(b) >= len(a) else a\n", "    return b\n"),
    ("r6", "timeout nao pula a duracao", "        return j + 1 if j < len(tok) and _DURACAO.fullmatch(tok[j]) else j\n", "        return j\n"),
    ("r7", "taskset -c consome 1 token so", "            return j + 2\n        return j + 1 if j < len(tok) and not", "            return j + 1\n        return j + 1 if j < len(tok) and not"),
    ("r8", "instalar nao troca o analisador", "    crn.analisar_cargo = analisar_cargo_estendido\n", "    pass\n"),
    ("r9", "VAR=valor nao e envoltorio", "    if _VAR.fullmatch(t) or b in", "    if b in"),
    ("r10", "nice -n consome 1 token so", 'j += 2 if tok[j] in ("-n", "-c") else 1', "j += 1"),
    ("r11", "o cd do script de dentro se perde", "    return \"\".join(_desembrulhar_stmt(seg) + sep for seg, sep in crn.dividir(comando", "    return \"\".join(_desembrulhar_stmt(seg).split('&&')[-1] + sep for seg, sep in crn.dividir(comando"),
)


def sha(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def rodar(texto: str) -> dict:
    with tempfile.TemporaryDirectory() as tmp:
        copia = Path(tmp) / "reconhecedor_mut.py"
        copia.write_text(texto, encoding="utf-8")
        env = {**os.environ, "RECONHECEDOR_MODULO": str(copia), "PYTHONDONTWRITEBYTECODE": "1"}
        try:
            r = subprocess.run([sys.executable, "-m", "pytest", "-q", "-p", "no:cacheprovider", str(TESTE)], capture_output=True, text=True, env=env, cwd="/tmp", timeout=90)
        except subprocess.TimeoutExpired:  # o mutante nao termina: o teste NAO passa
            return {"rc": 124, "falhas": ["TIMEOUT"], "resumo": "timeout de 90 s: o mutante nao termina"}
    falhas = [ln.split("::")[-1] for ln in r.stdout.splitlines() if ln.startswith("FAILED")]
    return {"rc": r.returncode, "falhas": falhas, "resumo": (r.stdout.strip().splitlines() or [""])[-1]}


def main() -> int:
    antes, texto = sha(ESTADO), ESTADO.read_text(encoding="utf-8")
    controle = rodar(texto)
    resultados = []
    for nome, descricao, velho, novo in MUTANTES:
        if texto.count(velho) != 1:
            resultados.append({"mutante": nome, "descricao": descricao, "erro": f"trecho aparece {texto.count(velho)}x (esperado 1x): nao aplicou"})
            continue
        r = rodar(texto.replace(velho, novo, 1))
        resultados.append({"mutante": nome, "descricao": descricao, **r, "morto": r["rc"] != 0 and bool(r["falhas"])})
    mortos = sum(1 for r in resultados if r.get("morto"))
    saida = {"estado": str(ESTADO), "sha_antes": antes, "sha_depois": sha(ESTADO), "controle_sem_mutacao": controle, "mutantes_mortos": f"{mortos}/{len(resultados)}", "mutantes": resultados}
    SAIDA.write_text(json.dumps(saida, ensure_ascii=False, indent=1), encoding="utf-8")
    for r in resultados:
        print(f"  {'MORTO ' if r.get('morto') else 'VIVO  '} {r['mutante']} {r['descricao']}: {r.get('erro') or r['falhas'][:3]}")
    ok_ctl = controle["rc"] == 0
    print(f"controle sem mutacao: {'passou' if ok_ctl else 'REPROVOU'} ({controle['resumo']}) | sha do original igual antes/depois: {saida['sha_antes'] == saida['sha_depois']}")
    print(f"{mortos}/{len(resultados)} mutantes mortos")
    return 0 if mortos == len(resultados) and ok_ctl and saida["sha_antes"] == saida["sha_depois"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
