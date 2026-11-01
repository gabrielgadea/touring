#!/usr/bin/env python3
"""JUIZ-TIMEOUT — o juiz declara INCONCLUSIVO quando a camada 2 estourou o prazo.

A regra está em `PREREGISTRO-JUIZ-TIMEOUT-2026-10-29.md`; este teste é a prova dela.

Cenários (o juiz roda como SUBPROCESSO — o exit code é o veredito):

  1. ABAIXO DO PATAMAR   -> veredito normal (exit 0/1, nunca 3)
  2. ACIMA DO PATAMAR    -> INCONCLUSIVO (exit 3) **mesmo com o critério de falso deny passando**
                            (o MESMO corpus sem timeout dá PRONTO: a única diferença é a taxa)
  3. SINAL AUSENTE       -> INCONCLUSIVO (exit 3): o motor não expõe o estado da camada 2
  4. SEM CAMADA 2 PEDIDA -> a regra não se aplica (exit != 3): o veredito de V2 é byte a byte o de sempre
  5. MUTANTES            -> dois mutantes do juiz, e os testes têm de MATAR os dois

O motor falso é parametrizado pelo ambiente (`FAKE_MODO`, `FAKE_TIMEOUTS`): em `timeout` ele
acrescenta o estado que o motor real devolve quando a camada 2 estoura
(`camada2: "unknown: prazo de … estourado …"`), em `sem_estado` ele não expõe `layer` nem
`camada2` (o juiz não tem como saber que a camada 2 rodou), em `bom` ele reporta `layer: 2`
como o release corrente. No modo `timeout` o motor FALSO **mantém** os achados: o que se prova
aqui é a REGRA do veredito, não a fidelidade do motor.

Uso: python3 test_juiz_timeout.py [-v]
"""

from __future__ import annotations

import importlib.util
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

JUIZ = Path(__file__).resolve().parent / "juiz_motor_vgp.py"
VERBOSO = "-v" in sys.argv

# O motor falso: o release corrente responde `layer: 2` e, quando a camada 2 estoura o prazo,
# acrescenta o estado `camada2` — e o veredito `allow` SEM achado (é o que perde denies).
MOTOR_FALSO = r"""#!/usr/bin/env python3
import json, os, sys

a = sys.argv[1:]
if a[:3] == ["vgp", "check", "--help"]:
    sys.exit(0)
conteudo = open(a[a.index("--content-file") + 1]).read()
nome = conteudo.split("#ESPERADO=")[1].split()[0] if "#ESPERADO=" in conteudo else None
achados = []
if nome:
    achados.append({"classe": "atributo", "nome": nome, "linha": 1, "severidade": "deny"})
modo = os.environ.get("FAKE_MODO", "bom")
saida = {"verdict": "deny" if achados else "allow", "findings": achados, "elapsed_ms": 2.0}
if modo in ("bom", "timeout"):
    saida["layer"] = 2
if modo == "timeout":
    contador = os.environ["FAKE_CONTADOR"]
    n = int(open(contador).read() or 0) if os.path.exists(contador) else 0
    with open(contador, "w") as f:
        f.write(str(n + 1))
    if n < int(os.environ.get("FAKE_TIMEOUTS", "0")):
        saida["camada2"] = "unknown: prazo de 1000 ms estourado esperando publishDiagnostics"
print(json.dumps(saida))
"""


def carregar_juiz(caminho: Path):
    """Carrega o juiz de um caminho arbitrário (o mutante é um juiz com o texto trocado)."""
    spec = importlib.util.spec_from_file_location("juiz_under_test", caminho)
    modulo = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(modulo)
    return modulo


def corpus_sintetico(raiz: Path) -> Path:
    """O corpus sintético do PRÓPRIO juiz (10 positivos de símbolo + 130 negativos)."""
    return carregar_juiz(JUIZ).corpus_sintetico(raiz)


def rodar(
    juiz: Path,
    corpus: Path,
    motor: Path,
    *,
    modo: str,
    timeouts: int = 0,
    contador: Path | None = None,
    camadas: bool = True,
    json_saida: Path | None = None,
) -> subprocess.CompletedProcess:
    cmd = [
        sys.executable,
        str(juiz),
        "--touring",
        str(motor),
        "--corpus",
        str(corpus),
        "--v5-estrito",
    ]
    if camadas:
        cmd += ["--arg-motor=--camadas", "--arg-motor=1,2"]
    if json_saida:
        cmd += ["--json", str(json_saida)]
    env = dict(os.environ)
    env["FAKE_MODO"] = modo
    env["FAKE_TIMEOUTS"] = str(timeouts)
    if contador:
        env["FAKE_CONTADOR"] = str(contador)
    return subprocess.run(cmd, capture_output=True, text=True, env=env, timeout=600, check=False)


def mutar(origem: Path, destino: Path, velho: str, novo: str) -> None:
    """Cópia do juiz com UMA troca. Se a troca não ocorrer uma vez, o mutante é no-op — o que
    faria o teste 'matar' um mutante que não existe (rc=0 sem sinal é a assinatura do erro)."""
    texto = origem.read_text()
    if texto.count(velho) != 1:
        raise SystemExit(f"MUTANTE INVÁLIDO: {velho!r} ocorre {texto.count(velho)}× (esperado 1)")
    destino.write_text(texto.replace(velho, novo))
    if destino.read_text() == texto:
        raise SystemExit(f"MUTANTE INVÁLIDO: {velho!r} -> {novo!r} não mudou nada")


class Registro:
    """Cada cenário devolve (passou, detalhe)."""

    def __init__(self) -> None:
        self.falhas: list[str] = []
        self.n = 0

    def checa(self, nome: str, condicao: bool, detalhe: str = "") -> None:
        self.n += 1
        marca = "PASS" if condicao else "FALHA"
        print(f"  {marca}  {nome}" + (f" · {detalhe}" if detalhe and (VERBOSO or not condicao) else ""))
        if not condicao:
            self.falhas.append(f"{nome}: {detalhe}")


def _ultima_linha(r: subprocess.CompletedProcess) -> str:
    linhas = [l for l in r.stdout.strip().splitlines() if l.strip()]
    return linhas[-1] if linhas else ""


def _linhas(r: subprocess.CompletedProcess,Agulha: str) -> list[str]:
    return [l for l in r.stdout.splitlines() if Agulha in l]


def cenario_abaixo(reg: Registro, corpus: Path, motor: Path, js: Path, raiz: Path) -> None:
    """Abaixo do patamar: o veredito é o de sempre (e PRONTO, porque o critério passa)."""
    r = rodar(
        juiz=JUIZ,
        corpus=corpus,
        motor=motor,
        modo="bom",
        timeouts=10,
        contador=raiz / "c1",
        json_saida=js,
    )
    v = json.loads(js.read_text())
    reg.checa(
        "abaixo do patamar: exit != 3 (veredito normal)",
        r.returncode in (0, 1),
        f"rc={r.returncode} · {_ultima_linha(r)}",
    )
    reg.checa(
        "abaixo do patamar: inconclusive = False no JSON",
        v.get("inconclusivo") is False,
        json.dumps({k: v.get(k) for k in ("inconclusivo", "timeouts_camada2", "chamadas")}),
    )
    reg.checa(
        "abaixo do patamar: o relatório mostra timeouts/denominador/patamar",
        "timeouts da camada 2:" in r.stdout and "patamar 20%" in r.stdout,
        _linhas(r, "timeouts da camada 2"),
    )
    reg.checa(
        "abaixo do patamar: PRONTO (o critério de falso deny passa)",
        r.returncode == 0 and v.get("pronto") is True,
        f"rc={r.returncode} pendencias={v.get('pendencias')}",
    )


def cenario_acima(reg: Registro, corpus: Path, motor: Path, js: Path, raiz: Path) -> None:
    """Acima do patamar: INCONCLUSIVO — e o falso deny NÃO é a pendência (o critério passa)."""
    r = rodar(
        juiz=JUIZ,
        corpus=corpus,
        motor=motor,
        modo="timeout",
        timeouts=40,
        contador=raiz / "c2",
        json_saida=js,
    )
    v = json.loads(js.read_text())
    pendencias = v.get("pendencias") or []
    reg.checa(
        "acima do patamar: exit 3 (INCONCLUSIVO)",
        r.returncode == 3,
        f"rc={r.returncode} · {_ultima_linha(r)}",
    )
    reg.checa(
        "acima do patamar: veredito: INCONCLUSIVO no relatório",
        "veredito: INCONCLUSIVO" in r.stdout,
        _linhas(r, "veredito"),
    )
    reg.checa(
        "acima do patamar: NUNCA PRONTO (pronto = False no JSON)",
        v.get("pronto") is False,
        f"pronto={v.get('pronto')}",
    )
    reg.checa(
        "acima do patamar: o motivo cita a taxa e o patamar",
        "patamar 20%" in " ".join(pendencias),
        pendencias,
    )
    reg.checa(
        "acima do patamar: a contagem de timeouts bate com a do motor (40)",
        v.get("timeouts_camada2") == 40,
        f"timeouts={v.get('timeouts_camada2')} chamadas={v.get('chamadas')} taxa={v.get('taxa_timeouts_camada2')}",
    )
    reg.checa(
        "acima do patamar: o falso deny NÃO é pendência (o critério passa; só o timeout decide)",
        not any(p.startswith("falso deny (") for p in pendencias),
        pendencias,
    )


def cenario_sinal_ausente(reg: Registro, corpus: Path, motor: Path, js: Path) -> None:
    """O motor não expõe o estado da camada 2: sinal ausente ≠ zero ⇒ INCONCLUSIVO."""
    r = rodar(juiz=JUIZ, corpus=corpus, motor=motor, modo="sem_estado", json_saida=js)
    v = json.loads(js.read_text())
    reg.checa(
        "sinal ausente: exit 3 (INCONCLUSIVO)",
        r.returncode == 3,
        f"rc={r.returncode} · {_ultima_linha(r)}",
    )
    reg.checa(
        "sinal ausente: o motivo diz que sinal ausente não é zero",
        any("sinal ausente não é zero" in p for p in (v.get("pendencias") or [])),
        v.get("pendencias"),
    )
    reg.checa(
        "sinal ausente: sinal = ausente no JSON",
        v.get("sinal_camada2") == "ausente",
        f"sinal={v.get('sinal_camada2')}",
    )


def cenario_zero_chamadas(reg: Registro, corpus: Path, motor: Path, js: Path, raiz: Path) -> None:
    """Corpus sem entrada: o juiz não chama o motor, não há denominador ⇒ nunca PRONTO.

    Acrescentado na revisão (touring-cb, 29/10): o mutante que troca o `ausente` deste ramo por
    `presente` sobrevivia aos 5 cenários originais.
    """
    import shutil

    vazio = raiz / "corpus_vazio"
    shutil.copytree(corpus, vazio)
    for nome in ("entradas.jsonl", "entradas_inline_neg.jsonl"):
        (vazio / nome).write_text("")
    js.unlink(missing_ok=True)
    r = rodar(juiz=JUIZ, corpus=vazio, motor=motor, modo="bom", json_saida=js)
    reg.checa(
        "zero chamadas: nunca PRONTO (exit != 0)",
        r.returncode != 0,
        f"rc={r.returncode} · {_ultima_linha(r)}",
    )
    v = json.loads(js.read_text()) if js.exists() else {}
    reg.checa(
        "zero chamadas: sinal = ausente (sem denominador)",
        v.get("sinal_camada2") == "ausente",
        f"rc={r.returncode} · sinal={v.get('sinal_camada2')} · {_ultima_linha(r)}",
    )


def cenario_sem_camada2(reg: Registro, corpus: Path, motor: Path, js: Path, raiz: Path) -> None:
    """Run que não pede a camada 2: a regra não se aplica (o veredito de V2 é o de sempre)."""
    r = rodar(
        juiz=JUIZ,
        corpus=corpus,
        motor=motor,
        modo="timeout",
        timeouts=40,
        contador=raiz / "c4",
        camadas=False,
        json_saida=js,
    )
    v = json.loads(js.read_text())
    reg.checa(
        "sem camada 2 pedida: exit != 3 (o veredito de V2 é o de sempre)",
        r.returncode in (0, 1),
        f"rc={r.returncode}",
    )
    reg.checa(
        "sem camada 2 pedida: sinal = nao_aplicavel",
        v.get("sinal_camada2") == "nao_aplicavel",
        f"sinal={v.get('sinal_camada2')}",
    )


# (nome, troca no texto do juiz, cenário que tem de pegá-lo)
MUTANTES = (
    (
        "comparação do patamar invertida",
        "taxa > PATAMAR_TIMEOUTS_CAMADA2",
        "taxa < PATAMAR_TIMEOUTS_CAMADA2",
        "acima do patamar",
    ),
    (
        "ausência tratada como zero",
        'if sinal == "ausente":',
        "if False:",
        "sinal ausente",
    ),
)


def cenario_mutantes(reg: Registro, corpus: Path, motor: Path, js: Path, raiz: Path) -> None:
    """Cada mutante do juiz tem de ser PEGO pelo cenário correspondente (e não pode ser no-op)."""
    for nome, velho, novo, cenario in MUTANTES:
        caminho = raiz / (nome.replace(" ", "_") + ".py")
        mutar(JUIZ, caminho, velho, novo)
        if cenario == "acima do patamar":
            r = rodar(
                juiz=caminho,
                corpus=corpus,
                motor=motor,
                modo="timeout",
                timeouts=40,
                contador=raiz / ("cm_" + nome.replace(" ", "_")),
                json_saida=js,
            )
        else:
            r = rodar(juiz=caminho, corpus=corpus, motor=motor, modo="sem_estado", json_saida=js)
        v = json.loads(js.read_text())
        pego = r.returncode != 3 or v.get("inconclusivo") is not True
        reg.checa(
            f"mutante morto: {nome} (cenário: {cenario})",
            pego,
            f"rc={r.returncode} (esperado 3) · inconclusivo={v.get('inconclusivo')}",
        )


def main() -> int:
    reg = Registro()
    with tempfile.TemporaryDirectory() as tmp:
        raiz = Path(tmp)
        corpus = corpus_sintetico(raiz)
        motor = raiz / "motor_falso"
        motor.write_text(MOTOR_FALSO)
        motor.chmod(0o755)
        js = raiz / "veredito.json"
        cenario_abaixo(reg, corpus, motor, js, raiz)
        cenario_acima(reg, corpus, motor, js, raiz)
        cenario_sinal_ausente(reg, corpus, motor, js)
        cenario_zero_chamadas(reg, corpus, motor, js, raiz)
        cenario_sem_camada2(reg, corpus, motor, js, raiz)
        cenario_mutantes(reg, corpus, motor, js, raiz)

    print(f"\n{reg.n - len(reg.falhas)}/{reg.n} cenários do JUIZ-TIMEOUT")
    for f in reg.falhas:
        print(f"  FALHA {f}")
    print("TODOS OS TESTES PASSARAM" if not reg.falhas else "REPROVOU")
    return 0 if not reg.falhas else 1


def test_cenarios_do_juiz_timeout():
    """Os cenários, como o pytest os ve.

    MEDIDO em 01/11 (V-8): este arquivo tinha so `main()`, e o pytest NAO o coletava — o
    `test_todo_teste_e_coletavel` reprovava por causa disso. Um `no tests ran` com exit 5 e lido
    por varios CI como "nada a fazer": o pipeline fica VERDE sem rodar o teste, que e exatamente
    o defeito que o proprio arquivo do meta-teste descreve.
    """
    assert main() == 0, (
        "os cenários do JUIZ-TIMEOUT reprovaram: rode `python3 scripts/test_juiz_timeout.py`"
    )


def test_l2_budget_flag_chega_ao_motor(tmp_path, monkeypatch):
    corpus = corpus_sintetico(tmp_path)
    motor = tmp_path / "motor_falso"
    motor.write_text(MOTOR_FALSO.replace(
        'a = sys.argv[1:]',
        'a = sys.argv[1:]\nwith open(os.environ["FAKE_ARGV"], "a") as f:\n    f.write(json.dumps(a) + "\\n")',
    ))
    motor.chmod(0o755)
    argv_log = tmp_path / "argv.jsonl"
    monkeypatch.setenv("FAKE_ARGV", str(argv_log))
    r = subprocess.run(
        [sys.executable, str(JUIZ), "--touring", str(motor), "--corpus", str(corpus), "--v5-estrito", "--l2-budget-ms", "30000"],
        capture_output=True,
        text=True,
        timeout=600,
        check=False,
    )
    assert r.returncode == 0, r.stdout + r.stderr
    chamadas = [json.loads(line) for line in argv_log.read_text().splitlines()]
    chamadas = [c for c in chamadas if c[:3] != ["vgp", "check", "--help"]]
    assert chamadas and all(c[-3:-1] == ["--l2-budget-ms", "30000"] for c in chamadas)


if __name__ == "__main__":
    sys.exit(main())
