# scripts/ — tooling operacional do workspace

## Description

Ferramentas que operam o ciclo de vida do Touring: deploy, propagação de
release, espelho `client/` e guardas de configuração. Todas versionadas aqui —
a lição de 18/08/2026 é que ferramenta de deploy fora do repo é irrevisável e
não sobrevive a uma máquina nova.

## Install

Nada a instalar além do symlink do deploy tool (uma vez por máquina):

```bash
ln -sfn "$(pwd)/scripts/update-touring" ~/.local/bin/update-touring
```

## Usage

```bash
update-touring                                   # rebuild + deploy completo
scripts/propagate-release.sh <versão>            # propagar release aos projetos pinados
python3 scripts/sync-client-skills.py --check    # verificar espelho client/
python3 scripts/sync-client-skills.py --apply    # sincronizar live -> client/
```

| Script | Papel | Testes |
|---|---|---|
| `update-touring` | Pipeline canônico de rebuild/deploy (kill → build → install dual-target → restart → verify). No PATH via **symlink** `~/.local/bin/update-touring`. Raiz derivada do próprio caminho do script — nunca a árvore congelada `~/.claude/rust`. | `test_update_touring.py` (invariantes de conteúdo, CI) |
| `propagate-release.sh` | Propaga uma versão para os projetos pinados: gates → `update-touring` → `toolchain install` → `touring update` por projeto → verify. `--rollback` reverte. | gate 1/6 roda `test_update_touring.py` com `UPDATE_TOURING_REQUIRE_SYMLINK=1` (skip vira falha no release) |
| `sync-client-skills.py` | Espelho versionado das skills vivas: direção única `~/.claude` → `client/`. `--check` (exit 1 sob drift) · `--apply [--prune]`. O sweep de junk e o prune são restritos às áreas espelhadas (`skills/rules/agents`) — `client/omarchy` é FONTE, nunca espelho. | `test_sync_client_skills.py` (CI, sem lado vivo via `client/MANIFEST.sha256`) |
| `test_mutants_config.py` | Guarda da configuração do cargo-mutants. | próprio |
| `touring_premium_refactor_2026/` | Arquivo histórico: geradores/fixtures do wave 2026-05-11 (55 scripts). Contêm crates-fixture (`w5-*`, `w7-*`, `w10-*`) com duplicação INTENCIONAL de scaffold — não são código de produção e rebaixam qualquer score de duplicação medido sobre `scripts/` inteiro. | — |

## Regras que estes scripts encapsulam

- **REGRA #19** — nunca `pkill`/`pgrep` por nome: `update-touring` resolve o
  dono do socket global (`global_daemon_pid`, via lsof + PID file +
  `/proc/<pid>/comm`), com socket/lock derivados de `$(id -u)`, nunca uid fixo.
- **Gotchas de verificação** — `touring --version` escreve em stderr (ler com
  `2>&1`, nunca `2>/dev/null`); não usar `| head -1` no output (EPIPE → SIGABRT
  sob `pipefail`); cortar a primeira linha em bash puro.
- **Espelho é gerado** — editar `client/` à mão é desfeito pelo próximo
  `--apply`; o lado vivo é a fonte.

## Tests

```bash
python3 -m pytest scripts/test_update_touring.py scripts/test_sync_client_skills.py \
    scripts/test_mutants_config.py -q
```

Rodam no CI (`.github/workflows/ci.yml`) sem o lado vivo — a integridade do
espelho é provada contra `client/MANIFEST.sha256`.

## Contributing

Mudanças nestes scripts passam pelos mesmos gates do workspace (CLAUDE.md §3):
todo invariante novo ganha um teste no arquivo `test_*.py` correspondente.

## License

Mesma licença do workspace Touring (ver `LICENSE` na raiz do repositório).
