---
okf_version: "1.0"
type: AuditReport
title: "Cross-audit — tudo implementado na sessão 2026-08-03"
description: >
  Auditoria de fidelidade de propósito sobre 31 arquivos-fonte e 8 de teste
  alterados em dois programas (defeitos vivos + índice Tantivy per-project).
  Três defeitos encontrados, corrigidos e provados por execução.
tags: [cross-audit, tantivy, per-project, purpose-fidelity, regra-0]
timestamp: 2026-08-03T16:20:00-03:00
plan_id: task_1785772241994108046
---

# Cross-audit — sessão 2026-08-03

## 1. VERDICT

**PASS com 3 defeitos corrigidos.** A superfície implementada cumpre o propósito
documentado; a auditoria encontrou três desvios reais — **dois deles introduzidos
nesta própria sessão** — todos corrigidos e provados por execução, nenhum
suprimido.

O achado mais grave (**F-1**) era um defeito de fidelidade que eu havia
introduzido horas antes, na F5: uma sondagem de saúde que respondia sobre o
projeto **errado** e que, num daemon per-project, teria ficado invisível.

## 2. SCORECARD

| Eixo | Resultado |
|---|---|
| Superfície auditada | 31 arquivos-fonte + 8 de teste, 6 crates |
| 6 dims P0 BLOCK | **0 FAIL** (F2.5 = 1.000 Pass no escopo de crate) |
| Órfãos novos (REGRA #0) | **0** |
| Ciclos de dependência | **0** |
| Débito humano introduzido | **0** |
| `cargo clippy --workspace -D warnings` | exit 0 |
| `cargo test --workspace` | exit 0 |

### Falso positivo que a auditoria descartou

`F2.5` e `F4.5` pontuaram **0.000 (Unranked)** contra um arquivo `.rs` isolado.
Não é falha: são dims de **workspace** (CVEs de dependência, gestão de pacote).
Verificado — um arquivo NÃO tocado (`throttle.rs`) dá o mesmo 0.000, e no escopo
de crate `F2.5 = 1.000 Pass` (0 CVEs) e `F4.5 = 0.700 Warn` (as 113 versões
duplicadas, débito pré-existente conhecido). **Ler dim de workspace em escopo de
arquivo produz zero espúrio.**

## 3. FINDINGS

### F-1 — `ctx_doctor` reportava o projeto do daemon, não o do chamador ⛔

**Introduzido nesta sessão (F5).** Eu resolvi a raiz com
`std::env::current_dir()`. Mas `ctx_doctor` executa **dentro do daemon**, então o
cwd é o do daemon.

Evidência executada:

```
daemon global PID=1460503  cwd=/home/gabrielgadea/projects/touring
```

Uma sondagem vinda de `konverter` receberia o índice de `touring`. Pior: num
daemon **per-project** o cwd coincide com o projeto, então o erro ficaria
**invisível** justamente onde a topologia é a nova — o tipo de defeito que só
aparece quando já causou dano.

**Correção (REGRA #0 — potencializa):** `ctx_doctor(project_root: Option<&Path>)`.
O wrapper MCP passa `self.config.project_root`, que ele **sempre teve** e não
usava; o cwd fica como fallback para chamadores sem contexto.

### F-2 — teste unitário escrevia no `$HOME` real e desfazia a F5b ⛔

O teste guardião da fachada chamava `global_tantivy()`, que resolve
`$HOME/.claude/touring/tantivy` e **cria o diretório**. Cada execução da suíte
**recriava o índice legado** que a F5b tinha acabado de aposentar.

Evidência executada:

```
antes:  ausente
test result: ok. 1 passed
depois: /home/gabrielgadea/.claude/touring/tantivy   ← RECRIADO PELO TESTE
```

**Correção:** o teste aponta `HOME` para um `TempDir` (`#[serial]`, pois `HOME` é
global ao processo) e restaura o valor original **inclusive no caminho de
falha** — um `panic!` com `HOME` num tempdir removido contaminaria todo teste
subsequente do binário.

**Prova pós-fix:**
```
antes:  ausente
test result: ok. 1 passed
depois: ausente — FIX PROVADO      HOME preservado: /home/gabrielgadea
```

### F-3 — `flush_buffer` descartava documentos sem contador ⚠

**Introduzido nesta sessão (F3).** O descarte por índice indisponível tinha
apenas `tracing::debug!`. Existe contador para o descarte na **entrada**
(`backpressure_drop`), mas nenhum para o descarte no **flush** — documentos
sumiam da observabilidade. É exatamente a falha silenciosa que a partição
per-project deveria eliminar, não criar.

**Correção (REGRA #0):** novo contador
`tantivy_stream_index_unavailable_drop_count`, propagado até o snapshot JSON
(`touring gate-metrics -j`), e o log promovido de `debug!` para `warn!`. A
contabilidade agora **fecha**:

```
enqueued = flush_docs + backpressure_drop + index_unavailable_drop
```

## 4. FUSED RISK

| Unidade | Risco antes | Depois |
|---|---|---|
| `ctx_doctor` (MCP curado) | diagnóstico do projeto errado, invisível em daemon per-project | raiz por parâmetro |
| suíte de testes | desfazia migração de dados a cada execução | isolada em tempdir |
| `flush_buffer` (caminho quente) | perda de documento sem rastro | contabilizada + `warn!` |

## 5. ROOT-CAUSE

Os três compartilham uma raiz: **resolver contexto a partir de estado ambiente
global** (`current_dir`, `$HOME`, log em vez de contador) em código que roda num
processo diferente do que originou a requisição.

É a mesma classe do defeito original desta sessão — o canal do stream que
*apagava* a raiz. Corrigi o canal e reintroduzi a mesma classe em dois lugares
novos. A lição institucional: **em código de daemon, contexto vem por parâmetro,
nunca do ambiente do processo.**

## 6. PROVENANCE

Enforcement lido da fonte, não assumido:
- `apply_quality_gate` (`composite.rs:84`) — só `DimStatus::Fail` limita o tier;
  `Warn` não limita.
- `symbols_page` (`store.rs:514`) — `WHERE is_definition = 1`, o que explica
  85.152 docs contra 263.771 linhas (backfill completo, estimativa minha errada).
- `ForbiddenCallPolicy::from_env` (`ctx_execute_tools.rs:36`) — política vem só
  de env var, lida no momento da chamada.

## 7. ACTIONS

**Feitas nesta auditoria:** F-1, F-2, F-3 corrigidos e provados.

**Pendentes, para decisão humana:**

1. **`ToolOutputsIndex` tem o mesmo padrão do defeito original** — singleton
   global em `~/.claude/touring/tool_outputs/`, mesmo writer lock exclusivo, sem
   partição por projeto. Registrado desde a estratégia; segue fora de escopo por
   proporção (indexa outputs, não símbolos), mas é dívida real.
2. **113 versões duplicadas de crate** (F4.5 = 0.700 Warn) — débito
   pré-existente, majoritariamente transitivo.
3. **Disco em 90%**; o legado (177 MB) em `tantivy.legacy-1785782683` pode ser
   apagado quando Gabriel confirmar.
4. **Deploy pendente**: as três correções deste cross-audit estão em disco, não
   no binário em execução.
