# Security Policy

Touring is an **agentic code harness** that indexes source, generates code, and
executes code on behalf of agents. Security is a first-class concern: the Code
Execution Gateway (CEG, stages X0–X9) sandboxes every code-bearing action behind
`landlock` + `rlimit` + a deny-by-default capability model *before* real execution.

## Reporting a Vulnerability

If you discover a security issue, please **do not open a public issue**. Instead,
report it privately to the maintainer (see `SUPPORT.md` for contact). Include:

- a description of the issue and its impact;
- steps to reproduce (a minimal `touring`/CEG invocation if applicable);
- affected version (`touring --version`) and platform.

You can expect an acknowledgement within a reasonable window and a coordinated
disclosure once a fix is available.

## Scope

In scope: sandbox escape from the CEG, capability-model bypass (e.g. reading
outside the granted `PathScope`, unexpected network egress), privilege escalation
via hooks, and code injection in the generator/VGP path.

Out of scope: issues that require an already-compromised host, or behavior of
third-party models the harness drives.

## Hardening Notes

- The CEG `Sandboxed` profile denies network and writes by default, and the
  capability `ENV_ALLOWLIST` never carries credential env vars (`AWS_*`,
  `GITHUB_TOKEN`, …).
- **Sandboxed subprocess credentials**: distinct from `ENV_ALLOWLIST`, the
  *toolchain-execution* path (compilers and `gh`/`aws`/`cargo` run **inside** the
  sandbox) applies a separate `CREDENTIAL_ENV_WHITELIST` so first-party tooling can
  authenticate. Deployments that run untrusted code and must withhold **all**
  secrets set `TOURING_SANDBOX_NO_CREDENTIALS=1` — the child then inherits only
  baseline shell/locale vars (`HOME`/`PATH`/`USER`/`LANG`/`LC_ALL`/`TERM`). See
  `crates/touring-ceg/src/gateway/sandbox_executor.rs`.
- `touring_file_ops` (the always-on MCP filesystem tool) is jailed to the project
  root via canonicalize + root-containment (`..`/symlink escapes are resolved and
  rejected); extend the allowed roots with `TOURING_FILE_OPS_ALLOW_ROOTS`
  (colon-separated). See `crates/touring-server/src/tools/file_tools.rs`
  (`enforce_path_within_roots`).
- Kernel enforcement (landlock `AccessNet`/`Scope`) degrades *loud*, not silent,
  on older kernels — see `crates/touring-ceg/src/capability/enforce_linux.rs`.
- Supply chain: `cargo deny check` (advisories + bans + licenses + sources) is a
  binding CI gate; `deny.toml` carries scoped, justified advisory ignores reviewed
  quarterly.
