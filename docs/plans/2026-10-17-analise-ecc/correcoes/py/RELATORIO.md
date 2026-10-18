# Correção dos hooks Python (ecc-py) — relatório do engenheiro

Worktree: `/home/gabrielgadea/projects/touring-wt-ecc-py` (HEAD solto `21cd6f19`). Patch: `py.patch` (sha256 em `py.patch.sha256`:
`c27398e7e2ea32dbec01bb67f19e2e5062ae646ccc95f882050bef988a5ea5ff`; inclui o item D2). O patch aplica limpo num `git archive HEAD` (verificado: `git apply --check`
e os 3 arquivos centrais ficam idênticos ao worktree) e os 3 arquivos de teste do guard passam nele (**641 passed**: hooks_bypass 439 + nested 181 + no_jq 21).
Nada foi escrito em `~/.claude/hooks/*`, em `client/` do main nem em outro worktree; sem commit, push, stash ou kill.

## 1. Resultado por item

| Item | Feito | Testes novos (vermelho no original) | Mutantes mortos |
|---|---|---|---|
| C1 guard aninhado | sim | `test_s25_block_git_nested.py` (181 casos; **107 vermelhos** no original) | 49/49 comportamentais (+1 equivalente, ver §2) |
| 2 falha muda sem `jq` | sim | `test_s25_block_git_no_jq.py` (21 casos; **19 vermelhos**) | no mesmo lote do C1 |
| 3 U+FEFF literal | sim | `test_hook_sources_visible.py` (2 casos; 1 vermelho) | 3/3 |
| 4 `CLAUDE_CODE_SUBAGENT_MODEL` visível | sim | 4 novos + 2 ajustados em `test_agent_model_gate.py`; 4 novos em `test_model_routing_adherence.py` (9 vermelhos de 16 selecionados) | 7/7 |
| 5 sensor mudo do `check_context.sh` | sim | `test_s42_context_sensor.py` (18 casos; **18 vermelhos**) | 12/12 |
| **D2** contorno de hooks do git | sim (ver §10) | `test_s25_block_git_hooks_bypass.py` (439 casos; **132 vermelhos** no original) | **121/121** na rodada final (122 na 1ª: 112 mortos + 10 testes fracos corrigidos) |
| 6 `test_skill_size_guard.py` sem chamador | **não era coletado**; ligado no CI e no release | 2 testes novos no próprio guard (+ `test_nada_acima_do_limite` endurecido) | 5/5 |

**Mutação**: cada mutante é uma troca textual única em CÓPIA da árvore (`assert count == 1` antes e `assert` de que a troca aconteceu; o harness reverte), rodando o arquivo de teste fixado. A primeira rodada achou **4 testes fracos** (mutante sobrevivente), todos corrigidos e re-mortos: (a) per-literal de `exec_scan` (o `join` já expunha o comando; agora há um caso cujo literal aninha um shell), (b) `printf | grep -q` restaurado (o padding do teste era comentário, que o novo mascarador apaga, então nunca chegava ao buffer de pipe; agora são linhas de código), (c) `positional means stdin` (faltavam controles `bash script.sh <<EOF` e `python3 script.py <<EOF` = dado do script), (d) guard do CI (um `skip` escapa de `pytest.raises(Failed)` e conta como PULADO, não como falha).
Único sobrevivente da rodada final: **«masker rodado como script outra vez (sem cache de import)»** — equivalente por construção (mesmo veredito; só custa ~15 ms), não é matável por teste funcional.

## 2. C1 — o guard da REGRA #11 diante das formas aninhadas

**Causa**: `_shell_mask.py` apagava todo literal entre aspas e todo corpo de heredoc, qualquer que fosse o destino; `block_git.sh` só casava `git <sub>` adjacente. Sonda reproduzida contra o original: 34 de 36 formas passavam (`evidencia/probe_forms.py`; contra o código novo, 0 problemas).

**Desenho** (`client/hooks/_shell_mask.py`, reescrito; `block_git.sh` o consome):
- Um **lexer** (aspas, `$'..'`, `$(..)`, crases, `<(..)`, heredoc `<<[-]`, here-string, redirecionamentos, comentários, `((..))`) substitui as três regex e o parser de heredoc por linha. Produz **vistas**: (a) o comando com os DADOS apagados (como antes); (b) cada texto entregue a algo que EXECUTA, recursivamente.
- Código re-escaneado: `bash|sh|zsh|dash|ksh|ash|fish|… -c` (qualquer cluster: `-lc`, `-ec`, `-o pipefail -c`, `--login -c`), `eval`, heredoc / here-string / `echo|printf|cat <<| bash` que alimenta shell, `$(..)`, crases, `<(..)`, substituições de heredoc **de delimitador sem aspas**, `find -exec/-execdir/-ok`, `su|flock|script -c`, `watch`, `touring exec|run --code`, `env -S`, `sudo -s`, e o comando que um lançador roda: `env`, `timeout N`, `nice`, `nohup`, `command`, `exec`, `sudo`, `doas`, `time`, `setsid`, `ionice`, `stdbuf`, `busybox`, `hw-slot … --`, `xargs`.
- `python3 -c`/`-Ic`, `perl -e`, `ruby -e`, `node -e/-p`, `php -r`, `awk`, e programa em stdin (heredoc, here-string, `echo | python3`): **negado** quando um argumento de chamada de execução (`os.system`, `subprocess.*`, `system`, `exec*`, `spawn*`, `execSync`, `shell_exec`, `Popen`; crases/`qx` em perl/ruby) contém git destrutivo. Corpo de heredoc para `python3` que só ESCREVE texto continua dado.
- Cada invocação `git` é **normalizada** (`git -C d reset '--hard'` → `git reset --hard`: opções globais caem, subcomando/flag entre aspas saem das aspas, qualquer outra palavra citada continua apagada).
- `xargs git <reset|clean|checkout|switch|restore|stash|rebase|filter-*>` (o stdin completa os argumentos, invisíveis) e `xargs git` sem subcomando viram NOTA → DENY nomeado. Exceções seguras: `xargs git stash list|show`, `xargs git restore --staged`.
- **Orçamento**: profundidade máxima 8; trabalho `max(8192, 16 × tamanho da entrada)`. Estouro vira DENY com o motivo (`profundidade > 8` / `trabalho N > orçamento M`) e a rota (`GIT_DESTRUCTIVE_OK=1` neste comando). Provado com 11 níveis de `bash -c` (354 KB) e 4.000 `system(` aninhados (fim em < 5 s).
- Token `GIT_DESTRUCTIVE_OK=1`: vale se for **código** em qualquer nível (externo, interno, dentro de `os.system('…')`); nunca como dado (aspas, corpo de heredoc, `echo '…'`, nem num `bash -c "echo 'GIT_DESTRUCTIVE_OK=1'; git reset --hard"`).
- Controles negativos mantidos (todos testados): `git status` aninhado, `echo "git reset --hard"`, `cat <<'EOF' > notas.md` citando o comando, `grep -n "reset --hard"`, `git commit -m "… git reset --hard"` (inclusive `$(cat <<'EOF' … don't … EOF)`), `rg 'git stash'`, `python3 -c "print('git reset --hard')"`, `python3 script.py -c "os.system(…)"` (o `-c` é do script), `bash script.sh <<EOF` (o stdin é dado do script), `git -C d stash list`, `git -C d restore --staged f`, `xargs git add`.
- Suíte `test_s25_*` + `test_s46_block_all`: 55 originais verdes; 257 com os novos.

**Furos extras achados (fora da lista; mesma causa, mesmo conserto)** — todos passavam no original:
1. `git -C <dir> reset --hard`, `git -c k=v clean -fd`, `git --no-pager …` (o padrão exigia `git reset` adjacente).
2. Argumento do próprio git entre aspas: `git reset '--hard'`, `git clean "-fd"`, `git reset $'--hard'`, `'git' reset --hard`, `\git reset --hard`.
3. `cat <<EOF && git reset --hard`: o marcador do heredoc **apagava o resto da linha**; `cat <<< "x"` + nova linha com git destrutivo: a here-string era lida como heredoc e engolia as linhas seguintes; o mesmo com `echo "<<EOF"` e com `(( x = 1 << 2 ))`.
4. Apóstrofo dentro de aspas duplas (`git commit -m "don't" && git reset --hard HEAD~1 && echo 'ok'`) pareava com uma aspa simples distante e apagava o `git reset` do meio.
5. **`set -o pipefail` + `printf | grep -q`**: com entrada > 64 KB, o `grep -q` sai no primeiro casamento, o `printf` morre de SIGPIPE (141) e o pipeline lê «sem casamento». Original: `git reset --hard HEAD` + 198 KB de padding = **permitido**. Agora `has_re`/`has_str` (sem `-q`, sem here-string: o bash grava here-strings grandes em arquivo temporário e um TMPDIR somente-leitura abriria tudo).
6. Comentário deixou de ser lido como comando (`git status # nunca rode git reset --hard` passa a ser permitido; era falso positivo).

**Residual declarado** (guard textual, como o `fable_consent_guard.sh`): programa montado em tempo de execução (`bash -c "$CMD"`, `eval "$x"`, variável passada a `os.system(c)`), arquivo de script (`bash x.sh`, `python x.py`), `ssh host '…'`, `docker exec`, `source <(…)`, `curl | sh`, alias/função, `git -c alias.x='!git reset --hard' x`. A docstring de `_shell_mask.py` lista isto.

**Custo**: mediana do hook 47 → 62–68 ms (`git status`, `cargo test | tail`, commit com heredoc, `bash -c`; n = 25 cada). Eram +28 ms até o mascarador ser importado em vez de executado como script (o import usa `__pycache__`, ignorado pelo git).

## 3. Item 2 — falha muda sem `jq`

`block_git.sh` agora lê o comando por **jq → python3 → JSON bruto**. Sem jq (ou JSON ilegível) o stderr nomeia a causa (`block_git: jq ausente do PATH; …`); com python3 o guard segue **completo**; sem nenhum leitor, o texto bruto é julgado grosso: git + (reset|clean|stash|rebase|restore|filter-*|checkout --|push --force/-d/:|branch -D|--discard-changes) = **DENY** com motivo + rota; senão libera **com aviso**. `GIT_DESTRUCTIVE_OK=1` no bruto libera. Também avisa quando o mascarador some (`mascarador indisponível`). Controle: payload válido sem comando continua mudo (rc 0, sem stdout nem stderr).
Teste: PATH restrito a symlinks (`cat grep sed readlink dirname awk` + jq/python3 por cenário). `test_s25_block_git_no_jq.py` cobre 9 comandos destrutivos no fallback bruto, 4 inofensivos, o token, JSON ilegível e o mascarador ausente.

## 4. Item 3 — C11 U+FEFF

`agent_model_gate.py:126` (`FRONTMATTER`) e `tests/test_agent_model_gate.py:195` trocados **por script** (nunca Write) por `﻿` (a `re` interpreta dentro de raw string). Verificado por regex e pela categoria Unicode `Cf` que não sobrou caractere invisível em `client/hooks` (nem nos outros arquivos); a regex casa com e sem BOM. Guard permanente: `test_hook_sources_visible.py` varre `client/hooks/**/*.{py,sh}` por categoria `Cf` (BOM, zero-width, bidi como U+202E, soft hyphen).

## 5. Item 4 — `CLAUDE_CODE_SUBAGENT_MODEL` visível

Comportamento mantido (as irmãs Kimi/MiniMax dependem do env). O log do gate grava `why: "env:CLAUDE_CODE_SUBAGENT_MODEL=<v>"` (`WHY_ENV_PREFIX`; antes `env:<v>`). `scripts/model_routing_adherence.py`: `gate.decididas_por_env = {total, por_valor}` (aceita os dois formatos, então o histórico do log continua contando) e o resumo curto ganha `· decididas por env: N` (0 aparece como 0; ausência de registro continua `registro ausente`). Teste de paridade: o `why` que o hook EMITE (`decide(...)`) é o que o relatório RECONHECE (`PREFIXO_WHY_ENV`) — predicado escrito em dois lugares.

## 6. Item 5 — sensor mudo do `check_context.sh`

- `status_monitor.sh` grava o **heartbeat** `<sessão>.seen` = `<epoch> <pct>` em TODO render (armado ou não) e carimba a flag com uma 2ª linha `ts=<epoch>` (a linha 1 segue sendo a porcentagem; flags antigas de 1 linha continuam válidas).
- `check_context.sh` grava uma linha por decisão em `~/.claude/touring/logs/context_check_journal.jsonl` (mesma família do `skill_suggestion_journal.jsonl`): `{ts, session, said, silent_reason, pct, sensor_age_s[, detail]}`. `silent_reason`: **abaixo** (heartbeat fresco, sem flag), **sem_sensor** (nenhum heartbeat: headless/subagente), **obsoleto** (heartbeat mais velho que `CONTEXT_SENSOR_STALE_SECS`, 900 s; heartbeat ilegível também), mais três que o conjunto pedido não cobria e ficariam mal classificadas sem nome próprio: **entregue** (flag já entregue), **monitor_projeto**, **sem_sessao** (sem `session_id` ou jq ausente; `detail` diz qual). Linha de silêncio só quando a razão MUDA ou a cada `CONTEXT_JOURNAL_EVERY` s (900), não uma por chamada (estado em `<sessão>.silent`).
- O que é **injetado** não mudou, byte a byte (teste de igualdade exata com o texto de antes).

## 7. Item 6 — `test_skill_size_guard.py`

Evidência: o `.github/workflows/ci.yml` lista os testes de `scripts/` um a um (nenhum glob); o `propagate-release.sh` roda `test_update_touring.py`, `test_touring_quality_score.py` e o glob `test_propagate_release*.py` — **nenhum cobre o guard** (`git grep` achava só `docs/plans`). Ligado em **dois** lugares: CI (`python3 -m pytest scripts/test_skill_size_guard.py -q -rs`) e `propagate-release.sh` (com `SKILL_SIZE_GUARD_REQUIRE_ROOTS=1`, mais a linha do `--dry-run`).
**Achado ao ligar**: o guard varre `~/.claude/skills` e `/home/gabrielgadea/projects/analise/.claude/skills` (caminhos da máquina). No runner do CI as duas raízes não existem, `varrer()` pula as ausentes e `test_nada_acima_do_limite` **passava medindo vazio** (e o modo script saía 0 «OK»): demonstrado no original com `Path.is_dir → False` (rc 0). Agora, sem raízes o teste **PULA dizendo isso** (`-rs`); com `SKILL_SIZE_GUARD_REQUIRE_ROOTS=1` **reprova**; o modo script sai 2. No CI rodam os controles positivo/negativo/fronteira/isenção (a lógica); a varredura real é do release, na máquina que tem as skills. `chmod +x` no arquivo (tinha shebang sem o bit; era a única falha do ruff nele).

## 8. Comandos e resultados

| Comando | rc / resultado |
|---|---|
| `python3 -m pytest client/hooks/tests scripts/test_sync_client_skills.py scripts/test_model_routing_adherence.py scripts/test_skill_size_guard.py -q -p no:cacheprovider -W error::UserWarning -rs` sob `hw-slot --label ecc-py` (**após o D2**) | **rc 0 — 1216 passed** (132 s) |
| `ruff check --no-cache client/hooks scripts/model_routing_adherence.py` | rc 0 (`All checks passed!`) |
| `ruff check --no-cache scripts/test_skill_size_guard.py scripts/test_model_routing_adherence.py` | rc 0 |
| `shellcheck client/hooks/block_git.sh client/hooks/check_context.sh` | rc 0 |
| `shellcheck client/hooks/status_monitor.sh` | rc 1 — 3 achados (SC2059 l.52, SC2034 `WIN_SRC` l.87 e `USAGE_ABS` l.138) **idênticos no HEAD** (`git archive`); nenhum é meu — ver §9.6 |
| `probe_forms.py client/hooks` (formas do C1, depois do D2) | `problems: 0` |
| `python3 -m pytest scripts/test_propagate_release*.py -q` (a linha nova do release) | 75 passed, 2 skipped |
| `python3 -m pytest scripts/test_skill_size_guard.py` com e sem `SKILL_SIZE_GUARD_REQUIRE_ROOTS=1` | 8 passed |
| testes novos contra `git archive HEAD` (original) | nested 107/181, no_jq 19/21, visible 1/2, sensor 18/18, env 9/16 vermelhos |
| mutação (harness em cópia, `hw-slot`) | C1 49/50 (1 equivalente); itens 27/27; **D2 121/121** (§10) |
| `hw-slot` | 2 chamadas devolveram rc 75 (`sem vaga após 600s`: slots de outras sessões + 94 °C > 88 °C; não é falha de teste); repetidas, verdes |

## 9. Avisos ao orquestrador

1. **Espelho `client/skills`**: a suíte pedida inclui `test_sync_client_skills.py`, que no snapshot `21cd6f19` reprovava 2 testes porque `client/skills/Touring/references/touring-per-project.md` está atrás do live (no **main** o `--check` dá CLEAN: o main carrega essa mudança não commitada). Rodei `sync-client-skills.py --apply` no worktree para a suíte ficar verde e **excluí do `py.patch`** esse arquivo e o `client/MANIFEST.sha256`. Não os aplique a partir do patch.
2. `scripts/test_model_routing_adherence.py` e `scripts/model_routing_adherence.py` também são editados por outro worktree (`touring-wt-roteamento`): minhas mudanças são localizadas (`resumo_gate`, `_linha_gate`, `PREFIXO_WHY_ENV`, 4 testes antes de `test_nudge_conta_por_evento_e_por_dia`) — possível conflito de merge, não de lógica.
3. Deploy: `_shell_mask.py` é importado do diretório do arquivo real de `block_git.sh` (`readlink -f`), então o symlink segue valendo; cria `client/hooks/__pycache__` (ignorado). `check_context.sh`/`status_monitor.sh` só produzem journal e heartbeat depois do redeploy e da próxima renderização da statusline.
4. Doc canônico a atualizar por quem implanta (live, fora do meu escopo): `~/.claude/skills/Touring/references/git-safety-protocol.md` — formas aninhadas negadas, notas `xargs`/orçamento e o comportamento sem jq.
5. Não feito: limpeza de `<sessão>.seen`/`.silent` acumulados em `context_alert/` (um arquivo minúsculo por sessão).
6. **Achados pré-existentes que NÃO corrigi** (fora do que foi pedido; REGRA #21 pede correção, então decida): `status_monitor.sh` tem 3 avisos do shellcheck idênticos no HEAD — `printf "${DIM}…"` com variável no formato (SC2059, info) e duas variáveis calculadas e nunca lidas, `WIN_SRC` (4 atribuições, marcada «Diag» no comentário) e `USAGE_ABS` (REGRA #0: ligar a um consumidor ou remover).
7. **Espelho `client/skills`, de novo**: no meio do D2 o live `~/.claude/skills/Touring/references/git-safety-protocol.md` mudou (alguém editou o texto da REGRA #11) e `test_sync_client_skills.py` voltou a reprovar 2 casos no worktree. Rodei `sync-client-skills.py --apply` **só no worktree** e excluí do `py.patch` os três arquivos do espelho (`touring-per-project.md`, `git-safety-protocol.md`, `MANIFEST.sha256`). No main o `--check` é a verdade.

## 10. D2 — contorno de hooks do git (aprovado pelo Gabriel)

**Motivo medido**: em 29/09 um agente rodou `git commit --no-verify` no `~/projects/analise` (pre-commit de PII/donos/LFS). O guard da REGRA #11 só julga git DESTRUTIVO; contornar hook não perde dado e passava.

**Contrato** (o texto da REGRA #11 fica com o lead; os fatos para redigi-lo):

- **Quando nega**: só se o repositório ALVO tem hook ativo. Alvo = o `cwd` do payload, ou o de `git -C <dir>` (encadeado: relativo soma, absoluto reseta; `~` e `$VAR` do ambiente do hook são expandidos; `-C` não resolvível cai no `cwd`), ou o de um `cd <dir> &&` anterior (**acréscimo meu**: sem ele `cd ~/projects/analise && git commit -n` olhava o repositório errado; `cd` sem argumento = `~`, `cd -` mantém o último lugar conhecido, `cd a b` é inválido e não move; `cd` dentro de `bash -c`/`$()` não vaza).
- **Hook ativo** = arquivo regular, executável e não-`.sample` em `git rev-parse --git-path hooks` (que já honra `core.hooksPath`, absoluto, relativo à raiz do worktree, com `~`; um `hooksPath` vazio ou inexistente = nada roda, mesmo com executável em `.git/hooks`). Worktree linkada usa os hooks do repositório principal (testado). `.pre-commit-config.yaml`: os ids vão no texto quando o shim do framework está instalado (hook que contém `pre-commit`); config sem shim não conta.
- **Formas negadas** (cada uma testada): `commit --no-verify` e `commit -n` (inclusive em cluster `-nm`/`-an`, depois de `-mx`/`-C HEAD`; abreviação `--no-veri…`), `push|merge|am|rebase|cherry-pick --no-verify`, `am -n`, `git -c core.hooksPath=<qualquer>` (qualquer caixa, qualquer subcomando), `--config-env=core.hooksPath=…`, `GIT_CONFIG_KEY_<n>=core.hooksPath` (+ `GIT_CONFIG_PARAMETERS`), `SKIP=<não vazio>`, `PRE_COMMIT_ALLOW_NO_CONFIG=1`, `HUSKY=0` — estas no próprio comando, em `env …`, em `export`/atribuição solta anterior, herdadas por `bash -c`/`eval`/heredoc/`$()`/wrappers (`timeout`, `nohup`, `xargs`…) e por interpretadores (`os.system`). As de ambiente só valem para subcomandos que rodam hook (`commit push merge am rebase cherry-pick revert pull`): `SKIP=x git status` passa.
- **Controles negativos testados** (passam): `push -n` (dry-run), `merge -n`/`rebase -n` (no-stat), `cherry-pick -n`/`revert -n` (no-commit), `log -n 5`, `commit -m -n`, `commit -mn` (mensagem «n»), `commit -F -n`/`-c -n`/`-C -n`/`-t -n`/`-am -n` (valor da opção), `commit -m x -- -n` (pathspec), `--message=-n`, `--no-ver` (ambíguo com `--no-verbose`), a flag dentro de `-m '…'`, `echo`, `grep`, comentário, `SKIP=` vazio, `HUSKY=1`, `PRE_COMMIT_ALLOW_NO_CONFIG=0`, `SKIP=x true; git commit` (escopo só do `true`), `export` dentro de `bash -c`/`$()` (não vaza), `-c user.name=…`, `-c my.core.hooksPath=…`.
- **Token**: `GIT_HOOKS_BYPASS_OK=1` por comando (código em qualquer nível, nunca dado: `echo 'GIT_HOOKS_BYPASS_OK=1'; git commit -n` continua negado). **Diferente** do `GIT_DESTRUCTIVE_OK=1` e sem snapshot. Os dois não se substituem: `GIT_DESTRUCTIVE_OK=1 git commit --no-verify` é negado (o teste do bypass vem **antes** do token destrutivo no script) e `GIT_HOOKS_BYPASS_OK=1 git reset --hard` também.
- **O deny nomeia tudo**, numa linha só (o JSON do motivo não aceita quebra): a forma, o repositório, o diretório de hooks e os executáveis (`hooks ativos em .git/hooks/: commit-msg, pre-commit`; ids do framework; corte em 12 com `(+N)`), a frase «contornar não perde dado, mas desliga a verificação que o repositório exige», a rota (rodar sem a opção e corrigir o que o hook reporta) e o token com o contraste com o destrutivo. Duas formas no mesmo comando saem juntas («`git commit --no-verify` e `SKIP=black` contornam…»); dois repositórios saem os dois; o sem hook não é citado.
- **Repo sem hook ativo, não-repositório, `hooksPath` vazio/ausente, `.sample`, arquivo não-executável, diretório em `hooks/`**: nada muda (allow).

**Desenho**: o mascarador (`_shell_mask.py`) só **observa**: troca `_git_sub_index` por `_git_globals` (índice do subcomando + diretórios `-C` + `-c`), carrega um `self.env` e um `self.cwd` com escopo (salvos/restaurados em `command()` e `text()`), e emite NOTAS `hooks-bypass:<forma>\x1f<dir>` (deduplicadas). Se a nota existe, `block_git.sh` chama **uma vez** `_shell_mask.resolve_main(<cwd do payload>)`, que olha o disco (`git rev-parse --show-toplevel` / `--git-path hooks`, timeout 5 s) e devolve o texto do deny (vazio = allow). Comando sem contorno **não paga nada** (nem lê o `cwd`). Falha do resolvedor é deny barulhento («não conseguiu verificar»), nunca allow mudo (testado com um `_shell_mask.py` que levanta exceção).

**Residuais (documentados, não corrigidos)**: (1) `git config core.hooksPath …`/`git config --local …` persistente e `--git-dir=`/`GIT_DIR=` não são julgados; (2) o caminho sem `jq` **e** sem `python3` (JSON bruto) não tem `cwd` nem notas: o bypass não é julgado ali (o destrutivo continua); (3) `cd` dentro de `( … )` vaza para o resto do texto (`(cd x); git commit -n` mira `x`; conservador na prática, pode errar o alvo para mais ou para menos conforme o que vem depois); (4) o texto de SKIP/HUSKY é negado em qualquer repo com hook ativo, mesmo que o hook não seja do pre-commit/husky (decisão literal do pedido: um falso positivo custa um token).

**Provas**: 439 testes no arquivo novo (repositórios temporários herméticos: `GIT_CONFIG_GLOBAL=/dev/null`, `HOME` próprio; fixtures `hooked`, `plain`, `pathed`, `pathed_missing`, `pathed_empty`, `framework`, `fake_framework`, `many`, `outside`, `tilde`, worktree linkada, árvore `parent/{sub,mid/repo,mid/plain}`) — **132 vermelhos no original** (`git archive HEAD` + o arquivo, com só `BYPASS_NOTE`/`SEP` injetados para o arquivo coletar; os 307 que passam são os controles de allow e de repo sem hook, que devem passar nos dois). O vermelho está nos casos de deny; verde no patchado: **439 passed**; suíte pedida **1216 passed rc 0**; `ruff` limpo; `shellcheck block_git.sh` rc 0; `probe_forms.py` (formas do C1) `problems: 0`.

**Mutação**: 122 mutantes na 1ª rodada (`spec_d2.py`, harness `mutate.py` em cópia, 3 fatias em paralelo dentro de **um** `hw-slot`): 112 mortos, 10 sobreviventes — todos viraram teste: `git -c`/`git -C` cortados no fim (a varredura estourava `IndexError`), `cd a b`, `export` dentro de `$()` vazando só o env, argumento com cara de atribuição (`-m SKIP=black`) e atribuição depois de palavra reservada, `GIT_CONFIG_KEY_` sem número, marca `(+N)` e junção « e » no texto, filtro de linhas do `resolve_main`. Dois sobreviventes eram **código redundante**, não teste fraco, e saíram do código: o ramo `git config --get core.hooksPath` (o `--git-path hooks` já o honra, inclusive `~`) e o `|| dir=""` / `${PWD}` do `payload_cwd` (a atribuição já deixa vazio e `git -C ""` é no-op). Rodada final: **121/121 mortos** de 123, mais os 2 equivalentes acima (removidos do código e do spec) — e os 4 mutantes que tocam as linhas simplificadas (`cwd argument dropped`, `cwd read from the wrong field`, `hooks dir fixed to .git/hooks`, `resolver sees no notes from stdin`) foram re-rodados no código final: 4/4. Resultados em `evidencia/mutacao_d2*.{log,json}`.

**Latência** (`evidencia/latency_d2.py`, 25 rodadas intercaladas, máquina compartilhada com outras sessões): sem contorno 83–88 mediana (não paga o resolvedor); com contorno + repo com hooks 134 ms; sem hooks 158 ms; com token 82 ms — o resolvedor custa ~50–70 ms só no caso raro.

**Para a REGRA #11 / `git-safety-protocol.md`** (live, com o lead): duas classes agora — destrutiva (ritual + `GIT_DESTRUCTIVE_OK=1`) e **contorno de hooks** (sem ritual; `GIT_HOOKS_BYPASS_OK=1`; só onde há hook ativo). Enforcement: `block_git.sh` + `_shell_mask.py`. Não editei texto nenhum em `~/.claude`.

## 11. Evidência

`evidencia/`: `probe_forms.py <dir_hooks>` (36 formas, DENY/allow), `mutate.py` + `spec_*.py` (harness e mutantes; D2: `spec_d2.py`, `spec_d2_shard.py`, `run_d2_shards.sh`), `mutacao_*.log|json` (resultados; D2: `mutacao_d2.json`, `mutacao_d2_rodada1.log`, `mutacao_d2_final.log`, `mutacao_d2_pos_simplificacao.log`), `suite_final.log` (antes do D2) e `suite_d2_full.log` (depois), `latency.py`, `latency_d2.py|.log`, `probe_d2.py` (14 casos DENY/allow de D2 contra repositórios temporários), `demo_empty_roots.py` (guard de skills medindo vazio no original).
