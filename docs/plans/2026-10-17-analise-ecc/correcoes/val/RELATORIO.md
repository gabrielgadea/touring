# VAL: corpo de heredoc deixa de ser lido como comandos (PreToolValidator)

Data: 18/10/2026. Worktree `/home/gabrielgadea/projects/touring-wt-ecc-val` (branch `wip/2026-10-17-ecc-val`, de `63549a0d`).
Arquivo único alterado: `crates/touring-hooks-core/src/hooks/pre_tool_validator.rs` (+577 / -9: implementação 960 -> 1192 linhas, testes 666 -> 1002).
Patch: `val.patch` (sha256 `8a6e5b31e3dec1abd418be20bbeb9fbb75401f47fe6fb58a6d4b2f047c40b5ca`, em `val.patch.sha256`). `git apply --check -R` no worktree confere.

## 1. Causa

`PreToolValidator::validate_command` tokeniza com `shell_pipelines` -> `ShellLexer`, um lexer de caracteres que conhece aspas, escapes, comentário e separadores, mas **não conhece heredoc**. Em `python3 - <<'EOF'` + `del L[0:1]` + `EOF`, a quebra de linha encerra o pipeline e cada linha do corpo vira «comando»; `del L[0:1]` casa o prefixo estático `del ` («Windows delete command — verify target»). Mesma mecânica para `dd = json.load(...)` (prefixo `dd `) e para `rm -f` dentro de um script só gravado por heredoc.

Dois efeitos que o relato original não tinha:

- **Buraco, não só falso positivo.** Um apóstrofo no corpo (`don't`) abre uma aspa simples que o lexer pareia com outra bem adiante e **engole o texto depois do heredoc**. Medido no corpus real: 2 comandos com um `rm -f` verdadeiro depois do `EOF` passavam calados (seção 5.2).
- **O defeito me negou durante o trabalho.** O validador instalado recusou um comando meu cujo heredoc de `python3 -` continha, como texto, `bash <<B` e `rm -rf /` («Recursive delete in bash»), até eu passar a editar por Edit/Write.

## 2. Desenho

Predicado reaproveitado? **Parcialmente, por referência, não por chamada.**

- `touring_ceg::gateway::gate::mask_shell_data` (`gate.rs:1651`) é alcançável (`touring-hooks-core` depende de `touring-ceg`; `cli_suggester.rs` e `post_bash.rs` o usam), mas **apaga todo corpo de heredoc sem olhar o destino**: para `bash <<EOF` o corpo é código e tem de continuar julgado. Também não distingue delimitador com aspas de sem aspas (corpo sem aspas é expandido: `$(…)` roda) e devolve texto mascarado de mesmo comprimento, não as palavras de que o validador precisa. Usá-lo reintroduziria exatamente o buraco que o caso `bash` testa.
- O julgamento do destino só existe em Python (`client/hooks/_shell_mask.py`: `_SHELLS`, `_shell_invocation`, `_stdin_code`). O desenho abaixo é a tradução mínima dessa regra para o lexer existente, sem um terceiro mascarador paralelo.

O `ShellLexer` passa a ler `<<[-]PALAVRA` e, na quebra de linha, lê os corpos pendentes até a linha-delimitadora. Só conta como operador o `<<` que **abre palavra** (`1<<2` e `$((1 << 3))` seguem como estavam). Um corpo é **dado** (sai do resultado) a menos que o shell pudesse executá-lo. O que não se prova dado **continua sendo comandos**, então a mudança só remove recusas por corpo:

| o corpo continua comandos quando | porquê |
|---|---|
| não há linha-delimitadora | o shell lê até o fim do texto |
| delimitador sem aspas e o corpo tem `$(` ou crase | corpo sem aspas é expandido: a substituição roda |
| algum comando do texto é shell ou entrega a um (`bash sh zsh dash ksh ash fish mksh rbash csh tcsh eval source . ssh su sudo doas at batch`, em **qualquer** palavra: `docker exec -i c bash`, `y=$(bash`, `/bin/sh`) | `cat <<EOF \| bash`, `bash <<EOF`, `cat > run.sh <<EOF` + `bash run.sh` |
| algum comando do texto roda um arquivo que outro grava (`> f`, `>> f`, `tee f` e depois `./f`, `timeout 60 f`) | script gravado e executado no mesmo comando (2 dos 25 casos reais) |

Corpos mantidos como comandos são lexados **uma vez, achatados** (`lex(corpo, false)`, sem heredocs): sem recursão, sem custo quadrático em `<<A` sem fim. Here-string (`<<<`) não muda: o leitor de delimitador para no terceiro `<` e `<<` volta a ser palavra. A API pública não mudou (12 símbolos `pub`, `touring ast meta`); só itens privados novos. Complexidade: `feed` ficou em CC 15 (o teto do aviso) movendo o braço de escape e os dois braços de heredoc para métodos.

## 3. Vermelho / verde

TESTE PRIMEIRO: os testes `heredoc_*` rodaram contra a **implementação original** + testes novos (`make_red.py`), depois contra o arquivo corrigido.

| | original | corrigido |
|---|---|---|
| testes executáveis (`cargo test -p touring-hooks-core --lib heredoc_`) | **5 falham de 7** (os 2 que passam são de caracterização por desenho: «shell continua julgado como código» e «`bash <<<` continua negado») | **10 passam de 10** |
| os 3 testes que citam símbolo novo (`may_run_text`/`TEXT_RUNNERS`, `written_paths`, `read_delimiter`) | não compilam contra o original (sem como ficar vermelhos) | passam |
| **casos** (as mesmas listas dos testes, replay em duas builds) | **68 erram de 154** | **0 erram de 154** |

Os 68 do original, por grupo: dado para programa permitido (`python3 -`, `cat`, `psql`, `node`, `cat > f`, `cd && …`, `FOO=1 …`, `timeout …`; corpos `del`, `dd`, `rm -rf /`, `mkfs`) **48 de 48**; formas de delimitador e dois heredocs **18 de 19**; apóstrofo no corpo escondendo o `rm -rf /` seguinte **1 de 9**; `'$(date)'` com aspas é dado **1 de 19**. Zero erros do original nos casos que já eram negados (shell, `bash <<<`, comando depois do corpo): os 56 casos «continua negado» passam nas duas builds, que é o que se queria provar.

Casos cobertos (todos nos testes): `del`, `dd if=`, `rm -rf /`, `mkfs` no corpo para `python3 -`, `cat`, `psql`, `node` -> permitido; os mesmos para `bash`, `sh -s`, `bash -s --`, `zsh`, `sudo bash`, `env … bash`, `docker exec -i c bash`, `ssh`, `source /dev/stdin`, `. /dev/stdin`, `cat <<EOF | bash`, `y=$(bash <<EOF …)` -> negado; `<<` com e sem aspas, `<<\EOF`, `<< 'EOF'`, `<<-` com tabs (e a linha com tab sem traço, que é corpo), dois heredocs, delimitador chamado `sh`, comentário depois do operador, comando perigoso depois do corpo (mesma linha e linha seguinte) -> negado.

## 4. Mutantes

Controle sem mutação primeiro: verde, 10 de 10 testes (o harness `val_mutants.py` só segue se o controle passa; mutante que não compila é INVÁLIDO, não morto).

**65 mutantes, 65 mortos, 0 sobreviventes** (rodada final, mesmo arquivo do patch). Por ramo novo:

| ramo | mutantes (mortos) |
|---|---|
| detectar heredoc (guarda de início de palavra, 2º `<`, newline lê corpos, fechar linha antes, `<<-`, brancos após `<<`, delimitador vazio, palavra de fallback, comentário lê corpos, topo lê heredocs, corpo mantido lexado sem heredocs = M06) | M01-M11, M44, M45 (13) |
| re-escanear o que não é dado (sem delimitador, aspas do delimitador, `$(`, crase, corpo-código largado, corpo-dado largado) | M12-M17 (6) |
| decidir se roda texto (`runs_text` ignorado / sempre / só o último comando, `.`, cada separador de palavra, 4 nomes da lista) | M18, M18b, M19-M21, M41, M42a-e, M43a-d (15) |
| arquivo gravado e executado (`>>` x `>`, alvo separado, alvo colado, `tee`, opções do `tee`, `./x` x `x`, nunca conta, wrappers, nunca grava) | W1-W9 (9) |
| leitor de delimitador e corpo (7 caracteres de parada, aspas, escapes dentro de `"…"`, tabs, comparação exata, fim do texto, junção de linhas) | M22a-f, M29-M40, M48 (22) |

A primeira rodada achou 4 sobreviventes (M30, M31, M32, M33c: aspa simples, barra, aspa dupla de fechamento e `\"` dentro de aspas duplas), todos equivalentes só para os casos felizes. Viraram o teste de unidade `heredoc_delimiter_words`, que chama `read_delimiter` direto, e a rodada final os mata. Não incluí o mutante que troca o `return` de fim de texto por `continue`: travaria em vez de falhar.

## 5. Replay contra comandos reais

Corpus: **81.524 comandos Bash únicos** extraídos de `~/.claude/projects/*/*.jsonl` e `*/*/subagents/*.jsonl` (somente leitura), dos quais **14.040 contêm `<<`**. É um superconjunto dos ~4.000 pedidos; o replay é em processo (teste `#[ignore]` `replay_recorded_commands`, `PRE_TOOL_REPLAY_IN/OUT`) e rodou nas duas builds.

| | antigo | novo |
|---|---|---|
| negações (`validate_command`) | 283 | 262 |
| **novas negações** | | **2** |
| **negações removidas** | | **23** |
| razão alterada, ainda negado | | 0 |

### 5.1 As 23 removidas: todas falso positivo da classe do defeito

| quantas | o que o corpo era | destino do heredoc |
|---|---|---|
| 15 | instrução Python `del L[…]`, `del os.environ[…]`, `del sys.modules[m]`, `del u["…"]; n += 1` | `python3 -`, `.venv/bin/python3 -`, `cat > test_*.py`, `cat >> test_cfg_driver.py` |
| 5 | variável Python chamada `dd` (`dd = json.load(…)`, `dd = d["dados"]`) | `python3 -`, `cat > probe2.py` |
| 3 | `rm -f "$out" "$err"`, `rm -f $f-*.png`, `rm -f /dev/null` dentro de **script só gravado** (`cat > shim/…`, `run.sh`, `lote4.sh`) seguido de `chmod +x`/`ls`, sem executá-lo | `cat > arquivo` |

As 3 últimas são decisão de política, não de sintaxe: gravar um script não é rodá-lo, e o arquivo pode ser rodado numa chamada Bash posterior (ver 6). As outras 20 são Python puro. **Nenhuma das 23 é recusada pela camada estrutural** (`bash_ast_validator`, que `pre_bash` roda antes): o resultado visível ao usuário muda nas 23.

Duas negações do replay inicial (os scripts gravados e **executados no mesmo comando**: `cat > $D/skill_ab.sh <<EOF … rm -rf …` + `timeout 600 $D/skill_ab.sh`) voltaram a ser negadas depois da regra do «arquivo gravado e executado». Foi o replay que as achou: o desenho inicial as liberava.

### 5.2 As 2 novas negações: verdadeiros positivos antes escondidos

Contrariando a meta «devem ser 0», aparecem 2, e as duas são o buraco da seção 1, não regressão:

- um `cat >> $F <<'RUST' …` com corpo Rust cheio de aspas, seguido de `…; rm -f target/audit-2026-09-14/r2/w3b.status; systemd-run …`;
- um `python3 - <<'EOF' …` com apóstrofos escapados (`<<\'MD\'`) no corpo, seguido de `rm -f /dev/null; cd … && rustfmt …`.

Nas duas há um `rm -f` verdadeiro, fora de heredoc, que a regra de `-f` do próprio validador nega em qualquer outro lugar. O lexer antigo pareava um apóstrofo do corpo com uma aspa adiante e o ocultava. Ler o limite do corpo certo é o que o expõe; **não há como remover os falsos positivos e preservar esse esconderijo acidental**. Se o Gabriel preferir tratar como regressão, o teste `heredoc_a_command_after_the_body_is_still_judged` (caso `don't`) é o que a define.

## 6. Gates

| gate | rc | resultado |
|---|---|---|
| `cargo fmt -p touring-hooks-core -- --check` | 0 | limpo |
| `cargo clippy -p touring-hooks-core --all-targets -- -D warnings` | 0 | limpo |
| `cargo test -p touring-hooks-core --no-fail-fast` | 0 | 540 passam, 0 falham, 1 ignorado (o replay); doctests: 8 ignorados |
| `cargo test -p touring-hook-handlers --features pre-hooks,post-hooks --no-fail-fast` | **0 só no overlay** | 739 passam, 0 falham. **Não compila num worktree limpo do HEAD** (abaixo) |
| `cargo fmt --all -- --check` | **1 no HEAD** | informativo, ver abaixo |

**HEAD `63549a0d` não é autocontido (falha pré-existente, não tocada por este patch, REGRA #21):**

1. `touring-hook-handlers/src/lib.rs` declara `pub mod vgp_l2;`, mas `hooks/vgp_l2.rs` é **não versionado** no main (`??`, 09/10). Resultado num worktree limpo: `E0583` no `cargo test` e `rustfmt` não resolve o módulo. Mesmo com o arquivo, faltam `touring_code::vgp` (`crates/touring-code/src/vgp/`, não versionado) e a crate `touring-pyright` (não versionada; está em `Cargo.toml` do main): `E0432`/`E0433`.
2. Para rodar o 4º gate de verdade copiei temporariamente para o worktree o trabalho não versionado do main sob `crates/` + `Cargo.toml`/`Cargo.lock` (**17 arquivos substituídos + 17 adicionados**, main HEAD `63549a0d`), exceto o arquivo deste patch, com backup e restauração byte a byte (`val_overlay.py`). Depois: `git status` do worktree = só `pre_tool_validator.rs`. O resultado (739/0) vale para «main com WIP + este patch».
3. `cargo fmt --all -- --check` no HEAD também acusa `crates/touring-server/build.rs:100` (linha em branco final) e `crates/touring-server/tests/build_info_freshness.rs:53` (falta uma). Não são do patch e ficaram como estão (fora do escopo de arquivo pedido); são duas correções de uma linha, e o Gabriel decide se entram.

## 7. Residuais (medidos quando possível)

1. **A camada estrutural (`bash_ast_validator`, crate `touring-hooks-shared`) lê o corpo do heredoc do mesmo jeito e não foi tocada.** Dos 48 casos «dado para programa», ela ainda recusa os 12 com `rm -rf /` no corpo; ponta a ponta (`pre_bash`) 36 de 48 passam. Os `del`, `dd` e `mkfs` passam nas duas camadas. Mesma classe de defeito, outra crate: fica como follow-up.
2. `sh <<< "rm -rf /"` já era **permitido** no original (só `bash` tem esquema no validador); o patch não muda e o teste não afirma o contrário.
3. Gravar o script numa chamada Bash e rodá-lo em outra não é visto. Antes isso era barrado por acidente (corpo lido como comando, 3 dos 23); agora o corpo é dado. Só o gravar-e-rodar **no mesmo comando** é preservado.
4. Programa dentro do corpo que dispara shell (`os.system("rm -rf /")` num `python3 -`) não é visto, como já não era: as aspas colavam o texto numa palavra.
5. Operador colado a uma palavra (`cat<<EOF`, `3<<EOF`) não é lido como heredoc e o corpo segue como comandos (o lado conservador); linha-delimitadora com `\r` idem.
6. Corpos mantidos como comandos são lexados com as regras de aspas de antes: um corpo-código com aspa desbalanceada continua a engolir o que vem depois dele, dentro do próprio corpo.

## 8. Reprodução

Scripts em `/tmp/claude-1000/-home-gabrielgadea-projects-touring/efe418c2-95f7-4a62-8bac-57080ed455b8/scratchpad/` (sessão): `val_all.sh` (red/verde + replay, gates, mutantes, gate 4 no overlay, numa só reserva `hw-slot`), `val_mutants.py`, `val_replay_diff.py`, `val_cases.py`. O corpus tem comandos de transcritos privados e **não** foi copiado para o repositório.
Replay manual: `PRE_TOOL_REPLAY_IN=<jsonl de strings> PRE_TOOL_REPLAY_OUT=<saída> cargo test -p touring-hooks-core --lib replay_recorded_commands -- --ignored`.
