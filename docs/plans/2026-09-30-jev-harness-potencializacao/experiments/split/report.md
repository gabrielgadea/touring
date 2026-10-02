---
type: ExperimentReport
title: S-2.0 — split congelado do corpus G1 (reconstrução)
description: O split que o gate do G1 prometeu e nunca foi escrito, reconstruído com a seed do corpus e congelado por hash. 8/8 testes.
plan_id: 2026-09-30-jev-harness-potencializacao
tags: [experiment, s-2.0, split, g1, holdout, jev]
subtask: S-2.0
owner: touring-02
timestamp: 2026-09-30T22:52:00-03:00
okf_version: "0.1"
---

# S-2.0 — split congelado do corpus G1

Subtask `task_1790819065619924538::S-2.0`, dona `touring-02`. Classificada no plano
como `--kind decision --fog hazy`: a **pergunta** ("existe o artefato?") foi respondida
por varredura; a **decisão** (70/15/15, estratificar por `weak_label` e não por rótulo)
é registro deste relatório e cabe ao Gabriel se a ratificar.

## Resultado em uma tela

**[F] O split não existia. Agora existe, congelado e testado.**

| | |
|---|---|
| Artefato | `~/.claude/touring/g1-corpus/split.json` — **39.866 B** |
| `split_seed` | **20260922** (a seed do corpus, de `sample_stats.json`) |
| sha256 do split | `deb107937948021a6ce33caffc10ea14b722f6ae72f69b041401f6c9d75bf3f1` |
| Gerador (versionado) | `docs/plans/2026-09-30-jev-harness-potencializacao/experiments/split/make_split.py` |
| Testes (versionados) | `…/experiments/split/test_make_split.py` — **8/8 PASS**, exit 0 |
| Prova de congelamento | regerar do zero com a mesma seed reproduz **byte-idêntico**, verificado **entre processos** |

## A caça (W2.0, já provada antes)

O Apêndice A de `harness-scan-t02.md` documenta a ausência em 5 frentes: nenhuma
chave de partição nos 6 JSONL do corpus; `g1_sample.py` e `g1_label.py` usam só
`str.split`; `g1_review_sheet.py` estratifica por **confiança**, não por item; a
única menção a split no plano 22/09 é a própria linha do gate (`:622`); nada em
`.remember/`, no plano 23/09 ou no git.

**A conclusão não mudou: o split nunca foi escrito.** Não é artefato perdido — o
produtor não produzia. A assinatura do que faltava era essa: `sample_stats.json` guarda
`seed`, `n` e estratos, mas nenhum arquivo guarda partição.

## Partições

| Conjunto | n | Estratos | train | val | test | held-out |
|---|---|---|---|---|---|---|
| `prompts` | 800 | 5 (por projeto) | 560 | 120 | **120** | 15,0% |
| `memories` | 486 | 9 (por `weak_label`) | 339 | 74 | **73** | 15,0% |
| `pairs` | 300 | 3 (por canal) | 210 | 45 | **45** | 15,0% |

Os três conjuntos fecham: 1.586 itens, cada um com exatamente uma parte.

## As três decisões que tomei (e por quê)

### D1 — 70/15/15, e não 80/10/10

**[I] (0,80)** O G3 faz fine-tune do `laya-multilingual` em cima do treino, e R15
registra que **não há relato de treino do Laya numa GPU de 8 GB** — a RTX 4060 desta
máquina é 8 GB. O treino precisa de volume. O outro lado é o gate do G3: "temperatura
por (tipo, nº de opções) no held-out", que precisa de sinal por classe — 15% de 800
prompts dá 120 itens de teste, ~19 por classe de intent, que é apertado mas suficiente
para uma curva de calibração. **[F]** 10% daria 80 itens e ~13 por classe: curto demais
para separar 8 intents numa curva. **O relatório de W1 (S-1.2) é quem decide se
sobe para 80/10/10** — e é bom que decida com os resultados da amostra de tags na mão.

### D2 — estratificar por `weak_label`, nunca pelo rótulo do professor

**[I] (0,95)** O `memories.jsonl` tem duas colunas de classe: `weak_label` (o prefixo
da chave, o "rótulo fraco" que o G1 mediu discordando do professor em **34,6%** dos
casos) e o rótulo do professor, que está no arquivo de labels separado. Particionar
pelo rótulo do professor seria **vazar o alvo para a partição**: o `test` deixaria de
ser cego justamente para a tarefa que o fine-tune vai aprender. `weak_label` é o
rótulo fraco, é o que o corpus já estratificava (`sample_stats.json` mostra 9 classes
com 66 nas maiores e 24 na menor), e ele é **derivado da chave, não do professor**.

Para `prompts` o estrato é `project` e para `pairs` é `channel` — os estratos que o
próprio G1 usou, preservando a distribuição de `analise` 480 / `touring` 229 / `Work` 44
e de `ann` 210 / `tfidf` 89 / `entries` 1.

### D3 — a seed é a do corpus, e é lida de verdade

**[F]** `split_seed = 20260922`, a mesma de `sample_stats.json`. Uma seed nova
produziria um split irreproduzível contra os artefatos que já existem, e o gate pede
reconstituição, não refresh. **[F]** O teste `test_seed_is_load_bearing` prova que a
seed é carregante: `deb107937948021a…` com 20260922, `52cf4c3aaa4a…` com 20260923. Sem
esse teste, "mesma seed reproduz byte-idêntico" seria-trivialmente verdadeiro se o
código ignorasse a seed.

## Como o congelamento é garantido

**[F] Determinismo por construção, não por confiança:** a parte de um item é função
de `(seed, estrato, id ordenado)`. O RNG é semeado por estrato com
`random.Random(f"{seed}:{nome}")` e a lista é ordenada por `id` **antes** do shuffle —
sem essa normalização, dois runs com a mesma seed divergiriam se a ordem do arquivo
mudasse. Nenhum `hash()`, nenhum mtime, nenhum relógio entra.

**[F] Três portas de prova, e uma delas é o que pega o erro real:**
1. `canonical()` — serialização `sort_keys` + separadores fixos: mesmo split → mesmos bytes.
2. `verify_frozen()` — **regera do zero** e compara com o disco. Gravar não basta; tem
   de provar que regerar reproduz o gravado.
3. `test_frozen_across_processes` — roda o `--check` num **interpretador novo**. É a
   que pega `hash()` randomizado por processo, que passaria em qualquer teste de
   mão-dupla dentro do mesmo processo.

**[F] E o erro que ela pegou, para não parecer que passou de primeira:** a primeira
execução gravou o arquivo e **crashou logo depois** — eu tinha renomeado `digest` para
`split_digest` (o hook apontou homônimo com `crate_50dim_matrix.py:112`) e deixei dois
call sites para trás. O `--check` da vez seguinte pegou: `NameError: name 'digest' is
not defined`. Sem a prova entre processos, o artefato de 39.866 B teria ficado em disco
com um gerador quebrado, e o próximo `--check` só descobreria na hora de rodar.

## Testes (8/8, exit 0)

| Teste | O que impede |
|---|---|
| `test_split_is_frozen` | o gate nominal do plano |
| `test_frozen_across_processes` | determinismo dependente de processo (`hash()`) |
| `test_every_item_has_exactly_one_part` | item sem parte, parte órfã, parte inválida |
| `test_labels_reachable_in_test` | rótulo do professor sem item no split (G2 não consegue avaliar) |
| `test_heldout_is_non_trivial` | held-out vazio, de 1 item, ou <5% do set |
| `test_stratification_preserved` | desvio >5 p.p. da fração-alvo por estrato — **medido: pior desvio 0,025, em `memories/decision/test`** |
| `test_seed_is_load_bearing` | seed decorativa |
| `test_artifact_matches_disk` | o que está gravado ≠ o que o código produz |

**[F] `test_labels_reachable_in_test` passou com 0 órfãos** — e isso não é óbvio:
`labels_recall_relevance.jsonl` tem 300 ids e `pairs.jsonl` tem 300, mas **não são os
mesmos ids** (o primeiro de cada arquivo difere). Passou porque todo id de labels que
existe no items correspondente está no split; o relatório do gerador expõe
`labels_without_item` para que a assimetria fique visível em vez de silenciosa.

## Bloco do hook, e por que o obey

**[F] O CEG negou uma escrita via `touring run`** (X6, `write_text` sob perfil
`Sandboxed`) quando eu tentava renomear símbolos num programa. **Não contornei** — o
arquivo é meu e a edição é legítima, mas o caminho certo era o `Edit` da ferramenta
canônica, que passa pelos hooks de reindex e health-delta. O hook também apontou 3
homônimos reais (`read_jsonl` em `TACO-wt/scripts/lib.py:180`, `digest` em
`crate_50dim_matrix.py:112`, `report` em `crates/touring-analysis/src/lib.rs:134`) e
o cascade de call sites foi útil: pegou o `build()` nos dois lugares. **[I] (0,7)** Os
"outros arquivos" que o cascade cita (`governor.rs:371`, `migrate_from_global.rs:135`,
`hook-performance-optimizer/scripts/*`) são **homônimos por nome, não consumidores** —
nenhum deles importa este módulo. **[F]** O alerta de `global` na linha 29 é falso
positivo: é o `if __name__ == "__main__":` do próprio script.

## O que este split NÃO resolve

**[I] (0,90)** Ele fecha o `--kind decision` do S-2.0 e habilita o W2.1 reproduzível,
mas **o G2 ainda tem uma dependência que o split não cumpre**: o plano manda o G2
rodar "keyword atual, Laya zero-shot, professor" (plan.md S-2.1), e o Laya zero-shot
precisa dos pesos do `laya-multilingual` + artefato de fine-tune (R13: projeto de 4
dias, issues abertas, nenhum relato de treino em GPU de 8 GB). **[F]** Essa é a linha
de base com maior risco de não existir, e ela é da Criticalidade do S-2.1, não minha.

**[F]** Nenhuma escrita foi feita em `memory.db`, nos JSONL do corpus, nem em código
fora de `experiments/split/`. Nenhum commit. O `split.json` é o único artefato criado
e ele vive fora do repo, junto do corpus — como a v2 decidiu para os dados do G1.

## Cross-references

- Apêndice A de `harness-scan-t02.md` — a prova de ausência que precedeu esta reconstrução
- `strategy-2026-09-22-jev-system-one.md:622` — a linha do gate que prometeu o split
- `experiments/g1-corpus/report.md:77-79` — os 65,4% de concordância que motivam D2
- `plan.md` S-2.0 (gate) · S-2.1 (G2 baselines) · S-2.2 (decisão de trilha)
