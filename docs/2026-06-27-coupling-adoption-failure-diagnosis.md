# Diagnóstico — Falha de Adoção do Acoplamento (medido ao vivo)

> **Data**: 2026-06-27 | **Origem**: observação de Gabriel — ao escrever a proposta de telemetria,
> o agente NÃO usou code-mode, scripts, skill nem o fluxo touring; a 1ª rodada saiu incompleta.
> **Método**: auto-medição acoplada (code-mode determinístico sobre `activity.jsonl` + `gate-metrics`).
> **Veredito**: o backend está construído e os hooks vivos, mas a **adoção pela LLM é ~zero** — e essa é
> a causa-raiz que precede (e contamina) todo trabalho posterior, inclusive a telemetria.

## 1. Evidência medida (FACT 1.0)

| Sinal (esta sessão) | Valor | Leitura |
|---|---|---|
| Sugestões de acoplamento emitidas pelos hooks | **27** (28.173 B ≈ 28 KB injetados) | o enrichment funcionou — ofereceu o caminho certo 27× |
| Execuções de **code-mode** (`ctx_execute`) | **0** | nenhuma |
| `ceg_captured` / `ceg_sandboxed` | 42 / **0** | 42 comandos rodados, 0 pelo gate real (`touring exec`) |
| `adoption_ratio` (métrica-mãe) | **UNMEASURABLE** | gap G1 provado ao vivo |
| `suggestion_uptake` | **UNMEASURABLE** (~0 observ.) | gap G2 provado ao vivo |
| `diagnose_health.py` (Layer-3) | rodou de 1ª, YELLOW 0.679, 5/5 ok | estava disponível o tempo todo; fiz à mão 4× |
| índice `symbol_count` | **238.703** | índice riquíssimo nunca consultado via `index find` |

## 2. Os meus passos — ad-hoc vs. acoplado

| Subtarefa | O que fiz | O que o acoplamento faria | Falha |
|---|---|---|---|
| Extrair métricas dos 6 docs | Agent Explore (LLM sintetiza) | script determinístico (regex) → completo na 1ª passada | violei *code-analyses-llm-synthesises* → **2ª rodada necessária** |
| Navegar código | subagents `grep`/`read` | `index find`/`ast`/`wiring` + `discover_symbol.py` | texto cego sobre índice semântico |
| Saúde (FASE 0) | doctor+status+gate-metrics+learning manuais 4× | 1× `diagnose_health.py` | re-derivei à mão |
| Multi-step (8+ passos) | na cabeça | `touring decompose` (DAG) | sem estrutura/checkpoint |
| Agregação de dados | `jq` ad-hoc | code-mode (`ctx_execute`) | 0 code-mode |
| Skill | nenhuma | `Skill("Touring")` no turno 1 | hook pediu todo turno |

## 3. Causa-raiz — três culpados

- **(A) Eu — adoção gulosa** `[FACT]`: prior forte para bash/grep/read/Agent, prior ~zero para touring/code-mode. Sob `U(a)=P·V−C`, o ad-hoc tem custo imediato menor → escolhido gulosamente. Prova viva da tese do backend.
- **(B) Backend — ativação parcial** `[FACT]`: o canal de code-mode mais ergonômico (MCP `touring_ctx_execute`, que os hooks mandaram usar) **está OFF** (0 tools touring MCP expostas). Scripts Layer-3 e inferlets existem via CLI mas **não se auto-invocam**.
- **(C) Design — passivo, não coercivo** `[INFERENCE 0.9]`: o enrichment **persuade** ("MUST touring…") via texto → tratado como **banner-blindness**. A `coupling-strategy` diz verbatim que persuasão semântica FALHA e que a solução é coerção arquitetural (mudar `U(a)`). 28 KB → ~0 adotado é a medida da falha.

## 4. Modelo do veredito: Construído ≠ Ativado ≠ Adotado

```
CONSTRUÍDO ✅  C1–C14, hooks (27 emits), CEG, engines, 11 scripts, inferlets, índice 238k
   ↓
ATIVADO    🟡  hooks emitem · MAS code-mode MCP OFF · scripts não auto-invocam
   ↓
ADOTADO    ❌  a LLM não seguiu (uptake ~0) — o elo final e mais fraco
```

"Não funciona" = **não mudou o comportamento**. A causa não é (só) código quebrado — o detector/sugestor
acertou (`code-mode-loop conf=0.95` exatamente no loop shell). O elo quebrado é **adoção + coerção ausente +
um canal-chave desligado**.

## 5. A implicação (por que resolver isto ANTES da telemetria)

Medir a persuasão falhar (telemetria/uptake) não muda comportamento. O fix real é tornar o caminho acoplado
o de **menor resistência** (coerção via `U(a)`), de modo que **todo trabalho posterior** — inclusive a
telemetria — seja feito de forma acoplada por construção:

1. **Reabilitar o code-mode sem depender do MCP** — um canal **CLI** (ou API) para o LLM rodar scripts no sandbox CEG.
2. **Master CLI commands** — orquestradores de 1-comando (como as master MCP tools), reduzindo o custo do caminho touring (1 comando vs 5).
3. **Auto-invocação** — skill + `diagnose_health.py` no SessionStart, não por memória.
4. **Coerção > persuasão** — em casos fortes, o hook **transforma** a ação (`blast_mutation`), não só sugere.

→ Proposta detalhada em `docs/2026-06-27-coupling-codemode-cli-and-master-commands.md`.
