# REGRA #11 v2 — Git Safety Protocol (git PERMITIDO sob protocolo anti-perda)

> **Auto-load** (constitutional) | **Version**: v2.1 | **Date**: 2026-10-17 (classe CONTORNO DE VERIFICAÇÃO; v2.0 23/08)
> **Authority**: Gabriel Gadea — revoga "GIT PROIBIDO" (v1, 06/04/2026) por ordem
> explícita: *"desative a REGRA #11 mas adotando sempre protocolos cuidados para não
> perdermos nenhum trabalho em disco"*.
> **Enforcement em runtime**: `~/.claude/hooks/block_git.sh` (git-safety-guard,
> PreToolUse Bash) — bloqueia a classe DESTRUTIVA sem o token de ritual e o CONTORNO DE VERIFICAÇÃO sem o seu token. É o **dono único**
> da política (S25, 26/09: `test_git_reset_hard_without_token_denied_by_exactly_one_hook`).
> Mensagem de commit pode CITAR operação destrutiva (heredoc, `-F`, `-m '…'` passam — sondado
> 27/09 com o comando real recusado como controle); o token dentro de um heredoc não libera.
> **Formas aninhadas (17/10/2026, C1 da análise do ECC)**: o guard re-escaneia como código o que é entregue a
> algo que executa — `bash|sh|zsh -c` (qualquer cluster: `-lc`, `-o pipefail -c`), `eval`, heredoc/here-string/pipe
> para shell, `$(…)`, crases, `<(…)`, `find -exec`, lançadores (`env`, `timeout`, `nice`, `sudo`, `hw-slot … --`,
> `xargs`…) e argumentos de `os.system`/`subprocess`/`system`/`exec*` em `python -c`, `perl -e`, `node -e` e afins.
> `git -C d`, `git -c k=v` e flag entre aspas (`git reset '--hard'`) são normalizados. `xargs git <destrutiva>` nega,
> porque os argumentos vêm do stdin. Profundidade > 8 ou trabalho acima do orçamento (`max(8192, 16 × entrada)`)
> nega nomeando o motivo. O token vale como código em qualquer nível, nunca como dado. **Residual declarado**:
> programa montado em runtime (`bash -c "$CMD"`, `eval "$x"`), arquivo de script, `ssh host '…'`, `docker exec`,
> alias e `git -c alias.x='!…'`. Prova: `client/hooks/tests/test_s25_block_git_nested.py` (181 casos) e a reprodução
> de 4.000 comandos reais com `git` (0 negações novas; `docs/plans/2026-10-17-analise-ecc/scripts/replay_block_git.py`).
> **Origem da v1 (nunca esquecer)**: `git stash`+`pop` destruiu 162 módulos de
> `touring-hooks/src/lib.rs` em 06/04/2026 (estado nunca commitado; 6h de retrabalho).

## Princípio

Git é ferramenta legítima do TACO. **NENHUMA operação pode perder trabalho em
disco.** O working tree não commitado é presumido insubstituível até prova em
contrário — a v1 caiu exatamente por um stash sobre estado nunca commitado.

## Classes de operação

| Classe | Operações | Política |
|---|---|---|
| **LEITURA** | `status` `log` `diff` `show` `blame` `ls-files` `stash list` `reflog` `fetch` `branch` (listar) | Livres, sempre |
| **ADITIVA** | `add` `commit` (incl. `--amend` local) `branch <nova>` `tag` `merge` (abortável) `push` (sem force) `clone` `init` `checkout -b` `switch -c` `restore --staged` (só unstage) | Livres — nunca destroem working tree |
| **CONTORNO DE VERIFICAÇÃO** (só em repo com hook ativo) | `commit --no-verify`/`-n` · `push`/`merge`/`am`/`rebase`/`cherry-pick --no-verify` · `am -n` · `-c core.hooksPath=…` · `--config-env`/`GIT_CONFIG_KEY_<n>`/`GIT_CONFIG_PARAMETERS` apontando para `core.hooksPath` · `SKIP=<não vazio>` · `PRE_COMMIT_ALLOW_NO_CONFIG=1` · `HUSKY=0` | **Token** `GIT_HOOKS_BYPASS_OK=1` por comando, sem ritual (nada se perde) |
| **DESTRUTIVA** | `stash` (push/save/pop/apply/drop/clear) · `reset --hard/--merge/--keep` · `checkout -- <path>` / `checkout -f` / `checkout .` · `switch --discard-changes` · `restore` (worktree) · `clean -f/-d` · `rebase` · `push --force`/`-f`/`--force-with-lease` · `branch -D` · `filter-branch`/`filter-repo` · `reflog expire` · `gc --prune=now` | **SÓ com o ritual + token** |

## Contorno de verificação (D2, Gabriel 17/10/2026)

**Origem**: em 29/09 um agente rodou `git commit --no-verify` no `~/projects/analise`, cujo pre-commit checa PII,
donos e LFS; o guard só julgava git destrutivo e deixou passar.

- **Só onde há algo a proteger**: o repositório ALVO (o `cwd` do comando, um `cd <dir> &&` anterior ou `git -C`) tem
  hook ativo = arquivo regular e executável com nome de hook do git (githooks(5): `pre-commit`, `commit-msg`,
  `pre-push`…) no diretório de `git rev-parse --git-path hooks`, que já honra `core.hooksPath`. `.sample`, backup
  (`pre-commit.bak`) e diretório não contam. Repo sem hook ativo: nada muda.
- **Formas**: as da tabela, também em `env`/`export`, herdadas por `bash -c`, `eval`, heredoc, `$()`, lançadores e
  interpretadores. As de ambiente (`SKIP`, `HUSKY`…) valem só para subcomandos que rodam hook. Não são contorno:
  `push -n` (dry-run), `merge -n`/`rebase -n` (no-stat), `cherry-pick -n`/`revert -n` (no-commit), `-n` como valor
  de opção (`commit -m -n`).
- **Rota**: rodar sem o contorno e corrigir o que o hook reporta. Intencional (hook quebrado, por exemplo):
  `GIT_HOOKS_BYPASS_OK=1` no próprio comando — código em qualquer nível, nunca dado. `GIT_DESTRUCTIVE_OK=1` não o
  substitui, e ele não substitui o destrutivo.
- **O deny nomeia** a forma, o repositório, os hooks ativos e os ids do framework pre-commit quando o shim dele está
  instalado. Falha do resolvedor é deny barulhento, nunca allow mudo.
- **Residual declarado**: `git config core.hooksPath …` persistente, `--git-dir=`/`GIT_DIR=`, o caminho sem `jq` e
  sem `python3`; `SKIP`/`HUSKY` negam em qualquer repo com hook ativo, mesmo sem pre-commit ou husky.
- **Prova**: `client/hooks/tests/test_s25_block_git_hooks_bypass.py` (439 casos) e o replay de 4.000 comandos reais
  com o `cwd` de cada um (`docs/plans/2026-10-17-analise-ecc/scripts/replay_block_git.py`): 1 negação nova, a de
  29/09.

## Ritual pré-destrutiva (obrigatório, na ordem)

1. **MEDIR** — `git status --porcelain` + `git stash list`: o que exatamente está
   em risco? Nomear os arquivos que a operação pode descartar.
2. **SNAPSHOT** — trabalho não commitado em risco → commit WIP em branch de
   segurança: `git checkout -b safety/<data>-<slug> && git add -A && git commit -m
   "WIP safety pre-<op>"` (depois voltar à branch). Estado fora do git (untracked
   ignorados, DBs, hooks) → snapshot snapper (`sudo snapper create -d "pre-<op>"`,
   btrfs no Omarchy, barato) e/ou `touring memory store`.
3. **CONFIRMAR COM GABRIEL** — obrigatório quando: push com qualquer force ·
   reescrita de histórico já publicado · a operação descarta mudanças que esta
   sessão não criou (outra sessão/humano pode estar editando).
4. **EXECUTAR** com a forma menos destrutiva e o token consciente no próprio
   comando: `GIT_DESTRUCTIVE_OK=1 git <op>`. O token é por-comando (nunca
   exportado na sessão) — cada uso é uma decisão, não um estado.
5. **VERIFICAR** — `git status` + build/teste rápido quando código foi tocado.

## Proibições que PERMANECEM (mesmo com token)

- **`git stash` como guarda de trabalho** — commit WIP em branch safety é SEMPRE
  preferível (stash é a arma do incidente de 06/04; pop com conflito perde estado).
  O guard exige token para qualquer stash mutante; o uso deve ser excepcional.
- **`git push --force` em branch compartilhada** — só `--force-with-lease`, só com
  aprovação explícita de Gabriel, nunca em `main`.
- **`git clean` sem `-n` (dry-run) antes** — medir o que será apagado primeiro.
- **Histórico publicado nunca é reescrito** sem ordem de Gabriel.

## Integração Touring (complementar, não substituto)

`touring memory store` antes de refactors L3+ segue valendo (snapshot semântico) ·
`touring status/e2e/doctor` seguem como gates de saúde · snapper é a rede de
sistema no Omarchy. Git soma versionamento real a isso — não o substitui nem é
substituído.

## Kill switch do guard

`GIT_GUARD_ENABLED=0` no topo de `block_git.sh` desliga o guard inteiro
(decisão humana). Leitura do comando: jq → python3 → JSON bruto, e o stderr nomeia a causa quando
um leitor falta. Sem leitor nenhum, git destrutivo no texto bruto é **negado** e o resto passa com aviso;
payload sem comando continua mudo. Estouro de profundidade ou de orçamento também nega (fail-closed),
sempre nomeando a rota (`GIT_DESTRUCTIVE_OK=1` no próprio comando).
