---
type: Report
title: VL0 §6 — camada 2 genérica do motor VGP (pool sobre `Servidor`)
description: Desenho, aceite medido (comando → rc → números), mutantes, sonda de custo e o que não foi verificado da camada 2 para pyright, rust-analyzer, tsserver e deno.
plan_id: 2026-10-08-vgp-motor-base
timestamp: 2026-10-09T21:30:00-03:00
tags: ["#kind:report", "#status:done", "#domain:harness"]
---

# VL0 §6 — camada 2 genérica

Worktree `/home/gabrielgadea/projects/touring-wt-l2` · branch `feat/2026-10-09-vgp-camada2-generica` (de `88c7cc8b`) ·
build em `CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-l2` · todo `cargo` que testa/lint/constrói por
`hw-slot --class rust`. **Nada foi commitado, empurrado, instalado, propagado nem apagado.** O único `cargo` fora do slot foi
`cargo fmt -p touring-pyright` (rustfmt de um crate, milissegundos). Crate `touring-pyright` com o mesmo nome e **zero**
dependência `touring-*` (`cargo tree` abaixo).

## 1. O que entrou

| arquivo (`crates/touring-pyright/`) | papel |
|---|---|
| `src/servidor.rs` (novo) | `enum Servidor { Pyright, RustAnalyzer, Tsserver, DenoLsp }`, `Servidor::para_lang`, `trait Sessao`, `struct Verificado`, `Ambiente` (o `PATH` visto pela descoberta, injetável), marcadores de projeto (`projeto_deno`, `dirs_ate_a_raiz`) |
| `src/lsp_nucleo.rs` (novo) | a metade LSP compartilhada: overlays → abrir → diagnósticos → fechar → delta contra o disco. `Modo::{Push, Pull}` lido do `initialize`, `Perfil`, `Normalizador`, `EscolhaDeModo` |
| `src/rust_analyzer.rs` (novo) | descoberta (`rustup which`), opções, portão de quiescência, normalização E0425/E0432/E0559/E0560/E0583/E0599/E0609 |
| `src/tsserver.rs` (novo) | descoberta em `node_modules/typescript`, cliente do protocolo tsserver (não é LSP), recusas (projeto inferido, sintaxe, JS sem `checkJs`) |
| `src/deno.rs` (novo) | descoberta (`deno.json[c]`, `node_modules/.bin/deno` ou `PATH`), sessão `deno lsp` (só a fonte `deno-ts`) |
| `src/ts_diag.rs` (novo) | tabela TS2304/2552 · 2339/2551 · 2305/2724 · 2307 · 2353/2561 → achado, **uma vez** para tsserver e deno |
| `src/achado.rs` (novo) | `L2Achado`, `L2Resultado`, `Verdict`, `Escopo`, `Severidade`, `Balde`, `severidade()` movidos de `pyright.rs` (re-exportados lá: nenhum caminho antigo quebrou) |
| `src/pool.rs` | pool genérico: chave `(servidor, raiz, sub)`, arranque em segundo plano, tetos por verificador, `TOURING_VGP_L2_OFF`/`_MAX_RA`/`_WARMUP_S`; API antiga intacta |
| `src/client.rs` | + pull (`pull_diagnostics`), quiescência/saúde do `experimental/serverStatus`, código JSON-RPC em `ServerError`, `reap_child` reutilizável |
| `src/pyright.rs` | `PyrightSession` agora é a instância pyright do núcleo (push forçado); `negacao`/`escopo`/`severidade` intocados |
| `tests/{ausente,rust_real,tsserver_real,deno_real,sonda_custo}.rs`, `tests/common/mod.rs` | testes novos; `pyright_real.rs`, `pool_real.rs`, `pool_guard.rs` **sem nenhuma alteração** |

### 1.1 Desenho

**`Servidor` (enum) + `Sessao` (trait).** O pool não olha dentro de uma sessão: guarda `Box<dyn Sessao>` por
`Chave { servidor, raiz, sub }`. `sub` é onde um projeto pode ter instalações independentes — o pacote cujo `node_modules`
fornece o `tsserver`, a raiz do workspace Cargo, a raiz do workspace Deno. Efeito colateral útil: a ausência é lembrada por
chave, então um `typescript` instalado depois é **outra chave** e não herda o backoff de 5 min.

**Ciclo de vida** (as regras do pool de pyright, generalizadas):

1. *lazy* — nada sobe até a primeira edição daquela chave.
2. *pré-checagem síncrona e pura* (`Servidor::pre_checar`): existe `Cargo.toml`/`node_modules/typescript`/`deno.json`? há `rustup`/`node`/`deno`? Falhou →
   `unknown` com o motivo **na hora**, backoff de 5 min, nenhuma thread, nenhum processo.
3. *arranque*: **pyright** continua síncrono sob o lock do slot (0,4 s, como sempre). **rust-analyzer, tsserver e deno** aquecem numa
   thread destacada, **preparada com a própria primeira edição**; enquanto isso a chamada devolve `unknown: aquecendo há N s` e
   ninguém espera. O slot não segura lock durante o aquecimento (travaria todo hook do projeto); fica marcado, conta como vivo e
   nunca é despejado. Aquecimento que falha → backoff com o motivo; estourou `TOURING_VGP_L2_WARMUP_S` (180) → idem.
4. *tetos*: `TOURING_VGP_L2_MAX` (2) **por verificador** — uma linguagem não despeja a outra — e `TOURING_VGP_L2_MAX_RA` (1) para o rust-analyzer.
   Ocioso > `TOURING_VGP_L2_IDLE_S` (900) fecha; o `Drop` (shutdown → exit → kill) roda numa thread, fora do relógio do hook.
5. *saúde da sessão* (`Verificado.saudavel`): `unknown` por **prazo/servidor morto** condena a sessão (como antes); `unknown` sobre o
   **arquivo** (rust-analyzer `unlinked-file`, tsserver fora de todo `tsconfig`, sintaxe inválida) **mantém** a sessão aquecida —
   jogar fora um workspace de segundos por causa de um arquivo órfão seria pagar à toa.
6. *interruptores*: `TOURING_VGP_L2_DISABLED=1` (tudo), `TOURING_VGP_L2_OFF=rust-analyzer,deno` (por verificador),
   `habilitar_neste_processo()` (opt-in por processo, **uma** trava para todos; o motivo diz `só o daemon mantém <servidor>`, idêntico ao antigo para pyright).

**Como cada verificador entrega o diagnóstico — e como se evita ler o resultado errado:**

| verificador | meio | casamento / portão |
|---|---|---|
| pyright | LSP **push** (forçado: o comportamento medido do 1.1.408 é o contrato) | `(uri, version)` exatos; limpeza do `didClose` (`version: null`) e versão antiga ignoradas — **inalterado** |
| rust-analyzer | LSP **pull** (`textDocument/diagnostic`), exigido (sem `diagnosticProvider` não há sessão) | só se acredita num pull depois de `quiescent: true` (o rust-analyzer responde **relatório vazio** a um pull enviado durante o carregamento); resposta que atravessa um `quiescent: false` é refeita; `ServerCancelled`/`ContentModified` são repetidos até o prazo; `health: error` → `unknown`; `health: warning` (dependências não carregaram) → achados **nunca negam** (rebaixados a `warn` com o motivo na mensagem) |
| tsserver | protocolo próprio: `open → projectInfo → syntacticDiagnosticsSync → semanticDiagnosticsSync → close` | resposta casada por `request_seq` (eventos e respostas atrasadas de outros pedidos são ignorados); iniciado com `--disableAutomaticTypingAcquisition` (liga a aquisição automática de tipos — que, ligada, é um `npm install @types/*` fora do controle do Touring; o efeito de **não** passar a flag **não foi exercitado** aqui, e o wiki do Context7 não documenta a flag; o que o teste prova é que a flag vai no `argv`), `--locale en` |
| deno lsp | LSP pull (anunciado) | só `source: "deno-ts"` (descarta `deno-lint` e `deno`) |

Pull não tem versão a casar: a resposta pertence ao estado do documento quando o servidor a atende, e um servidor trata as
mensagens de uma conexão em ordem (o `didOpen` precede o pedido). O que o pull **não** garante é que o projeto terminou de carregar —
daí o portão de quiescência. O núcleo escolhe o modo pelo `initialize` (`diagnosticProvider` presente → pull), não por servidor.

**API pública.** `pool::checar(...)` e `pool::reap_idle` e `pool::habilitar_neste_processo` **iguais**. Acrescentados:
`pool::checar_em(servidor, ...)`, `pool::checar_lang(lang, ...)` (usa `Servidor::para_lang`; `html`/`css`/desconhecida → `unknown`
dizendo que não há verificador), e os tipos em `achado::*` (também em `pyright::*`). Para o motor, a troca em
`hooks/vgp_l2.rs` / `cli/vgp.rs` é `input.lang != "python"` → `pool::checar_lang(&input.lang, …)`; a fusão por `(nome, linha)` continua do motor.

### 1.2 Política de severidade por verificador (VL0 §5 aplicada)

`severidade(balde, escopo)` é a **mesma** função do Python: inline/`/tmp` negam; arquivo do projeto só nega alvo `externo`; `forma`/`dinamico` só avisam.
O que cada servidor decide é o `escopo`:

* **Rust** — E0425/E0412/E0433 `nome`/próprio · E0432 `import`: `std|core|alloc` ou crate declarada no `Cargo.toml` do crate do arquivo → `externo`, `crate|self|super` ou crate não declarada → `projeto` · E0599/E0609/E0559-com-nome `atributo`: tipo da mensagem (ou o caminho antes de `::`) em `TIPOS_STD` → `externo`, senão `projeto` · E0560/E0559 `kwarg`/`projeto` · E0583 `import`/`projeto`. **O nome sai do trecho do código sob o `range`** (UTF-16), porque `no such value in this scope` e `no such field` não trazem nome.
* **TS/Deno** — TS2304 `nome`/próprio · TS2339 membro de tipo de lib (`string`, `number[]`, `Promise<…>`, `HTML*Element`, `typeof import(...)`) → `externo`, de tipo do projeto → `projeto` · TS2305/2724 especificador instalado em `node_modules` entre o arquivo e a raiz, `node:` ou built-in → `externo`, relativo/alias/não instalado → `projeto` · TS2307 sempre `projeto` (pode ser instalado depois) · TS2353/2561 `kwarg`.
* A tabela de "tipo de lib" (TS) e `TIPOS_STD` (Rust) é **calibração, não medição**: errar para o lado `projeto`, que só avisa. O corpus VTS/VRS dirá se `externo` aguenta o limiar de falso deny ≤ 1%.

## 2. O que a sonda aprendeu errando (fatos medidos, 09/10, rust-analyzer 1.98.0 · tsserver 5.8.3 · deno 2.9.6)

* **rust-analyzer, `diagnostics.experimental.enable=false` (o padrão) acha 1 de 3** nomes inventados (só E0560); ligado, acha os 3. Sem isso, "limpo" seria mentira.
* **Pull cedo demais devolve relatório `full` vazio** (`quiescent: false`, 0,5 s) — indistinguível de "sem erros". Com o portão, 3/3.
* **Push do rust-analyzer** publica a mesma versão duas vezes com 0 diagnósticos na fixture que o pull mostra com erro: inutilizável como fonte.
* **`unlinked-file`** (severidade 4) é como o rust-analyzer diz "este arquivo não está em nenhum `mod`"; **raiz sem `Cargo.toml` responde vazio com saúde `ok`** (daí a pré-checagem).
* **Dependência que não carrega** (offline): `health: warning` + `Failed to read Cargo metadata with dependencies`, e um `E0432` para a crate *declarada* — falso `externo`. Motivo do rebaixamento.
* **Lacunas nativas do rust-analyzer** (a camada 1 / `cargo check` cobrem): função inexistente num módulo existente (`util::inventada()`), tipo não resolvido em anotação `let`, caminho de vários segmentos em expressão.
* **tsserver**: o projeto fica retido entre `close` e novo `open` (2º `open`: 0,3–0,5 ms de `updateGraphDurationMs`); `projectInfo.configFileName = /dev/null/inferredProject1*` para arquivo fora do `tsconfig` — com ruído de ambiente (`Cannot find name 'process'`); `.js` sem `@ts-check` devolve semântica vazia.
* **deno lsp** anuncia `diagnosticProvider` mesmo sem o cliente declarar pull; o relatório mistura `deno-ts`/`deno-lint`; rejeita `"params": null` (o cliente já omite `null` do fio).

## 3. Aceite

> Todos os números abaixo vêm de execuções registradas em `scratchpad/lote*.log`; denominadores explícitos.

**3.1 Suítes atuais do touring-pyright, sem alteração de asserção, inclusive `--ignored`.**
`git diff 88c7cc8b` nos três módulos de teste pré-existentes: **0 linhas removidas** contendo `assert` ou `#[test]` (as únicas assinaturas `fn` removidas são de 4 funções privadas que não são teste: `reader_loop` virou `pub(crate)`; `dispose`/`evict_oldest`/`forget_empty` do pool foram generalizadas); `tests/pyright_real.rs`, `pool_real.rs`, `pool_guard.rs`: **sem diff**. Única mudança de encanamento: `pool.rs::tests` ganhou `use crate::pyright::{PyrightError, PyrightSession}` (o corpo de `pool.rs` não importa mais `PyrightSession`).
`cargo test -p touring-pyright --no-fail-fast` → **rc 0**: lib **172/172** (era 71: client 16→24, pool 20→33, pyright 35→35, + 6 módulos novos — `deno` 4, `lsp_nucleo` 15, `rust_analyzer` 22, `servidor` 9, `ts_diag` 13, `tsserver` 17 = 80; 71+8+13+80 = 172), `ausente` 2/2, `pool_guard` 1/1.
`cargo test -p touring-pyright --no-fail-fast -- --ignored --test-threads=1` → **rc 0**: `pyright_real` **15/15**, `pool_real` 1/1 (frio 638 ms; 19 quentes p95 = 67 ms, o antigo limite é 150), `rust_real` 3/3, `tsserver_real` 3/3, `deno_real` 1/1, `sonda_custo` 1/1 (sem parâmetros, sai).

**3.2 Testes por servidor (fixture mínima em tempdir; reais em `#[ignore]`).**

| servidor | fixture | achados no proposto (delta contra o original) | original limpo |
|---|---|---|---|
| rust-analyzer | crate de 2 arquivos (`lib.rs` + `util.rs`) | **3/3**: `nome_solto` (nome, E0425), `y` (kwarg, E0560), `pusha` (atributo, E0599, `Vec` → `externo` → **deny**); linhas 15/12/14 | **0** com delta e **0** sem delta (`Allow`) |
| tsserver (TypeScript 5.8.3 de `analise/frontend/node_modules`, ligado por symlink — nada instalado) | `tsconfig` + `m.ts` + `a.ts` | **4/4**: TS2305 `naoExiste`, TS2304 `naoDefinido`, TS2339 `z` (Ponto → `projeto`/warn), TS2339 `pusha` (`number[]` → `externo`/deny) | **0** |
| deno 2.9.6 | `deno.json` + `m.ts` + `a.ts` | **4/4** (os mesmos; `deno-lint`/info não vazam) | **0** |

Ausência (testes **não** ignorados): `tests/ausente.rs` (2/2) — projeto sem `Cargo.toml`, sem `node_modules/typescript`, sem `deno.json`, sem venv: `unknown` + motivo na 1ª chamada, `backoff` + o mesmo motivo na 2ª, `achados` vazios, **0 processos filhos**; e unit tests de descoberta com `Ambiente` injetado (`rustup`/`node`/`deno` falsos) em `rust_analyzer::tests::descoberta`, `tsserver::tests::descoberta`, `deno::tests`. Também não ignorados: `abrir_*_end_to_end` de rust-analyzer e tsserver (binário falso por `rustup`/`node` falsos) e 8 testes novos do cliente (pull, quiescência, saúde, `ServerCancelled`/`ContentModified`) contra um servidor falso que reproduz o relatório vazio.

**3.3 Mutantes** — ver §4: **18/18 mortos** (os 3 pedidos — cliente que aceita versão antiga/nula: M1a/b/c; descoberta que instala ou cai no binário global: M2a–e; servidor ausente que devolve vazio em vez de `unknown`: M3a/b — mais 8 de desenho: M4–M10).

**3.4 Sonda de custo** — ver §5.

**3.5 Gates.**

| comando (sempre via `hw-slot --class rust`) | rc | resultado |
|---|---|---|
| `cargo fmt --check -p touring-pyright` | 0 | limpo |
| `cargo clippy -p touring-pyright --all-targets -- -D warnings` | 0 | 0 avisos |
| `cargo tree -p touring-pyright --edges normal,build,dev` | 0 | só `serde`, `serde_json`, `thiserror` (+ proc-macro); **nenhum `touring-*`** |
| `cargo test -p touring-hook-handlers --features pre-hooks,post-hooks --no-fail-fast` | 0 | **733/733** (+ 4 ignorados pré-existentes), 0 falhas |
| `cargo test -p touring-lsp --no-fail-fast` (4º consumidor de `touring-pyright`; fora da lista do aceite) | 0 | 11/11 + 2/2, 0 falhas |
| `cargo test -p touring-server --no-fail-fast` | 0 | lib **1652/1652** e todos os 17 binários de teste de integração verdes, **0 falhas** (ver a nota abaixo sobre os 6 binários e2e) |

**Nota — os 6 binários e2e do touring-server (REGRA #21).** Na 1ª rodada (`lote3`), 32 testes de 6 binários (`binary_e2e` 14, `cli_wave6_e2e` 5, `e2e_hook_integration` 4, `f3_update_component_e2e` 5, `graph_service_e2e` 1, `w12_5_per_project_daemon_e2e` 3) falharam por **ambiente, não por código**: esses testes procuram `touring`/`touring-daemon` em `<raiz do workspace>/target/{debug,release}` a partir de `CARGO_MANIFEST_DIR` (ignorando `CARGO_TARGET_DIR`), e o worktree só tem o `target/` compartilhado `target/wt-l2`. Construí os dois binários do worktree (`cargo build -p touring-server -p touring-hooks`, via `hw-slot`) e os liguei por **symlink** em `<worktree>/target/debug/` (gitignorado; nada fora do worktree) — as suítes testam o código do worktree. Depois: 14 + 5 + 6 + 5 + 15 (+10 ignorados) + 3 = **48/48 verdes** e, na rodada final (`lote9`), `cargo test -p touring-server --no-fail-fast` inteiro → **rc 0**. Quem reproduzir num worktree novo precisa do mesmo passo (`TOURING_BINARY` só é lido por 3 dos 6 — `binary_e2e`, `e2e_hook_integration`, `graph_service_e2e` — e só vale para o `touring`, não para o `touring-daemon`).

## 4. Mutantes (aplicados um a um na cópia do worktree, restaurados e verificados byte a byte; nenhum comando git)

Runner: `scratchpad/mutantes.py` → `scratchpad/mut/mutantes.json` e `mutantes2.json`. "Morto" = o teste que deve notar falhou.

| # | o mutante | morto por |
|---|---|---|
| M1a | o cliente aceita diagnóstico de **qualquer** versão (apaga a comparação) | `publish_match_requires_uri_and_exact_version` · `wait_publish_skips_null_stale_and_other_uri…` · `reopening_the_same_uri_never_returns_the_previous_cleanup` · `configuration_requests_are_answered…` |
| M1b | aceita **versão nula** (a limpeza do `didClose`) | os mesmos 4 |
| M1c | aceita versão **antiga** (`v <= versão`) | os mesmos 4 |
| M2a | a descoberta do tsserver **cai no `tsserver` do `PATH`** | `tsserver::…descoberta::a_global_tsserver_on_path_is_never_a_substitute` |
| M2b | a do rust-analyzer **cai no `rust-analyzer` do `PATH`** | `rust_analyzer::…descoberta::a_global_rust_analyzer_on_path_is_never_a_substitute` |
| M2c | a descoberta **instala** (`rustup component add` em vez de `which`) | `…a_toolchain_without_the_component…` · `…the_binary_is_whatever_rustup_resolves…` (a testemunha registra o `argv`) |
| M2d | a consulta ao rustup **deixa `RUSTUP_AUTO_INSTALL` ligado** | `…the_binary_is_whatever_rustup_resolves…` (a testemunha registra `AUTO=0`) |
| M2e | o tsserver sobe **sem** `--disableAutomaticTypingAcquisition` (a flag some do `argv`; o `npm install` que ela evita não foi exercitado) | `abrir_runs_the_projects_tsserver_with_typing_acquisition_off_end_to_end` |
| M3a | **servidor ausente devolve vazio (`Allow`) em vez de `unknown`** | `a_failed_pre_check_reaches_the_hook_at_once…` · `failed_discover_is_unknown_and_backs_off_for_five_minutes` · `public_checar_without_a_venv_is_unknown_then_backs_off` |
| M3b | a pré-checagem do rust-analyzer fica cega (ausência só descoberta na thread) | `ausente::every_absent_verifier_is_unknown_with_its_reason_and_starts_no_process` |
| M4 | o pull **acredita no relatório vazio** (sem portão de quiescência) | 8 testes de `lsp_nucleo::tests`, entre eles `pull_check_waits_for_quiescence_and_is_not_fooled…` |
| M5 | o tsserver aceita **a primeira resposta**, não a do `request_seq` | 7 testes de `tsserver::tests` |
| M6 | `health: error` lido como projeto limpo | `a_workspace_that_failed_to_load_is_unknown…` |
| M6b | workspace degradado (`health: warning`) **nega** | `a_degraded_workspace_never_denies…` |
| M7 | `unknown` sobre o arquivo **derruba** a sessão aquecida | `session_health_follows_the_kind_of_failure_not_the_verdict` |
| M8 | `unlinked-file` medido como limpo | `an_unlinked_file_is_unknown_never_an_allow` |
| M9 | arquivo fora de todo `tsconfig` medido num projeto inferido | `a_file_outside_every_tsconfig_is_unknown_and_keeps_the_session` |
| M10 | pânico no spawner do aquecimento **sem** `catch_unwind` (o slot ficaria `aquecendo` para sempre) | `a_panicking_warm_up_ends_in_a_backoff_not_in_a_slot_stuck_warming` (10 s esperando o backoff que nunca vem) |

Na 1ª rodada M1a/b/c e M7 saíram "vivos" com `rc 1` em 0 s: **erro meu de argumento do cargo** (um só filtro posicional por execução), não teste fraco — corrigidos em `lote5` e mortos.

**Defeito do meu runner, achado pelo M10 e corrigido:** o runner restaurava o arquivo com `shutil.copy2`, que devolve o **mtime antigo**; o cargo então considera o build do mutante "fresco" e o **reaproveita** na execução seguinte (o 1º `cargo test` depois do M10 falhou exatamente no teste que o M10 mata, com o binário do mutante). Isso só contamina a **primeira** execução depois do último mutante de uma sequência (qualquer outro mutante muda o mtime e recompila o crate inteiro a partir do disco), e as medições finais do aceite foram refeitas depois de `touch` nos fontes (`lote9`). O runner agora faz `os.utime(path, None)` ao restaurar.

## 5. Sonda de custo (risco §7 da proposta)

`tests/sonda_custo.rs` (ignorado; parâmetros por ambiente), rodada **dentro de uma única retenção do slot `rust` do `hw-slot`**, depois de o slot liberar, **sem outro job do slot concorrendo**. Verifica o arquivo como está em disco
(`conteúdo == disco`), mede o tempo da 1ª chamada até o 1º resultado medido (inclui o aquecimento em segundo plano), 15 verificações quentes, e o RSS (`VmRSS`) somado de **toda a árvore de processos** filha do teste, amostrado a cada 50 ms.

**Temperatura: a máquina esteve entre 92 e 97 °C durante todo o trabalho** (`hw-slot` esperou o "88 °C" por ~1 h 15 min; as outras sessões mantinham `cargo test`/juiz nos slots `heavy`). Os números de tempo abaixo foram tomados nessa condição (provável estrangulamento térmico): são **teto**, não mínimo.

**3 repetições por linha** (`scratchpad/lote6.log`, `sondas/rep_*_{1,2,3}.json`; mediana, e entre colchetes o intervalo). "1º diagnóstico" = da 1ª edição até o 1º resultado medido, incluindo spawn + `initialize` + carga do projeto + quiescência; "RSS" = pico da **árvore** de processos (servidor + filhos, p.ex. `proc-macro-srv`, `node`); "quente" = verificação com a sessão já viva (15 por repetição).

| verificador | projeto (arquivo medido) | 1º diagnóstico | RSS pico | quente p50 / p95 |
|---|---|---|---|---|
| rust-analyzer | fixture de 2 arquivos (`src/lib.rs`, 95 B) | **2,16 s** [2,16–2,41] | **550 MiB** [550–550] | 0 / 0–1 ms |
| rust-analyzer | cópia **standalone** do `touring-pyright` (9.329 linhas, fora do workspace; `src/pool.rs`, 72 KB) | **2,67 s** [2,58–2,78] | **632 MiB** [631–632] | 12 / 16–18 ms |
| rust-analyzer | `crates/touring-pyright` **como membro do workspace touring** (`src/pool.rs`, 73 KB) | **4,23 s** [4,13–4,38] | **1.471 MiB** [1.424–1.474] | 11–12 / 13–17 ms |
| rust-analyzer + `buildScripts`/`procMacro` (`TOURING_VGP_L2_RA_BUILD=1`) | fixture | 2,26 s [2,21–2,36] | 601 MiB [597–616] | 0 / 1 ms |
| rust-analyzer + `buildScripts`/`procMacro` | standalone | 2,98 s [2,93–4,74] | 719 MiB [693–724] | 13–16 / 15–18 ms |
| tsserver 5.8.3 | `analise/frontend/constants.ts` (19,8 KB) | **2,11 s** [1,96–2,26] | **755 MiB** [754–756] | 2–3 / 3–5 ms |
| tsserver 5.8.3 | `analise/frontend/App.tsx` (7,2 KB; 1 repetição) | 2,37 s | 765 MiB | — |
| deno 2.9.6 | fixture com `deno.json` (`a.ts`) | **0,61 s** [0,61–0,76] | **312 MiB** [310–315] | 5–8 / 9–14 ms |

Em todas as linhas o 1º resultado foi `Allow` com `motivo: null` (arquivo original, limpo) e `encerrou_sem_orfaos: true` (nenhum processo filho vivo depois do teste). A temperatura no início de cada repetição ficou em 86–97 °C (mediana 95).

**Leitura para o risco §7:**

* **O custo do rust-analyzer é o do workspace, não o do arquivo.** O mesmo crate custa 632 MiB / 2,7 s isolado e **1,47 GiB / 4,2 s** dentro do workspace touring (o servidor carrega o grafo inteiro de membros, mesmo com `checkOnSave` e `cachePriming` desligados). O workspace touring **inteiro** como raiz não foi medido (por ordem) e é o caso que o daemon enfrentará; trate os 1,47 GiB como **piso** para edição em crate do touring.
* **O custo de manter quente é a memória, não o tempo.** Uma verificação quente custa 0–18 ms em todos os verificadores; o que se paga é o 1º diagnóstico (0,6–4,2 s, sempre em segundo plano) e a RSS residente (0,3–1,5 GiB por sessão viva).
* **`buildScripts`/`procMacro` ligados** adicionaram +51 MiB (fixture) e +87 MiB (standalone) e ~0,3 s na mediana, e só em projetos pequenos — por isso ficam desligados por padrão: o ganho (diagnósticos que dependem de macro procedural) não foi medido, e o custo num workspace com `build.rs` pesado também não (§7).
* **Teto agregado (aritmética sobre as medianas, não medição conjunta):** com os tetos padrão (`MAX_RA=1`, `MAX=2` por verificador) o pior caso com tudo quente ao mesmo tempo num daemon é ≈ 1,5 GiB (rust-analyzer) + 2 × 0,76 GiB (tsserver) + 2 × 0,31 GiB (deno) ≈ **3,6 GiB**, **sem** o pyright (não medido aqui); cada sessão ociosa por mais de 900 s é fechada. Numa máquina com 6+ sessões, ligar o rust-analyzer em **sombra** (`TOURING_VGP_L2_OFF` tirando-o) é a decisão conservadora até a VRS medir o ganho.

## 6. Fontes consultadas (Context7)

* **Language Server Protocol 3.17** (`/websites/microsoft_github_io_language-server-protocol_specifications_lsp_3_17`): pull diagnostics (`textDocument/diagnostic`, `DocumentDiagnosticParams`, `FullDocumentDiagnosticReport`/`UnchangedDocumentDiagnosticReport` — "um servidor só pode devolver `unchanged` se `resultId` foi enviado"), `LSPErrorCodes.ServerCancelled` (-32802, `DiagnosticServerCancellationData.retriggerRequest` padrão `true`), `publishDiagnostics.version` ("a versão do documento para a qual se publica"; ausente/nula quando fechado), `DiagnosticClientCapabilities`.
* **rust-analyzer** (`/rust-lang/rust-analyzer`): `rust-analyzer.checkOnSave`, `diagnostics.enable` / `diagnostics.experimental.enable` / `diagnostics.disabled`, `DiagnosticCollection` (três fontes independentes — `native_syntax`, `native_semantic`, `check` — fundidas em `diagnostics_for`: é por isso que o **push** pode publicar só a sintaxe primeiro), `cargo.buildScripts`/`procMacro`, `rust-analyzer/runFlycheck`.
* **TypeScript — Standalone Server (tsserver)** (`/microsoft/typescript`, wiki): formato de mensagem (requisições JSON por linha; respostas e eventos com `Content-Length`), `open`, eventos `semanticDiag`/`syntaxDiag`, lista de comandos em `ts.server.protocol.CommandTypes`. O wiki **não** documenta `semanticDiagnosticsSync`/`projectInfo`/`--disableAutomaticTypingAcquisition`: esses foram confirmados **por execução** contra o 5.8.3 do `analise/frontend` (probe `tsprobe*.py`), não pela doc.
* **Deno** (`/denoland/docs`): `deno lsp`, `initializationOptions { enable, unstable, … }`, `deno check` como o papel do `tsc --noEmit` no editor. Idem: `source: "deno-ts"` / pull foram **observados**, não lidos.

## 7. O que NÃO foi verificado

* **Precisão** (recall por classe, falso deny ≤ 1% por balde): não medida. A calibração `externo`/`projeto` (tipos de lib TS, `TIPOS_STD` Rust, dependência declarada) é hipótese de projeto; o juiz da VL0 §7 decide.
* **TSX/JSX/JS com servidor real**: o `scriptKindName` e o portão `checkJs`/`@ts-check` estão cobertos por servidor falso e por 2 sondas de arquivo `.tsx` limpo do `analise`; não há teste de erro inventado em `.tsx`/`.js` real.
* **rust-analyzer com `buildScripts`/`procMacro` ligados** (`TOURING_VGP_L2_RA_BUILD=1`): só medido em projetos pequenos (§5); em workspace grande ele roda um `cargo check` de build scripts — minutos e GB, **não medido** (a ordem era não indexar o workspace inteiro).
* **O workspace touring inteiro** no rust-analyzer: não medido, por ordem. O que se vê em §5 é o crate membro `touring-pyright` como raiz — o rust-analyzer carrega o **grafo do workspace** mesmo assim.
* **`deno lsp` e módulos remotos** (`https:`, `jsr:`, `npm:`): não há chave de "offline" no `deno lsp`; a fixture não tem import remoto, então se ele busca rede ao analisar um projeto real **não foi observado**.
* **pnpm/yarn PnP**, `tsserver` de versão ≠ 5.8.3, `deno` ≠ 2.9.6, rust-analyzer ≠ 1.98.0 (`TIPOS`, mensagens e códigos podem mudar).
* **Concorrência real** entre vários roots e linguagens no daemon (há testes de pool com sessões sem processo e um teste de 4 threads × 5 chamadas para pyright, não para os novos); **o daemon real** (`touring-hooks`) com o pool novo: compila e sobe nos e2e `w12_5_per_project_daemon_e2e` (3/3), mas nenhuma edição passou por `checar_lang` dentro dele — o motor ainda não chama a API nova.
* **Um `pyright` futuro que anuncie pull**: o núcleo o mantém em push (`EscolhaDeModo::Forcado`), então nem foi cogitado — não há medição de pull no pyright.
* **Linux apenas** (`/proc`, `PermissionsExt`).

## 8. Decisões que ficam com a coordenação

1. **Defaults de teto**: `MAX=2` por verificador e `MAX_RA=1`. O teste antigo de `Config` fixa `max == 2`, então o teto **continua 2, mas por verificador**.
2. **`TOURING_VGP_L2_OFF`** não está setado por padrão: o rust-analyzer fica **ligado** assim que o motor chamar `checar_lang("rust", …)`. Dado o custo de §5, recomendo ligar a VRS em sombra com a trava do daemon.
3. **TS2307 (módulo não encontrado)** entra como `import`/`projeto` (só avisa); o Python não nega `reportMissingImports`. Uma linha em `ts_diag.rs::REGRAS` se preferirem paridade estrita.
4. **Integração no motor**: trocar `input.lang != "python"` por `pool::checar_lang(&input.lang, …)` em `hooks/vgp_l2.rs` e `cli/vgp.rs` (não toquei — fora do escopo e a árvore está congelada).
