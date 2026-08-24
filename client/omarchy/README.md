# `client/omarchy/` — fonte versionada do OmarchyOS Agêntico

> Plano: `docs/plans/2026-08-23-omarchyos-agentico/plan.md` (Pln2, DAG `task_1787501528611029513`).
> **Direção**: ao contrário de `client/{skills,rules,agents}` (gerados do live), **esta árvore é a fonte** —
> o Omarchy instalado em `nvme-SM2P41C8-001TC5_KP102L1HDJDW` é provisionado a partir daqui.
> O sync (`scripts/sync-client-skills.py`) nunca toca esta árvore (`AREAS` é explícito).

## Contratos (válidos para todo script desta árvore)

| Regra | Detalhe |
|---|---|
| **Fail-closed** | Ferramenta ausente = `FAIL missing-tool <nome>`, nunca skip silencioso. Exit 0 só se **todos** os checks passaram. |
| **Saída** | Humana por padrão: uma linha por check `ok <nome> <evidência>` / `FAIL <nome> <evidência>` / `warn <nome> …` (warn não falha). `--json` → `{"phase":"P<n>","ok":bool,"checks":[{"name","status":"ok|fail|warn","evidence"}]}`. |
| **Bash** | `#!/usr/bin/env bash` + `set -euo pipefail`; `shellcheck -S warning` limpo; sem `|| true` fora de limpeza; **nunca `sudo` em scripts de background** (Quattro usa `pkexec`); discos **só por `/dev/disk/by-id/...`**, nunca `nvme0/1`. |
| **Python** | ≥ 3.12, **stdlib only** (`tomllib`, `json`, `hashlib`, `pathlib`, `subprocess`); `ruff check` limpo; `--json` e `--help` em todo CLI; testes em `tests/test_<nome>.py` com asserção exata. |
| **Segredos** | Nunca nesta árvore: tokens, passphrases, `disk_encryption`. Teste `test_no_secrets.py` faz grep. |
| **Caminhos** | Home fixa `gabrielgadea` (decisão D2): os caminhos absolutos de `settings.json` e `SYMLINKS.tsv` batem sem reescrita. `~/Work` é o diretório de trabalho dos agentes no Omarchy. |
| **Permissões** | Launcher do Omarchy = `claude --permission-mode auto` (fato, `bin/omarchy-agent:70`). Headless (timers/deck) = `acceptEdits`. `bypassPermissions` **proibido** em `deck.json`/`routines.toml` (teste). |

## Mapa da árvore

```
bin/
  migration_kit.sh        S-0.2  gera ~/omarchy-kit (rsync + MANIFEST.sha256 + SYMLINKS.tsv + kit.json)
  kit_check.sh            S-0.2  confere o kit (--self na origem; destino: hashes + `find -xtype l`)
  settings_stage.py       S-0.3  settings.json → stage A (sem hooks Touring/legados) | stage B (idêntico)
  hooks_runnable.py       S-0.3  todo `hooks.*[].hooks[].command` registrado é executável
  validate_p0.sh … validate_p8.sh, validate_all.sh   S-0.4
  wipe_target.sh          S-1.1  guarda do wipefs (serial + montagens + Pop com LUKS ativo)
  omarchy-skill-run       S-0.5  executor único: <skill> <model> <effort> <mode> <cwd> [--herdr] [--dry-run]
  routines_gen.py         S-0.6  routines.toml → systemd --user units | board [--json]
  cc_build.py             S-0.8  cc.json → ~/Work/cc/index.html (3 widgets) | --brain
  cidata_build.sh         S-0.9  pede credenciais → cidata_gen.py → cidata.iso em tmpfs
  cidata_gen.py           S-0.9  porta do wizard do ISO: archinstall JSON com partições calculadas
  cidata_iso.py           S-0.9  fallback ISO9660 (pycdlib) quando não há genisoimage/mkisofs/xorrisofs
skills-deck/              S-0.5  plugin Omarchy `gabriel.skills-deck` (manifest.json, BarWidget.qml, Panel.qml, deck.json)
extensions/omarchy-menu.jsonc     S-0.5
hooks/post-boot.d/20-touring-daemon · hooks/post-update.d/{30-touring-doctor,35-hooks-runnable,40-limine-rescan,45-touring-ci-fire}   S-0.7
adw/fragments/herdr-fanout.toml + adw/herdr-fanout-demo.toml   S-0.7
cc/ (template, cc.service, cc.timer, cc-serve.service)          S-0.8
cidata/user_configuration.template.json                        S-0.9
hypr/bindings.skills.lua                                       S-0.5 (opcional)
routines.example.toml                                          S-0.6
vendor/omarchy-plugin-validate                                 cópia de basecamp/omarchy@quattro (bash+jq) — valida o plugin no Pop
tests/                                                         pytest; tests/bin/herdr = mock
```

## Ordem de execução no Omarchy (resumo; detalhe no plano §3)

P1 `wipe_target.sh` (Pop) → instalação → `validate_p1.sh` · P2 `validate_p2.sh` · P3 `kit_check.sh ~/omarchy-kit` + `settings.stageA.json` + `validate_p3.sh` · P4 `update-touring` + `settings.stageB.json` + `validate_p4.sh` · P5 `omarchy-skill-run` + menu + plugin · P6 router + `validate_p6.sh` · P7 `routines_gen.py apply` + `validate_p7.sh` · P8 `cc_build.py` + `validate_p8.sh` · CLOSE `validate_all.sh` + `loop_converged.py`.

## Validação local (no Pop)

```bash
bash -n client/omarchy/bin/*.sh && shellcheck -S warning client/omarchy/bin/*.sh client/omarchy/hooks/*/*
ruff check client/omarchy/bin/*.py client/omarchy/tests
python3 -B -m pytest -q client/omarchy/tests   # -B: nenhum __pycache__ em client/ (teste de higiene do espelho)
client/omarchy/vendor/omarchy-plugin-validate client/omarchy/skills-deck
touring adw lint herdr-fanout-demo
client/omarchy/bin/validate_p0.sh --json
```
