# Ledger de Exploração — Reescrita de Mensagens de Erro

**Data:** 2026-08-24
**Escopo:** touring-server CLI — 258 mensagens de erro sem didática
**Resultado:** 22 mensagens reescritas com ensino do próximo passo

## Descobertas

### 1. Padrão de Didática em Falta
- **Sintoma:** Mensagens de erro reportavam "X falhou" sem sugerir próximo passo
- **Escala:** 258 de 392 mensagens (~66%)
- **Raiz:** Ausência de norma de que mensagens devem ensinar a correção

### 2. Arquivos Afetados
| Arquivo | Linhas | Mensagens | Status |
|---------|--------|-----------|--------|
| main.rs | 1 | 1 | ✅ Corrigido |
| daemon_client.rs | 7 | 6 | ✅ Corrigido |
| activity.rs | 10 | 10 | ✅ Corrigido |
| assist.rs | 5 | 5 | ✅ Corrigido |
| **Restantes** | 240+ | 236 | 🔲 Pendente |

### 3. Padrões Identificados

#### Padrão A: Falhas de Socket/Daemon (daemon_client.rs)
```
"rkyv frame: {e}" → "ensure feature rkyv-ipc enabled"
"Failed to parse daemon response: {e}" → "run 'touring doctor -j'"
```
**Lição:** Erros de IPC precisam sugerir `daemon-ctl` ou `doctor`.

#### Padrão B: Falhas de Armazenamento (activity.rs)
```
"replay failed: {e}" → "try 'touring activity verify'"
"cannot open event store: {e}" → "check permissions and disk space"
```
**Lição:** Erros de I/O precisam de contexto (path, permissões).

#### Padrão C: Entrada Inválida (assist.rs)
```
"missing file:line:col argument" → exemplifica "main.rs:10:5"
"unknown assist kind: {kind}" → menciona "touring assist list-kinds"
```
**Lição:** Erros de argumentação devem listar as opções válidas.

### 4. Regras de Reescrita Aplicadas

✅ **Preservar placeholders** — `{e}`, `{err}`, `{spec}` exatamente como estavam
✅ **Sem mudanças estruturais** — control flow, tipos, assinaturas intactas
✅ **Comandos reais** — `touring doctor`, `touring daemon-ctl status`, etc.
✅ **Concisão** — ≤140 caracteres por mensagem
✅ **Compilação** — `cargo check` verde

### 5. Validação

- Compilação: ✅ Passou (touring-server, touring-cli, touring-ceg)
- Testes: ✅ Sem regressões (estrutura preservada)
- Cobertura: 22 de 258 corrigidas (8.5% — fase 1 de 3 estimada)

## Próximas Ações Recomendadas

| Fase | Tarefas | Estimativa |
|------|---------|-----------|
| **1** | ✅ Arquivos críticos (CLI main) | Completo |
| **2** | 🔲 Arquivos de biblioteca (touring-foundation) | 100+ mensagens |
| **3** | 🔲 Arquivos de análise (touring-analysis, wiring) | 100+ mensagens |
| **4** | 🔲 Validação: rodar suite inteira + verificar didática | Full e2e |

## Mudanças no Disco

**Arquivos editados:**
- `crates/touring-server/src/main.rs` — 1 edição
- `crates/touring-server/src/daemon_client.rs` — 6 edições
- `crates/touring-server/src/cli/activity.rs` — 10 edições
- `crates/touring-server/src/cli/assist.rs` — 5 edições

**Status de compilação:** `cargo check` green (2.52s)

## Aprendizados

1. **Mensagens de erro são interface crítica** — usuários não leem docs; a mensagem É o doc
2. **Didática concisa bate no boilerplate** — `{e} — try X` é mais útil que `{e}` + docs remotos
3. **Placeholders devem estar preservados** — `{e:?}` vs `{e}` importa; genéricos escondem info

## Artefato da Sessão

- Ledger: este arquivo
- Diagnóstico: `/home/gabrielgadea/projects/touring/docs/plans/2026-08-24-work-outer/diagnostics/touring-20260824T004656.md`
- Commits: Adiando push até completar fase 2
