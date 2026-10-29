---
type: Plan
title: "Pendências consolidadas — 29/10/2026"
description: "Toda tarefa aberta dos planos janela-eficácia, injeção, portfolio-scripts, work-outer e VGP, mais as transversais, por janela de execução"
timestamp: 2026-10-29
---

# Pendências consolidadas (29/10/2026)

Fontes, todas verificadas contra o código e o git nesta data:
- `docs/plans/2026-10-26-touring-janela-eficacia/PLANO.md` §11 e `A3-preregistro.md`;
- `docs/plans/2026-10-26-injecao-contexto-hooks/PENDENCIAS-2026-10-29.md` (touring-c1, MiniMax; 34 itens);
- `docs/plans/2026-10-08-vgp-motor-base/PENDENCIAS-2026-10-29.md` (touring-7e, Kimi; 31 itens; DAG `task_1791581548206685387`);
- `docs/plans/2026-10-27-portfolio-scripts/RETOMADA.md` §"Estado verificado em 29/10".

`docs/plans/2026-10-2{6,7,8}-work-outer/` são manifestos automáticos do hook, sem pendência
(o DAG `task_1793033502719494636` está `done`); entram no próximo commit "acumulado".

**Restrição vigente: congelamento do A3 até 06/11/2026 04:00.** Nada de deploy, mudança em
`~/.claude/settings.json` ou edição de hook vivo que injeta contexto (nem dos dados que ele lê).
Commits, código e testes estão liberados; o que muda o que roda espera 06/11.

Tamanho: S (horas) · M (1 dia) · L (vários dias) · XL (semana+). Prioridade: P0 > P1 > P2 > P3.

---

## Feito em 29/10 (fora desta lista, registrado para não refazer)

| O quê | Evidência |
|---|---|
| A3: troca por timer systemd (o cron não disparava com a máquina dormindo); `a3_arm.py reset` | `40bb5c7f` |
| F2.4: falso positivo que bloqueava o workspace + bypass por desestruturação + regressão do 1º conserto | `20d2b05a`, `4a93eac0`, `3cd28e94` |
| portfolio-scripts: estado verificado; `.gitignore` do conteúdo minerado de transcripts | `b237e78d` |
| Relatórios de pendências VGP e injeção | `a6e17ea8` |
| DAG VGP limpo: V1, V0e1b, V2, V5, P2, P95C → completed; F21INNER → failed (fora do plano); P0B2 e JUIZ-TIMEOUT com id limpo; V3 passa a depender deles | `touring decompose validate` → valid, sem ciclos |
| Worktree `touring-wt-c1b` removido (snapshot `9b3547de` em `feat/c1c-cves`) | `git worktree list` |
| Nota obsoleta de `p2-redesign-gate-semelhanca.md` descartada (snapshot `32978c55` em `safety/2026-10-29-p2-redesign-nota`) | `git status` limpo no arquivo |
| **V-2 JUIZ-TIMEOUT** (touring-c1, revisada pela touring-cb): pré-registro, juiz com `INCONCLUSIVO` (exit 3), classe `medida` no `hw-slot` | `69d824a6`, `c59840bd`; 19/19 cenários, 4 mutantes mortos, `test_hw_slot.py` 34 passed; DAG `JUIZ-TIMEOUT` → completed |

---

## 1. Agora — código e testes, permitidos no congelamento

### 1a. VGP (DAG `task_1791581548206685387`)

| # | Item | P | Tam. | Depende de | O que fazer | Evidência |
|---|---|---|---|---|---|---|
| V-1 | **P0B2** — camada 2 nega atributo que existe | P0 | M | — | Rebaixar a aviso/abster quando o submódulo existe em disco ou a camada 1 confirma dinamismo; 3 regressões (`urllib.error` ×2: `ddc0a56fb49f`, `056b54167b0b`; `PIL.Image.LANCZOS`: `94a5472efe4c`); re-julgar num held-out **novo** selado (o 20261013f foi gasto) | `crates/touring-pyright/src/pyright.rs:59,321` (`NEGA_MODULO`); `diagnostics/j13/5c7b3d80_v3.json:65-110` |
| V-2 | ~~**JUIZ-TIMEOUT**~~ **FEITO em 29/10** (`c59840bd`); resta re-derivar o patamar (20%, provisório) quando a classe `medida` produzir um A/A por id | P0 | M | — | Novo pré-registro; contar timeouts por run e declarar `INCONCLUSIVO` acima do patamar do A/A (fail-closed), com testes e mutantes; classe de medição exclusiva no `hw-slot` (toma rust + heavy) | `docs/plans/2026-10-08-vgp-motor-base/scripts/juiz_motor_vgp.py`; `RELATORIO-INT97I-FASE-D.md:49-59` (521/738 timeouts) |
| V-3 | **V3** — camada 2 PRONTA | P0 | M | V-1 (V-2 feito) | Medir o daemon quente (o juiz mede CLI fria; DOC-V3-HOT-DAEMON) e re-julgar | `diagnostics/j13/5c7b3d80_v3.out` (2/600 falsos, CP95 1,05%, p95 1.027 ms, `NÃO PRONTO`) |
| V-4 | **INTADAPT** — integrar os adaptadores multi-linguagem | P1 | XL | — | Reconciliar `touring-wt-adapt` (base `c118aa52`, 09/10) com o main; reaplicar vtransv E1, vts, vhtml+política, vrs, F21K; build, gates, binário reproduzível, juízes por id, `RELATORIO-INT-ADAPT.md` | `crates/touring-code/src/vgp/lang/{rust,html}.rs` ainda stubs; `diagnostics/int-adapt/simulacao.txt` |
| V-5 | **F21K** — URL `javascript:` constante inerte | P2 | S | V-4 | Integrar `is_inert_constant_url`, testar constantes/payloads/concatenação, registrar aceite | só no worktree; main tem só o F21 posicional (`cwe_patterns.rs:138-141`) |
| V-6 | **VRS** — aceite do adaptador Rust | P1 | M | V-4 | Julgar no HORUST-REAL e corpora de dev (warns, `use super::*`, cfg, deny efetivo) | `RELATORIO-VRS.md:78,120-126` (recall de nome 5/20) |
| V-7 | **VHTML** — aceite do adaptador HTML | P1 | M | V-4 | Build/gates via INTADAPT, depois A6HTML e JHTML | `RELATORIO-VHTML.md:458-469` |
| V-8 | **SEM2** — corpora onda 2 | P1 | L | — | Completar denominadores (web: 83 HTML + 21 JS de 300), resolver concentração OSS, gerar selos externos que o validador atual aceite; classe sem n suficiente sai como **não provada** | `~/.claude/touring/vgp/corpus_holdout_web_20261012/meta.json` (`atinge_alvo=false`) |
| V-9 | **HORUST** — held-out Rust | P1 | M | V-6 | Julgar uma vez os 184 ids primários + 340 negativos; selar o estrato cfg ou registrar a exclusão | `diagnostics/horust-real/selo.json` (898 arquivos, 618 entradas) |
| V-10 | **V0e** — correções remanescentes das 606 fora de gate | P2 | M | — | Fechar os destinos ainda abertos da triagem | `diagnostics/V0e-fora-de-gate.md` |
| V-11 | **DOC-QUALITY-LOCK-FAIL-OPEN** — locks/orçamento do gate P0 | P1 | M | — | Desenhar e testar a separação **na fonte**; aplicar ao vivo só depois de 06/11 (ver 3.9) | `client/hooks/touring-quality-block-all.sh:93` (20 s); `RETOMAR-AQUI-2026-10-09.md:1124-1127` |
| V-12 | Aceite residual **VTS/VTSX**, **VCDN/VCSS**, **VL0 camada 2** | P2 | L | V-4 | Julgar TS/TSX/JS em corpora suficientes (TSX n=213<299, 9 negativos JS, `.d.ts`); juiz web com controles de rede/JSX/CSS-in-JS; medir VL0-L2 (precisão, concorrência, daemon, custo do rust-analyzer) sob `hw-slot` | `RELATORIO-VTS.md:266`; `RELATORIO-VTRANSV.md:322-337`; `RELATORIO-VL0-camada2-generica.md` |

### 1b. Injeção de contexto

| # | Item | P | Tam. | O que fazer | Evidência |
|---|---|---|---|---|---|
| I-1 | **Régua `context_budget`** — o instrumento que vai decidir o default | P0 | M | Piso de amostra (abaixo dele, "não medido" em vez de FAIL; hoje reprova com n=3); `denied` no denominador do `injection_follow_ratio`; implementar `fact_kb` e `nudge_follow_ratio` | `crates/touring-cli/src/cli/context_budget.rs:757-758,842-845`; 0 ocorrências de `fact_kb` |
| I-2 | **Nível corrente no KPI** | P1 | S | Expor `injection_level` e `injection_suppressed_bytes_count`/`_blocks_count` no `kpi`/`gate-metrics` | 0 ocorrências de `injection_level` na CLI |
| I-3 | **`HookResponse` duplicado** (§8.7 passo 0) | P2 | M (L3) | Remover o enum de `touring-hook-runtime` e reexportar o canônico | `crates/touring-hook-runtime/src/hook_runtime.rs:61` × `crates/touring-hooks-core/src/hook_response.rs:102` |
| I-4 | **Invariante "So what?"** | P2 | L | Predicado único sobre o payload de todo injetor (hoje só declarado em `pre_read.rs:3-4`, com 1 teste); ~34% do contexto o viola | `ANALISE-CONSOLIDADA.md` §11.4 |
| I-5 | **Taxonomia residual** | P3 | S | Reduzir os ~10,4% que caem em `other` | `round4/RELATORIO-RODADA-4.md:127` |

### 1c. Transversais (achados de 27–29/10)

| # | Item | P | Tam. | O que fazer | Evidência |
|---|---|---|---|---|---|
| T-1 | **40 linhas malformadas** no `run_journal.jsonl` | P1 | S | Provável escrita concorrente intercalada: uma escrita por registro com `O_APPEND` + lock, e leitor que conta a linha ruim em vez de pular calado | `~/.claude/touring/run_journal.jsonl` |
| T-2 | **e2e escrevem no journal real** | P1 | S | `HOME` temporário nos e2e que disparam `touring run` (o `g10_trusted_route_e2e` só desliga o gate de repetição) | `b6f747c8` |
| T-3 | **F2.4: lacuna de kwargs/dict com atribuição à esquerda** | P2 | S | `cfg = connect(password="…")` e `creds = {"api_key": "…"}` dividem no 1º `=` e nunca bloquearam; cobrir sem FP novo (censo do workspace antes/depois) | teste `test_keyword_as_an_earlier_value_is_not_the_assigned_name` |
| T-4 | **`decompose` sem estado "cancelado"** | P3 | S | Obsoletos (D1, D2, P0M, F21INNER, ids malformados) ficaram como `failed`, que mistura "falhou" com "não se aplica" | `touring decompose update --help` |
| T-5 | **Wiring cego a ponteiro de função** | P2 | M | Contar referência por ponteiro/`fn` como consumo | registrado no PLANO §11 |
| T-6 | **tantivy 0.27** (sai do zstd com advisory ignorado) | P3 | L (L4) | Upgrade com reindexação | `deny.toml` (RUSTSEC-2026-0391/0398/0403 ignorados com evidência) |

---

## 2. Em 03/11 — medição (só leitura, permitida)

| # | Item | O que fazer |
|---|---|---|
| M-1 | Ponto semanal da régua (S-15/D2) | `scripts/measure_tool_trio.py` + `scripts/measure_injection_effect.py` com o corpus declarado |
| M-2 | Semana do `script_portfolio_gate` | `python3 ~/.claude/skills/Touring/scripts/hooks/script_portfolio_gate.py --stats` (29/10: 50 eventos, 4,0% evitáveis); critério de promoção à la prose gate |
| M-3 | Conferência diária do A3 | `systemctl --user list-timers 'a3-*'` e `~/.claude/touring/a3_arm.log`: uma linha datada por dia; 30/10 deve gravar o braço B |

---

## 3. A partir de 06/11/2026 04:00 (fim do congelamento)

| # | Item | Depende de | O que fazer |
|---|---|---|---|
| 3.1 | **Análise do A3** | fim do bloco 2 | `a3_arm.py analyze` (unidade sessão, bootstrap semente 20261027, margem −1 pp); sensibilidades: sem sessões que atravessam a troca, e **por projeto** (touring × analise, emenda de 29/10) |
| 3.2 | **Terminar o release** | — | O rito do 30.4.100 morreu na suspensão de 28/10; o HEAD já passou dele (consertos do F2.4 etc.), então sobe a versão seguinte com `scripts/propagate-release.sh <v>`: toolchain, default, projetos (`analise` 30.4.99, `latina` 30.4.97), prova 5.5, verify |
| 3.3 | **Decisão do default de injeção** | 3.1, I-1 | Gabriel decide se `no-advisory` vira o default |
| 3.4 | **A/B do compactador (B1) e do mod de evicção (B3m)** | 3.1 | `TOURING_RESULT_COMPACTOR` (desligado) e `tools/mods/touring-compact-evict` (não instalado), cada um com seu A/B |
| 3.5 | **Calibração D2** (Phase 4) | 3.1 | Calibrar gates e compactadores com dado |
| 3.6 | **VGP V0e5** — remedição dos gates | 3.2 | `scripts/v0e_negacoes.py --desde/--ate` congelados, janela pós-deploy, S3/G10/write→run/G3/G11 separados, com denominador e IC |
| 3.7 | **VGP V4 → V6 → J** | V-3, 3.2 | Ligar o deny e coletar 5 sessões com zero falso deny (`scripts/juiz_v4_sessoes.py` hoje dá exit 3); V6 aplica o ponteiro em `~/projects/analise/.claude/rules/vgp-protocol.md` (hoje 6.326 bytes, o protocolo antigo) + docs/skill/espelho; J roda `juiz_plano_vgp.py`, `prova_vgp_motor.py` e a convergência no binário final |
| 3.8 | **VGP A6HTML → JHTML** | V-7, V-8 | Sessão Kimi nova no mesmo modo de permissão lê o brief `briefs/2026-10-10-a6-revisao-html-whatwg.md`; depois JHTML por id com `--exigir-limite-superior` |
| 3.9 | **Aplicar V-11 ao vivo** | V-11 | Provar que o gate P0 não falha aberto sob contenção |
| 3.10 | **analise: default do `simhash.py`** | 3.2 | A troca da analise-40, que espera o congelamento |

---

## 4. Decisões do Gabriel

| # | Decisão | Contexto |
|---|---|---|
| D-1 | **Rotacionar a chave `AIza…`** | Presente em `~/.claude/history.jsonl`; rotação no console do Google |
| D-2 | **23 worktrees restantes** | VGP (`wt-vl0`, `wt-vrs`, `wt-vrs2`, `wt-vhtml`, `wt-vts`, `wt-vtransv`, `wt-adapt`, `wt-semeado`, `wt-f21`, `wt-horust-real`, `wt-frag`, `wt-p0b`, `wt-p0bd`, `wt-roteamento`, `wt-tiers`) só depois do INTADAPT; os demais (`wt-ecc-{hk,mem,py,qa,val}`, `wt-onda1`, `wt-int97`, `wt-r98`) pedem a mesma medição feita no `wt-c1b` antes de sair |
| D-3 | **`SECURITY_REVIEW_MODEL`** (opcional) | Mudança no settings, só depois de 06/11 |
| D-4 | **F21INNER** como plano próprio | Sink `innerHTML` com taint/sanitização; fora do VGP |
| D-5 | **Residual zip** | RUSTSEC-2026-0367/0368/0369 aceitos; reavaliar quando sair candle com zip ≥ 9 |

---

## Ordem recomendada para "agora"

1. **V-1 P0B2** (em curso desde 29/10 ~14:50 com a touring-7e/Kimi em `crates/touring-pyright/**`;
   `pyright.rs` já modificado no working tree; ela não commita, a touring-cb revisa e commita): há
   falso deny real. A V-2 JUIZ-TIMEOUT foi entregue e commitada no mesmo dia.
2. **I-1 + I-2**: a régua precisa estar certa **antes** da análise do A3 em 06/11.
3. **T-1 + T-2**: o journal é insumo do gate de repetição e do KPI.
4. **V-4 INTADAPT**, que destrava V-5…V-9 e V-12.
