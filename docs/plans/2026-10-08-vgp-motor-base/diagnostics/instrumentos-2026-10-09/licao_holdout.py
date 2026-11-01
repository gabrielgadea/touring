"""Lição de 09/10/2026: holdout fresco do motor VGP — o zero do binário novo só vale com o binário velho medindo > 0 no mesmo corpus."""

from scripts.memoria.grafo_memoria import No, gravar

PROC = ("decomp:task_1791484992466538442",)
no = No(
    tipo="licao",
    slug="holdout-fresco-o-zero-do-binario-novo-so-vale-com-o-velho-medindo-positivo-no-mesmo-corpus",
    corpo=(
        "Holdout V0d (corte = maior ts do vivo 2026-10-09T18:13:31.894Z + 1 min, lido do arquivo; ate = 21:12:22.428Z "
        "congelado; transcripts truncados em ate): 77 negativos de arquivo (76 atribuído + 1 tmp) e 189 inline, 0 positivos "
        "(21 linhas Python no inventário V0a da janela, 0 passam classe_de). Meta de 200 de arquivo inalcançável: 115 edições "
        "limpas em 3 h, 39 (34%) podadas por não reconstruírem. Sobreposição com o conjunto de ajuste: 0 arquivos, 0 conteúdos, "
        "0 comandos. Concentração: 32 arquivos distintos (corpus_semeado.py = 24 dos 76), 6 sessões inline (2 = 72%). O juiz "
        "stock RECUSA corpus sem positivo ('degenerado', 0 chamadas): montei um híbrido no scratchpad (negativos do holdout + 65 "
        "positivos do vivo só como controle de liveness; corpus_holdout fica puro). Binário da Kimi e935: 0/77 e 0/189 falsos "
        "deny (limite superior 95% = 3,8% e 1,6%; ≤1% exigiria n≥299 sem falha). Controle da MEDIÇÃO: o binário antigo "
        "c8b503c4a10c58dd no mesmo corpus dá 11/77 (14,3%) e 4/189 (2,1%) — 15 entradas, 11 arquivos distintos, todas R2 "
        "(with-as, except-as, walrus, atribuição encadeada) ou R3 (`from __future__ import annotations`): os consertos "
        "generalizam por CLASSE de construto. p95 de 678 ms (igual nos dois binários, vivo 104 ms) vem de 2 arquivos do touring "
        "(corpus_semeado.py 691 ms, shell_inline.py 291 ms; todos os outros ≤25 ms, inclusive um de 34 KB): latência por "
        "CONTEÚDO, não por CPU nem por projeto. Armadilhas: pgrep -f casa o próprio shell (use [j] ou cmdline ancorada); "
        "o classificar_falsos_vgp.py fixa CORPUS no vivo — importar e apontar ao holdout; o estado_no_instante_v0d.py tem "
        "CORPUS default = vivo (passar o diretório explícito)."
    ),
    facetas={"kind": "lesson", "lang": "python", "domain": "vgp", "process": "medicao", "status": "verified"},
    procedencia=PROC,
)
print(no.chave, "→", gravar(no))
