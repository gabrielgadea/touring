#!/usr/bin/env python3
"""O portfólio de fluxos ADW como GRAFO — fluxos × peças, medido, nunca narrado.

Um portfólio que não se mede é uma pasta de arquivos. Este script lê os fluxos
(biblioteca `~/.claude/skills/Touring/adw-library` + projeto `.touring/adw`,
projeto vence por nome) e os fragmentos dos dois `fragments/`, e responde às
quatro perguntas que sustentam o «menor grafo» de Isenberg:

    lint          o portfólio está CONFORME? (parseia · todo fluxo com veredito
                  de prior-art · nenhum uso de peça inexistente; órfão é ⚠,
                  nunca reprova — REGRA #0 pede consumidor, não deleção)
    monolithic    quais fluxos não compõem peça alguma? (0 `[[use]]`)
    orphans       quais peças nenhum fluxo usa?
    reimplemented quais nós de fluxo REIMPLEMENTAM um nó de fragmento?
                  (assinatura = corpo normalizado — ver `_assinaturas`)
    mermaid       o grafo desenhado (nível 1 de Isenberg: desenhe antes de
                  automatizar) — fluxos `-->` peças usadas, `==>` ancestral
                  de prior-art

Origem: bundle `docs/plans/2026-08-23-graph-engineering` (rodada 2 sobre
Isenberg/Pocock/Jay E/DGM). RECONSTRUÍDO em 23/08/2026: o original morreu com o
scratchpad da sessão interrompida sem nunca ter pousado neste caminho — as
saídas gravadas em `measurements/*.depois.txt` e `portfolio.mmd` são o gabarito
de fidelidade desta reconstrução, e `measurements/assin.py` fixa a API pública
(`Portfolio(root)` com `.fluxos`/`.fragmentos`, itens com campo `arquivo`).

O veredito é o exit code: 0 CONFORME · 1 NÃO CONFORME · 2 erro de uso.
"""
from __future__ import annotations

import argparse
import re
import sys
import tomllib
from pathlib import Path

LIB = Path.home() / ".claude" / "skills" / "Touring" / "adw-library"

#: Arquivos da biblioteca que não são fluxos.
NAO_FLUXO = {"tiers"}

#: Abaixo disto um comando não é distintivo o bastante para que a repetição
#: signifique alguma coisa (`echo ok`, `true`). Medido: com o corpo inteiro a
#: maior colisão cai de 25 nós para 2.
MIN_ASSINATURA = 40


def _assinaturas(nos: dict) -> dict[str, str]:
    """A assinatura de um nó é o CORPO normalizado do que ele executa.

    A primeira versão usava executável + flags longas e colapsava: 25 dos 65 nós
    do portfólio viraram a mesma string `code:bash --`, porque todo nó `bash -c`
    carrega o trabalho no terceiro argumento. O detector devolveu 150 achados e
    quase todos eram falsos.

    Medido depois da correção: 57 assinaturas distintas para 60 nós, maior colisão
    de 2 — e as três que sobraram são repetições REAIS de `recall-pack`,
    `diagnose-pack` e `conflict-guard`. É a disciplina que este acervo já pagou
    para aprender: heurística se mede antes de virar regra, porque reprovar
    trabalho sadio ensina o operador a ignorar o gate.
    """
    out = {}
    for nome, no in (nos or {}).items():
        if not isinstance(no, dict):
            continue
        cmd, prompt = no.get("command"), no.get("prompt")
        if isinstance(cmd, list) and cmd:
            corpo = " ".join(str(c) for c in cmd)
        elif prompt:
            corpo = str(prompt)
        else:
            continue
        # `{{...}}` vira marcador: duas instâncias do MESMO trabalho não devem
        # parecer trabalhos diferentes só por ligarem outra variável.
        corpo = re.sub(r"\{\{[^}]+\}\}", "«var»", corpo)
        corpo = " ".join(corpo.lower().split())
        if len(corpo) >= MIN_ASSINATURA:
            out[nome] = f"{no.get('type', '?')}:{corpo}"
    return out


def _id(nome: str) -> str:
    """Identificador mermaid: tudo que não é [A-Za-z0-9] vira `_`."""
    return re.sub(r"[^A-Za-z0-9]", "_", nome)


class Portfolio:
    """O portfólio carregado: fluxos e fragmentos dos DOIS níveis, projeto vence.

    `fluxos` e `fragmentos` são dicts nome → registro; todo registro carrega
    `arquivo` (o path do TOML — a API que `measurements/assin.py` consome),
    `origem` (`library`|`project`), `nos` (dict bruto dos `[node.*]`), `usos`
    (lista dos `module` de cada `[[use]]`, na ordem do arquivo), `verdict` e
    `supersedes` (nomes canônicos com alias de leitura — ver `_purpose`).
    `erros` guarda todo TOML que não parseou; `arquivos_varridos` conta os
    arquivos regulares dos quatro diretórios.
    """

    def __init__(self, root: Path):
        self.root = root
        self.fluxos: dict[str, dict] = {}
        self.fragmentos: dict[str, dict] = {}
        self.erros: list[str] = []
        self.arquivos_varridos = 0
        proj = root / ".touring" / "adw"
        # library primeiro; o projeto entra depois e SOBRESCREVE por nome —
        # a mesma precedência do runner (`from-template` copia, projeto manda).
        for origem, base in (("library", LIB), ("project", proj)):
            self._varrer(origem, base)

    # ------------------------------------------------------------------ carga
    def _varrer(self, origem: str, base: Path) -> None:
        if base.is_dir():
            for f in sorted(base.iterdir()):
                if f.is_file():
                    self.arquivos_varridos += 1
                    if f.suffix == ".toml" and f.stem not in NAO_FLUXO:
                        self._carregar(self.fluxos, origem, f)
        frags = base / "fragments"
        if frags.is_dir():
            for f in sorted(frags.iterdir()):
                if f.is_file():
                    self.arquivos_varridos += 1
                    if f.suffix == ".toml":
                        self._carregar(self.fragmentos, origem, f)

    def _carregar(self, destino: dict, origem: str, f: Path) -> None:
        try:
            data = tomllib.loads(f.read_text(encoding="utf-8"))
        except (OSError, tomllib.TOMLDecodeError) as err:
            self.erros.append(f"{f.name} ({origem}): {err}")
            return
        purpose = data.get("purpose") or {}
        destino[f.stem] = {
            "arquivo": str(f),
            "origem": origem,
            "nos": data.get("node") or {},
            "usos": [str(u.get("module", "")) for u in (data.get("use") or [])
                     if isinstance(u, dict)],
            # `touring adw new` grava `prior_art_verdict`; `verdict` é aceito
            # como alias de LEITURA para não perder fluxos escritos à mão.
            "verdict": purpose.get("prior_art_verdict") or purpose.get("verdict", ""),
            "supersedes": (purpose.get("prior_art_ancestor") or purpose.get("supersedes")
                           or purpose.get("extends", "")),
        }

    # ------------------------------------------------------------- consultas
    def orfaos(self) -> list[str]:
        usadas = {m for fl in self.fluxos.values() for m in fl["usos"]}
        return sorted(n for n in self.fragmentos if n not in usadas)

    def monoliticos(self) -> list[tuple[str, int, str]]:
        """(nome, nós próprios, origem) dos fluxos sem `[[use]]` algum —
        ordenados do maior para o menor (o maior monólito é a maior dívida)."""
        linhas = [(n, len(fl["nos"]), fl["origem"])
                  for n, fl in self.fluxos.items() if not fl["usos"]]
        return sorted(linhas, key=lambda x: (-x[1], x[0]))

    def reimplementados(self) -> list[tuple[str, str, str, str]]:
        """(fluxo, nó, fragmento, nó) para cada nó de fluxo cuja assinatura é a
        de um nó de fragmento — a cópia que deveria ser um `[[use]]`."""
        por_assin: dict[str, tuple[str, str]] = {}
        for fnome, frag in self.fragmentos.items():
            for nnome, assin in _assinaturas(frag["nos"]).items():
                por_assin[assin] = (fnome, nnome)
        achados = []
        for xnome, fl in sorted(self.fluxos.items()):
            for nnome, assin in _assinaturas(fl["nos"]).items():
                if assin in por_assin:
                    achados.append((xnome, nnome, *por_assin[assin]))
        return achados

    def _ordem_arestas(self) -> list[str]:
        """Projeto primeiro, alfabético; depois library, alfabético — a ordem em
        que o gabarito `portfolio.mmd` foi gravado."""
        proj = sorted(n for n, f in self.fluxos.items() if f["origem"] == "project")
        lib = sorted(n for n, f in self.fluxos.items() if f["origem"] == "library")
        return proj + lib


# ---------------------------------------------------------------- subcomandos
def cmd_lint(pf: Portfolio) -> int:
    print(f"portfólio: {len(pf.fluxos)} fluxos · {len(pf.fragmentos)} peças · "
          f"{pf.arquivos_varridos} arquivos varridos")
    duras: list[str] = []
    for e in pf.erros:
        duras.append(f"TOML não parseia: {e}")
    conhecidas = set(pf.fragmentos)
    for nome, fl in sorted(pf.fluxos.items()):
        if not fl["verdict"]:
            duras.append(f"fluxo `{nome}`: sem `[purpose] prior_art_verdict` "
                         "(por que ESTE fluxo existe em vez de outro?)")
        for m in fl["usos"]:
            if m not in conhecidas:
                duras.append(f"fluxo `{nome}`: usa peça inexistente `{m}`")
    for d in duras:
        print(f"  ✗ {d}")
    for orf in pf.orfaos():
        print(f"  ⚠ fragmento `{orf}`: nenhum fluxo o usa "
              "(REGRA #0 — integrar ou dar-lhe consumidor)")
    print("NÃO CONFORME" if duras else "CONFORME")
    return 1 if duras else 0


def cmd_monolithic(pf: Portfolio) -> int:
    monos = pf.monoliticos()
    print(f"fluxos: {len(pf.fluxos)} · compostos: {len(pf.fluxos) - len(monos)} · "
          f"MONOLÍTICOS: {len(monos)}")
    for nome, nos, origem in monos:
        print(f"  ⚠ {nome} ({nos} nós, {origem})")
    return 0


def cmd_orphans(pf: Portfolio) -> int:
    orfs = pf.orfaos()
    print(f"peças: {len(pf.fragmentos)} · usadas: {len(pf.fragmentos) - len(orfs)} · "
          f"órfãs: {len(orfs)}")
    for o in orfs:
        print(f"  ⚠ {o}")
    return 0


def cmd_reimplemented(pf: Portfolio) -> int:
    achados = pf.reimplementados()
    print(f"fluxos {len(pf.fluxos)} × peças {len(pf.fragmentos)} · "
          f"repetições: {len(achados)}")
    for fluxo, no, frag, fno in achados:
        print(f"  ⚠ {fluxo}.{no} ≡ {frag}.{fno}")
    return 0


def cmd_mermaid(pf: Portfolio) -> int:
    orfs = set(pf.orfaos())
    L = ["graph LR",
         "  classDef frag fill:#eef6ff,stroke:#4a7fb5,stroke-width:1px;",
         "  classDef mono fill:#fff3e6,stroke:#c8873a,stroke-width:1px;",
         "  classDef comp fill:#eefaf0,stroke:#4a9b62,stroke-width:1px;",
         "  classDef orf fill:#fdeaea,stroke:#c0504d,stroke-dasharray:4 3;"]
    for nome in sorted(pf.fluxos):
        fl = pf.fluxos[nome]
        classe = "comp" if fl["usos"] else "mono"
        L.append(f'  F_{_id(nome)}["{nome}<br/><small>{len(fl["nos"])} nós</small>"]'
                 f":::{classe}")
    for nome in sorted(pf.fragmentos):
        classe = "orf" if nome in orfs else "frag"
        L.append(f'  P_{_id(nome)}(["{nome}"]):::{classe}')
    for nome in pf._ordem_arestas():
        fl = pf.fluxos[nome]
        for m in fl["usos"]:
            L.append(f"  F_{_id(nome)} --> P_{_id(m)}")
        anc = fl["supersedes"]
        if anc and anc in pf.fluxos:
            L.append(f"  F_{_id(nome)} ==> F_{_id(anc)}")
    print("\n".join(L))
    return 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(
        description="O portfólio de fluxos ADW como grafo — lint, monólitos, "
                    "órfãos, reimplementações e o desenho mermaid.")
    ap.add_argument("--root", type=Path, default=Path("."),
                    help="raiz do projeto (contém .touring/adw)")
    ap.add_argument("cmd", choices=["lint", "monolithic", "orphans",
                                    "reimplemented", "mermaid"])
    a = ap.parse_args(argv)
    pf = Portfolio(a.root.resolve())
    return {"lint": cmd_lint, "monolithic": cmd_monolithic, "orphans": cmd_orphans,
            "reimplemented": cmd_reimplemented, "mermaid": cmd_mermaid}[a.cmd](pf)


if __name__ == "__main__":
    sys.exit(main())
