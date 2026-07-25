---
plan: sprint-identify-root-cause-of
title: Sprint 4.6 — Daemon Root Cause Investigation (OS-level)
authored: 2026-05-24
level: L3
status: DRAFT
intent: |
  Sprint 4.6: identify root cause of touring-daemon silent death via OS-level instrumentation. Build strace wrapper, audit SIGKILL via auditd, scan SQLite assert paths, optional valgrind. Etapas: G strace wrapper script, H auditd rule for daemon, I SQLite assert scan, J valgrind soak test, K root-cause synthesis, L permanent fix via taco-forge perfect-edit.
quality_dimensions:
  - precision
  - scalability
  - performance
  - functionality
  - quality
  - detail
  - integration
  - dependencies
  - potentiation
ground_truth_ref: data/ground_truth.json
toolkit_version: taco-planning-v2.0
---

# Sprint 4.6 — Daemon Root Cause Investigation (OS-level) (Pln2)

> **Intent**: Sprint 4.6: identify root cause of touring-daemon silent death via OS-level instrumentation. Build strace wrapper, audit SIGKILL via auditd, scan SQLite assert paths, optional valgrind. Etapas: G strace wrapper script, H auditd rule for daemon, I SQLite assert scan, J valgrind soak test, K root-cause synthesis, L permanent fix via taco-forge perfect-edit.
> **Level**: L3 | **Authored**: 2026-05-24
> **Composite goal**: every dimension ≥ 8, no dimension < 7.

---

## 1. Ground Truth Summary

> Source — `data/ground_truth.json` produced by `ground_truth_collector.py`.

| Field | Value |
|-------|-------|
| `touring doctor` overall | DEGRADED |
| E2E composite score | ? |
| Wiring orphan count | 4550 |
| Index symbol count | 67698 |
| Evolution drift alert | none |
| Memory lessons applied | 5 |

### Symbols verified (VGP)

| Symbol | File | Line | Signature |
|--------|------|-----:|-----------|

### Past lessons applied

- **outcome:bash:transcript-1cba78e4:failure** — outcome:bash:transcript-1cba78e4:failure {"error":"Exit code 1\ntail: não foi possível abrir '/tmp/daemon-test2.log' para leitura: Arquivo ou diretório inexistente","resolution_input":{"command":"ls -
- **doc:lesson:inferlet-service-default-panic-fix** — doc:lesson:inferlet-service-default-panic-fix InferletService::default() panic fix (2026-04-12): Removed panic!, replaced with chain .or_else(|_| WasmRunner::new_on_demand()) + WasmRunner::new_sentine
- **outcome:bash:transcript-20a6858c:failure** — outcome:bash:transcript-20a6858c:failure {"error":"Exit code 2\n[1]+  Fim da execução com status 1      TOURING_DAEMON_SOCK=/tmp/touring-daemon-1000.sock nohup ./target/debug/touring serve >> /tmp/dae
- **outcome:bash:transcript-18919abf:failure** — outcome:bash:transcript-18919abf:failure {"error":"Exit code 124","resolution_input":{"command":"cat /tmp/touring-daemon.log 2>/dev/null | tail -20","description":"Check daemon log for errors"},"sessi
- **outcome:bash:transcript-89056438:failure** — outcome:bash:transcript-89056438:failure {"error":"Exit code 1\n/tmp/touring-daemon-1000.lock\n-rw-rw-r-- 1 gabrielgadea gabrielgadea   0 mai  4 23:21 /tmp/touring-daemon-1000.lock\n-rw-rw-r-- 1 gabri

### Known gotchas for target files


---

## 2. 9-Dimension Scores (Pln1 → Pln2)

> Source — `dimension_scorer.py`; amplifications — `dimension_amplifier.py`.

| Dim | Current | Target | Delta | Amplification |
|-----|--------:|-------:|------:|---------------|
| **precision** | 0.0 | 8.5 | 8.5 | to be measured |
| **scalability** | 0.0 | 8.5 | 8.5 | to be measured |
| **performance** | 0.0 | 8.5 | 8.5 | to be measured |
| **functionality** | 0.0 | 8.5 | 8.5 | to be measured |
| **quality** | 0.0 | 8.5 | 8.5 | to be measured |
| **detail** | 0.0 | 8.5 | 8.5 | to be measured |
| **integration** | 0.0 | 8.5 | 8.5 | to be measured |
| **dependencies** | 0.0 | 8.5 | 8.5 | to be measured |
| **potentiation** | 0.0 | 8.5 | 8.5 | to be measured |

**Composite**: 0.0 → 8.5 (delta 8.5).

---

## 3. Phases

> Built by `plan_scaffolder.py`; replaced with Sprint-4.6-specific Etapas G-L by author.
> DAG: Etapas G + H + I are read-only/parallel (Phase 1), then J (sequential), then K → L.

### Phase 1 — OS-LEVEL OBSERVABILITY (parallel, 3 items, requires sudo for H)

#### S-G: strace wrapper script around touring-daemon spawn [P0] [confidence: FACT]

- **File**: `~/.local/bin/touring-daemon-strace` (NEW shell wrapper, ~50 LOC)
- **Source truth**: Sprint 4.5 confirmed daemon dies SILENT (stderr cuts at offset 74 after WASM init); no Rust-level path captures it. `strace -p` is rejected by Linux yama=1 ptrace_scope on already-running processes; we must spawn daemon AS strace's child.
- **Change**:
  ```bash
  #!/usr/bin/env bash
  # Wraps touring-daemon with strace; logs syscalls + signals + exit_group
  set -u
  STRACE_LOG="${HOME}/.claude/touring/daemon-strace.log"
  exec strace -f -e trace=signal,exit,exit_group,abort,kill,tgkill \
       -o "$STRACE_LOG" \
       -- "${HOME}/.local/bin/touring-daemon" "$@"
  ```
- **Wire**: `update-touring start_daemon` adds env flag `TOURING_DAEMON_STRACE=1` to opt-into wrapper (default off — strace adds ~20% latency).
- **Blast radius**: 0 (additive wrapper)
- **Test**: spawn via wrapper, soak 5min, observe `daemon-strace.log` contains signal/exit entries when daemon dies; manual assertion via `grep "exit_group\|--- SIG" $STRACE_LOG`
- **Dimensions impacted**: a (precision — exact syscall capture), c (performance — 20% overhead acceptable for diagnostic), i (potentiation — reusable for future race debugging)
- **Enables**: Etapa K (root-cause synthesis) gets ground-truth syscall trace at death moment

#### S-H: auditd rule for SIGKILL/SIGTERM/SIGSEGV against touring-daemon [P0] [confidence: INFERENCE]

- **File**: `/etc/audit/rules.d/touring-daemon.rules` (NEW, requires sudo)
- **Source truth**: stderr capture loses all info post-SIGKILL; only kernel audit subsystem records the SENDER of SIGKILL. Per `man auditctl(8)`, syscall=kill catches kill(2)/tgkill(2)/tkill(2) targeting our PID.
- **Change**:
  ```
  -a always,exit -F arch=b64 -S kill -S tgkill -S tkill -F a1=9 -k touring_sigkill
  -a always,exit -F arch=b64 -S kill -S tgkill -S tkill -F a1=15 -k touring_sigterm
  -a always,exit -F arch=b64 -S kill -S tgkill -S tkill -F a1=11 -k touring_sigsegv
  ```
  Then `sudo augenrules --load && sudo systemctl restart auditd`. Query via `sudo ausearch -k touring_sigkill --start recent`.
- **Limitation**: auditd ON some kernels (apparmor restricted) requires elevated capabilities. Gabriel runs sudo; LLM cannot.
- **Blast radius**: 0 (new audit rule, no code)
- **Test**: trigger known kill — `sudo kill -9 $(pgrep -of touring-daemon)`; verify `sudo ausearch -k touring_sigkill | tail` shows entry with sender PID + comm
- **Dimensions impacted**: a (precision — sender PID identified), g (integration — kernel-level forensic complements Rust-level), i (potentiation — auditd captures ANY future kill source forever)
- **Enables**: Etapa K — definitively answers "who sent the signal" question

#### S-I: SQLite assert / libc abort scan in daemon-loaded crates [P1] [confidence: INFERENCE]

- **File**: `~/.claude/plans/sprint-4-6-daemon-root-cause/data/abort-scan.json` (output)
- **Source truth**: `libc::abort()` and SQLite assertions bypass `std::panic::set_hook`. `cargo tree` shows `rusqlite` + `tantivy` + `wasmtime` as transitive deps potentially calling `abort()`.
- **Change**: Python script `~/.claude/plans/sprint-4-6-daemon-root-cause/scripts/scan_abort_paths.py` walks Cargo.lock for crates ∈ {rusqlite, libsqlite3-sys, wasmtime, tantivy, capnp, parking_lot} and grep their `extern "C"` + `panic!` + `assert!` callsites in our consumer code. Emits JSON: `{ crate, file, line, kind, suspect_severity }`.
- **Blast radius**: 0 (read-only scan)
- **Test**: `python3 scan_abort_paths.py | jq '.findings | length'` returns > 0; manual review for false positives
- **Dimensions impacted**: a (precision — every potential abort callsite enumerated), e (quality — gives concrete targets for hardening), b (scalability — script reusable on every Cargo.lock change)
- **Enables**: Etapa K — assert/abort source narrowed to N specific callsites for instrumentation

### Phase 2 — OPTIONAL DEEP SCAN (sequential, 1 item)

#### S-J: valgrind soak test (4h) under sandbox env [P2] [confidence: SPECULATION]

- **File**: `~/.claude/plans/sprint-4-6-daemon-root-cause/data/valgrind-soak.log` (output)
- **Source truth**: Memory corruption (use-after-free / invalid free) on long-running daemons can manifest as SIGSEGV after hours of operation. `valgrind --tool=memcheck` catches this but introduces ~30× slowdown — only do if Etapas G/H point to memory issues.
- **Change**:
  ```bash
  valgrind --tool=memcheck --leak-check=full --error-exitcode=42 \
           --log-file=$LOG ~/.local/bin/touring-daemon &
  # Soak 4h; check log for "Invalid read/write" or process exit code
  ```
- **Gate**: only run if S-G strace shows SIGSEGV OR S-H auditd shows SIGSEGV. Otherwise SKIP.
- **Blast radius**: 0 (separate process)
- **Test**: post-soak, `grep "ERROR SUMMARY" $LOG` shows non-zero error count if memcorrupt detected; OR `echo $?` from valgrind ≠ 0
- **Dimensions impacted**: a (precision — line-level memcorrupt detection), c (performance — 30× overhead acceptable for diagnostic), i (potentiation — same harness reusable for future memory bugs)
- **Enables**: Etapa K — if memcorrupt, hardens fix decision toward unsafe/extern audit

### Phase 3 — SYNTHESIS + FIX (sequential, 2 items)

#### S-K: Root-cause synthesis [P0] [confidence: depends on evidence]

- **File**: `~/.claude/rust/docs/2026-05-XX-daemon-root-cause.md` (new diagnostic report)
- **Source truth**: cross-reference outputs of G/H/I (and J if run). Three possible verdicts:
  - **(a) External signal**: auditd shows sender PID/comm → action is hunt+fix the sender
  - **(b) libc abort / SQLite assert**: strace shows `abort` syscall + symbol from I → action is wrap the assert with Rust catch_unwind or fix the precondition
  - **(c) Memory corruption**: valgrind shows invalid R/W → action is unsafe/extern audit
- **Change**: Document the verdict with full evidence chain (strace excerpt + auditd entry + crash log + memory trace if any).
- **Blast radius**: 0 (documentation)
- **Test**: verdict has confidence ≥ FACT [0.9]; if INFERENCE only, escalate to Gabriel for next-iteration decision
- **Dimensions impacted**: a (precision — exact root cause), f (detail — full evidence chain), g (integration — synthesizes all 3 evidence sources)
- **Enables**: Etapa L — fix is specific, not blind

#### S-L: Permanent fix via taco-forge perfect-edit [P0] [confidence: FACT after S-K]

- **File**: depends on S-K verdict — likely `crates/touring-hooks/src/daemon.rs` OR new wrapper module
- **Source truth**: K verdict defines exact path
- **Change**: For each verdict:
  - (a) External signal: add `prctl(PR_SET_PDEATHSIG, 0)` to prevent parent-death cascades; OR add SIGTERM ignore for non-update-touring sources
  - (b) abort: wrap C-call in `catch_unwind` OR add precondition validation that prevents the assert
  - (c) memcorrupt: audit and rewrite unsafe block; add test that exercises the freed-pointer path
- **Method**: `taco-forge perfect-edit --operation rewrite --path <file>` (REGRA #14)
- **Blast radius**: ≤ 5 dependents (depends on file)
- **Test**: regression test that previously caused daemon death no longer kills it; soak 60min ≥ 100k reqs survives
- **Dimensions impacted**: e (quality — regression test prevents recurrence), d (functionality — daemon survives indefinite uptime), i (potentiation — fix applies to similar patterns elsewhere)
- **Enables**: composite_health_score → ≥ 0.85 (from 0.5), no more daemon-respawn churn, RL learning quality restored


---

## 4. DAG

> Mermaid + textual; emitted by `dag_builder.py --emit`.

```mermaid
graph LR
  start([Start]) --> P1[Phase 1] --> done([Pln2 ready])
```

Textual sequence:
P1 (1 sub) -> ... (more phases as authoring proceeds)

---

## 5. Verification Protocol

```bash
cargo check --workspace
cargo test --workspace
touring e2e -j
ruff check .
pyright
```

Acceptance:
- cargo: 0 errors
- tests: all green
- touring e2e: composite ≥ baseline
- ruff: 0 errors
- pyright: 0 errors
- wiring orphans delta ≤ 0

---

## 6. Potentiation Matrix (REGRA #0)

> Every Etapa surfaces what it **enables** beyond its immediate output.

| Etapa | Direct delivery | Enables |
|-------|-----------------|---------|
| **G** — strace wrapper | syscall+signal trace of daemon death | Reusable forensic harness for any future daemon race; opt-in via env var |
| **H** — auditd rule | kernel-level kill-sender identification | Permanent passive monitoring of daemon termination across ALL future sessions |
| **I** — abort-path scan | enumerated abort/assert callsites in deps | Reusable on any Cargo.lock change; catches new abort vectors as deps evolve |
| **J** — valgrind soak (gated) | line-level memcorrupt detection | Same harness usable for any other long-running binary (touring-hook in stress modes) |
| **K** — root-cause synthesis | FACT-confidence verdict + evidence chain | Documented runbook teaches future-TACO how to triage similar "silent death" patterns |
| **L** — permanent fix | regression-test-guarded daemon stability | composite_health_score ≥ 0.85; RL EMA quality restored; eliminates daemon-respawn churn that was poisoning hook hit rates

---

## Cross-references

- TACO-wt operates this plan once authored — see `~/.claude/skills/TACO-wt/SKILL.md`.
- The 9-dimension rubric — `~/.claude/skills/taco-planning/references/dimensions-rubric.md`.
- Amplification catalog — `~/.claude/skills/taco-planning/references/amplification-strategies.md`.
