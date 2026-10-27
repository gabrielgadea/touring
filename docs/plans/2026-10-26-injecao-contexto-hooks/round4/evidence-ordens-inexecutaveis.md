---
type: Evidence
title: "As ordens MUST chegam executáveis? — 58,0% do code-mode-contract chegam quebradas"
description: "A medição que troca 'o modelo ignorou' por 'a ordem era inválida'."
plan_id: 2026-10-26-injecao-contexto-hooks
okf_version: 1
timestamp: 2026-10-26
---

# As ordens chegam executáveis?

> **Origem**: o scout do corpus qualitativo acusou que a ordem `MUST` do
> `code-mode-contract` é **quebrada** — *"o programa fica aberto no meio do
> heredoc, sem o `EOF` final"*. A acusação foi testada no corpus inteiro.
> **Instrumento**: `/tmp/check_broken_directives.py` (400 sessões).

## O resultado

| família | ordens MUST/SHOULD/MAY | **quebradas** | % |
|---|---:|---:|---:|
| **`code-mode-contract`** | 1.854 | **1.076** | **58,0%** |
| `touring-suggest` | 4.765 | 303 | 6,4% |
| `session-contract` | 72 | 0 | 0,0% |

Sinais (contados sobre a diretiva **inteira**, não truncada):
`aspas-impares` **1.070** · `heredoc-sem-terminador` **513** · `programa-aberto` 600.

### 0.1 A normalização que evita o falso positivo

O shell usa o ideoma `'\''` para embutir uma aspa simples dentro de uma string
entre aspas simples. Contar aspas **cruas** dá falso positivo em toda ordem com
heredoc. O detector colapsa `'\''` → `'` **antes** de contar:

| | antes da normalização | depois |
|---|---:|---:|
| `code-mode-contract` quebradas | 1.074 (57,9%) | **1.076 (58,0%)** |

⇒ O achado **sobrevive** à normalização. (O `touring-suggest` tem 288 aspas
ímpares, mas **zero** heredoc-sem-terminador depois de normalizado.)

## O exemplo real

```
  MUST    touring run --lang bash --code 'mkdir -p /tmp/c3-inv && cat > /tmp/c3-inv/stage_a.py <<'\''PYEOF'\''
```

A aspa final nunca fecha: o shell ficaria esperando mais entrada.

## Por que isso acontece

`crates/touring-hooks-shared/src/turn_budget.rs:118-133` — `directives_only()`:

```rust
pub fn directives_only(context: &str) -> Option<String> {
    let kept: Vec<&str> = context.lines()
        .filter(|l| { let t = l.trim_start(); t.starts_with("MUST") || t.starts_with("SHOULD") })
        .collect();
    ...
    Some(format!("{}\n  (orçamento do turno esgotado — só as diretivas; ...)", kept.join("\n")))
}
```

O filtro **mantém a linha inteira** — inclusive a linha `MUST` cujo programa já
veio **truncado** do emissor original. O rodapé promete "só as diretivas" e
entrega **metade de um programa**.

## O que isso muda no diagnóstico

| leitura | veredito |
|---|---|
| ❌ antes | "`code-mode-contract` tem 0% de adesão ⇒ o modelo ignora diretivas" |
| ✅ agora | "**58% das ordens são sintaticamente inválidas** ⇒ a não-adesão é de **ordem quebrada**, não de desobediência" |

A distinção não é retórica: **conserta uma ordem invalida; não conserta um
modelo que ignora**. As duas correções são diferentes e exigem interventions
diferentes — e esta exige uma mudança de ~10 linhas no emissor, não uma
mudança de comportamento do modelo.

**Precedente**: harness2, `core/spec.py:163-167` — *"is in the range that
fabricates defects; use >= 10_000"*, e o princípio: **truncamento que fabrica
defeito é pior que ausência**.

## A correção (S-7)

Nunca emitir um programa truncado. Opções, todas no emissor:
1. **truncar em fronteira segura** (o programa fecha, ainda que incompleto);
2. **omitir a diretiva incompleta** quando ela não fecha;
3. **emitir um programa mínimo executável** no lugar do truncado.

**Testes exigidos**: `directives_only` nunca devolve ordem com aspas ímpares ou
heredoc sem terminador; as diretivas completas passam intactas; o rodapé
*"orçamento do turno esgotado"* continua sendo emitido (o teste existente
`crates/touring-hooks/tests/cli_suggester_e2e.rs:313` verifica isso).

---

_Medido por TACO, 26/10/2026. Reprodutível: `/tmp/check_broken_directives.py`._
