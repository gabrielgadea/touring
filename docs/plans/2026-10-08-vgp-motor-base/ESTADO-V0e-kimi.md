# ESTADO-V0e — consertos dos gates (executor: analise-8614d1f9, Kimi)

> **09/10/2026 ~03:15 UTC — TODOS OS ITENS COMPLETOS E VERDES. Falta SÓ: o
> relatório final à 6b (pronto abaixo) e o deploy do próximo patch (sinal DELA, como no A3).**
> Escrito às pressas porque o Gabriel desligou a máquina — retomar é ler este
> arquivo e mandar o relatório.

## Verificação final consolidada (rodada única, 03:15 UTC)

- `cargo test -p touring-ceg -p touring-hooks-core -p touring-hooks-shared` → rc 0 (**1620 passed / 0 failed**)
- `cargo test -p touring-cli --lib` → rc 0 (**647 passed / 0 failed**, 2 ignored pré-existentes)
- `python3 -m pytest scripts/test_jev_client.py -q` → rc 0 (**10 passed**)
- clippy `-D warnings` --all-targets: rc 0 em touring-ceg · touring-cli · touring-hooks-core · touring-hooks-shared (rodados por par durante cada item)

## O que cada item mudou (arquivo:linha de referência, o arquivo andou com os edits)

### V0e6 — git no sandbox morria com "fatal: ~/.config/git/ignore" (PRIMEIRO, achado da 6b)
- **Causa**: o G-C (30/08) apontou GIT_CONFIG_GLOBAL/SYSTEM a /dev/null, mas o
  `core.excludesFile` padrão (`~/.config/git/ignore`) é derivado FORA da config.
- **Conserto**: `crates/touring-ceg/src/gateway/sandbox_executor.rs` —
  `sandbox_read_roots()` ganhou `.config/git/ignore` (SÓ o arquivo: `config` ao
  lado pode ter credencial — a fronteira do `~/.config` × `gh/hosts.yml`).
- **Testes** (no mod tests do mesmo arquivo): `the_git_global_ignore_file_is_a_read_root_but_the_dir_stays_out`
  + `e2e_git_status_in_the_sandbox_matches_native_without_fatal` (e2e real:
  landlock + whitelist + git de verdade num repo tmp; RED reproduziu o fatal).
- RED 2 failed → GREEN 603 passed → mutação (sem a linha) 2 failed → restaurado.
- **Aceite e2e real medido**: `target/debug/touring run --lang bash --code
  'cd ~/projects/touring && git status --short | head -1'` → stdout
  `" M CHANGELOG.md\n"` IDÊNTICO ao nativo, stderr vazio (o binário debug, sem
  tocar o target/release do daemon).

### V0e1 — G1 EXECUTA a rota via updatedInput (o maior)
- **`crates/touring-cli/src/cli_suggester.rs`**:
  - `GateOverrides` ganhou `g1_rewrite_disabled` (kill switch
    `TOURING_G1_REWRITE_DISABLED=1`, lido no `from_env`, padrão do G2 do H1).
  - `burst_gate_with`: após o autodemovido e antes do deny clássico, o REWRITE
    (default): `permissionDecision: "allow"` + `updatedInput.command =
    touring run --lang bash --code '<a rajada fundida>'` (o programa já era
    montado pelo `fuse_burst_program` com escape de aspa). O `Denied` +
    `pending_g1` DESCERAM para o caminho do deny clássico (o rewrite não é
    turno cego, não entra no continuation-check). O fallback estrutural
    (`unreachable_in_sandbox` → None → a original roda nativa) já existia e
    foi endurecido no teste (`is_none()`).
- **Testes**: novos `g1_rewrites_the_burst_into_one_sandbox_run` (allow +
  updatedInput + os 3 comandos reais fundidos) e
  `g1_rewrite_disabled_falls_back_to_the_classic_deny` (kill switch INJETADO +
  controle positivo). Atualizados para o mundo do rewrite:
  `burst_window_isolated_per_agent`, `g1_burst_window_is_per_session`,
  `code_mode_still_fuses_grep_burst_inside_project` (denied→rewritten), e no
  cli_suggester_tests.rs `g1_segunda_passa_terceira_nega_com_a_rajada_no_remedio`
  → `g1_segunda_passa_terceira_reescreve_com_a_rajada_no_programa` (mede o
  envelope novo) e `g1_continuation_check_fecha_o_ab` (o A/B do deny agora é
  exercitado com o kill switch injetado via `code_mode_gates_with`).
  O helper `denied` do s24_route_tests ficou órfão → REMOVIDO (REGRA #0).
- **Paridade (aceite obrigatório)**: `e2e_burst_program_parity_native_vs_sandbox`
  no sandbox_executor.rs (ceg) — grep/cat/ls/wc/sed -n com caminho RELATIVO ao
  cwd + git: stdout e rc idênticos nativo × sandbox, zero "fatal".
- RED (E0560 o campo não existia + asserts) → GREEN 645 → **mutação** (condição
  invertida `if ovr.g1_rewrite_disabled`) matou 6 testes g1_ + 7 burst →
  restaurado 647 verde, clippy rc 0.
- **NÃO MEDIDO em produção**: a prova do rewrite no hook instalado exige o
  daemon com o código novo (o burst ledger vive no daemon) — fica para o
  deploy do próximo patch (o C1 da 6b mede com a amostra pós-deploy).

### V0e2 — guards sem rota NOMEIAM a alternativa
- **`crates/touring-hooks-shared/src/bash_ast_validator.rs`**: consts novas
  `DD_ROUTE` ("file-to-file copy is `cp`, sizing is `truncate` — raw `dd` is
  the user's to run") e `PIPE_SHELL_ROUTE` ("download, read, then run:
  `curl -O <url>` …, read the file, `bash <file>`").
- **`crates/touring-hooks-core/src/hooks/pre_tool_validator.rs`**: função nova
  `route_for_reason` (casa pela razão-const estável: dd → DD_ROUTE,
  pipe→shell → PIPE_SHELL_ROUTE, `del ` → DELETE_ROUTE) + aplicada nos DOIS
  call sites do deny (o fast path de prefixo E o caminho de pipeline, que
  negava com a razão CRUA — nunca teve rota, nem a de delete).
- O `--force` do brief = o do `rm` (o matcher genérico) — coberto pelo V0e4.
- Teste novo `route_less_guards_now_name_their_route` (dd/curl/wget/del).
- RED 1 failed → GREEN → mutação (dd sem rota) morta → restaurado.

### V0e4 — guard de rm aceita `: >` e `unlink`
- DELETE_ROUTE (shared) amplificado: "+ empty a file in place with `: > <file>`
  or remove a single entry with `unlink <file>`"; DELETE_ROUTE_EXAMPLES ganhou
  `: > stale.log` e `unlink stale.done` — o teste existente
  `every_delete_refusal_names_an_open_route` prova os dois abertos nos DOIS
  validadores.
- Teste novo `the_rm_route_names_in_place_emptying_and_unlink`. RED → GREEN →
  mutação morta → restaurado.

### V0e3 (parte Rust) — G7 com o R1 real + o reset consertado
- **O reset NUNCA funcionava**: `trim_matches` cobria só `'\"[]{},` — um path
  citado DENTRO de código (`pathlib.Path("/…")`) não casava a chave. REPRODUZIDO
  AO VIVO por mim: a 4ª leitura do cli_suggester.rs foi negada após um
  `touring run` exit 0 citando o arquivo (igual ao run-1791491632099 da 6b).
- **Conserto**: `path_fragments()` nova (quebra por todo char não-path, fica
  com fragmentos com `/`) substitui o trim no reset do G7.
- **A mensagem**: `r1_skeleton_path()` + `r1_route_for(fp)` novas — o caminho
  real `~/projects/touring/docs/plans/2026-08-24-code-mode-antecipa/evidence/
  repertorio/r1_varredura_agregado.py` (existe: 794 B) quando presente, senão a
  query de memória — NUNCA placeholder. Espelha o `_r1_route()` dela.
- Testes novos: `g7_reset_funciona_com_o_path_citado_dentro_de_codigo` +
  `g7_mensagem_nomeia_o_r1_real_ou_a_query_nunca_placeholder` (exige o caminho
  real QUANDO o arquivo existe na máquina — endurecido depois do GREEN).
- RED 2 failed → GREEN 6/6 G7 → mutação dupla (trim fraco + skeleton None)
  matou 3 (os 2 novos + 1 existente que prende a forma) → restaurado.

### Dívida — jev_client.py:264 AttributeError cru (V0c c2e3da31104b)
- **`scripts/jev_client.py`**: `sorted(questions.keys())` explodia quando
  `questions` não era Mapping. Conserto: validação `hasattr(questions, "keys")`
  ANTES do uso, com `fail("spec_error:questions precisa ser um Mapping com
  .keys() — recebido list, sem o atributo 'keys'")` — erro estruturado que
  nomeia o atributo ausente, no padrão do módulo, segue fail-open (journal+None).
  O `base` foi reordenado (ts/model primeiro; question_ids depois da validação).
  system_one CC 18→19 (+1 sobre violação pré-existente — registrado, não
  introduzido; a refatoração dele é fora do brief).
- Teste novo em `scripts/test_jev_client.py`:
  `test_questions_nao_mapping_devolve_none_com_erro_nomeado` (com FakeClient —
  zero rede). RED com o AttributeError cru na linha 264 exata → GREEN 10
  passed, ruff limpo → mutação (sem a validação) morta → restaurado.

## REGRA #21 aplicada fora do brief
- `ast_bridge.rs` o teste `the_workspace_claude_md_is_not_a_consumer` quebrou
  porque a premissa vivia num exemplo do CLAUDE.md vivo — que a reestruturação
  documental ordenada (o changelog ao CHANGELOG.md:390) removeu. Reescrito como
  `markdown_with_cited_code_is_not_a_consumer` com fixture inline (a premissa
  frágil era o defeito, não o extrator). Verde.

## O que falta (retomada)
1. **Mandar o relatório final à 6b** (uma mensagem: por item RED/GREEN/mutação
   com rc + arquivo:linha — o conteúdo é este arquivo resumido).
2. **Deploy do próximo patch**: SÓ com sinal dela (a ordem é a do A3: gates → bump à mão
   → dry-run → sinal → stop-before-propagate → propagação). NUNCA
   update-touring/propagate sem sinal.
3. Cargo.lock: o build regravou? Verificar `git status` no repo touring —
   os crates tocados declararam 30.4.93 durante os builds (o version vem do
   workspace). Nada commitado — só o Gabriel ordena.
