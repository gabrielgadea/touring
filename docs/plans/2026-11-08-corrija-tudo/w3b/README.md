# W3b — linha-base da latência do #8 (08–09/11/2026)

Pergunta: quanto o #8 (leitura contida do VGP em `caminho.rs`, `a4a8ed14`) custa na verificação,
medido em release contra um binário ANTERIOR ao #8 — a medição de 05/11 (p95 0,141 ms no pior
grupo) não tinha linha-base.

Método: `rodar_w3b.sh` — worktree em `208d5cd0` (pai de `a4a8ed14`, já com a sonda
`latencia_do_vgp_na_arvore`), os dois binários do teste compilados em release, 4 rodadas
INTERCALADAS (pré, head, pré, head…) de 1000 × todos os casos, sob `hw-slot --class medida`.
Dados por rodada: `rodadas.jsonl`; saída: `rodar.log`.

| | p95 por rodada (ms) | mediana |
|---|---|---|
| pré-#8 (`208d5cd0`) | 0,1132 · 0,1401 · 0,1189 · 0,1131 | 0,116 |
| HEAD (`11e5a7aa`+) | 0,1491 · 0,1340 · 0,1331 · 0,1565 | 0,1415 |

Por grupo (mediana dos p95, ms): css −0,002 · html −0,001 · python −0,010 · rust +0,006 ·
jsx +0,026 · ts +0,069.

Leitura: o #8 custa ~0,03 ms no p95 geral (TS é o grupo que mais paga, +0,07 ms) — da mesma ordem do
ruído entre rodadas do próprio pré-#8 (0,113↔0,140), e 0,02% do orçamento de 150 ms. Ressalvas: n = 4 rodadas por lado, máquina a 92–96 °C durante a medida; o
corpus do HEAD tem casos que o de `208d5cd0` pode não ter (a comparação por grupo é mais justa
que a geral). Veredito: custo do #8 desprezível frente ao orçamento; nenhuma ação.
