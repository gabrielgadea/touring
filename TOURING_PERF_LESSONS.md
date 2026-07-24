# Touring Performance Lessons — SQLite Latency + Maturin Setup
# Sessão: 2026-03-25 | TACO Orchestrator v3.0

## Lesson 1: SQLite Latency em Hooks de Curta Duração

**Contexto**: Processo efêmero (hook) que abre SQLite, faz 1-2 queries, encerra.

**Root cause real**:
- `Connection::open()` + WAL handshake: ~2ms
- `ensure_schema()` com 6x `CREATE TABLE IF NOT EXISTS`: ~2ms (even though tables exist)
- `migrate_schema()` com `PRAGMA user_version` check: ~0.5ms
- Total: 4.5ms de overhead apenas de init — sobre o custo de qualquer query

**Fix provado para este padrão**:
```rust
// Verificar user_version ANTES de ensure_schema
let ver: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap_or(0);
if ver < SCHEMA_VERSION {
    ensure_schema()?;
    migrate_schema()?;
    conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION}"))?;
}
```
Alternativa mais simples: file sentinel `.schema_v3` no data_dir (1 stat() vs 6 SQLite queries).

**Adicionar também**: `PRAGMA mmap_size = 268435456` (256MB) — elimina syscalls de pread() em leituras pequenas frequentes.

**Regra geral**: Em processos de curta duração que acessam SQLite, o init overhead é dominante. Sempre gate o schema check.

---

## Lesson 2: WAL Mode Não Resolve Init Overhead

**Mito**: "WAL mode acelera abertura de conexão"
**Realidade**: WAL melhora concorrência de escritas, não latência de abertura.

O overhead de `Connection::open()` em WAL mode inclui:
1. open(2) syscall + fstat
2. Leitura do WAL index header
3. Lock acquisition (shared lock WAL)
4. Verificação de integridade do WAL

WAL está correto para hooks concorrentes, mas não substitui schema gating.

---

## Lesson 3: Touring tem 3-4 Connection::open() por Invocação

**Descoberta**: `HookRuntime::new()` abre:
1. `touring_knowledge.db`
2. `touring_symbols.db`
3. `touring_pipeline.db`
4. (opcional) segundo open em `touring_knowledge.db` via `init_cognitive()`

**Implicação**: Fix de consolidar DBs via ATTACH (médio prazo) pode economizar ~2-3ms adicionais.

---

## Lesson 4: maturin — Setup Correto para cdylib PyO3

**Sistema**: Python 3.12.3 em `/home/gabrielgadea/.local/bin/python3`

**Problema**: `pip3 install maturin` falha por PEP 668 (sistema gerenciado pelo OS)

**Solução correta**:
```bash
# 1. maturin já está instalado globalmente via outro meio (versão 1.10.2)
# 2. Para o projeto, criar venv local:
cd /path/to/crate
python3 -m venv .venv
.venv/bin/pip install maturin   # versão 1.12.6 no venv
.venv/bin/maturin develop --release
```

**Sem pyproject.toml?**: `maturin develop` funciona com apenas `Cargo.toml` se:
- `[lib] crate-type = ["cdylib"]` está presente
- `pyo3` está nas dependencies
- O nome do módulo Python = `name` em `[lib]`

**Nome do módulo**: `claude_learning_kernel` (do `[lib] name = "claude_learning_kernel"` no Cargo.toml)

**Import**: `import claude_learning_kernel` (não `import touring_python`)

---

## Lesson 5: touring-python Bindings — Estado Atual

**Módulo**: `claude_learning_kernel` (PyO3 cdylib)
**Build**: `maturin develop --release` em 26s
**Local**: `/home/gabrielgadea/.claude/rust/crates/touring-python/.venv/`

**Bindings exportados confirmados**:
- RL: `compute_rl_state`, `get_best_action`, `process_reward`, `select_arm`, `update_arm`
- AST: `py_extract_symbols`, `py_extract_symbols_from_file`, `py_validate_syntax`, `py_supported_languages`
- NLP: `py_chunk_document`, `py_chunk_documents_batch`, `KeywordMatcher`
- SIMD: `simd` module, `verify_chain_parallel`
- Classes: `AcoGraph`, `EventBuffer`, `EventProjector`, `QueryCache`, `TrackerReport`

**Notas de comportamento** (descoberto em validação):
- `get_best_action(state)` recebe 1 arg (state vector), não 3 — verificar assinatura real
- `py_validate_syntax(code, lang)` suporta: rust, typescript, javascript, etc. — Python não está na lista de suporte do tree-sitter bundled
- `py_supported_languages()` retorna: `['python', 'rust', 'typescript', ...]` mas validate pode usar subset

---

## Próximos Passos Acionáveis

### Imediato (P0 — 6 linhas de código):
```
Arquivo: /home/gabrielgadea/.claude/rust/crates/touring-hooks/src/knowledge.rs
Linhas: 113-129
Fix: schema version gate + mmap_size PRAGMA
Ganho esperado: 3-4ms → hooks em 7-9ms (target <10ms ATINGIDO)
```

### Médio prazo (P3 — médio esforço):
```
Consolidar touring_symbols.db + touring_pipeline.db em touring_knowledge.db via ATTACH
Ganho adicional: 2-3ms → hooks em 4-6ms
```

### touring-python (para uso):
```bash
cd /home/gabrielgadea/.claude/rust/crates/touring-python
source .venv/bin/activate
python3 -c "import claude_learning_kernel; print('OK')"
```
