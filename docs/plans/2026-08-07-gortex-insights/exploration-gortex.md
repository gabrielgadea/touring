---
type: Exploration
title: "Gortex → Touring — exploração exaustiva e extração de insights"
description: "Exploração multi-rodada do zzet/gortex (engine de code-intelligence para agentes, Go, 1.1k★) confrontada com o estado verificado do Touring. 13 insights ranqueados por alavanca, 6 anti-adoções, 1 tese central."
plan_id: 2026-08-07-gortex-insights
tags: [exploration, gortex, benchmarking-externo, wiring, retrieval-eval, token-economy, afordancia]
timestamp: 2026-08-07T18:05:00-03:00
okf_version: "0.1"
---

# Gortex → Touring — exploração exaustiva e extração de insights

**Alvo**: [`zzet/gortex`](https://github.com/zzet/gortex) — Andrey Kumanyaev, Apache-2.0,
Go 1.26+, 1.1k★/101 forks. "High-performance code-intelligence engine for AI agents and
IDE": indexa código num grafo em memória, expõe por CLI + MCP + HTTP + daemon, 257
linguagens, multi-repo por default, 87 pacotes internos.

**Por que este alvo importa**: Gortex não é uma biblioteca adjacente — é o **mesmo animal
que o Touring**. Grafo de código, daemon de longa vida, superfície MCP, hooks de agente,
economia de tokens como tese. Dois sistemas resolvendo o mesmo problema com decisões
diferentes é a condição ideal para extrair sinal: cada divergência é um experimento que
alguém já pagou para rodar.

## VERDICT

O Gortex tem **resposta medida para exatamente as cinco fraquezas que eu medi no Touring
nesta sessão e na anterior** — e três delas são baratas porque a infraestrutura no Touring
já existe e está desligada.

A mais aguda: passei esta sessão inteira caçando órfãos falsos (5031 → 4246, quatro
defeitos de resolução). O Gortex **não tem esse problema de diagnóstico** — não porque
resolva melhor, mas porque **modela a falha de resolução como dado de primeira classe**:
`origin`/`tier`/`confidence` por aresta, nós placeholder `unresolved::`, e a contagem de
call sites não-ligados reportada **separadamente**. No Touring, quando o resolvedor falha,
a aresta simplesmente não nasce — e o produtor vira indistinguível de um órfão real.

E a coluna para isso **já existe no schema do Touring**: `wiring_map.contract_source`,
constante `ast_read` em **77.679 de 77.679 linhas**. É o mesmo defeito que a auditoria de
hoje batizou de F4 — *o dado certo existe e o call site o descarta* — repetido num campo
diferente.

## SCORECARD — Gortex vs Touring, por eixo

| Eixo | Gortex | Touring (verificado) | Veredito |
|---|---|---|---|
| Proveniência de aresta | `origin` (5 tiers) + `tier` + `confidence`; `unresolved::` | `contract_source` constante `ast_read` (77.679/77.679) | **Gortex** ⛔ |
| Baseline de regressão | `eval parity`: corpus congelado + `baseline.json` + `--epsilon` + `--update` | `.baseline/orphans-scoped.txt`; regravar = `rm` | **Gortex** ⛔ |
| Eval de retrieval | 156 casos curados, 3 tiers, R@1/5/20 + MRR, 3 rankers | 3 casos hardcoded, passa com 2/3 | **Gortex** ⛔ |
| Detecção de clones | MinHash 64-slot token-normalizado + LSH, aresta no índice | Type-1 exato, blocos de 6+ linhas (jscpd) | **Gortex** ⚠ |
| Contagem de tokens | tiktoken `cl100k_base`; chars/4 só como degradação | `bytes_saved / 4` permanente | **Gortex** ⚠ |
| Formato de wire | GCX1 (−27,4% mediano, round-trip 20/20, spec + 2 impls) | TOON, só `format_toon(&[Symbol])` | **Gortex** ⚠ |
| Hook de indução | `deny`: PreToolUse **nega e redireciona** Read/Grep/Bash | `cli-suggest`: injeta `additionalContext` (persuasão) | **Gortex** ⚠ |
| Higiene de descrição MCP | 95.060 bytes core; 66% eram prefixo repetido | 167 tools, 161 B/tool, **0,1%** de prefixo repetido | **Touring** ✅ |
| Aprendizado por reforço | `store_memory` (armazena) | `memory` + `learning reward` + LinUCB (**reforça**) | **Touring** ✅ |
| Harness de qualidade | `health_score` (4 sinais → A..F) | 50 dims, 6 P0 BLOCK, tiers de enforcement | **Touring** ✅ |
| Workflow durável | `workflow` (máquina de 3 fases) | ADW: journal fsync'd, `--resume-run`, Class-D, ZTE, racing | **Touring** ✅ |
| Contrato de convergência | — | `loop_converged.py`, 6 cláusulas, exit code é o juiz | **Touring** ✅ |

---

# TIER S — ataca fraqueza medida, infraestrutura já existe

## S1 — Proveniência por aresta: `unresolved` como dado, não como silêncio ⛔

**Fraqueza medida (esta sessão)**: `orphans_base` é a única cláusula de convergência
aberta, e ela não fecha porque **não sei separar órfão real de falha de resolução**.
Corrigi quatro defeitos de resolução e a contagem caiu 785 — provando que a maior parte
dos "órfãos" era cegueira do modelo, não código morto. Não tenho como saber quanto ainda é.

**O que o Gortex faz** (`docs/architecture.md`, `docs/features.md`):

1. **`origin` por aresta**, cinco tiers ordenados por força de evidência:
   `lsp_resolved` → `lsp_dispatch` → `ast_resolved` → `ast_inferred` → `text_matched`;
   mais um rótulo grosso `tier` ∈ {`lsp`, `ast`, `heuristic`} e um `confidence`.
2. **Nós placeholder `unresolved::`** — a chamada que não resolveu **não desaparece**; ela
   vira uma aresta para um nó explicitamente marcado como não-resolvido.
3. **`name_only_candidates`** — a contagem de call sites não ligados é reportada como
   número separado. Nas palavras da doc: *"honest handling"*.
4. **Atenuação por proveniência no ranking** — arestas de tier fraco pesam menos na
   centralidade, então a heurística não contamina o score.

**Por que isso é a maior alavanca do documento**: `wiring_map` **já tem a coluna**.

```
sqlite> SELECT contract_source, COUNT(*) FROM wiring_map GROUP BY contract_source;
ast_read|77679
```

Uma coluna de proveniência com um único valor é uma coluna que não existe. O call site que
grava `register_pub_symbol` é o **mesmo** que hoje corrigi para parar de gravar `"public"`
literal em vez de `sym.visibility` (achado F4 da auditoria). É literalmente o mesmo defeito
duas vezes: o dado certo existe, o call site o descarta.

**Efeito esperado**: `orphans_base` deixa de ser um número e passa a ser uma partição —
`{órfão_real, não_resolvido_ast, não_resolvido_heurístico}`. Só a primeira classe é dívida
REGRA #0; as outras duas são dívida do **resolvedor**, e ficam visíveis como tal em vez de
inflarem a métrica de código morto.

**Custo**: baixo. Coluna existe; enum de tier é novo; o call site é um; as 5 queries de
órfão ganham um `AND contract_source = 'resolved'`.

## S2 — Protocolo de baseline com tolerância e regravação sancionada ⛔

**Fraqueza medida**: recusei duas vezes regravar `.baseline/orphans-scoped.txt`, porque a
única forma de fazê-lo é `rm` — e apagar não é protocolo, é apagar. O resultado é uma
cláusula travada há duas sessões medindo *staleness*, não REGRA #0.

**O que o Gortex faz** (`gortex eval parity`): corpus de benchmark congelado por
linguagem, `baseline.json` com **piso de cobertura por linguagem**, contagem de linguagens
congelada, goldens de extração por feature — uma *"cerca de regressão de três vias"* — com
**`--epsilon` de tolerância**, `--lang` para escopar e **`--update` como caminho explícito
e sancionado de regravação**.

**A diferença que importa**: `--update` não é "ajustar o número para passar". É um comando
nomeado, com tolerância declarada, que deixa a regravação registrada como **decisão**. A
tentação que eu identifiquei corretamente ("mexer no juiz para passar é pior que ajustar o
número") existe porque o Touring só oferece o `rm`. Um protocolo com epsilon e `--update`
elimina a escolha entre dois males.

**Aplicado ao caso concreto**: a baseline de 5109 foi medida sob um modelo que contava
`pub(crate)` como API pública e perdia arestas por quatro defeitos. Sob `eval parity`, isso
seria um `--update` com nota de mudança-de-modelo — não um dilema moral.

**Custo**: baixo. É formato de arquivo + duas flags no `loop_converged.py`.

## S3 — Eval de retrieval com ground truth ⛔

**Fraqueza medida — e esta é minha própria falha**, encontrada ao verificar:

```rust
// crates/touring-server/src/cli/eval.rs:290
fn run_search_benchmark() -> BenchmarkResult {
    // Test RRF hybrid search with known queries
    let test_cases = vec![
        ("SessionHintEngine", true),
        ("apply_detail_level", true),
        ("nonexistent_xyz_42", false),
    ];
    ...
    status: if accuracy >= 0.5 { "pass" } else { "fail" }
```

Três defeitos numa função de 40 linhas:
1. O comentário diz *"Test RRF hybrid search"*, mas o corpo chama `cli-index-find` — um
   **lookup exato por chave**. A busca híbrida não é exercitada em nenhum ponto.
2. Dois dos três casos são tautológicos: perguntar a um índice se ele contém um símbolo que
   ele indexou.
3. O gate passa com `accuracy >= 0.50` sobre 3 casos — ou seja, **erra 1 de 3 e passa**.

Isto é exatamente o "gate frouxo que aprova para passar" que o Gabriel me mandou não
aceitar, dentro do meu próprio código, alimentando um `overall_score` que eu reporto.

**O que o Gortex faz** (`BENCHMARK.md` §5): `bench/fixtures/retrieval.yaml`, **156 casos
curados à mão**, três tiers de dificuldade (`exact` / `concept` / `multi_hop`), três
rankers comparados lado a lado (bm25 / winnow / ripgrep), métricas R@1 / R@5 / R@20 / MRR.

| Ranker | R@1 | R@5 | R@20 | MRR |
|---|---|---|---|---|
| bm25 | 42,3% | 55,1% | 63,5% | 0,479 |
| winnow | 37,8% | 50,0% | 64,1% | 0,439 |
| ripgrep | 0,0% | 17,3% | 29,5% | 0,061 |

E por tier (bm25): exact **96,8%** · concept **25,4%** · multi_hop **30,0%**.

**O detalhe que mais importa não é a metodologia — é que eles publicam o 25,4%.** Um número
ruim publicado é o sinal que faz melhorar. Um `overall_score` alto produzido por um gate
tautológico **remove** esse sinal. É a versão retrieval do que a auditoria de hoje concluiu:
*quando uma correção correta não move o número, suspeite do medidor.*

**Custo**: médio. O caro é curar as fixtures (156 casos com arquivo esperado). Mitigação:
começar com 30 casos vindos das minhas próprias sessões — o `memory recall` já tem o
histórico de o que eu procurei e onde estava.

---

# TIER A — ganho grande, custo médio

## A1 — Clones por MinHash+LSH, materializados no índice ⚠

**Fraqueza medida**: F1.3 está em **Warn (0,5902)** há três fases. Dedupliquei 372 linhas e
o número não moveu **um dígito** (0,3502 antes e depois, no caso do touring-dispatch).
Diagnostiquei corretamente que o corpus exclui código de teste — mas há uma causa mais
funda que só ficou visível ao ler o Gortex.

**O que o Touring mede** (`f1_3_duplication.rs`, verificado): Type-1 — *"runs of 6+
consecutive meaningful production lines recurring verbatim"*, estilo jscpd/SonarQube-CPD.
**Cópia literal**, módulo espaço em branco.

**O que o Gortex mede**: assinatura **MinHash de 64 slots token-normalizada** por corpo de
função substancial, computada **no momento da indexação**; descoberta de pares candidatos
por **LSH banding** com filtro de Jaccard; materializada como aresta `similar_to` com o
score no metadata. Mais uma aresta `semantically_related` que difunde os scores de clone
transitivamente pelo grafo.

**A diferença é de classe de clone, não de eficiência**: Type-1 só enxerga cópia literal.
Token-normalizado enxerga **Type-2** — o mesmo bloco com variáveis renomeadas, que é a
forma dominante de duplicação real em Rust (mesmo `match`, mesmo `?`-chain, nomes
diferentes). Isso explica de forma econômica por que a dedup não moveu o número: eu removi
clones que já contavam, e os que sobram são majoritariamente Type-2, invisíveis ao detector.

**Bônus de altíssimo valor**: `find_clones dead_only: true` — cruzar clone com código morto.
Duplicação **dentro de código sem consumidor** tem prioridade máxima e custo de remoção
zero. O Touring tem os dois lados do cruzamento (wiring orphans + F1.3) e nunca os cruzou.

**Nota de honestidade**: o Touring **já tem** infraestrutura de similaridade —
`touring-simd/src/similarity/`, `touring-cortex/src/similarity.rs`. Não é construir do zero;
é ligar ao pipeline de indexação e ao F1.3.

## A2 — Contabilidade de tokens de verdade ⚠

**Estado do Touring** (verificado): existe uma tool `touring_ctx_gain` descrita como
*"Token-savings dashboard"*, e o cálculo é:

```rust
// crates/touring-cli/src/cli/handlers/mcp.rs:419
let tokens_saved = bytes_saved_estimated / 4;
```

**Estado do Gortex**: tiktoken `cl100k_base` — o tokenizador real do Claude e do GPT-4 —
com *"chars/4 heuristic"* apenas como **degradação quando a inicialização falha**.
Persistência em `~/.gortex/sidecar.sqlite` com agregados cross-session por **modelo, repo e
linguagem**, mais `cost_avoided_usd` por provedor, e um dashboard de três buckets
(Hoje / 7 dias / Total).

**O ponto**: o Touring vive **permanentemente no modo de degradação do concorrente**. E o
`bytes/4` não é neutro — ele erra sistematicamente em código (identificadores longos,
símbolos, indentação tokenizam muito diferente de prosa). A economia de tokens é a **tese
fundadora** do Touring (o princípio STR em `tool-combination-patterns.md`, o pilar Code
Mode). Medir a tese fundadora com um divisor por 4 é o mesmo modo de falha do S3.

## A3 — GCX1: formato de wire compacto, aberto e medido ⚠

**Estado do Touring** (verificado): TOON existe, mas em uma única função —
`format_toon(&[Symbol])`. Não é um formato de wire; é um serializador de listas de símbolos.

**O que o Gortex fez**: GCX1 — formato de texto tab-delimitado, orientado a linha,
opt-in por chamada via `format: "gcx"`. **−27,4% de tokens medianos vs JSON** (tokenizador
cl100k_base), melhor caso −38,3%, **round-trip 20/20 fixtures**. Design:

- Header declara os campos **uma vez**: `GCX1 tool=search_symbols fields=id,kind,name,path,line,sig rows=3 total=7 truncated=false`
- Linhas separadas por TAB; alfabeto de escape **mínimo** (`\\`, `\t`, `\n`)
- **Consciência de tokenizador**: TAB e LF contam como whitespace no cl100k_base — a escolha
  do delimitador foi feita contra o tokenizador, não contra o byte
- Encoders afinados à mão para tools de alto tráfego; **fallback genérico** garante validade
- Versionamento no header com **fallback transparente para JSON** em versão desconhecida
- Implementações de referência MIT em **Go e TypeScript** com paridade byte-a-byte em goldens

**Por que adotar em vez de inventar**: a spec é publicada, tem duas impls MIT, e o número
(−27,4%) foi medido com round-trip verificado. Escrever um formato próprio custaria mais e
começaria sem corpus de fixtures. E o TOON, que o Touring já tem, é explicitamente o
**segundo tier** do próprio Gortex (~10-15%, e *lossy*).

**Composição com A2**: sem a contagem real de tokens (A2), não há como provar o ganho de A3
neste workspace. A2 é pré-requisito de A3.

## A4 — Hook `deny`: a validação externa da tese afordância > persuasão ⚠

Esta é a mais interessante conceitualmente, porque o Gortex **provou a minha própria tese
contra mim**.

`~/.claude/rules/touring-4-pillars.md` registra, com evidência, uma lição minha:
> *"adoption does not emerge from availability; it must be actively induced"* — e a tese ①:
> *"affordance changes `U(a)=P·V−C(tokens)`; persuasion does not"*, com a observação de que
> nudges MUST de confiança 0,95 foram ignorados **na própria sessão que os emitiu**.

O que o Touring construiu a partir disso: `cli-suggest`, um hook nativo em Rust (~1ms,
in-daemon) que injeta `additionalContext` com MUST/SHOULD. **Isso é persuasão** — texto
mais bem escrito, injetado mais perto, mas ainda texto que o modelo pode ignorar. (Duas
vezes nesta própria sessão o hook me sugeriu `touring run` para um loop Python e eu segui
com Bash — o dado está no transcript.)

O que o Gortex faz, e é o **modo default**:

| Modo | Comportamento |
|---|---|
| **`deny`** (default) | PreToolUse enriquece Read/Grep/Glob/Bash com contexto do grafo **e redireciona por negação** para as tools do grafo |
| `enrich` | nunca nega; rebaixa para `additionalContext` mole; PostToolUse aumenta a saída |
| `GORTEX_HOOK_BLOCK_EDIT` | escala o bloqueio para edição via shell (`sed -i`, `>`, `>>`, `tee`, scripts inline) |

O Touring **tem toda a infraestrutura** para o modo `deny` — o hook PreToolUse existe, é
nativo, roda em 1ms e já classifica a operação. A distância entre "injetar MUST" e "negar e
redirecionar" é **pequena em código e enorme em efeito**, e é exatamente o passo que a
minha própria rule diz que é o único que funciona.

**Ressalva de projeto**: o Touring é o sistema nervoso de UM operador. Negar por default é
uma decisão do Gabriel, não minha — modo `enrich` como default e `deny` como opt-in armado
é o desenho compatível com a governança atual (mesmo padrão de `TOURING_PILLAR_INDUCTION_ARMED`).

---

# TIER B — estratégico, custo alto ou fora do foco imediato

## B1 — `audit_agent_config`: a REGRA #16 executada por código

O Gortex tem uma tool que **varre** `CLAUDE.md`, `AGENTS.md`, `.cursor/rules`,
`.github/copilot-instructions.md`, `.windsurf/rules` procurando **referências obsoletas,
paths mortos e bloat**.

A REGRA #16 (CLAUDE.MD HYGIENE) do TACO define limites operacionais — 400 linhas hard, 300
soft, seções ≤ 50, tabelas ≤ 20 — e o mecanismo de enforcement é: *"TACO **RECUSA**
adicionar quando levaria a > 400L"*. Ou seja, **eu prometendo me policiar**. É a mesma
persuasão do A4, aplicada à constituição que define as regras.

Um linter que lê `~/.claude/CLAUDE.md` + `rules/*.md`, conta linhas, e — mais importante —
**verifica se cada path citado ainda existe** seria a versão-afordância. Alto valor: as
rules citam dezenas de paths de scripts, skills e crates, e eu não tenho como saber quais
apodreceram sem checar um a um.

## B2 — Facades por seletor vs allowlist que esconde

Convergência quase exata e independente: **Gortex 178 tools → 21 facades; Touring 173 →
~22 curadas**. Dois sistemas bateram no mesmo teto e curaram para o mesmo número.

**Mas a lição principal do Gortex não se aplica ao Touring, e registro isso como delta
negativo**: o problema deles era bloat de descrição — 95.060 bytes no preset core, dos
quais *"aproximadamente 66% eram prefixo comum repetido"*. Medi o Touring:

```
167 tools com description · 26.951 bytes · média 161 B/tool
prefixos de 40 chars repetidos: 40 bytes (0,1% do total)
```

**As descrições do Touring são enxutas e não têm o problema do Gortex.** Essa parte do
insight é inaplicável.

O que **sim** se aplica é a diferença de design: o Touring **esconde** — `apply_curation`
filtra o `list_tools`, mas o comentário no código é explícito: *"call_tool is unfiltered, so
hidden tools stay callable"*. Isso cria 151 tools que **existem, funcionam, e são
indescobríveis**. O Gortex **agrupa**: um `read` que infere arquivo-vs-símbolo pelo seletor
do `target`, um `analyze` com discriminador de operação. Agrupar preserva a descoberta;
esconder a destrói. Um agente novo no Touring não tem como saber que `touring_ctx_tee_retrieve`
existe.

## B3 — Arestas de contrato: as classes de órfão estrutural que listei

A auditoria de hoje enumerou seis classes de órfão que um modelo baseado em `use` não
consegue ver. O Gortex resolve várias delas com **arestas de framework/contrato**:

| Classe de órfão (minha auditoria) | Aresta do Gortex |
|---|---|
| Chamada por caminho qualificado (`crate::verifications::foo()`) | `calls` com `origin=ast_resolved` (não depende de `use`) |
| Alias de tipo | `aliases` + `typed_as` (implementei um caso hoje) |
| Re-export de fachada | **`re_exports` distinto de `imports`** — *"a dependency walk separates forwarding hops from consumption"* |
| Tipo de erro em posição de retorno | `returns`, `throws` |
| Consumo de configuração | `reads_config`, `uses_env` |
| Tipo derivado por macro | `generated_by`, `has_generated_members` |

**Convergência notável**: eu implementei `resolve_reexport` hoje, e o Gortex tem
`re_exports` como **tipo de aresta distinto** com justificativa explícita. Chegamos ao mesmo
lugar; eles chegaram com o modelo de dados certo (a distinção é do grafo, não do resolvedor).

Para o Touring, `uses_env` e `generated_by` são os de maior retorno: cobrem o
`std::env::var` e os símbolos derivados por macro — este último já registrado na memória
como gotcha (`cargo_dead_code_macro_indirection_2026_05_14`).

## B4 — Snapshot por (repo, commit) e as três rotas de reconciliação

**Gortex**: grafo serializado em gob+gzip, chaveado por **caminho do repo + hash do commit
git**, com validação de versão que invalida em upgrade de binário. Cold start de repo médio
em **~200ms** (era 3-5s). Três rotas explícitas:

| Rota | Quando |
|---|---|
| `incremental` | sem mudança em disco — pula parse, resolução e enriquecimento inteiros |
| `scoped` | só arquivos mudados são reparseados; resolução cross-file roda no delta |
| `full_retrack` | evict-and-reparse — forçado, ou disparado quando o churn passa de ~40% |

Mais: enriquecimento semântico chaveado por `(repo, provider, commit)`; snapshots por
`(repo, branch)` reaproveitáveis em troca de branch; worktrees compartilham a base;
detecção por mtime com modo **BLAKE3 Merkle** opt-in para diff por conteúdo.

**Contraste medido**: o `touring index rebuild` desta sessão levou **2m19** para 3.120
arquivos. O `full_retrack` é o único modo que o Touring tem para essa operação.

**Nota**: o Touring tem indexação incremental. O que falta é a **taxonomia explícita** com
gatilho quantitativo (o limiar de 40% de churn) e a chave `(repo, commit)` que torna a
invalidação determinística em vez de inferida.

## B5 — Isolamento de crash do tree-sitter

Opt-in `index.crash_isolation`: extração tree-sitter em **subprocessos worker**, com
contenção de SIGSEGV/OOM/hang por arquivo patológico, pool de workers de vida longa,
orçamento de extração por arquivo, teto de tamanho, e detecção por conteúdo de arquivos
minificados/bundled.

O Touring parseia **in-process no daemon**. Um segfault de gramática em um arquivo derruba o
daemon inteiro — que é singleton por usuário e serve todas as sessões CC. Risco baixo em
frequência, alto em blast radius.

## B6 — Economia de contexto: fetch condicional e fidelidade graduada

Três mecanismos que o Touring não tem nenhum equivalente:

1. **`if_none_match`** — hash de conteúdo nas tools que leem fonte. Código não mudado
   retorna `not_modified` com custo **quase zero de tokens**. É HTTP caching aplicado a
   contexto de agente.
2. **`smart_context fidelity: "graded"`** — retorna um `context_manifest` que **estratifica
   por distância no grafo**: símbolos focais em fonte completa, o anel de callers/callees
   como stubs, o resto como outline. Famílias grandes são esqueletizadas a um representante.
   `max_lines` faz truncagem **ciente de AST** (mantém o esqueleto de controle de fluxo,
   colapsa runs de folha).
3. **`estimate: true`** — projeta o custo em tokens da chamada **antes** de buscar.

O (3) é o mais barato e mais alinhado ao princípio STR: permitir que o agente pergunte
"quanto isso vai custar?" antes de gastar. O (1) tem valor especial no Touring, onde uma
sessão longa relê os mesmos arquivos muitas vezes.

## B7 — Metodologia de eval agent-graded, com delta negativo obrigatório

`docs/04-evaluation/` define um protocolo que vale por si:

- **15 tarefas semente** em 5 categorias (explicação arquitetural, segurança de refactor,
  localização de bug, análise de impacto, extração de contrato), vindas de sessões reais
- **3 agentes** (Claude Sonnet 4.6, GPT 5.4, Copilot CLI) × **2 modos** (com/sem as tools)
  = 6 runs por tarefa, 90 no total
- **Teste de ablação**: os runs COM as tools rodam duas vezes — com o prompt de produção e
  com o prompt **ablado** (a orientação "prefira o gortex" removida). Ambos publicados, para
  provar que a medição reflete capacidade da ferramenta e não condução por prompt
- **Classificador a/b/c**: (a) melhor, (b) equivalente, (c) **pior**
- **Exigência de delta negativo**: publicar só os (a) é eval incompleto. **Zero (c) em 15
  tarefas é sinalizado como viés metodológico** e obriga re-teste com outro juiz
- **Orçamento idêntico** nos dois modos (50k tokens, 5 min); estouro conta como
  "sem resposta", não como falha — isola qualidade de resistência

**A cláusula que eu deveria adotar imediatamente**: *zero resultados negativos é sinal de
viés, não de excelência*. Ela é a formalização exata do que o Gabriel me cobrou —
*"os gates existem para garantir qualidade, não para aprovar para passar"*.

---

# O que NÃO copiar

Registrar isto importa tanto quanto os insights: copiar por admiração dilui o Touring.

| Não copiar | Por quê |
|---|---|
| **257 linguagens** | ~165 são *"forest-backed signature-only"* — cobertura rasa contada como número de marketing. O Touring é Rust-cêntrico com `syn` (generics, trait bounds, lifetimes, derives, unsafe/async). A profundidade em UMA linguagem vale mais para o TACO que 165 parsers de assinatura. |
| **19 adaptadores de editor** | Custo de produto, não de capacidade. O Touring serve o Claude Code. |
| **Multi-repo por default** | O Touring acabou de estabilizar a topologia per-project rustup-like (Pln2 L1-L4). Multi-repo agora seria refazer a L4 recém-assentada. |
| **Web UI** | Não serve o operador do TACO. |
| **Overlays com branching (11 tools)** | Serve editor com buffers não salvos. O Claude Code escreve em disco. |
| **178 tools** | O Touring já tem 173 e o problema é o mesmo. Copiar mais superfície é copiar a doença. |

# Onde o Touring é estruturalmente superior

Não é simetria de cortesia — são quatro capacidades que o Gortex **não tem equivalente**:

1. **Learning Memory como feromônio ACO.** O Gortex tem `store_memory`/`query_memories` com
   `kind`/`importance`/`confidence` — armazenamento estruturado, e bom. Mas **não há loop de
   reforço**: nada mede o outcome e realimenta a política. O Touring tem `learning reward` +
   LinUCB + QTable + EMA. A diferença entre *lembrar* e *aprender*.
2. **50 dimensões de qualidade com 6 P0 BLOCK** e tiers de enforcement. O `health_score` do
   Gortex é 4 sinais (coverage + complexity + recency + churn) → 0..100 → A..F. Útil, mas
   uma ordem de grandeza mais raso, e sem classe de bloqueio.
3. **ADW / factory.** Workflows declarativos duráveis com journal fsync'd, `--resume-run`
   replay seguro a `kill -9`, detecção Class-D narrativa-vs-veredito, ZTE conformal, racing
   de lanes. O `workflow` do Gortex é uma máquina de três estados com gating de tools.
4. **Contrato de convergência.** `loop_converged.py` com 6 cláusulas onde **o exit code é o
   juiz** e a autoavaliação não conta. O Gortex não tem noção de "terminou".

**A síntese honesta**: o Gortex é melhor em **medir o que já sabe fazer**; o Touring é
melhor em **aprender e em governar o próprio processo**. As duas metades são complementares,
e a fraqueza do Touring é justamente aquela em que o Gortex é forte — e ela é a mais barata
de importar, porque medição é infraestrutura, não inteligência.

# A tese central

Três dos quatro achados de maior alavanca (S1, S3, A2) têm a **mesma forma**, e é a mesma
que a auditoria cruzada de hoje encontrou nos meus próprios quatro defeitos:

> **Um valor correto existe e é substituído por uma aproximação — e a aproximação apaga
> justamente o sinal que faria melhorar.**

- **S1**: a proveniência existe (`contract_source`) e é constante → não sei distinguir
  órfão real de falha do resolvedor.
- **S3**: existe uma busca híbrida e o benchmark testa lookup exato → não sei se a busca é boa.
- **A2**: existe um tokenizador real e eu divido bytes por 4 → não sei se a economia de
  tokens é real.
- (E hoje: **F4** — a visibilidade existe e o call site grava `"public"` → a correção
  REGRA #0 ficou invisível ao medidor.)

O padrão é estável o bastante para virar reflexo, e ele já está escrito no relatório de
hoje em forma menor: **quando uma correção correta não move o número, suspeite do medidor
antes de suspeitar da correção.** A versão generalizada, aprendida do Gortex, é mais forte:

> **Toda métrica que aproxima deve declarar a aproximação no ponto de leitura — e toda
> aproximação de conveniência é uma dívida que se paga em diagnósticos errados.**

O Gortex paga essa dívida explicitamente: reporta `name_only_candidates` separado, publica
que o retrieval de conceito é 25,4%, degrada para chars/4 **só quando o tiktoken falha e
diz que degradou**, e trata *zero resultado negativo* como sinal de viés. Nenhuma dessas
decisões é sobre tecnologia. Todas são sobre **recusar-se a deixar o número parecer melhor
do que a evidência sustenta** — que é literalmente a instrução permanente do Gabriel sobre
os gates.

# PROVENANCE

**Rodadas de exploração** (5, com verificação cruzada contra o Touring em cada uma):

1. Página do repo + WebSearch — identidade, escala (1.1k★), stack, topologia de diretórios
2. `README.md` · `internal/` (87 pacotes) · `docs/` (20 arquivos) · `BENCHMARK.md` ·
   post `from-gitnexus-to-gortex` — arquitetura e história de projeto
3. `docs/contracts.md` · `savings.md` · `wire-format.md` · `mcp-facade-v1.md` ·
   `architecture.md` — os cinco eixos de maior transferência
4. `docs/features.md` (catálogo exaustivo) · `skills.md` · `semantic-search.md` ·
   `mcp.md` (catálogo de tools) · `04-evaluation/` — superfície completa
5. `docs/04-evaluation/methodology.md` — protocolo de eval

**Verificações executadas no Touring** (comandos rodados, não inferidos):

```bash
touring doctor -j                                    # 6/6 ok — daemon saudável
sqlite3 .claude/touring/knowledge.db ".schema wiring_map"
sqlite3 … "SELECT contract_source, COUNT(*) … GROUP BY contract_source"   # ast_read|77679
sqlite3 … "SELECT consumer_type, COUNT(*) …"         # rust_import|77558, daemon_hook|121
sed -n '290,335p' crates/touring-server/src/cli/eval.rs   # os 3 casos hardcoded
sed -n '1,80p'  crates/touring-quality/src/verifications/f1_3_duplication.rs  # Type-1
grep -rn "tokens_saved" crates/*/src                 # bytes_saved_estimated / 4
grep -rc "#\[tool(" crates/touring-server/src/{server,tools}/*.rs   # 173
sed -n '669,715p' crates/touring-server/src/server/mod.rs  # apply_curation, ~22
python3 (regex sobre description=) # 167 tools, 26.951 B, 161 B/tool, 0,1% prefixo repetido
touring memory recall "gortex"                       # tema inédito — 0 entradas relevantes
```

**Limitações declaradas**:

- Os números do Gortex (−27,4% GCX1, R@5 55,1%, 300 arquivos/s, 200ms cold start) são
  **auto-relatados na documentação deles**. Não os reproduzi. Confiança: 0,7 — a metodologia
  é publicada e reprodutível, mas não executada por mim.
- **Não li o código-fonte Go do Gortex**, apenas README, 8 documentos de `docs/`, o
  `BENCHMARK.md`, os listings de `internal/` e `docs/`, e um post do autor. Afirmações sobre
  *implementação* interna (ex.: MinHash de 64 slots, LSH banding) vêm da documentação, não
  da leitura de código. Confiança: 0,8.
- O custo do `tools/list` do Touring foi medido **só nas descrições** (26.951 bytes). O
  JSON Schema dos parâmetros, que na prática domina o payload, **não foi medido**. A
  conclusão "as descrições do Touring são enxutas" é sólida; a conclusão "o `tools/list` do
  Touring é barato" **não foi estabelecida** — marcar `UNVERIFIED`.
- Não avaliei `internal/` pacote a pacote. Dos 87, ~30 foram tocados indiretamente via docs.

# ACTIONS

Priorizadas por alavanca ÷ custo. Nenhuma foi executada — esta é uma exploração.

**Imediatas (infraestrutura existe, ataca cláusula travada):**

1. **S1** — gravar proveniência em `wiring_map.contract_source` (enum de tier) + classe
   `unresolved` + reportar `name_only_candidates` separado. Particiona `orphans_base` em
   dívida-de-código vs dívida-de-resolvedor.
2. **S2** — protocolo de baseline com `--epsilon` e `--update` sancionado no
   `loop_converged.py`. Destrava a cláusula sem "ajustar o número".
3. **S3** — substituir `run_search_benchmark` por fixture curada (começar com 30 casos das
   minhas próprias sessões via `memory recall`) + R@k/MRR + baseline ripgrep.
   **Este é um defeito ativo no meu código, não uma melhoria.**

**Curto prazo:**

4. **A2** — tiktoken real no lugar de `bytes/4` (pré-requisito de A3).
5. **A1** — MinHash+LSH token-normalizado no F1.3, reusando `touring-simd/src/similarity/`;
   depois cruzar com órfãos (`dead_only`).
6. **B7** — adotar a cláusula "zero delta negativo = sinal de viés" no harness 50-dim.

**Requer decisão do Gabriel:**

7. **A4** — hook mode `deny` (armado por env, `enrich` como default). É mudança de
   governança da sessão, não de código.
8. **A3** — adotar GCX1 como formato de wire (spec externa, dependência de terceiro).
9. **B1** — linter de `CLAUDE.md`/`rules/` verificando paths mortos — mexe em
   `~/.claude/`, território do operador.

**Registrado, sem ação:** B3 (arestas de contrato), B4 (snapshot por commit), B5 (crash
isolation), B6 (fetch condicional). Alto valor, custo alto, sem urgência medida.

---
---

# RODADA 2 — código-fonte, prática de engenharia e trajetória de evolução

> Complemento de 2026-08-07T19:20-03:00. A rodada 1 declarou três limitações explícitas:
> **não li o código Go**, cobri ~30 dos 87 pacotes internos, e deixei o custo do `tools/list`
> como `UNVERIFIED`. Esta rodada ataca as três — e **reverte duas conclusões da primeira**.

## Correções à rodada 1

### C1 — O `tools/list` do Touring **não** é barato. Medido, não estimado.

A rodada 1 concluiu: *"as descrições do Touring são enxutas (161 B/tool, 0,1% de prefixo
repetido) — a lição de bloat do Gortex não se aplica"*, e marcou o payload total como
`UNVERIFIED`. Fechei a verificação levantando o servidor MCP e capturando o handshake real:

```
tools no tools/list CURADO : 23
bytes do payload           : 33.161
~tokens                    : 8.290

5 tools mais caras:
  2.947 B  touring_decompose
  2.313 B  touring_ast_find
  2.133 B  touring_audit
  1.903 B  touring_memory_store
  1.806 B  touring_ast_edit
```

**O alvo do facade do Gortex é ≤15.000 bytes para 21 tools.** O Touring entrega **33.161
bytes para 23** — 2,2× mais caro com um número praticamente idêntico de tools.

Os dados da rodada 1 estavam certos; a **inferência** estava errada. As descrições somam
~10 KB dos 33 KB; os ~23 KB restantes são **JSON Schema** (146 structs `Args` derivando
`JsonSchema`, 5.776 doc-comments `///` que viram descrições de campo). O remédio documentado
pelo Gortex ataca exatamente isso: *"campos de alta frequência permanecem tipados no nível
da tool; campos raros e específicos de operação migram para um objeto `options` validado,
descoberto via `capabilities`"*. **A lição se aplica — por um mecanismo diferente do que eu
descartei.** `touring_decompose`, sozinha, custa 2.947 bytes em **toda** sessão.

### C2 — Os hook modes são **quatro**, não dois. E o terceiro é o que interessa.

A rodada 1 descreveu `deny` vs `enrich` e enquadrou a escolha como *persuadir ou bloquear*.
`docs/agents.md` documenta quatro posturas (`GORTEX_CODEX_HOOK_MODE`):

| Modo | Comportamento |
|---|---|
| `enrich` (default no Codex) | enriquecimento consultivo, nunca bloqueia |
| `deny` | bloqueia leituras/buscas de fallback detectáveis pelo grafo |
| **`rewrite`** | converte **só** um `cat <fonte indexada>` **inequívoco** no espelho exato `gortex call read` |
| `suppress` | substitui o resultado bruto do PostToolUse por feedback do grafo |

`rewrite` é o meio que eu não tinha visto: **não persuade e não bloqueia — faz o certo pelo
agente**, e só quando a conversão é determinística. Comandos ambíguos permanecem consultivos
*mesmo sob `rewrite`*; compostos recebem só orientação.

Para o Touring isso é estritamente superior aos dois extremos. `deny` exige uma decisão de
governança do Gabriel (bloquear minhas ferramentas nativas); `enrich` é o que já temos e já
medimos falhar. `rewrite` não exige permissão nova, porque não remove capacidade — troca uma
chamada por outra equivalente e melhor. **A recomendação A4 muda: `rewrite` primeiro,
`deny` como opt-in armado.**

---

## O achado central da rodada 2 — a honestidade não é traço, é **prática sistêmica**

A rodada 1 formulou a tese na forma negativa: *uma aproximação substitui um valor correto e
apaga o sinal*. O código-fonte e o histórico de releases dão a forma **positiva**, que é
acionável como invariante:

> **Todo cálculo limitado anuncia o próprio limite — e nunca é somado ao ilimitado sem marca.**

Cinco instâncias independentes, em três camadas distintas do sistema:

| # | Instância | Onde | O que faz |
|---|---|---|---|
| 1 | `name_only_candidates` | resolver | call sites não ligados contados **separado**, jamais somados ao resolvido |
| 2 | `MetaReachTruncated` | `internal/reach` | registros truncados são *"bounded lower bounds, not exhaustive"*; `LookupCached` **falha fechado** em registro ausente em vez de disparar BFS |
| 3 | *"Caveat usage results containing only name-only matches"* | release v0.63.0 | um item de release dedicado a **pôr ressalva** em resultado fraco |
| 4 | *"Health reporting honesty on inert enrichment"* | release v0.61.4 | um fix cujo propósito é **parar de reportar saúde** de enriquecimento que não estava rodando |
| 5 | Degradação do clangd | `docs/lsp.md` | marcada `degraded`, `index_health` recomenda a correção, e `semantic_enrichment_ok` permanece `true` — **degradação intencional distinguida de falha** |

**E o dado decisivo: eu fiz a mesma coisa hoje, sozinho, sem conhecer o Gortex.** O comentário
que escrevi em `f1_3_duplication.rs` ao adicionar o anúncio de truncagem:

> *"A truncagem NÃO pode ser silenciosa. Um score de prefixo apresentado como score do escopo
> faz o gate parecer medir o que não mediu — e, pior, o corte por bytes torna o número quase
> imune a remediação real."*

Dois sistemas, linguagens diferentes, autores diferentes, chegando à mesma regra pela mesma
dor. **Design convergente sob pressão idêntica** é a evidência mais forte que existe de que
um princípio está certo (confiança 0,9).

A diferença é de status: no Gortex é prática sistêmica com cinco instâncias e releases
dedicadas a **restaurá-la quando ela falha**; no Touring é **uma** instância — a minha, de
hoje. Isso argumenta por promovê-la de correção pontual a **cláusula do harness**:

- toda dim que pode truncar **deve** expor um marcador de truncagem legível por máquina
  (hoje o `dir_scan_overflow` só produz texto na evidência);
- nenhum agregador pode compor truncado com completo sem propagar a marca;
- `loop_converged.py` deve **recusar convergência** sobre uma cláusula cuja evidência está
  marcada truncada — hoje ele a aceitaria em silêncio.

---

## TIER S (novos) — fraqueza medida, infraestrutura próxima

### S4 — A memória do Touring é um feromônio que **nunca evapora** ⛔

A rodada 1 registrou "Touring é superior em Learning Memory porque reforça, e o Gortex só
armazena". Verdade quanto ao reforço — e **incompleta**, porque o armazenamento deles tem
três mecanismos que o nosso não tem, e a falta deles corrói justamente o pilar que a
constituição chama de essência.

O que o Gortex tem (`CLAUDE.md` do repo, `docs/features.md`):

1. **Supersessão** — `store_memory id:"<novo>" supersedes:"<antigo>" body:"<correção>"`. A
   memória original **fica para auditoria** mas some do surfacing por default.
2. **Peso e fixação** — `kind` (invariant / constraint / convention / gotcha / decision /
   incident / reference), `importance` 1–5, `confidence` 0–1, pinning.
3. **Surfacing proativo com explicação** — `surface_memories task:"…" symbol_ids:"…"` no
   **início da tarefa**, ranqueado por sobreposição de âncora + de arquivo + hits de keyword
   + importance + pin + recência + confidence, devolvendo `match_reasons:["symbol:…"]` — a
   memória diz **por que** apareceu.
4. **Disciplina de retenção declarada** — "what to save" vs **"skip"**: pular play-by-play do
   trabalho recém-concluído, pular padrões deriváveis do código, pular duplicatas do CLAUDE.md.

O Touring tem `memory store` + `memory recall` (FTS5 + cosine/ANN + RRF). **Nenhum dos
quatro.**

**Evidência direta de dano, colhida nesta própria exploração**: o `memory recall "gortex"` da
rodada 1 devolveu, entre os 15 primeiros, `purpose-test-key-zx9` com o valor
`"PURPOSEMARKERZX9"` e `test:f67:minimal` com `"short value"` — **lixo de teste competindo em
pé de igualdade com lição real**. Sem `importance` não há como despriorizá-los; sem
supersessão não há como aposentar uma lição corrigida; sem política de retenção o repositório
só cresce.

Um feromônio ACO que não evapora **inverte o próprio mecanismo**: trilhas erradas continuam
guiando com o mesmo peso das certas. É uma falha na essência declarada do sistema, não num
periférico.

**Custo**: baixo-médio. Três colunas (`importance`, `pinned`, `superseded_by`) + um filtro no
recall + um comando `memory supersede`. O ranking proativo é o único item caro.

### S5 — Promover o invariante de truncagem a cláusula do harness ⛔

Ver o achado central acima. Custo: baixo. É um campo booleano no `DimScore` + uma cláusula no
`loop_converged.py`. Impacto: fecha a classe inteira de defeito que produziu o F2.6 (uma dim
P0 BLOCK que pontuava "configuração insegura" **sem nunca ter lido um arquivo de configuração**).

---

## TIER A (novos)

### A5 — Índice de alcance precomputado — `internal/reach` ⚠

Este é o único item onde li o código-fonte a fundo, e vale a transcrição.

**O problema que resolve**: `touring wiring impact <symbol> --depth N` faz **BFS ao vivo** a
cada consulta. O Gortex precomputa a alcançabilidade **de entrada** em três profundidades e
responde blast radius em *"O(seeds × reach) map lookups instead of a live BFS"*.

**Estrutura**: para cada nó semente (`ImpactSeedKind` — funções, tipos, campos; arquivos,
imports e parâmetros são pulados), guarda três *tiers* como **arrays paralelos** em
`Node.Meta`: IDs, confidences e labels, sob `MetaReachD1/D2/D3` + `*Conf` + `*Label`.
Ordenados por ID para determinismo. Tiers vazios são **deletados** do metadata, não gravados
como `[]` — mantém o payload gob de cold start enxuto.

**A parte mais transferível é a invalidação**, que é O(1) e não apaga nada:

- **`buildCounter`** — `atomic uint64` global, incrementado a cada mutação do grafo ou em
  `InvalidateIndex()`. Metadata com contador defasado é **ignorada**, não removida.
- **`reachProcessEpoch`** — 16 bytes aleatórios gerados no startup. Impede que o contador
  reciclado de um daemon reiniciado **case com registros persistidos do processo anterior**.
  Um registro só é confiável com `MetaReachComplete` + epoch correto + build correto.
- **Publicação em lotes de 256** para não segurar locks durante travessias de minutos; lotes
  obsoletos **abortam** se o contador mudou no meio.
- **`MetaReachComplete` escrito por último** — o marcador de completude só aparece depois dos
  dados, então um crash no meio nunca produz um registro que parece íntegro.
- **`topologyGate`** — condition variable serializando leitores contra escritores, adquirida
  **antes** do `ResolveMutex` (que é não-reentrante) para evitar deadlock.

**Custo declarado**: O(N · E_avg) com fan-in típico de ~200 nós por semente; **< 1 s em
grafos de 50k nós**.

**Aplicação ao Touring**: o `wiring_map` tem 77.679 linhas e 11.889 produtores. A trinca
epoch + contador + marcador-de-completude-por-último é diretamente portável e resolve um
problema que o Touring **tem hoje**: nada distingue metadata de wiring gravada por um daemon
anterior de metadata da build atual. Foi exatamente essa classe de problema que produziu os
303 módulos fantasma que purguei hoje.

### A6 — `compress_bodies` / `internal/elide` — 30-40% de corte ⚠

`compress_bodies: true` nas operações de leitura **elide corpos de função para stubs,
preservando assinatura e doc-comments**, com redução declarada de **30–40% de tokens em 14
linguagens**. O pacote tem `elide.go` + **`salience.go`** — a saliência decide o que manter.
Compõe com `fidelity_globs` (tier full/compress/omit por glob) e `max_lines` (truncagem
ciente de AST que preserva o esqueleto de controle de fluxo e colapsa runs de folha).

O Touring não tem equivalente. Toda leitura é integral ou por range de linhas — um corte
sintático, não semântico. Para um workspace Rust com funções longas e `impl` blocks densos,
manter assinatura + doc e elidir corpo é o corte de maior razão sinal/token que existe.

**Nota de composição**: A6 só é comprovável com A2 (tiktoken real). Sem contagem verdadeira,
"30-40%" seria mais uma estimativa por `bytes/4`.

### A7 — Gate de drift em CI + compactação condicional do banco ⚠

Dois itens de processo que atacam dívidas que o Touring tem medidas:

**(a) Gate de drift.** ~~O Makefile tem `claude-plugin-check` … a REGRA #5 é aplicada por
disciplina minha.~~ **RETRATADO na verificação de 21:05 — a afirmação era falsa.**

O `ci.yml` do Touring **já tem** gates anti-drift, e mais de um: `sync_metrics`
(anti doc-drift), `sync_reference` (anti reference-doc drift), `file_size_gate` (anti bloat)
e root hygiene. Mais que isso: o gate do `ARCHITECTURE.md` está numa posição **mais evoluída**
que a do Gortex. O comentário no workflow documenta a transição:

> *"este gate compara uma medição LIVE contra o bloco commitado em ARCHITECTURE.md, então
> QUALQUER commit tocando .rs o invalidava e deixava o build vermelho até alguém lembrar de
> rodar `--sync`. O drift é mecanicamente derivável, então falhar o build pedia a um humano
> que fizesse o que o script já faz. Agora o CI regenera e faz push do resultado."*

Com salvaguarda: em forks, onde o `GITHUB_TOKEN` não escreve, cai para falha dura *"para que
o drift seja reportado em vez de silenciosamente ignorado"*. O `claude-plugin-check` do
Gortex só falha; o do Touring **corrige e só falha quando não pode corrigir**. Aqui somos
melhores, e a recomendação A7(a) sai da lista.

Resta uma pergunta menor e legítima: os gates cobrem `ARCHITECTURE.md` e docs de referência —
**não** verifiquei se o eixo code→SKILL.md tem cobertura equivalente.

**(b) Compactação condicional.** v0.61.0: *"Boot-time store compaction: 6,8 GB → 2,06 GB em
81 segundos quando >50% de páginas mortas **e** >1 GiB recuperável"* — dois limiares
explícitos governando uma operação cara. O Touring tem `symbols.db` com 186 MB e
`knowledge.db` com 77.679 linhas e **nenhuma política de compactação**; o `wiring_map`
acumulou fantasmas até eu purgá-los à mão hoje.

---

## TIER B (novos)

| # | Insight | Estado do Touring |
|---|---|---|
| **B8** | **Versionamento ancorado no contrato com o agente.** MAJOR = tool removida/renomeada, argumento obrigatório removido, ou *"tool return values change structurally, breaking agent parsing"*. Daemon expõe `DaemonVersion` no ACK do handshake para o cliente **feature-gate**. Janela declarada: *"fingerprints de v0.60.0 compatíveis até v0.62.x"* | v30.3.1 sem declaração do que quebra um agente. **Eu mudei a semântica de `visibility` no wiring hoje** — isso altera a leitura de qualquer consumidor da métrica de órfãos, e não há canal para anunciar |
| **B9** | **Blocos marker-guarded no CLAUDE.md.** `gortex init` insere rotas entre marcadores; *"os marcadores evitam conflito de merge em reexecuções — a seção entre eles é sobrescrita, as instruções ao redor ficam intactas"* | CLAUDE.md do TACO é 100% manual com limite de 400 L policiado por mim. Uma seção regenerável e delimitada permitiria conteúdo derivado do grafo sem consumir o orçamento manual |
| **B10** | **Verificação comportamental da instalação.** *"Observe chamadas como `graph_stats`, não leituras de arquivo. Se o assistente ainda usa `Read`, os hooks não instalaram: rode `gortex init --hooks-only`"* | `update-touring --verify-only` checa processo e inode. Ninguém checa se o **comportamento** mudou. O KPI `pillar_induction_ratio` existe e mede adoção — mas não é usado como teste de instalação |
| **B11** | **Telemetria por allowlist hardcoded.** *"ao fixar a allowlist, o agregador fisicamente não consegue registrar métrica não autorizada"*; guarda de dimensão `^[A-Za-z0-9_.<>+-]{1,32}$` que **remove** qualquer token com separador de path ou espaço; buckets em vez de contagens exatas; `DO_NOT_TRACK` só pode **desabilitar**, nunca habilitar; sem endpoint default, nada sai | O Touring não emite telemetria externa. **Registrar como referência de design** para o dia em que emitir — é a arquitetura certa, e barata quando feita antes |
| **B12** | **Sweep modes do LSP** (`demand` / `full` / `off`): `demand` varre só arquivos com sinal de não-resolvido — *"um arquivo sem nenhum dos dois sinais é pulado, então restart quente não paga sweep por ele"*. Enriquecimento chaveado por `(repo, provider, commit)`. Fallback gracioso: binário ausente → erro estruturado `no_lsp_for`, nunca trava | `touring-lsp` é feature-gated e não integra ao pipeline de resolução. **O mais valioso aqui não é o LSP** — é o padrão *"trabalho caro só onde há sinal de que ele muda algo"*, aplicável ao reindex |
| **B13** | **Scoping por intenção**: `locate` (search) defaulta a repo; `reach` (usages/callers) defaulta a workspace; e o default *"só estreita dentro do workspace da sessão; nunca alarga além dele"* | Touring é per-project; a lição é o **princípio**: defaults derivados da intenção da operação, com uma fronteira de isolamento que default nenhum pode atravessar |
| **B14** | **Piso de teste por feature**: *"extratores de linguagem exigem ao menos 3 casos de teste dedicados"*, escrito no CONTRIBUTING; `go test -race ./...` obrigatório | REGRA #21 exige 0 falhas mas não exige N testes por feature nova. Um piso por classe de contribuição é complementar, não redundante |

---

## Eixo evolução — o que a trajetória v0.59 → v0.63 ensina

O Gabriel pediu explicitamente insights de **evolução e desenvolvimento**. Cinco meses de
releases mostram padrões de **processo** que valem mais que qualquer feature isolada:

**1. Regressão de recurso vira manchete.** A v0.62.0 se chama *"Startup is faster and stop
eating your RAM"* e publica os três números lado a lado:

| Métrica (corpus de 29 repos) | Antes | Depois |
|---|---|---|
| Cold ready | 17m35s | 13m21s |
| Pico de footprint | 5,3 GB | 3,7 GB |
| Alocação total | 92,3 GB | 70,6 GB |

O Touring **nunca publicou** um número de memória ou de cold start. Sei que o reindex leva
2m19 porque medi hoje; **não existe série temporal**, então não sei se melhorou ou piorou
desde abril.

**2. Duas convergências com o meu trabalho de hoje** — que reforçam que os problemas são
estruturais da classe de sistema, não acidentes do Touring:

- v0.61.3: *"Reindex writer-gate timeout tolerance (no longer fatal)"* ≈ o EAGAIN por timeout
  de socket no `index rebuild` que corrigi hoje.
- v0.61.0: *"Daemon bounce on upgrade (prevents pre-upgrade code serving)"* ≈ exatamente o que
  `touring-rebuild.md` descreve: *"após rebuild o daemon antigo segura o inode velho —
  `update-touring --verify-only` detecta, exit 4"*. **O Touring detecta e reporta; o Gortex
  corrige automaticamente.** Detectar é melhor que ignorar; corrigir é melhor que detectar.

**3. Mudança de contrato é anunciada como tal.** v0.62.0: *"Breaking change: prefixos de repo
agora obrigatórios; `RepoMetadata.Unprefixed` removido"* — em um MINOR, sob a exceção pré-1.0
explicitamente documentada em `docs/versioning.md`, com nota de release. A regra é: se cabe
em MINOR, **diga em voz alta por quê**.

**4. Endurecimento de segurança entra no fluxo normal.** v0.63.0: *"Symlink escape prevention
during indexing and artifact serving"*. Um indexador que segue symlinks para fora da árvore é
uma leitura arbitrária de arquivos. **Vale checar se o walker do Touring tem essa guarda** —
não verifiquei (marcar como pendência, não como achado).

**5. A superfície de tools aprende.** v0.60.0: *"Per-workspace tool surface learning (survives
restarts)"* — o conjunto de tools se **adapta por workspace** a partir do uso e persiste. O
Touring tem RL (LinUCB, QTable, EMA) e uma allowlist **estática** de 23 tools. A peça para
aprender a superfície curada por projeto já existe e não está ligada a ela.

---

## PROVENANCE — rodada 2

**Fontes lidas nesta rodada** (5 lentes, 3 batches paralelos):

- **Código-fonte**: `internal/reach/reach.go` (transcrição de estruturas, algoritmo e
  sincronização) · listagens de `internal/reach`, `internal/elide`, `internal/audit`
- **Instruções do próprio repo**: `CLAUDE.md` — o análogo direto da constituição TACO
- **Docs**: `cli.md` · `agents.md` · `lsp.md` · `multi-repo.md` · `telemetry.md` ·
  `versioning.md` · `onboarding.md`
- **Prática de engenharia**: `Makefile` · `CONTRIBUTING.md`
- **Fixture**: `bench/fixtures/retrieval.yaml` (schema + 20 entradas verbatim)
- **Trajetória**: página de releases, v0.59.1 → v0.63.1

**Verificações executadas no Touring**:

```bash
# fecha o UNVERIFIED da rodada 1 — handshake MCP real, não proxy
touring serve  ← initialize + notifications/initialized + tools/list
  → 23 tools, 33.161 bytes, top: touring_decompose 2.947 B
python3 (regex #[tool(name=…,description=…))   → 142 tools nomeadas, 22.740 B de descrição
grep -c "derive(.*JsonSchema"                   → 146 structs Args
grep -c "///" crates/touring-server/src         → 5.776 doc-comments (viram field descriptions)
```

**Blueprint desbloqueado para S3** — o schema exato da fixture de retrieval, que converte a
recomendação da rodada 1 em algo implementável:

```yaml
# gortex-seed-v2
cases:
  - { id: exact-MCPServer, tier: exact, query: "mcp Server type",
      expected: [internal/mcp/server.go::Server] }
  - { id: exact-IndexFile, tier: exact, query: "IndexFile",
      expected: [internal/indexer/indexer.go::Indexer.IndexFile] }
```

Três tiers com semântica distinta: `exact` (nome de símbolo — *"BM25 deve dominar"*),
`concept` (paráfrase em linguagem natural), `multi_hop` (relacional, **semântica de
any-hit** com múltiplos IDs válidos).

**O detalhe que torna isto quase gratuito para o Touring**: o formato de ID
`caminho/arquivo.go::Tipo.Método` é **o mesmo** que o `wiring_map` já usa
(`module_file::symbol_name`). A fixture não precisa de tradução — só de curadoria.

**Limitações desta rodada** (declaradas):

- Li **um** arquivo Go a fundo (`reach.go`, via transcrição do WebFetch, não do arquivo bruto
  — confiança 0,8). Os demais pacotes vieram de listagem de diretório + documentação.
  `internal/audit`, `internal/elide` e `internal/savings` seguem com implementação
  **não verificada** — sei o que fazem, não como.
- `.gortex.yaml` retornou uma versão mínima/genérica; **o schema de guard rules e
  `architecture: layers:` não foi confirmado** na fonte. Marcar `UNVERIFIED`.
- Os números de release (17m35s→13m21s, 6,8 GB→2,06 GB) são **auto-relatados nas notas de
  release**. Não reproduzidos. Confiança 0,7.
- **Não verifiquei** se o walker de indexação do Touring tem guarda contra escape por symlink
  (item 4 do eixo evolução). É uma **pendência de verificação**, não um achado.
- 57 dos 87 pacotes internos seguem sem cobertura direta.

## ACTIONS — rodada 2 (acumuladas com a rodada 1)

**Novas imediatas:**

10. **S5** — marcador de truncagem legível por máquina no `DimScore`; `loop_converged.py`
    recusa convergir sobre cláusula com evidência truncada. Fecha a classe do F2.6.
11. **S4** — `importance` / `pinned` / `superseded_by` na memória + `memory supersede` +
    filtro no recall. O feromônio precisa evaporar.
12. **C1** — mover campos raros dos schemas de tools para um objeto `options`; alvo: trazer o
    `tools/list` de 33.161 B para perto dos 15.000 B do facade. Começar por
    `touring_decompose` (2.947 B).

**Novas de curto prazo:**

13. **A7(a)** — gate de drift em CI para a tríade code→docs→skill (REGRA #5 por código).
14. **A5** — precomputar reach d1-d3 com epoch + build counter + `complete` escrito por
    último, para `wiring impact`.
15. **A6** — elisão de corpo preservando assinatura e doc (após A2).
16. **A7(b)** — política de compactação condicional com limiares para `symbols.db` /
    `knowledge.db`.

**Recomendação revista:**

17. **A4 → `rewrite` primeiro.** Não exige decisão de governança nova, porque não remove
    capacidade: troca uma chamada por outra equivalente e melhor, e só quando a conversão é
    inequívoca. `deny` permanece opt-in armado.

**Pendência de verificação (não é achado):**

18. ~~Checar se o walker de indexação do Touring impede escape por symlink.~~
    **RESOLVIDO na verificação de 21:05 — virou achado. Ver V4 abaixo.**

---

# VERIFICAÇÃO — 2026-08-07T21:05-03:00

Toda afirmação feita sobre o Touring **sem comando executado** foi reverificada. Resultado:
4 confirmadas, 1 refinada para algo mais preciso e mais grave, **1 falsa e retratada**,
1 pendência resolvida como achado real.

## Confirmadas (1.0)

| # | Afirmação | Evidência |
|---|---|---|
| **V2** | `loop_converged.py` aceita evidência truncada em silêncio | `grep -n "truncat\|overflow"` → **zero ocorrências** no script |
| **V3** | A truncagem só existe como texto | `DimScore` tem `value`/`status`/`evidence`/`suggestions`/`latency_ms` — **nenhum campo de truncagem**. `dir_scan_overflow → Option<u64>` é chamado em 3 sítios de `scope_report.rs` e o fato só chega ao consumidor dentro da `String` de evidência |
| **V1b** | Memória sem `importance`/`pinned`/`superseded_by` | schema de `memory_entries`: `key, value, tier, entry_type, access_count, last_accessed_at, created_at, accessed_at, file_path, graph_blast_radius, palace_path, embedding, outcome_reward, outcome_context` — os três ausentes |
| **V6** | `tools/list` = 23 tools / 33.161 bytes | segunda medição independente: **byte-idêntica** |

## Refinada — S4 fica mais preciso e pior

Eu escrevi *"o feromônio nunca evapora"*. A verificação mostra algo mais específico: **o
mecanismo de evaporação foi construído e não está sendo alimentado.**

- A coluna `outcome_reward` **existe e está ligada**: `shared.rs::outcome_reward_select()`
  monta a expressão SQL (degradando para `NULL` se a coluna faltar), e `memory.rs:310`
  documenta que ela *"feeds straight back into `case_value`"* — é ela que produz os buckets
  `positive`/`negative`/`unobserved` que aparecem em todo `memory recall`.
- **Mas**: `SELECT COUNT(*), COUNT(outcome_reward) FROM memory_entries` → **7360 total, 11
  com reward. 0,15%.**
- E `memory_list_order_clause` oferece ordenar por `last_accessed`, `created_at`, `key`, com
  default `access_count DESC` — **`outcome_reward` não é opção de ordenação.**

`touring memory store --reward` existe, está documentado em `memory.rs:56`, e **eu nunca o
usei** — nem nas duas lições que armazenei nesta própria exploração. O resultado é que o
`guidance` que o recall imprime (*"reuse the approach in `positive`… `unobserved` carries no
verdict"*) opera sobre um corpus onde 99,85% é `unobserved`.

**É o padrão F4 pela terceira vez**: o dado certo existe, o caminho de escrita não o
preenche, e a métrica que depende dele fica cega. E aqui o custo é máximo, porque o feromônio
ACO é a essência declarada do sistema.

## Falsa — retratada

**A7(a)** — afirmei que a REGRA #5 (co-evolução) é aplicada por disciplina minha e que o
Gortex tem a versão em código. Errado: o `ci.yml` do Touring tem `sync_metrics`,
`sync_reference`, `file_size_gate` e root hygiene, e o gate do `ARCHITECTURE.md` **auto-regenera
e faz push** em vez de só falhar — posição mais evoluída que o `claude-plugin-check` do
Gortex. Correção aplicada na seção A7 acima.

## Pendência resolvida — vira achado

**V4 — o walker de indexação do Touring segue symlinks, inclusive para fora da raiz.**

`crates/touring-cli/src/cli/handlers/index.rs:446`:

```rust
for entry in entries.flatten() {
    let path = entry.path();
    if path.is_dir() {                    // ← is_dir() SEGUE o symlink
        if !should_skip_dir(name, skip_dirs) { … walk(&path, …) }
```

`Path::is_dir()` chama `fs::metadata`, que resolve o link — ao contrário de
`symlink_metadata`. Não há `follow_links(false)`, não há `canonicalize` + verificação de
prefixo contra a raiz, e **não há conjunto de visitados**. Duas consequências:

1. **Escape da raiz** — um symlink apontando para fora do projeto tem seu conteúdo indexado
   no DB do projeto e fica pesquisável. É exatamente a classe que a v0.63.0 do Gortex
   endureceu (*"symlink escape prevention during indexing and artifact serving"*).
2. **Ciclo → recursão ilimitada** — `a/link -> a` recursa até estourar a pilha. Esta é a
   consequência mais imediata, e é um crash, não só uma questão de exposição.

**O padrão seguro já existe neste workspace**: `touring-bindings/src/capnp/discover.rs:157`
usa `WalkDir::new(root).max_depth(…).follow_links(false)`. Não foi aplicado no caminho de
indexação.

**Severidade calibrada** (confiança 0,9): para o uso atual — os projetos do próprio Gabriel —
o risco de exposição é baixo, porque exige um symlink dentro do repo. Como produto
per-project rodando sobre repositórios de terceiros, é lacuna real de endurecimento. O ciclo
infinito é um risco de disponibilidade independente de quem é o dono do repo.

## Ações que a verificação acrescenta

19. **V4** — `follow_links(false)` + verificação de prefixo canônico contra a raiz + conjunto
    de visitados no walker de `cli_index_rebuild`. Reusar o padrão de `capnp/discover.rs`.
20. **S4 revisado** — antes de qualquer coluna nova: **usar `touring memory store --reward`**.
    O mecanismo existe; alimentá-lo custa uma flag. Só depois avaliar `importance`/`pinned`/
    `superseded_by` e ordenação por reward.
21. ~~A7(a) gate de drift~~ — **removido, já existe e em forma melhor.**

