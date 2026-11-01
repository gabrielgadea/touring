---
type: ComponentContract
title: skill-picker — contrato de invocação para registro em hook
description: Onde o P8 picker mora, como se invoca, que devolve e o que um hook precisa saber para registrá-lo no settings.json sem ler o fonte.
plan_id: 2026-09-30-jev-harness-potencializacao
tags: [contrato, p8, skill-picker, hook, s-5.1]
subtask: S-5.1
owner: touring-02
timestamp: 2026-10-01T08:45:00-03:00
okf_version: "0.1"
---

# skill-picker — contrato de invocação

Resposta à pergunta da coordenadora: *"onde ele mora e como invocar: argv? stdin?
qual o payload?"*. **Não é `~/.claude/skills/skill-picker*`** — é um script do
repositório, como o resto dos scripts da wave.

## Onde mora

```
scripts/skill_picker.py          # o picker (15.781 B, 13 símbolos)
scripts/test_skill_picker.py     # 9/9 testes
docs/plans/2026-09-30-jev-harness-potencializacao/experiments/s5-skill-picker/
    report.md                    # a medição do gate (τ=0,55)
    queries.json                 # pool de 200 tarefas reais (seed 20261001)
```

## Como se invoca

**argv, sempre. Não lê stdin** — a pergunta vem em `--query`, e um hook que
passe a pergunta por stdin não teria como declarar o que fez num log.

```bash
# 1) catálogo só — o que o pré-filtro escolheria, ZERO chamadas à API
python3 scripts/skill_picker.py --query "<a tarefa>" --json

# 2) a cascata completa: catálogo → top-3 → Choice, com o gate
python3 scripts/skill_picker.py --query "<a tarefa>" --judge --json

# 3) sem chamar a API, mas mostrando o plano exato da chamada
python3 scripts/skill_picker.py --query "<a tarefa>" --dry-run --json

# 4) o catálogo inteiro (para depurar)
python3 scripts/skill_picker.py --catalog

# 5) override manual — ignora o gate por definição
python3 scripts/skill_picker.py --override skill-name --json
```

**Flags:** `--tau` (padrão **0,55**, medido) · `--json` · `--judge` · `--dry-run` ·
`--catalog` · `--override`.

## O que devolve

O JSON tem **dois campos que um hook precisa**, e um terceiro que é o ponto todo:

```json
{
  "query": "a tarefa",
  "catalog_size": 185,
  "shortlist": [{"key": "recipe-gmail-send", "score": 0.71, "aliases": []}],
  "tau": 0.55,
  "source": "shortlist_only | jev",
  "confidence": 0.82,
  "said": true,
  "silent_reason": null,
  "picked": "recipe-gmail-send",
  "writes_performed": 0
}
```

| campo | para que serve no hook |
|---|---|
| **`said`** | **`true` só quando há sugestão a dizer.** `false` = o gate calou |
| `silent_reason` | por quê calou: `shortlist_empty` · `below_tau` · `jev_said_none` · `low_confidence` · `jev_fail_open` · `adapter_unavailable:*` |
| `picked` | a skill escolhida (só quando `said`) |
| `confidence` | a confiança do Jev, quando ele respondeu |
| `shortlist` | os 3 candidatos com score — o que permite auditar a decisão |

## O que um hook deve fazer (e o que NÃO deve)

**A regra que o gate existe para impor: em `said: false`, o hook não diz nada.**
Não há mensagem, não há log de injeção, não há "talvez você precise de…". Um gate
que sugere mesmo quando calou não é um gate.

**`silent_reason` é o que se registra**, não a ausência — um hook que registra só
quando fala produz um journal vazio que parece "ninguém pediu skill", e é
indistinguível de "o gate está quebrado". A `decision` KPI já tem a forma certa
para isso (`unmeasured` ≠ `0`), e é o mesmo contrato.

## Limites que o hook herda

1. **[F] Latência.** Com `--judge`, ~370 ms por chamada (medido: p50 371,9 ms,
   max 612,8 ms). **[I] (0,85)** Isso é **incompatível com um hook síncrono de
   `UserPromptSubmit`** — seria 370 ms em cada prompt. **[I] (0,80)** O caminho
   certo é `SessionStart` (uma vez por sessão) ou um hook em background que
   escreve num arquivo, que outro passo lê. **O plano já registra "hook síncrono
   segue anti-uso"** no §4 do relatório; isto é a medida que sustenta.
2. **[F] Custo.** ~US$ 0,0000167 por chamada, com τ=0,55 disparando em 27,5% das
   tarefas → ~US$ 0,0046 por sessão. Irrelevante.
3. **[F] `--dry-run` e o catálogo não chamam a API** — o registro pode ser testado
   sem gastar nada.
4. **[F] O `state` enviado ao Jev é `{task, candidates}`** e nada mais; a
   `description` das skills entra como `criteria` contrastivo, e o `when_not_to_use`
   é o que separa skills irmãs.

## Cross-references

- `experiments/s5-skill-picker/report.md` — a medição (bimodal, τ=0,55, e o porquê do IDF)
- `scripts/skill_picker.py` · `scripts/test_skill_picker.py` (9/9)
- `scripts/jev_client.py` — `system_one(state, questions)`, fail-open devolvendo `None`
- `plan.md` S-5.1 (gate) · S-5.2 (herda τ daqui)
