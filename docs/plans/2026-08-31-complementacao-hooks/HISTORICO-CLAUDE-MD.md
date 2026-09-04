---
type: Archive
title: "Item 13 do CLAUDE.md — F0.3 e as waves de sinais dos hooks"
description: "Texto integral removido do CLAUDE.md do projeto na condensação de 2026-09-02, preservado sem edição."
tags: [claude-md, historico, arquivo]
timestamp: 2026-09-02T00:00:00-03:00
plan_id: 2026-08-31-complementacao-hooks
---

# Item 13 do CLAUDE.md — F0.3 e as waves de sinais dos hooks

> Removido do `CLAUDE.md` do projeto em 2026-09-02, na condensação que trouxe o arquivo de 332
> para perto das 200 linhas recomendadas pela documentação oficial do Claude Code. O
> invariante e os gotchas ficaram lá; o texto abaixo é a narrativa completa, preservada
> sem edição para quem precisar do racional.

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
    `exit=daemon-json`, `post_bash_dispatched` 0→1). **Mecanismo (Context7 `hooks`/`hooks-guide`)**: o `if`
    aceita UMA regra de permissão, sem operadores lógicos, e falha ABERTO em comando não parseável — o
    valor com `|` nunca casava comando simples e disparava por acaso em heredoc/loop. **Fix APLICADO
    (02/09, opção B aprovada por Gabriel)**: `if` removido de `pre-bash` E `post-bash` (backup
    `settings.json.bak-20260901T235541-if-removal`); provado na mesma sessão sem restart —
    `touring index find X` lançou os dois hooks no trace, mirror +1, `hook_dispatch_by_name` pre-bash/post-bash > 0.
    Efeito colateral deliberado: o `pre-bash` agora aplica seu gate (deny em comando que COMEÇA por
    `rm -rf/-r/-f`, `dd`, `killall`, `sudo rm`; pausa de cargo sob memória RED) a todo Bash — observar
    1 semana; estreitar o regex no binário (D8) se virar fricção. Lição: 34+ hipóteses a jusante
    (stdin, breaker, standalone) de um processo que não nascia — o primeiro instrumento deve provar que
    o processo EXISTE. Memórias: `f0.3:if-removido-provado:2026-09-02` (supersedes `f0.3:causa-raiz-fechada:2026-09-02`),
    `licao:fachada-all-hooks-nao-liga-features-proprias:2026-09-01`.
    **Wave 1 (02/09, mesma sessão)**: `SignalContext` v2 (`ProposedChange` Write/Edit + `tool_name` +
    `analysable_text()`, `touring-hooks-shared/src/signal_layer.rs`) montado nos 3 pré-hooks por
    `context_for_{write,edit,read}` (`touring-hook-handlers/src/shared/signal_pipeline.rs`); layers que
    agora veem o conteúdo PROPOSTO: `AstGrepRiskSignalLayer` (pre_write+pre_edit), `PySyntaxSignalLayer`
    (Write .py), `SecretsSignalLayer` (F2.4 via `scan_text`, P0, pre_write+pre_edit), antipatterns com
    `L{n}:`; KPI `hooks_complement` em `touring kpi -j`. **Gotcha**: `cargo test -p touring-hook-handlers`
    exige `--features pre-hooks,post-hooks` (sem default features os handlers nem compilam).
    **Wave TIER-2 (02/09, DAG `task_1788318067626657742`)**: **F10** `post_bash` lê `tool_response`
    (PostToolUse) e `error` (PostToolUseFailure) — antes lia `tool_input.stdout`, que o CC não envia, e todo
    outcome gravava exit 0; registrar `touring-hook post-bash` em `PostToolUseFailure` no `settings.json`
    SÓ após deploy do binário com F10 · **F9** `HookCallEntry.origin` (`post_bash`|`sdk`; legado `null`) e a
    razão do `hooks_complement` conta só `post_bash` · **S6** `shared/quality_signal.rs` (qualidade do
    arquivo APÓS o edit, função única com pre_write) · **S8** detector único `touring_code::cwe_scan`
    (CLI `scan vulnerabilities` delega; a cópia nunca emitia CWE-22) + `CweScanLayer` wired (pre_write,
    pre_edit; `new_string:L<n>`) · **S9** `shared/api_preview.rs` POLIGLOTA: superfície pública por
    `extract_symbols` (assinatura + `is_public`), diff por ASSINATURA, badge `[rust] semantic … · pub API N
    (+a/-r)` / `[python] pub API N`; fix em `touring-code` `detect_visibility` TS/JS (`export` vive no
    `export_statement` pai) · **S4 ★** `shared/api_cascade_preview.rs`: `[cascade] \`f\` signature
    changes — N call sites break: …` (call graph do "after" + `find_references`), poliglota. Lição: edits
    aplicados por script (fora do Edit tool) não passam pelo hook de reindex — `loop_converged` acusa
    órfãos falsos até `touring index rebuild`.
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

