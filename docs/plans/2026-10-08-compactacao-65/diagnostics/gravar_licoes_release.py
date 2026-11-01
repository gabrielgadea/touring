"""Grava as lições do passo 3 do release 30.4.93 (S8/A3, DAG task_1791479126715571926; 08/10/2026, noite).

Rodar do repo analise (o executor do grafo mora lá):
    cd ~/projects/analise && .venv/bin/python3 \
        ~/projects/touring/docs/plans/2026-10-08-compactacao-65/diagnostics/gravar_licoes_release.py
"""

import sys
from datetime import date
from pathlib import Path

sys.path.insert(0, str(Path.home() / "projects/analise"))
from scripts.memoria.grafo_memoria import Aresta, No, gravar  # noqa: E402

SESSAO = date(2026, 10, 8)
AQUI = "docs/plans/2026-10-08-compactacao-65/diagnostics/gravar_licoes_release.py (repo touring)"

ALVO_PELO_ENV = No(
    quando=SESSAO,
    tipo="licao",
    slug="update-touring-de-sessao-per-project-ciclou-o-daemon-do-projeto-pelo-env-herdado",
    corpo=(
        "Passo 3 do release 30.4.93 saiu exit 4: o `update-touring`, chamado de uma sessão do analise, herdou "
        "TOURING_DAEMON_SOCKET=analise/.touring/daemon.sock; o `daemon-ctl` stop/restart dele ciclou o daemon "
        "PER-PROJECT (PID 2928009 nasceu 21:33:04, no meio do build) e o GLOBAL (PID 3156, binário deleted) ficou "
        "intocado — enquanto o doctor lê o pidfile global. Dois resolvedores de alvo (env × pidfile) no mesmo "
        "script. Conserto: `unset` das duas variáveis na linha 50 (antes da 1ª chamada, linha 220), porque só o "
        "daemon global executa target/release. RED 2 falhas · GREEN 28 passed · mutação 2 falhas · passo 3 refeito "
        "rc 0 com global novo (PID 2971193). Método: o horário de início do processo desfaz 'reiniciou' × 'qual'."
    ),
    facetas={"kind": "lesson", "domain": "harness", "process": "debug", "lang": "bash"},
    procedencia=(AQUI,),
    arestas=(Aresta("licao:harness-de-aceite-herda-o-env-do-shell-e-fabrica-vermelho-deterministico:2026-10-08",
                    "relates-to"),),
)

HOME_FALSO = No(
    quando=SESSAO,
    tipo="licao",
    slug="home-falso-nao-isola-script-cujo-alvo-nao-deriva-do-home",
    corpo=(
        "O 1º teste RED do conserto rodou o `update-touring` de ponta a ponta com HOME falso e stubs: o "
        "install_symlinks sobrescreveu o stub, o restart 'falhou' e o fallback legacy resolveu o daemon global REAL "
        "pelo socket fixo /tmp/touring-daemon-1000.sock e pelo registry real — e o matou (21:46:49). Nada mais real "
        "foi tocado (5 links e 3 ELF conferidos). Um sandbox por variável isola só o que o código resolve por ela. "
        "Redesenho: prelude + função isolada + stub; e o aceite virou MEDIÇÃO antes==depois (links, PIDs de "
        "touring-daemon, mtimes) em cada rodada da suíte — ela roda no gate de todo release."
    ),
    facetas={"kind": "lesson", "domain": "harness", "process": "test", "lang": "bash"},
    procedencia=(AQUI,),
    arestas=(Aresta(f"licao:{ALVO_PELO_ENV.slug}:2026-10-08", "relates-to"),),
)

if __name__ == "__main__":
    for no in (ALVO_PELO_ENV, HOME_FALSO):
        r = gravar(no)
        print(r.chave, r.gravado, r.arestas_criadas, "/", r.arestas_pedidas, r.avisos)
