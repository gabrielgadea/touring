# RETOMADA — B · F · C · E (09/11/2026)

Pedido do Gabriel: «prossiga com: B · F · C · E» (revisão de arquitetura, após o candidato A —
`task_1794206289311361997`, CONVERGED, release 30.4.107). Decisões: `DECISOES.md`; fatos:
`fatos-{B,F,C,E}.md`. DAG `task_1794217886022321956`.

## Feito (commit · evidência)
- **E** `72614ba3` — `touring_foundation::hook_wire` (HookWire::parse/Decision/constantes);
  trace, tool_trio, exit_code_block_reason migrados. 3708/3708 + clippy.
- **F** `251a24cc` — `vgp::lang::coletor` (mais_proximo total, com_sugestao, reais_ordenados,
  empurrar); rust determinístico, css em chars. 1713/1713, replay VGP intacto, clippy.
- **C** `f682afda` — crate folha `touring-python-env` (Disco, raízes do interpretador,
  pyvenv.cfg, extensão C; DiscoReal na L2, DiscoContido na L1). 1917 + 5, clippy.

## Em curso — B (não commitado)
- `touring-code` depende de `touring-pyright`; `vgp/camadas.rs` (`camada2_em_vgp`,
  `fundir_camada2`, `alvo_do_escopo`) é a composição única; `sombra.rs` usa `fundir_camada2` e
  `escopo_de(alvo)`; `vgp_l2::mapear` delega; a CLI (`touring-server/src/cli/vgp.rs`) usa
  `camadas` e perdeu `no_momento`/`fundir_camada2` (helper de teste equivalente no mod tests).
- `cargo check --tests` de code+server+hook-handlers: rc 0.
- Falta: `cargo test -p touring-code -p touring-server -p touring-hook-handlers --features
  touring-hook-handlers/pre-hooks,touring-hook-handlers/post-hooks` (+ `-p touring-hooks` e2e
  `vgp_ativo_ordem_e2e`), clippy, commit; `nega()` dos handlers (opcional, DECISOES).
- Mudança de comportamento a registrar: L2 `unknown` não funde achados também na sombra (antes
  só a CLI não fundia).

## Depois
Release único (bump 30.4.108, `scripts/propagate-release.sh 30.4.108`, log com `~`), prova
(`provar_bypasses.py 30.4.108` + W4 `provar_w4.py` adaptado à versão), fechar DAG,
`loop_converged.py --task task_1794217886022321956 --scope crates/touring-code/src/vgp`.

## Atualização — release 30.4.108 bloqueado (09/11/2026, tarde)
- B commitado `7fb59013` (5626/5626). Versão 30.4.108 `3bd9aec6`; refs `93c97cd4`; dispatch `98edff7c`.
- Tentativas do release: drift docs (corrigido), disco cheio (safe-clean incremental: 199 GB),
  `updated_input_producers_are_pinned` (corrigido), e agora `standalone_table_parity`
  (`pre-bash#git-force`): causado por `28dbe17e` de OUTRA sessão (pre-bash deixa o git só com
  block_git.sh), que não atualizou esse teste.
- Medido no block_git.sh: `env -iS 'git reset --hard'` passava → corrigido `6eb0c5a0` (692/692).
  Ainda passam (pré-existentes; política): `git clean --force` (regex só curta) e
  `git checkout <caminho>` sem `--`. `~/.claude/hooks/test_block_git.py` (24/09) tinha leitor
  quebrado (substring com espaço) → corrigido; restam 8 falhas que codificam política antiga
  (`git checkout feat` negado). Decisão pedida ao Gabriel.

## Atualização — decisões do Gabriel aplicadas (09/11/2026, noite)
- «Aceitar e ajustar»: `standalone_table_parity` perdeu a exceção `pre-bash#git-force` (`96b7a8f3`);
  as linhas de git da prova W5 são perguntadas ao `block_git.sh` (`provar_bypasses.py`).
- «Corrigir clean --force agora»: `5597b23e` (clean --force, checkout <ref> -- <caminho>) e, após a
  revisão de segurança, `96b7a8f3` (prefixo de opção longa e cluster getopt em clean/checkout/
  restore; clean só passa em dry-run). Fixture `git_parity` 279 → 310, shell e Rust 0 divergências.
  `~/.claude/hooks/test_block_git.py`: 48 passed + 6 xfail strict (checkout de caminho SEM `--`:
  decisão de política pendente).
- `7817e905`: cluster getopt no lexer (env -uS, -iu NAME, sudo -nu) em Rust e no mascarador.
- Prova W5 contra o 30.4.107 instalado: 30/32, só as 2 linhas do `7817e905` (RED esperado).
- Próximo: `scripts/propagate-release.sh 30.4.108` → W5 + W4 contra 30.4.108 → fechar DAG → juiz.
