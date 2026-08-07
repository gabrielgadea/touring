# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [30.3.1](https://github.com/gabrielgadea/touring/releases/tag/v30.3.1) - 2026-08-07

### Added

- *(rl,ci,tantivy)* credit loop do Memento, registry per-project do tantivy e os três jobs vermelhos do CI

### Fixed

- *(tests)* resolve binários pelo workspace, não pelo home de um desenvolvedor
- *(ci)* rustfmt no código publicado e referência gerada em sincronia
- *(ci)* green the remaining jobs — deny, root hygiene, daemon E2E
- *(ci)* green the check+clippy, fuzz and coverage jobs
- *(daemon-ctl)* registry-first PID resolution + orphan-scoped reap fallback — kills cascading daemon kill (F-NEW-4)
- *(ci+release)* capnproto system dep + cargo fmt --all (35 files) + ARCHITECTURE crate-count 42

### Other

- Pln2 productization: per-project toolchains, touring update/component, installer, release pipeline (F0-F5 + PILOT)
# Touring — Product Release Changelog

Maintained by release-plz from tagged releases of the `touring` binary
(package `touring-server`). The workspace-wide engineering history lives in
the root `CHANGELOG.md` (toon-synth + hand-curated waves — not parseable as
keep-a-changelog, hence this dedicated file).

## [30.3.0] - 2026-07-24

First GA-ready cut — Pln2 productization (singleton global → installable,
versioned, per-project system). Highlights:

- Per-project daemons (per-socket lock, root pinned by construction).
- `touring update` (lockfile + deterministic `--rollback`) + `touring component`.
- `touring toolchain install --from-source | --from-url`; functional
  `install.touring.dev.sh` (sha256 fail-closed, sigstore-ready).
- Release CI: `bin/`-layout tarballs, sha256, SBOM, sigstore keyless.
- Security: crossbeam-epoch RUSTSEC-2026-0204 remediated; yanked `spin` replaced.
- `touring license status` — tier substrate visible, no gating.
