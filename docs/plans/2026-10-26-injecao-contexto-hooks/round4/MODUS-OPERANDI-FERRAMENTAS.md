---
type: Analysis
title: "Modus operandi das ferramentas — eficácia, eficiência e efetividade (medido no corpus)"
description: "Para que cada ferramenta é chamada, o que os resultados apresentam, e o trio eficácia/eficiência/efetividade por ferramenta e por classe de Bash."
plan_id: 2026-10-26-injecao-contexto-hooks
okf_version: 1
timestamp: 2026-10-26
---

# Modus operandi das ferramentas — o trio, medido

> ⚠ **CORRIGIDO EM 27/10/2026 — as tabelas de trio abaixo estão RETRATADAS.**
> Eram, em boa parte, artefato do instrumento (classe Bash pelo 1º token, retry
> por alvo, negação contada como erro; o `/tmp/measure_tool_trio.py` sumiu). Os
> números corretos vêm de `scripts/measure_tool_trio.py` (versionado, fixture de 9
> casos). Revisão: `../../2026-10-26-touring-janela-eficacia/REVISAO-2026-10-27.md`.
>
> | medida (200 sessões recentes, 27/10) | valor |
> |---|---|
> | chamadas | 12.791 · resultado 23,0 MB |
> | retry (chamada byte-idêntica, janela 8 = predicado do G6) | **1,5%** |
> | negado por hook | **4,2%** — Edit 9,6% · Write 8,8% · `bash` 10,5% · `cp` 10,4% |
> | Edit retry | **4,3%** (não 61,5%) |
> | `cat` | `cat-write` 330 (heredoc; vazio = sucesso), 94,0% · `cat-read` 78, 94,9% |
> | `grep` | eficácia **91,0%** (exit 1 = sem match, não erro) |
> | `cd`/`export` | prefixos de comando composto (2.246/2.246 `cd`) — não são classe |
> | `ctx_execute` 95,1% | artefato: o alvo (`code`) ficou fora do `cmd_norm` |
> | maior canal de resultado | **Read 39,6%** (9,1 MB; 4,07 MB só texto) · `touring run` 15,4% · `python3` 12,0% |



> **Pergunta de Gabriel (26/10/2026)**: *"é preciso saber para que cada ferramenta
> está sendo chamada e o que os resultados estão apresentando, a efetividade,
> eficiência e eficácia desse modus operandi. existe alguma forma mais inteligente
> de fazer isso?"*
>
> Corpus: **200 sessões** (`-touring`, `-analise`, `-konverter`).
> Instrumento: `/tmp/measure_tool_trio.py` (com a correção do `cmd_norm` — a
> primeira medição inflava o retry das ferramentas não-Bash; documentada em §5).

## 1. As definições (as que Gabriel usou)

| métrica | o que mede | como é medida aqui |
|---|---|---|
| **eficácia** | a chamada respondeu algo útil | 100% − (erro% + vazio%) |
| **eficiência** | o custo por chamada útil | retry% (a mesma chamada re-feita na janela de 8) |
| **efetividade** | a chamada mudou a trajetória | proxy: a chamada seguinte não é a mesma (o modelo não repete) — medida pelo retry |

## 2. Por ferramenta (trio)

| ferramenta | calls | erro% | vazio% | retry% | **eficácia%** |
|---|---:|---:|---:|---:|---:|
| Bash | 6.402 | 6,2 | 7,6 | 14,4 | 86,2 |
| Edit | 1.830 | 10,8 | 0,0 | **61,5** | 89,2 |
| Read | 999 | 5,0 | 0,3 | 20,8 | 94,7 |
| Write | 628 | 4,6 | 0,0 | 15,8 | 95,4 |
| SendMessage | 534 | 0,4 | 0,0 | 28,5 | 99,6 |
| `touring_ctx_execute` | 142 | 7,0 | 0,0 | **95,1** | 93,0 |
| Grep | 128 | 1,6 | 1,6 | 17,2 | 96,9 |
| StructuredOutput | 99 | 4,0 | **96,0** | 4,0 | 0,0 |
| Monitor | 84 | 0,0 | 0,0 | 1,2 | 100,0 |
| Agent | 80 | 0,0 | 0,0 | 38,8 | 100,0 |
| ListAgents | 45 | 0,0 | 0,0 | 15,6 | 100,0 |
| ToolSearch | 39 | 0,0 | 0,0 | 7,7 | 100,0 |

**Os outliers**:

- **`touring_ctx_execute`**: **95,1% de retry** — o mesmo programa é re-executado.
  A escada de trust (`--harvest`) existe para isso; o corpus mostra que ela não
  está sendo usada.
- **Edit**: **61,5% de retry** — o mesmo arquivo é editado de novo dentro de 8
  chamadas. O gate G3 (edit-sem-read) existe; o corpus mostra que o modelo edita
  sem ler, erra, e edita de novo.
- **StructuredOutput**: **96% vazio** — o resultado é um marcador, não conteúdo.
  Não é desperdício; é o formato.
- **SendMessage**: 28,5% de retry — a mesma mensagem para o mesmo destino.

## 3. Bash por classe (trio)

| classe | calls | erro% | vazio% | retry% | **eficácia%** |
|---|---:|---:|---:|---:|---:|
| `cd` | 1.679 | 7,4 | 5,6 | 16,8 | 87,0 |
| `touring run` | 648 | 5,9 | 3,7 | 16,0 | 90,4 |
| `export` | 465 | 3,2 | 1,1 | **39,6** | 95,7 |
| `TOURING_CODE_MODE=native` | 441 | 4,1 | 5,4 | 20,4 | 90,5 |
| `python3` | 329 | 8,5 | 8,2 | 8,8 | 83,3 |
| `timeout` | 285 | 5,6 | 3,2 | 15,8 | 91,2 |
| `grep` | 235 | **11,9** | **10,2** | 1,3 | **77,9** |
| `cat` | 165 | 4,2 | **46,1** | 16,4 | **49,7** |
| `echo` | 150 | 5,3 | 4,0 | 4,0 | 90,7 |
| `git` | 115 | 1,7 | 3,5 | 0,9 | 94,8 |
| `ls` | 98 | 8,2 | 8,2 | 4,1 | 83,7 |
| `set` | 94 | **12,8** | 5,3 | 10,6 | 81,9 |
| `touring decompose` | 85 | 3,5 | 11,8 | 3,5 | 84,7 |
| `sed` | 82 | 9,8 | 2,4 | 1,2 | 87,8 |
| `tail` | 82 | 2,4 | 12,2 | 7,3 | 85,4 |
| `hw-slot` | 73 | 0,0 | 6,8 | 5,5 | 93,2 |
| `sleep` | 47 | **36,2** | 6,4 | 0,0 | **57,4** |
| `cargo` | 57 | 0,0 | 10,5 | 3,5 | 89,5 |
| `for` | 54 | 1,9 | 13,0 | 11,1 | 85,2 |
| `ruff` | 48 | 2,1 | 2,1 | 8,3 | 95,8 |

**Os outliers**:

- **`cat`**: **46,1% vazio** — quase metade dos `cat` retorna quase nada. O
  `touring ast overview` (estrutura, não conteúdo) é a alternativa.
- **`grep`**: 11,9% erro + 10,2% vazio — o `grep` falha ou não acha. O
  `touring tantivy search` é a alternativa.
- **`sleep`**: 36,2% erro — o `sleep` em foreground é bloqueado por design; o
  modelo continua tentando.
- **`export`**: 39,6% retry — o modelo exporta a mesma variável de novo.
- **`cd`**: 16,8% retry — o modelo muda de diretório de novo.

## 4. O que os resultados apresentam

| resultado | o que é |
|---|---|
| **erro** | o comando falhou (exit ≠ 0) |
| **vazio** | o comando retornou quase nada (< 50 bytes, ou `[]`/`{}`/`null`) |
| **grande** | o comando retornou > 10.000 bytes |
| **persistido** | o comando foi grande demais e foi para disco ("Full output saved to") |

## 5. A correção do instrumento (a lição)

A primeira medição inflava o retry das ferramentas não-Bash: o `cmd_norm` era o
**nome** da ferramenta (todo Edit = "Edit"), então **qualquer** Edit na janela
contava como retry. A correção: o `cmd_norm` é a **ferramenta + o alvo** (o
arquivo, o destino, o padrão). Os números mudaram:

| ferramenta | retry antes | **retry depois** |
|---|---:|---:|
| Edit | 83,3% | **61,5%** |
| Read | 58,3% | **20,8%** |
| Grep | 57,8% | **17,2%** |
| Write | 49,0% | **15,8%** |
| SendMessage | 53,4% | **28,5%** |
| Monitor | 34,5% | **1,2%** |
| `touring_ctx_execute` | 95,1% | **95,1%** (inalterado — o programa é o mesmo) |

**A lição**: o instrumento mede o que mede; a medição é a medida.

## 6. Existe alguma forma mais inteligente?

**Sim.** O Touring **já mede** isso — e o corpus prova que a medição não está
sendo usada:

| o que | onde está | o que mede |
|---|---|---|
| **retry** | o gate G6 (`G6 redundant-exact-call`) | a mesma chamada re-feita |
| **re-inspeção** | o gate G7 (`G7 re-inspeção`) | a mesma leitura re-feita |
| **rajada** | o gate G1 (`G1 rajada`) | a mesma classe de chamada em rajada |

**A forma mais inteligente é medir na emissão, não no corpus.** O `touring
gate-metrics -j` já conta os retries; o que falta é **usar** a contagem para
decidir.

### 6.1 As formas inteligentes (as fontes)

| fonte | a forma | o que mede |
|---|---|---|
| **rtk** (rtk-ai/rtk) | **só falhas** — `cargo test`/`pytest` viram contagem | 60-90% de redução |
| **prime-agent** | **contexto como variáveis**, ferramentas como **código** | o harness RLM |
| **harness2** | **evidência como arquivos, prompt como ponteiro** | a ablação |
| **context-mode** | o hook programático | 98% de contexto economizado |
| **fast-jev** | **evicção** — o que já cumpriu o papel sai | a compactação |

### 6.2 O que fazer

1. **Medir na emissão** — o `gate-metrics` já conta os retries; wire o trio
   (eficácia, eficiência, efetividade) por ferramenta e por classe.
2. **Matar o desperdício conhecido**:
   - `cat` 46% vazio → `touring ast overview` (estrutura, não conteúdo);
   - `grep` 22% erro+vazio → `touring tantivy search`;
   - `sleep` 36% erro → o `Monitor` (background);
   - `export` 40% retry → a variável já está no ambiente;
   - `cd` 17% retry → o harness persiste o cwd;
   - `Edit` 61,5% retry → o gate G3 (edit-sem-read) existe; o corpus mostra que
     o modelo edita sem ler, erra, e edita de novo;
   - `ctx_execute` 95,1% retry → a escada de trust (`--harvest`) existe; o corpus
     mostra que ela não está sendo usada.
3. **A evicção** — o que já cumpriu o papel sai (fast-jev).

---

## 7. Reprodução

```bash
python3 scripts/measure_tool_trio.py   # versionado (27/10); o /tmp/ original sumiu
```

---

_Modus operandi das ferramentas — o trio, medido no corpus de 200 sessões._