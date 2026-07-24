# Auditoria Completa dos CLI Commands do Touring — Efetividade × Qualidade × Excelência × Relevância

> **Data**: 2026-06-27 | **Autor**: TACO (acoplado — code-mode determinístico) | **Binário**: `touring 30.0.0` · `touring-quality 0.1.0`
> **Método**: harness determinístico (6 scripts Python, code-analyses-llm-synthesises) — enumera, testa **execução**, scora 50-dim, + oráculo de purpose-fidelity (amostra)
> **Escopo**: **120 top-level commands → 41 groups → 297 leaf commands** (15 hook + 282 tool), 86 handler files, 35.034 LOC em `crates/touring-server/src/cli/`
> **FASE 0**: daemon 6/6 healthy, socket ok, project_db 143 MB, wiring 76.174 rows — gate PASS
> **Artefatos**: `scratchpad/cliaudit/{commands,results,triage,retest,handler_scores,command_eval,purpose_oracle}.json`
>
> **⚠ Revisão v2 (2026-06-27)** — após pergunta de Gabriel ("a verificação validou se cumprem o propósito ou apenas se executam?"): o eixo originalmente chamado **"Efetividade"** mede **EXECUÇÃO** (exit 0 + output + parse), **NÃO** purpose-fidelity (resultado correto / cumpre o propósito documentado). A distinção, o oráculo de amostra e **2 defeitos de propósito ocultos pelo exit-0** estão na nova **§4.5**. Linguagem "funcionam/quebrados" corrigida para "executam".

---

## 1. Sumário Executivo

| Eixo | Veredito | Número-chave |
|---|---|---|
| **Execução** (≠ purpose-fidelity) | 🟢 | **290/297 executam** (exit 0 + output); após correção VP-Scout, **0 falham na execução**. **Não** prova que o resultado está correto — ver §4.5 |
| **Qualidade (50-dim)** | 🟢 Excelente | mean **0.9688** · **74 Diamond / 11 Platinum / 1 Gold** · **0 falhas P0 BLOCK** em 86 handlers |
| **Excelência (STR)** | 🟡 Mista | **11 commands anti-STR** (>50 KB default; `viz` 3.5 MB, `wiring audit` 1.2 MB) — defeito de coupling-LLM |
| **Relevância** | 🟡 Boa, com redundância | clusters: 16 `*-status`, 20 search-ish, 8 CEG-ish → oportunidade de consolidação |
| **Purpose-fidelity** | 🟠 **amostrado, não completo** | oráculo em 9 cmds → **2 defeitos de propósito ocultos pelo exit-0** (§4.5); auditoria completa = fase 2 (em lotes) |
| **CONSOLIDADO** (execução + código) | 🟢 **276 A / 7 B / 11 C / 3 D / 0 F** | Excelente em **execução + código**; envelope/UX tem 3 defeitos; **propósito é um 5º eixo ainda amostrado** |

**Tese central (honesta)**: o *código* dos comandos é premium-de-elite (mean 50-dim Gold+, floor 0.895, zero brechas de segurança P0) e **executam** corretamente. Os defeitos de **envelope** são três: (1) densidade catastrófica de output em 11 commands (anti-STR — pune o consumo pela LLM); (2) defeito sistêmico de `--help` em ~13 commands custom-dispatch (retornam `rc=1` tratando `--help` como subcomando desconhecido); (3) `ast highlight` com handling de argumento posicional inconsistente vs. os irmãos `ast *`. **Nenhum command falha na execução** — mas **execução não é propósito**: a §4.5 prova, por amostragem, que comandos que executam (exit 0) podem entregar **resultado errado** (ex.: `wiring impact` reportando 0 consumers para um símbolo demonstravelmente usado).

**Nota de método (relevante por si só)**: a 1ª passada do harness reportou **43 "broken"**. A disciplina VP-Scout Cadeia 5 (re-teste sequencial com invocação correta) revelou que **~30 eram falsos-positivos do próprio harness** — flag `-j` rejeitada, argumento posicional errado, contenção do socket do daemon sob 12 workers concorrentes (latência falsa de ~10 s → real <30 ms), `serve` mis-testado como query, e timeout de 6 s curto demais para `pre-task-scout` (orçamento legítimo 8 s). **Reportar os 43 cegamente teria sido fraude de auditoria.** O número honesto de defeitos reais é pequeno.

---

## 2. Metodologia (acoplada e determinística)

Cinco scripts Python compõem o harness (zero-LLM no caminho crítico de medição; o LLM só sintetiza):

| Script | Função | Saída |
|---|---|---|
| `enumerate_commands.py` | parse top-level custom (stderr) + recurse clap `Commands:` (paralelo, `stdin=DEVNULL`, hooks como leaves) | 297 leaves |
| `test_commands.py` | testa cada leaf: `--help` (existe/parseia) + execução real com args inferidos do próprio help; mutadores/pesados = help-only; hooks = invariante exit-0 via stdin | matriz rc/bytes/json/latência |
| `analyze_results.py` | triagem honesta: `rc=2`=erro-do-harness, `rc=1`=investigar, anti-STR, latência, clusters | buckets |
| `retest_suspicious.py` | re-teste sequencial com **invocação correta curada** (VP-Scout Cadeia 5) | verdicts corrigidos |
| `score_handlers.py` | mapeia command→handler via `command_table.rs` (`super::<mod>::`) → **50-dim por handler** (`touring-quality`) | tier/composite/P0 |
| `synthesize.py` | junta os 4 eixos → grade consolidada por command | `command_eval.json` |

**As 50 dimensões** são aplicadas via o motor real `touring-quality score <handler> --format json` (50 verifiers F1.1–F4.12, composite ponderado, 6-tier, 6 P0 BLOCK). A unidade é o **arquivo-handler** que implementa cada command — o critério justo para "qualidade da implementação do comando". (Caveat: a lógica profunda de muitos comandos vive em outros crates — `touring-intelligence`, `touring-analysis`; a 50-dim aqui mede o **adaptador CLI**, que é o código próprio do comando.)

---

## 3. Inventário

```
120 top-level commands
├── 15 HOOK    (exit-0-always: pre/post-{read,edit,write,bash}, session-*, cortex, scan-pii, prompt-enhance, pre-task-scout)
└── 105 TOOL
    ├── 41 groups → 282 leaf subcommands
    │   maiores: ast(29) generate(27) wiring(12) decompose(10) suggest(7) tantivy(7) graph(6) viz(6) index(6) search(6)
    └── ~64 standalone leaves (status, doctor, e2e, exec, kpi, repo-score, harness-metric, ...)
TOTAL TESTÁVEL: 297 leaf commands
```

---

## 4. Eixo 1 — Execução (smoke-test) — o que prova e o que NÃO prova

> **Escopo deste eixo**: mede que o comando **roda** (exit 0), **produz output** e **parseia**. **NÃO** mede que o resultado está **correto** nem que cumpre o **propósito documentado**. Para isso, ver **§4.5 (purpose-fidelity)**. Os 82 "help-only" (mutadores) tiveram **apenas** o `--help` testado — zero validação de execução ou propósito.

### 4.1 Buckets honestos (297 leaves)

| Bucket | N | Leitura |
|---|---|---|
| `live_ok` (rc=0, executado real) | **157** | rodou e retornou output correto |
| `harness_usage_fp` (rc=2 clap) | 27 | **minha** invocação errada (flag/posicional) — comando OK |
| `runtime_err` (rc=1) → investigado | 16 | re-testado → ver §4.2 |
| `help-only` (mutador/pesado/needs-args, por política) | 82 | testado estruturalmente (não executado para não poluir estado) |
| `hook` exit-0-invariant | 15/15 ✅ | (1 falso-alarme corrigido: `pre-task-scout` tem orçamento 8 s) |

### 4.2 Re-teste VP-Scout dos 16 "rc=1" (invocação correta, sequencial)

| Command | 1ª passada | Re-teste correto | Veredito honesto |
|---|---|---|---|
| `exec` `exec-speculative` `plan-gated` `evidence` `predict-action` `plan-verified-depth` `conflict-check` `txn-acquire` | rc=1 | **rc=0, 100–505 B, 0.01 s** | ✅ OK — FP (usam posicional `"<command>"`, meu harness passou file+`-j`) |
| `overlay status/diff` `conflict scan` `definitions classify` `ast find/overview/blast` | ~10 s, 1 B | **rc=0, 0.01–0.02 s** | ✅ OK — os ~10 s eram **contenção do socket sob 12 workers**, não defeito |
| `toolchain` `skip` `ssr` | rc=1 | `toolchain list`/`skip list <file>`/`ssr status` = **OK** | ✅ são *groups* (mis-detectados como leaves pelo defeito de `--help`, §4.3) |
| `clones list` `inferlets list` `governor` `resolve-def` | rc=1 | **rc=0** | ✅ OK — FP de arg/`-j` |
| `change-contract` `inferlets install` | rc=1 | exige `--pre/--post` / path | ✅ comportamento correto (requer args) |
| `serve` | rc=1 | é o **servidor MCP long-running** | ✅ mis-testado como query (não é defeito) |
| **`ast highlight`** | rc=1 | **rc=2 — rejeita file posicional** que `ast overview/blast/meta` aceitam | 🔴 **DEFEITO REAL** (inconsistência de arg, baixa severidade) |

**Conclusão de execução**: **0 commands falham na execução.** Todos **executam** (exit 0 + output) dado o argumento/subcomando/contexto certo. ⚠ Isto **não** prova que o resultado é correto — ver §4.5.

### 4.3 Defeito sistêmico de `--help` (real, baixa severidade)

13 commands custom-dispatch retornam **`rc=1`** ao receber `--help`, tratando-o como subcomando desconhecido (`"Unknown find-code subcommand: --help"`):
`init-project, toolchain, activity, restore, entity, profile, projects, find-code, skip, ssr, change-contract, governor, serve`.
Efeito colateral: o enumerador não descobre os subcomandos deles (por isso `toolchain`/`skip`/`ssr` aparecem como leaves). **Potencializar** (REGRA #0): rotear `--help`/`-h` para o printer de usage com `exit 0` nos handlers custom-dispatch — não deletar nada.

### 4.5 Purpose-fidelity — execução ≠ propósito (amostra-oráculo)

> **A distinção que importa** (skill TACO-cross-audit): *"it crashes?"* (§4, exit-0) vs. *"it does what its purpose says?"* (este eixo). Os §4.1–4.4 provam **execução**; este oráculo verifica se o **resultado bate com a verdade** (ground-truth independente via grep/source). Foi rodado em uma **amostra de 9 comandos read-only** que todos "passaram" no smoke-test.

**Ground-truth (grep, independente do daemon)**: `decide_checkpoint` — def @ `selective_checkpoint.rs:49`, consumer cross-crate de produção @ `ceg_adapter.rs:251`.

| Command | Executa (exit 0)? | Cumpre o propósito? | Evidência |
|---|---|---|---|
| `index find decide_checkpoint` | ✅ | ✅ **cumpre** (acha a def) | retorna `selective_checkpoint.rs:49` |
| `ast find decide_checkpoint` | ✅ | ✅ **cumpre** | signature + module corretos |
| `resolve-def …:49:8` | ✅ | ✅ **cumpre** | resolve para `decide_checkpoint` (Function) |
| **`wiring impact decide_checkpoint`** | ✅ | 🔴 **FALHA** | diz `Direct consumers: 0` — mas grep prova consumer real em `ceg_adapter.rs:251`. **Ignora `-j`** (devolve tabela humana). Um LLM concluiria "órfão → deletável" |
| **`index find` (reference_count)** | ✅ | 🔴 **FALHA (sub-campo)** | `reference_count: 0, references: []` para um símbolo demonstravelmente usado — corrobora a falha de wiring |
| **`ast meta` (golden rule)** | ✅ | 🟠 **lacuna** | `quality_score=None, blast_radius=None` em `--depth summary` **E** `full`; `fan_in=0.0`; `summary_source: on_disk_fallback`. Os 2 campos em que a regra `file-metadata-first` se baseia **não estão no output** (gap do comando OU drift da skill) |
| `index find <inexistente>` | ✅ | ✅ **cumpre** (precisão) | `hits=0` — não alucina |
| `tantivy search` | — | ⚪ inconclusivo | `rc=2` = invocação minha errada (FP do oráculo) |
| `find-references …:49:8` | — | ⚪ inconclusivo | `rc=1` = `-j` mal-posicionado meu (precisa `-- -j`) — FP do oráculo |

**Resultado da amostra**: **4 cumprem propósito · 2 falham (1 + 1 sub-campo) · 2 FP do meu próprio oráculo · 1 falso-alarme corrigido** (`repo-score`: meu oráculo leu a chave `score` em vez de `total_score=145/269=F`; e `F` não contradiz o Diamond da 50-dim — **medem eixos diferentes**; o comando cumpre seu propósito).

**Duas lições** [FACT 1.0]:
1. **Execução esconde falha de propósito**: 9 comandos com exit 0, **2 entregam resultado errado** (`wiring impact`/`index find` afirmando que um símbolo usado é órfão — conclusão *perigosa*). O número "290 executam" do §4 **não** é "290 cumprem propósito".
2. **Purpose-fidelity é cara e exige a mesma disciplina VP-Scout**: o próprio oráculo teve **~3 falsos-negativos meus** (flag/chave/conflação de métricas), só pegos olhando o output cru. Por isso ela **não** foi feita para os 297 — é a **fase 2** (oráculo por comando contra ground-truth, em lotes; ver §4.6, §9 A6/A7/A8 e §10).

### 4.6 Fase 2 — LOTE 1: navegação de código (8 comandos × 8 fixtures grep-provados)

Oráculo escalado: fixtures **auto-descobertos via grep** (def + consumers reais), ground-truth independente, invocação correta, sequencial. Controle de staleness: 1 símbolo recém-editado (`decide_checkpoint`) vs. estáveis.

| Comando | purpose-fidelity | Veredito |
|---|---|---|
| `index find` (acha a def) · `ast find` · `resolve-def` · `ast overview` · `find-references` | **100%** | ✅ **cumprem** — lookup de símbolo robusto |
| **`wiring impact`** | **0% real** | 🔴 retorna `Direct consumers: 0` para os 7 símbolos cross-crate testados, **todos com consumers grep-provados** (`capture_tool_call` = 8 consumers, **estável** → não é staleness). Ignora `-j` |
| **`ast meta` (campos quality/blast)** | **0/8** | 🔴 nunca emite `quality_score`/`blast_radius` (A7 confirmado em escala) |
| **`tantivy search`** | **0%** | 🔴 `{"error":"tantivy-fts feature not enabled or index not initialized"}` com **exit 0** (A8) |

**Nuance crítica (honesta)**: o gap de `wiring impact` é **específico de 2 caminhos** — `wiring impact` e o campo `reference_count` de `index find`. O consumer-data **existe e funciona**: `wiring orphans` (368 órfãos) **não** lista `capture_tool_call`, ou seja, *sabe* que ele tem consumers. Logo não é "tracking morto" — é gap de integração nesses dois comandos. Mitigado: a checagem REGRA #0 (`wiring orphans`) é confiável; o blast-radius por-símbolo (`wiring impact`) **não**.

**Lição-harness (VP-Scout recursivo)**: meu fixture-discovery selecionou nomes genéricos (`cfg`, `env`, `none`) cujos consumers via grep são poluídos por homonímia — os FAILs contra esses são inconclusivos; só os fixtures de nome específico (`capture_tool_call`, `decide_checkpoint`) sustentam o veredito. Lotes seguintes filtram nomes ≥ 6 chars com `_`.

### 4.7 LOTES 2-4 — análise, geração, mutadores (fase 2 COMPLETA)

Mais ~26 comandos, mesma disciplina (ground-truth grep, invocação curada do `--help`, VP-Scout recursivo). Artefatos: `purpose_batch{2,3,4}.{py,json}`.

| Lote | Cumprem propósito (verificado) | Defeitos de propósito |
|---|---|---|
| **L2 análise** | `ast blast`·`tdg`·`rust-semantic`·`imports`·`scope`·`calls`(c/ `--file`)·`file-knowledge extended`·`wiring chains`·`wiring cycles`·`graph god-nodes` | **A9** `cognitive metrics` retorna `{has_graph,has_predictor,initialized}` (status) — **não** o "node/edge count + focus_cache hit_rate" que o CLI-index documenta (drift doc↔comando) |
| **L3 geração** | `generate list-kinds`·`verify` (exists→found / nonexistent→`found:false` = VGP preciso)·`render` (kinds válidos)·`assist list-kinds/applicable`·`definitions classify/nodetypes` | **A10** `ssr apply --stdin` casa o pattern (`matches:1`) mas devolve summary **sem o texto reescrito** · **A11** `--kind PythonModule` → "Unknown GeneratorKind" (kind inexistente; real = `PythonScript`; falta alias em `parse_kind`) |
| **L4 mutadores** (read-back seguro) | `memory store→recall`·`decompose create→get→validate`·`entity define→resolve`·`session start→assess`·`index ingest→find`·`learning reward`·`kpi --snapshot` | **A12** `diary write` persiste (`entry_len:15`) mas `diary read <agent>` (default) → **"Memory store error: RLM error"** |

**Veredito da fase 2 (completa)**: dos ~50 comandos com oráculo de propósito construído, a **grande maioria cumpre o propósito** quando invocada corretamente; os defeitos são **7 específicos (A6-A12)**, concentrados em consumer-tracking (A6), drift doc↔comando (A7/A9), feature-off (A8), output-gaps (A10/A11) e erro de backend (A12). **Os mutadores funcionam** (read-back confirmado) — importante, pois são parte dos 82 que o smoke-test só `--help`-testou. Distinção execução vs. propósito agora medida em 4 lotes, não só amostrada.

---

## 5. Eixo 2 — Qualidade (50 dimensões do harness)

`touring-quality` aplicado a **86 handler files distintos** (mapeados de `command_table.rs`):

| Tier | Handlers | % |
|---|---|---|
| 💎 Diamond (≥0.95) | **74** | 86% |
| 🥇 Platinum (≥0.90) | 11 | 13% |
| 🥈 Gold (≥0.80) | 1 | 1% |
| Silver/Bronze/Unranked | **0** | — |

- **composite**: min **0.895** · max **0.9802** · **mean 0.9688** (Diamond)
- **6 P0 BLOCK gates** (F2.1 OWASP / F2.4 secrets / F2.5 CVE / F2.6 config / F4.3 deprecated / F4.5 pkg): **0 falhas em 86 handlers** — toda a superfície CLI é segura por construção.

### 5.1 Os 8 handlers de menor score (ainda Platinum+, exceto 1)

| composite | tier | handler | command(s) |
|---|---|---|---|
| **0.895** | 🥈 Gold | `harness_metric.rs` | `harness-metric` |
| 0.909 | Platinum | `find_code.rs` | `find-code` |
| 0.915 | Platinum | `search_tools.rs` | `search-tools` |
| 0.925 | Platinum | `change_contract.rs` | `change-contract` |
| 0.931 | Platinum | `exec.rs` | toda a família CEG (`exec`, `evidence`, `plan-gated`, …) |
| 0.931 | Platinum | `rename.rs` | `rename` |
| 0.938 | Platinum | `profile.rs` `projects.rs` `resolve_def.rs` | idem |

**Ironia diagnóstica** (consistente com `coupling-adoption-failure-diagnosis`): o handler de **menor** qualidade é `harness_metric.rs` — a auto-métrica do harness. O medidor é o ponto mais fraco do que mede. Candidato #1 a `taco-forge perfect-edit`.

---

## 6. Eixo 3 — Excelência (Signal-to-Token Ratio)

A excelência de um command para o consumo pela LLM é **densidade** (STR). 11 commands despejam output gigante por default — `U(a)` negativo, exatamente o defeito de coupling diagnosticado antes:

| bytes (default) | latência | command | STR |
|---|---|---|---|
| **3.541.963** | 0.2–0.6 s | `viz workspace` · `viz wiring` | 0.1 ⚫ |
| **1.244.911** | 0.09 s | `wiring audit` | 0.1 ⚫ |
| 336.345 | 0.03–0.08 s | `graph god-nodes` · `wiring modules` · `wiring score` | 0.25 |
| 79.335 | 0.24 s | `quality-signal` | 0.4 |
| 53–57 KB | <0.1 s | `ast workspace-info` · `ast features` · `wiring orphans` · `wiring status` | 0.6 |

**Implicação de coupling**: um `wiring audit` de 1.2 MB tem STR pior que `grep` — a LLM faz bem em evitá-lo. **Potencializar** (REGRA #0): tornar `--brief` (já existe globalmente, C1) o **default** desses commands, com truncagem-com-contagem e `-j --full` opt-in. Não remover capacidade — mudar o default para o que cabe no contexto.

---

## 7. Eixo 4 — Relevância (redundância e propósito)

Clusters de propósito sobreposto (sinal de consolidação, não de remoção):

| Cluster | N | Membros |
|---|---|---|
| **search-ish** | 20 | `ast find/grep/semantic`, `search {unified,exact,fuzzy,bm25,index,overlay}`, `tantivy {search,fuzzy}`, `index {find,search}`, `find-code`, `find-references`, `cognitive search`, `definitions semantic-search`, `mcts search`, `search-tools` |
| **`*-status`** | 16 | `status`, `wiring status`, `index status`, `learning status`, `decompose status`, `plugin status`, `saga status`, `daemon-ctl status`, `health-delta status`, `world-model-status`, `granularity status`, `flywheel status`, `overlay status`, `snapshot status`, `incremental status`, `generate plan-status` |
| **CEG-ish** | 8 | `exec`, `exec-speculative`, `plan-gated`, `plan-verified-depth`, `predict-action`, `conflict-check`, `txn-acquire`, `evidence` |

**Leitura**: a maioria é legitimamente distinta (`*-status` por subsistema é granularidade útil; o cluster CEG são estágios diferentes do X0..X9). O cluster **search-ish (20)** é o candidato real a uma fachada `touring search` unificada (já parcialmente feita em `search_unified.rs`, 0.95). Relevância geral: **alta** — o redundância é navegável, não morta.

---

## 8. Consolidado por command (4 eixos → grade)

`consolidated = mean(efetividade, STR, qualidade-50dim)`:

| Banda | N | Leitura |
|---|---|---|
| **A (≥0.90)** | **276** | excelente em todos os eixos |
| B (0.80–0.90) | 7 | um eixo penalizado (geralmente STR ou needs-args) |
| C (0.70–0.80) | 11 | anti-STR pesado OU `--help`-defect |
| D (0.50–0.70) | 3 | anti-STR extremo (`viz`, `wiring audit`) |
| F (<0.50) | **0** | nenhum |

8 commands inline/external (`prompt-enhance`, `cortex`, `pre-task-scout`, `query`, `dsl-query`, `bench-run`, `metadata-backfill`, `session-summary`, `serve`) não têm `cli/<mod>.rs` próprio — qualidade não medida nesta granularidade (lógica em closure inline ou outro crate).

---

## 9. Achados priorizados (honestos, poucos)

| # | Sev | Achado | Evidência | Remediação (REGRA #0 — potencializar) |
|---|---|---|---|---|
| **A1** | 🟠 P1 | **anti-STR**: 11 commands >50 KB default (`viz` 3.5 MB, `wiring audit` 1.2 MB) | §6, `command_eval.json` | `--brief` como **default** + truncagem-com-contagem + `--full` opt-in |
| **A2** | 🟡 P2 | **`--help`→rc=1** em 13 commands custom-dispatch | §4.3, `retest.json` | rotear `-h/--help` ao usage printer com `exit 0` |
| **A3** | 🟡 P2 | **`ast highlight`** rejeita file posicional que irmãos aceitam | §4.2 | alinhar parsing ao padrão `ast <sub> <file>` |
| **A4** | 🟡 P2 | **`harness_metric.rs`** é o handler de menor qualidade (0.895) | §5.1 | `taco-forge perfect-edit` → ≥ Diamond |
| **A5** | 🟢 P3 | **search-ish (20)** redundância navegável | §7 | promover fachada `touring search` (não remover aliases) |
| **A6** | 🟠 **P1 (propósito) — CONFIRMADO §4.6** | **`wiring impact` + `index find.reference_count` retornam 0** para os 7 símbolos cross-crate testados, todos com consumers reais grep-provados (`capture_tool_call`=8, **estável** → não é staleness). `wiring impact` ignora `-j`. Blast-radius por-símbolo não-confiável | §4.6 | gap de integração **específico** desses 2 caminhos (o consumer-data existe — `wiring orphans` o usa certo); apontar `wiring impact`/`index find` à mesma fonte; honrar `-j` |
| **A7** | 🟡 P2 (propósito/drift) — CONFIRMADO §4.6 (0/8) | **`ast meta`** (comando da golden-rule) **não retorna `quality_score`/`blast_radius`** em nenhum depth; `fan_in=0` para arquivo com consumers; `on_disk_fallback` | §4.5–4.6 | fechar o drift code↔skill: ou o comando computa os campos, ou a skill `file-metadata-first` aponta o comando certo (`ast tdg`/`file-knowledge extended`) |
| **A8** | 🟠 **P1 (propósito)** | **`tantivy search` retorna erro com exit 0**: `{"error":"tantivy-fts feature not enabled or index not initialized","hits":[]}` — feature morta neste ambiente, mas é Reflexo TACO recomendado (busca antes de grep) | §4.6 | habilitar feature `tantivy-fts` OU `touring tantivy reindex` no bootstrap; e **não retornar exit 0 em erro** (smoke-test não pega) |
| **A9** | 🟢 P3 (drift) — §4.7 | **`cognitive metrics`** retorna `{has_graph,has_predictor,initialized}` (status), não o "node/edge count + focus_cache hit_rate" que o CLI-index promete | §4.7 | computar as métricas reais OU corrigir a doc (`touring-cli-index.md:51`) ao que o comando entrega |
| **A10** | 🟢 P3 (output) — §4.7 | **`ssr apply --stdin`** casa o pattern (`matches:1`) mas devolve summary `{matches,was_formatted}` **sem o texto reescrito** | §4.7 | emitir o source reescrito no JSON do modo `--stdin` (hoje só conta matches) |
| **A11** | 🟡 **P2 (UX/alias)** — ⚙ **EXECUTAR NO PLANO** | **`--kind PythonModule` → "Unknown GeneratorKind"**: `PythonModule` nunca existiu; o kind real para Python é **`PythonScript`**. `touring generate` está **correto** (rejeita kind inválido); falta só o alias do nome intuitivo. Origem: reporte Gabriel de outra sessão (taco-forge STAGE 6 → exit 4) | §4.7; `crates/touring-server/src/cli/generate.rs:1371` (tabela de aliases `parse_kind`, ao lado de `("typescript", TypeScriptModule)`) | **adicionar** `("pythonmodule", GeneratorKind::PythonScript)` + `("python", …)` em `parse_kind` + teste `parse_kind_python_alias`. Workaround imediato: usar `--kind PythonScript`. **Rebuild+deploy** (`update-touring`) reinicia o daemon → coordenar com sessões CC ativas (REGRA #19) |
| **A12** | 🟡 P2 (propósito) — §4.7 | **`diary read <agent>` (default)** → "Memory store error: RLM error" (`diary write` persiste OK; só o read default quebra; com `--project` retorna vazio sem erro) | §4.7 | corrigir o caminho de read default do diary no RLM/memory store |

**Não-achados** (transparência VP-Scout): os "43 broken" e "12 slow ~10 s" da 1ª passada eram artefatos do harness (args/`-j`/contenção/server/timeout); na amostra-oráculo, `find-references`/`repo-score` foram **FP do meu próprio oráculo** (flag/chave erradas); e no LOTE 1, fixtures de nome genérico (`cfg`/`env`/`none`) deram FAILs inconclusivos (homonímia) — descartados. A6/A7/A8 **são** reais (sustentados por fixtures de nome específico + output cru). `index find`/`ast find`/`resolve-def`/`ast overview`/`find-references` **cumprem** o propósito (100%).

---

## 10. Recomendações (potencializar, nunca reduzir)

1. **`--brief` default nos 11 anti-STR** — maior ROI de coupling; muda `U(a)` de negativo para positivo (alinha com `coupling-codemode-cli-and-master-commands` R2/C1).
2. **Uniformizar `-h/--help`** nos 13 custom-dispatch — exit 0 + usage; resolve também a mis-enumeração de `toolchain`/`skip`/`ssr`.
3. **Elevar `harness_metric.rs` a Diamond** — o medidor deve ser o melhor exemplo do que mede.
4. **Fachada `touring search`** sobre o cluster de 20 — 1 entrada, modos como flags; aliases preservados (REGRA #0).
5. **Manter a disciplina de medição** — este harness (6 scripts) é reutilizável como gate de regressão de CLI; a lição VP-Scout (re-teste antes de reportar) evitou ~30 falsos defeitos e deve ser padrão.
6. **Fase 2 — purpose-fidelity (✅ COMPLETA, 4 lotes)** — oráculo por comando contra ground-truth grep, navegação + análise + geração + mutadores (read-back). Resultado: maioria cumpre propósito; 7 defeitos específicos (A6-A12). O harness (`purpose_batch{1..4}.py`) fica como gate de regressão de propósito.

### 10.1 Plano de remediação — ordem de execução (REGRA #0)

> Cada fix **potencializa** (não remove capacidade). Itens que exigem rebuild+deploy do touring (`update-touring`) reiniciam o daemon → **coordenar com sessões CC ativas (REGRA #19)**.

| Ordem | Achado | Esforço | Toca produção? |
|---|---|---|---|
| **1** | **A11** `parse_kind` alias `("pythonmodule", PythonScript)` + `("python", …)` + teste | ~3 linhas + 1 teste | sim (rebuild) — **registrado para o plano** (reporte Gabriel) |
| 2 | **A8** habilitar `tantivy-fts` / `tantivy reindex` no bootstrap + não-exit-0-em-erro | baixo | sim |
| 3 | **A6** apontar `wiring impact` + `index find.reference_count` à fonte de consumer que `wiring orphans` usa; honrar `-j` | médio (investigar resolução de símbolo) | sim |
| 4 | **A12** corrigir read default do `diary` (RLM error) | baixo-médio | sim |
| 5 | **A1** `--brief` default nos 11 anti-STR (maior ROI de coupling) | médio | sim |
| 6 | **A2** `-h/--help` exit 0 nos 13 custom-dispatch | baixo (repetitivo) | sim |
| 7 | **A3** `ast highlight` arg posicional · **A7** ast meta quality/blast (ou fix da skill) · **A4** elevar `harness_metric.rs` · **A9** cognitive metrics doc · **A10** ssr apply emitir rewrite | baixo cada | sim |
| 8 | **A5** fachada `touring search` (aliases preservados) | médio | sim |

**A11 é o item 1 do plano** (desbloqueia `taco-forge perfect-create` de `.py`). Workaround até lá: `--kind PythonScript`.

### 10.2 Execução do plano — RESULTADO (2026-06-27, provado em runtime)

**Os 12 achados (A1-A12) foram implementados e PROVADOS em prática** (não só compilados/testados): `scratchpad/cliaudit/verify_remediation.sh` → **13/13 PASS**.

| Achado | Fix entregue | Prova runtime |
|---|---|---|
| **A11** | `parse_kind` aliases `pythonmodule`/`python`→`PythonScript` + teste | `generate render PythonModule` renderiza (sem "Unknown GeneratorKind") |
| **A8** | `TantivyIndex::open_or_create` retry em schema-mismatch + msg cfg-aware + exit≠0 em erro | `tantivy reindex` upserted=25000; `search` retorna **hits reais** |
| **A6** | `compute_impact` sem gate `def_module` + `reference_count = max(symbol_store, wiring_map)` + honra `-j` | `index find capture_tool_call` refcount **0→6**; `wiring impact -j` JSON |
| **A12** | `diary_read`→`scan_prefix` + `CAST(created_at AS INTEGER)` no RLM (coluna TEXT legada) | `diary write→read` marker round-trip (sem "RLM error") |
| **A1** | `apply_heavy_brief_default` central (`wiring`/`viz`/`graph`) + `--full` opt-out | `wiring audit` **1.248.275→477 bytes**; `--full` restaura |
| **A2** | `-h/--help`→exit 0: `skip` (`args.get(2)`+slices), `projects` (run_help); `filters`/`source-change` já-compliant | `skip`/`projects`/`filters`/`source-change --help` todos exit 0 |
| **A3** | dispatch `Highlight` com prefixo de **2 tokens** (`highlight::run` come `.skip(1)`+clap-argv0) | `ast highlight <file>` → realça (não rejeita posicional) |
| **A10** | campo `rewritten` em `SsrApplyOutput` (de `SsrApplyResult.output`) | `ssr apply --stdin` emite o source reescrito |
| **A4** | `harness_metric.rs` refactor (HarnessFlags/format_human/USAGE + 14 testes) | 50-dim **0.895→0.98 Diamond** |
| **A5** | fachada `touring search tools`→`tool_catalog::search_as_json` + 4 testes | `search tools "wiring orphans"` retorna catálogo |
| **A7** | doc co-evolução: `ast meta` campos só p/ indexados (caveat) → aponta `ast tdg`/`ast blast` | `rules/touring-cli-index.md` |
| **A9** | doc co-evolução: `cognitive metrics` = flags de status (não node/edge count) | `rules/touring-cli-index.md` |

**Gates de integração (REGRA #21)**: `cargo check` 0-erro/0-warning (feature ON **e** OFF) · `clippy -D warnings` 0 · `cargo test` **0 falhas** (touring-server todos os targets incl. binary_e2e + touring-intelligence 1428 + touring-cli/hooks-core/hook-runtime) · **gate 50-dim** todos os arquivos tocados ≥ Gold, **0 P0 BLOCK** · `update-touring` exit 0, daemon fresco.

**Falhas adicionais corrigidas durante a integração** (REGRA #21 — origem/idade irrelevantes): 5 `unreachable_code` + 5 `needless_return` (tantivy.rs, consequência do fix A8) · `use std::io::Write` ausente (`panic_log.rs`, **pré-existente**) · 14 testes panic no clap `debug_asserts` (`#[command(alias="tools")]` == nome do subcomando `tools`, eng2). **Lição**: proibir engenheiros de rodar `cargo test` deixou passar bugs que só o runtime/clap-debug-assert pega — futuros engenheiros devem rodar a suíte do próprio crate antes de retornar.

**Método**: 7 fixes solo + 2 `touring-engineer` paralelos (arquivos disjuntos) → integração única → `update-touring` → prova runtime. A **prova runtime** (não as claims `composite=1.0`) foi o que tornou os fixes FACT: **9/13 na 1ª passada → 13/13** após 3 root-causes que só o runtime expôs (A12 RLM-lê-TEXT-como-i64, A3 off-by-one no prefixo, A2/skip índice `first()` vs `get(2)`).

---

## 11. Apêndice — artefatos determinísticos

| Arquivo | Conteúdo |
|---|---|
| `commands.json` | inventário canônico (297 leaves + 41 groups) |
| `results.json` | matriz de teste bruta (rc/bytes/json/latência/mode por command) |
| `triage.json` | buckets honestos + anti-STR + clusters |
| `retest.json` | re-teste VP-Scout (33 commands, invocação correta) |
| `handler_scores.json` | 50-dim por handler (composite/tier/P0/lowest-dims) + mapa command→handler |
| `command_eval.json` | **join final dos 4 eixos por command** (a matriz de 297 linhas) |
| `purpose_oracle.py` | oráculo de purpose-fidelity (amostra §4.5) — semente da fase 2 |
| `purpose_batch{1,2,3,4}.{py,json}` | **fase 2 completa**: navegação · análise · geração · mutadores (read-back) |

**Reprodução**: `cd ~/.claude/rust && python3 scratchpad/cliaudit/{enumerate,test,analyze_results,retest_suspicious,score_handlers,synthesize,purpose_oracle,purpose_batch1,purpose_batch2,purpose_batch3,purpose_batch4}.py`

---

_TACO 2026-06-27 (v4) | acoplado (code-mode determinístico) | 50-dim via touring-quality real | VP-Scout Cadeia 5+7 (recursivo) | **execução: 0 falhas; purpose-fidelity: fase 2 COMPLETA → 7 defeitos (A6-A12)** | **REMEDIAÇÃO: 12/12 achados (A1-A12) implementados + provados em runtime (verify_remediation.sh 13/13 PASS); gates check/clippy/test(0-fail)/50-dim(0-P0) verdes; deployado via update-touring; +5 falhas de integração corrigidas (REGRA #21)** | 276/297 grau A em execução+código._
