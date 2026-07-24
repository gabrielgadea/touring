# Diagnóstico de Footprint de Tokens — Touring ↔ Claude Code ↔ LLM

> **Data**: 2026-06-26 | **Sessão de medição**: `e0f553d0-1efc-46fc-b1cf-e13dd8c3de2d`
> **Autor**: TACO (Opus 4.8 1M) sob ordem de Gabriel Gadea
> **Objetivo**: Mapear *absolutamente todo* ponto de contato do Touring com o Claude Code e
> com a LLM, quantificar o consumo de tokens de cada um, separar o aproveitado do desperdiçado,
> e responder se o `sccache` ajuda ou atrapalha o contexto.
> **Método**: medição empírica — `tiktoken cl100k` nos markdowns, `usage` real do transcript
> (`cache_creation`/`cache_read`), handshake MCP JSON-RPC real, execução dos hooks via o
> mesmo `stdin` que o Claude Code usa. Nada aqui é estimado sem marcação `[INFER]`.

---

## 0. Sumário executivo

1. **A impressão de Gabriel está correta e é mensurável.** O contexto-base desta sessão, antes de
   qualquer trabalho, é **~223.000 tokens** (`usage` do turno 1: input 28.495 + cache_creation
   178.252 + cache_read 16.054) `[FACT]`. **62% da base estática é markdown constitucional do
   Touring** (110.624 tokens medidos com tiktoken).

2. **Maior peça única estática: `rules/quality/` (51 D-rules) = 54.332 tokens** `[FACT]` — 30% da
   base inteira — carregada em **toda** sessão, inclusive nesta, que é uma conversa em
   `/home/gabrielgadea` (projeto `node`) sem nenhuma aplicação de quality-work de código Rust.

3. **Dois vilões de saída brutais** `[FACT]`: `touring status -j` = **41.703 tokens** e
   `touring wiring orphans -j` = **173.251 tokens** num único comando — e o hook `cli-suggest`
   **sugere ativamente rodá-los** ("MUST touring status -j") em cada chamada de Bash.

4. **MCP do Touring expõe 158 tools = 37.786 tokens de schemas** `[FACT]`. Nesta sessão estão
   **inativos** (cwd `/home/gabrielgadea`, daemon com `Permission denied`), mas em qualquer sessão
   dentro de `~/.claude/rust` somam **+38K tokens** à base. A "curadoria 102→22" registrada na
   memória **não está aplicada** no server real (drift).

5. **`cli-suggest`** dispara em **9 tipos de tool**; **57% de cada injeção é ruído repetido** — a
   mesma lista de 5 *past-failures* truncadas de 2026-05-11 em todo Bash `[FACT]`.

6. **O prompt-cache da Anthropic ESTÁ funcionando** (86% cache-read) `[FACT]` — sem ele o custo
   seria 5-10× pior. Mas cache não emagrece a base; só barateia relê-la (a 0,1×).

7. **`sccache` é ORTOGONAL a tokens** — é cache de *compilação Rust*, afeta tempo de build, **não**
   a janela de contexto. Está com **2,72% de hit em Rust** `[FACT]` (ruim), mas isso é outra frente.

---

## 1. Taxonomia — as 6 camadas de contato

| Camada | O que é | Quando custa | Custo Touring |
|---|---|---|---|
| **A. Estático** | CLAUDE.md, rules/, quality/, MEMORY.md, descrições de agents/skills/commands | **toda** sessão | **~113K tok** |
| **B. Hooks dinâmicos** | `additionalContext` injetado por evento | por sessão / prompt / tool | **~50K tok/sessão** |
| **C. MCP schemas** | `tools/list` do `touring serve` (158 tools) | só com cwd em workspace touring | **+38K tok (condicional)** |
| **D. Saídas CLI/MCP** | tool-results de `touring … -j` | sob demanda (mas hooks induzem) | **até 173K tok/comando** |
| **E. Subagentes** | system-prompts dos 5 `touring-*` + re-injeção de rules | ao spawnar agent | **~43K tok/agent + rules** |
| **F. Build/infra** | sccache, processos daemon | — | **0 tok (ortogonal)** |

---

## 2. Camada A — Contexto estático (injetado em TODA sessão)

Medição exata com `tiktoken cl100k_base`:

| Componente | Tokens | % da base | Aproveitamento |
|---|---:|---:|---|
| `rules/quality/` (51 D-rules) | **54.332** | 30% | ⚫ Nulo fora de quality-work de código |
| `rules/` top-level (14 arquivos) | **41.636** | 23% | 🟡 Relevante em code-work, não em conversa |
| `CLAUDE.md` | **8.703** | 5% | 🟢 Constituição (mas 437 linhas) |
| `MEMORY.md` | **5.953** | 3% | 🟢 Já compactado 93→16KB |
| Descrições dos 5 agents touring (na lista) | **~2.172** | 1% | 🟡 Parágrafos Wave 4/Wave 12 verbosos |
| **SUBTOTAL markdown Touring** | **110.624** | **62%** | |
| Catálogo de skills (516 SKILL.md, fração touring=36) | ~44.000 | 25%¹ | 🟡 449 de plugins, maioria nunca usada |
| Schemas dos tools nativos (Bash/Edit/Agent/Workflow…) | ~18.000 | 10%¹ | 🟢 Necessário |
| MCP instructions + SessionStart inject | ~5.000 | 3%¹ | 🟡 |

¹ Não-Touring ou misto — incluído para fechar a base medida de 178.252 (cache_creation turno 1).

**`rules/` top-level individuais** `[FACT — bytes]`:

```
taco-forge-canonical-workflows.md  25.156    touring-decision-matrix.md  17.437
TACO-subagent.md                   17.344    elite-50-quality.md         13.663
VP-Scout.md                        13.590    touring-rebuild.md          11.841
tool-combination-patterns.md       11.076    touring-cli-index.md         8.968
code-execution-gateway.md           7.655    touring-process-hygiene.md   6.122
disk-hygiene.md                     4.714    touring-elite.md             3.548
entity-identity.md                    609    file-metadata-first.md         523
```

> **Ponto cego mais caro**: esta sessão carregou as 51 D-rules + VP-Scout + taco-forge workflows
> (~110K tokens de procedimento de código Rust) numa tarefa de diagnóstico em diretório `node`.
> Zero aplicação. É carregado por *path-blindness*: tudo em `~/.claude/rules/**` entra sempre.

---

## 3. Camada B — Mapa EXAUSTIVO dos hooks (todos os ~35 eventos)

Cada handler foi executado com o `stdin` real do Claude Code e o `additionalContext` medido.

### 3a. Hooks que INJETAM tokens `[FACT — medido]`

| Evento → handler | Frequência | Tokens/disparo | Finalidade | Aproveitamento |
|---|---|---:|---|---|
| `SessionStart` → **session-start** | 1×/sessão | gera **140 KB** → **~500 tok**¹ | status+doctor dump | ⚫ "Connection refused" ×8 repetido |
| `SessionStart` → **session_startup_intelligence.py** | 1×/sessão | **~946** | estado global de inteligência | 🟡 Útil 1×, depois obsoleto |
| `UserPromptSubmit` → **prompt-enhance** | **toda mensagem** | **~444** | CoT + Precision Hints + VGP | ⚫ Genérico, idêntico sempre |
| `PreToolUse` → **cli-suggest** | **9 tipos de tool** | **~329** (Bash) | sugestões + past-failures | ⚫ **57% ruído repetido** |
| `PreToolUse` → **pre-read** | todo Read | **~100** | metadata (blast/quality) | 🟢 Relevante |
| `PreToolUse` → **pre-edit** | todo Edit | **~45** | pre-edit score | 🟢 Relevante |
| `PreToolUse` → **pre-bash** | todo Bash | **~22** | aviso pré-exec | 🟡 |
| `PostToolUse` → **post-edit** | todo Edit | **~34** | quality re-verify | 🟢 |
| `PostToolUse` → **post-write** | todo Write | **~22** | quality re-verify | 🟢 |

¹ O harness do Claude Code salvou os 143.632 bytes em
`tool-results/hook-f5a9a234…txt` e injetou só o preview (~2 KB). **O session-start parece um
vilão de 140 KB, mas o harness protege** — só ~500 tok chegam ao contexto. `[FACT]`

### 3b. Hooks de EFEITO COLATERAL — rodam, NÃO injetam tokens `[FACT — ctx=0]`

`post-bash`, `post-read`, `post-tool-rl` (RL reward), `post-tool-batch`, `instructions-loaded`,
`pre-compact`, `post-compact`, `enter-plan-mode`, `exit-plan-mode`, `subagent-start/stop`,
`file-changed`, `cwd-changed`, `task-created/completed/sync-*`, `session-stop`, `ceg-observe`,
`pre-grep`, `pre-glob`, `pre-write`, `worktree-create/remove`, `config-change`, `notification`,
`elicitation*`, `stop-failure`, `teammate-idle`, `setup`, `permission-request`, `cwd-changed`,
`telemetry_logger.sh`, `check_context.sh`, `rl_warmup.sh`, `session_env_setup.sh`,
`touring-startup.sh` → custam **latência/CPU e I/O em DBs**, zero tokens.

### 3c. Hooks GUARD — só injetam quando BLOQUEIAM `[FACT — ctx=0 no caminho feliz]`

`taco-forge-guard.sh`, `block_git.sh`, `touring-quality-block-all.sh`, `pre-edit-prevention`,
`doc-coverage-gate.sh`, `guard_settings.sh`.

### 3d. Custo de latência (não-token, mas real)

Um **único Bash** dispara **9 processos** (`pre-bash + block_git + taco-forge-guard + cli-suggest +
ceg-observe` no pre; `post-bash + post-tool-rl + check_context + telemetry_logger` no post). Cada um
é spawn de binário Rust (~50ms). Não consome tokens, mas atrasa toda ação.

### 3e. Anatomia do desperdício no `cli-suggest` `[FACT]`

Injeção de Bash dissecada (1.317 chars):

```
567 chars (43%) → sugestão MUST/SHOULD touring …    (genérica)
750 chars (57%) → "lições de erros passados"         ← RUÍDO REPETIDO IDÊNTICO
```

As 5 *past-failures* (`8ddc1d2b`, `1c29cc4b`, `06b15dc9`, `7dfd7d44`, `df45358a`) são **idênticas em
todo Bash da sessão**, fragmentos truncados de erros de **2026-05-11** sem relação com o comando
atual.

---

## 4. Camada C — Schemas MCP (condicional)

Handshake JSON-RPC real com `touring serve` (`tools/list`) `[FACT]`:

- **158 tools expostas** | tamanho total do `tools/list` = **151.146 bytes ≈ 37.786 tokens**
- Maiores schemas: `touring_decompose` (2.880 B), `touring_graph` (2.720 B),
  `touring_file_ops` (2.660 B), `touring_evolve` (2.536 B), `touring_suggest` (2.354 B)
- Média: 956 B/tool

**Status nesta sessão: INATIVO** — `mcp__touring__*` não aparece nos tools do transcript
(cwd `/home/gabrielgadea`, daemon retorna `Permission denied (os error 13)`). Custo nesta
sessão = **0 tok**.

**Quando ativo** (cwd em `~/.claude/rust`): **+37.786 tokens** na base. A memória registra
curadoria "102→22 MCP tools", mas o server real **expõe 158** → curadoria não aplicada.

---

## 5. Camada D — Saídas verbosas (o vilão sob demanda) `[FACT]`

Tamanho real do tool-result de comandos comuns (medido por execução):

| Comando | Bytes | Tokens (~) | Veredito |
|---|---:|---:|---|
| `touring wiring orphans -j` | **693.006** | **173.251** | ⚫⚫ Catastrófico |
| `touring status -j` | **166.814** | **41.703** | ⚫ Brutal |
| `touring gate-metrics -j` | 4.357 | 1.089 | 🟢 |
| `touring e2e -j` | 3.704 | 926 | 🟢 |
| `touring doctor -j` | 802 | 200 | 🟢 |

**O agravante**: o `cli-suggest` injeta "MUST touring status -j" / "MUST touring doctor -j" /
"SHOULD touring e2e -j" em cada Bash. Se a LLM obedece, despeja 42K (status) a 173K (orphans)
tokens por comando.

**Mitigação do harness** `[INFER ~0.85]`: tool-results gigantes são truncados (limite do Bash tool,
~30K chars ≈ 7,5K tok) ou persistidos em arquivo (como os 140 KB do session-start → 2 KB). Logo o
**dano real por comando é limitado a ~7-15K tokens**, mas o JSON truncado fica **inválido/inútil** —
pior dos mundos: gasta tokens e não entrega dado parseável.

---

## 6. Camada E — Subagentes (re-injeção) `[FACT + INFER]`

- **5 agents `touring-*`** (scouter/architect/engineer/auditor/scriber): system-prompts somam
  **172.687 bytes ≈ 43.172 tokens** `[FACT]`. Ao spawnar um agent, seu system-prompt entra no
  contexto **daquele** subcontexto.
- **Re-injeção de rules** `[INFER ~0.8]`: as rules são "user's private global instructions **for
  all projects**" → aplicam a subagentes também. Cada subagente paga novamente os ~110K de markdown
  Touring. Spawnar os 5 agents ≈ **5 × (110K rules + ~8K system-prompt próprio)** antes de cache.
- As **descrições** dos agents (parágrafos Wave 4/Wave 12) já custam ~2.172 tok na **lista** do
  contexto principal, sempre.

---

## 7. O prompt-cache da Anthropic está funcionando? `[FACT — usage do transcript]`

| Métrica (8 turnos únicos) | Tokens | Custo rel. |
|---|---:|---|
| `cache_read` (relido barato) | **4.678.447** | 0,1× |
| `cache_creation` (escrito) | 658.650 | 1,25× |
| `input` (não-cacheado) | 86.167 | 1,0× |
| **% cache-read** | **86%** | ✅ saudável |

- ✅ O cache absorve o prefixo estável (rules/skills/tools). **Está funcionando.**
- ⚠️ **Não resolve o fundo**: a base de ~178K é tão gorda que cada turno relê **~18.000
  tokens-equivalentes** só dela (178K × 0,1). Em 8 turnos já foram ~142K tok-equiv relendo regras —
  62% Touring, 30% D-rules irrelevantes à tarefa.
- ⚠️ **TTL = 5 min**: toda pausa > 5 min força `cache_creation` da base inteira de novo
  (178K × 1,25 ≈ **223K tokens caros**). É o pico de custo — cada "volta do café" reescreve a
  constituição.

> **sccache × prompt-cache**: análogos conceituais (ambos evitam recomputar), mas **domínios
> separados**. sccache = artefatos `.o`/`.rlib` em disco. prompt-cache = prefixo de tokens no
> servidor da Anthropic. Um **não** influencia o outro.

---

## 8. sccache — resposta direta `[FACT — sccache --show-stats]`

**NÃO otimiza nem consome contexto/tokens.** É cache de compilação Rust/C (REGRA #12). Acelera
`cargo build`. Está mal:

| Métrica | Valor | Veredito |
|---|---:|---|
| Hit-rate global | 8,38% | ⚫ |
| Hit-rate **Rust** | **2,72%** | ⚫ inútil |
| Rust hits / misses | 247 / 8.840 | |
| C/C++ | 47,08% | 🟡 |
| Assembler | 90,54% | 🟢 |

`[INFER ~0.75]` Causa provável (alinhada à REGRA #12): `incremental=true` brigando com sccache
(a regra manda `incremental=false` em dev), `rustflags`/`-Cmetadata` variando entre builds, ou
`SCCACHE_CACHE_SIZE` estourado. **Não muda em nada o consumo de tokens** — frente separada
(tempo de build).

---

## 9. Orçamento consolidado de tokens

### Cenário 1 — Sessão atual (cwd `/home/gabrielgadea`, MCP inativo) `[FACT]`
```
Base estática total .................. ~178.000 tok  (cache_creation turno 1, medido)
  ├─ markdown Touring ................ 110.624     (62%)
  │   ├─ quality/ 51 D-rules ......... 54.332
  │   ├─ rules/ top-level ............ 41.636
  │   ├─ CLAUDE.md ................... 8.703
  │   ├─ MEMORY.md ................... 5.953
  │   └─ agents desc ................. ~2.172
  ├─ skills catalog .................. ~44.000     (25%)
  ├─ tool schemas nativos ............ ~18.000     (10%)
  └─ MCP instr + session inject ...... ~5.000      (3%)
+ dinâmico/sessão (hooks) ............ ~50.000     (cli-suggest domina; ~21K é ruído)
```

### Cenário 2 — Sessão de trabalho em `~/.claude/rust` (MCP ativo) `[FACT+INFER]`
```
Base estática ........................ ~178.000
+ MCP schemas (158 tools) ............ +37.786      → base ~216K
+ touring status -j (se rodado) ...... +41.703  (trunc. ~7-15K)
+ touring wiring orphans -j .......... +173.251 (trunc. ~7-30K)
+ dinâmico hooks ..................... +50.000
+ por subagente touring spawnado ..... +43K system-prompt + re-rules
PICO REALISTA ........................ 300.000–450.000 tok  (>50% Touring)
```

---

## 10. Aproveitado vs. desperdiçado

| Item | Tokens | Aproveitado nesta tarefa? |
|---|---:|---|
| CLAUDE.md | 8,7K | ✅ define comportamento |
| `rules/` operacionais | 41,6K | 🟡 parcial |
| **`rules/quality/` 51 D-rules** | **54,3K** | ❌ **zero aplicação** |
| Skills plugins (449) | ~44K | ❌ maioria nunca tocada |
| `cli-suggest` past-failures | ~21K/sessão | ❌ ruído repetido |
| `prompt-enhance` | ~5K/sessão | 🟡 útil 1× |
| MCP 158 schemas (quando ativo) | 38K | 🟡 mas 158 vs 22 alvo |
| `status -j` / `wiring orphans -j` | 42K/173K | ❌ verbosidade tóxica |

**Desperdício estrutural: ~120-130K tokens da base** + **~26K/sessão** dinâmico, sem discriminação
de contexto.

---

## 11. Recomendações priorizadas (ROI quantificado)

### 🔴 P0 — máximo impacto, baixo esforço, reversível
1. **Tirar as 51 D-rules do auto-load** → mover `~/.claude/rules/quality/` para
   `~/.claude/skills/elite-quality/references/`, manter só a keystone `elite-50-quality.md`.
   **Ganho: −54.332 tok da base (−24%), toda sessão.**
2. **Enxugar `cli-suggest`**: (a) `TOURING_SUGGESTER_DISABLED=1` no `session_env_setup.sh`
   → **−~37K tok/sessão**; ou (b) remover o bloco `past-failures` + reduzir matchers de 9→3
   (Bash/Edit/Write) → **−~21K tok/sessão** mantendo as sugestões úteis.
3. **Capar saídas verbosas**: nunca rodar `touring status -j` / `wiring orphans -j` crus; usar
   `| jq` com projeção, `--limit`, ou campos específicos. Reescrever o `cli-suggest` para **não
   sugerir** `status -j`/`orphans -j` sem filtro. **Evita picos de 42K–173K tok.**

### 🟠 P1 — médio impacto
4. **Aplicar de fato a curadoria MCP 158→22** (a memória diz 102→22; o real é 158).
   **−~33K tok** em sessões com MCP ativo.
5. **Mover detalhe das rules gordas** (`taco-forge-canonical-workflows` 25KB,
   `touring-decision-matrix` 17KB, `TACO-subagent` 17KB) para references on-demand. −15-20K tok.
6. **`prompt-enhance` só no 1º prompt** da sessão. −4-5K tok/sessão.
7. **Encolher as descrições dos 5 agents** (parágrafos Wave 4/Wave 12 → 1 linha). −1,5K tok.

### 🟡 P2 — limpeza / frente separada
8. **Desinstalar plugins de skill não usados** (recipe-*, gws-* se inativos). 516 skills → reduzir
   ~44K tok da base. (Não é Touring, mas é a 2ª maior peça.)
9. **sccache (build, não tokens)**: investigar o 2,72% Rust — `incremental=false` +
   `SCCACHE_CACHE_SIZE` + estabilidade de `rustflags`. Impacto: tempo de build, **0 tok**.

> **Se aplicar P0 (1+2a+3): base cai de ~178K → ~124K (−30%), dinâmico cai ~37K/sessão, e os picos
> de 42K-173K por comando desaparecem.** Melhor ROI disponível, sem tocar em código Rust.

---

## 12. Apêndice — comandos de medição (reproduzível)

```bash
# Base estática (tiktoken)
python3 -c "import tiktoken,glob,os;e=tiktoken.get_encoding('cl100k_base');print(sum(len(e.encode(open(p).read())) for p in glob.glob(os.path.expanduser('~/.claude/rules/quality/*.md'))))"

# Usage real (cache) do transcript
python3 - <<'PY'
import json;T='<transcript>.jsonl'
for r in (json.loads(l) for l in open(T)):
  if r.get('type')=='assistant':
    u=r['message']['usage'];print(u.get('input_tokens'),u.get('cache_creation_input_tokens'),u.get('cache_read_input_tokens'),u.get('output_tokens'))
PY

# Schemas MCP (handshake JSON-RPC tools/list)  → 158 tools / 37.786 tok
# Output sizes
for c in "status -j" "wiring orphans -j" "doctor -j"; do o=$(touring $c 2>/dev/null); echo "$c: ${#o}B"; done

# sccache
sccache --show-stats
```

---

_Diagnóstico produzido sem invocar a skill Touring nem o `sequential-thinking` MCP — ambos
inflariam o exato contexto sob diagnóstico. Todo número marcado `[FACT]` é medido; `[INFER]` é
derivado com confiança indicada._
