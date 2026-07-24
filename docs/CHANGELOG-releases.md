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
