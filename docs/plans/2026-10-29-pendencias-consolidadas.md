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

**Sem congelamento desde 29/10 17:26.** O Gabriel encerrou o A3 no dia 0 (`c26b09cc`): deploy,
`settings.json` e hooks vivos estão liberados. O deploy do 30.4.101 foi lançado às 17:28, levando
os consertos de 28–29/10 (F2.4, P0B2, JUIZ-TIMEOUT só como script, SimHash v2 etc.).

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
| **V-1 P0B2** (touring-7e, revisada pela touring-cb; o caso PIL voltou uma vez porque a fixture não tinha a forma real do Pillow): abstenção com evidência positiva nas raízes do venv | `3675ed2c`; 191+2+1 testes, 15 com pyright real, teste contra o `Image.py` instalado, 3 mutantes mortos; DAG `P0B2` → completed |
| **V-2 JUIZ-TIMEOUT** (touring-c1, revisada pela touring-cb): pré-registro, juiz com `INCONCLUSIVO` (exit 3), classe `medida` no `hw-slot` | `69d824a6`, `c59840bd`; 19/19 cenários, 4 mutantes mortos, `test_hw_slot.py` 34 passed; DAG `JUIZ-TIMEOUT` → completed |

## Feito em 31/10 (meta do Gabriel: "resolva tudo de pendencias-consolidadas.md")

Tudo no código-fonte; **o binário vivo só muda no próximo release** (30.4.102).

| Item | O quê | Evidência |
|---|---|---|
| **I-1** | Régua: piso de amostra (`ADHERENCE_MIN_SAMPLE`=20, abaixo dele `not_measured`, nunca FAIL); MUST de deny de gate fora do denominador (`must_denied`); `fact_kb` sobre as `FACT_FAMILIES`; `nudge_follow_ratio` sobre os `gate_events` (emitted+denied; G2/G8 de reescrita fora) | `4b250401`; cli `context_budget` 39 + kpi verdes, 733/733 lib, clippy 0 |
| **I-2** | `injection_level` + `injection_suppressed_{blocks,bytes}_count` no `gate-metrics` e no `kpi` | `4b250401`; teste `um_corte_soma_no_contador_vivo_mesmo_sem_diario` |
| **I-3** (subagente Sonnet, revisado aqui) | `HookResponse` único em `touring-hooks-core`: ganhou `ContextWithUpdatedToolOutput`; o `emit` do runtime (trace + dedupe + sítio `emit-runtime`) virou o canônico; 13 goldens byte-exatos do enum antigo; `SITE_EMIT_CORE` sem emissor removido | `7523df7e`, `0cd05d5e`; core 544 + runtime 559 + handlers 814 + dispatch, `check --workspace` 0 |
| **I-4** | `touring_foundation::so_what::violation` — o "So what?" como UM predicado (vazio / sem alvo nomeado), medido em SOMBRA por família e no total (`context_budget.so_what`); não corta nada — silenciar é a decisão 3.3 | `4b250401`; 5 testes do predicado + 1 da régua |
| **I-5** | Taxonomia residual: no scan vivo de 31/10, `other` = **0 B de 1.070.962 B** injetados (23 famílias); nada a reduzir | `touring kpi -j` → `context_budget.by_family` |
| **T-1** (touring-65) | Journal: uma escrita por registro (+flock acima do PIPE_BUF); linha ruim CONTADA e exposta (aggregate, KPI, gate). Real: 400/47.150 inválidas históricas; 0 em 12 processos × 250 | `3948939b` |
| **T-2** (touring-65) | e2e g10 com HOME temporário; prova por conteúdo (journal real 13.437.983 → 13.437.983 B) | `32bb8561` |
| **T-3** (touring-65) | F2.4: `cfg = connect(password="…")` e `creds = {"api_key": "…"}` bloqueiam; `#` vira comentário na projeção; `==` não é ligação. Censo 0,985 → 0,986, 0 bloqueios | `521be247` |
| **T-4** | `cancelled` sempre existiu; o help escondia. Help com os 7 estados; 6 obsoletos do DAG VGP `failed` → `cancelled` | `c7ff3b42`; DAG: 6 cancelled, 0 failed |
| **T-5** | Wiring: função passada como VALOR pelo caminho (`m.insert("cli-x", crate::h::cli_x)`) conta como uso; 31 de 147 órfãos de função eram só isso. Identificador nu fica de fora (variável local) | `1a5d047d`; touring-code 974/974; efeito vivo pede release + `index rebuild` |
| **V-11 + 3.9** | As faixas do wrapper (17/10) já separavam arquivo de diretório na fonte; prova AO VIVO: com o lock real ocupado, o gate P0 nega em 0,12 s e libera o limpo em 0,11 s, 0 fail-open; mutante pré-faixas reprova (allow em 3,05 s, 2 fail-opens) | `73921140` (`diagnostics/v11-prova-contencao/`) |
| **3.10** | Default do `simhash.py` já é `distintos=True` (a analise usa o do skill Touring, não tem cópia) | `3b9119c1` |
| **D-7** | Staging do Premium Refactor fora do main (snapshot `safety/2026-10-31-premium-staging`); `validate_phase5` check 8 valida os workflows promovidos. **Achado**: o F1.3 de `scripts/` vai a 0,425 (duplicação REAL 17,1% combinada) — o 0,451 era o modo per-crate medindo 436 LOC de 62 mil | `1237fe99` |
| **D-6 (b)** | Replay A1 declara os 2 denominadores (E1.1 e taxonomia S-4); ruff 21→0 nos 3 arquivos (F821 real) | `1e76ac30`; r3 375/375 |
| **D-2** | 8 worktrees removidos: int97 (limpo); ecc-{val,hk,mem,py,qa}, r98, onda1 com snapshot em `safety/2026-10-31-wt-*`; não rastreados do onda1 em `~/.claude/touring/worktree-snapshots/touring-wt-onda1-untracked-2026-10-31.tar.zst` (2075/2075 conferidos). Restam o principal + 15 do VGP (esperam o INTADAPT) | `git worktree list` = 16 |
| **D-3** | `SECURITY_REVIEW_MODEL=claude-opus-5-5` no `env` do `~/.claude/settings.json` (default era `claude-opus-4-7`) | settings.json válido |
| **D-1** | Chave `AIza…` rotacionada pelo Gabriel | resposta de 31/10 |
| **V-10** (touring-65) | Destinos (b) do V0e: `-c` alias de `--code`; o timeout nomeia orçamento + `--timeout-ms` no `failure.message` (o motor lia a 1ª linha do stderr, a lição ficava na última); corpo que o bash não parseia ensina `--file` (`--args` é no-op em bash, medido). Censo corrigido: (b)=54, não 39. Limites: `--brief` não carrega `failure`; aspas quebradas no shell do CHAMADOR nunca chegam ao produto | `acbde526`, `739a352d`; server 1696 + ceg 613 verdes |
| **3.4 mecanismo** (subagente) | Bundle `docs/plans/2026-10-31-ab34-compactador-eviccao/`: fatorial 2×2 por dia (compactador × evicção), 8 dias, semente 20261105, `ab34_arm.py` + timers + runbook do mod; **nada armado**. Daemon herda o env de quem o spawna (`daemon_spawn.rs:185-214`): o `arm` reinicia com a flag e relê `/proc/<pid>/environ`. Antes do dia 0: release 30.4.102, pré-voo P1 (o transcript grava a saída compactada?) depois do V-3 quente, instalar o mod e pré-voo P3 | `ee600b85` (recommit de `fa8b6ed5`, que um `reset --soft` alheio tirou do histórico); 150 testes, 14 mutantes |
| **V-3 J14** (touring-8e) | Held-out `20261031b` (selo `e8d8016f45bff2c5`, 600 arq + 400 inline, sobreposição 0 nas 4 chaves); conserto do `api-request-blob` sem ts. **30.4.101 NÃO PRONTO**: v2 0/600 + 1/400; V3 1/600 + 1/400 (CP95 inline 1,18% > 1%), p95 frio 1.014,9 ms; referência 8fd0efb0 V3 10/600 + 2/400. O único caso inline, `2106f0ebf279` (`exit`), é comum a v2 e V3. **Quente**: p95 252,48 ms (1.064/1.064 rotas daemon, 4,02× menor). **Decisão do Gabriel (31/10): a latência passa a ser julgada no caminho quente**, com pré-registro novo a partir do próximo held-out | `443564a3`, `1256b7f0`, `b4ef21c8`, `0358b517`, `8b717222`; `B/diagnostics/j14/RELATORIO.md` |
| **D-4** (subagente) | F2.1 ganha o braço de XSS de DOM (`touring-analysis/src/quality/dom_sinks.rs`), taint intra-função como **aviso** (`advisories`, fora do score e do bloqueio). Censo: 0 arquivos com score alterado (touring e analise); 5 avisos abertos à mão na analise, 1 TP (`tools/sei-simulator/.../DocumentViewer.tsx:58`) e 4 FP; escada de promoção em `docs/plans/2026-10-31-f21inner/PLANO.md` §8 | `fe2d4512`; 1677 testes, 5 mutantes |

### Decisões do Gabriel em 31/10 (AskUserQuestion)

T-6 tantivy: **adiar**. 3.3: **manter full e medir** com a régua corrigida após o 30.4.102. D-6: **(b)**. D-7:
**aposentar o staging**. D-2: **medir e remover os limpos**. 3.4: **A/B completo agora**, **antes** do A3
(3.11 fica ativo, entra depois), **release só na virada do dia** durante o A/B. D-5: **manter aceito**. D-3, D-4,
3.11: **ativos**. D-1: **feito**.

### Em curso (retomada)

- **V-3** — touring-8e (Kimi): conserto do `ts` vazio por TDD e held-out **20261031b** já selado (`b4ef21c8`); falta J14 + quente×frio. Brief no transcript desta sessão; relatório para touring-00.
- **V-10** — touring-65 (MiniMax): `-c` alias de `--code`, orçamento no `failure.message` do timeout, rota `--file/--args` no erro de sintaxe do bash. Depois: **V-4 INTADAPT** (patches em `docs/plans/2026-10-08-vgp-motor-base/diagnostics/int-adapt/patches/`).
- **V-8 SEM2** → touring-8e depois do V-3.
- **3.4** — desenhando o pré-registro: compactador lido do env do DAEMON (`result_compactor.rs:114`, troca exige `touring daemon-ctl restart` na virada), mod de evicção lido do env da SESSÃO (`register.ts:80`, mod ainda não instalado em `~/.claude/mods`).
- **Release 30.4.102** depois de V-10 + V-3, na virada de um dia do A/B.
- **D-4 F21INNER** (plano próprio) e **D-7b** (deduplicar `scripts/`, F1.3 0,425) a abrir.

---

## 1. Agora — código e testes, permitidos no congelamento

### 1a. VGP (DAG `task_1791581548206685387`)

| # | Item | P | Tam. | Depende de | O que fazer | Evidência |
|---|---|---|---|---|---|---|
| V-1 | ~~**P0B2**~~ **FEITO em 29/10** (`3675ed2c`); o re-julgamento num held-out novo selado passa para V-3 | P0 | M | — | Rebaixar a aviso/abster quando o submódulo existe em disco ou a camada 1 confirma dinamismo; 3 regressões (`urllib.error` ×2: `ddc0a56fb49f`, `056b54167b0b`; `PIL.Image.LANCZOS`: `94a5472efe4c`); re-julgar num held-out **novo** selado (o 20261013f foi gasto) | `crates/touring-pyright/src/pyright.rs:59,321` (`NEGA_MODULO`); `diagnostics/j13/5c7b3d80_v3.json:65-110` |
| V-2 | ~~**JUIZ-TIMEOUT**~~ **FEITO em 29/10** (`c59840bd`); resta re-derivar o patamar (20%, provisório) quando a classe `medida` produzir um A/A por id | P0 | M | — | Novo pré-registro; contar timeouts por run e declarar `INCONCLUSIVO` acima do patamar do A/A (fail-closed), com testes e mutantes; classe de medição exclusiva no `hw-slot` (toma rust + heavy) | `docs/plans/2026-10-08-vgp-motor-base/scripts/juiz_motor_vgp.py`; `RELATORIO-INT97I-FASE-D.md:49-59` (521/738 timeouts) |
| V-3 | **V3** — camada 2 PRONTA | P0 | M | — (V-1 e V-2 feitos) | Medir o daemon quente (o juiz mede CLI fria; DOC-V3-HOT-DAEMON) e re-julgar | `diagnostics/j13/5c7b3d80_v3.out` (2/600 falsos, CP95 1,05%, p95 1.027 ms, `NÃO PRONTO`) |
| V-4 | **INTADAPT** — integrar os adaptadores multi-linguagem | P1 | XL | — | Reconciliar `touring-wt-adapt` (base `c118aa52`, 09/10) com o main; reaplicar vtransv E1, vts, vhtml+política, vrs, F21K; build, gates, binário reproduzível, juízes por id, `RELATORIO-INT-ADAPT.md` | `crates/touring-code/src/vgp/lang/{rust,html}.rs` ainda stubs; `diagnostics/int-adapt/simulacao.txt` |
| V-5 | **F21K** — URL `javascript:` constante inerte | P2 | S | V-4 | Integrar `is_inert_constant_url`, testar constantes/payloads/concatenação, registrar aceite | só no worktree; main tem só o F21 posicional (`cwe_patterns.rs:138-141`) |
| V-6 | **VRS** — aceite do adaptador Rust | P1 | M | V-4 | Julgar no HORUST-REAL e corpora de dev (warns, `use super::*`, cfg, deny efetivo) | `RELATORIO-VRS.md:78,120-126` (recall de nome 5/20) |
| V-7 | **VHTML** — aceite do adaptador HTML | P1 | M | V-4 | Build/gates via INTADAPT, depois A6HTML e JHTML | `RELATORIO-VHTML.md:458-469` |
| V-8 | **SEM2** — corpora onda 2 | P1 | L | — | Completar denominadores (web: 83 HTML + 21 JS de 300), resolver concentração OSS, gerar selos externos que o validador atual aceite; classe sem n suficiente sai como **não provada** | `~/.claude/touring/vgp/corpus_holdout_web_20261012/meta.json` (`atinge_alvo=false`) |
| V-9 | **HORUST** — held-out Rust | P1 | M | V-6 | Julgar uma vez os 184 ids primários + 340 negativos; selar o estrato cfg ou registrar a exclusão | `diagnostics/horust-real/selo.json` (898 arquivos, 618 entradas) |
| V-10 | **V0e** — correções remanescentes das 606 fora de gate | P2 | M | — | Fechar os destinos ainda abertos da triagem | `diagnostics/V0e-fora-de-gate.md` |
| V-11 | **DOC-QUALITY-LOCK-FAIL-OPEN** — locks/orçamento do gate P0 | P1 | M | — | Desenhar e testar a separação **na fonte**; aplicar ao vivo depois dos testes (ver 3.9) | `client/hooks/touring-quality-block-all.sh:93` (20 s); `RETOMAR-AQUI-2026-10-09.md:1124-1127` |
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


---

## 3. Antes presos ao congelamento — liberados desde 29/10 17:26

| # | Item | Depende de | O que fazer |
|---|---|---|---|
| 3.2 | ~~**Release 30.4.101**~~ **FEITO em 31/10 14:26** | — | A 1ª tentativa (29/10 17:28) morreu num desligamento às 18:04 (o inhibit só bloqueava suspensão); a 2ª, com `shutdown` também inibido, deu EXIT 0 — o binário instalado é `git: a6b9a550` (built 17:20:45Z): o rito começou no `3b9119c1`, mas o passo 2/6 gravou o HEAD de então, que já tinha o commit Python da touring-65 (só `scripts/*.py`; o Rust é idêntico): prova 43/43, doctor 9/9, global + toolchain default + `analise` + `konverter` em 30.4.101. `latina` segue em 30.4.97 por pin declarado (`[toolchain] channel = "30.4.97"` no `touring.toml` dela) |
| 3.3 | **Decisão do default de injeção** | I-1 | Sem o dado do A3 (encerrado). O default segue `full`; decidir com a régua corrigida (I-1) ou com um A3 refeito numa janela planejada |
| 3.4 | **A/B do compactador (B1) e do mod de evicção (B3m)** | — | `TOURING_RESULT_COMPACTOR` (desligado) e `tools/mods/touring-compact-evict` (não instalado), cada um com seu A/B; não colidem mais com o A3 |
| 3.5 | **Calibração D2** (Phase 4) | M-1 | Calibrar gates e compactadores com o dado da régua semanal |
| 3.6 | **VGP V0e5** — remedição dos gates | 3.2 | `scripts/v0e_negacoes.py --desde/--ate` congelados, janela pós-deploy, S3/G10/write→run/G3/G11 separados, com denominador e IC |
| 3.7 | **VGP V4 → V6 → J** | V-3, 3.2 | Ligar o deny e coletar 5 sessões com zero falso deny (`scripts/juiz_v4_sessoes.py` hoje dá exit 3); V6 aplica o ponteiro em `~/projects/analise/.claude/rules/vgp-protocol.md` (hoje 6.326 bytes, o protocolo antigo) + docs/skill/espelho; J roda `juiz_plano_vgp.py`, `prova_vgp_motor.py` e a convergência no binário final |
| 3.8 | **VGP A6HTML → JHTML** | V-7, V-8 | Sessão Kimi nova no mesmo modo de permissão lê o brief `briefs/2026-10-10-a6-revisao-html-whatwg.md`; depois JHTML por id com `--exigir-limite-superior` |
| 3.9 | **Aplicar V-11 ao vivo** | V-11 | Provar que o gate P0 não falha aberto sob contenção |
| 3.10 | **analise: default do `simhash.py`** | 3.2 | A troca da analise-40, que esperava o congelamento |
| 3.11 | **Refazer o A3** (opcional) | janela de 8 dias sem deploy | Ambiente homogêneo (todos os projetos na mesma versão); mecanismo pronto e desabilitado (`a3_arm.py`, timers `a3-{arm,reset}`) |

---

## 4. Decisões do Gabriel

| # | Decisão | Contexto |
|---|---|---|
| D-1 | **Rotacionar a chave `AIza…`** | Presente em `~/.claude/history.jsonl`; rotação no console do Google |
| D-2 | **23 worktrees restantes** | VGP (`wt-vl0`, `wt-vrs`, `wt-vrs2`, `wt-vhtml`, `wt-vts`, `wt-vtransv`, `wt-adapt`, `wt-semeado`, `wt-f21`, `wt-horust-real`, `wt-frag`, `wt-p0b`, `wt-p0bd`, `wt-roteamento`, `wt-tiers`) só depois do INTADAPT; os demais (`wt-ecc-{hk,mem,py,qa,val}`, `wt-onda1`, `wt-int97`, `wt-r98`) pedem a mesma medição feita no `wt-c1b` antes de sair |
| D-3 | **`SECURITY_REVIEW_MODEL`** (opcional) | Mudança no settings, liberada desde o fim do A3 |
| D-4 | **F21INNER** como plano próprio | Sink `innerHTML` com taint/sanitização; fora do VGP |
| D-5 | **Residual zip** | RUSTSEC-2026-0367/0368/0369 aceitos; reavaliar quando sair candle com zip ≥ 9 |
| D-6 | **Denominador do r3 (Errata E1.1): 6 famílias × taxonomia de 18** | As 6 famílias de E1.1 são a operacionalização de "só blocos de hooks do Touring" contra a taxonomia de então; a S-4 (26/10) expandiu a régua para 18 marcadores + 10 padrões (`crates/touring-foundation/src/injection_family.rs`). MEDIDO em 31/10 17:04 BRT (`ex.paridade()`, os 3 transcripts do KPI; 23 famílias com bytes, delta 0,00% por família): das 23, **17 são do Touring = 514.443 B de 794.362 B (64,7%)**, e E1.1 mede **164.090 B (20,7%)** delas como harness — o denominador de C-A1-1/C-A1-3. O dataset do r3 está congelado com esse rótulo (`data/MANIFEST.sha256`, 02/10): corrigir exige re-extrair + re-congelar + refazer o veredito downstream. Opções: (a) partição nova (taxonomia − `plugin-hook` − `generic-noise`) e re-congelar; (b) manter as 6 como definição do dataset e declarar os dois denominadores no relatório; (c) adiar. **Até decidir, a lista fica como está** — nada muda em disco, e o furo fica declarado no código (`scripts/r3_a1_kpi.py`, `TOURING_FAMILIES`, ATENÇÃO de 31/10) | `docs/plans/2026-10-02-jev-harness-r3/CRITERIOS-ACEITACAO.md:153-161`; `scripts/r3_a1_kpi.py:TOURING_FAMILIES` |
| D-7 | **F1.3 (Code Duplication) no escopo `scripts/` — herança do Premium Refactor** | O juiz de convergência (`loop_converged.py`) fica CONTINUE por UMA cláusula: `quality_gold` (tier Silver, composite 0,9383, blocker F1.3). MEDIDO em 31/10 17:4x: F1.3 dá **0,451** em `scripts/` — "LOC-weighted over 4 crate(s), 436 LOC", piores `w5-touring-storage`=0,100, `w10-touring-orchestration`=0,200, `w7-touring-bindings`=0,656, todos em `scripts/touring_premium_refactor_2026/staging/` (programa de mai/2026, 211 arquivos rastreados). **Não é da correção do r3**: com o escopo fatiado SÓ nos 4 arquivos tocados (`--include`), o F1.3 devolve o MESMO 0,451/436 LOC — a dimensão mede crates inteiros e ignora o fatiamento; e o arquivo isolado dá Diamond 0,9664. Consertar = deduplicar ou aposentar o staging do Premium Refactor (211 arquivos de outro programa): decisão do Gabriel, não uma edição de escopo alheio | `touring-quality score scripts --dims F1.3`; `scripts/touring_premium_refactor_2026/staging/` |

---

## Ordem recomendada para "agora"

1. **V-3 V3**: P0B2 e JUIZ-TIMEOUT foram entregues e commitados em 29/10. Falta montar um
   held-out **novo** selado (o 20261013f foi gasto), medir o daemon quente e re-julgar com o juiz
   novo, que declara INCONCLUSIVO sob timeout. O re-julgamento mede o binário construído do HEAD
   (o 30.4.101, se o rito de 29/10 fechar verde).
2. **I-1 + I-2**: sem o A3, a régua corrigida é o instrumento que vai decidir o default de injeção.
3. **T-1 + T-2**: o journal é insumo do gate de repetição e do KPI.
4. **V-4 INTADAPT**, que destrava V-5…V-9 e V-12.
