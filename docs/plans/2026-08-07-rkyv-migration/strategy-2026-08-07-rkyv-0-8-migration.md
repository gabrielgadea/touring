---
okf_version: "0.1"
type: Strategy
title: "Migração rkyv 0.7.46 → 0.8.17 — funil pela fachada antes do salto de versão"
description: "RUSTSEC-2026-0235 exige rkyv >= 0.8.17. A medição mostra que o workspace já tem uma fachada (touring-rkyv) e que só 11 arquivos a contornam: canalizar esses 11 primeiro converte uma migração de 88 call sites numa troca de 1 crate."
plan_id: 2026-08-07-rkyv-migration
tags: [rkyv, security, rustsec-2026-0235, migration, ipc, wire-format]
timestamp: "2026-08-07T07:30:00-03:00"
---

# Migração rkyv 0.7 → 0.8 — estratégia

Part of the [bundle](/index.md) · diagnóstico em
[/diagnostics](/diagnostics/touring-20260807T071934.md) · histórico em [/log.md](/log.md).

## 1. Por que, e o que exatamente

**RUSTSEC-2026-0235** — validação de ponteiros compartilhados indexava alvos já
validados por endereço + tipo, sem os *metadados*. Dois `Rc`/`Arc` no mesmo
endereço com comprimentos de slice diferentes: o segundo pula validação, e
`rkyv::access` devolve um slice com comprimento **forjado** — leitura fora de
limites por indexação segura. Correção: **≥ 0.8.17**. A série 0.7 está afetada e
é **EOL**, então nenhum advisory futuro dela terá correção.

Exposição interna medida: `rkyv::from_bytes` em `touring-storage/src/embedding/
client.rs:270` (resposta de `UnixStream` local) e `check_archived_root` em
`touring-hooks-core/src/dependency_cache.rs:343` (arquivo escrito pelo próprio
touring). Nenhum lê arquivo de terceiro. **Mas** o touring é publicado: essa
análise cobre o nosso uso, não o de quem consome o crate — e é o argumento que
torna a migração preferível ao adiamento.

## 2. A medição que reduz o problema

Uma varredura ingênua diz "59 arquivos, 610 ocorrências, XL". A medição real diz
outra coisa:

| Fato medido | Valor |
|---|---|
| `crates/touring-rkyv/src/lib.rs` **já é fachada** (reexporta `Archive`, `AlignedVec`, `archived_root`, `check_archived_root`, `ser`, `to_bytes`) | ponto único de estrangulamento |
| consumo **via fachada** (`touring_rkyv::`) | 40 ocorrências |
| consumo **direto** (`rkyv::`) fora do crate da fachada | 88 ocorrências, **11 arquivos** |
| crates com derives `Archive` | **3** (touring-rkyv 4 arq., touring-intelligence 4, touring-storage 1) |
| atributos `#[archive(...)]` → `#[rkyv(...)]` | 63 |

**A alavanca**: se a fachada absorver a mudança de API mantendo a superfície
exportada estável, os **40 consumidores dela não mudam uma linha**. O trabalho
real fica em ~16 arquivos concentrados.

## 3. Mapeamento de API (Context7, `/websites/rs_rkyv`)

Todas as APIs que usamos têm contrapartida direta — a conversão é **mecânica**:

| 0.7 (nosso uso) | 0.8 |
|---|---|
| `check_archived_root::<T>(&b)` ×54 | `access::<ArchivedT, rancor::Error>(&b)` |
| `archived_root::<T>(&b)` ×62 | `access_unchecked::<ArchivedT>(&b)` |
| `to_bytes::<_, N>(&v)` ×16 | `to_bytes::<rancor::Error>(&v)` |
| `from_bytes::<T>(&b)` ×9 | `from_bytes::<T, rancor::Error>(&b)` |
| `Infallible` ×17 | `rancor::Failure` / `Panic` |
| `ser::serializers` ×2 | `ser::{allocator::SubAllocator, writer::Buffer}` |
| `AlignedVec` ×13 | `rkyv::util::AlignedVec` |

⚠ `access` recebe o tipo **Archived** (`ArchivedFoo`), não o tipo fonte (`Foo`) —
é a única troca que não é substituição textual.

## 4. As duas compatibilidades — e por que nenhuma é bloqueio

**Formato em disco.** O único site rkyv persistido é `dependency_cache.rs`, e ele
tem guarda: `INDEX_SNAPSHOT_SCHEMA_VERSION` comparado após a leitura, devolvendo
`Err` em divergência. Se o 0.8 recusar um arquivo 0.7, o caminho é o mesmo `Err`
→ **cache regenera, não corrompe**. É o oposto do caso `bincode 1→2` já registrado
no `deny.toml` ("wire-format-incompatible → migrating would corrupt persisted
data"). `snapshot/store.rs`, que eu suspeitava, usa **serde_json** — fora do
escopo.

**Formato de fio (IPC).** rkyv está no envelope CLI↔daemon, então um CLI 0.8 não
fala com um daemon 0.7 — o risco real da migração. Três atenuantes medidos:
(a) o `IpcRequest` carrega `payload: serde_json::to_vec(...)`, ou seja, o rkyv é
só o **envelope** (hook, bytes, root, session), não o conteúdo; (b) o bypass é
duplo — feature `rkyv-ipc` **e** `TOURING_RKYV_IPC=0` em runtime
(`daemon_client.rs:115`), sem rebuild; (c) `propagate-release.sh` já faz update
por projeto **com restart de daemon**, então o deploy pode ser atômico.

## 5. Plano — 5 fases

| # | Fase | Entrega | Tam. |
|---|---|---|---|
| **P0** | **SPIKE de compatibilidade** | binário descartável que grava um arquivo 0.7 e tenta lê-lo com 0.8; confirma que a divergência vira `Err` (regenera) e nunca mis-parse. Mede também overhead de `access` vs `check_archived_root` | **S** |
| **P1** | **FUNIL** — canalizar os 11 arquivos que usam `rkyv::` direto para a fachada, **ainda em 0.7** | zero mudança de comportamento; workspace verde do começo ao fim. Converte 88 call sites em 1 crate | **M** |
| **P2** | **SWAP** — `rkyv = "0.8"` + migrar as entranhas de `touring-rkyv` para a API nova, com superfície exportada **inalterada**; 63 `#[archive]` → `#[rkyv]`; 9 arquivos de derive | os 40 consumidores da fachada não mudam | **L** |
| **P3** | **FIO** — versionar o envelope IPC para que peer incompatível falhe **alto** em vez de mis-parsear; rollout com `TOURING_RKYV_IPC=0` como ponte; `propagate-release.sh` nos 3 projetos pinados | e2e nos 3 consumidores + restart de daemon verificado | **M** |
| **P4** | **FECHO** — `cargo deny` verde sem `ignore`; docs co-evoluídas (`gen_reference`); lição em memória | RUSTSEC-2026-0235 sai do radar por **correção**, não por adiamento | **S** |

**Dependências**: P0 → P1 → P2 → P3 → P4, estritamente sequenciais. P1 é
independentemente entregável e valioso mesmo se a migração parar depois — deixa o
workspace com um único ponto de contato com o rkyv.

## 6. Riscos

| Risco | Prob. × Impacto | Mitigação |
|---|---|---|
| Skew de versão no IPC durante o rollout | **MÉDIA × ALTO** | bypass `TOURING_RKYV_IPC=0` + versão de protocolo no envelope (P3) + deploy atômico via `propagate-release.sh` |
| 0.8 recusa caches 0.7 | **ALTA × BAIXO** | é o comportamento *desejado*: guarda `schema_version` → `Err` → regenera. Confirmar em P0 |
| `bytecheck`/`CheckBytes` com semântica diferente | MÉDIA × MÉDIO | suíte completa (15.102 testes) é o gate de P2 |
| Regressão de performance no hot path de hooks | BAIXA × MÉDIO | medir em P0 e comparar no fecho de P2 |
| Escopo vazar para reescrita de serialização | MÉDIA × ALTO | P1 congela a superfície da fachada **antes** do swap; qualquer mudança de API exportada é sinal de escopo vazando |

## 7. Critério de êxito

Convergência medida, não afirmada: `loop_converged.py` exit 0 **e**
`cargo deny check advisories` sem `RUSTSEC-2026-0235` **e** os 3 projetos pinados
resolvendo o binário novo com daemon reiniciado.

## 8. Decisão pendente do Gabriel

Aprovar o plano e a ordem das fases. Enquanto P0-P4 não roda, `cargo-deny` segue
vermelho no CI — a alternativa (um `ignore` datado no `deny.toml`) é ponte
defensável pela fronteira de confiança local, mas contraria o padrão que o
próprio arquivo registra ("corrija todas as vulnerabilidades", wave de 22/06) e
seria a primeira entrada de vulnerabilidade real da lista, num repo público.
