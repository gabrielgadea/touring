#!/usr/bin/env python3
"""skill_picker.py — S-5.1 (P8): cascata de escolha de skill, com gate de silêncio.

O plano pede a cascata do cookbook (catálogo → top-3 → rejudgado), gate de
silêncio e override manual `!skill`. A prova externa que o plano cita é erro
17% → 7,3% — **número de terceiro, que este arquivo não reproduz e não afirma**.
O que ele faz é tornar o gate **medível**, que é a pré-condição que o próprio
plano escreve: *"Quando NÃO usar: se o gate de silêncio não puder ser medido"*.

A forma real do catálogo (medido em 01/10, 189 entradas em `~/.claude/skills/`):

| | n |
|---|---|
| entradas no diretório | 189 |
| com `SKILL.md` | **185** |
| sem `SKILL.md` (plugins, `.pytest_cache`, …) | 4 |
| **conteúdos distintos** (sha256 do `SKILL.md`) | **~83** |
| entradas que são symlink para `~/.agents/skills/` | 102 |

**[F] A razão de "~83" importa mais que qualquer outra linha acima:** 102 das 185
entradas são o MESMO arquivo por dois caminhos. Um catálogo que conte por entrada
mede a coisa errada — ofereceria ao modelo 102 cópias de `recipe-batch-invite`.
AChoice sai com uma opção por **conteúdo**, e o bloco de aliases vai junto, para
que a resposta possa serPresented com o nome que o usuário digitou.

**[F] Por que cascata e não uma Choice das 185:** as descrições dos
`frontmatter` são curtas — mediana de 70 caracteres, e 5 são curtas demais para
servir de critério (`TACO-subagent`, `diagnose-crash`, `omarchy`…). Uma Choice
com 185 opções de 70 caracteres é um `Choice` cuja função é ser aproximadamente
aleatória. A cascata é o que o cookbook prescreve: **BM25/pré-filtro
determinístico primeiro, modelo depois sobre 3** — e o pré-filtro é gratuito.

O gate de silêncio
------------------
Duas saídas que o modelo pode dar, e a segunda é a que o plano quer poder
provar: `Noul` ("nenhuma skill serve") e o silêncio. O gate decide se a
resposta é **dita ou calada**:

    --tau            acima disso, a sugestão é DITA
    --always-say     desliga o silêncio (para medir o gate contra ele mesmo)

Usar:
    python3 skill_picker.py --query "como faço um relatorio de incidente"     # catálogo só
    python3 skill_picker.py --query "..." --judge                            # com Jev
    python3 skill_picker.py --query "..." --judge --dry-run                  # sem chamar a API
    python3 skill_picker.py --query "..." --override recipe-gmail-send       # bypass manual
"""

from __future__ import annotations

import argparse
import json
import math
import os
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

SKILLS_DIR = Path(os.path.expanduser("~/.claude/skills"))

# As palavras que o próprio Claude Code usa para decidir carregar uma skill: o
# verbo do `description` mais rarezas do domínio. Um filtro que só casa
# substring vai devolver 185 opções; um que descarta stopword chega a zero. A lista é
# curta e **medida contra o frontmatter real**, não escolhida por gosto.
STOPWORDS = frozenset("""
a o e de do da das dos em no na nos nas um uma para por com sem que se ao aos
the of to in on for with and or is are be do this that how make use using
skill script hook plugin quando onde qual quais como criar escrever gerar
""".split())


@dataclass
class Skill:
    """Uma opção do catálogo: o `SKILL.md` deduplicado por conteúdo."""

    key: str                      # o nome canônico (o primeiro caminho, sorted)
    aliases: list[str] = field(default_factory=list)
    description: str = ""
    terms: frozenset[str] = frozenset()

    def to_choice_option(self) -> str:
        """A opção como vai para a Choice: nome + o que É e o que NÃO é.

        **[I] (0,85)** O critério de contraste é o que separa o rótulo: com
        "use esta skill quando…" a fronteira entre duas skills irmãs é
        invisível, e o modelo escolhe a prima. O campo `quando_nao_usar` é o
        que o próprio plano exige dos fluxos ADW (`[purpose].when_not_to_use`) —
        a mesma ideia, aplicada ao catálogo.
        """
        head = f"{self.key}: {self.description}"
        return f"{head} NÃO usar se a tarefa for de outraskill já nomeada acima."


def read_frontmatter(path: Path) -> dict:
    """Lê o bloco `---` do início do `SKILL.md`. Sem dependência externa."""
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return {}
    if not text.startswith("---"):
        return {}
    end = text.find("\n---", 3)
    if end == -1:
        return {}
    block = text[3:end]
    out: dict[str, str] = {}
    key = None
    for line in block.split("\n"):
        if line.startswith((" ", "\t")) and key:  # continuação de `description: >`
            out[key] += " " + line.strip()
            continue
        m = re.match(r"^([A-Za-z_][A-Za-z0-9_-]*):\s*(.*)$", line)
        if m:
            key = m.group(1)
            out[key] = m.group(2).strip().strip('"').lstrip(">").strip()
    return out


def term_set(text: str) -> frozenset[str]:
    words = re.findall(r"[a-zA-ZÀ-ÿ][a-zA-ZÀ-ÿ0-9_-]{2,}", text.lower())
    return frozenset(w for w in words if w not in STOPWORDS)


def build_catalog(root: Path = SKILLS_DIR) -> list[Skill]:
    """O catálogo, deduplicado por CONTEÚDO.

    Deduplicar por nome seria inútil (os nomes já são únicos); deduplicar por
    caminho seria inútil (são caminhos diferentes de propósito). O que
    duplica é o **conteúdo**: o espelho em `~/.agents/skills/` é o mesmo
    arquivo alcançado por dois nomes, e oferecer os dois ao modelo é oferecer
    a mesma escolha duas vezes.
    """
    import hashlib

    by_content: dict[str, Skill] = {}
    for entry in sorted(root.iterdir()) if root.is_dir() else []:
        skill_md = entry / "SKILL.md"
        if not skill_md.is_file():
            continue
        try:
            digest = hashlib.sha256(skill_md.read_bytes()).hexdigest()
        except OSError:
            continue
        front = read_frontmatter(skill_md)
        description = front.get("description", "")
        if digest in by_content:
            by_content[digest].aliases.append(entry.name)
            continue
        by_content[digest] = Skill(
            key=entry.name,
            description=description,
            terms=term_set(f"{entry.name} {description}"),
        )
    return sorted(by_content.values(), key=lambda s: s.key)


def idf_table(catalog: list[Skill]) -> dict[str, float]:
    """Quão raro é cada termo no catálogo — a metade BM25 que faz o trabalho.

    **[F] Medido em 01/10 sem isto:** a cobertura simples dispara em **95,8%**
    das 120 tarefas reais, idêntico em τ = 0,20 e τ = 0,80, com
    `lexcore-execute` ganhando **56 de 120**. A causa: a description dela é um
    parágrafo inteiro (59 termos; a mediana do catálogo é 11), então qualquer
    consulta que cite `executar`, `cargo`, `clippy` ou `checkpoint` — palavras
    que nearly toda tarefa de código tem — casa com ela. Cobertura de UM termo
    não separa; um termo comum vale o mesmo para todas.

    Com IDF, `cargo` (em muitas skills) pesa pouco e `remuneracao`
    (em uma) pesa muito. A tabela é calculada do catálogo real, não hardcoded:
    um termo raro hoje é o índice de amanhã se alguém escreve outra skill.
    """
    total = max(1, len(catalog))
    df: dict[str, int] = {}
    for skill in catalog:
        for term in skill.terms:
            df[term] = df.get(term, 0) + 1
    return {term: math.log(1 + total / count) for term, count in df.items()}


def score_skill(skill: Skill, q: frozenset[str], idf: dict[str, float]) -> float:
    """A soma dos pesos IDF dos termos que casaram, normalizada pela consulta.

    Normalizar pela **máxima possível** (a soma dos IDF de todos os termos da
    consulta) é o que torna o τ comparável entre consultas de tamanhos
    diferentes: uma consulta de 2 termos e uma de 15 precisam bater no mesmo
    τ, ou o gate aprende o tamanho da frase em vez do assunto.
    """
    hits = q & skill.terms
    if not hits:
        return 0.0
    weight = sum(idf.get(t, 0.0) for t in hits)
    ceiling = sum(idf.get(t, 0.0) for t in q) or 1.0
    score = weight / ceiling
    # Citar o nome é o sinal mais forte que existe — mas com o texto inteiro
    # da description, "recipe-*" casaria com as 41 recipes.
    if q & term_set(skill.key):
        score = min(1.0, score + 0.5)
    return score


def shortlist(query: str, catalog: list[Skill], limit: int) -> list[tuple[Skill, float]]:
    """O pré-filtro determinístico: cobertura ponderada por IDF."""
    q = term_set(query)
    if not q:
        return []
    idf = idf_table(catalog)
    scored = [(skill, score_skill(skill, q, idf)) for skill in catalog if skill.terms]
    scored = [(skill, sc) for skill, sc in scored if sc > 0.0]
    scored.sort(key=lambda pair: (-pair[1], pair[0].key))
    return scored[:limit]


def build_question(candidates: list[tuple[Skill, float]]) -> dict:
    """A Choice de 3, com as opções em criteria contrastivos."""
    criteria = {
        skill.key: skill.to_choice_option()
        for skill, _ in candidates
    }
    return {
        "which_skill": {
            "type": "choice",
            "instructions": (
                "Choose the ONE skill from this shortlist that should be loaded for the task, "
                "or choose `none` if none of them fits. "
                "Judge what the task REQUIRES, not which name sounds closest. "
                "If two fit, choose the more specific one."
            ),
            "criteria": {**criteria, "none": "No skill in this list fits the task. The right answer is to do it without a skill."},
        }
    }


def skill_picker(query: str, tau: float, catalog: list[Skill] | None = None, *, client=None) -> dict:
    """A cascata completa. Devolve sempre um dict — o gate é a decisão, não a
    resposta.

    A forma do retorno é o contrato: `said` diz se a sugestão foi DITA, e
    `silent` diz por quê quando não foi. Um gate que não pode explicar o seu
    silêncio não é um gate, é uma caixa-preta — e o plano diz que sem medir o
    gate o P8 não entra.
    """
    catalog = catalog if catalog is not None else build_catalog()
    # O nome é `candidates`, não `shortlist`: uma variável local chamada
    # `shortlist` sombreia a função `shortlist()` dentro do próprio escopo, e o
    # `UnboundLocalError` só aparece quando o `if not shortlist:` roda — que é
    # justamente o caminho que o teste do gate exercita primeiro.
    candidates = shortlist(query, catalog, limit=3)

    payload: dict = {
        "query": query,
        "catalog_size": len(catalog),
        "shortlist": [
            {"key": s.key, "score": round(sc, 4), "aliases": s.aliases}
            for s, sc in candidates
        ],
        "tau": tau,
        "source": "shortlist_only",
        "said": False,
        "silent_reason": None,
        "picked": None,
    }

    # Pré-filtro não achou NADA: calar aqui é o gate funcionando, e é o caso
    # mais comum — a maioria das tarefas não é uma skill.
    if not candidates:
        payload["silent_reason"] = "shortlist_empty"
        return payload

    top_score = candidates[0][1]
    # O corte ANTES do modelo: com o pré-filtro fraco, nem gastar chamada.
    if top_score < tau:
        payload["silent_reason"] = "below_tau"
        payload["top_score"] = round(top_score, 4)
        return payload

    try:
        from jev_client import system_one  # importado tarde: o dry-run não quer o SDK
    except Exception as exc:  # noqa: BLE001 — o gate não pode quebrar a sessão
        payload["silent_reason"] = f"adapter_unavailable:{type(exc).__name__}"
        return payload

    state = {"task": query, "candidates": [s.key for s, _ in candidates]}
    answers = system_one(state, build_question(candidates), client=client)
    if answers is None:
        # O adapter é fail-open por desenho: sem resposta, calar é a resposta
        # certa, porque dizer "não, nenhuma skill serve" com 100 chamadas de
        # latência seria pior do que não dizer nada.
        payload["silent_reason"] = "jev_fail_open"
        return payload

    answer = answers.get("which_skill", {})
    picked = answer.get("choice")
    confidence = answer.get("confidence")

    payload["source"] = "jev"
    payload["confidence"] = confidence
    if picked in (None, "none"):
        payload["silent_reason"] = "jev_said_none"
        return payload
    # Confiança baixa é o mesmo caso do `none`: calar.
    if isinstance(confidence, (int, float)) and confidence < tau:
        payload["silent_reason"] = "low_confidence"
        payload["picked"] = picked
        return payload

    payload["said"] = True
    payload["picked"] = picked
    return payload


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--query", required=True, help="a tarefa, em linguagem natural")
    ap.add_argument("--tau", type=float, default=0.55,
                    help="corte do gate — MEDIDO em 01/10 sobre 120 tarefas reais: "
                         "a distribuição do score é bimodal (56 em [0.4,0.5), 23 em [0.9,1.0]) "
                         "e o vale fica em ~0.55, onde o disparo cai de 93 para 27,5 pct. "
                         "Ver experiments/s5-skill-picker/report.md")
    ap.add_argument("--judge", action="store_true", help="chamar o Jev no shortlist")
    ap.add_argument("--dry-run", action="store_true", help="imprimir o plano sem chamar a API")
    ap.add_argument("--override", help="bypass manual: imprime a skill e sai")
    ap.add_argument("--catalog", action="store_true", help="imprimir o catálogo inteiro")
    ap.add_argument("--json", action="store_true", help="saída em JSON")
    args = ap.parse_args(argv)

    if args.override:
        out = {"said": True, "picked": args.override, "source": "manual_override", "silent_reason": None}
        print(json.dumps(out, ensure_ascii=False) if args.json else f"→ {args.override} (override manual)")
        return 0

    catalog = build_catalog()

    if args.catalog:
        for skill in catalog:
            alias = f"  (+{len(skill.aliases)} aliases)" if skill.aliases else ""
            print(f"  {skill.key:44} {skill.description[:52]}{alias}")
        print(f"\n{len(catalog)} opções de conteúdo (de {sum(1 + len(s.aliases) for s in catalog)} entradas)")
        return 0

    if args.dry_run or not args.judge:
        hits = shortlist(args.query, catalog, limit=3)
        if args.json:
            print(json.dumps({
                "query": args.query,
                "catalog_size": len(catalog),
                "shortlist": [{"key": s.key, "score": round(sc, 4), "aliases": s.aliases} for s, sc in hits],
                "tau": args.tau,
                "judged": False,
            }, ensure_ascii=False, indent=2))
        else:
            print(f"consulta: {args.query}")
            print(f"catálogo: {len(catalog)} opções (τ={args.tau})")
            for s, sc in hits:
                print(f"  {sc:6.3f}  {s.key:44} {s.description[:50]}")
            if not hits:
                print("  (nada — o gate calaria aqui, sem gastar chamada)")
        return 0

    out = skill_picker(args.query, args.tau, catalog)
    print(json.dumps(out, ensure_ascii=False, indent=2) if args.json else (
        f"→ {out['picked']}" if out["said"] else f"(silêncio: {out['silent_reason']})"
    ))
    return 0


if __name__ == "__main__":
    sys.exit(main())
