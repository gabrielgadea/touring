---
okf_version: "1.0"
type: AuditReport
title: "Cross-audit 2026-08-04 — auditoria cruzada completa da sessão (tantivy per-project + SIGPIPE + correções da 1ª auditoria)"
description: "Segunda auditoria cruzada da sessão. Audita o escopo inteiro implementado, incluindo as correções emitidas pela auditoria de 03/08. 3 defeitos reais encontrados e corrigidos, cada um provado por execução."
tags: [cross-audit, tantivy, per-project, sigpipe, purpose-fidelity, regra-21]
timestamp: "2026-08-04T14:40:00-03:00"
plan_id: 2026-08-02-excelencia-touring-cli
---

# Cross-audit 2026-08-04 — auditoria cruzada completa

> Auditoria **da sessão inteira**, incluindo o que a auditoria de 03/08 corrigiu.
> Auditar a correção anterior foi o que produziu o achado mais importante: **uma
> das minhas correções de ontem era parcial** — o mesmo defeito sobrevivia num
> call site irmão que eu não havia mapeado.

## 1. VERDICT

**Aprovado com 3 defeitos corrigidos.** O escopo (~14.887 LOC em 19 arquivos-chave)
cumpre o propósito documentado: cada projeto tem índice Tantivy próprio, todos
graváveis simultaneamente, contaminação zero — provado por execução bidirecional.
Zero P0 BLOCK em qualquer crate tocado. Zero dívida real (TODO/`unimplemented!`/
`allow(dead_code)` novos). Zero ciclos de dependência.

O achado que justifica ter feito a segunda auditoria: **F-B era uma correção
parcial minha de 03/08**. Corrigi `ctx_doctor` para receber a raiz por parâmetro e
não verifiquei o dispatcher irmão, que continuou passando `None`.

## 2. SCORECARD

| Gate | Resultado | Evidência |
|---|---|---|
| 6 dims P0 BLOCK (F2.1/F2.4/F2.5/F2.6/F4.3/F4.5) | **0 FAIL** nos 3 crates tocados | `touring-quality score crates/<c> --dims …` |
| F4.5 pkg-mgmt | 0.700 **Warn** (não FAIL, não cabe tier) | 113 versões duplicadas transitivas — pré-existente |
| `cargo check --workspace --all-targets` | **exit 0** | executado pós-correções |
| `cargo clippy --workspace --all-targets -- -D warnings` | **exit 0** | executado pós-correções |
| `cargo test --workspace` | **exit 0**, 0 falhas | `PIPESTATUS[0]=0`; filtro de `FAILED`/`panicked` capturou 0 linhas |
| Ciclos de dependência | **0** | `touring wiring cycles` |
| Órfãos (REGRA #0) | **0/10** símbolos da sessão | grep preciso, todos com consumidor real |
| `touring e2e -j` | **0.8757 pass** | acima do baseline 0.8749 |
| `touring doctor` | **6/6 OK** | |

## 3. FINDINGS — os 3 defeitos, todos confirmados por execução

### F-A · BLOCK · `tantivy_index.rs:1829` — placeholder codificado no lugar do diretório real

`ToolOutputsIndex::writer_guard` logava `Path::new("tool_outputs")` — uma string
fixa — porque a struct **não tinha** campo `index_dir`. Confirmado por contagem:
`TantivyIndex` tinha 1 ocorrência do campo, `ToolOutputsIndex` tinha 0.

Efeito: em degradação para leitura, o log dizia `dir=tool_outputs` em vez do
projeto real — inútil exatamente no momento em que o diagnóstico importa, e
enganoso numa frota de 4 projetos.

**Correção (potencializa, REGRA #0)**: campo `index_dir: PathBuf` adicionado; o
guard passou a reportar `&self.index_dir`. O log ganhou informação em vez de
perder código.

### F-B · BLOCK · `handlers/mcp.rs:1008` — minha correção de 03/08 era parcial

```rust
"doctor" => ctx_doctor(None),   // ← o dispatcher de batch
```

Ontem corrigi `ctx_doctor` para receber `project_root: Option<&Path>`. O
dispatcher de batch continuou passando `None`, caindo no cwd — que **dentro do
daemon é o cwd do daemon**, não o de quem perguntou. Um `doctor` via batch
reportava o projeto errado.

**Correção**: `ctx_batch_execute(project_root: Option<&Path>, items: &[Value])`;
a raiz atravessa o dispatcher.

**Lição institucional**: uma correção de assinatura só está completa quando *todos*
os call sites foram enumerados pelo compilador, não por grep. Foi exatamente o
erro que cometi na sessão com `signals.rs` (grep disse 2 consumidores; o compilador
achou 5 arquivos / 15 chamadas) — e reincidi aqui. O grep serve para planejar; o
compilador é o enumerador exaustivo.

### F-C · WARN · `reset_tool_outputs_global` — vazamento proporcional a resets

A função limpava `TOOL_OUTPUTS_REGISTRY`, abandonando entradas criadas com
`Box::leak`. Cada reset vazava um índice inteiro, e o vazamento crescia com o
número de **resets**, não de projetos — ilimitado em processo longo.

**Correção**: o reset limpa apenas `LAST_ATTEMPT` (o throttle, que é o propósito
documentado); o registry preserva as entradas vazadas, que continuam alcançáveis
e reutilizáveis.

## 4. FUSED RISK — o que restou, ranqueado

| Unidade | Risco | Natureza |
|---|---|---|
| 113 versões duplicadas (F4.5 = 0.700 Warn) | baixo, amplo | dívida transitiva pré-existente; decisão do Gabriel |
| Grafo release-TEST do `touring-server` (E0460/E0463) | contido | débito conhecido, documentado no CLAUDE.md do workspace; grafo normal e debug-test compilam |
| Backups legados (177 MB + 36 KB) com disco em 90% | operacional | decisão do Gabriel — apagar ou reter |

Nenhum é regressão desta sessão.

## 5. ROOT-CAUSE — a alavanca contrafactual

Os três defeitos compartilham **uma** causa: *estado que deveria ser por-projeto
sendo tratado como global*. F-A logava um nome global; F-B resolvia a raiz pelo
cwd global; F-C tratava o registry como cache descartável em vez de dono de
memória vazada. É o mesmo erro que a migração inteira existiu para corrigir —
o que faz sentido: a migração converteu 41 call sites, e defeitos residuais se
concentram justamente na fronteira que ela moveu.

Isso prediz onde procurar numa terceira auditoria: qualquer lugar que ainda
derive contexto de processo (cwd, `HOME`, singleton) em vez de recebê-lo por
parâmetro.

## 6. PROVENANCE — evidência executada, não afirmada

```
test a_second_tool_outputs_opener_degrades_to_read_only ....................... ok
test reset_keeps_the_registry_so_leaked_indices_stay_reachable ............... ok
test batch_doctor_reports_the_given_root_not_the_process_cwd ................. ok
test result: ok. 5 passed; 0 failed
```

O teste de F-B carrega a **contraprova**: com `Some(raiz)` reporta a raiz, com
`None` **não** reporta. Sem essa segunda metade ele passaria mesmo com a correção
revertida — e não provaria nada.

**Isolamento da frota, bidirecional:**

| símbolo | touring | analise | konverter |
|---|---|---|---|
| `tantivy_for` (nativo do touring) | **3 hits** (def. em `tantivy_index.rs:1361`) | `[]` | `[]` |
| `DataFrame` (nativo do analise) | **0** | **2 hits** | — |

Ambas as direções importam: um símbolo ausente em todo lugar não provaria nada
(erro que cometi antes nesta mesma sessão e corrigi).

**Frota — 4 índices independentes, 432,7 MB:**

```
touring                  83 arquivos    23,5 MB
analise                  70 arquivos   282,9 MB
transferegov_pipeline    88 arquivos    97,9 MB
konverter                40 arquivos    28,4 MB
```

Caminho lido de `index_dir_for` (`tantivy_index.rs:1328`), não presumido — meu
primeiro palpite (`tantivy_index/`) estava errado e a checagem falhou nos 4
projetos até eu ler a fonte.

## 7. ACTIONS

**Fechado nesta auditoria** — F-A, F-B, F-C corrigidos, testados e com gates verdes.

**Pendente, ação minha:** deploy das 3 correções (estão em disco, não no binário em
execução) — precisa do gate humano do Gabriel, é `update-touring`.

**Pendente, decisão do Gabriel:**

1. **Rotacionar `GEMINI_API_KEY`** — a chave foi purgada do histórico em 02/08 mas
   permanece comprometida até ser rotacionada no provedor. É o item de maior
   severidade em aberto e só o senhor pode executá-lo.
2. Backups legados (177 MB + 36 KB) com disco em 90% — apagar ou reter.
3. 113 versões duplicadas (F4.5 Warn) — atacar ou aceitar como dívida transitiva.

---

_Auditoria conduzida sob REGRA #21 (toda falha observada é corrigida, independente
de autoria ou idade) e REGRA #0 (correção potencializa; jamais reduz escopo para
silenciar aviso). Antecessora: `cross-audit-2026-08-03.md`._
