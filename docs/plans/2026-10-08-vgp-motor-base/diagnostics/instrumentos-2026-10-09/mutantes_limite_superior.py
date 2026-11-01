"""Mutantes do limite superior do juiz: cada um e aplicado a uma COPIA de `juiz_motor_vgp.py` (o original nunca e escrito;
o sha e conferido antes e depois) e o `--selftest` da copia TEM de reprovar. Um mutante que o selftest deixa passar
e um buraco no selftest, nao no mutante.

  m1  taxa pontual no lugar do limite superior
  m2  limite BILATERAL (alfa/2) no lugar do unilateral
  m3  a flag `--exigir-limite-superior` nao gera pendencia (pendencias_limite desligada)
  m4  `main()` ignora a flag (nao a repassa a `julgar`)
  m5  a flag vale sempre (o veredito sem a flag deixa de ser o de antes)
  m6  a pendencia nao diz quantos falsos o n permitiria
  m7  a bissecao do limite para cedo (2 iteracoes em vez de 80)
  m8  teto frouxo no limite (2x o teto de falso deny)

Uso: python3 -I mutantes_limite_superior.py <juiz_motor_vgp.py> <saida.json>
Exit 0 so se todos os mutantes morreram E a copia SEM mutacao passou (controle: o selftest nao reprova por outro motivo).
"""

import hashlib
import json
import subprocess
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

JUIZ, SAIDA = Path(sys.argv[1]).resolve(), Path(sys.argv[2])

LIMITE_DO_BALDE = "    return limite_superior(falsos, n, CONF_LIMITE)\n"
GATILHO_FLAG = "    if exigir_limite:\n        pend += pendencias_limite(m)\n"
# (nome, descricao, trecho original, trecho mutado, fragmento que a saida do selftest reprovado tem de trazer)
MUTANTES = (
    ("m1", "taxa pontual no lugar do limite", LIMITE_DO_BALDE, "    return falsos / n\n", "FALHA limite superior 95% de 0/600"),
    (
        "m2",
        "bilateral (alfa/2)",
        LIMITE_DO_BALDE,
        "    return limite_superior(falsos, n, 1.0 - (1.0 - CONF_LIMITE) / 2)\n",
        "FALHA limite superior 95% de",
    ),
    ("m3", "a flag nao gera pendencia", GATILHO_FLAG, "    if exigir_limite and False:\n        pend += pendencias_limite(m)\n", "5/600 e 2/600 com a flag"),
    (
        "m4",
        "main ignora a flag",
        "julgar(touring, a.corpus, a.v5_estrito, a.exigir_limite_superior)",
        "julgar(touring, a.corpus, a.v5_estrito)",
        "FALHA CLI com a flag",
    ),
    ("m5", "a flag vale sempre", GATILHO_FLAG, "    if True:\n        pend += pendencias_limite(m)\n", "sem a flag"),
    (
        "m6",
        "pendencia sem o n permitido",
        '            pend.append(f"{prefixo} — com n={n} o teto admite no máximo {perm} falso(s)")\n',
        '            pend.append(f"{prefixo}")\n',
        "falta «com n=600 o teto admite no máximo 1 falso(s)»",
    ),
    ("m7", "bissecao curta", "    for _ in range(80):\n", "    for _ in range(2):\n", "FALHA limite superior 95% de"),
    ("m8", "teto frouxo", '"limite_ate_o_teto": lim <= LIMIAR_FALSO_DENY,', '"limite_ate_o_teto": lim <= 2 * LIMIAR_FALSO_DENY,', "5/600 e 2/600 com a flag"),
)


def sha(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def rodar(nome: str, descricao: str, velho: str, novo: str, esperado: str, texto: str) -> dict:
    """Aplica UMA mutacao a uma copia no tmp e roda o selftest dela (timeout 600 s)."""
    if velho is not None and texto.count(velho) != 1:
        return {"mutante": nome, "descricao": descricao, "erro": f"o trecho original aparece {texto.count(velho)}x (esperado 1x): mutante nao aplicou"}
    with tempfile.TemporaryDirectory() as tmp:
        copia = Path(tmp) / "juiz_mut.py"
        copia.write_text(texto.replace(velho, novo, 1) if velho is not None else texto, encoding="utf-8")
        assert copia != JUIZ
        r = subprocess.run([sys.executable, "-I", str(copia), "--selftest"], capture_output=True, text=True, timeout=600)
    reprovou = "selftest: REPROVOU" in r.stdout
    traz = esperado in r.stdout
    falhas = [ln.strip() for ln in r.stdout.splitlines() if "FALHA" in ln and "motor " not in ln][:3]
    return {"mutante": nome, "descricao": descricao, "rc": r.returncode, "reprovou_no_selftest": reprovou,
            "saida_traz_o_fragmento_esperado": traz, "fragmento": esperado, "primeiras_falhas": falhas,
            "morto": r.returncode != 0 and reprovou and traz, "stderr": r.stderr[-300:]}


def main() -> int:
    antes, texto = sha(JUIZ), JUIZ.read_text(encoding="utf-8")
    jobs = [(n, d, v, nv, e, texto) for n, d, v, nv, e in MUTANTES]
    controle = ("controle", "copia SEM mutacao: o selftest tem de PASSAR", None, None, "selftest: 9/9", texto)
    with ThreadPoolExecutor(max_workers=2) as pool:
        res = list(pool.map(lambda j: rodar(*j), [controle, *jobs]))
    ctl, muts = res[0], res[1:]
    ctl_ok = ctl.get("rc") == 0 and ctl.get("saida_traz_o_fragmento_esperado") is True
    mortos = sum(1 for m in muts if m.get("morto"))
    saida = {"juiz": str(JUIZ), "sha_antes": antes, "sha_depois": sha(JUIZ), "controle_sem_mutacao": {"rc": ctl.get("rc"), "passou": ctl_ok},
             "mutantes_mortos": f"{mortos}/{len(muts)}", "mutantes": muts}
    SAIDA.write_text(json.dumps(saida, ensure_ascii=False, indent=1), encoding="utf-8")
    for m in muts:
        print(f"  {'MORTO ' if m.get('morto') else 'VIVO  '} {m['mutante']} {m['descricao']}: {m.get('erro') or m['primeiras_falhas'][:1]}")
    print(f"controle sem mutacao: {'passou' if ctl_ok else 'REPROVOU (rc ' + str(ctl.get('rc')) + ')'} | sha do juiz antes/depois iguais: {saida['sha_antes'] == saida['sha_depois']}")
    print(f"{mortos}/{len(muts)} mutantes mortos")
    return 0 if mortos == len(muts) and ctl_ok and saida["sha_antes"] == saida["sha_depois"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
