# Phase 2A: Security Audit — Touring Workspace

> Touring workspace · 2026-06-13 · DEFENSIVE security review (read-only, no mutation)
> Agent: comprehensive-review:security-auditor · Rust lens · threat model: AI code-execution daemon
> Evidence: real `file:line` + live `cargo audit` / `cargo deny` output. No invented findings.

## Executive verdict

**The Code Execution Gateway is genuinely strong — the rest of the surface is not gated by it.**

The CEG's capability model (deny-by-default, deny-wins), its X8 landlock supervision (kernel-enforced filesystem + network + IPC, fail-closed), and the daemon's two-layer panic isolation are **elite-grade and provable** (real E2E tests prove writes outside granted roots are kernel-blocked). But the CEG only governs `touring exec --real-exec` and the pre-bash observe hook. The **MCP tool surface (~169 tools, always on) bypasses the CEG entirely**: `touring_file_ops` is an unrestricted arbitrary-FS read/write/delete primitive, and `touring_ctx_execute` runs code with the forbidden-call scanner defaulting to *advisory-only*. Two documentation claims are **false** (SECURITY.md credential-env claim; the CEG "X5 deferred" comment contradicting the shipped landlock). The supply-chain gate (`cargo deny check advisories`) is **currently RED** (6 vulns, incl. one CVSS 8.7).

### Severity counts

| Severity | Count | IDs |
|---|---|---|
| **Critical** | 1 | SEC-01 (`touring_file_ops` arbitrary FS via MCP, no CEG) |
| **High** | 5 | SEC-02 (ctx_execute fail-open), SEC-03 (cargo-deny RED / CVSS 8.7), SEC-04 (credential env contradiction), SEC-05 (transcript-miner secret persistence), SEC-06 (`unsafe impl Send` HookRuntime) |
| **Medium** | 5 | SEC-07 (socket perms), SEC-08 (arbitrary file-read tools), SEC-09 (SSRF fetch_remote_wasm), SEC-10 (doc/landlock contradiction), SEC-11 (unbounded local file reads) |
| **Low** | 3 | SEC-12 (assist.rs guarded unwrap), SEC-13 (arg-injection touring_tdg), SEC-14 (transcript line size cap) |

### What is already elite (state it honestly)

- **Capability model** (`capability/profile.rs`, `builtins.rs`) — deny-by-default holds; empty profile denies all (`profile.rs:135`); deny-wins over allow (`profile.rs:108-117`); `Prompt` fails closed in the non-interactive daemon (`profile.rs:29-33`). Verified by 11 tests including `empty_profile_denies_by_default`, `deny_wins_over_allow`.
- **X8 landlock supervision** (`gateway/supervised.rs`) — real `landlock` LSM with FS + network (V4) + IPC scope (V6), built parent-side, enforced post-fork in `pre_exec`, **fail-closed** (`supervised.rs:373-385`: if not `KernelEnforced`, the spawn fails). **Real E2E proofs** that a write outside granted roots is kernel-blocked (`supervised.rs:627-646`), a read-only root is not writable (`:650-663`), and TCP bind is kernel-denied on Linux 6.7+ (`:716-755`). Loud degradation on old kernels, never silent (`enforce_linux.rs:266-272`).
- **Deny-wins decision** (`gateway/decision.rs:373`) — a hard static-block or a denied capability forces `Verdict::Deny` regardless of the composite score. A high score never overrides a hard gate.
- **Daemon panic isolation** — two independent layers: per-connection `tokio::spawn` absorbs panics (`daemon.rs:772`, comment `:793-796`), and real handlers are additionally `catch_unwind`-guarded on the actor thread (`daemon.rs:242`). One malformed request is **not** fatal to the singleton.
- **rkyv on untrusted bytes is validated** — the socket paths use `check_archived_root` / checked `from_bytes` (`daemon.rs:1003`, `:1079`), the on-disk cache uses `check_archived_root` (`dependency_cache.rs:313`); no unchecked `archived_root`/`access_unchecked` on attacker-tamperable data. Frame caps 16 MiB / 2 MiB.
- **No hardcoded secrets** found in source (only test fixtures flagging secrets as an anti-pattern).

---

## CRITICAL

### SEC-01 [Critical · CWE-22 / CWE-668] `touring_file_ops` — unrestricted arbitrary filesystem read/write/delete over MCP, bypassing the CEG

**`crates/touring-server/src/server/tools_core.rs:1050-1370`** (router_core — **always merged**, `server/mod.rs:428`). Params are raw, unvalidated `String` (`server/params.rs:662-685`).

The tool operates on the caller's raw `path`/`dest` with **no `canonicalize`, no workspace-root containment, no symlink rejection, and no CEG capability check**:

| Op | line | effect on attacker path |
|---|---|---|
| `read` | `tools_core.rs:1070` | `tokio::fs::read_to_string(path)` — read **any** file: `/etc/shadow`, `~/.ssh/id_rsa`, `~/.aws/credentials` |
| `write` | `:1084` | `tokio::fs::write(path,…)` — overwrite **any** writable file (`~/.bashrc`, `~/.ssh/authorized_keys`) |
| `append` | `:1099-1105` | append (create=true) to any file |
| `delete` | `:1116` | `remove_file` any file |
| `delete_dir` (force) | `:1340` | `remove_dir_all` — **recursive deletion of arbitrary trees** |
| `copy`/`move` | `:1209`/`:1230` | exfiltrate/relocate to arbitrary dest (parent auto-created `:1205`) |

**Attack scenario.** The MCP transport is stdio with no auth (`main.rs:76` — standard, trust boundary is the calling LLM/client). A prompt-injection that reaches the model (e.g. via an indexed file, a fetched page, or a malicious repo README) can emit a `touring_file_ops` call: `{"operation":"read","path":"/home/user/.ssh/id_rsa"}` to exfiltrate the private key into the conversation, or `{"operation":"write","path":"/home/user/.ssh/authorized_keys","content":"ssh-ed25519 AAAA… attacker"}` for persistence, or `{"operation":"delete_dir","path":"/home/user","force":true}` for destruction. None of this passes through the CEG — the gateway's landlock, capability deny-by-default, and `Sandboxed` profile govern only `touring exec`, never the MCP file tool.

**Remediation.** Route every file-system MCP tool through a canonicalize + root-containment guard, ideally the existing capability model:
```rust
fn guard_path(raw: &str, op: FsOp, ws_root: &Path) -> Result<PathBuf, McpError> {
    let p = std::path::Path::new(raw);
    // Reject before touching the fs; resolve symlinks for read, parent for write.
    let canon = if op.is_read() {
        std::fs::canonicalize(p).map_err(invalid)?
    } else {
        let parent = p.parent().ok_or_else(|| invalid("no parent"))?;
        std::fs::canonicalize(parent).map_err(invalid)?.join(p.file_name().unwrap())
    };
    if !canon.starts_with(ws_root) {
        return Err(McpError::invalid_params(
            format!("path escapes workspace root: {}", canon.display()), None));
    }
    Ok(canon)
}
```
Apply to `read/write/append/delete/delete_dir/copy/move/mkdir`. Then resolve the requested op against a `CapabilityProfile` (e.g. `builtins::sandboxed(ws_root)` for `read`, `staged_write` for `write`) so the MCP tools inherit the same deny-by-default contract the CEG already enforces. Until then, the single most leveraged daemon tool is an unrestricted FS primitive handed to an untrusted prompt.

---

## HIGH

### SEC-02 [High · CWE-94 / CWE-693] `touring_ctx_execute` — forbidden-call scan defaults to advisory (executes anyway); fail-open on scanner panic

**`crates/touring-server/src/tools/ctx_execute_tools.rs`** (router_ctx_execute — **always merged**, `mod.rs:429`). Executes arbitrary code in 11 languages incl. `bash`/`sh`.

- Policy default is **`Warn`**: when forbidden calls are detected, the code **still runs** and the finding is merely appended to stderr (`ctx_execute_tools.rs:170-183`, warn-merge at `:245-248`). A hard block only happens under `TOURING_CEG_FORBIDDEN_ENFORCE=1` (`:37`).
- The caller can self-exempt with `allow_forbidden: true` (`:180`).
- The scanner is **fail-open on panic**: `catch_unwind(|| ast_forbidden_scan(...)).unwrap_or_else(|| { warn!("…fail-open"); vec![] })` (`:144-148`) — a panic in the detector means *no detection*, execution proceeds.

The real isolation does route through `execute_in_sandbox` (`:226`), which applies the sandbox (rlimit/landlock-class confinement) — so this is **defense-in-degraded, not undefended**. But the *static* gate that should catch `os.system`, `subprocess`, `eval`, `socket` etc. is advisory-by-default and fails open. For an elite posture the static gate must fail **closed**.

**Remediation.** Default `ForbiddenCallPolicy` to `Block` (env opt-*out* `TOURING_CEG_FORBIDDEN_OFF=1` already exists at `:34`); on scanner panic, treat as **detected/blocked**, not empty:
```rust
std::panic::catch_unwind(|| ast_forbidden_scan(lang, code))
    .unwrap_or_else(|_| { tracing::error!("scanner panicked — failing closed");
                          vec!["__scanner_panic_fail_closed__".into()] })
```
Gate `allow_forbidden: true` behind a Trusted-profile assertion, not a free caller flag.

### SEC-03 [High · CWE-1395 / CWE-937] Supply chain: `cargo deny check advisories` is RED — 6 vulnerabilities incl. one CVSS 8.7

Live `cargo audit` (1,558 deps) and `cargo deny check advisories` both **fail**. The `deny.toml` `[advisories].ignore` list (deny.toml:26-37) predates the newest advisories, so the gate is currently red:

| Crate | Version | RUSTSEC | CVSS / class | Fix |
|---|---|---|---|---|
| **postgres-protocol** | 0.6.11 | RUSTSEC-2026-0179 | **8.7 High** — unbounded SCRAM iteration → CPU-exhaustion DoS | ≥0.6.12 |
| postgres-protocol | 0.6.11 | RUSTSEC-2026-0180 | 6.9 Med — panic decoding malformed `hstore` (DoS) | ≥0.6.12 |
| tokio-postgres | 0.7.17 | RUSTSEC-2026-0178 | 6.9 Med — panic on short `DataRow` (DoS) | ≥0.7.18 |
| pyo3 | 0.24.2 | RUSTSEC-2026-0176 | OOB read in `PyList`/`PyTuple` `nth` | ≥0.29.0 |
| pyo3 | 0.24.2 | RUSTSEC-2026-0177 | missing `Sync` bound on `new_closure` (unsound) | ≥0.29.0 |
| gix-date | 0.9.4 | RUSTSEC-2025-0140 | non-utf8 String (build-time only; *ignored* in deny.toml:36) | ≥0.12.0 |

Plus **10 warnings** (unmaintained/unsound): `atty` 0.2.14 (RUSTSEC-2024-0375 unmaintained + RUSTSEC-2021-0145 unaligned-read unsound — **not** in the ignore list), `bincode` 1.3.3, `instant`, `paste`, `proc-macro-error2` (RUSTSEC-2026-0173), `rustls-pemfile` (RUSTSEC-2025-0134), `rand` (RUSTSEC-2026-0097), `lru` (RUSTSEC-2026-0002).

**Risk.** The postgres advisories matter because `touring-bindings` ships a `postgis` module (`bindings/src/postgis/mod.rs`) — a malicious or compromised Postgres endpoint can DoS the client (SCRAM CPU-exhaustion, panic on malformed rows). pyo3 0.24 is the Python FFI boundary (PyO3 also flagged separately as RUSTSEC; soundness + OOB read).

**Remediation.** Bump `postgres-protocol`/`tokio-postgres` (point releases, low risk — do first), bump `pyo3` 0.24→0.29 (also closes the separate pyo3 advisory line; API migration needed), replace `atty`→`std::io::IsTerminal`. Add the unfixable-now IDs to `deny.toml` *with a tracking issue and a removal date*, and **wire `cargo deny check` into CI as a required gate** (verify `.github/workflows/ci.yml` runs it — if not, that's the gap that let this go red).

### SEC-04 [High · CWE-522 / CWE-200] Credential-env contradiction: the runtime sandbox passes ALL cloud credentials into the child; SECURITY.md claims the opposite

There are **two different env models** and they disagree:

1. **Capability model** `ENV_ALLOWLIST` (`capability/builtins.rs:17`) = `PATH HOME USER LANG LC_ALL TERM TZ` — credential-free. SECURITY.md:31-32 and the CEG docs cite *this* one ("credential env vars … are never in `ENV_ALLOWLIST`"). ✓ true for the capability model.
2. **The actual runtime sandbox** `apply_credential_whitelist` (`gateway/sandbox_executor.rs:542-593`) — the function that builds the *real* subprocess env via `cmd.env_clear()` then re-injects **`GITHUB_TOKEN`, `GH_TOKEN`, `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_SESSION_TOKEN`, `GOOGLE_APPLICATION_CREDENTIALS`, `KUBECONFIG`, `NPM_TOKEN`, `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`** into the sandboxed child. This is what `run_supervised` actually calls (`supervised.rs:347`).

So a sandboxed/supervised run **does** receive the full cloud-credential set, directly contradicting the security policy's claim. The intent (let `gh`/`aws`/`kubectl` authenticate) is legitimate, but: (a) it is **on by default** for every supervised run, not opt-in; (b) it is extensible by *any* env via `TOURING_SANDBOX_EXTRA_WHITELIST` (`:582`); (c) SECURITY.md actively misleads readers about it. Combined with SEC-02 (ctx_execute warn-default) and the fact that landlock V4 only blocks net on kernel ≥6.7, code that gets to run can read these credentials from its environment and, on an older kernel or via an allowed channel, exfiltrate them.

**Remediation.** (1) Make credential pass-through **opt-in per invocation** (a `--with-credentials` flag / Trusted-profile-only), not the default for `Sandboxed`. (2) Correct SECURITY.md to describe the *real* runtime model and the `CREDENTIAL_ENV_WHITELIST`. (3) Unify the two env models so the documented `ENV_ALLOWLIST` is the source of truth and credential injection is an explicit, audited capability grant.

### SEC-05 [High · CWE-532 / CWE-312] Transcript miner persists raw tool errors + resolution commands to the searchable memory store with no redaction

**`crates/touring-server/src/ingest/transcript_miner.rs`** (background sweep, opt-in `TOURING_TRANSCRIPT_MINER`). It mines `~/.claude/projects/*.jsonl`, extracts `error_text` (truncated `tool_result` content, `:458-461`) and `resolution_input` (the **full tool-call input JSON** — e.g. the bash command that fixed the error, `:462`), and persists both verbatim into `MemoryStore` (`:763-773`, `store.store(entry)` at `:773`) keyed `outcome:…:transcript-<hash>`. **No `redact_secrets` / sanitize is applied** — note `redact_secrets()` *exists* (`sandbox_executor.rs:599`) but is not called here.

**Attack/leak scenario.** If a transcript line contains a secret in a tool error or in the resolution command — e.g. a bash command `export AWS_SECRET_ACCESS_KEY=… && terraform apply`, or a `tool_result` echoing a token in an error — that secret is copied into the local memory DB and becomes retrievable by `touring memory recall`, injected into future hook context, and surfaced to the model. Secrets that were transient in one session become persistent and searchable.

**Remediation.** Apply `redact_secrets()` (extended with the standard token regexes — `gh[ps]_…`, `sk-…`, `AKIA…`, `xox[bap]-…`, PEM headers) to `error_text` and a stringified `resolution_input` *before* `store.store(entry)`. Add a test fixture with a planted token proving it's redacted before persistence.

### SEC-06 [High · CWE-662] `unsafe impl Send for HookRuntime` over 10+ `RefCell` fields, no SAFETY justification

**`crates/touring-hook-runtime/src/hook_runtime.rs:695`** — `unsafe impl Send for HookRuntime {}` with **no `// SAFETY:` comment**. The type holds ≥10 `RefCell<…>` fields (`ann_recall` :293, `stable_session` :327, `session_bus` :333, `span` :354, `pensieve` :410, `learning_loop` :417, `heat_map` :422, `last_edited_file` :589, `entity_registry` :593, `triad_state` :663). `RefCell` is `!Sync` by design; its runtime borrow checking is not atomic, so two threads borrowing concurrently is a data race → UB (not just a panic).

**Why it is not (yet) actively exploited.** The daemon replaced the legacy `Arc<Mutex<HookRuntime>>` with a single-owner mpsc-actor model (`daemon.rs:126`, `run_project_actor` `:207`) — the runtime is owned by one actor thread and commands are serialized through a channel, so in the current call graph it is effectively single-threaded. The `unsafe impl Send` is what lets it cross the `thread::spawn` boundary into that actor. The soundness hole is **latent**: any future code that clones a handle into a second thread, or any `&HookRuntime` shared across the tokio worker pool, reintroduces the data race silently — and there is no compiler check, because the `unsafe impl` overrides it.

**Remediation.** Either (a) document the invariant rigorously (`// SAFETY: HookRuntime is owned by exactly one actor thread; never shared by reference across threads; the mpsc protocol serializes all access`) **and** add a `debug_assert` / newtype that enforces single-ownership, or (b) the robust fix — replace the `RefCell` fields with `Mutex`/`parking_lot::Mutex` (or `tokio::sync::Mutex` for the async-held ones) and **delete the `unsafe impl Send`**, letting the auto-trait derive correctly. Option (b) removes a whole class of future-regression UB.

---

## MEDIUM

### SEC-07 [Med · CWE-276 / CWE-377] Daemon Unix socket created in world-traversable `/tmp` with no explicit permission hardening

**`crates/touring-dispatch/src/daemon.rs:594`** — `UnixListener::bind(&socket_path)?` at `/tmp/touring-daemon-<uid>.sock` (`cli/mod.rs:139`), with **no `set_permissions(0o600)`** on the socket and **no umask** narrowing before bind. The socket's mode is whatever the process umask yields (commonly `0o755`/`0o775` → other users can `connect`). `/tmp` is shared and world-traversable. On a multi-user host, another local user could connect to the daemon socket and issue requests — including `touring_file_ops` (SEC-01), which then runs with the daemon-owner's privileges → local privilege/data escalation.

The daemon does correctly use `flock LOCK_EX` for singleton ownership (per the topology docs), and the capnp embed sockets (`capnp_embed.rs:81-93`) share the `/tmp` pattern.

**Remediation.** After `bind`, `std::fs::set_permissions(&socket_path, Permissions::from_mode(0o600))` (or set umask `0o077` before bind). Better: move the socket to a per-user runtime dir `${XDG_RUNTIME_DIR}/touring-daemon.sock` (mode `0o700` dir, user-only), with the `/tmp` path as a documented fallback only. Apply the same to the capnp embed sockets.

### SEC-08 [Med · CWE-22] Multiple metadata/core tools read arbitrary user-supplied paths with no containment

Beyond SEC-01: arbitrary-file-read via MCP at `tools_metadata.rs:424`, `:861` (only an `exists()` precheck), `:990`, `:1058`; `tools_core.rs:1524`, `:1563` — all `fs::read_to_string(&user_path)` with no canonicalize/root check. Information disclosure of any readable file. The `canonicalize` calls that do exist (`tools_core.rs:961`, `:1447`) are for *display formatting*, not a security boundary. **Remediation:** same `guard_path` from SEC-01, applied to every path-taking tool.

### SEC-09 [Med · CWE-918] SSRF + remote-code-supply in `fetch_remote_wasm` (CLI inferlet install)

**`crates/touring-server/src/cli/handlers_inferlet.rs:280-292`** — `reqwest::blocking::get(url)` on a user-supplied URL (dispatch `:261-262` routes any `http(s)://` inferlet URI here), with **no host allowlist and no private-IP/metadata-endpoint block**, then runs the fetched WASM. Classic SSRF (reach `169.254.169.254`, RFC1918 services) plus arbitrary-module supply. Reachable via the `touring` CLI, **not** via an MCP `#[tool]` (no tool calls it), which lowers but does not eliminate the risk (a prompt-injected `touring inferlets install <url>` Bash call reaches it). **Remediation:** allowlist hosts/schemes; block link-local + RFC1918 + `.internal`; pin/verify a content hash of the fetched WASM before execution.

### SEC-10 [Med · doc-truth] CEG self-contradiction: `pre_exec.rs` says X5/landlock is "deferred / not yet isolated"; `supervised.rs`/`enforce_linux.rs` ship real landlock

`gateway/pre_exec.rs:23-33` (module docs) states the sandbox "is not yet filesystem-isolated (landlock is **P4.2**)" and the production X5 runner is `deferred_dry_run` (does not execute) — while `gateway/supervised.rs` and `capability/enforce_linux.rs:216` ship and test real kernel-enforced landlock (P4.2 *delivered*). Both are partly true (the default `touring exec` gate path is non-executing; `--real-exec` invokes the real supervised path), but the contradiction in the crown-jewel module's own docs will mislead an auditor or contributor about whether isolation is live. Combined with `supervised.rs:419-426` ("`touring exec` CLI is still analysis-only in P2") vs the actual `--real-exec` wiring at `cli/exec.rs:376-377`, the CEG's status comments are stale. **Remediation:** reconcile the module docs to the shipped state: "X8 landlock is live and gates `--real-exec`; the default `touring exec` path is analysis-only by design."

### SEC-11 [Med · CWE-400] Unbounded `read_to_string` of local files on hot/indexing paths

`graph_service.rs:791` (indexes a changed file on the watcher hot path), `tools/file_tools.rs:185`, `:606`, `tools/mod.rs` (10 sites), `cli/source_change.rs:122/192/314` — all `std::fs::read_to_string` with no size cap, loading whole files into memory. Errors are handled (no panic), but a pathologically large file in a watched project causes memory pressure on the singleton daemon. The *wire* paths are correctly bounded (rkyv 16 MiB / saga 2 MiB), so this is local-only. **Remediation:** a `read_capped(path, MAX)` helper (e.g. 32 MiB) for indexing/tool reads; skip + warn over the cap.

---

## LOW

### SEC-12 [Low · CWE-248] `assist.rs` `.parse().unwrap()` on user line:col — guarded, CLI-only
`cli/assist.rs:250,251,256` parse-unwrap user cursor coords, but each is **doubly guarded** by an `.is_ok()` re-check on the same string immediately above (`:242`, `:247`), and `parse_cursor_spec` is reachable only from the `touring assist` **CLI** process (not the daemon dispatch table). Practically unreachable; a panic would kill only an ephemeral CLI client, not the singleton. Defensive-quality smell, not a DoS vector. **Remediation:** replace with `?`/`map_err` to remove the smell and satisfy a future `#![deny(clippy::unwrap_used)]`.

### SEC-13 [Low · CWE-88] Argument injection in `touring_tdg` (dead-gated)
`tools_new.rs:78` passes a user `path` as a positional arg to `touring ast tdg <path>` with no `--` separator → a value like `--flag` is parsed as an option by the child. Impact bounded to a read-only analysis subcommand, and the tool is gated behind the **dead** `mcp-curated` flag (not compiled by default). **Remediation:** add `--` before user positionals.

### SEC-14 [Low · CWE-400] No per-line size cap in transcript/watcher line readers
`transcript_miner.rs:718` and `watcher.rs:202` use `BufReader::read_line` with no per-line byte cap; a multi-GB single line loads into one `String`. Off all hot paths (opt-in background sweep, local files only). **Remediation:** cap line length; skip over-long lines.

---

## Architectural note: the MCP surface is the un-gated twin of the CEG

The CEG was built to be the single chokepoint for code execution and it does that job well for `touring exec`. But the **MCP tool router (~169 always-on tools, `mod.rs:417-449`) is a parallel, ungoverned execution/IO surface**: `touring_file_ops` does raw FS, `touring_ctx_execute` runs code with an advisory-only static gate, the metadata tools read arbitrary files — none of them consult a `CapabilityProfile` or the gateway. The dead `mcp-curated`/`mcp-legacy` flags (Cargo.toml:93-94, both `[]`; gate at `mod.rs:442` only *adds* 3 tools) mean the intended "22 curated tools" reduction never happened. **The #1 security lever toward elite is to put the MCP surface behind the CEG capability model** — make every path-taking and code-running tool resolve its operation against a profile and a root-containment guard, the same deny-by-default discipline the gateway already proves it can enforce. That single move neutralizes SEC-01, SEC-02, and SEC-08 at once and unifies Touring's security posture behind its strongest component.
