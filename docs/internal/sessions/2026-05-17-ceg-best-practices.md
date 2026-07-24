# CEG — Best-Practices Research Dossier (P0.3)

> **Plan**: `2026-05-17-ceg-pln2-plan.md` (Code Execution Gateway, Pln2)
> **Phase**: P0.3 — best-practices research, research-only, no code edits
> **Date**: 2026-05-17 | **Author**: TACO research subagent
> **Sources**: context7 MCP (primary) + WebFetch fallback (ast-grep changelog, landlock changelog, tokio docs)

The CEG is a Rust subsystem of the Touring daemon that intercepts every code-bearing
action (Bash, Write, MCP `ctx_execute`, inferlets, jobs) and runs it through a 10-stage
pipeline (X0..X9) before production execution. This dossier captures concrete,
citable best-practice guidance for the five external-technology dependencies that
underpin the sandbox + capability-gate stages.

---

## Deno permission model

Deno is the canonical reference for a deny-by-default, granular capability model for
code execution. The CEG `Capability` / `CapabilityProfile` enums should mirror its
shape almost 1:1.

- **Secure-by-default**: access to system I/O is *denied* unless explicitly granted.
  The runtime grants nothing implicitly — the gateway must reproduce this: the default
  `CapabilityProfile` is empty (all stages must opt-in).
- **Granular allow flags**: `--allow-read`, `--allow-write`, `--allow-net`,
  `--allow-env`, `--allow-run` (plus `--allow-sys`, `--allow-ffi`, `--allow-import`).
  Each is independently grantable — there is no monolithic "trusted" flag. The CEG
  `Capability` enum should have one variant per resource class with the same split.
- **Paired deny flags override allow**: `--deny-read`, `--deny-write`, `--deny-net`,
  `--deny-env`, `--deny-run`. Deny *always wins* over allow — e.g.
  `--allow-env --deny-env=AWS_ACCESS_KEY_ID,AWS_SECRET_ACCESS_KEY` permits all env
  vars *except* the named secrets. CEG `CapabilityProfile` must carry both an allow-set
  and a deny-set per resource, with deny evaluated last.
- **Scoped values, not booleans**: each capability is scoped to a concrete value space:
  - read/write → **path subtrees** (a granted path covers the directory tree beneath it),
  - net → **`host:port`** tuples,
  - run → **command names** (`rm`, `sudo`),
  - env → **env-var keys**.
  CEG `Capability` variants should carry a typed scope payload (`PathScope`,
  `HostPort`, `CommandName`, `EnvKey`) rather than a bare `bool`.
- **Config-file form** mirrors the flag form with `allow` / `deny` / `ignore` arrays
  per resource, e.g. `"run": { "allow": true, "deny": ["rm", "sudo"] }`. The CEG
  capability policy should be expressible declaratively in the same allow/deny/ignore
  three-state form so profiles are auditable.
- **Runtime API `Deno.permissions`** has `query`, `request`, `revoke` (+ `*Sync`
  variants), each taking a `PermissionDescriptor` `{ name, path? }` and returning a
  `PermissionStatus` with `state ∈ {"granted","denied","prompt"}` and a `partial`
  flag. This is the model for CEG's X6 capability-gate decision states: a request can
  resolve to GRANTED / DENIED / PROMPT (escalate-to-human) rather than a binary pass.
- **Prompt suppression**: prompts are auto-suppressed when stdout/stderr is not a TTY
  or `--no-prompt` is set — a non-interactive run with an insufficient profile fails
  closed instead of hanging. CEG's daemon context is always non-interactive, so the
  X6 gate must treat "would-prompt" as DENIED unless a human-approval channel exists.

**CEG application**: shapes the **P2.1 `Capability` / `CapabilityProfile` enums** and
the **X6 CAPABILITY-GATE** stage — deny-by-default, allow-set + deny-set with
deny-wins, typed per-resource scope payloads, three-state GRANTED/DENIED/PROMPT
decision outcome.
**Source**: context7 `/websites/deno` — `docs.deno.com/api/deno/~/Deno.Permissions`,
`docs.deno.com/runtime/fundamentals/security`, `docs.deno.com/go/config`.

---

## Linux landlock LSM

Landlock is an unprivileged Linux LSM that lets a process restrict its *own* ambient
rights — perfect for the CEG, which can sandbox a child without root. The
`landlock` Rust crate (`landlock-lsm/rust-landlock`) is the canonical safe wrapper.

- **Latest stable crate**: `landlock` **v0.4.4** (actively maintained, MSRV Rust 1.68,
  all architectures supported as of 0.4.4). Pin `landlock = "0.4.4"`.
- **Ruleset builder API** is a fluent chain:
  `Ruleset::default()` (or `Ruleset::from(ABI::Vn)`) →
  `.handle_access(AccessFs::…)` (declare which access types the ruleset governs) →
  `.create()` → `.add_rule(PathBeneath::new(PathFd::new(path)?, AccessFs::…))?` →
  `.restrict_self()` (the irreversible enforcement call, applied to the calling thread).
- **`AccessFs` is a bitflag set** of file-access rights: `Execute`, `WriteFile`,
  `ReadFile`, `ReadDir`, `RemoveDir`, `RemoveFile`, `MakeChar`, `MakeDir`, `MakeReg`,
  `MakeSock`, `MakeFifo`, `MakeBlock`, `MakeSym`, `Refer` (ABI≥2), `Truncate` (ABI≥3),
  `IoctlDev` (ABI≥5). Helpers: `AccessFs::from_all(abi)` (all rights for an ABI),
  `AccessFs::from_file(abi)` (only the rights legitimate on a *file*, not a dir).
  Compose with `make_bitflags!(AccessFs::{ReadFile | ReadDir})`.
- **`PathBeneath` rules grant access to a path subtree** (directory tree), matching the
  Deno path-subtree model exactly — one rule per granted root. Use `PathFd::new` to
  open the path; a `PathBeneath` carrying file-only rights on a directory will error.
- **ABI versioning is explicit** via the `ABI` enum (`ABI::V1` … `ABI::V6`):
  - ABI 1 = base FS sandboxing; ABI 2 = `Refer` (rename/link across dirs);
    ABI 3 = `Truncate`; ABI 4 = TCP bind/connect control (`AccessNet`);
    ABI 5 = device IOCTL; ABI 6 = abstract UNIX sockets + signal scoping.
  - **ABI 4 (TCP control)** lets landlock also gate `--allow-net`-style network
    capability — CEG can use one LSM for both FS and net sandboxing if kernel ≥ 6.7.
- **`CompatLevel` governs graceful degradation** — `BestEffort` (default; silently
  drops unsupported access on older kernels), `SoftRequirement`, `HardRequirement`
  (errors if the feature is unavailable). Set per-call via `.set_compatibility(...)`.
  For CEG: use **`BestEffort`** so a kernel < 5.13 (no landlock) or an old kernel
  missing ABI 4/5 still runs — the sandbox degrades, it does not crash.
- **Kernel-version handling / degradation**: landlock first shipped in **kernel 5.13**.
  On older kernels `restrict_self()` is a no-op under `BestEffort`. The crate's idiom
  is `check_ruleset_support(min_abi, max_abi, closure, partial)` to probe support.
  v0.4.4 adds a `LandlockStatus` type to query which kernel features are live. The
  CEG must record the *effective* ABI achieved per run (a degraded sandbox is a
  weaker security posture and the X9 LEARN stage should log it).
- **`restrict_self()` is per-thread and irreversible** — it cannot be undone and
  applies only to the calling thread (and its future children). In the CEG this must
  be applied **inside the child** after fork, before `exec` (see tokio `pre_exec`).

**CEG application**: drives **P2.4 `apply_landlock`** — the function that builds a
`Ruleset` from the resolved `CapabilityProfile`, maps `PathScope` allow-set entries to
`PathBeneath` rules, selects `AccessFs` bitflags from the capability kind, runs under
`CompatLevel::BestEffort`, and is invoked from the child's `pre_exec` hook.
**Source**: context7 `/websites/rs_landlock_0_4_3` (`docs.rs/landlock/0.4.3/...`) +
WebFetch `github.com/landlock-lsm/rust-landlock/blob/main/CHANGELOG.md` (v0.4.4 notes).

---

## seccomp

seccomp-bpf filters *syscalls* (orthogonal to landlock, which filters *FS/net access*).
The modern Rust option is `seccompiler` (by rust-vmm, used in Firecracker).

- **Crate of choice**: `seccompiler` **v0.5.0** — pure-Rust, no `libseccomp` C
  dependency, no `build.rs` native linkage. Preferred over `libseccomp-rs` (FFI
  bindings to the C library) for a clean, statically-linked Touring daemon build.
- **Filter construction**: `SeccompFilter::new(rules, mismatch_action, match_action,
  target_arch)` where `rules: BTreeMap<i64, Vec<SeccompRule>>` maps a syscall number
  to OR-bound rules. An **empty `Vec`** for a syscall = allow unconditionally;
  per-rule `SeccompCondition`s can match on argument values (`SeccompCmpArgLen`,
  `SeccompCmpOp`, `SeccompCondition::new(arg_idx, len, op, value)`).
- **Actions**: `SeccompAction::{Allow, KillProcess, KillThread, Trap, Errno(n), Log,
  Trace(n)}`. For an allow-list jail, set `mismatch_action = KillProcess` and
  `match_action = Allow`. `mismatch_action` and `match_action` **must differ** or
  `SeccompFilter::new` returns `BackendError::IdenticalActions`.
- **Compile + install**: convert `SeccompFilter` → `BpfProgram` (a `Vec<sock_filter>`)
  via `TryInto`; this prepends an architecture-validation prologue. A program over
  **4096 BPF instructions** fails with `BackendError::FilterTooLarge` — keep the
  allow-list small. Install with `apply_filter(&bpf_prog)` which does
  `prctl(PR_SET_NO_NEW_PRIVS)` + `seccomp(SECCOMP_SET_MODE_FILTER)` for the **calling
  thread**. Verify with `prctl(PR_GET_SECCOMP) == 2`.
- **Composition with landlock**: seccomp and landlock stack cleanly and are
  complementary — landlock = "which files/dirs/sockets", seccomp = "which syscalls".
  Both require `PR_SET_NO_NEW_PRIVS`, both are per-thread/irreversible, both belong in
  the child's `pre_exec` *before* `exec`. Apply **landlock first, then seccomp** (so
  the landlock setup syscalls are not blocked by the seccomp filter).
- **Threat-model verdict for CEG**: landlock alone covers the *primary* CEG threat —
  untrusted code reading/writing outside its sandbox or opening network connections
  (ABI 4). seccomp adds defense-in-depth against syscall-level escapes
  (`ptrace`, `keyctl`, `bpf`, `userfaultfd`, `clone` namespace tricks) that landlock
  does not see. It is **worth adding as an opt-in hardening layer** but is **not
  required for the MVP** — the BPF instruction budget and the per-arch syscall-number
  maintenance burden make a curated allow-list non-trivial. Recommendation:
  ship landlock in P2.4; gate seccomp behind a `ceg-seccomp` Cargo feature for a
  later hardening wave.

**CEG application**: an **optional** hardening path for **P2.4** — feature-gated
`apply_seccomp` invoked from `pre_exec` after `apply_landlock`. Not on the MVP
critical path.
**Source**: context7 `/rust-vmm/seccompiler` (`context7.com/rust-vmm/seccompiler`) +
crate registry (`crates.io/crates/seccompiler` → 0.5.0).

---

## ast-grep

ast-grep powers the CEG X2 STATIC-ANALYSIS stage (polyglot risk-pattern matching).
Touring already depends on `ast-grep-core` 0.36; the plan targets an upgrade.

- **Latest version**: `ast-grep` **0.42.2** (0.42.0 released 2026-03-16, 0.42.1 on
  2026-04-04). The Rust library crates are `ast-grep-core`, `ast-grep-language`,
  `ast-grep-config`.
- **Metavariable syntax** (stable across 0.36→0.42):
  - `$VAR` matches a **single named** AST node. Names must be uppercase `A-Z`,
    underscore, or digits, starting right after `$`.
  - `$$VAR` matches a single **unnamed** node.
  - `$$$VAR` matches **zero or more** nodes (e.g. function args), and is **lazy** —
    `foo($$$A, b, $$$C)` binds `$$$A` to everything before the first `b`.
  - A metavariable must be a whole AST node — `mix$VAR` / `use$HOOK` do **not** work.
- **Pattern construction**: build a `Pattern` and prefer the **fallible**
  constructor `Pattern::try_new` (returns `Result`) over the infallible `Pattern::new`
  (panics on a malformed pattern). Touring memory W11.6 records a real production bug:
  `touring-code::polyglot` used the panicking `Pattern::new` and crashed on
  fuzz-malformed input — the fix was `Pattern::try_new` + `map_err`. The CEG X2 stage
  **must** use `try_new` and treat a pattern-compile failure as a non-fatal,
  fail-open analysis skip.
- **Rust library breaking changes 0.36 → 0.42** (these land in the **0.38** refactor
  "decouple ast-grep from tree-sitter"):
  - `AstGrep` is now an **alias for `Root`**.
  - Tree-sitter-specific methods moved off the `Language` trait into a **new
    `LanguageExt` trait** — callers that used those methods must now import
    `LanguageExt`.
  - `StrDoc` and related types **relocated to `ast_grep_core::tree_sitter`** module.
  - **0.38** "remove language bound in matcher" — the `Language` generic was dropped
    from core matcher APIs; matcher/pattern code that was generic over `L: Language`
    needs its signatures updated.
  - **0.36** moved processing/matching to worker threads, altering the `Worker` trait
    signature and execution model.
- **tree-sitter ABI / version per release** (the W11.6 risk area):
  - 0.42.0 bumps **tree-sitter to v0.26.7** (+ web-tree-sitter 0.26.7); 0.41.0 used
    tree-sitter v0.26.5.
  - tree-sitter's **default grammar ABI is now 15**; any grammar regenerated with a
    current tree-sitter CLI emits ABI 15. The tree-sitter runtime is
    **backwards-compatible** with grammars built against older ABIs but **not
    forwards-compatible** — a runtime expecting ABI ≤14 **cannot load an ABI-15
    grammar**.
- **ABI-compatibility caveat (CRITICAL — confirms Touring memory W11.6)**: the
  `tree-sitter` runtime version, the `tree-sitter-<lang>` grammar crates, and
  `ast-grep-core` must all agree on the parser ABI. An ast-grep upgrade that bumps the
  bundled tree-sitter runtime forces **every** `tree-sitter-<lang>` grammar dependency
  in the Touring workspace to be re-pinned to a version generated against the matching
  ABI. A mismatch (e.g. an ABI-14 grammar against an ABI-15-expecting runtime, or vice
  versa) is exactly the "polyglot grammar ABI break" W11.6 recorded — it manifests as a
  panic / load failure at parse time, not a compile error. The upgrade **must** be
  done as one atomic bump of `ast-grep-core` + `ast-grep-language` + every
  `tree-sitter-*` grammar crate, validated by the polyglot test suite + the W11.6
  fuzz targets *before* merge.

**CEG application**: drives **P1.3** (ast-grep 0.36 → 0.42.x upgrade — atomic
multi-crate bump, all `tree-sitter-*` grammars re-pinned, `LanguageExt` import + the
0.38 matcher signature changes applied) and the **X2 STATIC-ANALYSIS** stage
(`Pattern::try_new`, fail-open on compile error).
**Source**: context7 `/websites/ast-grep_github_io` (metavariables, `LanguageExt`
note) + WebFetch `github.com/ast-grep/ast-grep/blob/main/CHANGELOG.md`,
`ast-grep.github.io/blog/new-ver-38.html` + crates.io / tree-sitter release notes for
ABI 14/15. *(context7 had thin coverage of the `ast-grep-core` Rust crate API and ABI
versioning — web fallback used for the changelog, the 0.38 blog post, and tree-sitter
ABI specifics.)*

---

## tokio process isolation

The CEG X8 SUPERVISED-EXEC stage spawns and supervises the actual child process.
`tokio::process::Command` is the async wrapper over `std::process::Command`.

- **`Command` builder**: `Command::new(program)`, `.arg` / `.args`, `.env` / `.envs`,
  `.env_remove`, `.env_clear`, `.stdin/.stdout/.stderr(Stdio)`. It mirrors
  `std::process::Command` and exposes the std command via `as_std()` / `as_std_mut()`.
- **Environment hygiene**: call **`.env_clear()`** to drop *all* inherited environment
  variables, then re-add only the env keys the resolved `CapabilityProfile` allows
  (this is the runtime enforcement of the Deno-style `--allow-env` allow-set). Never
  let a child inherit the daemon's environment by default.
- **`pre_exec` — the sandbox injection point**:
  `unsafe fn pre_exec<F>(&mut self, f: F) -> &mut Command where F: FnMut() -> io::Result<()> + Send + Sync + 'static`.
  The closure runs **in the child, post-fork, just before `exec`**. This is where the
  CEG applies `apply_landlock` (and optional `apply_seccomp`) and `rlimit` caps so the
  restrictions are live before the untrusted binary is even loaded. Multiple closures
  run in registration order. It is `unsafe` because the post-fork environment is
  constrained — only async-signal-safe operations are valid (no `malloc`, no
  allocation, no mutex). Build the `Ruleset`/rlimit values **before** the closure and
  move only plain data in.
- **Process-group isolation**: `process_group(pgroup: i32)` sets the child's PGID
  (`setpgid` equivalent). Pass `0` to put the child in its **own new process group** —
  this lets the CEG signal the *entire* subtree (the child plus anything it spawns)
  with one `kill(-pgid, SIG…)`, and prevents terminal-driven `SIGINT` from leaking in.
- **Timeout enforcement**: there is no built-in timeout — wrap `child.wait()` in
  `tokio::time::timeout(dur, child.wait())`; on elapse, kill the process group. Use
  `tokio::select!` to race `child.wait()` against the timeout / a cancellation
  channel, exactly as in the tokio docs' read-stdout-while-waiting example.
- **Output capture**: set `.stdout(Stdio::piped())` / `.stderr(Stdio::piped())`, then
  `child.stdout.take()` and read concurrently with `AsyncReadExt::read_to_end` on a
  spawned task while you `wait()` — reading and waiting must be concurrent or a full
  pipe buffer can deadlock the child. `Command::output()` does piped capture +
  wait in one call and returns `Output { status, stdout, stderr }`, suitable when no
  streaming/interleaving is needed.
- **`kill_on_drop` + reaping**: default is `false` — a spawned child keeps running
  after its `Child` handle drops. Set **`.kill_on_drop(true)`** so a panic or early
  return in the X8 supervisor terminates the sandboxed child instead of leaking it.
  Tokio reaps exited children on a **best-effort** basis in the background; for a
  strict guarantee the supervisor must explicitly `child.wait().await` (or
  `child.kill().await` then `wait()`). Always `.await` the wait — never rely solely on
  drop-time reaping for a security-sensitive subsystem.

**CEG application**: drives **P4.2 / X8 SUPERVISED-EXEC** — `env_clear` + allow-set
env re-injection, `process_group(0)` for whole-subtree signal control, `pre_exec`
closure as the landlock/seccomp/rlimit injection point, `tokio::time::timeout` +
`select!` for the execution budget, piped concurrent stdout/stderr capture, and
`kill_on_drop(true)` + explicit `wait()` for leak-free reaping.
**Source**: context7 `/websites/rs_tokio_1_49_0` (`docs.rs/tokio/1.49.0/...`) +
WebFetch `docs.rs/tokio/latest/tokio/process/struct.Command.html`.

---

## Decisions for the plan

| Concern | Decision | Pin / Cargo.toml |
|---|---|---|
| **FS + net sandbox** | `landlock` crate — latest stable, ABI 1–6, `CompatLevel::BestEffort` for graceful degradation on kernels < 5.13 | `landlock = "0.4.4"` |
| **Low-level syscall wrappers / fd ops** (`PathFd`, rlimit, `setpgid`) | `rustix` — modern, safe POSIX bindings, statically linked, no C dep; on the 1.x line | `rustix = "1.1"` (latest 1.1.4) |
| **seccomp** | **Add as opt-in defense-in-depth, NOT on the MVP critical path.** Use `seccompiler` (pure-Rust, no `libseccomp` C dependency) behind a `ceg-seccomp` Cargo feature. Reason: landlock alone covers the primary CEG threat (FS/net escape); seccomp's value is syscall-level escape hardening, but a correct per-arch allow-list within the 4096-instruction BPF budget is a maintenance cost better deferred to a hardening wave. Apply landlock-then-seccomp inside `pre_exec`. | `seccompiler = "0.5"` (feature `ceg-seccomp`) |
| **ast-grep upgrade** | Upgrade `ast-grep-core` + `ast-grep-language` + `ast-grep-config` to **0.42.x** (target 0.42.2). | `ast-grep-core = "0.42"`, `ast-grep-language = "0.42"` |

### ast-grep 0.36 → 0.42 ABI-risk verdict

**MEDIUM-HIGH risk — confirmed by Touring memory W11.6.** The danger is *not* the
ast-grep Rust API breakage (the 0.38 `LanguageExt` trait split, `Root`/`AstGrep`
alias, `StrDoc` → `ast_grep_core::tree_sitter` relocation, and the "remove language
bound in matcher" signature change are all mechanical, compile-time-caught fixes).

The real risk is the **tree-sitter parser ABI**: 0.42 bundles tree-sitter v0.26.7,
whose default grammar ABI is **15**, while older grammars target ABI ≤14. The
tree-sitter runtime is backward-compatible but **not forward-compatible** — an
ABI-15-expecting runtime cannot load an ABI-14 grammar (and the reverse fails too).
A mismatch surfaces as a **runtime panic / grammar-load failure at parse time, not a
compile error** — precisely the W11.6 polyglot grammar break.

**Mitigation (mandatory)**: perform the upgrade as **one atomic multi-crate bump** —
`ast-grep-core`, `ast-grep-language`, and **every** `tree-sitter-<lang>` grammar crate
in the Touring workspace re-pinned to versions generated against the matching ABI.
Validate with the polyglot test suite **and** the W11.6 cargo-fuzz targets *before*
merge; do not pin the grammar crates independently of the ast-grep bump.

### Sources

- context7: `/websites/deno`, `/websites/rs_landlock_0_4_3`, `/rust-vmm/seccompiler`,
  `/websites/ast-grep_github_io`, `/websites/rs_tokio_1_49_0`.
- Web fallback (used for ast-grep changelog + ABI versioning, landlock CHANGELOG,
  crate version registry): `github.com/ast-grep/ast-grep/blob/main/CHANGELOG.md`,
  `ast-grep.github.io/blog/new-ver-38.html`,
  `github.com/landlock-lsm/rust-landlock/blob/main/CHANGELOG.md`,
  `crates.io/crates/{seccompiler,rustix,landlock}`,
  `docs.rs/tokio/latest/tokio/process/struct.Command.html`,
  tree-sitter release notes (ABI 14/15).
