---
okf_version: "1.0"
type: CLAUDE
title: "Touring — Fonte Canônica (workspace do produto)"
description: "Regras e referências do workspace fonte do Touring (L1 da arquitetura rustup-like). Auto-carregado pelo harness CC ao abrir sessão neste diretório."
plan_id: docs
tags: [rules, workspace, rustup, per-project]
timestamp: 2026-08-20T11:15:00-03:00
---

# Touring — Fonte Canônica (workspace do produto)

> Este diretório é a **fonte canônica** do Touring (Pln2 produtização, movida de
> `~/.claude/rust` em 24/07/2026 — aquele root está CONGELADO, nunca desenvolver lá).
> Camada L1 da arquitetura rustup-like: L1 fonte → L2 `~/.touring/toolchains/<v>/`
> → L3 shim CC (`~/.claude/hooks/touring-hook`) → L4 `<projeto>/.touring/`.

## Regras deste workspace

1. **Rebuild SEMPRE via `update-touring`** — nunca `cargo build` standalone para deploy
   (pipeline kill→build→install→restart→verify; REGRA #19: daemon via `touring daemon-ctl`,
   jamais pkill). O daemon global roda `target/release/touring-daemon` DESTE workspace.
2. **`cargo build` aqui NÃO afeta projetos pinados** — toolchains instaladas
   (`~/.touring/toolchains/`) são cópias imutáveis. Propagar exige passo explícito:
   `touring toolchain install --from-source . <versão> --force` e, por projeto,
   `touring update --project <root>` (rollback: `touring update --rollback`).
2.1. **Propagação reprodutível em 1 comando (02/08/2026)**: `scripts/propagate-release.sh <versão>`
   encadeia gates → `update-touring` → `toolchain install` → `toolchain default` →
   `touring update` por projeto → verify. Rollback: `scripts/propagate-release.sh --rollback`.
   Flags: `--dry-run --skip-build --skip-gates --skip-freeze --no-default`.
   **Gotchas que o script encapsula** (ambos verificados por execução):
   (a) `touring update --all-projects` SEM canal resolve pelo lock de cada projeto —
   mantém a versão velha e NÃO propaga a nova; COM canal (`touring update <v> --all-projects`)
   arrasta o workspace FONTE de `dev` para `<v>`, quebrando o dev iterativo. Por isso o
   script itera projeto a projeto pulando a fonte.
   (b) `touring --version` escreve em **stderr**, não stdout — `2>/dev/null` apaga a
   versão e qualquer gate de verificação passa sem verificar nada.
   (c) o gate 5.5 (prova comportamental, 35 asserções) tem **retry-once** com
   re-espera do doctor (28/08): a estreia da prova reprovou por transiente — project
   actor drenando índice — e o operador leu EXIT=0 porque `| tail` engoliu o exit
   (`${PIPESTATUS[0]}` é a leitura correta; o script agora falha só após 2 tentativas).
2.2. **`update-touring` é versionado (18/08/2026)**: vive em `scripts/update-touring`
   e chega ao PATH por `~/.local/bin/update-touring`, um **symlink** — a mesma forma
   já usada para os binários. Antes existia só em `~/.local/bin/`: a ferramenta que
   faz todo deploy estava fora do controle de versão. Duas consequências que só
   apareceram ao versioná-la, ambas corrigidas: (a) a raiz do workspace tinha como
   default `~/.claude/rust`, a árvore **congelada** — de qualquer terminal sem
   `TOURING_WORKSPACE_ROOT` exportada, o script reconstruía e instalava a árvore
   errada em silêncio; hoje a raiz é derivada do **próprio caminho do script**;
   (b) todo check de processo perguntava "existe algum `touring-daemon`?", o que na
   topologia per-project casa os daemons de **outros** projetos — o fallback de kill
   chegava a `kill -9` neles (o cascading kill que a REGRA #19 existe para impedir),
   `start_daemon` nunca subia o daemon global, e `verify` reprovava por causa de
   processo alheio. Agora há um resolvedor único (`global_daemon_pid`) que identifica
   o **dono do socket global**. Guardado por `scripts/test_update_touring.py`
   (invariantes de conteúdo no CI; o symlink verificado no gate 1/6 do
   `propagate-release.sh`, onde o lado vivo existe).

3. **Gates (REGRA #21 — 0 falhas)**: `cargo check` + `clippy -D warnings` + testes dos
   crates tocados + `touring e2e -j` (baseline composite 0.8749) antes de declarar pronto.
   Validators por fase do programa: `docs/plans/touring-productization-pln2/validate_*.sh`.
4. **Débito conhecido**: grafo release-TEST do touring-server quebrado (E0460/E0463,
   fingerprints stale do move F4′) — grafo normal e debug-test compilam; harness E2E
   roda em debug invocando o binário release.
5. **Gotcha de sessão**: sessões CC exportam `TOURING_DAEMON_SOCKET` (precedência sobre
   o walk-up per-project). Para reproduzir o ambiente de um projeto pinado:
   `env -u TOURING_DAEMON_SOCKET -u TOURING_DAEMON_SOCK ...`.

6. **Espelho `client/` (18/08/2026)**: `client/{skills,rules,agents}` é a cópia
   versionada das skills que rodam em `~/.claude/`. **Direção única: o live é a
   fonte, `client/` é gerado.** Editar `client/` à mão não muda nada que executa e
   é desfeito pelo próximo sync. Operar: `python3 scripts/sync-client-skills.py
   --check | --apply [--prune]`. Nasceu de uma cópia em massa em 25/07 que ninguém
   comparava — foi assim que a correção de shell injection chegou ao espelho e às
   instanciações mas **nunca à biblioteca implantada** de onde `adw from-template`
   copia. Agora há três guardas: `--check` no `propagate-release.sh` (comparação
   completa, exige o lado vivo), `scripts/test_sync_client_skills.py` no CI
   (integridade contra `client/MANIFEST.sha256`, funciona sem o lado vivo) e
   exclusão estrutural de estado de runtime (`.claude/`, `__pycache__`, `*.db`).
   **Correção 19/08**: o espelho nunca adotou arquivo NOVO. `is_noise` recebia o
   caminho **absoluto**, que contém `~/.claude` — e `.claude` está na lista de
   ignorados — então todo arquivo do lado live era ruído e a varredura contribuía
   nada; só caminhos já conhecidos sincronizavam, e `--check` dizia CLEAN com
   arquivo faltando. O teste passava porque sua raiz falsa era um `tmp_path` sem
   `.claude`. `is_noise` agora **recusa** caminho absoluto — o que revelou um
   quarto sítio de chamada que a correção pontual não tinha achado.

7. **Hashtag library (v30.4.0, 12/08/2026)**: toda memória carrega facetas
   `#facet:value` (7 facetas: kind/purpose/lang/domain/process/artifact/status).
   Workflows: store auto-deriva + aceita `--tag` explícita; snippets em código
   levam codetag `// #tags: kind:… purpose:… domain:…` (post_write/edit indexam
   sozinhos; remover o codetag apaga a memória); consulta por
   `memory query "#kind:… #lang:…"`, `recall "<texto> #faceta"`,
   `memory moc <tópico>` (mapa emergente) e `portfolio "<intento> #kind:…"`.
   Guia canônico: `docs/memory-hashtag-library.md`.

8. **Portfólio de fluxos ADW (v30.4.1, 19/08/2026)**: fluxos são **compostos**, não
   copiados. `[[use]] module/as/with` faz inlining de um fragmento sob namespace
   (`recall.memory`); o kit tem 11 peças (`recall-pack · prior-art · diagnose-pack ·
   fanout-lenses · gate-rust · gate-quality50 · conflict-guard · human-approve ·
   phase-close · converge · critic-panel`). Todo fluxo publicado declara `[purpose]`
   com `when_not_to_use` — é o campo que deixa o portfólio **descartar** um fluxo em
   vez de só recomendar o mais próximo, e o minerador o indexa **antes** do cabeçalho
   para o teto de 600 chars cair no boilerplate. Fan-out `parallel` tem forma estática
   (`branches = [...]`) e **dinâmica** (`branches = "{{vars.x}}"` + `template`, clonado
   por valor em runtime — o `Send` do LangGraph); `max_branches` é verificado **antes**
   de rodar qualquer ramo. `on_branch_fail`: `all|any|ignore|best_effort|quorum:N`.
   Criar: `touring adw new` (prior-art com veredito obrigatório) · inspecionar:
   `touring adw explain` (grafo plano) · `touring adw fragments`. Bundle:
   `docs/plans/2026-08-18-graph-engineering-flow-portfolio/`.
   **Expansão do portfólio 28/08/2026** (bundle `docs/plans/2026-08-28-adw-specs-expansao/`):
   a library saltou para **19 specs** — 6 novos codificam o modus operandi que era manual:
   `release-gate` (4 gates medidos + pausa humana antes de deploy) · `memory-curation`
   (memórias antigas verificadas contra o código, veredito stale/valid/resolved) ·
   `exercise-idle-infra` (KPIs STUB/afordâncias uso-zero → exercício de estreia por item) ·
   `code-mode-adherence` (régua M1 → calibração E/A/M por failure_kind) · `adw-curation`
   (o meta-ADW: cura a própria library por evidência) · `guard-sweep` (100% dos guards;
   a estreia achou 3 falhando e forçou as correções). Todos com prior-art create_new
   registrado, lint 0 erros e evidência comportamental em promotions.json.
   **Potencialização 28/08/2026** (bundle `docs/plans/2026-08-28-adw-potencializacao/`):
   `adw race` deixou de copiar `target/`/`.git`/`.claude` por lane (era N×~30GB);
   teto do sandbox de nó alinhado ao executor real (600s, guard D8 cruzado); lint novo
   `readonly_sem_sandbox` (leitor provado ganha o convite ao CEG — 3 fragments aplicados);
   nó `agent` ganhou `retries` de TRANSPORTE (exit≠0/timeout; teto A14=3 é ERRO de lint;
   veredito segue no gate); ZTE exercitado ao vivo (bypass conformal auditado, KPI 0→0.02);
   `error-teach` promovido à library (10 runs de evidência); KPI `plan_refine_iters`
   corrigido (produtor grava `{version,iterations}`, consumidor só lia array).
   **Skills como ADWs 28/08/2026** (bundle `docs/plans/2026-08-28-skills-adw-potencializacao/`):
   library em **23 specs** — o modus operandi das 5 skills TACO virou comando:
   `cross-audit` (7 fases: harmony_map/scan_debt/prove_invariants como nós code +
   auditor com o craft + report datado) · `plan-excellence` (pipeline Pln2:
   ground_truth_collector → scaffold → author → gap_detector P0 + plan_validator
   como gate falante) · `skill-refine` (REFINE por evidência: mine_transcripts +
   quality_gate → propostas roteadas, nunca apply) · `converge-close` (judge_attest
   → loop_converged → doc_link_gate, zero agentes — Lei L2 pura); analysis-loop já
   era ADW-nativa (profile_to_adw verificado por execução, lint 0/0). Três lições
   de executor pagas pelas estreias: (a) agente headless herdava os hooks da sessão
   e o `work-outer` o capturava — `_agent_claude` agora spawna com
   `TOURING_WORK_OUTER_DISABLED=1` (nó de ADW já é gatado pelo próprio flow);
   (b) gate mudo → feedback vazio → retry degrada (13→6→0 FACT medidos) — gates
   FALANTES (a REASON ensina a correção, A5); (c) **texto de agente nunca entra no
   sandbox**: o X6 classifica o programa inteiro (posicionais inclusos) e prosa com
   cara de comando vira deny não-determinístico — lint novo
   `sandboxed_gate_reads_agent_text` (com precedência sobre o convite dual) +
   2 fragments da library corrigidos (worker-critic-pair, critic-panel); e os
   placeholders angulados `<x>` em `echo` de REASON casavam com o detector de
   redirect do `command_writes` (falso escritor) — guard da library agora nomeia
   os 4 escritores DELIBERADOS.

9. **Reivindicação atômica + Wayfinder no `decompose` (v30.4.1)**: `touring decompose
   claim <task> --owner <id> [--lease-secs N]` / `release` — `ready` só LÊ, então duas
   sessões que o consultam recebem o MESMO subtask; `claim` é um UPDATE condicional e
   exatamente uma vence. `touring decompose ticket … --kind decision|implementation
   --fog clear|hazy|unknown --origin-ticket <id>` e `frontier <task>`: decisões abertas
   **bloqueiam** a fronteira de implementação, e névoa não avaliada reporta `unknown`,
   nunca `clear`.

10. **Code mode ATIVO neste workspace (v30.4.14+, 25/08)**: `.touring/touring.toml`
    declara `[code_mode] mode = "code"` — este workspace é o **piloto** da apresentação
    por escopo. Efeito (**recalibrado no S3, 27/08**): inspeção **ISOLADA PASSA**, de
    qualquer classe — o que colapsa é a **RAJADA**: a 2ª chamada da mesma classe
    (`grep`/`cat`/`find`/`ls`/`wc`/`sed-n`) dentro de **300s** volta negada com as duas
    fundidas em 1 programa (a 1ª executou intacta). Medido em 115 transcripts: 77,5% do
    volume de inspeção está em rajadas ≥2; a lista fixa anterior cobrava 100% da fricção
    para capturar o mesmo, negando `find` (56% isolada) e isentando `sed-n`/`ls` (746
    chamadas, ~76% em rajada). `cat >` heredoc é escrita e passa. Um `touring run` zera
    a janela.
    Resolução: prefixo `TOURING_CODE_MODE=<v>` no comando → env do hook → alias
    `TOURING_CODE_ONLY=1` → `touring.toml` → default `both`. Relaxar por-comando:
    prefixo `TOURING_CODE_MODE=native` (exportar no shell NÃO chega ao hook — processos
    irmãos). Kill switch humano: `TOURING_CODE_GATES_DISABLED=1` (todos os gates).
    O `TOURING_T3_FUSE_DISABLED=1` saiu com o T3-B no S10 (27/08): o gate de turno
    media `t3_turn_fused = 0` em produção — o PostToolUse fecha o turno entre as
    chamadas, então ele nunca acumulava; o predicado de rajada do S3 o absorveu.
    **Efeito no handshake MCP (S1
    TRANSPORT, 27/08)**: um escopo que declara `code` passa a anunciar só a fachada
    search+execute — as tools MCP (nome de tool verificado no handshake, não é
    crate) `touring_search`/`touring_ctx_execute`/`touring_memory_recall`
    — em vez das ~23 curadas; antes a fachada existia mas exigia
    `TOURING_MCP_CODE_MODE=1` por sessão, então a declaração ficava desligada em
    silêncio. Só o ANÚNCIO estreita: todo tool escondido segue invocável por nome
    via `tools/call`. Precedência: `TOURING_MCP_ALL_TOOLS` > `TOURING_MCP_CODE_MODE`
    `=1` força / `=0` desliga > `touring.toml` > curada. O tipo e o parser da
    declaração são ÚNICOS (`touring_foundation::code_mode`), consumidos pelos dois
    executores — hook e handshake não podem divergir. Estratégia por contexto
    completa: `~/.claude/skills/Touring/references/code-mode-operational.md`. **Propagação
    (regra 2 ampliada)**: o rótulo da toolchain NÃO prova o build — 24/08 o lock dizia
    `30.4.14` rodando binário de 24/08. Após propagar, a prova é SEMPRE comportamental
    (um deny T3/modo-code ao vivo no projeto alvo), nunca por versão.
    **S5 (30.4.19–20, 29/08)**: o G10 também vê o **par write→run** — `cat > script.py`
    + `python3 script.py` do MESMO path (inclusive no MESMO tool_use multi-linha,
    a forma real) na janela de 600s, negado com
    `touring run --file <o próprio script>` (remédio 1:1). E dois furos fechados:
    `2>&1`/`2>/dev/null` e prefixo `VAR=...\n` não cegam mais a classe exec
    (origem: turno de 60 Bash do analise em que o replay negava só 2).
    **Aperto geral 30.4.21 (29/08, ordem de Gabriel)**: G7 re-inspeção advisory
    2ª/deny 3ª (era 3ª/5ª) · G1 rajada deny 3ª (era 4ª), hint na 2ª · G10
    exec-burst deny 5ª (era 10ª) · par write→run deny no 2º (era 3º) · G3
    edit-sem-read deny no 2º seguido (era 3º) · heredoc inline (`python3 -c`/
    `- <<EOF`) entrou na rajada como classe `python-inline` com remédio 1:1 (o
    corpo verbatim). Inalterados por medição/desenho: rajada de inspeção 2ª/300s
    (janela calibrada no joelho), G2/G8/G9 (1ª), G6 (2ª), G5 (telemetria).
    **Extensão 29/08 (ordem de Gabriel)**: python-inline READ-ONLY (corpo sem
    escrita/rede/subprocesso/DB — classificado pelo CORPO, `open(` com modo
    mutante detectado por posição do 2º arg) cai na PRÓPRIA rajada de inspeção
    (deny 2ª/300s, corpos fundidos em `--lang python`); escritor segue só no
    G10 — usado como leitura, o interpretador ganhava 4 passes onde `cat`
    ganha 1.

11. **Diretrizes de elaboração de código E/A/M (28/08/2026)**: como ESCOLHER a rota
    (E1-E6: loop/agregação/≥3 fatos → programa; colapso no executor, D8), ACERTAR o
    programa (A1-A14: SDK plana, JSON tipado, erros que ensinam, output-limit explícito,
    `touring.parallel` pool 10, prompt byte-estável, **cap 3 retries — na 4ª muda de
    estratégia**, segredos nunca no sandbox, escada de trust) e MEDIR (M1-M3: `touring
    kpi -j` → `code_mode_adherence`, piso 0.8). Corpo: `docs/code-mode.md` §Diretrizes +
    rule auto-load `~/.claude/rules/code-elaboration-directives.md`. Enforcement:
    `BestPracticesGate` (`crates/touring-quality/src/builtins/best_practices.rs`) verifica
    declaração + aderência M1.

12. **Code-mode-sinal F1-F6 entregue (2026-09-01)**: a **superfície SDK híbrida** do
    `--orchestrate` está wired fim-a-fim. **F2** S4 híbrida: 8 hooks canônicos
    hardcoded em `crates/touring-code/src/sdk.rs` + tipos derivados do
    `run_journal.jsonl` via `signal_report_from_journal` (Rust) + `scripts/gen_sdk.py`
    (CI sem toolchain). **F3** PostToolUse-sync: sink JSONL em
    `~/.claude/touring/sdk_signal_mirror.jsonl` via
    `crates/touring-code/src/sdk_signal_mirror.rs` (6 testes). **F4** SDK Python
    tipada: `record_hook_call` injetado no template in
    `crates/touring-server/src/cli/run.rs:298` + wrap `query()` que cronometra cada
    chamada. **F5** BestPracticesGate: 4ª regra `signal_use` em
    `crates/touring-quality/src/builtins/best_practices.rs` (mirror counts distinct
    hooks, threshold 6/8 = 75%, severity mantida em Warn-severo por design). **F6**
    6 critérios AND + 2 KPIs secundários: `code_mode_signal_use()` em
    `crates/touring-cli/src/cli/kpi.rs:478` + exemplo `kpi_f6_smoke.rs`. DAG
    `task_1788196388043002698` todo done; bundle completo em
    `docs/plans/2026-08-31-code-mode-sinal/` (5 phase reports, 5 typed abstracts,
    4 signal reports). **Lesson (2026-08-30, exercitada)**: rebuild parcial de
    `touring-cli` NÃO atualiza binário `touring`/`touring-daemon` (vêm de
    `touring-server`); daemon embute `touring-cli` via linkagem estática e carrega
    handlers uma vez no boot → após editar RPC handlers, sempre `cargo build -p
    touring-server --release` + `update-touring` (kill+restart). Toolchain
    `30.4.28` propagada nativamente em 01/09/2026 (commit `8d4cda0`); 3 projetos
    pinados (touring + analise + konverter) em 30.4.28 com `code_mode_signal_use`
    ativo em `touring kpi -j`. Próxima wave: PostToolUse wirar em satélites +
    measurement adoption (F6 secondary KPIs).

13. **F0.3 — entrega viva do `post-bash` (01/09/2026, Wave 0 da strategy sinais-ativos)**:
    sonda em sessão fresca provou que o PostToolUse(Bash) do Claude Code **não alimentava
    o mirror** (0/47; daemon atendeu 5/47) e que o fallback standalone do `touring-hook`
    estava **morto** em dev e toolchain — `all-hooks` da fachada `touring-hooks` só
    encaminhava a `touring-dispatch/all-hooks` e nunca ligava as features PRÓPRIAS que
    gateiam os braços standalone de `src/main.rs` (`_ => unknown subcommand`, exit 0, mudo).
    Entregue sob TDD: (a) `TOURING_HOOK_TRACE_FILE` (armado no `settings.json env`) → 1 linha
    JSON por invocação `{hook, route daemon|standalone|stateless, exit_reason, stdin_state,
    stdin_bytes, elapsed_ms, session_id}` via `atexit` (`touring-hook-runtime/src/hook_trace.rs`);
    (b) `read_stdin` tolera stdin não-bloqueante (`EAGAIN` esperado até EOF) e expõe o estado;
    (c) `all-hooks` liga `pre-hooks/post-hooks/session-hooks/utilities` + guard
    `tests/feature_parity_standalone.rs` (cfg + binário real com `TOURING_NO_DAEMON=1`);
    (d) `hook_dispatch_by_name` em `touring gate-metrics -j`. **Causa-raiz do vivo FECHADA
    (02/09, toolchain 30.4.30 propagada, trace vivo)**: o `post-bash` **nunca era lançado** — o
    registro em `~/.claude/settings.json` (PostToolUse/Bash) carrega `"if": "Bash(cargo *|rustc *|
    touring *|cd *rust*|*touring*|*cargo*|*rustc*)"` (o `pre-bash` tem o mesmo); em 8 Bash vivos o
    trace mostrou `post-bash` em 1 (texto com `["touring", …]`), e `touring index status -j` /
    `touring doctor -j | …` NÃO o dispararam, enquanto `post-tool-rl`/`post-tool-batch` (matcher
    `*`, sem `if`) rodaram em todos; quando roda, o caminho é íntegro (`route=daemon`,
    `exit=daemon-json`, `post_bash_dispatched` 0→1). Fix (settings.json, human gate): remover o
    `if` do `post-bash`. Lição: 34+ hipóteses a jusante (stdin, breaker, standalone) de um
    processo que não nascia — o primeiro instrumento deve provar que o processo EXISTE.
    Memórias: `f0.3:causa-raiz-fechada:2026-09-02` (supersedes `f0.3:post-bash-entrega-viva:2026-09-01`),
    `licao:fachada-all-hooks-nao-liga-features-proprias:2026-09-01`.
    **Wave 1 (02/09, mesma sessão)**: `SignalContext` v2 (`ProposedChange` Write/Edit + `tool_name` +
    `analysable_text()`, `touring-hooks-shared/src/signal_layer.rs`) montado nos 3 pré-hooks por
    `context_for_{write,edit,read}` (`touring-hook-handlers/src/shared/signal_pipeline.rs`); layers que
    agora veem o conteúdo PROPOSTO: `AstGrepRiskSignalLayer` (pre_write+pre_edit), `PySyntaxSignalLayer`
    (Write .py), `SecretsSignalLayer` (F2.4 via `scan_text`, P0, pre_write+pre_edit), antipatterns com
    `L{n}:`; KPI `hooks_complement` em `touring kpi -j`. **Gotcha**: `cargo test -p touring-hook-handlers`
    exige `--features pre-hooks,post-hooks` (sem default features os handlers nem compilam).
    **Wave 2 (02/09, ordem "prossiga")**: **S3** `MissingImportsLayer` (pre_write) — imports do conteúdo
    PROPOSTO via `extract_imports_resolved` (`expand_use_arg` achata `use a::{B, c::{D as E}, *}`: antes cada
    import agrupado lia como faltante; alias conta pelo alias; último segmento em vez de `ends_with`), tipos
    conhecidos por `FileKnowledgeDB::find_pub_symbols_by_name` (IN por nome, same-crate first — `pre_edit`
    migrado; antes `all_pub_symbols` varria a wiring_map inteira por edit), `use` crate-aware por
    `suggest_imports_for` (`touring_code::ast::x`/`crate::x`; o legado gerava `crate::crates::touring-code::src::…`),
    `is_builtin_type_name` única para os 3 detectores · **S10 (ex-B4)**: hooks gateiam o callgraph por
    `call_graph::supports_call_graph` (TS/JS destravados) e `enrich_with_callgraph` devolve callers DISTINTOS
    (6 chamadas de `main` liam como HOTSPOT de 6 callers) · **A3** `RelatedSymbolsLayer` (pre_write): nome que
    o arquivo novo declara e o `SymbolStore` já define noutro arquivo → `[related] … homonym (VP-Scout chain 4)`
    · **A2** `CrossCallerLayer` (pre_edit): chamada que o Edit MUDA × `find_references` → `[C08] … N other call
    sites`; o pipeline do pre_edit roda mesmo sem contexto assembled. A2/A3 v1 são determinísticos (índice de
    símbolos); a rota ANN é v2 com o mesmo gatilho. Índice nos hooks: `runtime.symbol_store()` (método).

## Referências

- Instruções do crate principal: `crates/touring-server/.claude/CLAUDE.md`
- Instruções do crate de código inteligente: `crates/touring-code/.claude/CLAUDE.md`
- Instruções do crate de qualidade: `crates/touring-quality/.claude/CLAUDE.md`
- Instruções do crate CLI: `crates/touring-cli/.claude/CLAUDE.md`
- Programa de produtização: `docs/plans/touring-productization-pln2/00-INDEX.md`
- Bundle code-mode-sinal F1-F6: `docs/plans/2026-08-31-code-mode-sinal/` (artefatos: phases/, knowledge/, sdksignal-report-f*.json)
- Estratégia canônica do bundle: `docs/plans/2026-08-31-code-mode-sinal/strategy-2026-08-31-yetzirah-v1.1.md`
- Guia da biblioteca de hashtags (facetas, codetags, MOCs): `docs/memory-hashtag-library.md`
- Constituição TACO global: `~/.claude/CLAUDE.md` (autoridade: Gabriel)
