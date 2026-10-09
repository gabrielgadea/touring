# REGRA #11 v2 — Git Safety Protocol (git PERMITIDO sob protocolo anti-perda)

> **Auto-load** (constitutional) | **Version**: v2.0 | **Date**: 2026-08-23
> **Authority**: Gabriel Gadea — revoga "GIT PROIBIDO" (v1, 06/04/2026) por ordem
> explícita: *"desative a REGRA #11 mas adotando sempre protocolos cuidados para não
> perdermos nenhum trabalho em disco"*.
> **Enforcement em runtime**: `~/.claude/hooks/block_git.sh` (git-safety-guard,
> PreToolUse Bash) — bloqueia a classe DESTRUTIVA sem o token de ritual. É o **dono único**
> da política (S25, 26/09: `test_git_reset_hard_without_token_denied_by_exactly_one_hook`).
> Mensagem de commit pode CITAR operação destrutiva (heredoc, `-F`, `-m '…'` passam — sondado
> 27/09 com o comando real recusado como controle); o token dentro de um heredoc não libera.
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
| **DESTRUTIVA** | `stash` (push/save/pop/apply/drop/clear) · `reset --hard/--merge/--keep` · `checkout -- <path>` / `checkout -f` / `checkout .` · `switch --discard-changes` · `restore` (worktree) · `clean -f/-d` · `rebase` · `push --force`/`-f`/`--force-with-lease` · `branch -D` · `filter-branch`/`filter-repo` · `reflog expire` · `gc --prune=now` | **SÓ com o ritual + token** |

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
(decisão humana). O guard é fail-open em erro de parse (consistência com os
demais hooks — a constituição cobre o residual).
