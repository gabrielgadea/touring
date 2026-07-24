# W13 Publishing Pipeline — Promotion Guide (repo-premium)

> **ESTADO 2026-07-24 (Pln2 F5)**: PROMOÇÃO DE ARQUIVOS FEITA — `release-plz.yml`
> e `docs-rs-mirror.yml` já copiados para `.github/workflows/` (working tree);
> `ci.yml` e `release.yml` já estavam lá, e o sigstore keyless foi INCORPORADO
> ao `release.yml` (o `sigstore-release.yml` citado abaixo não existe mais como
> arquivo separado). Resta apenas o **commit + tag** — sequência exata em
> `docs/plans/touring-productization-pln2/RELEASE-CHECKLIST.md`. O guia abaixo
> permanece como registro histórico.

> **Authored**: 2026-05-31 (W13.5 sigstore + W13.6 release-plz materialized).
> Direct writes to `.github/workflows/` are blocked by a security hook, so the
> workflows are **staged here** and Gabriel promotes them manually (git is
> Gabriel's domain — REGRA #11).

## What is already in place (repo root — live)

| File | Status | Purpose |
|------|--------|---------|
| `release-plz.toml` | ✅ committed | Binary-product release config (`publish = false`; version + CHANGELOG + tag + GitHub release for the `touring-server` package). TOML validated. |
| `deny.toml` | ✅ refreshed + GREEN | `cargo deny check` passes all 4 gates (advisories/bans/licenses/sources). See the W13 cargo-deny entry in `09-CHANGELOG.md`. |
| `CHANGELOG.md` | ✅ exists | release-plz maintains this for the product. |

## What to promote (staged → `.github/workflows/`)

```bash
# From repo root, after reviewing each file:
mkdir -p .github/workflows
cp scripts/touring_premium_refactor_2026/staging/w13-github-workflows/release-plz.yml      .github/workflows/
cp scripts/touring_premium_refactor_2026/staging/w13-github-workflows/sigstore-release.yml  .github/workflows/
cp scripts/touring_premium_refactor_2026/staging/w13-github-workflows/docs-rs-mirror.yml    .github/workflows/
# CI gate (cargo-deny/audit/test) lives in docs/ci-template.yml — promote as ci.yml:
cp docs/ci-template.yml .github/workflows/ci.yml
```

## Secrets required: NONE beyond the auto-provided `GITHUB_TOKEN`

- **release-plz**: `publish = false` → no `CARGO_REGISTRY_TOKEN`, no crates.io account. Only `GITHUB_TOKEN` (auto).
- **sigstore (cosign)**: **keyless** OIDC — the GitHub Actions workflow identity *is* the signing identity (`id-token: write`). No personal sigstore account, no key management. This is what unblocked the W13.5 "HIGH external deps" note: keyless needs no credentials.

## Release flow

```
push to main
   └─► release-plz.yml (release-pr job)  → opens "release PR": bump version + update CHANGELOG.md
merge the release PR
   └─► release-plz.yml (release job)     → creates tag vX.Y.Z + GitHub release
release published
   ├─► build-release / distro-matrix.yml → cross-compile 4 targets → upload *-<target>.tar.gz   [W14]
   └─► sigstore-release.yml              → SHA256SUMS + cosign keyless sign + verify + CycloneDX SBOM
```

## Dependency note (build artifacts)

`sigstore-release.yml` signs the `*-<target>.tar.gz` artifacts that a **build
job must upload first**. That builder is the W14 cross-compile matrix
(`scripts/touring_premium_refactor_2026/staging/w14-github-workflows/distro-matrix.yml`).
Promote it alongside, or wire a `build-release` job, before the first signed release.

## Verification done at authoring time (anti-theater)

- `cargo deny check` → **exit 0, all 4 gates GREEN** (real fixes: wasmtime CVE,
  fastembed→rustls, rustls-webpki/rand patches; 5 transitive advisories tracked).
- `cargo check --workspace` → exit 0 with the refreshed dependency graph.
- All 3 workflow YAMLs parse (`yaml.safe_load`); `release-plz.toml` + `deny.toml`
  parse (`tomllib`).

## Targets covered by the sign matrix

`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`,
`aarch64-apple-darwin` — matches `deny.toml [graph].targets` and the W14 distro matrix.
