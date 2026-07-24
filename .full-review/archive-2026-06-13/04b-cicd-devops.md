# Phase 4B: CI/CD & Release / DevOps Practices Review

> Touring workspace · 2026-06-13 · agent: deployment-engineer (4B)
> North star: release/ops posture worthy of a Premium, Elite-of-Market open-source Rust tool
> (daemon + CLI + installer). Read-only. All claims cite real `file:line`.
> Builds on Phase 2 (SEC-03 cargo-deny RED ungated; perf has no CI guard) and Phase 3
> (CI runs `--lib` only; never *runs* `--tests`; no coverage floor; fuzz never run; version 0.1.0).

---

## Verdict (one paragraph)

Touring has **two parallel CI/CD realities**. The *active* one (`.github/workflows/ci.yml` +
`release.yml`) is a thin, honest, but **sub-elite** pipeline: check + clippy, `--lib`-only tests,
4 Python dogfood gates, a 2-target release matrix. The *aspirational* one lives **outside**
`.github/` — a far more complete `scripts/ci/per-project-deployment.yml.template` (fmt, MSRV,
semver-checks, shellcheck, multi-OS), plus Dockerfiles, Homebrew/Scoop manifests, and an
`install.touring.dev` script — all **skeletons with placeholders, never activated**, gated on
masterplan deliverables (W13.5 sigstore, W13.6 release-plz, B-W4 distribution) that never landed.
The result is an elite *intent* with a sub-elite *enforced* surface. Compounding this: **the repo
has no `.git` directory at all** (`ls .git` → not found), so `on: push/pull_request` triggers
**cannot fire** — the entire pipeline is, today, **untested and unfireable**. For an elite *public*
repo this is the gating issue: every other CI improvement is theoretical until the repo is a real
git repo on GitHub with a first commit + a `v*` tag (masterplan B-W1, Gabriel-dependent).

**Severity counts (4B):** 2 Critical · 7 High · 8 Medium · 4 Low.

---

## The No-Git / CI-Reality Paradox (CRITICAL — the root finding)

**Evidence:**
- `ls -la .github/` → `.github/workflows/{ci.yml, release.yml}` exist.
- `ls .git` → **"Arquivo ou diretório inexistente"** (no git repo). Confirmed up-tree:
  `/home/gabrielgadea/.git` absent; `~/.claude/.git` absent. The workspace is managed
  *entirely without git* (REGRA #11 — Touring is the source of truth internally).
- `.gitignore` **does** exist (`.gitignore:1`, "N02 — Master Plan H0 / QA-01") — i.e. the repo
  is *prepared* for git, but not *under* git.
- `ci.yml:14-16` triggers on `push: [main, master]` + `pull_request`. `release.yml:18-21`
  triggers on `push: tags: ["v*"]`.

**The contradiction:** GitHub Actions `on: push/pull_request/tags` are **git-native events**.
With no git history, no remote, no commits, and no tags, **not a single workflow has ever run or
can run**. The CI is *authored* but *never exercised* — the YAML has never been validated against
a real Actions runner. `release.yml:13` even admits this: *"authored ahead of first public release;
first CI run may surface target-specific dep issues."*

**Why it matters for elite:** A Premium, Elite-of-Market OSS repo's single most load-bearing
trust signal is a **green CI badge on a real, reproducible pipeline**. README badges claiming
versions/counts (DOC-01/04) with a pipeline that has never executed is the inverse of elite — it is
*Potemkin CI*. An external contributor cannot reproduce a build state that has never existed.

| Severity | **🔴 CRITICAL — CICD-01** |
|---|---|
| Operational risk | CI/release pipelines are unfireable; YAML is unvalidated; no contributor-reproducible build; release tagging impossible. The masterplan (memory: B-W1 "falta Gabriel publicar repo+tag") confirms this is the live blocker. |
| Concrete fix | (1) Gabriel publishes the repo to GitHub with an initial commit (operator-only — TACO is git-prohibited). (2) Push a real `v*` tag matching the crate version (see CICD-04). (3) Let `ci.yml` run once; reconcile any failure (Phase 2/3 predict several: `--tests` hang, deny RED). (4) Document the internal no-git ↔ external-git boundary in CONTRIBUTING (DOC-07 / CICD-15). This finding is the **dependency root**: every gate below is inert until CICD-01 is resolved. |

---

## CI Completeness vs an Elite Rust Repo

### CI Gate Gap Matrix

Legend: **Present?** = step exists in `ci.yml` (the *active* workflow). **Binding?** = fails the
build on violation (not fail-open / not `|| true` / not `continue-on-error`).

| # | Elite-expected gate | Present in ci.yml? | Binding? | Evidence / gap |
|---|---|---|---|---|
| 1 | `cargo check --workspace --tests` | ✅ | ✅ | `ci.yml:36` |
| 2 | `cargo clippy -D warnings` | ✅ | ✅ | `ci.yml:38` — genuinely good |
| 3 | `cargo test` (unit) | ⚠️ partial | ✅ | `ci.yml:64` runs `--lib` **only**; integration/daemon tier skipped (T-02) |
| 4 | `cargo test --tests` (integration run) | ❌ | — | `ci.yml:36` *compiles* `--tests` but **never runs** them (T-10); `graph_service_e2e` hang is the stated reason |
| 5 | `cargo fmt --check` | ❌ | — | **Absent from ci.yml.** No `rustfmt.toml`/`.rustfmt.toml` exists. CONTRIBUTING claims `cargo fmt` style (`CONTRIBUTING.md` "Code Style") but no gate enforces it. Present in the *inactive* template (`per-project-deployment.yml.template:142`, W12-scoped only) |
| 6 | `cargo deny check` (advisories/bans/licenses) | ❌ | — | **SEC-03 RED & ungated.** `deny.toml` is a high-quality 23KB policy (advisories `:20`, bans `:43`, licenses v2 `:201`, sources locked `:246`) but **no CI step invokes it**. postgres-protocol RUSTSEC-2026-0179 (CVSS 8.7) + pyo3 are **not** in the ignore-list (`grep 0179 deny.toml` → not found) → a real `cargo deny check` would fail today |
| 7 | `cargo doc --no-deps -D warnings` | ❌ | — | Absent. DOC-06: `missing_docs` enforced on 8 small crates but not touring-server/-intelligence (1,756 pub items) |
| 8 | Coverage floor (`cargo llvm-cov --fail-under`) | ❌ | — | T-04: no coverage measured/gated; no `codecov.yml`/`.codecov.yml` |
| 9 | Fuzz smoke (`cargo +nightly fuzz run -- -runs=N`) | ❌ | — | T-05: 8 targets in `fuzz/fuzz_targets/` (found 5 real bugs, W11.6) **never run in CI** |
| 10 | MSRV verification (`cargo-msrv`) | ❌ | ⚠️ | Absent from ci.yml. Template has it (`:216`) but `continue-on-error: true` (`:245`) = advisory. MSRV drift documented: workspace `rust-version=1.80` (`Cargo.toml:145`) vs touring-foundation override `1.75` |
| 11 | Multi-platform matrix in CI | ❌ | — | `ci.yml` is `ubuntu-latest` single-OS (`:27`); `release.yml:34-42` has Linux+macOS but only at *release* time. macOS/Windows compile breakage caught only at tag push |
| 12 | Security scanning (SAST / supply-chain) | ❌ | — | No CodeQL, no `cargo-deny` advisories, no dependency-review action, no Dependabot/Renovate config |
| 13 | Perf P99 regression guard (Phase 3 T-03) | ❌ | — | `latency_p99_guard.rs` covers ast-parse only; `ceg_baseline.rs` is a *bench* never in CI. The p99=488ms/p999=1.3s hook tail (Phase 2) has **zero** CI guard |
| 14 | Integration tests w/ timeout harness (T-02) | ❌ | — | `.config/nextest.toml` **already configures** `slow-timeout terminate-after` (`:18,:50,:54`) and a `ci` profile with JUnit (`:23-32`) — but **CI uses `cargo test`, not `cargo nextest`**, so the timeout harness is dead config |
| 15 | sync_metrics (doc anti-drift) | ✅ | ✅ | `ci.yml:73` — but **under-checks** (DOC-02: first-`\d+ crates`-match only; body drift invisible) |
| 16 | file_size_gate (anti-bloat) | ✅ | ✅ | `ci.yml:75` — good |
| 17 | gen_reference (reference anti-drift) | ✅ | ✅ | `ci.yml:77` — but **under-checks** (DOC-04: string-literal extraction, misses `#[tool]` macros) |
| 18 | wiring_integrity (cycle gate) | ⚠️ | ❌ | `ci.yml:79` ends `|| true` → **fail-open theater**; a reintroduced cycle would not fail the build |
| 19 | health-delta (regression streak) | ⚠️ | ❌ | `ci.yml:81-86` explicitly "advisory only", "non-blocking", needs the `touring` binary absent on hosted runners → **always a no-op in GitHub-hosted CI** |
| 20 | root hygiene (no backup artifacts) | ✅ | ✅ | `ci.yml:87-96` — good, cheap |
| 21 | Concurrency cancel-in-progress | ❌ | — | No `concurrency:` block; redundant runs on rapid pushes will pile up |
| 22 | Pinned actions (SHA, not float tags) | ❌ | — | All actions use float tags (`@v4`, `@stable`) — supply-chain risk for a security tool (CICD-13) |

**Score: 6/22 elite gates present-and-binding** (clippy, check, file_size, gen_reference*, sync_metrics*, root-hygiene; * = present but under-checking). The dogfood gates are a genuinely *novel, good* idea — the elite pattern is invented here — but they (a) under-check (Phase 3) and (b) two of them are fail-open (`|| true`, advisory).

### The missing CI steps as concrete YAML

Add a `fmt-deny-doc` job (cheap, fast, closes gaps 5/6/7/12) and a `coverage` job (gap 8):

```yaml
  fmt-deny-doc:
    name: fmt + deny + doc
    runs-on: ubuntu-latest
    timeout-minutes: 30
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { components: rustfmt }
      - uses: Swatinem/rust-cache@v2
      - name: cargo fmt --check                       # gap 5
        run: cargo fmt --all -- --check
      - name: cargo-deny (advisories + bans + licenses + sources)   # gap 6 / SEC-03
        uses: EmbarkStudios/cargo-deny-action@v2
        with: { command: check }
      - name: cargo doc -D warnings                   # gap 7
        env: { RUSTDOCFLAGS: "-D warnings" }
        run: cargo doc --workspace --no-deps

  coverage:
    name: coverage floor
    runs-on: ubuntu-latest
    timeout-minutes: 60
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: taiki-e/install-action@cargo-llvm-cov
      - uses: Swatinem/rust-cache@v2
      - name: cargo llvm-cov (fail under floor)        # gap 8 / T-04
        run: cargo llvm-cov --workspace --lib --fail-under-lines 75 --lcov --output-path lcov.info
      - uses: codecov/codecov-action@v4
        with: { files: lcov.info, fail_ci_if_error: true }

  nextest:
    name: integration (nextest, timeout-harnessed)     # gap 4/14 / T-02
    runs-on: ubuntu-latest
    timeout-minutes: 60
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: taiki-e/install-action@nextest
      - uses: Swatinem/rust-cache@v2
      - name: cargo nextest (ci profile uses .config/nextest.toml slow-timeout)
        run: cargo nextest run --workspace --profile ci -E 'not test(graph_service_e2e)'
      - uses: actions/upload-artifact@v4
        if: always()
        with: { name: junit, path: target/nextest/ci/junit.xml }
```

Add fuzz smoke (gap 9) and a perf guard (gap 13) as their own jobs:

```yaml
  fuzz-smoke:
    name: fuzz smoke (60s/target)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@nightly
      - run: cargo install cargo-fuzz --locked
      - name: smoke each target (regression on the 5 W11.6 bugs)
        run: |
          cd fuzz
          for t in $(cargo fuzz list); do
            cargo +nightly fuzz run "$t" -- -runs=10000 -max_total_time=60
          done
```

Add `concurrency` + pin actions to SHAs (gaps 21/22) at the top of `ci.yml`:

```yaml
concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true
```

---

## Release Engineering Assessment

### What's solid in `release.yml`
- Real matrix: Linux x86_64-musl (static) + macOS aarch64 (`release.yml:34-42`).
- `fail-fast: false` so the Linux artifact ships even if macOS breaks (`:34`) — pragmatic.
- Strip + tar + **SHA256 per archive** (`release.yml:63-74`).
- A native-arch smoke test (`touring --help`) before upload (`release.yml:76-83`).
- `if-no-files-found: error` (`:91`) — won't silently ship nothing.
- `install.sh` **verifies SHA256 and refuses on mismatch** (`install.sh:53-61`) — a real,
  not cosmetic, integrity check. Detects platform, supports `TOURING_VERSION` pin, smoke-tests.

### Release findings

| Severity | Finding | Evidence | Operational risk | Fix |
|---|---|---|---|---|
| **🔴 CRITICAL CICD-04** | **Version mismatch: `v*` tags on a `0.1.0`, `publish=false` crate.** | `Cargo.toml:142,145` `version="0.1.0"`, `publish=false`; binary prints `0.1.0` (DOC-01); `release.yml:20` tags `v*`; install.sh default `v31.0.0` (`install.sh:6`); homebrew/scoop hardcode `0.30.0`. **Five different versions across the release surface.** | A `v30.0.0` tag producing a binary that says `0.1.0` makes Homebrew's `assert_match version` test (`touring.rb:61`) fail, breaks `install.sh` version pinning, and destroys every version-derived trust signal. `publish=false` also means `cargo install touring` (B-W4) is **impossible** — a headline distribution channel is structurally blocked. | Set the real version in `[workspace.package].version` (e.g. `30.3.6`), flip `publish=false`→ remove for the published crate(s), tag `v30.3.6`. Make `touring --version` the single source of truth all docs/manifests derive from. |
| **🔴/High CICD-05** | **Three-way repo identity contradiction.** | `anthropics/touring` (Dockerfile.distroless:48 LABEL source, scoop checkver, homebrew tap `touring.rb:7`) vs `gabrielgadea/touring` (install.sh:14 `TOURING_REPO`) vs `releases.touring.dev` / `touring.dev` domain (homebrew `:22-38`, scoop URLs, install.touring.dev). | An installer pointing at `gabrielgadea/touring` while Docker/Homebrew/Scoop point at `anthropics/touring` and a `touring.dev` domain that may not exist = every published install path resolves to a different (possibly nonexistent) location. Day-0 installability is broken across all 4 channels. | Pick ONE canonical repo + ONE release host. Parametrize all manifests from it. Until decided, this blocks B-W4. |
| **High CICD-06** | **Distribution-target triple mismatch.** | `release.yml:38` builds `x86_64-unknown-linux-musl`; Homebrew expects `x86_64/aarch64-unknown-linux-gnu` (`touring.rb:33,37`); Scoop expects `*-pc-windows-msvc` (`scoop/touring.json`) the release matrix **never builds**; Homebrew expects `aarch64-apple-darwin` (release builds it ✅) AND `x86_64-apple-darwin` (release does **not**). | Homebrew on Linux would 404 (gnu vs musl); Scoop on Windows would 404 (no Windows target); Intel macOS would 404. 3 of ~5 advertised platforms are undeliverable. | Align the release matrix to every advertised target, or trim the manifests to what's actually built. Add `x86_64-apple-darwin`, Windows MSVC, and aarch64-linux if Homebrew/Scoop are to stay. |
| **High CICD-07** | **No supply-chain provenance: no SBOM, no signing, no SLSA, no attestation.** | `grep cosign\|sbom\|sigstore\|slsa\|provenance\|attest .github/` → **zero hits**. Dockerfile comments admit signing is deferred ("W13.5 sigstore signing"). SHA256 exists but is **unsigned** — an attacker who controls the release can rewrite both archive and `.sha256`. | For a tool that **executes code on behalf of agents** (its own SECURITY.md), shipping unsigned, unattested binaries via `curl | sh` is the highest-leverage supply-chain gap. SHA256 without a signature proves integrity-vs-corruption, not integrity-vs-tampering. | Add `cosign sign-blob` / sigstore keyless on the archives; emit an SBOM (`cargo-cyclonedx` or `syft`); add `actions/attest-build-provenance` (SLSA L3). Sign the distroless image manifest. |
| **High CICD-08** | **`curl | sh` installer with no signature verification and a TOFU trust model.** | `install.sh:47` `curl ... | fail`; verifies SHA256 (`:53-61`) but the checksum is fetched from the **same origin** as the archive (`install.sh:49`) → no out-of-band trust anchor. README install one-liner (`install.sh:5`) is `curl | sh`. | Same-origin checksum gives no protection against a compromised release host. | Pair with CICD-07 cosign; document `--proto '=https' --tlsv1.2` (the `install.touring.dev.sh` already pins TLS — promote that to the canonical installer). |
| **Med CICD-09** | **Docker / Homebrew / Scoop are skeletons, never built or tested in CI.** | `grep docker .github/` → none. Dockerfiles (`scripts/docker/Dockerfile.{alpine,distroless}`) carry "SKELETON — not yet wired into CI" (`:13`); homebrew/scoop have `PLACEHOLDER_REPLACED_BY_RELEASE_PIPELINE` hashes. | The distroless image (a genuinely elite choice — no shell, ~80MiB) is never built, so it may not even compile; the release-plz pipeline that fills the placeholders doesn't exist. | Add a `docker build` job to CI (build-only, no push, before publish); wire release-plz or a `update-manifests` job to inject real version+sha. |
| **Med CICD-10** | **Binary-target footgun: `--bins` over `-p touring-hooks -p touring-server`.** | `release.yml:60-61` builds `-p touring-server -p touring-hooks --bins`. But 7 `[[bin]]` exist across the workspace, including a **duplicate bin name** `touring-web-server` declared in BOTH `touring-web-server` and `touring-bindings` (`grep '[[bin]]'`). | `--bins` scoped to 2 packages happens to produce the right 3 (touring/touring-hook/touring-daemon), but the duplicate bin name is a latent `cargo` ambiguity error waiting for a scope change. | Build explicit `--bin touring --bin touring-daemon --bin touring-hook`; resolve the duplicate `touring-web-server` bin name. |
| **Low CICD-11** | **`generate_release_notes: true` only** — no curated CHANGELOG in the release. | `release.yml:108`. A real `CHANGELOG.md` exists (auto-synth from 102 TOON checkpoints) but isn't attached. | Auto notes from commit messages on a repo with no git history (CICD-01) will be empty/meaningless. | Feed `CHANGELOG.md` section into `action-gh-release` `body_path`. |

---

## Observability & Ops (long-lived daemon production-readiness)

### What's solid (don't regress)
- **Graceful shutdown is real.** `daemon_main.rs:6-9`: SIGINT/SIGTERM handled via `tokio::signal`,
  calling async `graceful_shutdown()` to flush WAL + LinUCB + CRDT *before* exit — and the doc note
  records that a prior C-level handler that bypassed this "risked data loss" (a fixed regression).
- **Health endpoint:** `touring doctor -j` (5-component health) + `touring status -j`
  (composite_health_score). **Metrics:** `touring gate-metrics -j` (hdrhistogram percentiles).
- **Crash log path** configurable (`TOURING_CRASH_LOG_PATH`); panic isolation two-layer (Phase 2).
- **Idle watchdog** exists and is opt-in (`TOURING_IDLE_TIMEOUT_SECS`, default off on workstations).

### Ops findings

| Severity | Finding | Evidence | Risk | Fix |
|---|---|---|---|---|
| **High CICD-12** | **53 `eprintln!` bypass structured tracing on the daemon plane.** | `grep eprintln! crates/*/src` → 53 non-test sites (Phase 0 scope cited 244 incl. tests). | A long-lived daemon writing to raw stderr can't be filtered by level, structured-logged, or shipped to an aggregator; mixes with hook stderr (which is parsed). Phase 2 also noted `tracing::debug!` for real errors (silent unless `RUST_LOG=debug`). | Route all daemon diagnostics through `tracing` with proper levels; reserve `eprintln!` for pre-runtime bootstrap only. |
| **Med CICD-13** | **No runbook / SLO / SLI / on-call docs.** | `ls docs/` → no runbook/ops/slo/incident files; SECURITY.md + SUPPORT.md + CONTRIBUTING exist but no operational runbook. | An elite daemon ships a "how to operate, what's normal, what to page on" doc. p99=488ms is a known anomaly with no documented SLO to measure against. | Add `docs/ops/runbook.md` (start/stop/restart via `daemon-ctl`, socket model, recovery), define an SLO (e.g. p99 hook dispatch < 50ms) and wire CICD-13's perf guard to it. |
| **Med CICD-14** | **Unix socket in `/tmp` without explicit `0o600` (echoes SEC-07).** | `/tmp/touring-daemon-1000.sock` (CLAUDE.md topology); Phase 2 SEC-07. | Multi-user host: another local user could connect to the RPC socket. | `chmod 0o600` on bind; document in runbook. |
| **Low CICD-15** | **Config surface is enormous and undocumented: 118 distinct `TOURING_*` env vars.** | `grep -hoE 'TOURING_[A-Z_]+' crates/*/src \| sort -u \| wc -l` → **118**. | No single reference for the config surface; reproducible dev/prod setup is hard; secret-bearing ones (SEC-04) hide in the noise. | Generate a `docs/reference/env-vars.md` from the source (dogfood pattern); separate runtime config from secrets. |

---

## Environment, Config & Secret Management

- **SEC-04 (credentials in sandbox)** is a CI/ops concern too: the sandbox forwards
  `GITHUB_TOKEN/AWS_*/ANTHROPIC_API_KEY` into children (Phase 2), and SECURITY.md's hardening
  note (`SECURITY.md:30-31`) claims they're "never in `ENV_ALLOWLIST`" — true of that constant,
  false of the separate `CREDENTIAL_ENV_WHITELIST`. **Doc ↔ code drift on a security claim.**
- No `Dependabot`/`Renovate` config → 1,558-package lockfile (Phase 0) drifts with no automated
  bump PRs. (Compounds CICD-01: no git → no PRs anyway.)
- Reproducible dev setup: **no `rust-toolchain.toml`** → contributors get whatever `stable` is on
  the day; MSRV 1.80 declared but unpinned for builds. Add a pinned `rust-toolchain.toml`.

---

## Gate Enforcement Reality — fail-open theater audit

Phase 3 flagged the dogfood gates as under-checking. Examining *binding* behavior:

| Gate | ci.yml line | Binding verdict |
|---|---|---|
| sync_metrics | `:73` | **Binding** but under-checks (first-match-only, DOC-02) |
| file_size_gate | `:75` | **Binding**, sound |
| gen_reference | `:77` | **Binding** but under-checks (string-literal, DOC-04) |
| wiring_integrity | `:79` | **FAIL-OPEN** — `|| true` swallows all failures → theater |
| health-delta | `:81-86` | **FAIL-OPEN** — "advisory only", no-op without the binary on hosted runners |
| missing-docs (generator) | `:42-51` | **Binding**, narrow (1 crate) |
| clippy / check | `:36,:38` | **Binding**, genuinely good |

So of the "quality gates" job, **2 of 5 (wiring, health-delta) are non-binding by construction.**
This is the elite anti-pattern: a gate that *looks* enforced but can never fail. The template's
MSRV + semver-checks (`per-project-deployment.yml.template:245,287`) are *also* `continue-on-error:
true` — advisory ramp-ups that, if ever activated, would still not block. **The pattern of
"add the gate, then defang it" repeats across both the active and inactive workflows.**

---

## #1 CI/CD Lever Toward Elite

**Resolve CICD-01 first (Gabriel-only): make the repo a real git repo on GitHub with an initial
commit and a `v<real-version>` tag.** Until then, every gate is inert and unvalidated — there is no
elite CI without a CI that has *run*. It is the dependency root of B-W1 and of this entire phase.

**Then, the highest-ROI engineering lever is a single `fmt-deny-doc` job + flipping the two
fail-open gates to binding.** One ~30-line job closes the most dangerous *enforced-able* gaps at
once: `cargo fmt --check` (gap 5), **`cargo deny check` (SEC-03 RED — the live CVSS-8.7 advisory)**
(gap 6), and `cargo doc -D warnings` (gap 7). Combined with removing `|| true` from
`wiring_integrity` and gating `nextest` (the timeout config already exists in `.config/nextest.toml`),
this converts Touring's *aspirational* elite pipeline into a *binding* one — turning the dogfood
philosophy ("every claimed invariant → a gate that fails the build") from slogan into mechanism.
The infrastructure (deny.toml, nextest.toml, fuzz targets, Dockerfiles) is **already written and
high-quality** — the gap is purely *activation and binding*, not authorship.

---

## Cross-references for Phase 5

- **CICD-01 (no-git) blocks B-W1** (masterplan, memory: "falta Gabriel publicar repo+tag").
- **CICD-04 (version 0.1.0/publish=false)** is the same root as DOC-01 and blocks `cargo install` (B-W4).
- **CICD-06/05** block B-W4 distribution (Homebrew/Scoop/Docker/cargo).
- **Gap 6 (cargo-deny)** is the CI half of Phase 2 SEC-03; **Gap 13 (perf guard)** is the CI half of T-03; **Gap 8/4 (coverage/nextest)** are the CI half of T-04/T-02.
- The **fail-open gates** (CICD / gate-enforcement audit) corroborate Phase 3's "gates exist but under-check / present-but-unrun" cross-cutting signal.
