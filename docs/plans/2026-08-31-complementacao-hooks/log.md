---
type: Log
title: "Log — 2026-08-31-complementacao-hooks"
description: "Chronological history of the phases closed in this bundle."
plan_id: 2026-08-31-complementacao-hooks
okf_version: 1.0
tags: ["#kind:log", "#artifact:log"]
timestamp: 2026-08-31T15:54:05.750649-03:00
---

# Log — 2026-08-31-complementacao-hooks

Cada entrada é um fecho de fase registrado por `loop_phase_close.py`.
O plano: [`plan.md`](/plan.md)

## 2026-08-31T15:54:05.750649-03:00 — F1 done

F1 specs handler-by-handler concluída: 15 specs (H1-H15) entregues em docs/plans/2026-08-31-complementacao-hooks/specs/H1-H15.md. Cada spec inclui evento+matcher+comando+JSON schema+latência budget+teste de aceitação. VGP rodada: 10/15 subcmds CLI JÁ existem; 4 precisam ser criados em F1.5 (H4 pub-api, H6 scan vulnerabilities, H11 find references, H12 identity derive). Padrão arch:generator-hooks-integration (subprocess fire-and-forget) aplicado a todos. Falta: atualizar DAG para incluir F1.5 como subtarefa intermediária.

## 2026-09-01T17:52:06.044953-03:00 — F0-fix-medicao-alias-kpi-entrega done

F0 done: (1) template orchestrate grava nome CANONICO (typed name ou reverse-alias; forward alias so no transporte — pin de 1 ocorrencia); (2) signal_use_from_lines pura: used = intersecao com HookName::ALL, non_canonical_calls visivel — PROVADO VIVO pos-deploy: mirror gravou ast_blast canonico e KPI leu {used:5, ratio:0.625, non_canonical:9} (era ratio 1.0 inflado); (3) feeder post_bash LOUD + diagnostico da entrega: handler correto por invocacao direta (payload realista -> mirror +1), shim resolve toolchain 30.4.29 correta, SEM overrides em settings.local — o evento da SESSAO viva nao alcanca o handler; proxima sonda GRATIS: sessao CC fresca (hipotese snapshot de hooks); fallback: wrapper tee no settings.json (gate humano). RED-GREEN nos 3 fixes; 1575+491+634 verdes.

## 2026-09-01T17:52:06.337148-03:00 — A1-check-compile-layer done

A1 done: modulo check_compile (touring-hook-handlers): resolve_crate_name (walk-up ate [package]), debounce 30s por crate, spawn detached de cargo check -p --message-format=short (reparent via disown, reaper anti-zombie), veredito entregue EXATAMENTE 1x (rename .delivered) como 1 linha densa; attach_check_signal wira post_edit (wrapper run_returning) e post_write (impl+wrapper) — Allow vira Context quando ha veredito. Fecha P9 verify-after (medido 17%) por afordancia (D8). 7 testes do modulo + suite 634/634 com --features post-hooks (descoberta: a suite default NAO compila post-hooks — gates finais devem usar a feature), clippy 0. Prova viva no proximo deploy (fim da wave).

## 2026-09-01T17:55:21.042744-03:00 — B5-tdg-grade-layer done

B5 resolvido por VERIFICACAO, sem codigo: pre_edit.rs:936-973 ja computa o TDG composite (TdgReport::from_components), emite Q-220 diagnostic APENAS em grades D/F (to_diagnostic_opt), com grade_letter + composite no tracing — exatamente a letra + acao STOP que a exploracao propunha, ja em producao desde a Wave 12 (2026-04-27). A exploracao tinha a ressalva certa: verificar antes de implementar (VP-Scout Cadeia 3, Already Implemented). Nenhuma mudanca necessaria; candidato encerrado como ja-coberto.

## 2026-09-01T17:58-03:00 — recon A2 (pré-pausa)
- `touring assist applicable` = refactorings POSICIONAIS (auto_import/extract_function, 53ms) — NÃO é o sinal de sítios análogos do A2; o candidato exige rota C08/ANN-similaridade (design real, não wiring).
- A2/A3/B4/F9 pendentes; decisão de pausa/continuação com Gabriel.

## 2026-09-01T17:57:46.909592-03:00 — PreCompact snapshot

Loop active. Pending: [A2-assists-cross-caller-layer,A3-related-symbols-layer,B4-call-graph-compacto-layer,F9-regua-hooks-complement-kpi]. Resume: `touring decompose ready task_1788294728027014117`.

## 2026-09-01T18:15 — Exploração sinais ativos pré-write (agente prewrite-wiring)

Achados estruturais (evidência com path:linha, agente read-only):

1. **`ctx.source` morto em produção**: os 3 call sites passam `""` — `pre_write.rs:182`, `pre_edit.rs:464`, `pre_read.rs:970`. `CweScanLayer::enrich` faz `detect_cwes(ctx.source)` → devolveria 0 achados sempre.
2. **`SignalContext` sem campos de mutação** (`signal_layer.rs:14-25`): nem `new_string`/`old_string`/`tool_name` — layer não distingue Write de Edit nem vê o que muda.
3. **`AstGrepRiskSignalLayer` lê do DISCO** (`ast_grep_signal.rs:256-265`) e só está registrado em `pre_read` (`pre_read.rs:962`) — risco roda quando Claude LÊ, nunca quando ESCREVE. Motor por string existe: `scan_source_cached` (`ast_grep_signal.rs:163`, usado pelo CEG `static_stage.rs:109`).
4. **Órfãos** (REGRA #0): `CweScanLayer` (`touring-hook-runtime/src/shared/scan.rs:209`; detector DUPLICADO em `touring-server/src/cli/scan.rs:54`), `TemporalDriftLayer` (`drift.rs:79`) — zero `add_layer`.
5. **`pre_edit` não usa o pipeline como pipeline**: ~350 linhas de `push_str(" | ")` imperativo (`pre_edit.rs:125-449`) num único `StaticSignalLayer` score 1.0 (`:458-461`).
6. **TS/JS têm call graph na lib e são inalcançáveis pelos hooks**: `call_graph.rs:118` despacha rust/py/ts/js, mas `pre_edit.rs:1416-1418` + `callgraph_signal_for_write` filtram só `.rs`/`.py`.
7. **3 enums de linguagem divergentes**: `ast::Lang` (11+2), `polyglot::Lang` (22), `detect_language()` (17 strings) — nenhum superconjunto.
8. **Quality gates sem eixo de linguagem**: 12/18 builtins são Stub/external (clippy/fmt/cargo-deny) — "agnóstico" = só enforça Rust, silencioso no resto.
9. Budget do pipeline é de CARACTERES, não tempo; sem deadline por layer (alvo declarado `pre_write.rs:14`: <50ms).

Lente externa (Context7): Semgrep — severity de security = likelihood × impact × confidence (metadata obrigatória CWE/OWASP); ast-grep — severity error/warning/info/hint + note pedagógica + composição all/any/inside/has.

## 2026-09-01T18:30 — Checkpoint de desligamento (ordem de Gabriel)

Estado completo persistido para retomada: memória-mestre `retomar:sinais-hooks-signal-layer:2026-09-01` + 4 satélites `pendente:{A2,A3,B4,F9}...` linkados + `strategy:sinais-ativos-pre-write:2026-09-01` + RETOMAR-AQUI.md neste bundle. Strategy aguarda HUMAN GATE (rota A recomendada). DAG task_1788294728027014117 segue 3/7.

## 2026-09-01T21:35-03:00 — Retomada (sessão 5ec1030b): OUTER re-executado + sonda F0.3 NEGATIVA

- OUTER determinístico refeito nesta sessão (gate exige mtime ≥ arming): `strategy-loop` (evidence pass), diagnostic
  `diagnostics/touring-20260901T202445.md`, explore ledger `sinais-ativos-pre-write--signalcontext-v2---wiri` convergido
  (external marcada visitada com nota Context7), strategy emendada para v1.1.
- **Sonda F0.3 negativa**: 0/47 Bash vivos alimentam o mirror; post_bash atendido pelo daemon em 5/47. Provas, hipóteses
  falsificadas (34+ sondas) e próximo passo em 1 comando: memória `f0.3:post-bash-entrega-viva:2026-09-01` + §Emenda v1.1
  da strategy. Defeitos colaterais achados: standalone `post-bash` morto na toolchain 30.4.29; `read_stdin` degrada mudo.
- Verificado: S0 premissa vale — `SignalContext::new(&rel_path, "")` em
  `crates/touring-hook-handlers/src/hooks/{pre_write.rs:182,pre_edit.rs:464,pre_read.rs:969}` (paths corrigidos).
- HUMAN GATE reapresentado com rota **A′** (Wave 0 = F0.3a–d → Wave 1 = S0+TIER-1 → Wave 2 = TIER-2).

## 2026-09-01T22:20-03:00 — Wave 0 (F0.3a–d) EXECUTADA sob TDD (rota A′ aprovada por Gabriel)

- RED provado: cfg `pre-hooks`/`post-hooks` OFF no bin, `unknown subcommand: post-bash`, sem trace; compile-RED nos testes de
  `read_all_with_timeout` e `hook_dispatch_by_name`. GREEN: 10 testes hook-runtime + 4 feature_parity (binário real) + 1
  foundation. REVERT-PROOF: sem o tratamento de `WouldBlock`, `nonblocking_socket_with_late_shutdown_is_read_whole` falha.
- Gates: `cargo clippy -p {foundation,hook-runtime,hooks,dispatch,cli} --all-targets -D warnings` limpo; suítes lib 1325+498+421+499+9 = 0
  falhas; `touring e2e -j` overall 0.845 pass (wiring 0.68 / ast 0.78 em warn, pré-existentes); `cargo check -p touring-cli -p touring-dispatch` ok.
- Causa-raiz do F0.3c: `all-hooks = ["touring-dispatch/all-hooks"]` nunca ligava as features próprias da fachada → todos os braços
  standalone compilados fora. Causa-raiz do VIVO segue aberta; instrumento `TOURING_HOOK_TRACE_FILE` armado no settings.json (autorizado).
- Sem deploy/restart (decisão de Gabriel): binários live seguem toolchain 30.4.29. Próximo: propagar (`propagate-release.sh` ou
  `toolchain install --from-source` + `touring update`) → 1 Bash vivo → ler `~/.claude/touring/hook_trace.jsonl`.
- DAG: +S0 e TIER-1 (S1,S2,S3,S5,S7) dependentes de F0.3*; A2/A3/B4 passam a depender de S0 (B4 ≡ S10).
- Achado colateral: `~/.claude/settings.json:954` contém literal classificado como segredo pelo gate F2.4 (pré-existente) — bloqueia
  o Edit tool nesse arquivo; a linha do trace entrou por bypass por-comando com backup `settings.json.bak-f03a-*`.

## 2026-09-01T21:58:25.191751-03:00 — F0.3a-hook-trace-file done

hook_trace.rs (touring-hook-runtime): 1 linha JSON por invocacao do touring-hook via atexit quando TOURING_HOOK_TRACE_FILE esta set (route daemon|standalone|stateless, exit_reason, stdin_state/bytes, elapsed_ms, session_id); wired em main.rs em todos os pontos de saida; env armado no settings.json (autorizado por Gabriel). 5 testes unitarios + 3 testes com o binario real.

## 2026-09-01T21:58:25.419854-03:00 — F0.3b-read-stdin-loud done

read_stdin tolera stdin nao-bloqueante (EAGAIN esperado ate EOF/deadline) via read_all_with_timeout; StdinReadState + last_stdin_read expostos para o trace; warn em empty/timeout/read-error/parse-error. 5 testes com socketpair (nonblocking+late shutdown, blocking, empty, timeout, nomes); revert-proof executado (sem o tratamento de WouldBlock o teste falha).

## 2026-09-01T21:58:25.652442-03:00 — F0.3c-standalone-post-bash-parity done

Causa-raiz: all-hooks da fachada touring-hooks so encaminhava a touring-dispatch/all-hooks e nunca ligava pre-hooks/post-hooks/session-hooks/utilities proprias -> todos os bracos standalone de main.rs compilados fora (unknown subcommand em dev e toolchain). Fix no Cargo.toml + guard tests/feature_parity_standalone.rs (cfg + binario real spawnado com TOURING_NO_DAEMON=1: post-bash vivo, trace route=standalone, stdin ok, session_id).

## 2026-09-01T21:58:25.876104-03:00 — F0.3d-hook-dispatch-by-name done

record_hook_dispatch_named/hook_dispatch_by_name em touring-foundation gate_metrics (OnceLock<Mutex<BTreeMap>>, fora do snapshot para nao quebrar goldens); chamado no ator do daemon a cada dispatch; exposto como chave irma hook_dispatch_by_name em touring gate-metrics -j (cli/gate.rs). Teste unitario.

## 2026-09-01T22:05:48.341725-03:00 — S0-signal-context-v2 done

SignalContext v2 (touring-hooks-shared/signal_layer.rs): ProposedChange {Write{content} | Edit{old_string,new_string}} + campos tool_name/proposed + with_tool_name/with_proposed/analysable_text (proposta vence source; vazio cai em source). Call sites reescritos via helpers unicos context_for_write/edit/read em shared/signal_pipeline.rs (pre_write.rs, pre_edit.rs, pre_read.rs); literal do e2e atualizado. Descoberta: em pre_write/pre_edit o pipeline e 1 StaticSignalLayer com contexto pre-montado — os analisadores de conteudo rodam fora dele; S0 destrava plugar layers TIER-1 reais no pipeline.

## 2026-09-01T22:08:31.675511-03:00 — F9-regua-hooks-complement-kpi done

hooks_complement em touring kpi -j (kpi.rs): hook_dispatch_by_name (daemon, F0.3d) desde hook_dispatch_since_epoch (novo em gate_metrics) x entregas canonicas ao mirror na mesma janela (ts>=epoch; HookName::ALL; alias contados a parte) -> post_bash_dispatched, mirror_deliveries_since, post_bash_delivery_ratio (null sem despacho; available:false antes do 1o despacho). E a regua que a sonda F0.3 nao tinha. 3 testes puros (hooks_complement_from).

## 2026-09-01T22:15:26.679012-03:00 — S1-ast-grep-source-layer done

AstGrepRiskSignalLayer::enrich prefere ctx.analysable_text() (conteudo proposto, SignalContext v2) via scan_source_cached e cai para scan_path_cached sem proposta (pre_read intacto). Layer registrado nos pipelines de pre_write e pre_edit (ja estava em pre_read). Testes: 2 novos no layer (arquivo inexistente + proposta vence disco limpo), 8/8 layer_tests; handler pre_write flags [risk] unwrap=1 em arquivo novo (test_pre_write_flags_unwrap_in_proposed_content_of_a_new_file); pre_write 68/68, pre_edit+pre_read 204/204 com --features pre-hooks,post-hooks.

## 2026-09-01T22:15:26.917756-03:00 — S7-py-syntax-layer done

PySyntaxSignalLayer (hooks-shared/qa_syntax.rs): parse tree-sitter do conteudo proposto de todo Write .py antes de gravar; Edit (fragmento) e pre_read pulados com razao declarada (E4). Score 0.95, mensagem ensina a correcao (A5). Registrado no pipeline de pre_write; qa_syntax re-exportado em crate::shared (hook-handlers/lib.rs). Testes: 3 no layer + handler test_pre_write_flags_python_syntax_error_in_proposed_file.

## 2026-09-01T22:17:19.615566-03:00 — S5-antipatterns-with-lines done

pre_write::antipattern_signals passa a usar detect_antipatterns_with_lines e prefixa cada aviso com L{linha} (linha 0 fica sem prefixo) — sinal acionavel (A5) sobre o conteudo PROPOSTO. maybe_add_eval_check preservado. Teste test_pre_write_antipattern_signal_carries_line_numbers (RED comportamental -> GREEN); suite pre_write 69/69 com features; clippy handlers (features de producao) limpo.

## 2026-09-02T01:20-03:00 — Wave 1 (S0 · F9 · S1 · S7 · S5 · S2) executada sob TDD nesta mesma sessão

- **S0** SignalContext v2 (`ProposedChange`, `tool_name`, `analysable_text`) + helpers `context_for_{write,edit,read}`.
- **F9** `hooks_complement` em `touring kpi -j` (despachos por hook × entregas ao mirror na mesma janela; `hook_dispatch_since_epoch`).
- **S1** `AstGrepRiskSignalLayer` lê o conteúdo proposto; registrado em pre_write e pre_edit.
- **S7** `PySyntaxSignalLayer` (Write .py); **S5** antipatterns com `L{n}:`; **S2** `scan_text`/`SecretScan` no F2_4 + `SecretsSignalLayer` (P0) em pre_write e pre_edit.
- Evidência: cada item RED→GREEN (S1/S5 com RED comportamental; S0/S7/F9 compile-RED); suítes com `--features pre-hooks,post-hooks`
  (pre_write 73/73, pre_edit+pre_read 204/204), hooks-shared 4+8+3, quality 3, touring-cli 3; clippy `-D warnings` limpo em
  shared/handlers(produção)/fachada/quality/cli/foundation; `touring e2e` 0.8456 pass (wiring 0.68 / ast 0.78 warn pré-existentes).
- Pendente para a próxima sessão: S3 (design em `design:S3-missing-imports-layer:2026-09-02`), TIER-2 (S4/S6/S8/S9/S10) e TIER-3 (A2/A3);
  propagação da toolchain + leitura de `hook_trace.jsonl` (causa-raiz do vivo); commit da branch safety.

## 2026-09-01T22:21:07.703912-03:00 — S2-secrets-entropy-layer done

F2.4 no pre: scan_text/SecretScan/ALLOW_SECRETS_PRAGMA expostos em touring-quality f2_4_secrets.rs (mesmo detector do gate P0, sem duplicacao) + SecretsSignalLayer (hook-handlers shared/secrets_signal.rs) sobre o conteudo PROPOSTO (Write content / Edit new_string), score 1.0, nunca ecoa o valor, honra o pragma; registrado em pre_write e pre_edit; dep touring-quality adicionada aos handlers (ja no grafo via ceg). Testes: 3 quality + 3 layer + 1 handler (test_pre_write_flags_hardcoded_secret_in_proposed_new_file).

## 2026-09-01T23:00:34.363483-03:00 — S3-missing-imports-layer done

S3 MissingImportsLayer entregue: imports do CONTEUDO PROPOSTO via extract_imports_resolved (use-lists agrupados agora achatados por expand_use_arg — antes cada import agrupado lia como faltante; alias conta pelo alias; ultimo segmento em vez de ends_with); fonte dos tipos conhecidos = find_pub_symbols_by_name (IN indexado por nome, same-crate first) em vez de all_pub_symbols (scan da wiring_map inteira por hook) — pre_edit migrado para a mesma fonte; suggest_imports_for gera use path crate-aware (touring_code::ast::x vs crate::x, lib.rs/mod.rs colapsados; legado crate::crates::touring-code::src corrigido); lista unica de builtins is_builtin_type_name para os 3 detectores; layer 'missing_imports' no pipeline do pre_write com lookup lazy (arquivo limpo = 0 DB); teste de integracao prova o sinal [import] com o path concreto.

## 2026-09-01T23:00:34.632237-03:00 — B4-call-graph-compacto-layer done

B4 re-escopado como S10 entregue: callgraph dos hooks destravado para TS/JS — pre_edit.callgraph_signal_for_file e pre_write.callgraph_signal_for_write gateiam por touring_code::ast::call_graph::supports_call_graph (predicado unico ao lado do dispatch, debug_assert de concordancia) em vez da lista local .rs/.py; compactacao: enrich_with_callgraph agora devolve callers/callees DISTINTOS (main chamando helper 6x lia como HOTSPOT de 6 callers e 'callers: [main, main, main, main, main (+1 more)]').

## 2026-09-01T23:09:54.866839-03:00 — A3-related-symbols-layer done

A3 RelatedSymbolsLayer (pre_write) entregue com motor deterministico: nomes que o arquivo PROPOSTO declara (struct/enum/trait/type/union/fn; class/def; class/function/interface/type/enum) e que o SymbolStore (runtime.symbol_store().find_symbol) ja define em OUTRO arquivo viram '[related] X already defined in f:l (+N more) — homonym (VP-Scout chain 4)'; nomes genericos/curtos nunca chegam ao indice (MAX 8 nomes/arquivo); sinal so em Write. Rota ANN (embedding de sitios) fica como v2 — nada aqui a fecha. Modulo touring-hook-handlers/src/shared/related_symbols.rs.

## 2026-09-01T23:09:55.106126-03:00 — A2-assists-cross-caller-layer done

A2 CrossCallerLayer (pre_edit) entregue: quando o Edit MUDA uma chamada (conjunto de expressoes name(...) sem whitespace difere entre old/new; definicoes fn/def/function e macros name!( nao sao chamadas; keywords e nomes <4 chars excluidos), o SymbolStore (find_references) responde os OUTROS call sites -> '[C08] total changes here and has N other call sites in M files: f:l …' (cap 5 callees, 4 sites nomeados). O pipeline do pre_edit passa a rodar tambem quando so este sinal existe (antes exigia contexto assembled nao-vazio). Motor deterministico; a rota ANN de sitios analogos fica como v2 com o mesmo gatilho.

## 2026-09-01T23:20:00-03:00 — convergência medida, commit e propagação iniciada

- `loop_converged.py` exit 0: DAG 17/17 · quality Platinum 0.937 · 0 P0 · escopo inteiro · orphans baseline (5389) · cargo check verde (test+clippy rodados à mão nos 4 crates tocados: touring-code 700, hooks-core 488, storage(knowledge) 247, handlers(prod) 674; clippy -D warnings limpo) · `touring e2e` 0.846.
- Commit `4440fb1` na branch `safety/2026-08-31-audit-closure` (waves 0-2, 87 arquivos; working tree limpo).
- Bump `Cargo.toml` 30.4.29 → 30.4.30 e `scripts/propagate-release.sh 30.4.30` disparado (gates → build release + update-touring → freeze → default → update por projeto → prova comportamental). Veredito do `hook_trace.jsonl` (causa-raiz F0.3) na entrada seguinte.
- Instrumentos que só existem no binário novo: `hook_dispatch_by_name` em `touring gate-metrics -j` (vazio no daemon 30.4.29) e `hooks_complement` em `touring kpi -j` (null no 30.4.29) — medidos ANTES do restart para baseline honesta.

## 2026-09-01T23:45:00-03:00 — propagação 30.4.30 verificada · causa-raiz F0.3 FECHADA

- Propagação: build release 6m45s · update-touring (daemon global PID novo, 30.4.30) · toolchain congelada e default · `analise`/`konverter` 30.4.29→30.4.30 · prova comportamental 39/39 · verify OK. Doctor: `wiring_diagnostic` warning (pré-existente desde o início da sessão).
- `hook_trace.jsonl` existe (a env `TOURING_HOOK_TRACE_FILE` do `settings.json` alcança os hooks) — 48 linhas ao fim da forense; `route/exit_reason/stdin_state` presentes em todas.
- **Veredito**: o `post-bash` NÃO é lançado pelo Claude Code na maioria dos Bash. Registro em `~/.claude/settings.json`: `touring-hook post-bash` com `"if": "Bash(cargo *|rustc *|touring *|cd *rust*|*touring*|*cargo*|*rustc*)"` (idem `pre-bash`). Em 8 Bash após o restart, `post-bash` apareceu no trace em 1 (heredoc python com o texto `["touring", …]`/`projects/touring/`); `echo …` (probe A), `touring index status -j` (probe B) e `touring doctor -j | …` não o dispararam — `post-tool-rl`/`post-tool-batch` (matcher `*`, sem `if`) rodaram em todos. Quando roda: `route=daemon`, `exit=daemon-json`; `hooks_complement.post_bash_dispatched` 0→1.
- Consequência: as hipóteses da forense de 01/09 (stdin não-bloqueante, circuit breaker, fallback standalone) estavam a jusante de um processo que nunca nascia; o fallback standalone morto (F0.3c) era real, mas não era a causa do vivo.
- Fix proposto (settings.json → human gate, não aplicado): remover o `if` do `post-bash`; decidir o do `pre-bash`. Refinamento F9: `post_bash_delivery_ratio` = 5.0 porque o numerador conta entregas ao mirror de qualquer origem (CLI `index find` etc.) — separar por origem.
- Memórias: `f0.3:causa-raiz-fechada:2026-09-02` (supersedes `f0.3:post-bash-entrega-viva:2026-09-01`).
