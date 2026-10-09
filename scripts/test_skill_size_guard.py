#!/usr/bin/env python3
"""Teste de guarda — tamanho de SKILL.md acima do corte de compactação.

POR QUE ESTE TESTE EXISTE (medido, 08/10/2026)
  Depois de cada compactação o Claude Code reinjeta as skills invocadas
  truncadas em 20.000 caracteres, com o marcador:
    "[... skill content truncated for compaction; use Read on the skill path...]"
  O que fica DEPOIS do corte o modelo deixa de ver — o procedimento, os
  comandos, as regras de ouro. A Touring tinha 41.661 e voltava pela metade.

REGRA
  SKILL.md ate LIMIJE chars (2.000 de folga sob o corte, para que uma
  edicao pequena nao requebre o limite). Alem disso: REPROVA.

ISENCAO
  ~/.claude/skills/synced/** — vem da sincronizacao do claude.ai e seria
  sobrescrito na proxima sincronizacao. A razao e impressa, nao silenciosa.

COMO PROVAR QUE O TESTE FUNCIONA (controle positivo e negativo)
  Positivo: um SKILL.md sintetico de LIMIJE+1 chars num tmpdir REPROVA.
  Negativo: um de LIMIJE-1 chars PASSA.
  Um teste de guarda que nunca falhou nao distingue "nada grande" de "nao olha".

Uso:
    python3 scripts/test_skill_size_guard.py           # exit 0 se OK
    python3 scripts/test_skill_size_guard.py --verbose # tabela por skill
    python3 -m pytest scripts/test_skill_size_guard.py # coletavel
"""
from __future__ import annotations

import argparse
import sys
import tempfile
from pathlib import Path

try:  # coletavel por pytest, executavel como script (o modelo do test_drift_coverage)
    import pytest
except ImportError:  # pragma: no cover
    pytest = None  # type: ignore[assignment]

LIMITE = 20_000
FOLGA = 2_000  # alvo do gerador = LIMITE - FOLGA

#: As raizes varridas. Os projetos ficam num array no topo, por nome —
#: o mesmo padrao de scripts/test_drift_coverage.py.
SKILLS_DIRS: tuple[Path, ...] = (
    Path.home() / ".claude/skills",
    Path("/home/gabrielgadea/projects/analise/.claude/skills"),
)

#: Prefixos isentos, com a razao — exigida pelo pedido da analise-6b.
ISENCOES: dict[str, str] = {
    "synced": (
        "vem da sincronizacao do claude.ai; seria sobrescrito na proxima "
        "sincronizacao, e mexer aqui nao dura"
    ),
}


def is_isento(caminho: Path, raiz: Path) -> str | None:
    """Razao da isencao, ou None se o caminho e auditado."""
    try:
        rel = caminho.relative_to(raiz)
    except ValueError:
        return None
    for parte in rel.parts[:-1]:
        if parte in ISENCOES:
            return ISENCOES[parte]
    return None


def varrer(raizes: tuple[Path, ...] = SKILLS_DIRS) -> tuple[list[dict], list[dict]]:
    """Retorna (violacoes, isentos). Cada item de violacao tem nome e chars."""
    violacoes: list[dict] = []
    isentos: list[dict] = []
    for raiz in raizes:
        if not raiz.is_dir():
            continue
        # `**` e obrigatorio: `synced/<skill>/SKILL.md` esta DOIS niveis abaixo
        # da raiz. Com `*/SKILL.md` o glob nao alcanca nada de synced — e o
        # teste de isencao passa por ter medido VAZIO (medido 08/10/2026).
        for skill_md in sorted(raiz.glob("**/SKILL.md")):
            nome = skill_md.parent.name
            texto = skill_md.read_text(encoding="utf-8", errors="replace")
            chars = len(texto)
            razao = is_isento(skill_md, raiz)
            if razao:
                isentos.append({"nome": nome, "chars": chars, "razao": razao,
                                "caminho": str(skill_md)})
            elif chars > LIMITE:
                violacoes.append({"nome": nome, "chars": chars, "limite": LIMITE,
                                  "excesso": chars - LIMITE, "caminho": str(skill_md)})
    return violacoes, isentos


# --------------------------------------------------------------------------
# helpers de teste (funcionam com pytest E como script)
# --------------------------------------------------------------------------
def _sintetico(tmp: Path, nome: str, chars: int) -> Path:
    """Um SKILL.md de exatamente `chars` caracteres, com frontmatter valido."""
    d = tmp / nome
    d.mkdir(parents=True, exist_ok=True)
    cabeca = "---\nname: x\ndescription: sintetico\n---\n\n"
    corpo = "x" * max(0, chars - len(cabeca))
    (d / "SKILL.md").write_text(cabeca + corpo, encoding="utf-8")
    return d


def test_nada_acima_do_limite():
    """A varredura real das raizes do usuario passa."""
    violacoes, _ = varrer()
    nomes = ", ".join(f"{v['nome']} ({v['chars']})" for v in violacoes)
    assert not violacoes, f"{len(violacoes)} SKILL.md acima de {LIMITE}: {nomes}"


def test_controle_negativo_um_char_abaixo_passa():
    """LIMIJE-1 chars: tem que passar. Sem isto, um teste que sempre falha tambem 'cumpre'."""
    with tempfile.TemporaryDirectory() as t:
        tmp = Path(t)
        _sintetico(tmp, "ok", LIMITE - 1)
        violacoes, _ = varrer((tmp,))
        assert not violacoes, f"um arquivo de {LIMITE-1} reprovou: {violacoes}"


def test_controle_positivo_um_char_acima_reprova():
    """LIMIJE+1 chars: tem que reprovar. Prova que o teste OLHA o tamanho."""
    with tempfile.TemporaryDirectory() as t:
        tmp = Path(t)
        _sintetico(tmp, "grande", LIMITE + 1)
        violacoes, _ = varrer((tmp,))
        assert violacoes, f"um arquivo de {LIMITE+1} passou: o teste nao mede o tamanho"
        assert violacoes[0]["chars"] == LIMITE + 1
        assert violacoes[0]["excesso"] == 1


def test_isencao_synced_e_impressa_com_razao():
    """A isencao nao e silenciosa: a razao viaja com o registro.

    Reproduz a FORMA REAL (`<raiz>/synced/<skill>/SKILL.md`), nao um atalho
    com `synced` no primeiro nivel — `varrer` globa `*/SKILL.md`, entao um
    `tmp/synced/SKILL.md` nunca seria alcancado e o teste passaria por um
    motivo errado.
    """
    with tempfile.TemporaryDirectory() as t:
        tmp = Path(t)
        _sintetico(tmp / "synced", "grande", LIMITE + 5_000)
        violacoes, isentos = varrer((tmp,))
        assert not violacoes, f"synced deveria estar isento: {violacoes}"
        assert len(isentos) == 1, f"esperava 1 isencao, veio {len(isentos)}"
        assert isentos[0]["nome"] == "grande"
        assert isentos[0]["razao"], "a razao da isencao veio vazia"


def test_isencao_aplica_ao_padrao_real_de_synced():
    """O caminho de verdade (`synced/pptx/SKILL.md`) e isento; um irmao nao."""
    with tempfile.TemporaryDirectory() as t:
        tmp = Path(t)
        _sintetico(tmp / "synced", "do-sync", LIMITE + 5_000)
        _sintetico(tmp, "do-usuario", LIMITE + 5_000)
        violacoes, isentos = varrer((tmp,))
        nomes_i = {i["nome"] for i in isentos}
        nomes_v = {v["nome"] for v in violacoes}
        assert "do-sync" in nomes_i, "synced/<skill>/SKILL.md deveria estar isento"
        assert "do-usuario" in nomes_v, "<skill>/SKILL.md fora do synced deve reprovar"


def test_fronteira_exata_no_limite_passa():
    """Exatamente LIMITE chars passa; e LIMITE+1 que reprova. A fronteira e medidas."""
    with tempfile.TemporaryDirectory() as t:
        tmp = Path(t)
        _sintetico(tmp, "exato", LIMITE)
        violacoes, _ = varrer((tmp,))
        assert not violacoes, f"{LIMITE} chars deveria passar: {violacoes}"


def _rodar() -> int:
    violacoes, isentos = varrer()
    print(f"limite: {LIMITE} chars  ·  alvo do gerador: {FOLGA}")
    print(f"varredas: {len(SKILLS_DIRS)} raizes\n")
    if isentos:
        print("ISENTAS:")
        for i in isentos:
            print(f"  {i['nome']:<26} {i['chars']:>7} chars  — {i['razao']}")
        print()
    if violacoes:
        print(f"VIOLACOES ({len(violacoes)}):")
        for v in violacoes:
            print(f"  {v['nome']:<26} {v['chars']:>7} chars  (+{v['excesso']} sobre o limite)")
        return 1
    print("OK: nenhuma SKILL.md acima do limite.")
    return 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--verbose", action="store_true", help="mostra o tamanho de todas")
    args = ap.parse_args(argv)
    if args.verbose:
        for raiz in SKILLS_DIRS:
            if not raiz.is_dir():
                continue
            for p in sorted(raiz.glob("*/SKILL.md")):
                n = len(p.read_text(encoding="utf-8", errors="replace"))
                marca = "ISENT" if is_isento(p, raiz) else ("ACIMA" if n > LIMITE else "ok")
                print(f"  {marca:<6} {n:>7}  {p.parent.name}")
    return _rodar()


if __name__ == "__main__":
    raise SystemExit(main())