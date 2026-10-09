"""Grava as lições da 3ª reconciliação de drift (08/10/2026, noite; DAG task_1791474484429430085, R5/R6).

Rodar do repo analise (o executor do grafo mora lá):
    cd ~/projects/analise && .venv/bin/python3 ~/projects/touring/docs/plans/2026-10-08-drift-reconciliacao/gravar_licoes_noite.py
"""

import sys
from datetime import date
from pathlib import Path

sys.path.insert(0, str(Path.home() / "projects/analise"))
from scripts.memoria.grafo_memoria import Aresta, No, gravar  # noqa: E402

SESSAO = date(2026, 10, 8)
AQUI = "docs/plans/2026-10-08-drift-reconciliacao/gravar_licoes_noite.py (repo touring)"

SONDA = No(
    quando=SESSAO,
    tipo="licao",
    slug="backup-dentro-do-escopo-revela-o-drift-que-o-escopo-esconde",
    corpo=(
        "Re-rodando o aceite do drift às 20h: A1/A3 rc 1 com 9 sinais novos (C1 +1, C4 +3, C5 +5), todos em backups "
        "datados do bundle compactacao-65 — candidatos óbvios a (c) 'citação histórica'. Mas o backup é cópia de um "
        "arquivo VIVO em client/, que fica FORA do escopo do scanner (1.561 arquivos; client/ ausente). Conferindo o "
        "original: 2 dos 9 eram defeito real lá — subcomandos hook-memory-* inexistentes (rc 1 'Unknown subcommand'; "
        "são handlers de dispatch cli-hook-memory-*) e o crate absorvido resource-monitor nos exemplos de 3 agentes. "
        "Corrigido na fonte; ledger 113→122, juiz 382→391, bateria 7/7 rc 0. Método: sinal em cópia → perguntar "
        "onde vive o original e se o escopo o cobre, antes de dar (c)."
    ),
    facetas={"kind": "lesson", "domain": "harness", "process": "audit", "lang": "markdown"},
    procedencia=(AQUI,),
    arestas=(Aresta("licao:regressao-se-mede-nos-importadores-nao-no-diretorio-do-modulo:2026-10-08", "relates-to"),),
)

ESPELHO = No(
    quando=SESSAO,
    tipo="licao",
    slug="espelho-client-corre-de-claude-editar-o-espelho-e-dar-apply-desfaz-a-correcao",
    corpo=(
        "sync-client-skills.py copia ~/.claude/{agents,skills,rules} → client/ (o que EXECUTA é ~/.claude). Corrigi 4 "
        "arquivos em client/ e o --check acusou 'alterado fora do sync (editado à mão)' com 'corrigir: --apply' — que "
        "teria SOBRESCRITO a correção com a versão antiga e deixado o --check verde. Rota certa: aplicar as mesmas "
        "substituições na fonte (assert count==1), exigir fonte == espelho byte a byte (4/4 True) e só então --apply, "
        "que virou no-op de conteúdo (+0 ~0 -0, 377 iguais) e só regravou o MANIFEST.sha256. É o gate do passo 0 do "
        "propagate-release."
    ),
    facetas={"kind": "lesson", "domain": "harness", "process": "audit", "lang": "python"},
    procedencia=(AQUI,),
    arestas=(Aresta(f"licao:{SONDA.slug}:2026-10-08", "relates-to"),),
)

GIT_SANDBOX = No(
    quando=SESSAO,
    tipo="licao",
    slug="git-grep-no-sandbox-do-touring-run-morre-no-ignore-global-e-o-laco-imprime-zero",
    corpo=(
        "Localizando 9 sinais com um laço 'git grep' dentro de touring run --lang bash: exit 0 e '0 arquivos' para "
        "os 9 padrões. O stderr tinha 'fatal: cannot use ~/.config/git/ignore as an exclude file' (sem permissão no "
        "sandbox) — o git morria e o laço contava zero. Fora do sandbox, com controle positivo (a versão corrente: 38 "
        "linhas em 8 arquivos), os 9 apareceram. Reincidência da lição de 26/09 (sandbox cego a ignorados): todo zero "
        "vindo do sandbox exige um padrão que SABIDAMENTE existe no mesmo laço."
    ),
    facetas={"kind": "lesson", "domain": "harness", "process": "audit", "lang": "bash"},
    procedencia=(AQUI,),
    arestas=(Aresta(f"licao:{SONDA.slug}:2026-10-08", "relates-to"),),
)

if __name__ == "__main__":
    for no in (SONDA, ESPELHO, GIT_SANDBOX):
        r = gravar(no)
        print(r.chave, r.gravado, r.arestas_criadas, "/", r.arestas_pedidas, r.avisos)
