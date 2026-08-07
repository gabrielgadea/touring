---
okf_version: "1.0"
type: Diagnostic
title: "Auditoria dos hooks Pre/PostToolUse que tocam o índice Tantivy"
description: >
  Classificação dos 41 call sites de global_tantivy() por disponibilidade da raiz
  de projeto, com foco no caminho de ESCRITA (post-edit/post-write → stream actor).
tags: [tantivy, hooks, per-project, auditoria, f3]
timestamp: 2026-08-03T13:40:00-03:00
plan_id: task_1785772241994108046
---

# Auditoria — hooks e indexação Tantivy

Primeiro entregável da **F3** (a estratégia manda auditar os 41 sites *antes* de
converter). Plano: [/strategy-2026-08-03-tantivy-per-project.md](./../strategy-2026-08-03-tantivy-per-project.md).

## 1. Hooks registrados que alcançam o Tantivy

Do `settings.json`, os que importam para indexação:

| Evento | Hook | Relação com o índice |
|---|---|---|
| **PostToolUse** `Edit` | `post-edit` | **ESCRITA** — `try_send_symbol` → actor, com fallback síncrono `upsert_symbol` |
| **PostToolUse** `Write` | `post-write` | **ESCRITA** — idem |
| PreToolUse `Read` | `pre-read` | leitura (enriquecimento de sinais) |
| PreToolUse `Edit`/`Write` | `pre-edit`, `pre-write` | leitura (sinais de qualidade) |
| PreToolUse `Grep`/`Glob` | `pre-grep`, `pre-glob` | leitura (D43) |
| SessionStart | `session_hooks` | leitura |

## 2. Os 41 call sites por classe

| Classe | Arquivos | n | Situação |
|---|---|---:|---|
| **1 — root já em uso** | `cli/tantivy.rs` (6), `session_hooks.rs` (4), `cli/handlers/index.rs`, `cli/memory.rs`, `cli_e2e.rs` | 13 | `rt.project_root` **já é usado no mesmo arquivo** — conversão mecânica |
| **1b — root disponível** | `post_edit.rs`, `post_write.rs` | 2 | usam `runtime.project_root` em `make_relative` poucas linhas acima |
| **2 — MCP** | `tools_tantivy.rs` | 5 | sessão MCP; precisa de param ou `normalize_project_root(cwd)` |
| **3 — sem root aparente** | `lifecycle/*` (8), `signals.rs` (5), `tantivy_stream.rs` (3), `metadata_collector.rs`, `handlers/mcp.rs` | 18 | inspeção caso a caso |
| — | `tantivy_index_tests.rs` | 2 | testes |

`signals.rs` (5) e `metadata_collector.rs` são **somente leitura** (`global_tantivy()?`
seguido de busca) — conversão simples assim que a assinatura da função receber a raiz.

## 3. ⚠ O achado: a raiz é **apagada**, não apenas não-passada

O caminho de escrita real é assíncrono:

```
post_edit / post_write  ──try_send_symbol(SymbolDoc)──►  STREAM_TX  ──►  actor  ──►  flush_buffer
   (TEM runtime.project_root)         canal global          (sem contexto)      global_tantivy()
```

```rust
// tantivy_stream.rs:44
static STREAM_TX: OnceLock<mpsc::Sender<SymbolDoc>> = OnceLock::new();
```

**Um único canal global.** Hooks de projetos diferentes empurram para o mesmo
`mpsc`, e o actor drena tudo para um índice só. `SymbolDoc` carrega apenas
`file_path` **relativo** e `crate_name` — nenhuma referência ao projeto.

Isso distingue este caso de todos os outros: nos demais a raiz existe e só não é
passada adiante; **aqui ela é descartada na fronteira do canal**. Converter
`post_edit`/`post_write` para `tantivy_for(Some(root))` **não** resolveria — o
caminho quente é o `try_send_symbol`, e ele perde a informação logo em seguida.

É também o mecanismo que **produz** a contaminação e a eviction documentadas na
estratégia: os documentos de todos os projetos chegam misturados ao mesmo writer.

### Forma da correção (F3)

O payload do canal precisa carregar a raiz, e o flush precisa agrupar por ela:

```rust
static STREAM_TX: OnceLock<mpsc::Sender<(PathBuf, SymbolDoc)>>;

fn flush_buffer(buffer: &mut Vec<(PathBuf, SymbolDoc)>) {
    // agrupar por raiz → resolver tantivy_for(Some(root)) por grupo →
    // um commit POR ÍNDICE (commit é por índice; commitar por documento
    // multiplicaria o custo do fsync)
}
```

O agrupamento não é detalhe de performance: `commit()` é por índice, então um
buffer heterogêneo sem agrupamento faria N commits em vez de um por projeto.

## 4. Ordem de conversão que isto sugere

A estratégia previa 1 → 2 → 3 por dificuldade. A auditoria **reordena**: o
`tantivy_stream` deve vir **primeiro dentro da F3**, porque é o caminho de
escrita e porque é o único que exige mudança de *forma* (o tipo do canal), não só
de assinatura. Converter os leitores antes deixaria escrita e leitura apontando
para índices diferentes durante a janela — o pior estado possível.

Ordem revisada da F3:

1. `tantivy_stream` + `post_edit`/`post_write` (a escrita, junta e coerente)
2. Classe 1 restante (leitores com root já em uso)
3. Classe 2 (MCP)
4. Classe 3 restante (`lifecycle/*`, `signals.rs`, `metadata_collector.rs`)

## 5. Evidência

```
call sites fora do módulo: 41
STREAM_TX: OnceLock<mpsc::Sender<SymbolDoc>>          # tantivy_stream.rs:44
post_write.rs:292  try_send_symbol(doc.clone())
post_edit.rs:631   try_send_symbol(doc.clone())
daemon.rs:618      spawn_stream_actor()
flush_buffer()     global_tantivy() → upsert_symbol ×N → commit
```
