"""Mutantes do estado no instante completo (HO13): cada um e aplicado a uma COPIA de `estado_no_instante_v0d.py`
(o original nunca e escrito; sha conferido antes e depois) e `test_estado_no_instante_v0d.py` (via $ESTADO_MODULO) TEM de
reprovar. O controle (copia sem mutacao) tem de passar — senao o teste reprova por outro motivo.

  m1  «so o __init__»: o `from pkg import mod` nao traz o submodulo (o defeito do P0M, 6a25b00f2584)
  m2  os pacotes ancestrais de `import a.b.c` nao entram
  m3  relativo sem o `__init__` do pacote corrente
  m4  `estado_completo` ignora o modulo que nao reconstroi
  m5  `overlays_e_faltas` esconde a falta (volta ao silencio do legado)
  m6  sem a segunda base (o diretorio do script)
  m7  aceita modulo fora da raiz do projeto
  m8  `modulos_do_projeto` volta a regra antiga
  m9  o overlay grava o conteudo de HOJE (o arquivo em disco) em vez do reconstruido no instante

Uso: python3 -I mutantes_estado_no_instante.py <estado_no_instante_v0d.py> <test_estado_no_instante_v0d.py> <saida.json>
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
    ("m1", "so o __init__ (sem o submodulo do from-import)", "        for nome in nomes:\n", "        for nome in ():\n"),
    (
        "m2",
        "sem os pacotes ancestrais",
        '        if init.is_file():\n            out[init] = "direto" if alvo else "ancestral"\n',
        '        if init.is_file() and alvo:\n            out[init] = "direto"\n',
    ),
    ("m3", "relativo sem o __init__ do pacote corrente", "        if nivel:  # o pacote corrente", "        if False:  # o pacote corrente"),
    (
        "m4",
        "estado_completo ignora a falta",
        "        if corpus_v0d._conteudo_depois(str(mod), entrada[\"ts\"], indice) is None\n",
        "        if corpus_v0d._conteudo_depois(str(mod), entrada[\"ts\"], indice) is None and False\n",
    ),
    ("m5", "overlays_e_faltas esconde a falta", "            faltas.append(str(mod))\n", "            pass\n"),
    ("m6", "sem a 2a base (dir do script)", "else [raiz, arquivo.parent]", "else [raiz]"),
    ("m7", "aceita modulo fora da raiz", "            if raiz not in f.parents or f == arquivo.resolve():\n", "            if f == arquivo.resolve():\n"),
    ("m8", "volta a regra antiga", "    return list(modulos_tipados(entrada, texto))\n", "    return modulos_do_projeto_legado(entrada, texto)\n"),
    (
        "m9",
        "overlay grava o arquivo de HOJE",
        "        arq.write_text(conteudo)\n        out.append(",
        "        arq.write_text(Path(mod).read_text())\n        out.append(",
    ),
    ("m10", "profundidade 2 ignora o que ja esta coberto", 'cobertos = {str(Path(o["path"]).resolve()) for o in over}', "cobertos = set()"),
    ("m11", "profundidade 2 conta a propria entrada como lacuna", '        cobertos.add(str(Path(e["path"]).resolve()))', "        pass"),
)


def sha(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def rodar(texto: str, esperado_passar: bool) -> dict:
    with tempfile.TemporaryDirectory() as tmp:
        copia = Path(tmp) / "estado_mut.py"
        copia.write_text(texto, encoding="utf-8")
        env = {**os.environ, "ESTADO_MODULO": str(copia), "PYTHONDONTWRITEBYTECODE": "1"}
        r = subprocess.run([sys.executable, "-m", "pytest", "-q", "-p", "no:cacheprovider", str(TESTE)], capture_output=True, text=True, env=env, cwd="/tmp", timeout=300)
    falhas = [ln.split("::")[-1] for ln in r.stdout.splitlines() if ln.startswith("FAILED")]
    return {"rc": r.returncode, "falhas": falhas, "resumo": (r.stdout.strip().splitlines() or [""])[-1]}


def main() -> int:
    antes, texto = sha(ESTADO), ESTADO.read_text(encoding="utf-8")
    controle = rodar(texto, True)
    resultados = []
    for nome, descricao, velho, novo in MUTANTES:
        if texto.count(velho) != 1:
            resultados.append({"mutante": nome, "descricao": descricao, "erro": f"trecho aparece {texto.count(velho)}x (esperado 1x): nao aplicou"})
            continue
        r = rodar(texto.replace(velho, novo, 1), False)
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
