---
type: ReleaseChecklist
title: "Release 30.3.0 — checklist git + runbook de publicação (Gabriel)"
description: "Sequência exata para cortar a primeira release GA-ready do Touring: commit do programa Pln2, tag SemVer, publicação de artefatos e DNS — tudo que ficou na fronteira git/infra"
tags: [release, pln2, f5, git-boundary, runbook]
timestamp: 2026-07-24
plan: /00-INDEX.md
---

# Release 30.3.0 — Checklist (git + infra são teus; tudo abaixo está PRONTO)

> Estado ao entregar: workflows promovidos no working tree, CHANGELOG cortado,
> artefato local em `dist/`, packaging corrigido, `validate_phase5.sh` 10/10.
> **Decisão de numeração**: proposto **v30.3.0** — consistente com
> `[workspace.package] version`, toolchains instaladas e pins dos projetos
> (mudar para 1.0.0-rc.1 agora criaria drift com meta.toml/locks já vivos;
> os `v30.3.4` antigos no CHANGELOG eram convenção de wave, não tags reais).

## 1 · Git — commit do programa + tag (REGRA #11: só tu executas)

```bash
cd ~/projects/touring

# (se o repo ainda não é git) git init && git add -A && git commit -m "..."
# Revisar o que o programa mudou (working tree já contém TUDO):
git status
git diff --stat

git add -A
git commit -m "Pln2 productization: per-project toolchains, touring update/component, installer, release pipeline (F0-F5 + PILOT)

- W12.5 per-project daemons (per-socket lock, root pinned by construction)
- W12.3 touring update (lockfile + rollback) + touring component
- toolchain install --from-source/--from-url; install.touring.dev.sh ativo
- release.yml: bin/ layout + sigstore keyless; release-plz + docs-rs-mirror promovidos
- security: crossbeam-epoch RUSTSEC-2026-0204 + spin yanked remediados
- CLAUDE.md 3 camadas + rules/touring-per-project.md"

git tag -a v30.3.0 -m "Touring 30.3.0 — first GA-ready cut (Pln2 productization)"
git push origin main --tags   # requer remote configurado
```

**O que a tag dispara** (workflows já promovidos em `.github/workflows/`):
- `release.yml` — builda musl+darwin, tarballs `bin/`-layout + sha256 + SBOM +
  cosign keyless + smoke; publica GitHub Release.
- `release-plz.yml` — mantém CHANGELOG/versão/tag nas releases FUTURAS
  (publish=false; só `GITHUB_TOKEN`).
- `docs-rs-mirror.yml` — espelho de docs.

## 2 · Infra — publicar artefatos + DNS

```bash
# Artefato local já pronto (mesmo shape do CI):
ls ~/projects/touring/dist/
#   touring-x86_64-unknown-linux-gnu.tar.gz         (72M)
#   touring-x86_64-unknown-linux-gnu.tar.gz.sha256  (0bf5b671…)
```

1. **Servidor de releases**: subir `dist/*` (e/ou os artefatos do GitHub Release
   do CI) para `releases.touring.dev/<versão>/` — o instalador espera
   `<base>/<versão>/touring-<triple>.tar.gz` + `.sha256` (+ `.sigstore` p/ GA).
   Alternativa zero-infra: usar o GitHub Release como base —
   `TOURING_RELEASES_BASE=https://github.com/<org>/touring/releases/download/v30.3.0`.
2. **DNS**: `install.touring.dev` → servir `scripts/packaging/install.touring.dev.sh`
   (estático); `releases.touring.dev` → bucket/CDN dos artefatos.
3. **Smoke pós-publicação** (1 comando):
   `curl --proto '=https' --tlsv1.2 -sSf https://install.touring.dev | sh -s -- --version 30.3.0 --dry-run`
4. **Homebrew/Scoop**: `scripts/packaging/homebrew/touring.rb` (targets = matrix
   real, layout bin/) e `scoop/touring.json` prontos; os `sha256` são preenchidos
   pelo pipeline; requerem tap/bucket (`homebrew-tap`, `scoop-bucket`) — Windows
   segue W14.

## 3 · Pós-release (voltar para mim quando quiseres)

- `touring toolchain install --from-url <URL do release> 30.3.0` num projeto —
  prova o canal público end-to-end.
- W14: matrix expansion (macOS Intel, Linux ARM, Windows) + latest-resolver do
  instalador.
