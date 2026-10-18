# Tiers da memória: espaço de nome, não escada de calor (decisão B)

Worktree espelho: `/home/gabrielgadea/projects/touring-wt-tiers` (cópia do int97; HEAD `c118aa52` mais as mudanças não commitadas do int97).
Lote único sob uma reserva `hw-slot --class rust --label tiers`, de 02:48:39 a 03:21:55 (18/10/2026), `CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-tiers`, rustc 1.98.0. Logs em `logs/` (`00-summary.txt` tem o rc de cada passo).

## 1. Achado que muda o enquadramento: o «mover vivo» era inerte

O passo 4 do `TdLearningLoopHandler` enumerava com `rlm.search("", Some(tier), 100)`. Mas `RlmMemory::search` devolve vazio quando a consulta FTS tokeniza para nada (`fts_query("")` é a string vazia; o guard «Empty query after tokenization» está em `rlm.rs` desde o commit `a38ae33e`). O passo compilava, rodava a cada Stop e não promovia nenhuma entrada.

Prova no baseline (`logs/01-baseline-control.log`), com uma entrada `Working` lida 12 vezes:

```
PROBE search("", Working, 100)      -> 0 hits
PROBE scan_prefix("", Working, 100) -> 1 hits (access_count=13)
```

Consequência: um teste vermelho contra o handler atual é impossível por construção. O perigo real é o «conserto óbvio» do passo inerte, trocar a enumeração por algo que funciona (`scan_prefix`). Isso religaria a destruição de namespace que a decisão B quer impedir. O teste protege contra isso.

## 2. Ciclo de testes (logs 01, 02, 03)

Teste novo: `touring-cortex` `handlers::learning::tests::td_learning_loop_never_moves_an_entry_between_tiers`. Duas entradas (`Ephemeral` e `Working`), cada uma lida 12 vezes (acima dos limiares 3 e 10 do passo antigo). Roda o `TdLearningLoopHandler` num contexto Stop com `persistence` (um hook event recente, `graph.db` em tempdir sob `.claude/touring/`) e `rlm` Some. Afirma que:

- a passada TD(0) chegou ao fim (linha em `learning_qtable`; sem isso as demais asserções valeriam também para um skip precoce);
- `get(key, tier_original)` ainda devolve o valor;
- `get(key, tier_acima)` devolve `None`.

| Fase | Handler | Resultado |
|---|---|---|
| 01 controle | baseline (passo 4 original `search("")`) | `ok`, 2 passed (o teste + a sonda descartável); o teste passa porque o passo é inerte |
| 02 vermelho | passo 4 com enumeração por `scan_prefix` (mover ativo) | `FAILED`, rc 101: `learning.rs:399` «tier-probe:ephemeral must still be found at its original address (ephemeral)» |
| 03 verde | passo 4 removido (estado final) | `ok`, 1 passed |

Condição declarada: nas fases 01 e 02 o `rlm.rs` já estava no estado final. A remoção de `maintain_tiers`/`TierPolicy`/`TierMaintenanceReport`/`priority` e os docs não tocam `search`, `get` nem `store`, então o comportamento exercitado é o do baseline. A sonda descartável existiu só na variante 01 e não está no patch.

## 3. Mutantes sobre a árvore corrigida (logs 09, 10)

Controle sem mutação: o passo 03, verde.

| Mutante | Mudança | Resultado |
|---|---|---|
| M1 literal | reintroduz o passo working→reference exatamente como era (`search("")`) | sobrevive (rc 0). É mutante equivalente: o passo é inerte, não há comportamento a pegar |
| M2 scan | mesmo passo, enumerando com `scan_prefix("", Some(Working), 100)` | **morto**, rc 101: `learning.rs:395` «tier-probe:working must still be found at its original address (working)» |

O arquivo `learning.rs` foi restaurado ao estado final ao fim do lote (sha256 verificado: `cdc06081…f5b8`).

## 4. Gates (REGRA #21), todos verdes

| Passo | Comando | Resultado |
|---|---|---|
| 04 | `cargo fmt --all -- --check` | rc 0 |
| 05 | `cargo clippy -p touring-intelligence -p touring-cortex -p touring-hooks --all-targets -- -D warnings` | rc 0 |
| 06 | `cargo test -p touring-intelligence -p touring-cortex --no-fail-fast` | rc 0: 20 suítes, 2798 passed, 0 failed, 16 ignored |
| 07 | `cargo test -p touring-hooks --test e2e_r1_to_r20` | rc 0: 22 passed |
| 08 | `cargo check --keep-going --all-targets` nos 22 pacotes que dependem de `touring-intelligence` (lista de `cargo tree -i touring-intelligence --workspace`) | rc 0 (oráculo da remoção: o compilador) |

O passo 08 foi acrescentado além dos gates pedidos: remover `pub fn`/`pub struct` só se prova sem consumidor compilando todo dependente reverso, inclusive `touring-python`, `touring-bindings`, `touring-server` e `touring-cli`.

## 5. O que foi removido

- `RlmMemory::maintain_tiers`, `TierPolicy`, `TierMaintenanceReport` (`rlm.rs`, 110 linhas). Eram o único GC de entradas `ephemeral` (TTL) e as demais demoções/promoções automáticas; nunca tiveram chamador de produção.
- Passo «4. Tier promotion» do `TdLearningLoopHandler` e as constantes `TIER_WORKING` (3) e `TIER_REFERENCE` (10). O doc antigo prometia também «50→core», que nunca existiu.
- `MemoryTier::priority()`: zero chamadores (varredura de `.priority()` nos arquivos que usam `MemoryTier` e o oráculo do passo 08). O doc dela afirmava a escada («higher = more persistent»), o que contradiz a decisão. REGRA #0: sem consumidor e sem propósito, remove.
- Testes: `rlm.rs` `maintain_tiers_succeeds_on_default_policy`; `e2e_r1_to_r20.rs` `r7_rlm_maintain_tiers` e `r7_maintain_tiers_empty_db`.

Acrescentado:

- o teste do handler (seção 2);
- `rlm.rs` `tier_is_part_of_the_address_and_a_restore_under_another_tier_moves_the_row`, que prova o que o novo doc afirma (tier faz parte do endereço, regravar a chave em outro tier move a linha, uma linha só);
- doc do `enum MemoryTier` e de cada variante (namespace; nada expira nem move sozinho; calor/uso é sinal separado, opção C do canvas, futura, atrás de avaliação);
- doc do módulo e do struct do handler;
- a linha `rlm` do `crates/touring-intelligence/src/rl/memory/README.md`.

## 6. Consumidores verificados

Grep de `maintain_tiers|TierPolicy|TierMaintenanceReport|TIER_WORKING|TIER_REFERENCE` em `crates/ scripts/ client/ docs/ docs-site/ agent-harness/`: código só em `rlm.rs`, `e2e_r1_to_r20.rs` e no handler (todos tratados). Nenhum Python, CLI, MCP ou skill cita. Restam apenas registros históricos, deixados como estão porque reescrevê-los falsificaria o histórico:

- `docs/plans/2026-08-12-followups-l3/log.md` e `phases/H1.md` (relato da wave H1, cita `maintain_tiers`);
- `docs/baselines/wiring-pre-refactor-2026-05-11.json` (snapshot de wiring);
- `docs/audits/.baseline/orphans-scoped.txt:1629` (lista `TierMaintenanceReport` como órfão; é teto de baseline, remover só baixa a contagem).

## 7. Fora de escopo, só reportado

- **`consolidate_memory`** (MCP, `touring-server/src/server/tools_analysis.rs:1386`) chama `MemoryStore::promote_tier` (`memory_store.rs:429`): get, delete, store em outro tier. É mover explícito, pedido pelo operador que nomeia tier de origem e de destino, então é escolha de endereço consciente. Mantido; o doc novo do `MemoryTier` o cita.
- `touring-cortex/src/handlers/lifecycle.rs:1032`: o comentário diz «Upgrade from Working → Reference», mas o código é um `store` direto em `Reference` com chave nova. Só redação desatualizada; não toquei.
- `touring-hook-runtime/src/hook_memory.rs`: outro modelo de tier (tabelas próprias `hook_patterns_ephemeral/working/semantic`, `run_promotion` e um `MemoryTier` homônimo). Outro armazenamento, sem relação com `memory_entries`.
- **Não verificado**: `memory_store.rs:420` faz `self.rlm.search("*", Some(tier), limit)`. A consulta vira o token entre aspas `"*"`; pela mesma razão do achado 1 é provável que devolva vazio, mas não rodei. Vale uma sonda.

## 8. Patch

`tiers.patch` (+ `.sha256`) foi gerado contra o **estado pré-edição** dos 4 arquivos, não contra o `HEAD`: 3 dos 4 (`rlm.rs`, `e2e_r1_to_r20.rs`, `README.md`) já têm mudanças não commitadas do int97 em relação ao `HEAD`, e um `git diff HEAD` as embute, não aplicando limpo sobre o int97. Provas feitas:

- `git apply` do `tiers.patch` sobre cópia do pré-estado reproduz byte a byte os 4 arquivos do worktree;
- os 4 arquivos de pré-estado têm o mesmo sha256 dos arquivos atuais de `touring-wt-int97` (comparação só de leitura), logo o patch aplica no int97.

Para quem quiser a forma literal pedida, `tiers-vs-HEAD.patch` (+ `.sha256`) é o `git diff HEAD` dos mesmos 4 arquivos.

Lição sugerida para a memória (não gravei): «passo de manutenção que enumera com consulta FTS vazia é inerte; antes de criar o teste vermelho, medir se o mecanismo move algo. O risco está no conserto óbvio, e o teste tem de apanhar o mutante que o aplica, não o literal».
