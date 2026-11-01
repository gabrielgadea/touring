---
type: Analysis
title: "Impacto por hook — cada conteúdo injetado gera o efeito esperado? E de que forma?"
description: "Rodada 3/4: conformidade por família, forma do efeito, e o confronto entre o contrato declarado pelo próprio harness e a prática medida."
plan_id: 2026-10-26-injecao-contexto-hooks
okf_version: 1
timestamp: 2026-10-26
---

# Impacto por hook — o conteúdo injetado produz o efeito esperado?

> **Rodada 3/4 da análise.** Complementa `ANALISE-CONSOLIDADA.md` (volume, régua,
> interruptor) e `IMPACTO-QUALITATIVO.md` (os cinco modos de influência).
> Este documento responde à pergunta que faltava: **para CADA conteúdo injetado por
> CADA hook, o modelo fez o que se esperava — e de que forma?**
>
> Corpus: **400 sessões** (`-home-gabrielgadea-projects-touring`,
> `-analise`, `-konverter`), **28.295 blocos de contexto do Touring**.
> Instrumento: `scripts/measure_injection_effect.py` (reescrito nesta rodada).

---

## 1. O que esta rodada corrigiu — 4 defeitos de instrumento

A rodada 2 mediu o comportamento por *adesão proximal*. Ao reexaminar o
instrumento contra o corpus real, quatro defeitos apareceram. **Todos quatro
inflavam a confiança da leitura** — três deles escondiam injeção que existe.

| # | defeito | sintoma | correção |
|---|---|---|---|
| D1 | **A régua classifica por cabeçalho.** O caminho de *orçamento de turno esgotado* emite o payload **sem cabeçalho** (`pre_bash.rs` / `cli_suggester`) | **5.630 sermões caíam em "outros"** — o maior bucket, invisível | taxonomia própria, por **conteúdo**, com regra explícita para o payload sem cabeçalho |
| D2 | **O SPEC de `evidence-guard` e `post-tool-echo` estava errado.** Assumia "o bloco traz a falha anterior" | ambos 100% **INDETERMINADO** — não por ausência de efeito, mas por **modelo de sinal errado** | sinais novos: `leu_stderr` e `rodou_orphans` |
| D3 | **O sinal de `code-mode-deny` media a coisa errada.** "O modelo rodou o comando sugerido" — mas o gate **reescreve** a chamada | 0% "sem efeito" era artefato | sinal `fundiu_programa`: a chamada seguinte virou `touring run` |
| D4 | **O instrumento não filtrava `is_touring`** — media hooks de terceiros junto | o bucket "outros"inchava com 2.171 blocos alheios | filtro explícito: só hooks do Touring |

**D1 é o mais grave**: a régua (§7 do documento principal) não via **19% do
contexto do Touring**. E o invisível era exatamente o sermão — a categoria
que se supunha rara.

### 1.1 A prova de D1

O payload sem cabeçalho, literal:

```
  MUST    touring run --lang bash --code 'mkdir -p /tmp/c3-inv && cat > ...'
  (orçamento do turno esgotado — só as diretivas; `TOURING_TURN_BUDGET=<bytes>` ajusta o teto)
```

A régua casaria `[TOURING SUGGEST` — que não está aí. Resultado: **o sermão
real é 4.647 blocos; a régua media 906.** O mesmo hook (`touring cli-suggest:
classifying operation (Rust, in-daemon)...`) emite as duas formas.

---

## 2. A tabela — conformidade por família (400 sessões, 28.295 blocos)

| família | n | % do Touring | **CONFORME** | **CONTRÁRIO** | forma dominante |
|---|---:|---:|---:|---:|---|
| `pre-edit` | 6.636 | 23,4% | **75,6%** | 0,0% | `usou-o-fato-depois` (4.960) |
| `touring-suggest` | 4.647 | 16,4% | **0,0%** | 0,0% | `nenhum` (4.647) |
| `post-edit` | 3.060 | 10,8% | 0,8% | 0,0% | `nenhum` (3.037) |
| `outros` (não classificado) | 2.953 | 10,4% | 15,7% | 0,0% | `usou-o-fato-depois` (463) |
| `evidence-guard` | 2.148 | 7,6% | 0,0% | 0,0% | `nenhum` |
| `code-mode-contract` | 1.840 | 6,5% | 0,0% | 0,0% | `nenhum` |
| `similar-fail` | 1.676 | 5,9% | **INDETERMINADO (100%)** | — | `nenhum` |
| `generic-noise` (`[generic]`) | 1.499 | 5,3% | — (não é contexto) | — | `nenhum` |
| `post-tool-echo` | 1.332 | 4,7% | 0,0% | 0,0% | `nenhum` |
| `code-mode-deny` | 970 | 3,4% | 0,0%¹ | 0,0% | `nenhum` |
| `plugin-hook` | 578 | 2,0% | 2,9% | 0,0% | `nenhum` (561) |
| `pipefail-evidence` | 530 | 1,9% | 0,0% | 0,0% | `nenhum` |
| `compact-warning` | 255 | 0,9% | **46,3%** | 0,0% | `gravou-estado` (118) |
| `pre-edit-prevention` | 86 | 0,3% | **98,8%** | 0,0% | `usou-o-fato-depois` (85) |
| `session-contract` | 71 | 0,3% | 0,0% | 0,0% | `nenhum` |
| `pre-read` | 14 | 0,05% | 7,1% | 0,0% | `nenhum` |

¹ `code-mode-deny` mede "a chamada seguinte virou `touring run`" dentro da
janela de 3 chamadas. O efeito real do gate é a **reescrita da própria
chamada** — que o transcript não expõe como tool_use. Ver §6.

### 2.1 O agrupamento que decide tudo

| classe | famílias | n | % | **CONFORME agregado** |
|---|---|---:|---:|---:|
| **FATO** (o modelo não encontraria sozinho) | `pre-edit`, `post-edit`, `pre-edit-prevention`, `pre-read`, `compact-warning` | 10.051 | 35,5% | **≈ 52%** |
| **SERMÃO** (o modelo sabe e é instruído) | `touring-suggest`, `plugin-hook`, `session-contract` | 5.296 | 18,7% | **≈ 0,3%** |
| **DIRETIVA** (programa pronto, sem contexto) | `code-mode-contract` | 1.840 | 6,5% | 0,0% |
| **ALERTA** | `evidence-guard`, `similar-fail`, `pipefail-evidence`, `post-tool-echo` | 5.686 | 20,1% | 0,0% |
| **RUÍDO** | `generic-noise` | 1.499 | 5,3% | — |
| **COERÇÃO** (reescreve a chamada) | `code-mode-deny` | 970 | 3,4% | por construção (§4.2 do principal) |
| não classificado | `outros` | 2.953 | 10,4% | 15,7% |

> **A correlação é quase perfeita:** *fato* → 52% · *sermão* → 0,3% ·
> *alerta sem ação concreta* → 0%. A variável que decide não é o volume nem
> a prominência do bloco: é **o conteúdo carregar um fato que o modelo não
> tinha** — ou **não carregar nada**.

---

## 3. O mapa hook a hook — efeito esperado × efeito medido × forma

| hook (evento) | conteúdo | **o que se esperava** | **o que aconteceu** | **forma** |
|---|---|---|---|---|
| `touring-hook pre-edit` (PreToolUse:Edit) | blast radius, chamadores, hot spots, `related docs` | o modelo considera quem depende do arquivo | **75,6% conforma** | `usou-o-fato-depois` — **distal**: a sessão inteira depois toca o caminho nomeado |
| `touring-hook pre-edit-prevention` (PreToolUse:Edit) | "Pre-edit prevention: <defeito>" | o modelo volta no defeito antes de editar | **98,8% conforma** | `usou-o-fato-depois` |
| `touring-hook` (SessionStart / compact) | "Contexto em N% — grave o estado em disco (RETOMADA…)" | o modelo grava o estado | **46,3% conforma** | `gravou-estado` |
| `touring-hook pre-bash` (PreToolUse:Bash) | "Similar command failed recently (similarity=68%)" | o modelo evita o caminho que já falhou | **INDETERMINADO — e o aviso não nomeia alvo** (§5) | `nenhum` |
| `touring-hook post-bash` (PostToolUse:Bash) | "`python3` exited 0, mas a saída mostra erros após um pipe sem `set -o pipefail`" | o modelo trata o exit code mascarado | **0%** | `nenhum` |
| `evidence_guard.py` (PostToolUse:Bash) | "⚠ EVIDENCE GUARD · DEGRADED · … → leia o stderr e o exit code" | o modelo lê stderr/exit code | **0%** | `nenhum` |
| `touring-hook post-tool-batch` | "post-tool-batch: N arquivos … MUST touring wiring orphans -j" | o modelo roda o lint de órfãos | **0%** | `nenhum` |
| `touring cli-suggest` (PreToolUse) | `[TOURING SUGGEST · …] MUST touring ast meta …` | o modelo roda o comando sugerido | **0%** (2,1% **cita** sem rodar) | `nenhum` |
| `touring cli-suggest` sob orçamento esgotado | `MUST touring run --lang bash --code '…'` (sem cabeçalho) | o modelo executa o programa fundido | **0%** | `nenhum` |
| `touring-hook` (PreToolUse:Bash) | `[CODE MODE · rajada] … rode o lote de uma vez` | o modelo troca a rajada pelo programa | **0%** na janela de 3 chamadas | `nenhum` |
| `touring-hook` (PostToolUse:Edit) | "post-edit verification: …" | o modelo verifica | **0,8%** | `nenhum` |
| `painel_emanacao` / `skill_picker` / `delegation_nudge` | dashboards de pendência | o modelo toca alguma pendência | **2,9%** | `nenhum` |
| `touring-hook` (PostToolUse:Bash) | `[generic]` | — | — | **ruído puro** |

### 3.1 A forma, em quatro verbos

| forma | significado | onde aparece |
|---|---|---|
| `usou-o-fato-depois` | o bloco nombrou algo; a sessão **mais tarde** foi lá | 5.529 casos (fato) |
| `internalizou-e-agir` | o modelo **citou** o conteúdo **e** a ação mudou | 56 casos (1,1% do `pre-edit`) |
| `internalizou-sem-agir` | o modelo **citou** e **não** agiu | 98 casos (`touring-suggest`) |
| `nenhum` | nada observável | 22.710 |

**A "internalização" é rara e quase sempre distal.** O modelo raramente
*reasons* sobre o conteúdo injetado (1,1%); ele **age em cima dele** muito
mais (75,6%) — o fato entra na decisão sem passar pelo texto. Isso **invalida
instrumentos de adesão proximal**, inclusive o `injection_follow_ratio` da
régua (§7 do principal): o conteúdo que funciona é justamente o que o
instrumento marcaria como "não seguido".

---

## 4. O contrato do próprio harness, e a prática medida

O Touring **declara** o critério de qualidade do contexto em código:

`crates/touring-hook-handlers/src/hooks/pre_read.rs:1-7`

```rust
//! Pre-Read Hook — HIGH-SIGNAL-ONLY context injection.
//!
//! Philosophy: SILENCE IS THE DEFAULT.
//! Only inject context when it passes the "So what?" test:
//! - Is it actionable? (Claude can DO something differently)
//! - Is it non-derivable? (Claude WON'T find this by reading the file)
//! - Is it specific to THIS file? (Not generic information)
```

E o invariante tem **teste**, em `pre_read_tests.rs:1220-1241`
(`audit_purpose_silence_is_default_invariant` — "core invariant"):
*um arquivo só com metadados DEVE ser silencioso*.

O mesmo contrato aparece em `pre_bash.rs:3`, `pre_edit.rs:582`,
`instructions_loaded.rs:17` e `pre_edit_prevention.rs:309`.

### 4.1 As quatro violações medidas

| teste do "So what?" | família que o viola | n | % do contexto do Touring |
|---|---|---:|---:|
| **"é acionável?"** | `similar-fail` — alerta sem alvo nomeado | 1.676 | 5,9% |
| **"é acionável?"** | `code-mode-contract` — programa pronto que ninguém executa | 1.840 | 6,5% |
| **"não é genérico?"** | `generic-noise` — o payload é literalmente `[generic]` | 1.499 | 5,3% |
| **"é acionável?"** | `touring-suggest` — sermão com 0% de adesão proximal | 4.647 | 16,4% |

> **O harness viola o próprio invariante em ~34% do contexto que injeta.**
> E o invariante está **testado** — o teste cobre o caminho `pre-read`
> (14 blocos no corpus), mas não a política **global** de silêncio.

### 4.2 O `[generic]` — o bloco que não é nada

1.499 blocos (5,3%) cujo conteúdo completo é a string `[generic]`. Não há
proposição, não há caminho, não há símbolo: **a régua não teria como
classificá-los, e o modelo não tem o que fazer com eles**. É o menor
denominador possível de contexto injetado — e ele existe porque o hook
respondeu.

---

## 5. O defeito do `similar-fail` — um alerta que não nomeia o alvo

**1.676 blocos (5,9%)** do contexto do Touring são este aviso:

```
Similar command failed recently (similarity=68%)
```

O modelo recebe um **percentual** e **nada mais**: qual comando falhou, qual
foi o erro, o que fazer. A advertência é **inacionável por construção**.

### 5.1 A cadeia de código — o dado existe e não chega ao modelo

| # | sítio | fato |
|---|---|---|
| 1 | `post_bash.rs:199-206` | **produtor**: se o comando falhou, grava `command_to_states(&outcome.command)` + `reason = outcome.error_pattern` no Pensieve |
| 2 | `pensieve.rs:26-33` | o store guarda `FailedPath { state_hash, reason: String, depth, timestamp, hit_count }` — **`reason` é texto legível** |
| 3 | `pensieve.rs:117,139` | **a API descarta**: `check_known_failure` → `Option<f64>` (só o score); `check_known_failure_seq` → idem |
| 4 | `pre_bash.rs:236` | **o consumidor**: `format!("Similar command failed recently (similarity={:.0}%)", sim * 100.0)` — só o `f64` |

**O defeito é de interface, não de dados.** A API de consulta devolve o
*quão* parecido, e nunca o *o quê*. A correção é de superfície:

```rust
// hoje: devolve o score
pub fn check_known_failure(&self, state: u64) -> Option<f64>
// o que a mensagem precisa: o motivo guardado + o comando que falhou
pub fn check_known_failure_detail(&self, state: u64) -> Option<FailureMatch>
// FailureMatch { similarity: f64, reason: String, command: String }
```

> **A mesma falha estrutural aparece em `pre_edit.rs:436`** ("⚠ pensieve:
> similar path had recorded failures (N% match)") e em
> `pretask.rs:326` ("⚠ pensieve: similar task failed before (penalty=…)"):
> três consumidores, uma mesma API muda.

**Custo medido:** 1.676 blocos por 400 sessões — **5,9% de todo o contexto
injetado pelo Touring é um aviso que o modelo não pode obedecer.**

---

## 6. `code-mode-deny` — o único caso em que o instrumento não dá conta

O gate de rajada **reescreve** a chamada: o payload entrega o programa fundido
pronto (`touring run --lang bash --code '…'`) e o Claude Code o executa. O
efeito, portanto, **não aparece como uma tool_use seguinte** — ele é a
própria chamada, reescrita. O transcript expõe a chamada original e o texto
do hook, mas não a chamada reescrita.

Por isso: 0% de `fundiu_programa` **não significa** "o gate não age". A
rodada 1 mediu o gate pela telemetria de produção: **13 negações, 1,000 por
construção, 581,9 KB elidos** (§4.2 do principal). **O gate funciona; o
instrumento é que não o alcança.** Registrado como **limitação**, não como
resultado.

O mesmo vale para `code-mode-contract` (1.840 blocos): o programa **está**
pronto no payload, mas o modelo não o executou em nenhum caso medido.

---

## 7. O que a medição **não** prova (limites honestos)

1. **Conformidade ≠ atribuição.** A medição mostra que o efeito declarado
   *aconteceu*. Não mostra que *aconteceu por causa* da injeção — seria
   possível o modelo fazer isso de qualquer jeito. **Atribuição exige o A/B de
   corpus** (`crates/touring-hooks-shared/tests/s8_corpus_ab.rs`), que ainda
   não existe com os três braços da política.
2. **A janela proximal é 3 chamadas.** Efeitos distais são medidos contra a
   sessão inteira; efeitos proximais (o gate) contra 3 chamadas.
3. **`similar-fail` é INDETERMINADO por construção** (§5) — não por ausência
   de efeito, mas por ausência de alvo. É o único "número" que não é número.
4. **`outros` (10,4%) continua sem taxonomia** — restam famílias de
   cabeçalho raro; a próxima rodada deve fechá-las.
5. **Corpus = 3 projetos, 400 sessões.** Não é representativo do universo
   de uso do Touring; a taxa de adesão do sermão (0%) é um piso, não uma
   média.

---

## 8. O que muda, na prática

| ação | base | efeito esperado |
|---|---|---|
| **Cortar o sermão** (`no-advisory`) | 5.296 blocos (18,7%) com ~0,3% de adesão | −18,7% de contexto, ~0 perda |
| **Cortar `similar-fail`** até a API devolver o motivo | 1.676 blocos (5,9%) | −5,9% de contexto inacionável |
| **Cortar `[generic]`** | 1.499 blocos (5,3%) | −5,3% de ruído puro |
| **Manter o fato** (`pre-edit`, `pre-edit-prevention`) | 9.796 blocos (34,6%) com 52% de conformidade | é o que produz efeito |
| **Investigar `code-mode-contract`** | 1.840 blocos (6,5%) com 0% | ou executa, ou não injeta |
| **Manter o gate** (`code-mode-deny`) | 970 blocos + 13 negações | funciona por coerção |

**A regra que emerge:** *o Touring deve injetar **fatos** (o que o modelo não
encontraria sozinho) e **ações prontas** (que o modelo executa). Deve injetar
**avisos com alvo nomeado**, nunca avisos com percentual. E deve ficar
silencioso quando não sabe dizer algo acionável — que é exatamente o que o
`pre_read.rs:1-7` já manda fazer, e o que 34% do volume não cumpre.*

---

## 8.1 O interruptor também estava errado — corrigido nesta rodada

Medir o interruptor contra o corpus (400 sessões, 14,6 MB de contexto do
Touring) revelou que **o `no-advisory` cortava só 13,6%** — e o desenho
prometia cortar o sermão, os dashboards e o log de depuração.

**A causa era a mesma taxonomia por cabeçalho.** O caminho de *orçamento de
turno esgotado* emite o sermão **sem cabeçalho**; a `families()` não encontrava
marcador ⇒ payload "desconhecido" ⇒ e a assimetria conservadora de
`no-advisory` (não descartar o que não se reconhece) ⇒ **passava**.

| | antes | **depois** |
|---|---:|---:|
| bytes de contexto do Touring no corpus | 14.408.935 | 14.644.672 |
| **cortados por `no-advisory`** | **1.958.064 (13,6%)** | **2.888.894 (19,7%)** |
| preservados | 12.450.871 (86,4%) | 11.755.778 (80,3%) |

Famílias acrescentadas à taxonomia (`client/hooks/injection_policy.py`):

| família | classe | por quê |
|---|---|---|
| `similar-fail` | **Advisory** | alerta sem alvo: inacionável (§5) |
| `generic-noise` | **Advisory** | payload = a palavra `[generic]` |
| `delegation-nudge` | **Advisory** | variante `[M3 delegação]` do nudge (2,9% de impacto) |
| `code-mode-contract` | Fact | programa fundido pronto |
| `session-contract` | Fact | o bloco `MUST touring doctor -j` |
| `compaction-gaps` | Fact | `[COMPACTAÇÃO · lacunas do resumo]` — 378 KB, o maior bloco isolado |
| `compact-warning` | Fact | **46,3% de conformidade** — funciona |
| `pipefail-evidence` | Fact | o exit code mascarado |
| `failure-echo` | Fact | `⚡ last fail:` — nomeia o comando e o erro |
| `quality-gate` | Fact | os avisos `[E<n>]` |

**Verificação:** `scripts/test_injection_policy.py` → **70/70 PASS** (24
asserções novas na rodada 3/4) · `scripts/guard_injection_policy.py` →
**152/152, 0 lacunas, 0 nomes obsoletos**.

> **O que `no-advisory` corta hoje são exatamente as três classes de efeito
> medido zero** — sermão (0,0% de adesão), alerta sem alvo e ruído. **O que
> fica é o canal de fato**, que é o único com 52% de conformidade.

---

## 9. Correções ao documento principal (obrigatórias — drift)

Os números das rodadas 1–2 **permanecem válidos** para o que mediram; os
**seguintes são superados** por esta rodada:

| # | onde | antes | **agora** | por quê |
|---|---|---|---|---|
| C1 | §4.1, §4.6, §10 | "sermão = 906 blocos / 92→7" | **4.647 blocos; adesão proximal 0,0%; 2,1% cita sem agir** | D1: o caminho sem cabeçalho não era contado |
| C2 | §4.6 | `pre-edit` 58,5% conforme | **75,6%** | sinal `distal_tocou` corrigido + filtro de terceiros |
| C3 | §10 | "persuasão 1,3% de adesão" | **≈ 0,3% agregado; 16,4% do contexto** | o denominador era o sermão visível, não o real |
| C4 | §7 (régua) | "a régua mede o que emite" | **a régua não vê 19% do contexto do Touring** (§1, D1) | defeito de taxonomia, não só de métrica |
| C5 | §10 | "nenhum contador de adesão em produção (fato)" | mantido — e agora sabemos que o **fato é a única classe que funciona (52%)** | a lacuna de telemetria custa justamente a medição da parte boa |

---

## 10. Reprodução

```bash
python3 scripts/measure_injection_effect.py --max-sessions 400 --window 3
# → tabela de conformidade por família, forma do efeito e o que não dá para medir

python3 scripts/measure_injection_impact.py --max-sessions 400
# → volume por hook e por evento (bytes, blocos, adesão/impacto proximal)

# o defeito do similar-fail, por código:
grep -n "Similar command failed recently" crates/touring-hook-handlers/src/hooks/pre_bash.rs
grep -n "fn check_known_failure" crates/touring-intelligence/src/reasoning/pensieve.rs
```

Evidência bruta: `evidence/conformidade-por-familia-r3.txt` ·
`evidence/injectors-corpus.txt`.

---

_Análise por TACO sob ordem de Gabriel Gadea, 26/10/2026 — rodada 3/4.
Instrumento reescrito nesta rodada: `scripts/measure_injection_effect.py`._
