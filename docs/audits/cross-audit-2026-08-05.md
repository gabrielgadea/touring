---
okf_version: "1.0"
type: AuditReport
title: "Cross-audit 2026-08-05 — tudo o que foi implementado na sessão Memento (fases 1-4, rodada 2, M1-M4)"
description: "Auditoria cruzada de 21 arquivos Rust + 1 Python. 3 defeitos reais encontrados e corrigidos, o principal suprimindo 67,4% da classificação de casos que a própria sessão havia implementado. Duas medições minhas foram refutadas pelos dados."
tags: [cross-audit, memento, rl, case-bank, purpose-fidelity, regra-21]
timestamp: "2026-08-05T00:50:00-03:00"
plan_id: 2026-08-04-memento-rl-insights
---

# Cross-audit 2026-08-05 — a sessão Memento inteira

> Auditoria do que **esta sessão** produziu. O achado principal é o pior tipo:
> um recurso que passava em todos os testes e não funcionava em produção,
> porque a fixture afirmava a forma que eu **presumi**, não a que o sistema
> escreve.

## 1. VERDICT

**Aprovado com 3 defeitos corrigidos.** O escopo — 21 arquivos Rust + 1 Python,
4 fases + 4 movimentos — cumpre o propósito documentado após as correções.
Zero P0 BLOCK. Zero dívida nova. Zero órfãos. Zero ciclos.

O achado que justifica a auditoria: **`repair_from` classificava 1.122 de 3.446
reparos (32,3%), não os 3.446**. Toda a maquinaria de M1/M2 — a classe positiva,
a partição rotulada, o ranking por valor — operava sobre um terço da base. Seis
testes unitários passavam.

## 2. SCORECARD

| Gate | Resultado | Evidência |
|---|---|---|
| 6 dims P0 BLOCK (F2.1/F2.4/F2.5/F2.6/F4.3/F4.5) | **0 FAIL** em 5 arquivos centrais | todos `Pass` ou `NotApplicable`, score 1.00 |
| Piso de entrega Gold (0.80) | **pass** | `touring-quality score … --fail-below 0.80` exit 0 |
| `cargo check --workspace --all-targets` | **exit 0** | pós-correções |
| `cargo clippy --workspace --all-targets -D warnings` | **0 erros** | pós-correções |
| Testes (6 crates tocados) | **0 falhas** | incl. 1.310 no `touring-dispatch` |
| Tripwire do registry, **2 perfis** | verde | default **e** `--features acp-protocol` |
| Ciclos de dependência | **0** | `touring wiring cycles --min-depth 2` |
| Órfãos entre os 18 símbolos novos | **0** | cada um com consumidor real |
| `touring doctor` | **6/6 OK** | `kind_unknown=0` após convergência |
| `touring e2e` | **0.8736 pass** | 81/84, 2 WARN de baseline, 0 falhas |

## 3. FINDINGS — 3 defeitos, todos confirmados por execução

### F-1 · BLOCK · `repair_from` exigia string onde o dado é objeto

```rust
let then = parsed.get("resolution_input").and_then(|v| v.as_str())?;   // ← as_str()
```

`redacted_lesson_value` grava `error` como **string** e `resolution_input` como
**objeto** (`{"file_path":…}`, `{"command":…}`). `as_str()` num objeto retorna
`None`, então o caso inteiro era descartado.

**Medido sobre as 3.478 entradas reais:**

```
classificados REPAIR  ANTES do fix   1122   ( 32.3%)
classificados REPAIR DEPOIS do fix   3446   ( 99.1%)
negativos genuinos restantes           32   (os bloqueios do CEG)
```

**2.324 reparos — 67,4% da base — eram invisíveis.** M1 e M2, a entrega
principal da rodada 3, operavam sobre um terço do banco.

Por que os testes não pegaram: eu escrevi a fixture com `resolution_input` como
string. Os seis testes verificavam a forma que eu presumi.

**Correção (potencializa)**: `json_field_as_text` aceita string **ou** valor
estruturado, serializando o objeto. A forma antiga continua funcionando — o
conserto alarga, não troca.

### F-2 · BLOCK · o contador de crédito mentia

```rust
if conn.execute("UPDATE memory_entries SET outcome_reward = ?1 WHERE key = ?2", …).is_ok() {
    n += 1;
}
```

`Connection::execute` devolve `Ok(linhas_afetadas)`. Um UPDATE que não encontra
a chave devolve `Ok(0)` — e era contado como crédito. A **única** métrica que
reporta se o laço de atribuição fechou reportaria sucesso sem ter escrito nada.

**Correção**: `updated += rows`.

### F-3 · BLOCK · crédito local contra recall federado

`memory recall` varre **7** `memory.db` (todos os projetos); `cli_memory_credit`
abria só o do projeto corrente. Um caso servido por outro projeto nunca poderia
ser creditado — o laço ficaria aberto exatamente para as lições cross-project
que a federação existe para expor.

**Correção (potencializa)**: o crédito percorre o mesmo conjunto federado que o
recall lê.

## 4. FUSED RISK — o que restou

| Unidade | Risco | Natureza |
|---|---|---|
| WIRING 0.75 — 6.350 órfãos (42,9%) | baixo, amplo | baseline do workspace, pré-existente |
| AST 0.76 — CC 16-24 em 3 scripts de `taco-planning` | baixo | pré-existente, fora do escopo tocado |
| 46 arquivos "quentes" (3+ edições em 7d) | informativo | reflete a própria sessão |
| `CaseLedger` é estado **em memória do daemon** | contido | recall e crédito precisam da mesma vida do daemon; um restart perde o pendente. Aceitável (é um join de curto prazo), mas é limite real |

Nenhum é regressão desta sessão.

## 5. ROOT-CAUSE — a alavanca contrafactual

Os três defeitos compartilham a causa: **eu verifiquei o contrato que escrevi, não
o que o sistema produz.**

- F-1: fixture com a forma presumida em vez da forma gravada.
- F-2: presumi que `execute` sinalizava sucesso da *escrita*; ele sinaliza sucesso
  da *chamada*.
- F-3: presumi que crédito e recall liam o mesmo banco; o recall é federado.

O antídoto não é mais teste unitário — é **testar contra a forma real**. Os seis
testes de regressão novos usam a fixture verificada no banco vivo, e o
`the_string_form_still_classifies` garante que o conserto alargou sem trocar.

## 6. PROVENANCE — evidência executada

```
real_shape_regression_tests::the_shape_the_miner_really_writes_is_recognised ... ok
real_shape_regression_tests::the_object_resolution_reaches_the_positive_class ... ok
real_shape_regression_tests::the_string_form_still_classifies ................. ok
real_shape_regression_tests::empty_and_absent_actions_are_not_repairs ......... ok
real_shape_regression_tests::prose_mentioning_the_field_name_is_not_a_repair .. ok
real_shape_regression_tests::field_reader_handles_every_json_shape ............ ok
test result: ok. 6 passed; 0 failed
```

**Duas medições minhas foram refutadas pelos próprios dados nesta auditoria** —
registradas porque a correção é mais informativa que o acerto:

1. **"0/200 reparos classificam"** → amostra de 200 linhas enviesada. A população
   diz **1.122/3.478 (32,3%)**. O defeito era severo, não total. Uma amostra com
   `LIMIT` sem `ORDER BY` não representa a população.
2. **"os `module_file` são caminhos fantasma → defeito de mangling no indexador"**
   → era **staleness transitória**. O contador convergiu `53 → 18 → 0` conforme o
   indexador incremental registrou os produtores — exatamente a corrida que o doc
   do próprio campo prevê (*"consumer entries inserted before their producer"*).
   Nenhum defeito de indexador existe.

Também corrigido durante a auditoria: meu parser tratou `NotApplicable` como FAIL
nas dims F2.5/F4.5 — são dims de manifesto, não de arquivo. Zero P0 FAIL real.

## 7. ACTIONS

**Fechado nesta auditoria** — F-1, F-2, F-3 corrigidos, com 6 testes de regressão
sobre a forma real e todos os gates verdes.

**Pendente, ação minha:** nenhuma.

**Concluído após a auditoria** — **deploy** (`update-touring`, 04/08 21:55,
`UPDATE_EXIT=0`, build 6m59s, doctor 6/6, e2e 0.8739 pass). Verificado por
execução, não por metadado: `memory store --reward` grava a coluna (0.85) e sem
a flag ela fica `NULL`; `memory credit` fecha o laço com o blend previsto
(0.85 + α·(1.0−0.85) = **0.88**); e o canal `cases` serve reparos cujo
`resolution_input` é **objeto** — a prova de que o conserto do F-1 vale sobre o
dado real, não sobre a fixture.

**F-4 · encontrado durante a verificação do deploy · corrigido**

`touring --version` reportava `built: 18:36:50Z` para um binário escrito às
21:55:51 — 6h32 de deriva. Causa lida em `build.rs:28-29`: o script só declara
`rerun-if-changed` para `Cargo.toml` e para si mesmo, então um rebuild motivado
por edição de fonte não o re-executa e `VERGEN_BUILD_TIMESTAMP` congela. É
exatamente o campo que se consulta para responder *"estou rodando o binário que
acabei de buildar?"* — a pergunta que o gotcha de daemon `(deleted)` já custou
caro. Forçar o rerun foi **rejeitado**: recompilaria `touring-server` inteiro a
cada build. Correção (REGRA #0, alarga): campo novo `binary: <ts> (mtime)` lido
do executável em disco a cada invocação, que não tem como congelar; um teste
fixa que ele é leitura de disco e não constante de compilação. **Em disco, não
no binário em execução** — só entra no ar no próximo `update-touring`.

**Pendente, decisão do Gabriel:**

1. **Rotacionar `GEMINI_API_KEY`** — purgada do histórico em 02/08, mas
   comprometida no provedor até ser trocada. Maior severidade em aberto.
2. **Redeploy** para ativar o F-4 (campo `binary:`) — cosmético; pode esperar o
   próximo ciclo.
3. Backups legados (177 MB + 36 KB) com disco em 90%.
4. 113 versões duplicadas transitivas (F4.5 Warn).
5. Propagar aos 3 projetos pinados exige `scripts/propagate-release.sh` — o
   `update-touring` cobre só a camada L1.

---

_Conduzida sob REGRA #21 (toda falha observada é corrigida, independente de
autoria) e REGRA #0 (a correção potencializa: `json_field_as_text` alarga o
contrato, o crédito federado amplia o alcance). Antecessoras:
`cross-audit-2026-08-03.md`, `cross-audit-2026-08-04.md`._
