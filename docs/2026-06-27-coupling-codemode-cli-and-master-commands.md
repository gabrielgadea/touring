# Proposta — Code-Mode via CLI/API + Master CLI Commands (resolver adoção antes da telemetria)

> **Data**: 2026-06-27 | **Tipo**: proposta de arquitetura | **Autoridade**: Gabriel Gadea
> **Precede**: a telemetria (`2026-06-27-coupling-telemetry-infrastructure.md`) — resolver o canal de
> execução e a densidade dos resultados muda **como todo trabalho posterior é feito**.
> **Fundamenta-se em**: verificação empírica de 15 componentes (rodados ao vivo), a engine real
> `ctx_execute_impl` (`ctx_execute_tools.rs:176`), os docs `2026-06-26-*` (I2/DADL, P3), e o diagnóstico
> de falha de adoção (`2026-06-27-coupling-adoption-failure-diagnosis.md`).
> **Posição no arco** (cross-reference dos 9 nós): este é o **nó ⑥** — mapa mestre em
> `2026-06-27-coupling-roadmap-master-crossref.md`. Refinamentos verificados pós-cross-reference em **§8**.

---

## 0. Sumário

Antes de medir a adoção (telemetria), é preciso **tornar o caminho acoplado adotável**. A verificação
empírica abaixo prova que o caminho touring hoje **pune quem o toma**: produz megabytes (anti-STR), dá
vereditos errados com confiança quando o índice está degradado, e o único canal de code-mode (MCP) está
desligado. A não-adoção é **parcialmente racional** — não é só prior da LLM.

A proposta tem duas pernas:
1. **Code-Mode sem MCP** — um comando **CLI** `touring run` (wrapper de ~40 linhas sobre a engine
   `ctx_execute_impl` que **já existe**) + um **SDK/API** ("touring como a *lib* dos scripts", P3 dos docs)
   para orquestração. CLI resolve o canal; API resolve a ergonomia de orquestrar.
2. **Master CLI Commands** — promover os 11 scripts Layer-3 + os master-ish a comandos `touring` nativos,
   **mas só após corrigi-los** (densidade `--brief` default, fail-soft, < 2 KB) — senão promoveríamos defeito.

O fio condutor: **mudar `U(a)`** (afordância estrutural barata), não persuadir. Um caminho acoplado denso,
correto e de 1-comando tem `U` positivo → é adotado por construção.

---

## 1. Verificação empírica de efetividade (a evidência) `[FACT 1.0]`

Rodei 15 componentes sobre alvos reais (harness `verify_components.py`, determinístico). Matriz:

| Componente | rc | output | tempo | veredito de efetividade |
|---|---|---|---|---|
| `diagnose_wiring.py` | 0 | **4.9 MB** | 8.2s | ⛔ envelope catastrófico (carrega o `audit` inteiro); lógica OK (`real_orphan_count=0`) |
| `discover_workspace.py` | 0 | **125 KB** | 0.3s | ⛔ despeja workspace inteiro |
| `diagnose_health.py` | 1 | 18 KB | 0.5s | 🟡 veredito correto (YELLOW 0.679, 5/5), envelope grande |
| `read_file.py` | 1 | 10.8 KB | 0.1s | ⛔ **falso positivo**: disse RED/quality=0.00 p/ arquivo **Diamond 0.977** |
| `discover_symbol.py` | 0 | 930 B | 2.8s | ⛔ **falso negativo**: `found=0` p/ `ctx_execute_impl` (existe em `:176`) |
| `pre_edit_gate.py` | 0 | 1.5 KB | 0.5s | ✅ conciso, veredito claro |
| `analyze_blast.py` | 0 | 673 B | 0.1s | ✅ conciso |
| `analyze_callers.py` | 0 | 560 B | 0.1s | ✅ conciso |
| `analyze_quality.py` | 0 | 12.8 KB | 1.0s | 🟡 útil mas grande |
| `vgp_batch.py` | 1 | 962 B | 0.1s | 🟡 conciso (rc=1 por símbolo não-indexado — degradação) |
| `repo-score` | 0 | 2 KB | 0.2s | 🟡 conciso, mas grade **D** |
| `repo-health` | 0 | 2.5 KB | 0.3s | ✅ markdown executivo conciso |
| `harness-metric` | 0 | 193 B | 0.0s | ⛔ **composite 0.132** (stateful=0, evolving=0) — auto-medição alarmante, ignorada |
| `kpi` | 0 | 2.3 KB | 0.2s | ✅ conciso, falsificável |
| `exec` | 0 | 138 B | 0.0s | ⛔ **gate-only** — não executa, não devolve resultado |
| `ctx_execute` (engine) | — | — | — | ⬜ existe (`:176`), compilado e em `CURATED_TOOLS`; **inalcançável ao vivo**: MCP *server* não conectado + sem CLI (ver §8) |

### As 4 falhas sistêmicas

1. **Densidade catastrófica (anti-STR)** `[FACT]` — `diagnose_wiring` 4.9 MB, `discover_workspace` 125 KB,
   `read_file` 10.8 KB. Violam o princípio nuclear do backend. O LLM que rodasse `diagnose_wiring` receberia
   **4.9 MB** → custo de token proibitivo → `U(a)` fortemente negativo. **O caminho acoplado tem STR pior que
   `grep`** — é ativamente punitivo, não só "subutilizado".
2. **Vereditos errados com confiança (sem fail-soft)** `[FACT]` — `discover_symbol` falso-negativo,
   `read_file` falso-positivo, ambos porque confiam no índice/`ast meta`/tantivy que estão **degradados** nesta
   sessão e **não aplicam Cadeia 7** (confirmar via grep). Dar resposta errada com confiança é **pior que não
   rodar** — corrói a confiança no caminho acoplado.
3. **Auto-medição alarmante e ignorada** `[FACT]` — `harness-metric` = **0.132**, `repo-score` = **D**. O
   sistema se mede como imaturo e **ninguém consome** a medição (loop aberto — o mesmo G2 do diagnóstico).
4. **Canais de execução quebrados** `[FACT, refinado §8]` — `exec` é gate-only; `ctx_execute` existe (`:176`,
   em `CURATED_TOOLS`) mas seu único canal é o MCP *server*, que **não estava conectado** naquela sessão — e
   não há canal **CLI**. O código de execução não falta; falta o **canal ergonômico** (o que R1 entrega).

### O que funciona (a base sólida a preservar)
A **lógica** dos scripts é boa: `diagnose_wiring` detecta `real_orphan_count=0` corretamente; `diagnose_health`,
`pre_edit_gate`, `analyze_blast`, `kpi`, `repo-health` são concisos e corretos. **O problema é o envelope e a
robustez, não o miolo.** Logo: corrigir, não reescrever.

---

## 2. CLI vs API — a avaliação (resposta à pergunta)

| Pergunta | Resposta | Por quê |
|---|---|---|
| Como o LLM **submete** code-mode? | **CLI** (`touring run`) | usa o prior bash→cli (P3); não depende do MCP (que está off); 1 wrapper sobre engine existente |
| Como o código **orquestra** touring de dentro? | **API/SDK** (`touring` como lib) | P3 verbatim: *"fazer touring ser a lib natural desses scripts"*; mais ergonômico e seguro que `subprocess` (que o sandbox bloqueia) |
| Master workflows comuns? | **Master CLI commands** | os workflows fixos (scout/read/health) viram 1 comando — afordância de custo mínimo |

**Veredito**: não é "CLI ou API" — são **camadas complementares**. CLI é o *canal*; a API é o *vocabulário*
dentro do canal; os master commands são os *atalhos congelados* dos workflows mais comuns.

---

## 3. Code-Mode sem MCP — arquitetura

### Camada 1 — `touring run` (compute-in-code) · reuso máximo · **S**
Um adaptador CLI espelhando o adaptador MCP (`tools_ctx_execute.rs`, 30 linhas), sobre a **mesma engine**:

```
touring run --lang python --file script.py            # ou --stdin, ou --code '...'
            [--args '["a","b"]'] [--timeout-ms 30000] [--allow-forbidden] [--brief]
```
- Chama `ctx_execute_impl(language, code, args, timeout_ms, cwd, allow_forbidden)` → `{stdout, stderr,
  exit_code, duration_ms, forbidden_calls, *_truncated}`.
- `--brief` aplica o **C5 Active Summarizer** ao stdout (< 200 tokens; preserva exit-code + error_lines).
- 11 linguagens, forbidden-call detection, 1 MB cap — **tudo já implementado**. Custo: ~40 linhas + registro
  no `command_table.rs`. **Resolve o gap do MCP-off para compute-in-code imediatamente.**

### Camada 2 — SDK `touring` + `touring run --orchestrate` (orchestrate-in-code) · **M**
O sandbox de code-mode bloqueia `subprocess.run` — então o código **não pode** chamar `touring` por subprocess.
Para orquestrar (1 script → N comandos touring → sumário), o caminho limpo (alinhado a I8 "daemon = Shared
Context Store"):
- um **binding leve** (`touring` em python/js) que fala com o **socket Unix do daemon** via RPC (sem subprocess,
  sem reimplementar): `from touring import index, ast, wiring; index.find("Foo")`;
- `touring run --orchestrate` concede a capability `Net(daemon-socket)` ao sandbox (deny-by-default relaxado só
  para o socket do daemon).
- O resultado: `touring run --orchestrate --file plan.py` roda um script que orquestra a stack inteira em 1
  execução, devolvendo só o sumário — o **−60-85% tok** do CodeAct, sem MCP.

### Reuso (VGP-verificado)
| Peça | Já existe | Local |
|---|---|---|
| Engine de execução | ✅ `ctx_execute_impl` | `crates/touring-server/src/tools/ctx_execute_tools.rs:176` |
| Adaptador (a espelhar) | ✅ MCP (`tools_ctx_execute.rs`) | mesmo crate |
| Gate de capabilities | ✅ CEG X0–X9 | `touring-ceg` |
| Inferlets WASM | ✅ 17 | `touring inferlets run` |

---

## 4. Master CLI Commands — promover **após corrigir**

Assim como criamos as master **MCP tools** (`touring_audit`, `touring_search`, `touring route`), criamos master
**CLI commands** — mas a verificação (§1) proíbe promover os scripts como estão. **Pré-condição de promoção (3
invariantes)**:
- **I-densidade**: output `--brief` por **default** (< 2 KB; array gigante elidido com contagem). **Já parcialmente entregue** `[VERIFICADO §8]`: A1/C1 `--brief` default em produção (`main.rs:252`, `apply_heavy_brief_default`; `wiring audit` 1.248.275→477 B).
- **I-fail-soft**: índice degradado → **Cadeia 7** (confirmar via grep) → nunca veredito errado com confiança; marcar `degraded:true`.
- **I-correção**: usar a fonte de verdade certa (`touring-quality` 50-dim, não o `ast meta` quality_score que deu 0.00 num arquivo Diamond).

### Catálogo proposto (cada um: 1 comando, < 2 KB, fail-soft)
| Master command | Funde | Substitui o script | Estado |
|---|---|---|---|
| `touring scout <sym>` | index find + ast find + wiring impact + memory + gotcha | `discover_symbol.py` (corrigir falso-neg) | corrigir |
| `touring read <file>` | ast meta + blast + tdg + **touring-quality** + rust-semantic | `read_file.py` (corrigir falso-pos) | corrigir |
| `touring health` | doctor + status + gate-metrics + learning + drift | `diagnose_health.py` (só `--brief`) | quase pronto |
| `touring guard <file>` | pre-edit gate (blast+tdg+gotcha+memory) | `pre_edit_gate.py` | pronto |
| `touring audit <file>` | offensive CWE + 6 P0 quality | `touring_audit` (MCP→CLI) | ✅ promovido (`cli/audit.rs`) |
| `touring map [dir]` | workspace-info + wiring chains + quality sweep | `discover_workspace.py` (corrigir 125 KB) | corrigir |
| `touring blast <files>` | blast + wiring impact + cross-feature + cycles | `analyze_blast.py` | pronto |
| `touring investigate <topic>` | search + index + wiring chains + memory → mapa | (novo) | construir |

> Os já-nativos (`repo-score` D, `harness-metric` 0.132, `repo-health`, `kpi`) **permanecem** mas entram na
> fila de correção — o `harness-metric` 0.132 é, ele mesmo, o **gate de qualidade** que mede se um master
> command atinge o padrão (executable/inspectable/stateful/governed/performant/evolving).

---

## 5. A ligação com a adoção — por que isto resolve antes da telemetria

```
componente denso (4.9MB) + canal off + veredito errado  →  U(a) NEGATIVO  →  LLM evita (racional)  →  não-adoção
     ↓ (esta proposta)
master command <2KB + touring run (canal) + fail-soft correto  →  U(a) POSITIVO  →  caminho de menor resistência  →  adoção por construção
```
`[INFERENCE 0.9]` A telemetria de adoção/uptake só faz sentido **depois** disto: medir uptake de um caminho que
pune é medir uma escolha racional de evitá-lo. Corrigir densidade + canal + correção **muda `U(a)`** — e então
todo trabalho posterior (inclusive a própria telemetria) é feito de forma acoplada, porque o acoplado passa a
ser o mais barato. Afordância, não sermão.

---

## 6. Roadmap (T-shirt, dependências acíclicas)

| Fase | Entregável | Tam | Dep | Risco |
|---|---|---|---|---|
| **R1** ✅ | `touring run` (Camada 1) — wrapper sobre `ctx_execute_impl` + `--brief` (C5) | **S** | — | **DONE 2026-06-27**: `cli/run.rs` + touring-ceg dep; build/test(5)/clippy 0; 50-dim Diamond 0.9751, 6/6 P0; deployed + runtime-proven (compute, --brief, exit-prop, forbidden) |
| **R2** | 3 invariantes (densidade/fail-soft/correção) aplicados aos 11 scripts + `lib_touring.py` | **M** | — | MED — corrigir falso-pos/neg |
| **R3** ✅ | Master commands `scout/read/health/guard/map/blast` (CLI wrappers sobre os scripts Layer-3 R2) | **M** | R2 | **DONE 2026-06-28**: `cli/master.rs` (`script_for` + `forward`, propaga exit-code, `args[2..]` verbatim) + 6 entradas `command_table`; deployed + runtime-proven **sem MCP** (scout `index_count:1`, guard GO 0.77, map, health YELLOW 0/1/2). 8 unit tests, clippy 0, master.rs 50-dim **Diamond 0.9797**. `audit` **promovido a CLI** (2026-06-28): `cli/audit.rs` adaptador sobre `run_audit` (padrão MT-1 — 1 engine, 2 adaptadores MCP+CLI), exit-code = verdict (Info=0/Warn=1/Block=2), 7 tests, **Diamond 0.9757**; release-built + runtime-proven sans-MCP (`touring audit <file> -j` → `{verdict,block_count,warn_count,info_count,findings}` exit 0). **Bonus REGRA #21**: corrigido bug sistêmico de parsing do envelope `{count,definitions}` de `index/ast find -j` (helper `parse_definitions` em `lib_touring`) em **5 scripts** (scout/callers/guard/vgp/investigate estavam sempre-vazios). **Bonus REGRA #21 (regressão pós-deploy)**: `build_consumer_generator_plans` (`generator_tools.rs`) tornado **fail-soft** — `wiring orphans` indisponível (exit≠0/stdout vazio, ex.: DB per-project quase-vazio resolvido pelo cwd) agora devolve `{ok:true,count:0,plans:[],degraded:true,note}` em vez de quebrar (`test_consumer_wiring_cli_exits_0` verde; helpers `degraded_plans`+`fetch_wiring_orphans_raw`, CC mantido em 18). Root cause arquitetural — o walk-up de project-DB trata **qualquer** `.claude/` como boundary → 29 `.claude/touring/symbols.db` stray em subdirs de `crates/`; o fix durável é a robustez no código (DBs seriam recriados), o fix do walk-up pertence à Fase 1 (daemon per-project) da productization. |
| **R4** ✅ | SDK `touring` + `touring run --orchestrate` (Camada 2, socket RPC) | **L** | R1 | **DONE 2026-06-28**: `cli/run.rs` — SDK Python read-only (`query` + `index_find`/`ast_blast`/`ast_overview`/`wiring_status`/`search`, payload-keys VGP-verificados) + flag `--orchestrate` (injeta o SDK; exige `--lang python`). build/clippy 0, 9 run-tests, 50-dim **Diamond 0.9793**, 6/6 P0; deployed + runtime-proven (3 queries reais via socket, **sem MCP**). **C12 NÃO era pré-req** — o canal usa socket RPC direto (`daemon_client` wire format); `plan_tool_chain` fica para R4.1 (planejar a cadeia de queries). Ver §8.7 (descoberta de segurança). |
| **R5** ✅ | `touring investigate` (novo) + auto-invocação opt-in no SessionStart | **M** | R3 | **DONE 2026-06-28**: `investigate.py` (topic map search+index+wiring+memory, 3 invariantes R2) + comando `master::investigate`; hook `investigate_on_start.py` (SessionStart, **opt-in default-OFF** via `TOURING_INVESTIGATE_ON_START=1`, fail-open exit 0) wirado em settings.json (6ª entrada, JSON validado). Runtime-proven (`symbol_matches:1`, 10 search hits); investigate.py 50-dim **Diamond 0.9816**. |
| **R6** ✅ | `harness_gate.py` — gate 50-dim (≥ 0.80) da superfície dos master commands | **S** | R3 | **DONE 2026-06-28**: `harness_gate.py` pontua master.rs + 7 scripts + lib_touring via `touring-quality` (I-correção, não proxy); **PASS 9/9 ≥ Platinum** (master.rs 0.9797, investigate.py 0.9816, scripts 0.937–0.972). Gate executável (exit 1 abaixo do floor, exit 3 unverified). NB: o `touring harness-metric` 6-eixos pré-existente é distinto (KPI de gate-metrics, não confundir). |

**Caminho crítico**: R1 (canal, quick win) → R2 (corrigir o que existe) → R3 (master commands) → R4 (orquestração — **precisa do C12 caller**, §8).

---

## 7. Riscos e mitigações

| Risco | P×I | Mitigação |
|---|---|---|
| Promover scripts defeituosos | (corrigido) | §4 — 3 invariantes são pré-condição de promoção |
| `touring run --orchestrate` abre vetor de ataque (MAESTRO) | MED×HIGH | capability só `Net(daemon-socket)`, deny-by-default no resto; CEG X2 forbidden-call mantido |
| Densidade volta a crescer | MED×MED | `harness-metric` (R6) gateia; `--brief` é default, não opt-in |
| Índice degradado dá veredito errado | (mitigado) | I-fail-soft (Cadeia 7) obrigatório; `degraded:true` explícito |
| Duplicar o que o MCP já faz | LOW | é o mesmo padrão `run_audit`+adaptador (MT-1): engine única, 2 adaptadores |

---

## 8. Refinamentos verificados pós-cross-reference (2026-06-27)

> Verificação VP-Scout (Cadeia 5/6) de 4 Explore agents paralelos contra o código real, durante o
> cross-reference dos 9 nós (`2026-06-27-coupling-roadmap-master-crossref.md`):

1. **"MCP desligado" → MCP *server* não conectado, não tool ausente** `[VERIFICADO]`. `ctx_execute_impl`
   existe (`ctx_execute_tools.rs:176`), está em `CURATED_TOOLS` (`server/mod.rs:73-104`) **sem** cfg guard.
   Faltava o *server* MCP conectado + um canal CLI. → **R1 entrega o canal CLI**; o código de execução já existe.
2. **C12 (MCTS tool-planning) é pré-requisito de R4, não de R1** `[VERIFICADO]`. `plan_tool_chain()`
   (`tool_planning.rs`) está pronta (Diamond, testes verdes), exposta via `reason_tools.rs:16`; o **indutor
   automático** do snippet code-mode no cli-suggester é follow-up. R1 Camada 1 **não** depende; R4 **depende**.
3. **I-densidade já parcialmente entregue** `[VERIFICADO]`. A1/C1 `--brief` default em produção (`main.rs:252`).
   Resta fail-soft (Cadeia 7) + correção (fonte de verdade `touring-quality`).
4. **Duas camadas de curadoria MCP** `[VERIFICADO]`: (a) `CURATED_TOOLS` filtra o `list_tools` response (já
   funciona, sem flag); (b) `#[cfg(feature = "mcp-curated")]` compila as W2 tools. Propostas devem nomear a camada.
5. **Master commands reusam engines prontas do backlog** `[VERIFICADO]`: `touring read`=C3+C5; `touring
   health`=MT-1 `touring_audit`; `touring route`=C7 (já existe); densidade=C1+C2+C3. O doc **orquestra**, não
   duplica — só `touring run` + C12 caller são trabalho novo.
6. **Retirado** `[NÃO-CONFIRMADO]`: "4/15 systemic failures" não consta do adoption-failure (63 linhas);
   "+22pp harness>modelo" vem de `harness-architecture-insights.md`, não da coupling-strategy.

7. **Descoberta de segurança — o canal de orquestração já existe desde R1** `[VERIFICADO empiricamente 2026-06-28]`.
   A premissa de que R4 precisaria *conceder* a capability `Net(daemon-socket)` está **incorreta**: o sandbox
   do `touring run` (R1) **já alcança** o socket do daemon. Prova runtime: um `touring run --lang python` que faz
   `socket.connect("/tmp/touring-daemon-1000.sock")` → conecta, envia, recebe (`{"output":...,"success":true}`).
   Causa: (a) `socket` **não** está na forbidden-list Python (`forbidden_patterns.rs` lista só eval/exec/subprocess/
   os.system/pickle/os.remove — não rede); (b) o `Net(*)=Deny` do `enforce_linux.rs` é o perfil do CEG
   `run_gateway` X0..X9, mas `ctx_execute_impl` (que `touring run` usa) aplica **só** a forbidden-detection estática;
   (c) o landlock do sandbox permite `/tmp` (`os.path.exists(sock)==True`). **Implicação**: `--orchestrate` adiciona
   *ergonomia* (o SDK Python), **não** um vetor novo — o vetor já abriu com R1. O SDK expõe só queries read-only, mas
   o `query()` genérico (e o socket cru) permitem qualquer hook, inclusive mutação. **Hardening proposto (follow-up,
   mitigação MAESTRO)**: (i) confinar o socket no sandbox por padrão (landlock ruleset sem `/tmp` ∪ seccomp em
   `connect(AF_UNIX)`) e liberar só sob `--orchestrate`; (ii) allowlist read-only server-side (o daemon recusa hooks
   de mutação vindos de contexto orchestrate). Decisão de Gabriel — é mudança de superfície de segurança.

---

## Referências
- Verificação empírica: `verify_components.py` (15 componentes, este doc §1)
- **Mapa mestre do arco** (9 nós, cross-reference verificado): `2026-06-27-coupling-roadmap-master-crossref.md`
- Backlog (C12 caller = pré-req de R4): `2026-06-26-coupling-backlog.md`
- Engine code-mode: `crates/touring-server/src/tools/ctx_execute_tools.rs:176` (`ctx_execute_impl`); adaptador MCP `tools_ctx_execute.rs`
- Diagnóstico de adoção: `docs/2026-06-27-coupling-adoption-failure-diagnosis.md`
- Telemetria (posterior): `docs/2026-06-27-coupling-telemetry-infrastructure.md`
- Docs de research: `2026-06-26-{touring-llm-coupling-strategy (P3),harness-architecture-insights (I2/DADL),touring-capability-map}.md`
- Scripts Layer-3: `~/.claude/skills/Touring/scripts/` (11) + `repo-score`/`repo-health`/`harness-metric`/`kpi`
