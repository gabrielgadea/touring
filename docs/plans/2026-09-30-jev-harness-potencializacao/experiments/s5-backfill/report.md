---
type: ExperimentReport
title: S-5.2 — prévia do backfill de tags (zero escritas): 2.878 memórias mudariam
description: O pacote de evidência que o S-5.7 precisa. 300 chamadas reais, τ=0,8 do contrato do G2, 107 mudariam (71 acima do corte). A transição dominante lesson→outcome está CORRETA e a tag atual é que está errada. Nenhuma escrita: `TagSource::Jev` não existe.
plan_id: 2026-09-30-jev-harness-potencializacao
tags: [experiment, s-5.2, w5, backfill, tags, pre-evidencia]
subtask: S-5.2
owner: touring-02
timestamp: 2026-10-01T11:35:00-03:00
okf_version: "0.1"
---

# S-5.2 — prévia do backfill (relatório para o Gabriel decidir)

**Zero escritas.** `writes_performed: 0`, e não por disciplina: a marca de origem
que o plano exige (`derived_by=jev-1.13.0`) **não existe e não é criável pela
CLI**. Este relatório mede o ganho para que a decisão do `TagSource::Jev` venha
com o número na mão, e não depois.

- **Script**: `scripts/jev_tags_backfill.py` · F2.1 **1.000 (Diamond)**
- **Dados**: `experiments/s5-backfill/preview.json` (300 linhas completas)

## O resultado

| | |
|---|---|
| Chamadas | **300/300 responderam** (`unmeasured = 0`) |
| τ | **0,80**, lido de `g2-baselines/results.json.threshold_recommended.memory_kind` |
| Razão do τ | `faixa [0.8, 1.0]: acc=0.9, support=0.274` |
| **Mudariam** | **107 de 300** (acordo 0,643) |
| **Mudariam acima de τ** | **71 de 300** |
| Latência p50 / max | 379,9 ms / 612,8 ms |
| Custo da prévia | ~US$ 0,005 |
| **Escritas** | **0** |

### Extrapolação para o estoque inteiro **[I] (0,75)**

**[F]** O alvo tem **12.164 entradas** com `kind` de origem `auto` dentro dos 22
valores de produção (`insight` 7.489 · `lesson` 4.396 · `reference` 257 ·
`decision` 15 · resto Residual). A amostra de 300 reproduz essa proporção
(`insight` 183 · `lesson` 106 · `reference` 11), então a extrapolação linear é
defensável:

| | na amostra | no estoque (12.164) | custo |
|---|---|---|---|
| mudariam | 107/300 = 35,7% | **~4.338** | ~US$ 0,072 |
| **acima de τ** | 71/300 = 23,7% | **~2.878** | **~US$ 0,048** |

**[I] (0,85) O lote inteiro custa menos de cinco centavos** — com a economia que o
G2 mediu (medir é essencialmente de graça), o argumento de custo não existe.

## A transição dominante está CORRETA — e é o que torna isto defensável

**[F] `lesson → outcome`: 93 das 106 memórias `lesson`.** Confiança média
**0,85**, e **68 das 93 acima de τ=0,8**. As três de confiança 1,0:

```
conf=1.0  lesson -> outcome  outcome:bash:transcript-dd05e59f:failure
   '{"error":"Exit code 1\n  ✓ qgis_agents_provider.py — expressões soltas → fallback real\n…'
conf=1.0  lesson -> outcome  outcome:bash:transcript-988b90b1:failure
   '{"error":"Exit code 1\n2026-09-26T18:27:09Z ERROR touring…'
conf=1.0  lesson -> outcome  outcome:bash:transcript-3d82055d:failure
   '{"error":"Exit code 1\nFound 932 errors.\n\n55\tTID252\trelative-imports\n…'
```

**[I] (0,90) O texto É um registro de execução**, e o critério de `outcome` no
vocabulário é literalmente *"the recorded result of a command, run, or
experiment, with reward or exit status"*. A chave também diz
`outcome:bash:…:failure`. **[I] (0,95) O que está errado é a tag atual, não a
decisão do Jev** — `derive_tags` viu `entry_type` e caiu em `lesson`, e a
derivação determinística não tem como ler o JSON de dentro do valor.

**[I] (0,85)** Isto é a **mesma história do vocabulário, uma segunda vez** (a
primeira está em `w1-tags/report.md`): no G2,
o 8% da S-1.1 contra a tag de produção era vocabulário; aqui, o `lesson→outcome`
é a derivação determinística sendo grosseira onde o texto é específico. **Nos dois
casos, o sistema determinístico erra na fronteira e o sistema tipado acerta.**

### As outras transições

| transição | n |
|---|---|
| `lesson → gotcha` | 4 |
| `lesson → decision` | 3 |
| `lesson → strategy` | 3 |
| `lesson → doc` | 1 |
| `lesson → insight` / `→ observation` / `reference → strategy` | 3 |

**[F] Nenhuma transição vai para um valor fora dos 22** — a Choice nunca pediu o
que o vocabulário não tem, o que confirma que o filtro é o vocabulário e não o
modelo.

## O que trava a gravação, e por que não é um detalhe

**[F]** Três fatos, todos verificados por execução em 01/10:

1. `touring memory tag <KEY> <TAG>` **não tem `--source`** — grava `explicit`.
2. `TagSource` (`tags.rs:506`) é **enum fechado** de 4: `Backfill · Auto ·
   CodeSync · Explicit`. Valores reais na coluna: 24.835 auto · 15.825 backfill ·
   494 code_sync · 2.860 explicit.
3. A reversão que a coluna `source` permite **é** `DELETE … WHERE source='jev'` —
   **SQL**, que o plano proíbe.

**[I] (0,95) Marcar como `explicit` seria mentira e irreversível.** O doc-comment
diz *"stated by the author/LLM at creation (highest trust)"* — um Jev não é o
autor nem o creation, e o que ele mediu foi ECE 0,113 com τ=0,8. Declarar
"highest trust" algo que só vale acima de um corte de 0,8 inverte o sentido da
coluna.

**[F]** Uma correção que o quality gate me fez: a primeira versão montava o
fragmento `IN (…)` com f-string e o gate **reprovou em F2.1 (CWE-89, 0,02)**. Hoje
o SQL é 100% parametrizado (`?,?,?` com os 22 valores vinculados) e o F2.1 está
**1.000**. O gate estava certo: eu tinha escrito a lista de uma constante do
código, não entrada de usuário — mas a forma correta não custa nada.

## O que o Gabriel precisa decidir (duas palavras, não uma)

O S-5.7 passou a ser **uma edição só** com duas partes, e as duas precisam de
assinatura:

| # | Mudança | Onde | Sem ela |
|---|---|---|---|
| (a) | `Facet::Kind::seed_values()` += `fix`/`report`/`audit` | `tags.rs:251-278` | 37/73 dos rótulos do professor ficam inalcançáveis (o G2 mediu) |
| (b) | `TagSource::Jev` (+ `as_str() → "jev"`) | `tags.rs:506-526` | o backfill não é reversível nem honesto |

**[I] (0,90) Sem (b), a resposta é não fazer.** O plano já pede que nada gravado
o seja; um backfill que grava `explicit` e não sabe desfazer viola isso na
primeira leva. **[I] (0,85)** Com (b), o trabalho restante é mecânico: um
`TagSource::Jev` + um `jev_tags_backfill.py` com a escrita (o mesmo script, com
o ramo de escrita atrás de `--write` e `--tau`).

## Cross-references

- `scripts/jev_tags_backfill.py` · `experiments/s5-backfill/preview.json`
- `experiments/g2-baselines/results.json` → `threshold_recommended.memory_kind` (τ=0,80) e `split_sha256` (`deb10793…`, o meu)
- `crates/touring-intelligence/src/rl/memory/tags.rs:251-278` (`seed_values`) · `:506-526` (`TagSource`)
- `experiments/w1-tags/report.md` — S-1.1, a mesma história do vocabulário pela metade
