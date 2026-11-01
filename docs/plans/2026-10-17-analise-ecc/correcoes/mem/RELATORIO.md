# Correções de memória e aprendizado (C6, C7, C8)

Worktree `/home/gabrielgadea/projects/touring-wt-ecc-mem` (HEAD solto `98301443`). Nada foi commitado.
Patch: `mem.patch` (17 arquivos, +1209 −98; o arquivo novo entra via `git add -N`).
sha256 do patch: `9d87b6ebb429ed797aff0cb8f329ac661acdb84d23362ba2fce94cc1dcaa0385`.
`git apply --check --reverse mem.patch` passa no worktree, ou seja, o patch é exatamente a árvore atual.

| Item | Decisão | Estado |
|---|---|---|
| C6 | `memory store` redige segredos e remove bidi, invisíveis e controles em todo caminho de escrita; o recall e as listagens redigem na saída | feito, testado, 20 de 20 mutantes mortos |
| C7 | `prevented` não tem produtor, então é reportado como `null` com `prevented_measured:false` e uma nota; nenhum produtor foi inventado | feito, testado |
| C8 | nenhum consumidor natural e seguro existe; não liguei e não removi | só evidência e proposta, a decisão é do Gabriel |

## C6: escrita e leitura de memória sem varredura

### Desenho

O redator existente (`redact_secrets`, que morava em `touring-ceg::gateway::sandbox_executor`) foi movido para
`touring-foundation::text_sanitize` (`crates/touring-foundation/src/text_sanitize.rs`). O motivo é a direção de dependência:
`touring-ceg → touring-hooks-shared → touring-intelligence`, então o `RlmMemory`, que é o funil de escrita, não pode importar o CEG.
O `sandbox_executor` agora faz `pub use touring_foundation::text_sanitize::redact_secrets;`, de modo que todos os chamadores antigos
(`transcript_miner`, `sandbox_output_store`, os testes T-09) continuam resolvendo. Há um redator só. No `sandbox_executor.rs` saíram os três imports que ficaram sem uso (`Cow`, `LazyLock`, `Regex`) e o comentário da dependência `regex` em `touring-ceg/Cargo.toml` foi atualizado (o `regex` continua usado em `summarize.rs`). A lógica foi preservada;
o único acréscimo é a contagem, e a função auxiliar `redact_line` foi extraída para manter a complexidade baixa.

`text_sanitize.rs` expõe `redact_secrets`, `redact_secrets_counted`, `strip_invisible`, `sanitize_memory_text` (invisíveis primeiro,
depois segredos, para que um zero-width enfiado dentro de um token não o esconda), `Sanitized` (`text`, `secrets`, `invisible`, `to_json`),
`sanitize_json_strings` e `sanitize_json_items`. A operação é idempotente: uma segunda passada devolve o mesmo texto e conta 0.

Remove-se U+200B a U+200F, U+202A a U+202E, U+2066 a U+2069, U+FEFF fora da posição inicial, o bloco Tag U+E0000 a U+E007F e os controles C0
exceto `\n` e `\t`.

### Caminhos de escrita cobertos

| Caminho | Onde | Como |
|---|---|---|
| CLI `touring memory store` e handler RPC do daemon | `touring-hook-runtime/src/ceg_impls.rs:227` (`cli_memory_store`) | `CleanedTexts` limpa `value` e `outcome_context` antes de qualquer sumidouro (linha SQL, espelho ANN e embedding). A resposta traz `"sanitized": {"secrets": n, "invisible": m}`, sempre presente, com zeros quando nada mudou |
| Funil único de gravação (`store`, `store_with_file_path`, `store_with_palace`, `cli_memory_store`, lifecycle, hook_registry, learn) | `touring-intelligence/src/rl/memory/rlm.rs:680` (`store_rich`), via `RichMemoryEntry::sanitized_texts` | defesa em profundidade, idempotente |
| Handler MCP `touring_memory_store` | `touring-server/src/server/tools_core.rs` | limpa antes do embedding (um vetor do texto cru codificaria o segredo) e devolve `sanitized` na resposta |
| `MemoryStore::store` e `store_batch` (segundo sumidouro, o chunk do `SemanticRecall`) | `touring-server/src/memory_store.rs:253` (`store_with_embedding`) | o chunk guardava o texto cru mesmo com a linha RLM limpa |

Não há outro `INSERT INTO memory_entries` em código de produção. Os que sobram são migração (`consolidation.rs`, cópia de linhas já
existentes) e testes.

### Caminhos de leitura cobertos (sem reescrever o banco)

| Caminho | Onde |
|---|---|
| `touring memory recall`, canais `entries` e `cases` (os casos rotulados mostram os `outcome:*` que `entries` esconde, então eram uma segunda saída) | `touring-cli/src/cli/memory.rs:1018-1019` |
| `touring memory query` | `ceg_impls.rs:488` |
| `touring memory list` | `touring-cli/src/cli/memory.rs:1363` |
| `RlmMemory::search`, que os handlers do cortex repassam como contexto (`[rlm] …`, `[recent] …`, `ToolSearch past`) | `rlm.rs:108` (`MemoryMatch::sanitized`) |
| MCP `touring_memory_recall` (metade `SemanticRecall`: `content` e `metadata`) | `memory_store.rs:175` (`MemoryResult::sanitized`) |

Leituras por chave (`RlmMemory::get`, `get_str`, `get_readonly`, `scan_prefix`, `MemoryStore::get`) ficam cruas de propósito: quem as chama
lê, altera e regrava estado (diário do agente, JSON). Veja as pendências.

### Testes novos (todos verdes)

- `touring-foundation`: 8 testes unitários em `text_sanitize` (redação com chave montada em runtime, idempotência com contagem 0,
  texto normal intacto, bidi, BOM, token partido por zero-width, JSON recursivo).
- `touring-intelligence` (`rlm.rs`, `sanitize_tests`): `store_rich` redige e limpa antes de a linha entrar (inclui `outcome_context`),
  texto normal byte a byte, ponto fixo, `search` limpa linha legada sem reescrevê-la.
- `touring-hook-runtime` (`ceg_impls.rs`): store redige e reporta `sanitized` (secrets 1, invisible 2), texto limpo reporta zeros e fica
  intacto, espelho ANN recebe o texto limpo, `query` limpa linha legada sem reescrevê-la.
- `touring-cli` (`memory.rs`): recall e list limpam linha legada, o canal `cases` também, entrada limpa passa intacta.
- `touring-server` (`memory_store.rs`): store limpa os dois sumidouros, texto normal intacto, `query` limpa linha RLM legada e chunk
  semântico legado (conteúdo e metadata).
- Os testes montam a chave falsa em runtime (`["sk", "-", "..."].concat()`) e carregam o pragma `touring-quality:allow-secrets`. O gate F2.4
  barrou a primeira versão do arquivo; a segunda passou.

### Mutantes (20 de 20 mortos; com os 4 de C7, 24 de 24; troca conferida por contagem exata do trecho e sha256 do arquivo mutado)

Foundation (cada um isolado): F1 faixa bidi, F2 controles C0, F3 bloco Tag, F4 regra do BOM, F5 passada PEM, F6 passada de tokens, F7 ordem
(redigir antes de limpar), F8 contagem de idempotência, F9 ramo Array do JSON. `RlmMemory`: R1 `value`, R2 `outcome_context`, R3 `search`.
Hook-runtime: C1 `apply` (espelho ANN), C2 `query`, C3 relatório `sanitized`. CLI: L1 recall `entries`, L2 recall `cases`, L3 list.
Servidor: S1 funil `store_with_embedding`, S2 `.sanitized()` no `query`. Cada mutante foi morto pelo teste previsto para ele.
(Os 4 mutantes de C7 estão na seção própria. Ao final, os 17 arquivos conferem byte a byte com o snapshot anterior aos mutantes; a única diferença é a correção intencional do meu teste.)

### Vermelho no original

Neutralizei o ponto único da correção (`sanitize_memory_text` virando identidade, o comportamento de antes) e rodei os testes novos:
`touring-intelligence` 3 vermelhos, `touring-hook-runtime` 3, `touring-cli` 3, `touring-server` 3 e `touring-foundation` 3 (os demais testes do
foundation exercitam os primitivos, cobertos pelos mutantes F1 a F9). Os testes de controle (texto normal intacto) ficam verdes no original,
como devem. Cada crate de consumo tem exatamente 1 controle verde.

### Censo do banco legado (somente leitura, contagens)

`~/projects/touring/.claude/touring/memory.db` tem 22.188 linhas. O guard alteraria 12 (0,05%): 5 com formato de token (github 1, openai 3,
aws 1), 6 com linha de variável de credencial e 3 com invisível ou controle. As chaves são lições que falam de segredos, por exemplo
`github-public-secret-purge-2026-08-02` e `sec05-transcript-miner-redact-2026-06-14`, e dois `gate-reject:*`. Provavelmente são exemplos
em texto de lição e não credenciais vivas, mas não verifiquei os valores, de propósito. Ao ser lida, uma lição desse tipo mostrará
`[REDACTED]` onde cita o exemplo. `~/.claude/touring/memory.db` (636 linhas): 0.

## C7: `touring gotcha stats` publicando `total_prevented: 0`

### Evidência

- `knowledge.db` do projeto: 13 gotchas, `SUM(hit_count)` = 388.840 (um padrão sozinho, `.unwrap()`, tem 290.622), `SUM(prevented_errors)` = 0,
  0 linhas com `prevented_errors ≠ 0`. É o número da análise, reproduzido.
- `FileKnowledgeDB::increment_gotcha_prevented` (`touring-storage/src/knowledge/gotchas.rs`) existe, mas não tem chamador de produção
  (9 referências no índice, todas em `knowledge/tests.rs`). Os 5 `increment_gotcha_hit` de produção (`pre_edit`, `pre_read`, `post_edit`,
  `pre_edit_prevention`, `cli_e2e`) contam **exibições** do aviso, não erros evitados.
- Não há produtor natural. O gotcha casa por substring do **caminho** do arquivo (`get_gotchas_for_file`, usado em pre_edit e pre_read), e
  esse caminho não muda depois do aviso, então "aviso seguido de mudança" não é observável. Só o `pre_write` casa por conteúdo
  (`get_gotchas_for_content`), e ali nem `hit_count` é incrementado. A única causalidade plausível seria "o padrão de erro não reapareceu
  nas próximas edições do mesmo arquivo", e isso é ausência de evidência, não medição. Inventar esse produtor trocaria um zero falso por um
  número plausível e sem causalidade, a pior forma de instrumento que mente.
- A auditoria cruzada de 01/10 já havia visto o número (`gotcha_prevented_errors: 0`, achado A-5) e deixou "apontado para o dono do e2e".

### Correção

`FileKnowledgeDB::PREVENTED_MEASURED = false` e `PREVENTED_NOTE` nomeiam a lacuna ("falta um sinal ligando aviso exibido a mudança posterior
de ação"). Os métodos `prevented_measured()`, `prevented_note()` e `reported_prevented(raw)` fazem o relato honesto. O valor cru continua na
coluna e em `gotcha_stats()`. Passam pelo gate todas as superfícies que **mostram** o valor:

| Superfície | Antes | Agora |
|---|---|---|
| `touring gotcha stats` (`cli/gotcha.rs`) | `total_prevented: 0` | `total_prevented: null`, `prevented_measured: false`, `prevented_note: "…no producer…"` |
| `touring memory stats` (`GotchaStats`) | `0` | `Option<usize>` → `null` + `prevented_measured` |
| `evolution drift` e `evolution insights` | número | `null` + `prevented_measured` |
| `e2e` (`gotcha_prevented_errors`) | `0` | `null` + `gotcha_prevented_measured` |
| MCP `touring_gotcha` (`list` e `stats`, `tools_infra.rs`) | número | `null` + `prevented_measured` |

Guarda contra deriva (`knowledge/tests.rs`): `prevented_flag_matches_the_existence_of_a_production_producer` varre as fontes do workspace e falha
se o flag e a existência de um chamador de produção divergirem **nos dois sentidos**. Quem ligar um produtor real é obrigado a virar o flag.

Testes: `unmeasured_prevented_is_reported_as_null_not_zero` (inclui o caso de uma escrita só de teste na coluna não vazar), o guarda acima,
e `gotcha_stats_reports_prevented_as_null_with_the_missing_piece` e `memory_stats_reports_prevented_as_null_too` no CLI.
Mutantes (todos mortos): P1 flag virando `true`, P2 `reported_prevented` devolvendo `Some(raw)`, G3 `cli_gotcha_stats` voltando ao valor cru,
G4 `memory stats` voltando ao valor cru. Vermelho no original: `reported_prevented` devolvendo o cru deixa 3 testes vermelhos
(1 no storage, 2 no CLI).

**Importante**: os testes de `touring-storage` só existem com `--features knowledge`. Sem a feature, `cargo test -p touring-storage` roda
0 testes de gotcha e "passa". Rodei com `--features knowledge,session-hooks` (313 testes) e foi isso que contou.

## C8: `TierManager` órfão

Decisão: **nem liguei nem removi.** Evidência:

- `touring index find TierManager`: 3 definições, referências só em `crates/touring-intelligence/tests/cross_audit.rs` (mais os testes do próprio arquivo).
  `README.md` do módulo e `mod.rs` o listam.
- É só um decisor puro (`evaluate(tier, access_count, days_idle)`), sem acesso a banco. A implementação com SQL é outra:
  `RlmMemory::maintain_tiers(&TierPolicy)` (`rlm.rs:1290`), "Call once per session-start". Também não tem chamador de produção (3 referências, todas em
  `touring-hooks/tests/e2e_r1_to_r20.rs`). Os dois divergem: `TierManager` rebaixa `Working` para `Ephemeral` após 30 dias ociosos e promove por
  `access_count ≥ 10` sem janela; `maintain_tiers` nunca rebaixa `working`, usa janelas e rebaixa `core→reference` e `reference→working`.
- Não existe comando `touring memory …` de manutenção (os subcomandos são stats, recall, credit, store, tag, tags, query, sync-tags, link, links,
  suggest-links, backfill-tags, moc, communities, unlink, list, reindex) nem tarefa periódica de memória no daemon.
- Impacto de ligar um sweep, medido em **somente leitura** no `memory.db` do projeto (22.159 linhas na hora da medição):

  | Regra de `maintain_tiers` (política padrão) | Linhas afetadas |
  |---|---|
  | `reference → working` (ocioso mais de 14 dias) | **15.484 de 16.745** (92%) |
  | GC de `ephemeral` (**DELETE** após 48 h) | 5 |
  | `core → reference` | 1 |
  | `ephemeral → working` e `working → reference` | 0 e 0 |

  O tier `reference` guarda 8.520 `transcript_lesson`, 7.863 `insight` e 362 `lesson`. Já o `TierManager` rebaixaria 10.992 `reference` e
  514 `working`, e promoveria 356 `reference` para `core`. Além disso, `semantic` (4.804), `local` (85) e `episodic` (5) estão fora da escada e nenhum
  dos dois os toca. Ou seja, a coluna `tier` hoje é taxonomia de texto livre, não degrau de promoção, e o primeiro sweep reclassificaria
  em massa e apagaria linhas sem nenhuma evidência de que isso melhora o recall.

### Proposta (a decisão é sua)

- **A. Remover `TierManager`** (`tier_manager.rs`, a linha `pub mod tier_manager;` e a linha do README, mais a seção 1 de `tests/cross_audit.rs`).
  Comportamento inalterado, elimina o duplicado divergente e deixa `maintain_tiers` como única implementação. É o que eu recomendo.
- **B. Ligar com segurança** (se você quer manutenção de tier): novo `touring memory maintain`, com `--dry-run` como padrão, que só reporta contagens
  como as da tabela; `--apply` exige snapshot e confirmação, e o GC de `ephemeral` fica atrás de flag própria. Antes disso é preciso decidir o que
  `reference` significa (hoje é onde 92% das entradas moram) e ajustar a política. Eu não ligaria no início de sessão antes dessa decisão.

## Gates

| Gate | Resultado |
|---|---|
| `cargo fmt --all -- --check` | rc 0 |
| `cargo check -p …(8 crates) --all-targets` | rc 0 |
| `cargo clippy -p …(8 crates) --all-targets -- -D warnings` e `-p touring-storage --features knowledge,session-hooks` | rc 0 e rc 0 |
| `touring-foundation` | 613 passam, 0 falham |
| `touring-intelligence` | 1931, 0 |
| `touring-ceg` | 612, 0 |
| `touring-hook-runtime` | 503, 0 |
| `touring-storage` padrão / `--features knowledge,session-hooks` | 160, 0 / 313, 0 |
| `touring-quality` (só um comentário foi tocado) | 493, 0 |
| `touring-cli` | 701, 0 |
| `touring-server` (com os binários do worktree) | 1817, 0 |
| dependentes em `touring-hooks`: `master_plan_e2e`, `post_wave_rtk_cross_audit`, `cli_handlers_e2e`, `e2e_r1_to_r20` | 214, 0 |

Duas ressalvas de ambiente, ambas resolvidas e não escondidas:
1. A primeira passada do `touring-hook-runtime` teve 1 falha minha: eu esperava `deploy [REDACTED]xy`, mas depois de remover os invisíveis o token
   fica contíguo e a redação leva o "xy" junto (`deploy [REDACTED]`). O comportamento é o correto e o teste foi corrigido.
2. A primeira passada do `touring-server` teve 38 falhas de testes de integração que procuram `touring` e `touring-daemon` em `<worktree>/target/debug`
   (derivam de `CARGO_MANIFEST_DIR`, não de `CARGO_TARGET_DIR`). Compilei os dois binários **deste** worktree (`cargo build -p touring-server -p
   touring-hooks`) e criei os symlinks em `<worktree>/target/debug/` (pasta ignorada pelo git). Com isso, 1817 passam e 0 falham.

Os `cargo` rodaram todos sob `hw-slot --class rust` com `CARGO_TARGET_DIR=…/target/wt-frag`; a vaga demorou mais de 1 hora porque o ecc-qa a segurava; por isso enfileirei uma execução única com check, testes, clippy e mutantes.

## Pendências e riscos

1. **Redator existente é agressivo na linha.** A passada de variável de credencial troca tudo depois do primeiro `=` ou `:` de uma linha que contém
   `GITHUB_TOKEN`, `ANTHROPIC_API_KEY` etc. Uma lição que diga "GITHUB_TOKEN: não logar" vira `GITHUB_TOKEN: [REDACTED]`, e um valor JSON em uma linha
   que cite esses nomes perde o resto da linha. Usei o mesmo redator, como pedido, e a resposta de `store` informa a contagem. Se isso incomodar,
   o conserto é uma passada que exija que o valor tenha cara de segredo, e fica melhor num redator só, no foundation.
2. U+200C e U+200D (ZWNJ e ZWJ) estão na faixa pedida e são removidos: sequências de emoji com ZWJ se desmontam e a formatação persa e indic
   é afetada. CR é um C0 e também sai (CRLF vira LF).
3. **Leituras por chave seguem cruas** (`RlmMemory::get`, `get_str`, `get_readonly`, `scan_prefix`, `MemoryStore::get`, `bindings/python`). O
   `agent_diary` (`scan_prefix`, linhas 437 e 633) devolve entradas ao modelo e pode servir linha anterior ao guard. As escritas novas já chegam
   limpas.
4. O corpo do handler MCP `touring_memory_store` não tem teste direto (`TouringServer::new()` usa caminhos reais); o efeito está coberto por
   `MemoryStore::store`, e o formato de `sanitized` por `Sanitized::to_json` no foundation. O mesmo vale para `tools_infra.rs` (`touring_gotcha`).
5. **Consumidores do zero falso de `prevented`**, não alterados por estarem fora do escopo e de outro dono:
   `touring-hook-handlers/src/hooks/pre_write.rs:666` usa `wilson_adjusted_score(1.5, prevented, hit_count)`. Com `prevented` sempre 0, o limite
   inferior de Wilson é 0 para qualquer `hit_count > 0` (conferido: `wilson_lb(0,n) = 0`), então todo gotcha já disparado recebe score 0 no
   `pre_write` e fica no fim do orçamento de contexto. Proposta: enquanto `!PREVENTED_MEASURED`, usar a confiança neutra (`base * 0.5`).
   `post_tool_rl.rs:94` já protege com `prevented > 0`.
6. Fora do Rust e não tocados: `agent-harness/cli_anything/touring/core/gotcha.py:66-77` e `touring_cli.py:637` imprimem "Errors prevented: 0";
   `client/hooks/session_startup_intelligence.py:499` imprime `prevented_errors=**0**` por gotcha (espelho gerado, o vivo é a fonte). A métrica
   `gotcha_hit_rate_pct` do e2e (achado A-5 da auditoria de 01/10, valor `"2987015.4"`) é outro instrumento que mente e não foi tratada aqui.
7. As linhas legadas **não foram reescritas** (pedido do Gabriel). Se quiser purgar as 12, vale um `touring memory scrub --dry-run` futuro.
8. `f2_4_secrets.rs:1143` teve só o comentário atualizado (o padrão de tokens mudou de casa); por isso `touring-quality` entrou nos gates.

## Como aplicar

No checkout alvo: `git apply --check mem.patch && git apply mem.patch`, depois `cargo fmt --all -- --check`, `cargo clippy -p touring-foundation -p
touring-intelligence -p touring-ceg -p touring-hook-runtime -p touring-storage -p touring-quality -p touring-cli -p touring-server --all-targets -- -D
warnings` e `cargo test` dos mesmos pacotes (storage com `--features knowledge,session-hooks`). Reconstruir o daemon exige `update-touring`
(o handler RPC mudou).
