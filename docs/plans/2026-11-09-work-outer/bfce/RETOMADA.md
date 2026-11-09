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
