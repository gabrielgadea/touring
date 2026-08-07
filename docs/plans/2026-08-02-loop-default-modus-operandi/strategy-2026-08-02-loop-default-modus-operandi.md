---
type: Strategy
title: O loop como modus operandi padrão — afordância, não persuasão
description: Alinhar constituição (CLAUDE.md + rules) e camada estrutural para que o OUTER determinístico e a convergência medida sejam o default, não um modo opt-in.
plan_id: 2026-08-02-loop-default-modus-operandi
tags: [loop, constitution, enforcement, adw, convergence]
timestamp: 2026-08-02T14:10:00-03:00
okf_version: "0.1"
---

# O loop como modus operandi padrão

Parte do [bundle](/index.md). Diagnóstico: [diagnostics/touring-20260802T135423.md](/diagnostics/touring-20260802T135423.md).

## 1. Pergunta

O modus operandi **padrão** do TACO já é o da skill `loop-engineering`, com todos
os passos que garantem trabalho bem-feito e completo?

**Resposta medida: não.** A disciplina existia e funcionava — mas só quando
invocada explicitamente. Fora disso, zero enforcement.

## 2. Evidência

### 2.1 KPI de aderência (`~/.claude/loop-engineering/compliance.jsonl`)

159 registros, 23/07 → 31/07/2026:

| flow | completo | incompleto | aderência |
| --- | --- | --- | --- |
| `strategy-outer` | 67 | 30 | 69,1 % |
| `cross-audit` | 59 | 3 | 95,2 % |
| **default (sem invocação)** | — | — | **sem registro: nada era armado** |

A camada estrutural **funciona** onde existe (95 % no cross-audit). A lacuna não
é de eficácia, é de **cobertura**: `loop_outer_arm.py` só armava sobre slash
command ancorado, então a esmagadora maioria dos turnos rodava sem gate.

### 2.2 Lição institucional que fixa a direção da solução

`touring memory recall "protocol-adherence-diagnosis:2026-07-23"` — diagnóstico
do próprio Gabriel: **persuasão não muda `U(a) = P·V − C(tokens)`; afordância
muda.** Causa-raiz C3: "skill = persuasão passiva, 17 passos sem gate
estrutural"; C5: nudges `cli-suggest` MUST a conf 0.95 ignorados 2× *na própria
sessão que os emitiu*.

**Consequência de projeto**: acrescentar prosa ao CLAUDE.md é a intervenção
comprovadamente ineficaz. A prosa é necessária (é a especificação) mas nunca
suficiente — o que decide é a estrutura.

## 3. Defeito encontrado no caminho (bloqueante)

Ao rodar o OUTER determinístico para produzir esta evidência, o próprio OUTER
falhou de forma silenciosa.

**Sintoma**: `strategy-loop` encerrou o nó `explore_loop` em 2 iterações de 4,
declarando `on_dry`, enquanto o ledger CCE registrava 30 e 4 achados novos
naquelas duas rodadas — nenhuma seca.

**Causa-raiz** (`adw.py:770`):

```python
new_findings = int(found.group(1)) if found else 0   # ← marcador ausente = zero
```

`touring explore` **nunca emitiu** `NEW_FINDINGS=<n>`, e os specs ainda passavam
a saída por `tail -c 1800`. Silêncio virava "zero achados" virava "rodada seca".
Após `dry_rounds=2` o loop terminava alegando convergência.

**C08 (cross-caller compare)** — assimetria clássica, 1 de 3 callers correto:

| spec | body do loop | emite o marcador? |
| --- | --- | --- |
| `scout-perpetuo.toml` | `scout_perpetuo.py cycle` | ✅ (`:170`, com teste) |
| `strategy-loop.toml` | `touring explore …` | ❌ |
| `explore-plan.toml` | `touring explore …` | ❌ |

É a **mesma classe de defeito** que `loop_converged.py` corrigiu em 02/07/2026
("`dag_done` era fail-OPEN → agora fail-CLOSED"): um veredito de terminação que
passa na ausência de evidência. A lição foi aprendida num script e não propagada
ao irmão.

**Gravidade**: máxima para este objetivo. Padronizar um OUTER que declara
"explorado até secar" após 2 rodadas produtivas seria institucionalizar
confiança fabricada.

## 4. Decisões

| # | Decisão | Razão |
| --- | --- | --- |
| D1 | Marcador ausente ⇒ **desconhecido**, nunca zero (`adw.py` fail-closed) e registrado no journal (`dry_signal`) | um veredito de terminação jamais pode passar por falta de evidência (Lei L2) |
| D2 | `explore_until_dry.py` emite `NEW_FINDINGS=<n>` em **stderr**, última linha, e **silencia** quando nenhuma rodada rodou | stderr preserva stdout como JSON estrito; última linha sobrevive a `tail -c`; silêncio em `--status` impede que uma chamada read-only finja rodada seca |
| D3 | Flow **default** `work-outer` armado por prompt de trabalho substantivo | fecha a lacuna de cobertura do §2.1 por afordância |
| D4 | Manifesto do `work-outer` **menor** que o do `strategy-outer` (diagnostic + ledger, sem strategy doc), cap 2 continuações | proporcionalidade: 1 comando satisfaz; falso-positivo custa ≤ 2 avisos e libera |
| D5 | Detecção por **modo imperativo**, não por radical aberto | radical aberto casou `audit` dentro de `TACO-cross-audit`; e em inglês o imperativo é homógrafo do substantivo (`o fix do audit ficou bom`) — ambos pegos por teste |
| D6 | `work-outer` **nunca rebaixa** um flow invocado explicitamente | senão um prompt de trabalho no meio do fluxo perdoaria artefatos já devidos |
| D7 | Gate de entrega do CLAUDE.md passa a ter veredito por **exit code** | a checklist auto-avaliada era exatamente o modo de falha que a Lei L3 existe para eliminar |
| D8 | Kill switch humano `TOURING_WORK_OUTER_DISABLED=1` | muda comportamento global de sessão; reversão deve ser trivial e humana |

## 5. Prova executada

- `adw.py` + `explore_until_dry.py` + `scout_perpetuo.py`: **59 testes** verdes,
  incluindo 3 novos (marcador em stderr, silêncio sem rodada, fail-closed).
- `test_flow_guard.py`: **35 testes** verdes, incluindo 6 novos (arma em trabalho
  real, não arma em conversa, kill switch, não-rebaixamento, bundle estável).
- **End-to-end do defeito**, mesmo tópico, mesmo ledger:

| | rodadas | achados novos | veredito |
| --- | --- | --- | --- |
| antes | 2 | 30, 4 | `on_dry` (falso) |
| depois | 4 (orçamento exaurido) | 13, 4, 15, 2 | `converged: false` (honesto) |

34 achados que o loop quebrado nunca teria encontrado.

## 6. Escopo deliberadamente não feito

- **`adw lint` não valida que o body de um `loop` sabe emitir o sinal.** É o que
  deixou D2 passar. Estaticamente indecidível para nó `code` arbitrário; o
  runtime agora o expõe (`dry_signal: absent` no journal + outcome), que é a
  defesa durável. Lint heurístico fica como potencialização registrada.
- **Recall da detecção é conservador por escolha.** "faça os fixes:" (3 palavras)
  não arma. Perder um arme não custa nada à sessão; armar à toa custa avisos.
  Ajuste = `MIN_WORK_WORDS` e a lista de verbos, ambos com teste.

## 7. Aberto para decisão humana

1. Reabilitar `block_git.sh` (`GIT_GUARD_ENABLED=1`) agora que a publicação
   terminou.
2. CI do touring: `coverage`, `integration tests` e `fuzz targets` seguem
   vermelhos (REGRA #21) — `check + clippy`, `MSRV`, `cargo-deny`, `quality
   gates` e `doctests` já passam.
