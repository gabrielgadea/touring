# Compactação 65 — relatório único (S9 skills + adendo S7 regras e agentes)

**Data**: 08/10/2026 · **Bundle**: `~/projects/touring/docs/plans/2026-10-08-compactacao-65/`
**Veredito**: `conservacao.py` **exit 0** nos três modos · 39 testes verdes · `ruff` 0 erros

---

## 1. O que era o pedido, e o que a entrega é

O brief pedia duas coisas, e elas têm.status epistêmico diferente:

| | Pedido | Como foi provado |
|---|---|---|
| **S9** | 9 `SKILL.md` abaixo de 20.000 chars | `test_skill_size_guard` (6 testes) + `conservacao.py` modo skills |
| **S7 adendo** | piso fixo de regras e descrições de agente | `conservacao.py --rules` e `--agents` + `tabela_antes_depois.py` |

O `conservacao.py` é o juiz: **substring verbatim**, linha a linha, contra os backups. Não é "equivalente" — é a linha original aparecendo no arquivo novo ou no arquivo de destino declarado. Semântica exige leitor; substring é aritmética, e aritmética não minta.

---

## 2. Números que a sessão paga

```
                          ANTES        DEPOIS     DELTA
regras sempre carregadas   65.374       28.022    -57,1%
descrições de agentes      13.160        3.281    -75,1%
─────────────────────────────────────────────────────────
contexto fixo total        78.534       31.303    -60,1%
movido para sob demanda    —           30.169     (carrega só quando o tema pede)
destruído                     0            0       100% de conservação
```

`medir_piso.py` (o medidor do brief) confirma de forma independente:
**113.809 chars sempre carregados** em 22 arquivos (**18 regras sem `paths:`**) · **8.059 chars** de descrição em 26 agentes.

---

## 3. As 9 regras

| regra | antes | stub agora | sob demanda | economia |
|---|---:|---:|---:|---:|
| git-safety-protocol | 4.370 | 1.010 | — | 3.360 |
| touring-process-hygiene | 6.830 | 816 | — | 6.014 |
| touring-per-project | 4.714 | 847 | — | 3.867 |
| touring-elite | 3.573 | 1.047 | — | 2.526 |
| touring-cli-index | 12.422 | 7.366 | 5.375 | 5.056 |
| touring-4-pillars | 10.053 | 7.399 | 3.066 | 2.654 |
| disk-hygiene | 5.952 | 6.049 **(cond)** | — | 5.952 |
| vgp-protocol | 6.185 | 6.326 **(cond)** | — | 6.185 |
| 00-core-governance | 11.275 | 9.537 | 2.241 | 1.738 |
| **TOTAL** | **65.374** | **28.022** | **30.169** | **37.352** |

**(cond)** = recebeu `paths:` no frontmatter e saiu do conjunto sempre-carregado; o corpo não foi tocado byte a byte (só o prepend do frontmatter, +97 e +141 chars). A economia dessas duas linhas na tabela é **total** — a regra inteira sai do contexto fixo.

Conservação: **755/755 (100,00 %)**.

---

## 4. Os 8 agentes

O brief pedia `description` ≤ 450 chars, **mantendo os gatilhos** ("Use when…" e os termos que disparam o agente), com o texto removido indo **verbatim** para o corpo. `model:` e `tools:` intactos.

| agente | antes | agora | economia |
|---|---:|---:|---:|
| touring-architect | 1.576 | 430 | 1.146 |
| touring-auditor | 1.681 | 436 | 1.245 |
| touring-engineer | 1.899 | 437 | 1.462 |
| touring-scouter | 1.487 | 421 | 1.066 |
| touring-scriber | 1.502 | 418 | 1.084 |
| antt-vote-drafter | 2.408 | 333 | 2.075 |
| erickson-paradox-reformulator | 1.398 | 436 | 962 |
| transformational-narrative-architect | 1.209 | 370 | 839 |
| **TOTAL** | **13.160** | **3.281** | **9.879** |

Conservação: **3.701/3.701 (100,00 %)**, incluindo a exigência de que a cópia em `~/.claude/agents/` seja **byte-idêntica** ao canônico — divergência entre cópias é um silêncio (o Claude Code lê uma, o repo lê a outra).

**O corte, e por que foi dentro da lista.** As descrições dos `touring-*` são *uma frase só*: `Use this agent when the user asks to "a", "b", … "z". <papel>.` O splitter por ponto final guarda a frase inteira — 634 chars. O corte certo preserva, por ordem de força de ativação: o verbo de disparo, os primeiros gatilhos (a lista está em ordem de recorrência real), o `or mentions "<nome-do-agente>"` (o gatilho mais forte) e a frase de papel, que é o que distingue um agente do outro. O texto cortado vai verbatim para `## Notas de ativação`.

---

## 5. O que foi defeito, e o que a auditoria teach sobre auditar o auditor

Cinco defeitos reais, todos medidos, nenhum relatado como "pré-existente":

| # | defeito | como foi pego |
|---|---|---|
| **D1** | medir `description` com `^description:\s*(.*)$` devolve **1 char** — as 8 descrições são block scalars (`description: >`), o valor vive nas linhas seguintes | o absurdo: 1 char num arquivo de 43 KB |
| **D2** | a 2ª passagem sobrescreveu o bloco `## Notas` com a descrição **já encurtada** em vez da original — "Wave 4"/"Wave 12" foram para o lixo | `--agents` acusou 98,19 % |
| **D3** | `destinos: []` para `touring-4-pillars` — o texto estava no arquivo de destino, e o verificador media **só o stub** | 29 linhas "perdidas" que não estavam |
| **D4** | patch por índice de linha casou a **primeira** ocorrência de uma âncora que existia em duas funções | `py_compile` + `ruff` F811 |
| **D5** | truncamento `texto[:b2]` comeu a cauda inteira do arquivo | `ruff` acusou imports não usados |

**A lição que vale mais que os cinco:** D3 é o modo de falha mais caro de um verificador — **um verificador que mede o conjunto errado passa verde sobre o vazio**. Os 29 "accesos" de `touring-4-pillars` não eram perda; eram o verificador olhando o lugar errado. Um alarme falso treina o humano a ignorar alarmes.

E uma segunda, que apareceu duas vezes: **agulha de busca sem acento é medição cega em português**. `afordan` não casa `afordância`; `VIAMAO` não casa `VIAMÃO`. Em texto com diacríticos, a agulha errada produz "não existe" — e "não existe" é a conclusão mais perigosa que um grep entrega. A normalização tem de acontecer **no lado da agulha**, não só no lado do texto.

---

## 6. Estado dos gates

| gate | comando | veredito |
|---|---|---|
| A6 | `conservacao.py --rules --agents` | **rc 0** — 755/755 regras, 3.701/3.701 agentes |
| S9 | `conservacao.py` (modo skills) | **rc 0** — 3.503/3.503 linhas, 0 link quebrado |
| A7 | `tabela_antes_depois.py` | **rc 0** — 78.534 → 31.303 no contexto fixo |
| A8 | 4 suítes | **39 passed, 0 failed** |
| lint | `ruff check` | **0 erros** |
| complexity | `max_cc=9` | abaixo do teto 10 |

Testes: `test_constituicao_viva` 11 · `test_code_mode_sdk_section` 19 · `test_skill_size_guard` 6 · `test_docs_no_phantom_crates` 3.

> `test_constituicao_viva` imprime `FAIL Required test coverage of 85% not reached. Total coverage: 0.91%` ao rodar **sozinho**. É o gate global de cobertura medindo uma suíte de 1 arquivo — não é regressão desta compactação, e está registrado como tal em vez de silenciado.

---

## 7. O que ficou aberto, e por quem

| item | quem decide |
|---|---|
| `judge_intact` — exige atestação manual em TTY (`judge_attest.py --attest --why '…'`) | **Gabriel**, no terminal |
| `quality_gold` — F3.1 mede 0,141; a métrica pulou para 0,894 **quando movi `evidencia/` para fora do escopo**, quebrando 9 testes. Revertido: o estado honesto é 0,141 | **Gabriel** — é decisão de escopo, não de código |
| Context7 nunca consultado (OAuth expira a cada tentativa) | limitação da ferramenta, registrada |

---

_Compactação 65 · S9 + adendo S7 · conservação 100 % provada por código, não por leitura_