# TACO Iter37 — EC53: entry_points wired como Signal 6d em compose_edit_context

**Data**: 2026-04-11
**Iteração**: 37
**EC implementado**: EC53
**Arquivos modificados**: `crates/touring-hooks/src/pre_edit.rs`
**Resultado**: 0 erros cargo check, 1452 tests passing (touring-hooks), sem regressão

---

## EC53 — `ecosystem::entry_points()` wired como Signal 6d em `compose_edit_context`

### Problema
`entry_points()` em `ecosystem.rs` tinha 0 callers em produção — apenas chamado em testes
(`wiring.rs:643`, `ecosystem.rs:169`). A função retorna todos os arquivos registrados como
`entry_point` ou `library` no `module_ecosystem` — os "anchors" do projeto.

O sinal de edição de entry points estava completamente ausente do pipeline de pre_edit:
Claude podia editar `src/lib.rs` ou `src/main.rs` sem receber aviso de blast radius máximo.

### Mudança

**pre_edit.rs** — Signal 6d adicionado em `compose_edit_context`, após Signal 6c:

```rust
// ── Signal 6d: Entry point guard — warn when editing a project anchor file ──
// EC53: First production caller of ecosystem::entry_points().
// Entry points (main.rs, lib.rs) are the project's public API boundary —
// editing them has maximum blast radius. This signal surfaces that context.
{
    let eps = crate::ecosystem::entry_points(db);
    if eps.iter().any(|ep| ep == file_path) {
        parts.push(format!(
            "entry-point: '{}' is a project anchor ({} registered) — edits here have maximum blast radius",
            file_path,
            eps.len(),
        ));
    }
}
```

### Design decisions
- **Condicional**: sinal só dispara quando o arquivo sendo editado É um entry point registrado
  (evita ruído desnecessário em edições de arquivos internos)
- **Contagem contextual**: `eps.len()` no texto dá ao LLM noção de quantos anchors existem
  no projeto — contexto para calibrar a cautela
- **Natural fit**: Signal 6c já usa `low_integration_modules`, Signal 6d é o complemento —
  enquanto 6c alerta sobre módulos SUBINTEGRADOS, 6d alerta sobre módulos que são ANCHORS
- **Registro pré-requisito**: entry points precisam ser registrados via `register_module`
  (já chamado em `post_read.rs:115`) para aparecerem no resultado

### Impacto
- `entry_points()` tem agora **1 caller real em produção** (era 0) — via `pre_edit.rs`
- Signal 6d passa a ser emitido quando Claude edita `src/lib.rs`, `src/main.rs`, etc.
- Pipeline de pre_edit agora cobre o quadrante oposto ao Signal 6c:
  - 6c: "este módulo está SUBINTEGRADO" (baixo score)
  - 6d: "este módulo é um ANCHOR" (máximo blast radius)

---

## Validação

```
cargo check --workspace       → Finished (0 errors)
cargo test -p touring-hooks   → 1452 passed, 0 failed, 1 ignored
```
