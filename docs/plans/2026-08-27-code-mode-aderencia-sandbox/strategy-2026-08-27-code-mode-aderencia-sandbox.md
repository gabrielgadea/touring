<!-- OKF document -->
---
okf_version: "1.0"
type: Strategy
title: "Aderência a code mode — diretrizes de infraestrutura + enriquecimento do sandbox CEG"
description: "Estratégia final do loop 2026-08-27: ESCOLHER/ACERTAR/MEDIR — 24 diretrizes com fontes primárias + plano dos 10 gaps do sandbox em 5 fases + régua MED-1."
plan_id: 2026-08-27-code-mode-aderencia-sandbox
tags: [code-mode, ceg, sandbox, aderencia, estrategia, loop]
timestamp: 2026-08-28T00:55:00-03:00
authority: Gabriel Gadea
status: final
---

# Aderência a code mode + sandbox CEG — estratégia (final)

## 1. A tese que ordena tudo

**Afordância mora no executor; aderência mora na forma do que é apresentado.**
O modelo escreve código melhor do que emite JSON de tool-call porque código é a
forma mais representada no pré-treino (Cloudflare: *"LLMs are better at writing
code to call MCP than at calling MCP directly"* — tokens de tool-call são
formatos sintéticos "nunca vistos in the wild"). Medido: **CodeAct (ICML 2024,
arXiv:2402.01030) +20,7pp de success rate** (74,4% vs 53,7% JSON) e 2,1 turnos
a menos. A medição interna confirma o mecanismo: a rota code passou de 15%→90%
do eixo quando o deny passou a **entregar o programa pronto** (25/08).

A aderência decompõe em **ESCOLHER** (o modelo opta pelo programa quando ele é
objetivamente melhor), **ACERTAR** (o programa funciona de primeira) e **MEDIR**
(sem régua, aderência é narrativa).

## 2. Diretrizes de infraestrutura para criação de código (entregável A, final)

### ESCOLHER — a apresentação decide

| # | Diretriz | Fonte primária |
|---|---|---|
| **E1** | UMA ferramenta de execução + as demais tools como **API tipada (.d.ts/stub) no prompt**, nunca o catálogo inteiro de schemas | Cloudflare code mode; nosso stub S4 byte-estável ✓ |
| **E2** | Híbrido com **regra de custo declarada**: "loops/condicionais/agregação → programa; operação simples → tool direta". Always-exclusive REJEITADO ("taxa o caso comum") | dsh 15/06; TanStack create-system-prompt.ts:63-71; nosso S3 ✓ |
| **E3** | O colapso mora no **executor** e o deny **nomeia a rota de volta** — omitir schema sem enforcement não enforce nada (dsh provou: modelo chamou tools escondidas e EXECUTARAM) | dsh 07/08; nosso S1+T3 ✓ |
| **E4** | A instrução do transport abre nomeando **TODOS os args obrigatórios** (`{code}` sem `{description}` perde o programa em INVALID_ARGS) | dsh 15/06 §"What the model sees" |
| **E5** | **Progressive disclosure** p/ catálogo grande: filesystem como descoberta / search_tools por nível / lazy tools | Anthropic code-execution-with-mcp; TanStack SKILL.md §4 |
| **E6** | Caminho code **objetivamente mais barato**: 150k→2k tokens (98,7%); 70-77% output tokens; loops 11-15× mais curtos | Anthropic; MCP discussion #1780 |

### ACERTAR — a forma do SDK decide

| # | Diretriz | Fonte primária |
|---|---|---|
| **A1** | **SDK plana**: funções top-level autocontidas, nunca fluent/OO profundo (alucinação de parâmetros + this-binding) | code-mode-arch.md §2; TanStack `external_*` |
| **A2** | **Um objeto de config nomeado**, nunca N posicionais de mesmo tipo primitivo | arch.md §2/§5A |
| **A3** | **.d.ts com JSDoc DENSO** (restrições concisas, enums estritos, `@throws`) — o tipo é restrição de raciocínio | arch.md §2; dsh jsonSchemaToTs→JSDoc |
| **A4** | **Bindings retornam JSON canônico TIPADO** (ToolOutputMap), nunca prosa para raspar | dsh 20/07 typed-tool-returns |
| **A5** | **Erros estruturados, taxonomia ortogonal** (exception/timeout/abort/worker-exit/invalid-output/output-limit) + código estável machine-routable | dsh 11/06; nossa RunFailureKind (dsh-derived ✓) |
| **A6** | **output-limit explícito**, nunca truncamento silencioso — o modelo ESCOLHE um resultado menor | dsh 20/07; nosso spill+retrieval_hint ✓ |
| **A7** | **Concorrência declarada na instrução**: "read-only independentes PODEM sobrepor; mutantes correm sozinhas, em ordem" | dsh SDK_INSTRUCTIONS:256 (maxParallelSubCalls=10) |
| **A8** | **Exemplo canônico completo** no prompt (fetch paralelo + reduce + return único) | TanStack create-system-prompt.ts:85-100 |
| **A9** | **Prompt byte-estável** (ordem lexicográfica) — prefix cache; retry append-only (120× discount) | dsh ts-types.ts:273; arch-rust §2 |
| **A10** | **Retry loop com autocorreção** (lint → execução → diagnóstico com linha/coluna → re-inferência), **máx 3 tentativas** | arch.md §2; arch-rust §5 |
| **A11** | **Trust paritário ao bash** (containment ≠ boundary, sem cerimônia unsafe-ack); **segredos NUNCA entram no sandbox** (TanStack CRITICAL — diverge do nosso default; ver §6) | dsh 15/06; TanStack SKILL.md |
| **A12** | **Fail-LOUD** em linguagem desconhecida — SDK na linguagem errada por fallback silencioso é o pior mundo | dsh 31/07 language-dispatch |
| **A13** | **Scripts bons viram ativos**: graduação por telemetria (provisional 10 exec ≥90% → trusted 100 exec ≥95% → tool nativa); registry vetorial top-K + hash AST p/ dedup | arch.md §4; TanStack trust strategies; nosso harvest+escada ✓ |

### MEDIR — a régua decide

| # | Diretriz | Fonte |
|---|---|---|
| **M1** | **Régua dedicada de aderência** (`successfulExecuteCalls/total`, compilationFailures, runtimeFailures, redundantSchemaChecks, wasted-attempts). **O Touring não tem o equivalente ao CME do TanStack → item MED-1 na fase SDK-1** | TanStack models-eval (primário, local) |
| **M2** | Aderência é **modelo × apresentação**: abaixo do piso de capacidade nenhum prompt salva (mistral responde markdown sem invocar; granite emite TS inválido; qwen alucina resultado) | TanStack eval-config.ts:49-56 |
| **M3** | Contenção determinística no substrato: fuel metering + epoch interruption ("não evitável por código malicioso"); nosso busy budget via /proc é o análogo subprocess | Context7 wasmtime; arch-rust §3 |

### Anti-padrões medidos (17 no relatório do agente; os mais afiados)

Omitir schema sem colapsar o executor · prompt declara capability sem a rota ·
transport sem nomear args obrigatórios · namespaces profundos · .d.ts gigante
sem top-K · always-exclusive · erro-string achatado · truncamento silencioso ·
double success contract · prompt não-determinístico · **segredos no sandbox** ·
REPL persistente (quebra reconstrutibilidade) · node:vm como sandbox · fallback
silencioso de linguagem · result elision como substituto · aliases sanitizados.

## 3. Plano dos 10 gaps do sandbox (entregável B) — fases INNER

| Fase | Gaps | Risco |
|---|---|---|
| **RUN-1** | deno runtime-aware args · venv read-only (pandas/pydantic/httpx via PYTHONPATH, criado no host) · preflight `touring sandbox-runtimes status` | baixo |
| **SEG-1** | UDP via seccomp no pre_exec · memória **híbrida** (RLIMIT_AS ~25% RAM + cgroup quando slice delegado existir) · read-narrowing por roots enumerados **com gate de compat** (suite CEG + bateria) | médio |
| **NET-1** | `--allow-net-port` (Landlock NetPort — builder já aceita) + residual documentado ("443 fala com qualquer host") | médio |
| **SDK-1** | orchestrate Node SDK · sub-calls paralelas (pool bounded 10) · scratch dir estável (`TOURING_SCRATCH_DIR`) · **MED-1: régua de aderência do `touring run`** (counters + KPI no journal) | baixo |
| **OUT-1** | streaming de saída | alto, último |
| **DOC-1** | diretrizes E/A/M publicadas em `docs/code-mode.md` + strategy v-final + memory | — |

Ordem: **RUN-1 → SEG-1 → NET-1 → SDK-1 → OUT-1**; DOC-1 no fim. Cada fase fecha
com cross-audit de escopo + 50-dim (6 P0) + `loop_phase_close.py`. Deploy = gate
humano no fim (update-touring).

### Critérios de aceite medidos (não declarados)

- RUN-1: `--lang ts` executa TS com tipos via deno; preflight lista 11
  linguagens presente/ausente; `import pandas` OK.
- SEG-1: UDP p/ 8.8.8.8:53 **não sai**; alloc 2GB falha e 512MB passa;
  `cat ~/.aws/credentials` falha mesmo existindo + bateria compat verde.
- NET-1: `--allow-net-port 443` permite `curl :443` e nega `:80`; sem flag,
  deny-all segue.
- SDK-1: `--lang js --orchestrate` funciona; paralelas medidas < sequenciais;
  scratch persiste entre runs; `touring kpi` carrega a razão de aderência.
- OUT-1: saída incremental de run longo (probe com sleep).

## 4. O que NÃO muda

Contenção kernel-first (Landlock FS+TCP+IPC) — nenhum gap abre o que o kernel
fecha; grants são explícitos por flag. `env_clear` + whitelist de credenciais e
o opt-out `TOURING_SANDBOX_NO_CREDENTIALS=1` seguem. Fachada MCP por escopo (S1)
e gates de rajada (S3/G10) — medidos hoje.

## 5. Evidência

- Diagnóstico do loop: `diagnostics/touring-20260827T213107.md`
- Cross-audit: `docs/audits/cross-audit-2026-08-27.md`
- Pesquisa primária (25 diretrizes, 17 anti-padrões, 7 open questions): agente
  pesq-aderencia — Cloudflare blog.cloudflare.com/code-mode · Anthropic
  anthropic.com/engineering/code-execution-with-mcp · CodeAct arXiv:2402.01030
  (ICML 2024) · dsh 5 notas (15/06, 07/08, 20/07, 31/07, 11/06) · TanStack
  models-eval + SKILL.md + create-system-prompt.ts (clone local) · Context7
  /denoland/docs + /bytecodealliance/wasmtime
- Docs locais: `docs/analyses/code-mode-arch{,-rust}.md`

## 6. Nota de postura — segredos no sandbox

TanStack marca "passar API keys ao sandbox" como CRITICAL; nosso default passa
credenciais (gh/aws/cargo precisam autenticar) com opt-out
`TOURING_SANDBOX_NO_CREDENTIALS=1`. A janela de exfiltração hoje é: rede TCP
deny-all (kernel ✓) + **UDP aberto (fecha em SEG-1)** + FS legível (read root
`/`, estreita em SEG-1). Com SEG-1 fechado, o default fica defensável; a
divergência de default com o TanStack fica registrada aqui como decisão
consciente, revisitável se um host rodar código não-confiável.

## 7. Adendos da leitura direta dos 2 docs (Gabriel pediu leitura própria)

1. **NET-2 (futuro): proxy transparente com whitelist de DOMÍNIOS** — a resposta
   completa ao host-filtering que o Landlock não faz (arch.md §3-C: "toda saída
   passa obrigatoriamente por um proxy de auditoria com lista de permissões").
   NET-1 (por porta) fica como está; o proxy é a evolução.
2. **WASM guest (fuel/epoch, arch-rust §3): NÃO adotar agora.** Os budgets
   normativos deles (memória 32-128MB, timeout 500-2000ms, cold <50µs) são para
   snippets efêmeros puros; o nosso sandbox precisa ler o projeto e rodar
   toolchain pesado (rustc/go) — o subprocess com Landlock é a camada certa para
   o nosso caso, e WASM isolaria DEMAIS (sem FS do projeto). Fuel/epoch ficam
   como referência se um dia existir uma camada de snippets puros sem FS.
3. **Escada de trust com números** (provisional: 10 exec ≥90%; trusted: 100 exec
   ≥95%) — medir a nossa escada de harvest (run.rs) contra esses thresholds e
   alinhar/documentar a divergência (entra em SDK-1).
4. **Cap de 3 tentativas no loop de auto-depuração** (arch-rust §5) — carregar
   como default nos retries com feedback dos ADWs/gates (entra em DOC-1 como
   diretriz A14).

## 8. Entrega (v-final, 28/08/2026)

Todas as 5 fases do roadmap fechadas com prova ao vivo + `loop_phase_close.py`:

| Fase | Entregue | Prova ao vivo |
|---|---|---|
| RUN-1 | deno runtime (js/ts com tipos), `LANGUAGE_CANDIDATES` (11 langs), preflight `touring sandbox-runtimes status`, venv `setup-venv` (pandas/pydantic/httpx via PYTHONPATH) | bateria 9/9 linguagens verde |
| SEG-1 | UDP negado no kernel (seccomp BPF `socket(AF_INET*, SOCK_DGRAM)` → EPERM), RLIMIT_AS 80% RAM (medido: deno V8 morre ≤32GiB, passa 48GiB), read-narrowing por roots enumerados | UDP 8.8.8.8:53 PermissionError; `~/.aws/credentials` FECHADO; compat 9/9 |
| NET-1 | `--allow-net-port` (Landlock NetPort) + waiver só-de-rede no gate; residual documentado (porta fala com qualquer host) | `curl :443` com flag = HTTP 200; `:80` = exit 7 (kernel); sem flag = deny |
| SDK-1 | `parallel` nos DOIS SDKs (pool 10, erro por slot, origin sob mutex); SDK JS Promise-based (node/bun/deno) gerado das MESMAS tabelas (D8); MED-1 régua `code_mode_adherence` no `touring kpi`; escada trust verificada ligada (TanStack ≥10@90% / ≥100@95%) | py 3,76×, js 56× vs sequencial; guard `js_sdk_mirrors_the_same_method_table` |
| OUT-1 | `--stream` (tee incremental no stderr do pai; envelope dono do stdout) + salvage no timeout (parcial viaja no sentinela -2 com hash+spill; stderr do filho lidera) | A@0,01s B@1,01s C@2,01s; `progresso: 90/100` viaja no -2 |
| DOC-1 | diretrizes E/A/M + A14 (cap 3 retries) publicadas em `docs/code-mode.md`; manual atualizado (flags novas, `parallel`, SDK JS, salvage, max 600000) | esta seção |

**Residuais conscientes** (não-gaps, decisões): TOURING_SCRATCH_DIR revertido (X6 nega
write-redirection — exige design de write-root grant antes de reintroduzir); NET-2 proxy
por domínio (futuro); WASM guest não adotado (§7.2); stub .d.ts do SDK JS não emitido
(o .pyi segue sendo o contrato canônico de leitura — P23).
