---
type: CrossAudit
title: "Cross-audit 2026-08-07 — a sessão do modelo de wiring"
description: "Auditoria de fidelidade de propósito sobre tudo que a sessão alterou. 4 defeitos encontrados no próprio trabalho da sessão, incluindo o que tornava a correção REGRA #0 invisível ao medidor."
plan_id: 2026-08-07-rkyv-migration
tags: [cross-audit, purpose-fidelity, wiring, regra-0, regra-21, visibilidade]
timestamp: 2026-08-07T17:45:00-03:00
okf_version: "0.1"
---

# Cross-audit 2026-08-07 — a sessão do modelo de wiring

Escopo: **tudo que esta sessão alterou** — 4 correções no modelo de wiring, a
purga de fantasmas, o diagnóstico do timeout, F2.6, o teto de varredura, o
refactor do trait `Verification`, dois refactors de teste e o bundle OKF.

A pergunta desta auditoria não é "compila?" — é **"faz o que diz que faz?"**. E a
resposta, para quatro pontos do meu próprio trabalho, era **não**.

## VERDICT

**PASS com 4 defeitos encontrados e corrigidos durante a auditoria.** Nenhum
deles apareceria em teste unitário: todos são divergência entre o contrato
documentado e o comportamento real.

O mais grave é o quarto, e ele é irônico: **o medidor de órfãos não conseguia ver
a correção que a REGRA #0 manda fazer.**

## SCORECARD

| Eixo | Evidência |
|---|---|
| Superfície auditada | 11 arquivos + 50 verificadores migrados |
| Símbolos novos | 9 privados · 4 `pub`, **todos com consumidor externo verificado** |
| Dívida (`TODO`/`FIXME`/`unimplemented!`/`allow(dead_code)`) | **0** |
| `unwrap()` em produção nos arquivos tocados | **0** |
| Órfãos nos meus arquivos | 26 → 21 falso-positivo (chamada qualificada) + 5 analisados |
| 6 dims P0 BLOCK | F2.1 0.9972 · F2.4 0.9852 · F2.5 1.0 · F2.6 1.0 · F4.3 1.0 · F4.5 0.70 (Warn) |
| Tier 50-dim | **Platinum** 0.9386, `blockers: []` |

## FINDINGS

### F1 — `raise_timeout_floor` não honrava o contrato que documenta ⛔

A doc dizia "an explicit `--timeout` from the operator always wins". A
implementação usava `compare_exchange(DEFAULT=120, 1800)` — uma **sentinela**.
`--timeout 120` é byte-idêntico ao default, então a escolha explícita do operador
era silenciosamente elevada a 1800: o oposto exato do contrato.

Meu próprio teste usava `45` e passava. Cobria o caso que o autor pensou, não o
que o **propósito** implica — que é precisamente a distinção que esta skill existe
para pegar.

**Correção**: `TIMEOUT_SET_BY_OPERATOR: AtomicBool`, marcado no parse de
`--timeout`. Marca a **escolha**, não o valor. Teste de regressão
`explicit_timeout_equal_to_the_default_still_wins`, mais um mutex serializando os
dois testes que compartilham os mesmos estáticos.

### F2 — `dir_scan_overflow` varria a árvore 14× por pontuação ⚠

O anúncio de truncagem ficava em `score_scope_native`, que roda **uma vez por dim
`ScopeNative`** — 14 delas, em `par_iter`. Cada chamada percorria a árvore inteira
fazendo `metadata` de ~2000 arquivos. Onde antes havia zero syscalls, passou a
haver ~28 mil, concorrentes.

**Correção**: computado **uma vez** por escopo em `score_scope` e passado adiante.
Os dois call sites de fallback per-crate computam o seu (caminho raro, mesmo
escopo).

### F3 — `evidence_marks_not_applicable` era `pub` sem consumidor externo ⚠

Único `pub` genuíno sem consumidor fora do módulo (grep confirmou 0). Reduzido a
`pub(crate)` — visibilidade que descreve o uso real.

### F4 — O modelo de wiring conta `pub(crate)` como `pub` ⛔ **(achado central)**

**Sintoma**: depois de reduzir `auto_remediation` e
`DEFAULT_DAEMON_READ_TIMEOUT_SECS` para `pub(crate)` — a correção que a REGRA #0
pede —, os dois **continuaram listados como órfãos**.

**Prova em SQL**:

```
sqlite> SELECT symbol_name, visibility FROM wiring_map
        WHERE symbol_name IN ('auto_remediation','DEFAULT_DAEMON_READ_TIMEOUT_SECS',
                              'is_under_generated_tree') AND consumer_file IS NULL;
DEFAULT_DAEMON_READ_TIMEOUT_SECS|public
is_under_generated_tree|public
auto_remediation|public
```

Os três são `pub(crate)` na fonte.

**Causa-raiz**: `cli/handlers/index.rs` chamava
`register_pub_symbol(&rel_path, &sym.name, kind_str, "public")` — string
**literal** —, descartando `sym.visibility`. E esse campo **já é computado**:
`Symbol::detect_visibility` distingue `pub(crate)` → `Visibility::Crate`, e
`Visibility::as_str()` já devolve `"crate"`.

O dado certo existia. O filtro certo existia (as 5 queries de órfão filtram
`visibility = 'public'`). Faltava o elo, e o call site o jogava fora.

**Correção — potencialização pura (REGRA #0)**: ligar o que já existe, sem
adicionar nada. A métrica passa a significar *"superfície de API pública sem
consumidor"*, que é o que ela sempre alegou medir.

**Efeito medido** (rebuild + reindex, 3120 arquivos, 0 erros):

```
sqlite> SELECT visibility, COUNT(*) FROM wiring_map WHERE consumer_file IS NULL
        GROUP BY visibility;
public|11090
crate|799
```

Os quatro símbolos do diagnóstico agora gravam `crate`, e **799 símbolos
`pub(crate)`** saíram da conta de API pública. Órfãos: **4554 → 4246**.

Total da sessão: **5031 → 4246 (−785)**, sem apagar uma linha de código —
apenas medindo o que a métrica sempre disse medir.

## FUSED RISK

| Unidade | Defeito | Severidade | Por quê |
|---|---|---|---|
| `cli/handlers/index.rs` | F4 visibilidade descartada | **alta** | corrompe a métrica que governa a REGRA #0 em todo o workspace |
| `daemon_client.rs` | F1 sentinela vs escolha | **média** | ignora comando explícito do operador, em silêncio |
| `scope_report.rs` | F2 14 varreduras | baixa | custo, não correção |
| `verifications/mod.rs` | F3 `pub` excessivo | baixa | ruído na superfície pública |

## ROOT-CAUSE

Três dos quatro têm a mesma forma: **um valor correto existia e foi descartado ou
aproximado por sentinela.**

- F1 aproximou "o operador escolheu" por "o valor difere do default".
- F4 aproximou "a visibilidade" pela constante `"public"`.
- F2 recomputou por dim o que é invariante por escopo.

A lição, e é a mais transferível da sessão: **quando uma correção correta não move
o número, suspeite do medidor antes de suspeitar da correção.**

## PROVENANCE

Comandos executados, não inferidos:

- `sqlite3 .claude/touring/knowledge.db "SELECT symbol_name, visibility …"` — a prova de F4
- `grep -rn` por consumidor externo de cada um dos 26 órfãos nos meus arquivos
- `python3` + `re` para visibilidade real na fonte (regex de shell falhou 2× com alternância vazia / parênteses — corrigido para Python)
- `cargo test -p touring-server --lib read_failure` → 4/4
- `cargo test -p touring-quality --lib` → 382/382
- `cargo check -p touring-cli` → 0 erros

Limitações declaradas: `touring ast meta` devolveu `blast/quality = None`
(`on_disk_fallback` em cache-miss, comportamento documentado) — o blast radius
foi obtido por outra via. `touring wiring cycles` não devolveu JSON parseável
nesta execução; **ciclos ficam `UNVERIFIED` neste relatório.**

## ACTIONS

Corrigido nesta auditoria: F1, F2, F3, F4 — todos com teste ou prova executada.

Aberto, **decisão humana**:

1. **`orphans_base`** — única cláusula de convergência não atendida. Com F4
   corrigido a contagem muda de novo, e para melhor (helpers crate-internos saem
   da conta). A baseline segue medida sob o modelo antigo; **não a regravei**.
2. **F1.3 = 0.5901 (Warn)** — Pass exige ≥0.80 ≈ 4% de duplicação, cerca de metade
   das linhas atuais. Alvos de produção localizados em `/phases/P5.md`.
3. **Ciclos de dependência** — `UNVERIFIED`, o comando não devolveu JSON.

Registrado e não corrigido (fora do escopo desta sessão, sem regressão
introduzida): a raspagem de `use` continua cega a literais de string e prosa — o
guard de aspas cobre só o extrator de alias que adicionei.
