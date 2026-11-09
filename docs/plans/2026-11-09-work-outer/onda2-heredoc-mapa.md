# Onda 2 — leitores de heredoc fora do lexer (mapa, 09/11/2026)

DAG `task_1794206289311361997`, subtask `onda2`. Onda 1 (programa efetivo único) em
`7cf12bad` + `2b7c020c`. Fonte: levantamento só-leitura (subagente sonnet), linhas
conferidas na leitura e não executadas — cada RED abaixo é hipótese até rodar.
`touring-ceg`, `touring-cli` e `touring-hook-handlers` já dependem de `touring-hooks-shared`.

## API nova no lexer (uma, consumida por H3/H5/H7)

`heredocs(cmd) -> Vec<Heredoc { owner: Vec<String>, body: Range<usize>, quoted, closed, data }>`
com offsets de byte do texto original (`data` = veredito do H1: dado provado). Pede um
cursor com offset no `ShellLexer` (hoje só `Chars`). Junto: ler redireção de fd
(`2>&1`, `&>`), pois `&` hoje fecha o pipeline (`feed`) — elimina a lista do H3, o
`is_stage_boundary` do H6 e o `strip_fd_redirections` do H7.

## Sítios, por impacto de segurança

| # | Sítio | Uso | Divergência (entrada) | Migração |
|---|---|---|---|---|
| H7 | `touring-ceg/src/gateway/gate.rs:1651` `mask_shell_data` (consumidores gate.rs:279, mode.rs:178, shell.rs:454, post_bash.rs:513) | X6 detecta escrita (`bash_writes_a_file`) — **brecha real**: escrita mascarada vira passe | `cat <<< x⏎echo a > f`; `x=$((1<<2))⏎echo a > f`; `bash <<'EOF'⏎echo a > f⏎EOF`; `cat <<EOF⏎$(echo a > f)⏎EOF`; delimitador `<<'E-F!'`/`<<"A B"` | `heredocs()` e branquear só os spans `data` (comprimento preservado) |
| H4 | `touring-hook-handlers/src/hooks/vgp_execucao.rs:437` `sem_corpos_de_heredoc` | sombra VGP: que script python roda | `echo "<<EOF"⏎python3 a.py`; `cat <<EOF; echo⏎x⏎EOF⏎python3 a.py`; `bash <<EOF⏎python3 a.py⏎EOF` | apagar; `shell_pipelines(command)` já descarta corpos de dado. A linha :668 do teste fixa o errado |
| H3 | `touring-cli/src/code_mode_gates/shell.rs:100` `mutation_scan_view`, :274 `python_inline_body`; desistências `contains("<<")` em chain.rs:591 (G2), :638 (G9), shell.rs:633 | gates do code mode | `python3 -c 'print(1 << 3)'` → `None`; `sed -i s/a/b/ f; grep x <<< y` → G9 desiste | `heredocs()` p/ corpo e dono; G2/G9 sobre o texto sem corpos `data` |
| H5 | `touring-hook-handlers/src/hooks/pre_bash.rs:883` `heredoc_python` | sombra VGP python | 1º `<<` só; falso positivo `echo python3 && cat <<'E'>s`; `<<-`; `find("\nTAG")` por prefixo | `heredocs()` com `owner` python |
| H6 | `touring-hook-handlers/src/hooks/result_compactor.rs:331` `last_stage` | compactação (baixo) | `cat <<EOF⏎x⏎EOF⏎cargo test` avalia `cat`; `x=$((1<<2)); git log` | última pipeline de `shell_pipelines` + fd-redirect |

## Testes que fixam o comportamento atual

- H7: `s24_shell_data_tests` (gate.rs:1809).
- H4: `so_o_comando_simples_conta_nunca_o_texto_da_linha`, `heredoc_sem_corpo_e_herestring_e_marcador_entre_aspas` (:662).
- H3: unit.rs `s5_*` (:161, :172, :210), `g8_ignora_heredoc…` (:458), `cat_com_redirect_e_escrita_nao_inspecao` (:579); rows.rs `g2_g9_rows`, `s24_rows`, `g10_route_rows`.
- H5: `extrai_heredoc`, `heredoc_de_shell_que_cita_python_nao_e_programa_python`, `extrai_python_c`.
- H6: `test_last_stage_respects_quotes_redirections_and_heredocs` (:501 fixa `"cat "`), `test_payload_commands_are_left_alone`.

## Ordem

RED de H7 primeiro (brecha de escrita no CEG), depois H4 (só apagar), H3, H5, H6. Não
editar a working tree enquanto um `propagate-release.sh` estiver rodando — o build sai dela.
