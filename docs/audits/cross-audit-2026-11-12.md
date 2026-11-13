# Cross-audit 12–13/11/2026 — aprofundamento do `touring-code` (C1–C8)

**Alvo**: `crates/touring-code` e os consumidores tocados pelo aprofundamento (DAG
`task_1794458342745990025`, plano em `docs/plans/2026-11-12-touring-code-aprofundamento/`).
**Base**: `a3bfb950` (a branch avançou para `90befad5` por um commit de outra sessão em `client/hooks/`).
**Estado**: fases 1–7 concluídas, mais uma rodada final em 13/11 (F13–F16) aberta pelo veredito do juiz.
Nada commitado, nada instalado: tudo vive na árvore de trabalho.

Evidência bruta: `~/claude-artefatos/touring/cross-audit-20261112/` (relatórios dos engenheiros, sondas,
logs RED/GREEN) e `~/claude-artefatos/touring/g1/` (gates medidos pelo lead; `g1-rodada1/` é a primeira passagem).

Neste relatório, **"medido pelo lead"** é o que eu executei com a árvore parada; **"relato"** é número de
engenheiro, conferido por leitura mas não reexecutado um a um.

## Veredito

A árvore final passa em todos os gates do workspace (medido pelo lead com a árvore parada, 13/11 05:35–05:52,
depois da rodada final; a passagem anterior, de 01:53–02:21, está em `g1-rodada2/`):

| Gate | Resultado |
|---|---|
| `cargo fmt --all --check` | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo test --workspace --no-fail-fast` | exit 0 · 315 alvos · 330 linhas de resultado, 0 `FAILED` · 18.393 testes passando, 173 ignorados |
| `cargo test -p touring-hook-handlers --features pre-hooks,post-hooks,session-hooks` | 850 passando, 0 falhas |
| `cargo test -p touring-storage --features knowledge` | 336 passando, 0 falhas |
| `pytest scripts/test_gen_sdk.py` | 16 passando |
| `touring index rebuild` | geração 49 completa · 7.776 arquivos · 185.466 símbolos · 0 erros |
| `touring e2e -j` (binário instalado) | 0,866 `pass` |

Sem P0 nem P1 aberto. O achado de maior peso não era do refactor: **o grafo de dependências que seis
sinais de pré-edição consultam estava vazio em produção**. A frente C8 o resolve, lendo as arestas que o
wiring já grava; só passa a valer com deploy.

O segundo achado de peso é contra o próprio C8 e saiu do veredito do juiz: a primeira versão lia como
dependência também os palpites por nome do wiring, 91% das arestas. Corrigido em 13/11 (F13), junto com três
consertos do detector de órfãos e do resolvedor (F14–F16). Detalhe na seção "Rodada final".

## Fase 1 — MAP

Sete módulos profundos, um por candidato, mais a frente C8:

| # | Módulo | O que fica atrás do seam |
|---|---|---|
| C1 | `ast::grammar` | identidade da língua, gramática e parse com orçamento; único sítio de `Parser::new()` (18 → 1) |
| C2 | `ast::graph::references::FileReferences` | o que conta como referência; 12 extratores `pub` → 0 fora do crate |
| C3 | escrita única em `ast::graph` + `ast::store` | indexar / reindexar / remover como uma operação, com rollback |
| C4 | `vgp::lang::vista` | overlay sobre disco: existe / é arquivo / é diretório / ler |
| C5 | `sdk_signal_mirror::Mirror` | formato da linha do espelho; 13 sítios que o conheciam → 0 (2 exceções nomeadas) |
| C6 | — | superfície lazy do `IncrementalPipeline` removida |
| C7 | heat + orçamento do pipeline | evicção decidida pelo heat; teto de memória com default derivado da máquina |
| C8 | arestas resolvidas no `SymbolIndex` | carregador único, índice por geração (`IndexSlot`), `importers_of` / `importers_status` |

## Fase 2 — PURPOSE AUDIT

Auditor somente leitura sobre C1, C3, C6 (`aud-proposito.json`): invariantes das notas conferidas com
execução; 4 parciais, todas fechadas depois (tabela duplicada de extensões, `AsyncSharedPipeline` sem
consumidor, doc do escritor SQLite, e o ramo de reentrada do cache de parsers — este último **segue aberto**
como seam sem chamador de produção, custo baixo).

Afirmações de engenheiros testadas:

- **Refutada em parte** — "`.mts`/`.cts` entram no índice" (C1): o parser os lia e a política de índice os
  recusava. Consertado em F1.
- **Refutada pelo lead** — "o pré-read já paga dois clones do índice hoje" (minha, em 12/11): no `HEAD` o
  índice carrega na primeira consulta e cada pré-read o clona **uma** vez depois disso
  (`hook_runtime_ext.rs:29`, `pre_read.rs:~988` em `a3bfb950`).
- **Refutada pela sonda** — "o índice de produção nunca foi carregado, custo 0" (engenheiro): mesma leitura
  de código acima; o custo é zero só até a primeira consulta.

Defeitos antigos confirmados por sonda executada (`probe.rs`): `replace_file_symbols` apaga arestas e
ninguém as regrava (latente; some com a retirada do ramo `dependencies`, decisão aberta); `co_edit_weight`
lido sempre 0,0 (**consertado**, RED `left 0.0, right 0.75`); seis leitores por caminho contra escritor por
chave de módulo (**consertado** pelo C8).

## Fase 3 — DEBT SCAN

`scan_debt.py crates/touring-code` → 19 marcadores, abertos um a um: 18 falsos (detectores que procuram a
string `todo!()`, casos de teste, a palavra "TODO módulo" em comentário) e 1 real, o
`#[allow(unused_imports)]` de `tests/snapshot_pheromone_e2e.rs:7`, removido em F3.

## Fase 4 — HARMONY CHECK

0 ciclos, 0 módulos de nota baixa. Órfãos: ver "Juiz de convergência" abaixo (a contagem do script é do
workspace inteiro; a linha de base antes da frente era 708).

## Fase 5 — FIX & POTENTIALIZE

Doze correções, cada uma com teste falhando antes (logs `f*-red.log` / `f*-green.log`):

| # | Achado | Conserto |
|---|---|---|
| F1 | o parser lê `mts, cts, htm, zsh, jsonc`; o índice recusava sem razão declarada | política os admite; um teste liga política e parser (exceção nomeada: `go`, `java`) |
| F2 | `cli_shadow_validate` julgava extensão desconhecida como Rust | `unsupported_extension`; as 23 extensões da tabela antiga cobertas |
| F3 | `#[allow(unused_imports)]` | supressão removida |
| F4 | `--overlay` com chave relativa lia o disco calado | chave resolvida contra a raiz e normalizada |
| F5 | `BestPracticesGate` contava aliases como hooks canônicos (8 aliases liam "8 de 8") | conta só os 8 canônicos; teste de paridade com o SDK |
| F6 | `gen_sdk.py` × Rust divergiam; o smoke não comparava | journal real congelado (58.142 linhas): 57.568 execuções nos dois; comparação automatizada |
| F7 | `anyhow` sem uso | removida |
| F8 | espelho ilegível lido como "no mirror yet" | `unreadable: <erro>`; ausente sai `null`, nunca `0` |
| F9 | `touring status -j` de `$HOME` varria a pasta inteira | não varre fora de projeto; não segue symlink de diretório; sinal `null` com razão |
| F10/F11 | 8 sítios com `unwrap_or(Lang::Rust)` (inclui `touring rename`) | erro explícito; teste-guarda reprova fallback novo |
| F12 | dependências sem uso em `touring-code` | `once_cell`, `derive_more`, `lru` removidas; `public-api` → dev |

**F9 medido pelo lead, ao vivo** (mesmo comando, de `~`, sem `TOURING_DAEMON_SOCKET`): binário instalado
30.4.109 → **55,1 s**, sinal `0` com erro; binário debug da árvore → **0,46 s**, sinal `null` com a razão
"cwd is `$HOME` or `/`, not a project". O plugin da barra do Omarchy roda esse comando a cada 120 s.

### Revisão em dois eixos (padrões × spec)

`review-standards.md` (5 duras, 17 de julgamento) e `review-spec.md` ((a) 2, (b) 8, (c) 10). Fechados:
falha silenciosa em 4 pontos; `SymbolIndex::clear()` sem limpar a cobertura; docs que afirmavam paridade
memória × SQLite; `Mirror::read` abortando com uma linha de UTF-8 inválido; `.markdown` perdido; teste de
ordem que passava por acaso; e o achado mais sério — **a ordem do rebuild dependia do heat**: o teste de
determinismo ficou vermelho (`cold "criar" × hot "No"` na linha 0), a ordenação saiu do `index rebuild` e do
`file-knowledge populate`, e o `PrioritizedPipeline`, sem consumidor, foi removido.

Ficam como julgamento, sem dono: mapeamento `Refusal → erro` em cinco lugares com mensagens diferentes;
`Grammar` com quatro `match` paralelos; o par `(String, String)` consumidor/produtor.

## Fase 6 — E2E PROOF

- Suíte do workspace inteira, executada pelo lead (tabela do veredito). Inclui os testes de integração
  novos: `index_write_path`, `file_edges`, `file_references_characterization`, `vgp_vista_overlays`,
  `sdk_cross_emitter`, `graph_service_e2e`, `cli_handlers_e2e`, `heat_producers_tests`,
  `standalone_table_parity`, e os guardas de classe `lang_fallback_guard` e `index_loader_guard`.
- Prova ao vivo do F9 (acima).
- Sonda do C8 sobre cópia dos bancos reais do `touring` (11.579 arquivos, 76.925 arestas): o raio de impacto
  bate com um oráculo SQL em 35/35 arquivos com wiring; antes dava "1 arquivo" em 50/50. **Correção de 13/11**:
  o oráculo lia as mesmas linhas com a mesma definição de aresta, então a concordância provou que o índice
  carrega o que o banco tem, e não que o banco diz a verdade. 91% daquelas arestas eram palpite por nome
  (seção "Rodada final").
- Mutação (relato): 26 mutantes no C8, 4 no C7, mais os de C2, C3 e F5 — todos derrubados por teste nomeado.
- Um teste **antigo** pegou o que os novos não viam: `standalone_table_parity` acusou que o pré-edit passara
  a emitir "sem cobertura" em toda chamada. Resultado: o hook de uma invocação não carrega o índice, e o
  sinal só fala quando informa (ver C8).

**Limite da prova**: os caminhos do daemon (sinais de pré-edição, `status` com `file_edges`, refresh por
hook) foram provados por teste e por sonda, **não no daemon em produção** — isso exige deploy.

## C8 como foi construído, e o que custa

Desenho confirmado por Gabriel em 12/11: o wiring é o único escritor de arestas de arquivo; o índice as
carrega; `dependencies` não ganha escritor (seria o quarto grafo).

- Carga **sob demanda**, uma vez, pelo primeiro hook do daemon que precisa de um sinal de grafo. Projeto
  aberto e ocioso não paga. `touring status` nunca dispara a carga.
- Índice compartilhado por referência (zero cópia por leitura) e cache do HNSW por geração.
- Kill switch humano `TOURING_FILE_EDGES_DISABLED=1`, visível em `touring status -j` → `file_edges`.
- Refresh por hook: um import acrescentado ou removido aparece em `importers_of` sem rebuild.
- Texto do sinal: agregado com teto de 400 caracteres (`TOURING_GRAPH_BLAST_MAX_CHARS`); "sem cobertura" só
  quando o wiring lê arquivos daquela extensão e este ficou de fora.
- **Regra de aresta (F13, 13/11)**: um par consumidor→produtor só é aresta quando alguma linha dele no
  `wiring_map` não é `ast_inferred`. A regra vive em um lugar (`file_edge_filter!`) e os três leitores a
  dividem. Sem importador, o texto diz "nenhum import resolvido aponta para este arquivo", nunca "isolado".

Medido em build otimizada isolada, máquina a 93–96 °C (latências ±70 %; alocador do sistema), **com a regra
antiga (≈77 mil arestas)**. Com a regra do F13 o índice carrega ≈6,7 mil; os custos abaixo ficam como teto,
não foram remedidos:

| | Valor |
|---|---|
| Carga do índice, primeira leitura | ~259 ms (149 ms só símbolos) |
| Memória anônima por projeto carregado | +133 MB |
| Pré-read em CILA 4, índice compartilhado | ~28 ms (hubs ~34 ms) |
| Mesmo mecanismo antigo sobre o índice carregado | ~133 ms |
| Clone inteiro do índice (o que o `HEAD` paga por pré-read depois de carregado) | ~31 ms |
| Projeção para o `analise` (48.257 arquivos, 47.990 arestas) | ~26 MB de arestas; clone ~5× |

Não há métrica de latência por hook no daemon (`gate-metrics` só traz o agregado), então o efeito em
produção não tem linha de base medida.

## Mudanças de saída observável (valem após deploy)

1. **Índice**: `.mts .cts .htm .zsh .jsonc` passam a ser indexados.
2. **`touring ast blast`**: −1 nos 1.401 arquivos que se listavam como consumidor de si mesmos; consumidores
   ordenados; sem o consumidor virtual do daemon; caminho absoluto acha a linha; erro de consulta aparece.
3. **`touring ast heat`**: lê o heat do runtime; mapa ocupado → `null` com razão;
   `blast_radius_weight_has_producer: false`.
4. **`touring status -j`**: seções `pipeline` (com `budget_source`), `heat` e `file_edges`; fora de projeto,
   sinal de qualidade `null` com razão.
5. **`touring file-knowledge populate` / `index rebuild`**: sem `hot_first`/`first`; rebuild ganha `index_reload`.
6. **Ferramentas de AST e `rename`**: extensão desconhecida → erro `unsupported_extension`, nunca análise como Rust.
7. **VGP**: dois falsos achados somem (`#[path = "./x.rs"]` com overlay; recurso HTML em pasta que só o
   overlay cria); `.ts` sem nome não é TypeScript; `--overlay` relativo funciona.
8. **KPI**: `code_mode_signal_use` pula linha inválida e a conta em `skipped_lines`; `hooks_complement` com
   espelho ausente sai `null` + `mirror_absent`; o gate de boas práticas conta só hooks canônicos.
9. **`gen_sdk.py`**: conta execuções como o Rust; `_extras` vira `--extras`; recusa journal em que nada parseia.
10. **Sinais de pré-edit/pré-read/pré-write (daemon)**: raio de impacto por import resolvido em vez de
    "isolado" (arquivo sem importador lê "nenhum import resolvido aponta para este arquivo"); `bfs_hops`
    aposentado; hotspots nomeiam arquivos; `detect_changes` não credita homônimos e ganha `coverage`;
    `resolve_ctx` devolve `imported_by=[]` para homônimo ou arquivo não indexado, onde antes herdava todos.
11. **Pipeline do daemon**: teto de memória ligado por padrão (`max(64, (H/8 − 260)/1,43)` MB de texto;
    `TOURING_PIPELINE_MEMORY_MB=0` desliga); a vítima da evicção é o arquivo mais frio.
12. **Boot do daemon**: heat semeado do histórico de edições; aviso em stderr se o histórico for ilegível.

## Rodada final (13/11) — o juiz, os órfãos e a regra de aresta (F13–F16)

O juiz de convergência parou em `orphans_base` com quatro órfãos novos. Os quatro tinham consumidor real
(conferido por grep). Puxar esse fio achou um defeito do detector e um defeito do próprio C8.

**Causa comum.** `record_inferred_consumers` (`touring-storage/src/knowledge_wiring.rs`) só resolvia
qualificador que fosse nome de MÓDULO; o resto caía no palpite por nome, com teto de 4 produtores (mesmo crate
primeiro, depois ordem alfabética). `Verdict::parse`, `FileReferences::new`, `Mirror::from_env` e
`family.as_str()` perdiam a vaga para homônimos. Medido pelo lead no `wiring_map` do `touring`: 101.730 das
114.060 linhas de consumidor eram `ast_inferred`.

| | O que era | O que ficou | Medição |
|---|---|---|---|
| **F13** | O C8 carregava toda linha do `wiring_map` como aresta de arquivo, palpite incluído | `file_edge_filter!` (regra única dos três leitores) exige uma linha não inferida por par | 77.655 → 6.729 arestas (−91,3%); `domain_circuit.rs` 1.237 → 2 importadores, os dois abertos e reais |
| **F14** | Chamada por tipo e método por receptor sem crédito | R1 (`Tipo::assoc`, exato ou nada) e R2 (método só em módulo de tipo com import resolvido), aditivos | sonda sobre cópia do banco: órfãos no escopo 706 → 542; 164 saem, 0 entram; nenhum símbolo perdeu consumidor |
| **F15** | `use crate::plugin::r#trait::{..}` não resolvia (procurava `r#trait.rs`) | o identificador cru é desfeito em um ponto (`resolve_module_layout`) | 5 linhas não resolvidas → 0; `plugin/trait.rs::as_str` ganha `plugin/registry.rs` |
| **F16** | `import pkg.mod as m` + `m.Nome` gravado como palpite, embora o módulo venha do caminho do import | grava `ast_resolved`, dentro do passo do próprio arquivo no rebuild | projeção no `analise`: arestas `.py` 9.650 → ~12.076 de 12.743 (não é rebuild medido) |

Como cada um foi provado: teste que falha antes (logs RED guardados), mutante que derruba o teste e é
restaurado com sha256 conferido (3 em F13/F14, 4 em F15/F16), e sonda no banco real aberto só para leitura
ou em cópia. Relatório do engenheiro: `fix-f13-f14.json` (chaves de primeiro nível e `round2`).

**O que os testes do C8 não viram, e por quê.** As fixtures gravavam linhas resolvidas, e o oráculo da sonda
usava a mesma definição de aresta do produto. A memória `wiring-64pct-heuristico` (08/08) já dizia que blast
radius repousava em palpite por nome; não a consultei ao recomendar o desenho do C8. Erro meu de processo,
registrado na própria memória.

**Limites declarados.**
- R2 é palpite, e fica no tier inferido: em 15 créditos só-por-receptor abertos à mão, 3 eram verdadeiros
  (`.len()`, `.is_empty()`, `.as_str()` sobre `Vec`/`String` contam para um tipo que só convive no arquivo).
  Não cria aresta de arquivo nem altera a lista `trusted` de órfãos (8.731 antes e depois, na sonda do F14).
- Arestas verdadeiras que a regra do F13 ainda deixa de fora: chamada qualificada por módulo ou por tipo sem
  `use`, e import Python que o resolvedor não coloca (80 pares no `analise`). Ticket S-04 do esforço de
  pendências (`task_1794549390311758325`).
- Nada disto está no daemon: o índice vivo foi escrito pelo binário instalado. O efeito sobre
  `touring wiring orphans` e sobre os sinais só existe depois de deploy e rebuild do índice.

**Achado do semáforo de hardware.** Um job `rust` precisa de `rust-1` e de um slot `heavy`, e os waiters não
têm fila nem prioridade (cada um consulta a cada 2 s). Com outra sessão mantendo os dois `heavy`, o build Rust
ficou mais de 30 min sem entrar, com a temperatura parecendo a causa porque o log só anuncia o primeiro motivo.

## Juiz de convergência

Comando: `loop_converged.py --task task_1794458342745990025 --scope <raiz> --bundle docs/plans/2026-11-12-work-outer`.

**Primeira passagem (13/11 02:38): exit 1.** Seis cláusulas verdes (juiz íntegro, DAG 9/9, qualidade Platinum
0,935, sem P0, escopo medido inteiro, `cargo check`) e duas vermelhas:

- `cross_audit`: o bundle não tinha `audit-plan-completion.sh`. Escrito em 13/11
  (`docs/plans/2026-11-12-work-outer/audit-plan-completion.sh`): confere os artefatos de C1–C8 e F13–F16, este
  relatório e os gates, e exige que os gates tenham COMEÇADO depois da última mudança de fonte.
- `orphans_base`: 4 órfãos novos (`references.rs::new`, `sdk_signal_mirror.rs::from_env`,
  `plugin/trait.rs::as_str`, `portfolio/mod.rs::parse`), todos falsos. Origem e conserto na seção acima.

**Passagem final (13/11 05:55): exit 1, sete de oito cláusulas verdes.** `dag_done` 13/13, qualidade Platinum
0,935, `cross_audit` rc 0, `cargo check` verde. Vermelha só `orphans_base`, com os mesmos quatro nomes. Ela
consulta `touring wiring orphans` do binário instalado sobre o índice que ele escreveu; o conserto do detector
está na árvore (na sonda, os quatro ganham consumidor e o escopo vai de 706 a 542 órfãos, 0 novos) e só chega
à cláusula com deploy e rebuild do índice. A linha de base do juiz não foi editada. Saída literal em
`~/claude-artefatos/touring/g1/juiz-final.out` e em `RETOMADA.md` da pasta do plano.

## Não provado / fora do alcance desta frente

- Comportamento no daemon em produção (depende de deploy).
- Memória do pipeline e latência do C8 em projeto grande: só o `touring` foi medido.
- Amostragem de `status.pipeline` por vários dias para validar o default do teto.
- `SESSION_PREFETCH_FILES = 16` não foi medido; `blast_radius_weight` do heat não tem produtor.
- A camada ponderada (Dijkstra) é o maior custo restante num hub (~16 de ~34 ms).

## Achado de medição, não corrigido (decisão de Gabriel)

O componente `index` da saúde composta está em 0,88 contra os 0,98 documentados no `CLAUDE.md`, antes e
depois do rebuild. Medido: dos 12.406 arquivos que `cli_e2e.rs::count_code_files` conta, 8.582 estão
indexados, 2.580 são elegíveis mas não têm símbolo algum a extrair (1.399 JSON), 724 são ignorados pelo git e
520 estão em `client/`, excluído do índice. A métrica divide "arquivos com símbolo" por "arquivos com
extensão de código", contados por um caminhador próprio fora da política de índice; cai sozinha conforme o
repositório acumula dados. Não é "escritor novo sem ingest" e não veio desta frente. Corrigir é redefinir um
KPI com linha de base.

## Decisões abertas com Gabriel

1. **Commit e deploy desta frente.** Forma dos commits (os hunks de C1–C8 e F1–F16 se entrelaçam em arquivos
   compartilhados) e deploy (bump + `scripts/propagate-release.sh`), seguido de `touring index rebuild`. É o
   deploy que fecha o juiz: `orphans_base` lê o índice escrito pelo binário instalado.
2. Sinais do C8 ligados por padrão (como construído, já com a regra do F13) ou `TOURING_FILE_EDGES_DISABLED=1`
   no primeiro deploy.
3. Retirar o ramo morto de `dependencies` (C8-S5) e o terceiro grafo (`file_relations`, dado de maio).
4. Plugin da barra: `cd` para o projeto em `taco-cache-refresh.sh` (cópia vendorizada, fora do repositório).
   Até o F9 ser instalado ele mantém um `touring status -j` a 99% de CPU por ~55 s a cada 120 s.
5. Refrigeração da máquina: 94–96 °C com um job pesado; o `hw-slot` só libera build Rust abaixo de 88 °C.
6. Semáforo de hardware: dar fila ou prioridade aos waiters e anunciar todos os motivos de espera (achado da
   rodada final).
7. Redefinir a cobertura de índice da saúde composta (achado acima; ticket S-01).
8. Admitir `go`/`java` e `.markdown` na tabela do parser e do índice.

Achados medidos e não corrigidos têm ticket no esforço de pendências
(`docs/plans/2026-11-13-touring-code-pendencias/`, DAG `task_1794549390311758325`).
