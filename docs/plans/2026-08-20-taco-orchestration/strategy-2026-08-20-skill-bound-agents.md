---
okf_version: "1.0"
type: Strategy
title: "Agentes de ADW ligados a skills — o O de TACO"
description: "Ligar cada etapa dos fluxos ADW à skill TACO que já codifica aquele ofício, via um campo `skill` no nó agent validado no lint, fragments compostos e wiring da library"
tags: [adw, skills, orchestration, taco, agent-nodes]
timestamp: 2026-08-20T02:05:00-03:00
plan_id: 2026-08-20-taco-orchestration
---

# Agentes de ADW ligados a skills — o "O" de TACO

## O diagnóstico, medido

O `agent` é um dos seis tipos de nó do runner (`NODE_TYPES = {code, agent, gate,
loop, human, parallel}`) e compila para uma sessão headless:

```python
cmd = ["claude", "-p", prompt, "--output-format", "json"]
cmd += ["--agents", json.dumps({role: definition}), "--agent", role]
cmd += ["--allowedTools", *tools]
```

A persona já viaja: `compile_persona()` renderiza dez seções ordenadas (STANCE,
LENS, SCOPE, BAR, BURDEN, CANNOT, FORBIDDEN, BLIND, ESCALATE, OUTPUT) para o
`--agents`. O que **não** viaja é o ofício:

| medição (20/08/2026) | valor |
|---|---|
| ocorrências de `Skill` em `adw.py` (2.870 linhas) | **0** |
| ocorrências de `Skill` nas 10 specs da library | **0** |
| nós `agent` em execução hoje sem skill alguma | **8**, em 6 specs |
| `Skill` é invocável por agente headless? | **sim** — provado |

A prova do último item é literal: `claude -p "Invoke the Skill tool with
skill='taco-planning' …" --allowedTools Skill` respondeu
`SKILL_OK=yes loaded=# taco-planning — Touring-Grounded Plan Excellence`.

**Portanto**: o mecanismo sempre existiu e nenhum fluxo o usa. Todo agente de ADW
que planeja re-deriva do zero o que `taco-planning` já codifica (9 dimensões,
ground truth VGP, `plan_validator`); todo agente que audita re-deriva o que
`TACO-cross-audit` já codifica (6 fases, as quatro formas de errar o veredito).
Isso não é orquestração — é execução paralela de conhecimento duplicado, que é
exatamente o antipadrão P1 que a família acabou de eliminar entre as skills.

## A estratégia

Uma **afordância**, não uma exortação: o fluxo declara o ofício, e o runner o faz
chegar. Cinco fases.

### F1 — o primitivo: campo `skill` no nó agent

```toml
[nodes.plan]
type = "agent"
skill = "taco-planning"          # ← novo
prompt = "..."
allowed_tools = ["Read", "Grep", "Glob"]
```

O runner então: (a) **valida no lint** que a skill existe em disco — uma skill
inexistente é erro de lint, nunca um no-op silencioso em runtime; (b) **valida
que ela não está `off`** em `skillOverrides` (medido: 28 estão) — ligar uma skill
desabilitada produziria exatamente o defeito recorrente desta sessão, um
instrumento que parece funcionar; (c) **injeta `Skill` em `allowedTools`
automaticamente** — declarar a skill sem a ferramenta é mandar o agente usar algo
que ele não pode chamar; (d) **prefixa uma instrução determinística** ao prompt;
(e) marca o nó como **não read-only**, porque uma skill pode fazer qualquer coisa
(`READ_ONLY_TOOLS` não a contém, e essa é a resposta honesta).

### F2 — os fragments: composição, não cópia (P1)

Peças reutilizáveis que já trazem o nó ligado, para `[[use]]`:

| fragment | skill ligada | etapa que ele resolve |
|---|---|---|
| `plan-pack` | `taco-planning` | OUTER 10 — planejar/decompor |
| `audit-pack` | `TACO-cross-audit` | INNER 13 / CLOSE — fidelidade de propósito |
| `skilling-pack` | `TACO-skilling` | qualquer etapa que produza procedimento reusável |

### F3 — wiring da library

Os 8 nós `agent` existentes recebem a skill do seu ofício (`feature.plan` →
`taco-planning`, `audit` → `TACO-cross-audit`, etc.), por composição.

### F4 — o loop nomeia as próprias etapas

`loop-engineering/SKILL.md` passa a citar os fragments nos passos 10 e 13, em vez
de descrever o ofício em prosa.

### F5 — gate e testes

Suíte para os dois modos de falha silenciosa (skill inexistente; skill `off`) mais
a injeção de `Skill` em `allowedTools`, no CI.

## O que NÃO está no escopo

Catálogo global de agentes (o runner decidiu, por design, que personas são
declaradas **inline** para o `.toml` continuar portátil — a skill segue a mesma
regra: é um campo do nó, não um ponteiro para registro externo). Também não entra
descoberta automática de "qual skill para qual nó": o fluxo declara.

## Critério de convergência

`loop_converged.py` exit 0, com evidência de que um nó ligado a skill **de fato**
carrega `Skill` em `allowedTools` e falha no lint quando a skill não existe ou
está desabilitada — provado por mutação, nunca por leitura.
