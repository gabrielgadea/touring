---
type: Report
title: "Reconciliação do delta de drift 31/08 → 08/10/2026 (561 → 669 → 380)"
description: "Disposição sinal-a-sinal dos 111 sinais novos (45 falsos positivos de predicado + 66 registros datados legítimos), 3 correções de predicado no scanner com testes pos/neg, juiz atualizado para o total reconciliado 380."
plan_id: 2026-10-08-drift-reconciliacao
bundle: docs/plans/2026-10-08-drift-reconciliacao
tags: [drift, reconciliacao, scanner, regression-guard]
timestamp: 2026-10-08T16:45:00-03:00
delegacao: "analise-6b, aprovada por Gabriel em 08/10/2026"
---

# Reconciliação do delta de drift — 31/08 → 08/10/2026

## TL;DR

O juiz `scripts/test_drift_coverage.py` fixava 561 sinais (congelado 31/08); o scan
media 669. O delta por **conjunto de itens** é **111 sinais novos** (+3 do baseline
resolvidos no interim). Todos os 111 receberam exatamente uma disposição
(`ledger.jsonl`, verificado por `reconciliar.py verify` exit 0):

| Disposição | n | O quê |
|---|---|---|
| **(a) corrigido na fonte** | **0** | Nenhum doc vivo apresentava fato errado — o delta inteiro era registro datado ou falso positivo de régua |
| **(b) falso positivo do scanner** | **45** | 3 predicados corrigidos (C4 contexto, C4 ground-truth do help, C6 ground-truth do código) + 1 exclusão (docs/ fora do gt C6) |
| **(c) registro datado legítimo** | **66** | docs/plans/&lt;data&gt;-*, docs/audits/, turn logs, changelog embutido no CLAUDE.md raiz — história não se reescreve |

O literal do juiz moveu **561 → 380** (= 81+11+103+159+26), com docstring apontando
este bundle. O número final é MENOR que o baseline porque a régua corrigida também
remove falsos positivos do corpus antigo — conta fechada abaixo.

## Tabela por categoria (31/08 · régua velha hoje · régua final · disposições do delta)

| Cat | 31/08 | hoje (régua velha) | delta novo | (a) | (b) | (c) | final (régua nova) |
|---|---|---|---|---|---|---|---|
| C1 versões | 36 | 81 | +47 novos, −2 resolvidos | 0 | 0 | 47 | **81** (régua inalterada) |
| C3 crates fundidos | 11 | 11 | 0 | — | — | — | **11** |
| C4 comandos CLI | 279 | 305 | +26 | 0 | 22 | 4 | **103** (−180 no corpus todo) |
| C5 crates em fence | 151 | 159 | +8 | 0 | 0 | 8 | **159** (régua inalterada) |
| C6 env vars | 84 | 113* | +30 novos, −1 absorvido | 0 | 23 | 7 | **26** (−65 no corpus todo) |
| **Total** | **561** | **669** | **111** | **0** | **45** | **66** | **380** |

\* 113 medido tanto pela sessão delegante quanto nesta — coincidência de env; a régua
velha (`os.environ`) era não-determinística por construção: o snapshot de 31/08
registrou 5 vars TOURING_ no ambiente; esta sessão tem 8.

**Conta fechada do 380**: 561 − 3 (resolvidos no interim) − 180 (FP C4 do baseline
que a régua nova remove) − 65 (FP C6 do baseline que a régua nova remove) + 66 (do
delta que permanecem como (c) declarados) + 1 (C1: 36−2+47=81 já contabilizado acima;
a identidade exata por categoria está em `delta.json` e no scan final).

Identidade por categoria (verificável via `reconciliar.py verify`):
C1: 36 − 2 + 47 = 81 · C3: 11 · C4: 279 − 180 + 4 = 103 · C5: 151 + 8 = 159 ·
C6: 84 − 65 + 7 = 26.

## Os 3 sinais do baseline resolvidos no interim (gone)

| Sinal | Destino medido |
|---|---|
| C1: duas versões antigas (`delta.json` → `gone.C1`) | citações removidas/editadas por terceiros entre 31/08 e 08/10 (régua C1 não mudou). Os literais ficam em `delta.json` — não se repetem aqui de propósito: citá-los neste .md os reintroduziria como sinais (medido: 380→382 em 08/10, corrigido) |
| C6: a var de threads do Rayon (`delta.json` → `gone.C6`) | **absorvida pela régua nova**: a var sempre existiu no código (1 arquivo .rs em crates/); o gt antigo (env) não a via na sessão de 31/08 |

## As 3 correções de predicado (disposição b) — cada uma com teste pos/neg

1. **C4-contexto** (`test_scanner_predicates.py`, 4 testes): citação de comando só
   conta em code fence, inline `` `...` `` ou linha de shell `$/#/❯`. Prosa
   portuguesa ("o touring deixa de ser um CLI") casava como citação — 18 dos 26
   sinais novos do C4.
2. **C4-path** (mesmo arquivo): `.../projects/touring timeout 900` casava o
   **diretório** do workspace; `./target/debug/touring scan` casava o path de build.
   O predicado novo exclui "touring" precedido de `/`. Trade-off declarado:
   invocação por path de build deixa de contar como citação (majoritariamente em
   audits datados — perda de cobertura aceita, ganho de precisão grande).
3. **C4-ground-truth** (1 teste de régua): o `touring --help` imprime **100% no
   stderr** (stdout vazio, rc=0 — medido 08/10); o scanner lia só stdout, então o
   ground truth era **vazio desde 31/08** (o próprio snapshot registra
   `cli_commands_count: 0`): TODO token citado virava "potencialmente stale".
   Corrigido para stdout+stderr → 143 comandos reais.
4. **C6-ground-truth** (4 testes): `os.environ` de quem roda → vars TOURING_*
   referenciadas no **código-fonte** (.rs/.py/.sh/.toml, fora de
   target/node_modules/.git/.touring-cache/.venv **e fora de docs/** — artefatos de
   documentação, inclusive scripts de planos como este bundle, não batizam vars;
   medido: o próprio gerador do ledger contaminou o gt 26→19 antes da exclusão).

## Controle positivo (A3) — duas pernas

1. **Snapshot** (`knowledge/scan-2026-08-31.json`, commit ec2e5928):
   `reconciliar.py snapshot-check` reproduz **561 = 36+11+279+151+84 exato** das
   listas do artefato. Exit 0.
2. **Worktree** (`git worktree add /tmp/drift-3108 ec2e5928`, removido ao final com
   `git worktree remove`): scanner **da época** rodando sobre o estado da época com
   env reconstruído (as 5 vars do snapshot) mediu **C1=36 ✓ C3=11 ✓ C5=151 ✓
   C6=84 ✓** — e C4=290 (+11). A divergência foi investigada e tem nome: os 11
   tokens extras vêm de 4 arquivos `2026-08-31-*` **criados no commit ec2e5928
   (01/09 08:21), depois do instante do scan (31/08 10:41)** — prova por
   `git log --diff-filter=A`. O snapshot registra o instante do working tree, não o
   commit; o método (worktree+régua da época+env da época) é fiel nas 4 categorias
   determinísticas e diverge apenas pela janela documentada.

## Arquivos

**Editados**: `scripts/drift_semantic_scan.py` (3 predicados + exclusão docs/ no gt
C6 + fix Pyright `dict[str, Any]` + `datetime.utcnow` deprecated + `import os`
órfão) · `scripts/test_drift_coverage.py` (literais reconciliados 380, fixture
pytest `errors` com assert no teardown, sem returns bool, memoização dos scans,
docstring → ledger) · bundle novo `docs/plans/2026-10-08-drift-reconciliacao/`
(reconciliar.py, extrair_ocorrencias.py, classificar.py, provar_predicados.py,
gerar_ledger.py, test_scanner_predicates.py, delta.json, ocorrencias.json,
classificacao.json, ledger.jsonl, TABELA.md, RELATORIO.md).

**Nenhum .md de documentação foi editado** (zero disposições (a) — nenhum doc vivo
continha fato errado; os 66 (c) são história legítima).

**Sujos citados por sinais (c), não tocados** (medidos via `git status --porcelain`
no início): `CLAUDE.md` raiz (11 versões do delta em entradas de changelog
datadas), `crates/touring-server/.claude/CLAUDE.md` (30.4.29, heading de wave),
`crates/touring-code/.claude/CLAUDE.md` (touring-language, mapa de migração).
Nenhum precisava de edição — as citações são legítimas. `scripts/test_docs_no_phantom_crates.py`
estava sujo (trabalho de outra sessão): rodado, não tocado — OK (55 arquivos, 0 violações).

## Aceites (exit codes medidos nesta sessão)

| # | Comando | Resultado |
|---|---|---|
| A1 | `python3 scripts/test_drift_coverage.py` | **exit 0** — 8 tests, 0 failures |
| A2 | `python3 -m pytest scripts/test_drift_coverage.py -q` | **exit 0** — 8 passed (coleta sem "fixture 'errors' not found"; modo script segue 8/8) |
| A3 | `reconciliar.py verify` (scan vivo) | **exit 0** — snapshot 561 ✓ · 111 sinais × 111 linhas 1× · disposição×scan-atual consistente |
| A4a | `python3 scripts/test_docs_no_phantom_crates.py` | **exit 0** — 55 arquivos, 0 violações |
| A4b | `ruff check` nos 8 .py tocados | **exit 0** — All checks passed |
| extra | `test_scanner_predicates.py` (standalone e pytest) | **exit 0** — 12/12 (pos+neg por predicado) |

## Perguntas para Gabriel (não decidi sozinho)

1. **C1 cresce por construção**: toda release vira sinal stale assim que a versão
   sobe (36→81 em 38 dias, tudo (c) legítimo). Manter reconciliação periódica
   manual (este modelo), ou evoluir o predicado C1 para "só conta se doc VIVO
   apresenta a versão como atual"? A segunda opção zera o ruído mas redefine o
   que o C1 mede.
2. **C4 e invocação por path de build**: o predicado novo exclui
   `./target/debug/touring <cmd>` (citação legítima em audits). Aceito como
   trade-off, ou o predicado deve distinguir "path que termina no binário" de
   "path que é o diretório do repo"?
3. **Changelog inline no CLAUDE.md raiz**: as 11 versões do delta ali são entradas
   datadas legítimas num arquivo que já excede o teto de 400 linhas (Regra #16) —
   mover a seção de histórico para `CHANGELOG.md` ou `docs/` reduziria o ruído C1
   E a poluição do CLAUDE.md, mas é edição em arquivo fora do escopo desta
   delegação. Fica registrado como potencialização pendente.

## Adendo 08/10 (tarde) — movimento do changelog + trabalho concorrente

1. **Changelog inline do `CLAUDE.md` raiz movido para `CHANGELOG.md`** (decisão de
   Gabriel, 08/10): as entradas datadas 15–30 da seção "Regras deste workspace"
   (rodadas 8/9, cross-audits e releases de 14–22/09) saíram verbatim para a parte
   curada do `CHANGELOG.md`, com nota de proveniência; as entradas operacionais
   vivas permanecem (1–14 + três renumeradas 15–17). `CLAUDE.md`: 555 → 245 linhas
   (teto Regra #16 = 400). **Efeito medido neste juiz: zero** — as versões movidas
   têm outras citações no escopo do scanner, então C1 ficou em 81 (medido, não
   presumido). Um falso alarme meu marcou o movimento como falho: o *pointer* que
   escrevi citava os literais das versões — reintroduzindo-as; corrigido.
2. **2 sinais C5 novos durante a reconciliação** (`touring-ctx-execute`,
   `touring-scriber`): cercas de código em backups de skills do bundle datado
   `docs/plans/2026-10-08-compactacao-65/` (trabalho concorrente da analise-6b).
   Disposição (c) no ledger (lote "2026-10-08-tarde", linhas 112–113) e o literal do
   juiz moveu 380 → **382**. O `reconciliar.py verify` passou a cobrir o **delta
   vivo** (scan atual − snapshot, união com o lote congelado) — é o que o torna
   permanente: qualquer sinal futuro exige linha no ledger, e as disposições (b) não
   quebram a contagem por terem sumido do scan por construção.

Bateria final (08/10 ~17:20): A1 exit 0 (8/8, 382) · A2 exit 0 (8 passed) ·
verify exit 0 (snapshot 561 + união 113 + consistência) · predicados 12/12 ·
phantom guard exit 0 · ruff exit 0.

---
_v1.1 — 2026-10-08 | sessão analise (delegação analise-6b) | ledger: ledger.jsonl (113 linhas: 111 do delta 31/08→08/10 + 2 do lote "2026-10-08-tarde")_

## Adendo 08/10 (noite) — juiz da coordenadora (R5) e 3ª reconciliação

Re-rodada da bateria pela coordenadora (~20h): A1/A2/A3 **rc 1** — 9 sinais novos depois das 17:20
(C1 +1, C4 +3, C5 +5), todos em backups e diagnóstico datados do bundle `2026-10-08-compactacao-65`.
Disposição (c) nos 9 (ledger 113 → 122, lote `2026-10-08-noite`); literal do juiz 382 → **391**.

**Dois deles eram defeito real no arquivo VIVO**, fora do escopo do scanner (que não varre `client/`):
os subcomandos `hook-memory-*` ensinados pelo agente scouter e pela referência de hooks não existem
(rc 1 "Unknown subcommand"; são handlers de dispatch `cli-hook-memory-*`, `hook_registry.rs:1532-1535`)
→ rota CLI `memory recall/store`; e o crate absorvido `resource-monitor` nos exemplos de três agentes
→ `crates/touring-foundation/src/sentinel/guard/mod.rs:80` (`start_ticker` verificado). Corrigido na
**fonte** `~/.claude` e no espelho `client/`, byte a byte iguais; `sync-client-skills --check` CLEAN
(o espelho corre de `~/.claude` para `client/`: `--apply` sobre o espelho editado teria desfeito a correção).

Achado à parte, sem ação aqui: `MemoryGuard` existe duplicado em `touring-foundation` e
`touring-resilience` (ambos membros do workspace).

Bateria da coordenadora (08/10 ~21:10): A1 rc 0 (391) · A2 rc 0 (8 passed) · A3 verify rc 0 ·
A4a rc 0 · A4b ruff rc 0 · predicados 12/12 · sync --check rc 0. Lições no grafo:
`licao:backup-dentro-do-escopo-revela-o-drift-que-o-escopo-esconde:2026-10-08` (+2 ligadas).

## Adendo 09/10 (madrugada) — o release desloca o C1 (4ª reconciliação)

O release propagado às 01:11:50Z fez o previsto na pergunta 1 acima ("C1 cresce por construção"): a versão
planejada virou a corrente e saiu do C1 — a linha 47 do ledger, que era (c) "plano cita o release planejado",
foi RECLASSIFICADA para (a) com o campo `mudanca` (o bump do Cargo.toml é a mudança na fonte); a versão anterior
virou stale (17 citações datadas em auditorias e planos) → nova linha (c), lote `2026-10-09-release`. Um terceiro
sinal era citação minha do número de um release FUTURO na RETOMADA da VGP — corrigido na fonte ("o PRÓXIMO
release"), sem linha no ledger. Total do juiz inalterado (391: −1 +1). Ledger 122 → 123. A pergunta 1 segue
aberta para o Gabriel: sem mudar o predicado, cada release exige esta reconciliação de duas linhas.
