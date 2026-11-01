# Touring Per-Project — topologia rustup-like

<!-- Sem frontmatter e sem `paths:` de propósito: o daemon e o hook são globais. Um `---` de abertura sem
     fechamento passa a truncar a regra no 1º `---` futuro (L4-004, I13: o mesmo defeito das regras #11/#19). -->

**4 camadas:** L1 fonte `~/projects/touring` → L2 `~/.touring/toolchains/<v>/`
(imutável) → L3 shim CC `~/.claude/hooks/touring-hook` → L4 `<proj>/.touring/`.

- **Resolução:** `lock > pin > dev` — `.touring/toolchain.lock` (máquina) sobre
  `.touring/touring.toml` (pedido do humano).
- **Versão sai em stderr:** `.touring/bin/touring --version` sem `2>/dev/null`.
- **`TOURING_DAEMON_SOCKET` vence o walk-up** — para reproduzir:
  `env -u TOURING_DAEMON_SOCKET -u TOURING_DAEMON_SOCK <cmd>`.
- **Atualizar:** `touring update --project <root>` · reverter: `--rollback`.
- **Nunca editar** `.touring/bin/*` nem `toolchain.lock` à mão.

> Corpo canônico: `~/.claude/skills/Touring/references/touring-per-project.md`