---
type: Strategy
title: Reparo do subsistema de memória — 4 defeitos, 2 features
description: Causas-raiz verificadas em código para list/reindex/gotcha_stats/erro-engolido, mais separação do ruído outcome:* e a família KPI touring.memory.*.
plan_id: 2026-08-02-memory-subsystem-repair
tags: [memory, daemon, kpi, recall, cli]
timestamp: 2026-08-02T15:00:00-03:00
okf_version: "0.1"
---

# Reparo do subsistema de memória

Parte do [bundle](/index.md). Antecedente:
[avaliação da memória](/../2026-08-02-loop-memory-recovery/strategy-2026-08-02-loop-memory-recovery.md).

## Correção de um diagnóstico anterior

Na avaliação de 14:45 eu afirmei que `memory store` era **não-atômico** — que a
linha commitava e a RPC reportava falha. **Estava errado, e a correção muda o
conserto.** O handler (`ceg_impls.rs:145-162`) já é explicitamente fail-open no
ANN: *"the SQL store already succeeded; an ANN failure only sets
ann_indexed=false"*. A causa real dos "stores fantasma" é a **mesma do reindex**
— timeout de leitura do cliente (§D2). Um root cause, não dois.

## Os quatro defeitos (causas-raiz verificadas em código)

### D1 — `memory list` lê um formato abandonado · confiança 1.0

`cli_memory_list` (`touring-cli/src/cli/memory.rs:365`):

```sql
SELECT file_path, notes, read_count, COALESCE(last_read_at,'')
FROM file_knowledge WHERE file_path LIKE '__memory__:%'
```

Consulta `knowledge.db/file_knowledge` procurando entradas codificadas como
pseudo-arquivos `__memory__:tier:type:key` — encoding legado. `store`, `recall`
e `stats` usam `memory.db/memory_entries`.

**Medido**: `file_knowledge` = 4.270 linhas, com o prefixo `__memory__:` = **0**;
`memory_entries` = **6.923**. Por isso `list` sempre devolve `count: 0`.

**Fix**: reescrever contra `memory_entries` (mesma fonte de `stats`/`recall`).
`parse_memory_row` e o `ORDER BY read_count` do encoding legado ficam órfãos →
remover (REGRA #0).

### D2 — `reindex` bloqueia o ator do daemon e estoura o timeout · confiança 0.85

`cli_memory_reindex` re-embeda **todas** as 6.921 entradas de forma síncrona
dentro do ator do daemon. O cliente (`daemon_client.rs:143`) faz
`stream.read_to_end` sob `DAEMON_READ_TIMEOUT_SECS` (~15 s). Consequências
observadas ao vivo:

1. o `reindex` estoura o timeout → `success=false`;
2. o daemon **continua processando**, e todo RPC de memória subsequente fica na
   fila e também estoura — inclusive `recall`, que funcionava;
3. os `store` daquela janela **commitaram mesmo assim** (6.921 → 6.923,
   conferido por SQL) — daí a aparência de não-atomicidade;
4. `doctor` seguia 5/6 ok: a degradação era do **subsistema**, não do processo.
   Restaurado com `touring daemon-ctl restart` (REGRA #19).

**A indexação em si NÃO está quebrada** (medido após o restart): o corpus foi de
5.520 → **6.977** embeddings, **0 duplicados**, **0 entradas fora**. Ou seja, o
`reindex` *completou* o trabalho — apenas não conseguiu reportá-lo. O gap de
21 % que medi às 14:45 era **estado acumulado** de um reindex que nunca havia
rodado até o fim, não falha estrutural de indexação. Corolário: `corpus_coverage`
já subiu de 0,797 para **1,008**, e o remédio para o gap era exatamente este
comando — que era inutilizável por causa do timeout.

**Fix** (reportar e não bloquear, *não* reescrever a indexação): (a) incremental
por padrão — só as entradas ausentes do corpus, com `--all` para forçar; (b)
retorno imediato com progresso, execução em lote cedendo o ator entre chunks (ou
via `touring jobs`); (c) `--timeout` maior aceito para a variante `--all`.

### D3 — `gotcha_stats` reporta campos que não são o que medem · confiança 1.0

`gotchas.rs:197` documenta o próprio retorno: `(total_count, total_hits,
total_prevented)`. O chamador (`cli/memory.rs:54`) mapeia posicionalmente para
`total_count` / `unresolved_count` / `resolved_count`.

Logo `unresolved_count: 383.107` é a **soma de `hit_count` das 13 gotchas**, e
`resolved_count: 0` é a soma de `prevented_errors`. Não há inconsistência de
dados — há **nomes mentirosos**.

**Fix**: renomear os campos de `GotchaStats` para `total_hits` /
`total_prevented` (o que de fato se mede). Se resolved/unresolved forem
desejáveis, exigem coluna de resolução no schema — fora deste escopo, registrado.

### D4 — o CLI engole a mensagem do daemon · confiança 1.0

`daemon_client.rs:147`: `anyhow::bail!("Daemon returned success=false")` —
`response.output` (que carrega o `{"error": …}` do handler) é **descartado**.
Foi o que tornou D2 indiagnosticável e o que me levou ao diagnóstico errado
acima. Viola "falhe loud" (Princípio operacional #6).

**Fix**: propagar o `error` do payload na mensagem; sem `error`, incluir um
trecho do output.

### D5 — `--sort` do `list` nunca funcionou (achado durante a implementação) · confiança 1.0

`MemoryCmd::List` enviava `{"sort_by": sort}` mas `cli_memory_list` lê
`payload["sort"]`. A flag era **silenciosamente ignorada** e toda listagem caía
no ramo default. Mesma família de D1/D3: contrato divergente entre as duas
pontas, sem ninguém para reclamar. Corrigido junto.

## As duas features

### F1 — separar `outcome:*` do corpus de recall

Medido: 58,4 % do acervo é auto-gerado; as **8 entradas mais recuperadas do
acervo inteiro** são todas `outcome:*:failure`; automáticas são recuperadas
**2,4×** mais que lições curadas.

**Proposta**: `recall` exclui `outcome:*` **por padrão**, com
`--include-outcomes` para reativar. Os outcomes seguem indexados e consultáveis
(o hook `cli-suggest` sugere `touring memory recall "exec:cd"` para outcomes de
uma classe de comando — esse caso continua funcionando com a flag).
**Alternativa considerada e descartada**: parar de indexá-los — tornaria o caso
de uso acima impossível.

### F2 — família KPI `touring.memory.*`

Infra existente: `cli/kpi.rs` com `Commitment`/`resolve_derived` e as famílias
`touring.coupling.*` / `touring.adw.*` / `flow_compliance_ratio`. Acrescentar:

| KPI | Significado |
| --- | --- |
| `touring.memory.corpus_coverage` | `embeddings / memory_entries` — **0,797 → 1,008** após o restart; é o KPI que teria gritado o gap |
| `touring.memory.curated_recall_share` | fração das recuperações que vai para entradas não-automáticas — hoje **0,228** (alvo: subir com F1) |
| `touring.memory.never_recalled_ratio` | fração nunca recuperada — hoje **0,200** |

O primeiro KPI é a justificativa retroativa da família inteira: o gap de corpus
existiu por semanas, tinha remédio de um comando, e **ninguém podia vê-lo**.

São derivados de SQL sobre `memory.db` — zero instrumentação nova, mesmo padrão
dos derivados de ADW.

### Débito colateral corrigido: MSRV do clippy

`clippy.toml` ainda declarava `msrv = "1.85"` enquanto `Cargo.toml` já dizia
`1.95` — drift que **eu deixei** ao realinhar o MSRV mais cedo hoje. Corrigido
(REGRA #21). Consequência: com o MSRV honesto, o clippy destravou lints que
dependem de let-chains (estáveis desde 1.88) e **39 sites** de
`collapsible_if` passaram a falhar em todo o workspace. Bounded e mecânico →
corrigidos via `cargo clippy --fix`, não silenciados.

### As 3 falhas de CI abertas — causas-raiz (Gabriel: "corrija absolutamente tudo")

**`fuzz targets`** · confiança 0.95. `taiki-e/install-action` entrega um
`cargo-fuzz` **musl-estático**, e o cargo-fuzz usa o próprio triple de compilação
como target default → tentava compilar para `x86_64-unknown-linux-musl`, cujo std
não existe no runner (`E0463: can't find crate for std`). Fix: `--target
x86_64-unknown-linux-gnu` explícito. O `+nightly` que adicionei antes estava
certo e era necessário — só não era **suficiente**.

**`coverage` + `integration`** · confiança 1.0, **causa única**. Ambos morriam no
mesmo teste, `touring-generator --test e2e_diagnostic_rfc100`, com:

```
panicked at e2e_diagnostic_rfc100.rs:126:40: touring binary not built — skipping
```

O código **mente**: `expect("… — skipping")` anuncia um skip e panica. Duas
causas somadas: (a) `locate_binary` só procurava em `target/{debug,release}`, mas
`cargo llvm-cov` redireciona o build para `target/llvm-cov-target/`, então o
binário é invisível de dentro da run de cobertura; (b) ausência do binário virava
falha em vez de skip. Fix nos **5 sites de 2 arquivos** (`touring-generator` e
`touring-hooks`): `locate_binary` passa a honrar `CARGO_TARGET_DIR` e a sondar
`llvm-cov-target`, e `touring_bin_or_skip()` **pula de verdade**, nomeando-se
pela thread do libtest. É a mesma classe do fix que fiz hoje mais cedo em
`w12_5_per_project_daemon_e2e.rs` — lição aprendida num arquivo e não propagada
aos irmãos, exatamente como D1/D3/D5.

## Plano (6 fases)

| Fase | Escopo | Crate |
| --- | --- | --- |
| **P1** | D4 erro-engolido (desbloqueia o diagnóstico das demais) | touring-server |
| **P2** | D1 `list` contra `memory_entries` + remover código legado órfão | touring-cli |
| **P3** | D3 renomear campos de `GotchaStats` + calls | touring-storage, touring-cli |
| **P4** | D2 reindex incremental + não-bloqueante | touring-cli |
| **P5** | F1 filtro `outcome:*` no recall + flag | touring-cli |
| **P6** | F2 família KPI + commitments | touring-cli |

Dependências: P1 primeiro (torna as falhas legíveis); P2/P3 independentes; P4
depois de P1; P5/P6 por último. Convergência por fase: `cargo check` +
`clippy -D warnings` + testes do crate + `touring e2e -j`.

## ██ GATE HUMANO ██ — por que preciso da sua aprovação

1. **Deploy**: as correções só valem em runtime após `update-touring` (rebuild +
   **restart do daemon**), que afeta os 13 projetos e qualquer sessão CC
   concorrente. É ação irreversível-no-curto-prazo e fora do meu mandato
   autônomo (golden rule 4).
2. **D3 muda a superfície pública de `touring memory stats`** — o JSON passa a
   ter `total_hits`/`total_prevented`. Se algo seu consome esses campos, é
   quebra.
3. **F1 muda o comportamento default do `recall`** para toda sessão.
