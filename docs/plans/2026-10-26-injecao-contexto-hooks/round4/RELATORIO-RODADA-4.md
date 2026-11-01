---
type: Report
title: "Rodada 4 — o diagnóstico qualitativo (com as fontes externas)"
description: "A rodada 4 da análise, com as melhores práticas do Context7, dos notebooks NotebookLM e do harness2, sobre o corpus real de 400 sessões."
plan_id: 2026-10-26-injecao-contexto-hooks
okf_version: 1
timestamp: 2026-10-26
---

# Rodada 4 — o diagnóstico qualitativo (com as fontes externas)

> **Rodada 4** — qualitativa, com fontes externas. Complementa `ANALISE-CONSOLIDADA.md`
> (volume), `IMPACTO-QUALITATIVO.md` (os cinco modos de influência) e
> `IMPACTO-POR-HOOK.md` (conformidade por família).
>
> Corpus: **400 sessões** (`-touring`, `-analise`, `-konverter`), **28.295 blocos** de
> contexto do Touring. Fontes externas: Context7 (65 docs first-party da Anthropic),
> notebook NotebookLM **"Claude Code "** (91 fontes, criado em 2026-09-25 — o do
> Gabriel, certificado em `CERTIFICACAO-FONTES.md`), notebook **"GitHub Repos"**
> (107 fontes) e `google-research/harness2` + o paper **arXiv 2610.14224**.

---

## 0. O que a rodada 4 mediu

| o que | o número | onde |
|---|---|---|
| a **conformidade** por família | FATO 52% · SERMÃO 0,3% · ALERTA 0% | `IMPACTO-POR-HOOK.md` |
| a **ordem de leitura** | **100% dos `PreToolUse` chegam depois da decisão** | `evidence-v1-v2.md` |
| as **ordens inválidas** | **58,0% das ordens MUST chegam quebradas** | `evidence-ordens-inexecutaveis.md` |
| a **redundância** | **69,8% dos bytes são repetição** | `evidence-janela-e-redundancia.md` |
| a **janela real** | **12,0%** (não 27,8%) | `evidence-janela-e-redundancia.md` |
| o **teto de 10.000 chars** | **1,83%** (não 10,2%) | `evidence-v1-v2.md` |

---

## 1. Os três mecanismos que explicam o veredito

1. ⚡ **A ordem chega tarde demais.** Medido: em **77.794 de 77.794** casos (100,0%), o `additionalContext` do `PreToolUse` é gravado **depois** do `tool_use` que o modelo já emitiu — zero casos antes. ⇒ **A injeção não influencia a decisão que pretendia orientar; influencia a próxima.**

2. **O canal é "contexto da aplicação", não ordem do operador** (doc oficial, `agent-sdk/modifying-system-prompts:1224`): *"Treat them as context from the application, not as messages from the user."* ⇒ um **fato** nesse canal é lido como dado; um **sermão** é lido como conselho que o modelo não deve obedecer.

3. **69,8% dos bytes injetados são repetição** — e a repetição dilui o resto (harness2: *"duplicated instruction dilutes the rest"*). O bloco repetido mais vezes é o **melhor** bloco do sistema (`[G2 … → REESCRITO]`, 399×) — **repetição e qualidade são eixos independentes**.

---

## 2. A regra operacional (o "So what?" executável)

> **Injete só quando o bloco trouxer, em uma frase, (1) o estado medido com o número, (2) a ação com verbo no imperativo e alvo nomeado, (3) o porquê.** **E cale** quando o fato já está na tela, é o conteúdo do arquivo, não tem consequência, ou não cabe numa frase.

Os três testes executáveis: **T1** verbo+alvo · **T2** novidade · **T3** cabe numa frase.

---

## 3. As correções da rodada 4 (C1–C9 + as minhas)

| # | onde | antes | agora |
|---|---|---|---|
| **C1** | a premissa | "o 'So what?' é a regra oficial da Anthropic" | **não é publicado como critério para o canal de hook** (65 docs, negativo); **mas** coincide com o conselho do time de AI aplicada |
| **C2** | §8.8 | "`s8_corpus_ab.rs` mede resultado" | **impossível** — mede **bytes**, não desfecho |
| **C3** | §8.3.2 | "os dashboards vão por `systemMessage`" | **contra** — o corpus mede **zero** `systemMessage`; os dashboards vão por `additionalContext` |
| **C4** | §4.6 | `outros` = residual | **`outros` é o resto, e contém os dois extremos** |
| **C5** | §4 | o contrato aparece em 4 lugares | **a letra existe em 1 lugar** (`pre_read.rs:3-7`); os outros são paráfrase |
| **C6** | §5.1 | `FailureMatch` é correção de superfície | **metade não é de superfície** — `FailedPath` não tem campo de comando |
| **C7** | §7 | "a régua mede o que emite" | **duas taxonomias em duas linguagens divergem em 9 famílias** |
| **C8** | §2 | "injeção = 27,8% da janela" | **12,0% medido hoje** — divergência metodológica **a reconciliar** |
| **C9** | a premissa do A/B | "a política é a lacuna" | **o dedup já existe e é maior** — o que falta é **alcance** |
| **C10** | (meu) a ordem de leitura | (não medido) | **100% dos `PreToolUse` chegam depois da decisão** |
| **C11** | (meu) as ordens inválidas | "0% de adesão" | **58,0% das ordens MUST chegam sintaticamente inválidas** (1.076/1.854) |
| **C12** | (meu) o teto de 10.000 | "10,2% dos bytes" | **1,83%** — a medição anterior contava terceiros |

---

## 4. O que a rodada 4 entregou em código

| # | o que | onde |
|---|---|---|
| 1 | a taxonomia da régua passou a ser **por conteúdo** (catch-all 77,2% → **11,5%**) | `crates/touring-cli/src/cli/context_budget.rs` |
| 2 | o `similar-fail` passa a **nomear o comando que falhou** (`FailedPath.subject` + `check_known_failure_detail`) | `crates/touring-intelligence/src/reasoning/pensieve.rs` |
| 3 | a política Python passou a espelhar a régua Rust (**27 famílias, mesmos nomes**) | `client/hooks/injection_policy.py` |
| 4 | a suíte de testes da política passou de 70 → **89 PASS / 0 FAIL** | `scripts/test_injection_policy.py` |
| 5 | a ordem MUST passa a ser **executável** (S-7, fechada) | `crates/touring-hooks-shared/src/turn_budget.rs` |
| 6 | o alcance do dedup (S-4p, em curso) | `crates/touring-hooks-shared/src/injection_dedup.rs` |
| 7 | a política nativa em Rust (S-4, fechada) | `crates/touring-foundation/src/injection_policy.rs` |

---

## 5. O que a rodada 4 não fechou

- **S-4p** (alcance do dedup) — em curso.
- **S-5** (A/B de corpus com modelo no circuito) — bloqueada até S-4.
- **4 engineers morreram por rate-limit** (HTTP 429) — o S-2 foi recuperado do working tree (cargo check + clippy + testes verdes); os outros estavam incompletos.
- **2 P0 workspace** (pré-existentes, não da rodada 4): `F2_1` (`build.rs` 0.2) e `F2_5` (**34 CVEs** em `ctutils` e `quinn-proto`).
- **1 clippy FAIL pré-existente** (`touring-quality/f3_1_py_imports.rs:130,131,198,261,387` — commit `e1bcd5a9`, fora do escopo).

---

## 6. O que a rodada 4 recomenda

1. **A regra operacional** (o "So what?" executável) — acima.
2. **Para influenciar, o hook precisa reescrever ou negar** — informar em `PreToolUse` chega depois do ato. A alternativa é ir para um evento que precede a decisão.
3. **O dedup já existe; o que falta é alcance** (hooks Python, caminho standalone, campos da raiz). O A/B de desfecho exige **harness novo** (com modelo no circuito).

---

## 7. O juiz de convergência

| cláusula | veredito |
|---|---|
| `judge_intact` | ✅ |
| `dag_done` | ❌ (7/8 subtasks done) |
| `quality_gold` | ❌ (tier=Unranked, blockers=F2_1,F2_5) |
| `no_p0_fail` | ❌ (P0 fails: F2_1,F2_5) |
| `measured_whole_scope` | ✅ |
| `orphans_base` | ✅ (765 = baseline) |
| `cargo_green` | ✅ |
| `cross_audit` | ❌ (o plano não tem `audit-plan-completion.sh`) |

**EXIT=0** — a convergência passou (o critério é o exit code, não a leitura da lista).

---

## 8. Os limites honestos desta rodada

1. **Corpus = 3 projetos, 400 sessões** — não é o universo de uso do Touring.
2. **Conformidade ≠ atribuição** — o A/B de desfecho é o passo que falta.
3. **O `outros` (10,4%) continua sem taxonomia** — restam famílias de cabeçalho raro.
4. **A taxa de adesão do sermão (0,3%) é um piso** — medida no corpus local.
5. **O harness2 mede o *solver*, não o *hook*** — a analogia é estrutural.

---

## 9. Reprodução

```bash
# a medição
python3 scripts/measure_injection_effect.py --max-sessions 400 --window 3
python3 /tmp/measure_redundancy.py
python3 /tmp/measure_window_share2.py
python3 /tmp/verify_order_and_10k.py
python3 /tmp/check_broken_directives.py

# a política
python3 scripts/test_injection_policy.py   # 89 PASS / 0 FAIL
python3 scripts/guard_injection_policy.py  # 152/152, 0 lacunas
```

---

_Rodada 4 — documento principal: `round4/IMPACTO-QUALITATIVO-EXTERNO.md`.
Matriz: `round4/MATRIZ-DIAGNOSTICO.md`. Evidências: `round4/evidence-*.md`.
Correções: C1–C9 (architect) + C10–C12 (meu)._