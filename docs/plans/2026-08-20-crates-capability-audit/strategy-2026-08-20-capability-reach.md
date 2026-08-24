---
okf_version: "1.0"
type: Strategy
title: "A capacidade existe; o fluxo não a alcança"
description: "Auditoria de alcance das capacidades dos 42 crates do Touring pelos fluxos ADW e pelo harness loop-engineering, nos eixos explorar/planejar/ler/prever/verificar/atestar/auditar/qualidade/validar"
plan_id: 2026-08-20-crates-capability-audit
tags: [adw, loop-engineering, capability-reach, orphan-capability, ceg, dspy, conformal]
timestamp: 2026-08-20T02:30:00-03:00
---

# A capacidade existe; o fluxo não a alcança

Ligado a [index](./index.md) · diagnóstico em [diagnostics/touring-20260820T022335.md](./diagnostics/touring-20260820T022335.md)

## Objetivo

Medir — não estimar — quanto da capacidade construída nos 42 crates (≈660k LOC) é
**alcançável por um fluxo** (spec ADW, script do harness ou hook registrado), e converter
a diferença em potencialização dirigida. A pergunta não é "o que o Touring tem"; é
"o que ele tem e nenhum fluxo consegue chamar".

## A medição

Superfície real extraída do binário vivo (`touring --help`, **stderr** — o `--help` não
escreve em stdout, gotcha já documentado para `--version`; meu primeiro instrumento mediu
0 comandos e teria "provado" que nada é usado).

| via de alcance | comandos |
|---|---:|
| superfície total | **138** |
| alcançado por um **spec ADW** | **14** |
| alcançado só por script do harness | 25 |
| registrado como **hook** (via própria) | 13 |
| ciclo de vida / instalação (não é passo de fluxo) | 27 |
| **nunca alcançado por via alguma** | **67** |

Os 14 que um spec ADW alcança: `adw · memory · explore · portfolio · gotcha ·
investigate · conflict-check · blast · audit · wiring · scout · read · learning · index`
— **a camada de descoberta e memória, inteira**. Os fluxos exploram e lembram bem.

## O achado central

Cruzando a superfície com os eixos nomeados no pedido:

| eixo | comandos no eixo | em spec ADW | nunca alcançados |
|---|---:|---:|---:|
| planejar / decidir | 9 | **0** | 7 |
| prever / calibrar | 8 | **0** | 8 |
| conformidade / atestar | 3 | **0** | 3 |
| auditar / qualidade | 13 | **0** | 11 |
| explorar / ler (além do já usado) | 14 | **0** | 13 |
| **total** | **47** | **0** | **42** |

**Nenhum fluxo ADW decide, prevê, atesta ou pontua com a máquina que existe para isso.**
Ele descobre, executa e pede para um LLM julgar. A camada decisória foi construída,
compila, roda — e é inalcançável a partir de qualquer spec.

## Verificado por execução (não por `--help`)

Cada item abaixo foi **executado** nesta sessão; o valor é a saída real.

| comando | saída real | o que isso substitui hoje |
|---|---|---|
| `touring health` | `GREEN`, exit 0, composite 0.768 | doctor+status+drift refeitos à mão pelo harness |
| `touring guard <file>` | `GO`, composite 0.765, blast 0 | blast+tdg+gotcha+memory encadeados à mão |
| `touring route --depth 3 --files 12 …` | `L3 · Orchestrated · parallelism=4 · P1→P2→P5→P6` | roteamento CILA decidido por julgamento do LLM |
| `touring calibrate-confidence -j` | `calibrated:true`, τ=0.80, cobertura 0.9, **`defer_hitl:true`**, destilado de **1779** observações, n=653 | o portão humano posto à mão, sem garantia de cobertura |
| `touring predict-action 'cargo test --workspace'` | ~~p(sucesso)=**0.990**, confiança alta, 1779 observações~~ **ver correção abaixo** | nada — não há previsão antes de gastar o comando |
| `touring world-model-status -j` | `durable:true`, **12.302 exemplos**, brier 0.0001, snapshot em disco | nada |
| `touring evidence '<cmd>'` | veredito `Deny` (0.675) com bundle por eixo: X2 1.0 · X3 1.0 · X4 0.5 · X5 1.0 · **X6 0.0** | veredito sem forense por eixo |
| `touring consistency --a-nodes … --b-nodes …` | `ged=0 distance=0.000 consistent=true` | merge de fan-out `parallel` por **concatenação** |
| `touring budget-verify --root … --node …` | `conserved: Σ 2 node budgets ≤ root` em ℕ⁶ | nada — orçamento do DAG não é conservado por prova |
| `touring assist list-kinds` | inclui **`auto_wire`** | REGRA #0 cumprida à mão, órfão a órfão |
| `touring quality-signal -j` | gargalo `acyclicity` **com `cycle_paths`** | sinal sem causa-raiz |

> **Correção (D3, 20/08)**: esta linha listava o `predict-action` como capacidade
> verificada. **Ela era um quarto instrumento inerte, e eu não vi.** Executei-o uma vez,
> com um único comando, e li 0.990 como evidência de que funcionava. Com seis comandos
> — `ls`, `cargo build --release`, `python3 -c 'sys.exit(1)'`, `false`,
> `grep -r xyz /nonexistent`, `sleep 1` — **todos devolvem p=0.990312, `confidence: High`
> e `matched_observations` igual ao corpus inteiro**, com `distinct_features: 1`. Era a
> taxa-base vestida de previsão. Causa-raiz: `features_of` em `cli/predict.rs` passava o
> envelope `{"tool_name":…,"tool_input":{…}}` onde `from_pre_tool` espera o objeto
> `tool_input`; o comando nunca chegava ao extrator e `intent_class` caía no default
> constante. Corrigido em D3, com duas guardas — uma exige classes distintas para
> comandos distintos, a outra fixa o envelope como o modo de falha.
>
> **Lição de método**: uma execução com uma entrada não distingue função de constante.
> Os outros três instrumentos desta seção foram pegos porque o valor era obviamente
> errado (0.19, `false`, 0). Este devolvia um número plausível.

## Três instrumentos que se declaram e não medem

Mesma forma de defeito que esta sessão vem encontrando: parece saudável, não faz nada.

1. **`harness-metric` composite = 0.19** — com `stateful: 0.0` e `evolving: 0.0`. A métrica
   oficial de qualidade do harness diz que o harness não é estatal nem evolutivo. Ninguém
   a consulta, então o veredito nunca chegou a lugar algum.
2. **`attest-contract` → `attested: false`** — o contrato constitucional (pin blake3 de
   CLAUDE.md + `rules/*.md` + vereditos estruturais por cláusula) **não está atestado**.

   > **Correção (D2, 20/08)**: eu escrevi aqui que `judge_attest.py` "reimplementou em
   > Python uma versão mais fraca do que já existia em Rust". **Falso — eles atestam
   > coisas diferentes e são complementares**: `attest-contract` cobre a CONSTITUIÇÃO
   > (CLAUDE.md + rules), `judge_attest.py` cobre os GRADERS do loop (sha256 dos scripts
   > + inventário de cláusulas por AST). Nenhum substitui o outro.
   >
   > E o `attested: false` tinha causa mecânica, não constitucional: `CRITICAL_RULES`
   > exigia `"touring-native tooling-canonical-workflows.md"` — prosa fundida a um nome
   > de arquivo, **com espaço no meio**. Nenhum arquivo assim pode existir, a "REGRA #14"
   > que ele respaldaria não está no CLAUDE.md, e nenhuma rule cobre canonical workflows.
   > O atestado reprovava para sempre por um deslize de digitação. Removido em D2, com
   > guarda que recusa espaço, exige `.md` e recusa lista vazia.
3. **`granularity status` → `total_pulls: 0`** — bandit de fator de split de tarefa com
   zero puxadas. Quatro braços, nenhum dado. Aprendiz ligado a nada não aprende; só parece.

E **`repo-score` = grade D** com **4 de 11 categorias `"source":"stub"`** valendo **90 dos
269 pontos**: um terço da escala executiva é uma constante que não pode se mover. O próprio
payload carrega a dica do que ligar (`wire to cargo deny check + unwrap_audit`).

## A camada mais funda: capacidade sem verbo

Abaixo do CLI há capacidade que nem comando tem — inalcançável por construção.

| símbolo | crate | consumidores fora do crate |
|---|---|---:|
| `DspyCompiler` · `BootstrapFewShot` · `MCTSTeleprompter` · `CompiledPrompt` | touring-cortex/dspy | **0** |
| `MctsPlanningFascicle` | touring-cortex/fascicles | **0** |
| `EvidenceAdapter` | touring-cortex/fascicles | **0** |

`touring-cortex/src/dspy` é otimização de prompt **a partir de demonstrações** — com
assinaturas prontas para `code_generation`, `code_reflection`, `test_generation`. Os nós
`agent` dos ADWs hoje carregam persona escrita à mão. A máquina para **compilá-la a partir
dos runs que deram certo** existe, compila, e nenhuma linha fora do próprio crate a toca.

## Deliverables propostos

Atômicos, independentemente entregáveis, ordenados por razão valor/custo.

| # | entrega | eixo | tam |
|---|---|---|---|
| **D1** | Fragmento `gate-health` + `gate-guard`: trocar as cadeias manuais pelos masters `health`/`guard` nos specs e no harness | qualidade | **S** |
| **D2** | Ligar `attest-contract` ao `judge_attest.py` (Rust é a fonte; Python vira consumidor) e fazer `attested:false` bloquear | conformidade | **S** |
| **D3** | Nó `code` de `predict-action` antes de cada gate caro (cargo/test): abortar barato o que a previsão reprova | previsão | **S** |
| **D4** | `calibrate-confidence` como árbitro do **portão humano** do loop — defer com garantia de cobertura, em vez de regra escrita à mão | previsão | **M** |
| **D5** | `consistency` como critério de merge do fan-out `parallel` (hoje: concatenação) | validação | **M** |
| **D6** | `route` no intake do `factory`/`decompose`: nível CILA e topologia por comando, não por julgamento | planejamento | **M** |
| **D7** | `budget-verify` na criação do DAG — conservação ℕ⁶ provada, não presumida | planejamento | **M** |
| **D8** | Despapelar `repo-score`: ligar as 4 categorias stub (90 pts) às fontes que o próprio payload indica | qualidade | **M** |
| **D9** | Alimentar `granularity` com o resultado real de cada split de DAG (fechar o laço do bandit) | planejamento | **M** |
| **D10** | `assist auto_wire` no fecho de fase: REGRA #0 por comando | excelência | **M** |
| **D11** | Expor `harness-metric` no `loop_converged.py` como cláusula, com piso — a métrica do harness passa a ter consequência | conformidade | **M** |
| **D12** | Verbo CLI para a pilha DSPy (`touring prompt compile`) e compilação das personas dos nós `agent` a partir dos runs vencedores | orquestração | **L** |

## Sequenciamento e dependências

```
D1 ─┐
D2 ─┼─→ D11 (harness-metric só vira cláusula depois que atestação e masters estão de pé)
D3 ─┘
D6 ──→ D7 ──→ D9        (rotear → provar orçamento → realimentar o bandit)
D5 (independente)
D8 (independente)
D10 (independente)
D4 ──→ (revisita o portão humano; depende de D3 estar em produção para ter sinal)
D12 (independente, maior; depende de nada, entrega por último por custo)
```

Aciclico. D1-D3 são S e destravam D11. D12 é a única L.

## Riscos

| risco | prob | impacto | mitigação |
|---|---|---|---|
| `calibrate-confidence` deferir demais e travar o loop (D4) | MÉDIA | ALTO | rodar em sombra por N fases, comparar com o portão atual antes de trocar |
| `attest-contract` bloquear tudo ao virar cláusula (D2) | ALTA | MÉDIO | mesma regra do `judge_intact`: cláusula que **some** bloqueia, cláusula que **muda** fala sem bloquear |
| Ligar stub do `repo-score` derrubar a nota D→F ao medir de verdade (D8) | ALTA | BAIXO | é o objetivo: nota honesta baixa vale mais que constante alta |
| `consistency` recusar merges hoje aceitos (D5) | MÉDIA | MÉDIO | medir GED da população atual antes de escolher o limiar |
| DSPy exigir demonstrações que não temos (D12) | MÉDIA | MÉDIO | os `.recordings` dos ADWs já existem; medir volume antes de construir |

## O que NÃO foi medido

- Cobertura de teste real dessas capacidades (`mutation-test` existe e não foi rodado — é caro).
- Se as 85+ ferramentas MCP têm a mesma taxa de alcance que o CLI.
- `clones` precisa de `detect` antes de `stat`; a duplicação do workspace segue não medida.
