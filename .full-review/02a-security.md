# Phase 2a: Security (F2.1–F2.6 + unsafe) — Touring Workspace

> Methodology: **evidence-grounded, verify-don't-assume.** Every finding cites a `crates/<x>/src/...:line` actually Read or literal command output. Prior findings (SEC-01) were re-verified against live code, not carried forward. `touring-quality`'s own crate excluded from secret scans (its detector fixtures false-positive on its own scanner). Read-only; no edits, no git, no process kills.

## Verdict

**Strong for a local agentic dev tool; one genuinely network-dangerous default in an opt-in web binary.** 0 Critical · 1 High · 4 Medium · 3 Low. The security *substrate* is elite (CEG landlock fail-closed, SEC-01 remediated + wired + tested, parameterized SQL/FTS, validated zero-copy IPC, 0 CVEs, real secret-redaction, zero hardcoded secrets). The single real risk is the **web dashboard binding `0.0.0.0` with no auth + permissive CORS** — but that binary is *not built by default* (`required-features = ["bind-web"]`), which is why it's High, not Critical.

| Sev | Count | Items |
|-----|-------|-------|
| 🔴 Critical | 0 | — |
| 🟠 High | 1 | SEC-02: web dashboard `0.0.0.0:3000` + no auth + CORS `Any` (network-exposed unauthenticated API incl. server-side `touring` exec) |
| 🟡 Medium | 4 | SEC-03 daemon Unix socket no `set_permissions` · SEC-04 `find`/`tree`/`glob` follow in-jail symlinks out of root · SEC-05 web error/path disclosure + no security headers · SEC-06 `cargo deny bans` FAIL (schemars dup) |
| 🟢 Low | 3 | SEC-07 `escape_html` missing `'` · SEC-08 postgis `transmute` (auto-trait drop, soundness grey-area) · SEC-09 `buffer_pool.rs` 2 undocumented `unsafe` |

---

## ✅ Already Elite (verified, credited — do NOT regress)

These are real, wired, and tested — not aspirational.

### CEG capability model + landlock (the headline control) — REAL
- `run_supervised` (`crates/touring-ceg/src/gateway/supervised.rs:344-404`) confines every routed run via a **`pre_exec` closure that is fail-closed**: `rs.restrict_current_thread()` must return `EnforcementLevel::KernelEnforced` or the spawn returns `Err` → *the sandboxed code never runs unconfined* (supervised.rs:382-388). Plus `apply_rlimit(&caps)` (CPU/AS/nofile/nproc/fsize caps).
- Landlock is the **real `landlock` crate v0.4** (`crates/touring-ceg/Cargo.toml:42`), ABI **V6**, built in `build_landlock_ruleset_with_net_and_scope` (`enforce_linux.rs:357-417`): filesystem path-jailing (`path_beneath_rules` for read/write roots), **deny-by-default network** (empty `bind/connect_tcp_ports` ⇒ AccessNet handled-but-unauthorised ⇒ kernel denies all TCP, enforce_linux.rs:402-414), opt-in IPC scope (deny cross-sandbox signals + abstract sockets). `CompatLevel::BestEffort` degrades loudly on old kernels (the run reports its actual posture).
- Capability profiles are deny-by-default (`crates/touring-ceg/src/capability/builtins.rs`): `ENV_ALLOWLIST` (builtins.rs:17) is exactly `PATH HOME USER LANG LC_ALL TERM TZ` — **no `AWS_*`/`GITHUB_TOKEN`/credential vars**. `apply_credential_whitelist` strips the rest before exec.
- The gateway IS wired into real execution paths: `touring exec` CLI (`crates/touring-server/src/cli/exec.rs:180-192,485-513,1268-1282`) and the hook runtime (`crates/touring-hook-runtime/src/ceg_adapter.rs:141-167`).
- **Honest nuance (not a defect, but read it):** the *pre-exec hook* deny is **opt-in via `CEG_ENFORCE=1`** (`ceg_adapter.rs:68-73,162`). With it unset (default), the hook is *advisory* — it injects context/warnings and feeds RL/metrics but does not hard-block. This is the documented fail-open safety invariant ("the gateway never blocks a session"). The **kernel sandbox of `run_supervised` is unconditional** for code routed through it; the *advisory-by-default* part is only the PreToolUse Bash gate. Credit the sandbox as elite; do not over-claim that arbitrary agent Bash is sandboxed by default — it is observed, not jailed, unless `CEG_ENFORCE=1`.

### SEC-01 path traversal — REMEDIATED + WIRED + TESTED (prior finding closed)
- Containment core `enforce_path_within_roots` (`crates/touring-server/src/tools/file_tools.rs:58-92`): `canonicalize()` resolves `..`/symlinks, then `roots.iter().any(|r| canonical.starts_with(r))`. `must_exist=false` validates the canonical *parent* for create/write.
- **Wired to the live MCP tool**: `touring_file_ops` handler calls `guard_fs_path` → `enforce_path_within_roots` at `crates/touring-server/src/server/tools_core.rs:1116-1120` *before every disk op* (read/write/append/delete/stat/mkdir/copy/move/tree/glob/list); copy/move also guard the dest (tools_core.rs:1257,1279).
- **Proven by 5 dedicated SEC-01 tests** (file_tools.rs:809-866): `enforce_denies_path_outside_root` (`/etc/passwd` → Err), `enforce_defeats_dotdot_traversal_escape` (canonicalization defeats `project/../escape`), etc. A regression fails the build. **PoC `{"operation":"read","path":"/etc/passwd"}` → denied** (`test_path_outside_root`, file_tools.rs:786-803).

### Other verified-clean
- **Zero hardcoded secrets** (F2.4): full scan of `ghp_/gho_/sk_live_/sk-/xoxb-/AKIA/AIza/PEM`; every hit is a `#[cfg(test)]` redactor fixture (CEG `redact_secrets` test inputs proving scrubbing works), an env-var *name* (`"TOURING_CEG_..."`, the correct read-from-env pattern), or excluded `touring-quality` detector fixtures. **No crypto deps** (`md-5/sha1/hmac/des/rc4` absent from all Cargo.toml) ⇒ no weak-crypto-for-security. Non-CSPRNG `thread_rng` used only for NN weight init (RL actor-critic), never for tokens/keys.
- **No SQL injection** (F2.1): every dynamic `format!`-built query interpolates **`&'static str` table/column names** (`schema_guard::TABLE_*`, `text_col()` → `&'static str` at recall.rs:100), never user input; all *values* are bound (`?1`/`$1`/`params![]`). FTS5 search (`crates/touring-intelligence/src/rl/memory/recall.rs:267-323`) binds the user query as `?1` (recall.rs:311) **and** escapes FTS5 operator chars into a phrase query (recall.rs:282-287) to block FTS-syntax abuse.
- **No command injection** (F2.1): the few `Command::new("sh").arg("-c")` sites take **hardcoded literals** — `command -v <bin>` over an allowlist (sandbox_executor.rs:144-170) or `sleep 0.05` in a `#[cfg(test)]` (sandbox_executor.rs:1362). Sandbox runtime args are vectorized `execve` argv (resolve_language_args, sandbox_executor.rs:183-200). Web `/api/mcp/call` uses vectorized `Command::args` (`shell_touring_value`, mod.rs:983-989) + whitelist + `valid_tool_arg` (no leading-dash/flag injection, charset-restricted, ≤200 chars, mod.rs:442-475).
- **unsafe: ELITE** (sampled ~13 production clusters, ~92% `# Safety`-documented, **0 high-risk**): rkyv untrusted-IPC paths validate with `check_archived_root`/`from_bytes` (bytecheck) before any field deref + 2 MiB body bound (`daemon.rs:1042,1118`); raw `archived_root` unchecked is confined to trusted local mmap + `#[cfg(test)]`. `#![forbid(unsafe_code)]` in `touring-contracts/-identity/-lsp/-hooks-saga`. (Detail in SEC-08/09.)
- **No CVEs**: `cargo deny check advisories` → **`advisories ok`**. `licenses ok`, `sources ok`.
- **No insecure deserialization**: untrusted bytes are `serde_json` (schema-validated) over the trusted daemon socket or the server's own `touring`-CLI stdout; `bincode::deserialize` (snapshot/store.rs:82) reads a local trusted snapshot.
- **SECURITY.md** present with a real private/coordinated-disclosure policy describing the CEG as the security model.

---

## 🟠 HIGH

### SEC-02 — Web dashboard binds `0.0.0.0` with zero auth + permissive CORS
- **CVSS-ish:** ~8.0 (AV:A/AC:L/PR:N/UI:N) on a LAN; lower if host is firewalled. **CWE-306** (Missing Authentication for Critical Function) + **CWE-942** (Permissive CORS) + **CWE-668** (Exposure to Wrong Sphere).
- **Evidence:**
  - `crates/touring-bindings/src/web/server/mod.rs:2259` — `let addr: SocketAddr = ([0, 0, 0, 0], 3000).into();` (hardcoded all-interfaces; no host env override — only `TOURING_WEB_PROJECT`/snapshot *paths* are configurable).
  - CORS `crates/touring-bindings/src/web/server/mod.rs:2195-2200` — `CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any)` (= `Access-Control-Allow-Origin: *`).
  - **No auth middleware** on the router (`build_app`): only `CorsLayer` + `TraceLayer`; ~30 routes incl. `/api/mcp/call`, `/api/speculate`, `/api/jobs` take `State`/`Json` with no credential extractor. A grep for `authorization|bearer|api.key|authenticate` across the web crates found no auth code.
  - `POST /api/mcp/call` (mod.rs:496-518) defaults `dry_run=true` but a body `{"dry_run":false}` triggers **server-side `touring <subcmd>` execution** (mod.rs:514-518).
- **Attack scenario:** A machine on the same LAN (or any browser page the user visits, via CORS `*`) reaches `http://victim:3000/api/mcp/call` unauthenticated and runs whitelisted `touring` subcommands server-side in the victim's project, reading wiring/index/quality/memory output (information disclosure of the codebase + RL/memory state). Command *injection* is blocked (whitelist + vectorized argv + no-dash arg validation), so this is an **unauthenticated network-reachable read-mostly command-trigger + info-disclosure**, not RCE.
- **Why High not Critical:** the `touring-web-server` binary is feature-gated `required-features = ["bind-web"]` and `default = []` (`touring-bindings/Cargo.toml`), so it is **not built/run by default**; the curated MCP server proper (`touring-server`) is **stdio-only**, no network bind (`crates/touring-server/src/main.rs` `rmcp::transport::io::stdio()`). Exposure only materializes when someone explicitly builds+runs the dashboard.
- **Remediation:**
  ```rust
  // mod.rs:2259 — default to loopback; require explicit opt-in for non-local.
  let host = std::env::var("TOURING_WEB_HOST").unwrap_or_else(|_| "127.0.0.1".into());
  let addr: SocketAddr = format!("{host}:3000").parse()?;
  // mod.rs:2195 — explicit localhost allowlist, not Any:
  CorsLayer::new()
      .allow_origin(["http://127.0.0.1:3000".parse().unwrap(),
                     "http://localhost:3000".parse().unwrap()])
      .allow_methods([Method::GET, Method::POST])
      .allow_headers([CONTENT_TYPE]);
  // Add a bearer-token middleware gating /api/* before any non-loopback bind is permitted;
  // hard-gate POST /api/mcp/call {dry_run:false} behind it.
  ```

---

## 🟡 MEDIUM

### SEC-03 — Daemon Unix socket created with no `set_permissions` (umask-dependent in shared `/tmp`)
- **CVSS-ish:** ~5.0 (AV:L/PR:L) — local multi-user host only. **CWE-276** (Incorrect Default Permissions).
- **Evidence:** `crates/touring-dispatch/src/daemon.rs:633` — `let listener = UnixListener::bind(&socket_path)?;` with **no** subsequent `set_permissions`/`from_mode`/`chmod` (confirmed by grep — none in daemon.rs). The global socket path is `/tmp/touring-daemon-<uid>.sock` (`crates/touring-hooks-core/src/ipc.rs:56`), a shared world-traversable dir. Same pattern at `crates/touring-bindings/src/capnp/embed.rs:91,93`.
- **Attack scenario:** With no explicit mode, the socket is `0777 & ~umask` (typically `srwxr-xr-x`). On a *multi-user* host any local user can `connect()` to the daemon RPC and drive indexing/file-read/generator operations under the daemon-owner's privileges. On a single-user workstation (the stated norm) impact is negligible.
- **Remediation:** immediately after bind, `std::fs::set_permissions(&socket_path, Permissions::from_mode(0o600))`, or place the socket inside a `0700` per-user dir (the project-local `<dir>/.touring/daemon.sock` path, ipc.rs:45, is preferable to `/tmp`).

### SEC-04 — `touring_file_ops` `find`/`tree`/`glob` follow in-jail symlinks out of the root
- **CVSS-ish:** ~4.8 (AV:N via MCP/prompt-injection/AC:L) — read-only escape. **CWE-59** (Link Following) + **CWE-22** (residual traversal).
- **Evidence:** `guard_fs_path` jails only the *root* argument (tools_core.rs:1120). The recursive walkers then descend without per-entry containment or symlink-skip: `find_ws_recursive` (`crates/touring-server/src/tools/file_tools.rs:598-651`) follows directory symlinks (no `symlink_metadata` skip — unlike the unused `FileTools::find_recursive` which *does* skip symlinks at file_tools.rs:323-329). `tree`'s inner `build_tree` (tools_core.rs:1322-1385) and `glob` (tools_core.rs:1296-1308) likewise recurse via `is_dir()`/`read_dir` with no re-containment.
- **Attack scenario:** a symlink already inside the project root (e.g. `crates/x/link -> /etc`) lets an attacker who can call `touring_file_ops` with `{"operation":"tree","path":"<root>","include_hidden":true}` or `find` enumerate/read file *paths and content* outside the jail (the `find` content-filter reads file bodies, file_tools.rs:640). Reachable via prompt-injection into the always-on MCP tool. The single-file `read`/`copy` ops are safe (their target is canonicalized + contained), so this is an enumeration/content-read leak through the recursive ops only.
- **Remediation:** in `find_ws_recursive`/`build_tree`, skip symlinked dirs (mirror `FileTools::find_recursive:323-329 — symlink_metadata().is_symlink() → continue`), or re-`canonicalize` + `starts_with(root)` each entry before descending/emitting.

### SEC-05 — Web server leaks internal error text + absolute paths to clients; no security headers
- **CVSS-ish:** ~4.3. **CWE-209** (Information Exposure Through Error Message) + **CWE-693** (Protection Mechanism Failure / missing headers).
- **Evidence:** `AppError::into_response` (`crates/touring-bindings/src/web/server/mod.rs:84-93`) serializes `self.to_string()` (raw subprocess stderr / IO error) into the HTTP body. Handlers embed absolute paths: e.g. mod.rs:627,632,657 (`path.display().to_string()`), `format!("file not found: {rel}")` (mod.rs:546). No CSP/HSTS/X-Frame-Options/X-Content-Type-Options anywhere in the web crates (grep returned nothing); responses set only `Content-Type` (mod.rs:90).
- **Attack scenario:** combined with SEC-02, a network client maps the host's filesystem layout and internal errors. Low severity in isolation; the missing CSP removes a defense-in-depth layer behind the `inner_html` rendering.
- **Remediation:** return generic client error bodies (log detail server-side); add a `SetResponseHeaderLayer`/security-headers layer (`X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`, restrictive CSP).

### SEC-06 — `cargo deny check bans` FAILS (duplicate `schemars`/`schemars_derive`)
- **CVSS-ish:** N/A (supply-chain hygiene, not an exploitable CVE). **D08/D44.** Already surfaced as A1 in Phase 1.
- **Evidence:** `cargo deny check bans` → `error[duplicate]: found 2 duplicate entries for crate 'schemars'` + `schemars_derive` → `bans FAILED`. Root cause: `touring-harness-mcp/Cargo.toml` pins `schemars = "0.8"` vs workspace 1.2.1 (rmcp 1.2). (`image`/`tiff` also pull a second tree but are warnings.) `advisories/licenses/sources` all **ok**.
- **Remediation:** `schemars = { workspace = true }` in `touring-harness-mcp/Cargo.toml` (1-line; verify rmcp 1.2 accepts 1.x — Phase 1 A1).

---

## 🟢 LOW

### SEC-07 — `escape_html` omits the single-quote `'`
- **CWE-79** (defense-in-depth gap, no concrete exploit found). `crates/touring-bindings/src/web/components/tables.rs:9-14` escapes `& < > "` but not `'`. Values interpolated into a single-quoted attribute context would not be neutralized. In practice renderers interpolate into text/double-quoted contexts (chains.rs escapes dynamic values before SVG/flow markup; wiring.rs SVG comes from the trusted `dot` pipeline), so no live XSS — but the helper is incomplete vs OWASP output-encoding. **Fix:** add `'` (and `/`) to the escape table.

### SEC-08 — `postgis` `transmute` drops an auto-trait (soundness grey-area)
- **CWE-704** (Incorrect Type Conversion). `crates/touring-bindings/src/postgis/mod.rs:133,158` — `unsafe { std::mem::transmute(params) }` converts `Vec<Box<dyn ToSql + Sync + Send>>` → `Vec<Box<dyn ToSql + Sync>>`. **No untrusted-input path** (`params` is internal), so not UB-from-untrusted-data. But transmuting a trait object to drop an auto-trait marker relies on identical vtable/data-pointer layout — true for current rustc, not language-guaranteed. The `async-sqlx` path in the same file (mod.rs:196-203) does the same bridge with **zero unsafe**. **Fix:** prefer that path or rebuild the `Vec` without transmute.

### SEC-09 — 2 undocumented `unsafe` blocks in `buffer_pool.rs`
- **CWE-1104** (style/discipline). `crates/touring-simd/src/buffer_pool.rs:42,59` (`dealloc` + `from_size_align_unchecked`) lack a per-block `// SAFETY:` comment (the rest of the file's unsafe is documented; ~92% of sampled workspace unsafe carries `# Safety`). Necessary (SIMD-aligned raw alloc), low risk — internal allocation only. **Fix:** add `// SAFETY:` justifying the layout invariant; optionally workspace-deny `clippy::undocumented_unsafe_blocks`.

---

## Notes for the consolidated report

- **The "elite security" claim is real and earned** for the substrate (CEG landlock fail-closed, SEC-01 wired+tested, parameterized SQL/FTS, validated IPC, 0 CVEs, 0 secrets). The narrative should state precisely: *kernel sandbox is unconditional for routed code; the agent-Bash PreToolUse gate is advisory unless `CEG_ENFORCE=1`.*
- **Single highest-risk lever:** SEC-02 (`0.0.0.0` + no-auth web dashboard). It is High only because the binary is opt-in (`bind-web`); if that ever becomes default-built, escalate to Critical. The 3-line `127.0.0.1` default + CORS allowlist closes most of it.
- **Two 1-line/small fixes with outsized value:** SEC-03 (`set_permissions(0o600)` on the socket) and SEC-06 (`schemars = { workspace = true }`).
- SEC-04 is the only residual in the otherwise-elite SEC-01 work: jail the *root* but not the recursive descent's symlinks.
