//! **S-10 / R9 — transactional shared state (read-set / write-set + lock manager).**
//!
//! The CRDT semantic graph gives *eventual* convergence, but two concurrent CEG
//! executions that mutate overlapping shared resources (the same symbol-index
//! entry, the same `outcome:*` key, the same CRDT node) can still interleave into
//! a lost update before convergence reconciles them. R9 adds the missing
//! transactional discipline: each action declares the resources it **reads** and
//! **writes** ([`AccessDeclaration`]), and a [`TxnLockManager`] serializes any two
//! actions whose access-sets conflict while letting disjoint ones run in parallel.
//!
//! # Conflict rule (the heart of S-10)
//!
//! Two declarations conflict iff one's **write-set** intersects the other's read-
//! or write-set — the classic write-write and read-write hazards. **Read-read
//! never conflicts**, so any number of pure readers proceed together. This is the
//! minimal serialization that prevents lost updates (the OP4 §5.2.4 guarantee)
//! without over-locking.
//!
//! Per-`Execution` [`Evidence`](super::typestate::Evidence) is deliberately *not*
//! a shared resource here — each execution owns its own ledger, so it can never
//! conflict with another's. Only the genuinely shared surfaces are tracked:
//! the symbol index, the outcome store, CRDT nodes, and filesystem paths.

use std::collections::{HashMap, HashSet};

/// Stable identifier of an in-flight execution holding declarations.
pub type ExecutionId = u64;

/// A shared resource an action reads or writes. Per-execution-local state (the
/// Evidence ledger) is intentionally absent — it cannot be a cross-execution
/// hazard.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AccessPath {
    /// A symbol-index entry (VGP reads these).
    Symbol(String),
    /// An `outcome:*` RL ledger key (PREDICT reads, LEARN writes).
    OutcomeKey(String),
    /// A CRDT semantic-graph node (LEARN merges these).
    CrdtNode(u64),
    /// A filesystem path (SUPERVISED-EXEC side effects).
    Path(String),
}

/// The read-set and write-set an action declares before acquiring.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccessDeclaration {
    /// Resources the action reads.
    pub reads: HashSet<AccessPath>,
    /// Resources the action writes.
    pub writes: HashSet<AccessPath>,
}

impl AccessDeclaration {
    /// An empty declaration (reads nothing, writes nothing — never conflicts).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder: declare a read of `path`.
    #[must_use]
    pub fn reading(mut self, path: AccessPath) -> Self {
        self.reads.insert(path);
        self
    }

    /// Builder: declare a write of `path`.
    #[must_use]
    pub fn writing(mut self, path: AccessPath) -> Self {
        self.writes.insert(path);
        self
    }

    /// `true` if this declaration and `other` conflict — i.e. one's write-set
    /// intersects the other's read- or write-set. Read-read never conflicts.
    #[must_use]
    pub fn conflicts_with(&self, other: &AccessDeclaration) -> bool {
        !self.writes.is_disjoint(&other.writes)
            || !self.writes.is_disjoint(&other.reads)
            || !other.writes.is_disjoint(&self.reads)
    }

    /// Build an `AccessDeclaration` from a (tool, payload) pair by inferring the
    /// read-set only. The tool name becomes a `Symbol(tool)` entry; any payload
    /// substring starting with `/`, `~/`, or `file://` becomes a `Path` entry.
    /// No writes are declared — the declaration is a pure reader, suitable for
    /// observe-mode CEG captures (X0) where the harness has not yet decided
    /// whether the call will mutate state.
    ///
    /// **ES3 P1 / S-10 (2026-06-01)** — wired by `pre_exec::run_gateway` as
    /// defense-in-depth; the real OP4 §5.2.4 lost-update guard for writes lives
    /// in `supervised.rs` X8 (ES3 P2).
    #[must_use]
    pub fn from_tool_payload(tool: &str, payload: &str) -> Self {
        let mut decl = Self::new().reading(AccessPath::Symbol(tool.to_owned()));
        for token in payload.split_whitespace() {
            let trimmed = token.trim_matches(|c: char| {
                c == '\'' || c == '"' || c == ',' || c == ';' || c == '(' || c == ')'
            });
            if let Some(path) = trimmed.strip_prefix("file://") {
                decl = decl.reading(AccessPath::Path(path.to_owned()));
            } else if trimmed.starts_with("~/") || trimmed.starts_with('/') {
                decl = decl.reading(AccessPath::Path(trimmed.to_owned()));
            }
        }
        decl
    }

    /// `true` if this declaration writes nothing (a pure reader).
    #[must_use]
    pub fn is_read_only(&self) -> bool {
        self.writes.is_empty()
    }

    /// **ES3 P2 (2026-06-02)** — full shell-syntax write inference.
    ///
    /// Like [`from_tool_payload`](Self::from_tool_payload) (which it calls
    /// first, inheriting the read-set), this additionally scans the payload
    /// for shell write signatures and adds the inferred paths to the
    /// write-set. Supports:
    ///
    /// * **Redirects** — `>`, `>>`, `2>`, `&>` (the target of the redirect
    ///   is a write).
    /// * **Write-tool commands** — `rm`, `mv`, `cp`, `touch`, `mkdir`,
    ///   `chmod`, `chown`, `sed -i` (the first whitespace-delimited
    ///   argument is a write candidate).
    ///
    /// # Conservative under-declaration (R-04)
    ///
    /// We **prefer false negatives over false positives**. The policy
    /// acknowledges that shell syntax is rich (quotes, command
    /// substitution, control flow) and that a strict parser would
    /// require hundreds of lines; instead, `extract_path_token`
    /// requires an absolute, `~/`-prefixed, or `file://`-prefixed token
    /// to even register a write. Relative paths and quoted-glob
    /// patterns are *not* declared. The kernel-enforced W
    /// (`build_landlock_ruleset_*`) is the real safety net — this
    /// declaration is the advisory layer that the lock manager consults
    /// to detect conflict.
    ///
    /// Non-Bash tools inherit the read-set only (the inference step is
    /// a no-op for them).
    #[must_use]
    pub fn from_tool_payload_full(tool: &str, payload: &str) -> Self {
        let mut decl = Self::from_tool_payload(tool, payload);
        if tool != "Bash" {
            return decl;
        }
        // Detect redirect operators: >, >>, 2>, &>
        for op in [" > ", " >> ", " 2> ", " &> "] {
            if let Some(idx) = find_unquoted(payload, op) {
                let after = &payload[idx + op.len()..];
                if let Some(target) = extract_path_token(after) {
                    decl = decl.writing(AccessPath::Path(target));
                }
            }
        }
        // Detect write-tool commands: rm, mv, cp, touch, mkdir, chmod, chown, sed -i
        for cmd in [
            "rm ", "mv ", "cp ", "touch ", "mkdir ", "chmod ", "chown ", "sed -i ",
        ] {
            if let Some(idx) = find_unquoted(payload, cmd) {
                let after = &payload[idx + cmd.len()..];
                if let Some(target) = extract_path_token(after) {
                    decl = decl.writing(AccessPath::Path(target));
                }
            }
        }
        decl
    }
}

/// ES3 P5 — apply `crate::hook_runtime::IsolationMode` to an
/// [`AccessDeclaration`]. In `Solo` mode, this is a no-op (paths stay
/// absolute — the single-agent path). In `Worktree(path)` mode, every
/// `AccessPath::Path` is rewritten so it is rooted at the worktree directory
/// (idempotent if already inside the worktree). Non-`Path` variants
/// (`Symbol`, `OutcomeKey`, `CrdtNode`) are passed through unchanged — they
/// are not filesystem paths. Pure over its arguments — never panics.
///
/// Honest scope (CAH roadmap §3): capability-readiness, not current demand.
/// P5 substrate is the wiring; the production-grade lock manager
/// integration (consuming this declaration) is a followup wave.
pub fn apply_isolation_mode(
    decl: AccessDeclaration,
    mode: &touring_hooks_shared::isolation_mode::IsolationMode,
) -> AccessDeclaration {
    // S-13 (2026-06-06): name IsolationMode from the leaf crate directly, not via
    // `crate::hook_runtime` — this breaks the gateway → hook_runtime edge.
    use touring_hooks_shared::isolation_mode::IsolationMode as M;
    match mode {
        M::Solo => decl,
        M::Worktree(root) => {
            let root_str = root.to_string_lossy().to_string();
            let rewrite = |p: AccessPath| -> AccessPath {
                match p {
                    AccessPath::Path(s) if !s.starts_with(&root_str) => {
                        // Worktree-relative → absolute under root
                        let joined = root.join(&s);
                        AccessPath::Path(joined.to_string_lossy().to_string())
                    }
                    other => other, // already inside, or non-Path variant
                }
            };
            let reads = decl.reads.into_iter().map(rewrite).collect();
            let writes = decl.writes.into_iter().map(rewrite).collect();
            AccessDeclaration { reads, writes }
        }
    }
}

/// **ES3 P2 (2026-06-02)** — extract the first absolute, `~/`-prefixed, or
/// `file://`-prefixed token from `s`. Scans ALL whitespace-delimited tokens
/// (not just the first) so that commands like `sed -i 'expr' /tmp/path` —
/// where the path is the second argument after the command — are handled
/// correctly. Leading/trailing quotes and shell metacharacters are stripped
/// from each candidate before the prefix check.
///
/// Returns `None` for relative paths and for empty/whitespace-only strings.
/// The strict prefix requirement is the under-declaration policy: a path
/// that does not look like an absolute filesystem target is not a write we
/// are willing to claim.
fn extract_path_token(s: &str) -> Option<String> {
    for token in s.split_whitespace() {
        let cleaned = token.trim_matches(|c: char| {
            c == '\'' || c == '"' || c == ';' || c == '|' || c == '&' || c == '(' || c == ')'
        });
        if cleaned.is_empty() {
            continue;
        }
        if cleaned.starts_with('/') || cleaned.starts_with("~/") || cleaned.starts_with("file://") {
            return Some(cleaned.to_owned());
        }
    }
    None
}

/// **ES3 P2 (2026-06-02)** — find the first occurrence of `pattern` in `s`
/// that is NOT inside single or double quotes. This is the quote-aware
/// keyword matcher used by [`AccessDeclaration::from_tool_payload_full`] so
/// that destructive-looking substrings inside string arguments (e.g. the
/// `rm` inside `echo 'rm -rf /tmp/x'`) do not trigger a write declaration.
///
/// The state machine tracks single-quote and double-quote mode separately
/// (shell convention: a single quote inside double quotes is literal, and
/// vice versa). Returns the byte offset of the match start.
fn find_unquoted(s: &str, pattern: &str) -> Option<usize> {
    if pattern.is_empty() {
        return None;
    }
    let s_bytes = s.as_bytes();
    let p_bytes = pattern.as_bytes();
    let plen = p_bytes.len();
    let mut i = 0;
    let mut in_single = false;
    let mut in_double = false;
    while i + plen <= s_bytes.len() {
        let c = s_bytes[i] as char;
        if c == '\'' && !in_double {
            in_single = !in_single;
            i += 1;
            continue;
        }
        if c == '"' && !in_single {
            in_double = !in_double;
            i += 1;
            continue;
        }
        if !in_single && !in_double && &s_bytes[i..i + plen] == p_bytes {
            return Some(i);
        }
        // Advance by one UTF-8 char (ASCII fast-path is the common case).
        i += 1;
    }
    None
}

/// The outcome of attempting to acquire access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcquireResult {
    /// Granted — the declaration is now registered as active.
    Granted,
    /// Blocked by the conflicting active execution (its id).
    Conflict(ExecutionId),
}

impl AcquireResult {
    /// `true` only for [`AcquireResult::Granted`].
    #[must_use]
    pub fn is_granted(&self) -> bool {
        matches!(self, AcquireResult::Granted)
    }
}

/// A dependency-aware lock manager: serializes conflicting access-sets, permits
/// disjoint ones. Advisory and non-blocking — callers `try_acquire`, and on
/// [`AcquireResult::Conflict`] back off and retry after the holder releases. The
/// CRDT layer remains the eventual-convergence substrate beneath this.
#[derive(Debug, Default)]
pub struct TxnLockManager {
    active: HashMap<ExecutionId, AccessDeclaration>,
}

impl TxnLockManager {
    /// An empty lock manager.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Try to acquire access for `id`. Grants (and registers) iff no *other*
    /// active execution's declaration conflicts; otherwise returns the id of one
    /// conflicting holder. Re-acquiring for an already-active `id` updates its
    /// declaration only when conflict-free against the others.
    pub fn try_acquire(&mut self, id: ExecutionId, decl: AccessDeclaration) -> AcquireResult {
        for (&other_id, other_decl) in &self.active {
            if other_id != id && decl.conflicts_with(other_decl) {
                return AcquireResult::Conflict(other_id);
            }
        }
        self.active.insert(id, decl);
        AcquireResult::Granted
    }

    /// Release `id`'s declaration. Returns `true` if it was active.
    pub fn release(&mut self, id: ExecutionId) -> bool {
        self.active.remove(&id).is_some()
    }

    /// Number of active (acquired, not-yet-released) executions.
    #[must_use]
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// `true` if `id` currently holds an acquisition.
    #[must_use]
    pub fn is_active(&self, id: ExecutionId) -> bool {
        self.active.contains_key(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sym(s: &str) -> AccessPath {
        AccessPath::Symbol(s.to_owned())
    }

    #[test]
    fn read_only_declarations_never_conflict() {
        let a = AccessDeclaration::new()
            .reading(sym("foo"))
            .reading(AccessPath::CrdtNode(1));
        let b = AccessDeclaration::new()
            .reading(sym("foo"))
            .reading(AccessPath::CrdtNode(1));
        assert!(
            !a.conflicts_with(&b),
            "read-read on the same resource must not conflict"
        );
        assert!(a.is_read_only());
    }

    #[test]
    fn write_write_on_same_resource_conflicts() {
        let a = AccessDeclaration::new().writing(AccessPath::CrdtNode(1));
        let b = AccessDeclaration::new().writing(AccessPath::CrdtNode(1));
        assert!(a.conflicts_with(&b));
    }

    #[test]
    fn write_read_hazard_conflicts() {
        let writer = AccessDeclaration::new().writing(AccessPath::CrdtNode(1));
        let reader = AccessDeclaration::new().reading(AccessPath::CrdtNode(1));
        assert!(writer.conflicts_with(&reader), "write-read is a hazard");
        assert!(reader.conflicts_with(&writer), "read-write is symmetric");
    }

    #[test]
    fn disjoint_writes_do_not_conflict() {
        let a = AccessDeclaration::new().writing(AccessPath::CrdtNode(1));
        let b = AccessDeclaration::new().writing(AccessPath::CrdtNode(2));
        assert!(!a.conflicts_with(&b), "disjoint write-sets parallelize");
    }

    #[test]
    fn lock_manager_serializes_conflicting_writes_permits_disjoint_reads() {
        let mut mgr = TxnLockManager::new();

        // task1 writes CrdtNode(1) + Symbol("a") → granted.
        let t1 = AccessDeclaration::new()
            .writing(AccessPath::CrdtNode(1))
            .writing(sym("a"));
        assert_eq!(mgr.try_acquire(1, t1), AcquireResult::Granted);

        // task2 writes a DISJOINT CrdtNode(2) → granted immediately (parallel).
        let t2 = AccessDeclaration::new().writing(AccessPath::CrdtNode(2));
        assert_eq!(mgr.try_acquire(2, t2), AcquireResult::Granted);
        assert_eq!(mgr.active_count(), 2);

        // task3 reads Symbol("foo") + CrdtNode(1) → conflicts with task1's write.
        let t3 = AccessDeclaration::new()
            .reading(sym("foo"))
            .reading(AccessPath::CrdtNode(1));
        assert_eq!(mgr.try_acquire(3, t3.clone()), AcquireResult::Conflict(1));
        assert!(!mgr.is_active(3), "blocked task must not be registered");

        // release task1 → task3 now acquires (no lost update; serialized after t1).
        assert!(mgr.release(1));
        assert_eq!(mgr.try_acquire(3, t3), AcquireResult::Granted);
        assert!(mgr.is_active(3));
    }

    #[test]
    fn pure_readers_coexist_in_the_manager() {
        let mut mgr = TxnLockManager::new();
        let r1 = AccessDeclaration::new().reading(sym("shared"));
        let r2 = AccessDeclaration::new().reading(sym("shared"));
        assert!(mgr.try_acquire(1, r1).is_granted());
        assert!(mgr.try_acquire(2, r2).is_granted());
        assert_eq!(
            mgr.active_count(),
            2,
            "two readers of the same resource coexist"
        );
    }

    // ---- ES3 P1 / S-10 (2026-06-01): from_tool_payload coverage ----

    #[test]
    fn from_tool_payload_derives_symbol_from_tool_name() {
        let decl = AccessDeclaration::from_tool_payload("Bash", "echo hello");
        assert!(
            decl.reads.contains(&AccessPath::Symbol("Bash".to_owned())),
            "tool name must produce a Symbol(\"Bash\") read entry; got {:?}",
            decl.reads
        );
    }

    #[test]
    fn from_tool_payload_extracts_absolute_paths() {
        let decl = AccessDeclaration::from_tool_payload(
            "Read",
            r#"please read /home/gabrielgadea/.claude/rust/Cargo.toml"#,
        );
        assert!(decl.reads.iter().any(|p| matches!(p, AccessPath::Path(s) if s == "/home/gabrielgadea/.claude/rust/Cargo.toml")),
            "absolute path must be captured; got {:?}", decl.reads);
    }

    #[test]
    fn from_tool_payload_extracts_tilde_and_file_uri_paths() {
        let decl = AccessDeclaration::from_tool_payload(
            "Edit",
            "open ~/projects/foo.rs and also file:///etc/hosts",
        );
        assert!(
            decl.reads
                .iter()
                .any(|p| matches!(p, AccessPath::Path(s) if s == "~/projects/foo.rs")),
            "tilde path must be captured; got {:?}",
            decl.reads
        );
        assert!(
            decl.reads
                .iter()
                .any(|p| matches!(p, AccessPath::Path(s) if s == "/etc/hosts")),
            "file:// URI must be captured (stripped of scheme); got {:?}",
            decl.reads
        );
    }

    #[test]
    fn from_tool_payload_never_declares_writes() {
        // Observe-mode invariant: from_tool_payload is a pure reader by design.
        let decl = AccessDeclaration::from_tool_payload(
            "Bash",
            "rm -rf /tmp/x && touch /tmp/y file:///tmp/z",
        );
        assert!(
            decl.is_read_only(),
            "from_tool_payload must never declare writes"
        );
        assert!(
            decl.writes.is_empty(),
            "writes set must be empty: {:?}",
            decl.writes
        );
    }

    // ---- ES3 P2 / S-2-1 (2026-06-02): from_tool_payload_full coverage ----

    #[test]
    fn infers_write_from_redirect_to_absolute_path() {
        let decl = AccessDeclaration::from_tool_payload_full(
            "Bash",
            "echo hello > /tmp/es3p2-redirect-1.log",
        );
        assert!(
            decl.writes
                .contains(&AccessPath::Path("/tmp/es3p2-redirect-1.log".to_owned())),
            "absolute path after ' > ' must be declared a write; got {:?}",
            decl.writes
        );
    }

    #[test]
    fn infers_write_from_redirect_to_tilde_path() {
        let decl =
            AccessDeclaration::from_tool_payload_full("Bash", "printf 'x' >> ~/es3p2-append.txt");
        assert!(
            decl.writes
                .contains(&AccessPath::Path("~/es3p2-append.txt".to_owned())),
            "tilde path after ' >> ' must be declared a write; got {:?}",
            decl.writes
        );
    }

    #[test]
    fn infers_write_from_rm_command() {
        let decl = AccessDeclaration::from_tool_payload_full("Bash", "rm /tmp/es3p2-rm-target.dat");
        assert!(
            decl.writes
                .contains(&AccessPath::Path("/tmp/es3p2-rm-target.dat".to_owned())),
            "rm <abs-path> must be declared a write; got {:?}",
            decl.writes
        );
    }

    #[test]
    fn infers_write_from_mv_command() {
        let decl = AccessDeclaration::from_tool_payload_full(
            "Bash",
            "mv /tmp/es3p2-src.txt /tmp/es3p2-dst.txt",
        );
        assert!(
            decl.writes
                .contains(&AccessPath::Path("/tmp/es3p2-src.txt".to_owned())),
            "mv first arg must be declared a write (source removed); got {:?}",
            decl.writes
        );
    }

    #[test]
    fn infers_write_from_sed_in_place() {
        let decl = AccessDeclaration::from_tool_payload_full(
            "Bash",
            "sed -i 's/foo/bar/' /tmp/es3p2-sed.txt",
        );
        assert!(
            decl.writes
                .contains(&AccessPath::Path("/tmp/es3p2-sed.txt".to_owned())),
            "sed -i must be declared a write; got {:?}",
            decl.writes
        );
    }

    #[test]
    fn false_positive_echo_quoted_rm_rf() {
        // I-01: quoted strings must NOT trigger write declaration. The
        // rm -rf text is just an argument to echo, not a destructive op.
        let decl =
            AccessDeclaration::from_tool_payload_full("Bash", "echo 'rm -rf /tmp/something'");
        assert!(
            decl.is_read_only(),
            "echo with quoted rm -rf string must not declare writes; got {:?}",
            decl.writes
        );
    }

    #[test]
    fn false_positive_git_status_is_pure_read() {
        // I-01: looks like it might have a write surface (--porcelain),
        // but git status never mutates; from_tool_payload_full must
        // recognise it as pure-read.
        let decl = AccessDeclaration::from_tool_payload_full("Bash", "git status --porcelain");
        assert!(
            decl.is_read_only(),
            "git status must remain pure-read even with write-inference active; got {:?}",
            decl.writes
        );
    }

    #[test]
    fn non_bash_tools_never_declare_writes() {
        // Edit/Write/MultiEdit each have their own write syntax; for P2 we
        // only infer for Bash. Other tools keep the read-only contract.
        let decl = AccessDeclaration::from_tool_payload_full("Edit", "rm /tmp/es3p2-edit-tool.rs");
        assert!(
            decl.is_read_only(),
            "non-Bash tool with rm-shaped payload must stay read-only under from_tool_payload_full; got {:?}",
            decl.writes
        );
    }
}
