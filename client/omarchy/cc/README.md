# Command Centre — instalação

O Command Centre gera um HTML auto-contido com 3 widgets (artefatos, routine board,
skills deck) e o serve em `http://127.0.0.1:7777`.

## Pré-requisitos

- Python ≥ 3.12 (stdlib only, sem dependências externas)
- systemd --user com lingering habilitado: `loginctl enable-linger $USER`
- `omarchy-skill-run` em `~/.local/bin/` (escrito por `client/omarchy/bin/omarchy-skill-run`)
- `cc_build.py` em `~/.local/lib/omarchy/` (copiado pelo passo 2 abaixo)

## Instalação

```bash
# 1. Copiar cc_build.py para ~/.local/lib/omarchy/
mkdir -p ~/.local/lib/omarchy
cp client/omarchy/bin/cc_build.py ~/.local/lib/omarchy/

# 2. Criar cc.json com os caminhos do seu sistema
mkdir -p ~/Work/cc
install -Dm644 client/omarchy/cc/cc.json.example ~/.config/omarchy/cc.json
# Editar ~/.config/omarchy/cc.json se necessário (FORA da raiz servida:
# qualquer arquivo em ~/Work/cc é publicado em 127.0.0.1:7777)

# 3. Copiar units para ~/.config/systemd/user/
cp client/omarchy/cc/cc.service    ~/.config/systemd/user/
cp client/omarchy/cc/cc.timer      ~/.config/systemd/user/
cp client/omarchy/cc/cc-serve.service ~/.config/systemd/user/

# 4. Habilitar e iniciar
systemctl --user daemon-reload
systemctl --user enable --now cc.timer cc-serve.service

# 5. Instalar como web app no Omarchy
omarchy-webapp-install "Command Centre" http://127.0.0.1:7777 globe
```

## Uso

- **Acesso**: abrir `http://127.0.0.1:7777` no browser ou via web app Omarchy.
- **Rebuild manual**: `python3 ~/.local/lib/omarchy/cc_build.py --config ~/.config/omarchy/cc.json`
- **Brain**: `python3 ~/.local/lib/omarchy/cc_build.py --brain` gera
  `~/Work/cc/brain/index.html` a partir de `~/Work/brain/moc.md` (produzido por
  `touring memory moc <topico>` — o topico e' obrigatorio). A saida fica **dentro**
  da raiz publicada pelo `cc-serve.service` (`~/Work/cc`), que e' o que faz
  `http://127.0.0.1:7777/brain/` resolver. A fonte (`moc.md`) fica fora da raiz
  de proposito: publicar `~/Work` inteiro exporia `pessoal/`, `runs.log` e `state/`.
- **Status**: `systemctl --user status cc.timer cc-serve.service`

## Widgets

| Widget | `data-widget` | Fonte |
|--------|--------------|-------|
| Artefatos & Runs | `artifacts` | `~/Work/artifacts/` (últimos 20 por mtime) + `~/Work/runs.log` (últimas 10 linhas) |
| Routine Board | `routines` | `routines_gen.py board --json` + `~/Work/state/validate-*.json` |
| Skills Deck | `deck` | `~/.config/omarchy/plugins/gabriel.skills-deck/deck.json` |

Fonte ausente → o widget mostra `pendente — <caminho>` e o build sai com código 0.

## Tema

Lê `~/.config/omarchy/current/theme/colors.json` (chaves: `background`, `foreground`, `accent`).
Se ausente, usa paleta Catppuccin Mocha (dark) / Latte (light) como fallback.
`prefers-color-scheme` é respeitado via CSS media query.
