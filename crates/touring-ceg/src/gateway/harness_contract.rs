//! ES2 P1/P2 — the constitutional sink-token contract (EAGLE B-6).
//!
//! Where [`super::change_contract::ChangeContract`] gates a self-mutation by a
//! before/after [`super::harness_metric::HarnessQuality`] delta, [`HarnessContract`]
//! pins the *constitution itself* — `CLAUDE.md` + `rules/*.md` — by a blake3
//! digest plus per-claim structural attestation. This is EAGLE's sink token Sk
//! made executable: a verified, hash-pinned contract surface that exists as a
//! runtime object instead of an implicit always-loaded file.
//!
//! Honest scope (the B-6 carve-out): the harness can pin, attest, and (ES2 P3)
//! re-inject the contract at compaction boundaries, but it CANNOT force the
//! model's attention to keep the tokens non-evicted — that is an inference-layer
//! guarantee one level below a tool/prompt harness. [`HarnessContract::attest`]
//! proves the contract is intact and verifiable; it does not claim attention
//! non-eviction.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// The REGRA #16 documented hard line limit for `CLAUDE.md` (a hygiene SHOULD,
/// remediated by moving sections to `rules/`, not a structural MUST).
const CLAUDE_MD_HARD_LIMIT: usize = 400;

/// Structural corruption ceiling for the `CLAUDE.md` line count: twice the
/// REGRA #16 hard limit. This is a *corruption / runaway tripwire* — a
/// duplicated or accidentally-appended constitution blows past it — deliberately
/// distinct from the 400-line hygiene SHOULD (which the live constitution
/// legitimately exceeds while carrying orchestrator complexity). Asserting the
/// hygiene SHOULD as a contract MUST would attest-false on a sane-but-over-target
/// file and would be theater; a file under `2 * hard_limit` is structurally sound.
const CLAUDE_MD_STRUCTURAL_CEILING: usize = CLAUDE_MD_HARD_LIMIT * 2;

/// The critical auto-load `rules/*.md` files whose absence is a genuine
/// constitutional breach — each backs a HARD REGRA: canonical workflows (#14),
/// process hygiene (#19), and the code-execution gateway.
const CRITICAL_RULES: &[&str] = &[
    "touring-native tooling-canonical-workflows.md",
    "touring-process-hygiene.md",
    "code-execution-gateway.md",
];

/// REGRA markers whose presence in `CLAUDE.md` is structurally required — the
/// constitutional safety invariants: git-prohibition (#11), process-hygiene
/// (#19), and the anti-injection guard (#20).
const REQUIRED_REGRA_MARKERS: &[&str] = &["REGRA #11", "REGRA #19", "REGRA #20"];

/// The constitutional property a claim asserts about the pinned constitution.
/// One claim maps to exactly one [`ClaimAttestation`] verdict (typed per-claim,
/// unlike [`super::change_contract::ContractVerdict`]'s flat `Vec<String>`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContractClaim {
    /// `CLAUDE.md` exists and is non-empty at the constitution root.
    ConstitutionPresent,
    /// `CLAUDE.md` line count is at or under the structural corruption ceiling.
    LineCountUnderStructuralCeiling {
        /// The ceiling that was checked (`2 * REGRA #16 hard limit`).
        limit: usize,
    },
    /// A required REGRA marker (e.g. `"REGRA #11"`) is present in `CLAUDE.md`.
    RegraMarkerPresent {
        /// The marker string searched for.
        marker: String,
    },
    /// A critical auto-load `rules/*.md` file is present.
    RuleFilePresent {
        /// The rule file name (relative to `rules/`).
        file: String,
    },
}

/// The typed per-claim verdict — the granularity
/// [`super::change_contract::ContractVerdict`] collapses into a flat
/// `violations: Vec<String>`, made explicit so each claim carries its own
/// executable pass/fail plus evidence (emitted for BOTH outcomes — the
/// executable-proof residue that makes B-6 verifiable rather than textual).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimAttestation {
    /// The claim being attested.
    pub claim: ContractClaim,
    /// `true` iff the claim held against the real on-disk constitution.
    pub passed: bool,
    /// Human-readable evidence line, emitted for both pass and fail.
    pub evidence: String,
}

/// The hash-pinned, runtime-attested constitutional contract — EAGLE's sink
/// token Sk made into a typed harness object.
///
/// Mirrors [`super::change_contract::ContractVerdict`]'s aggregate shape (an
/// overall `bool` + per-claim evidence): `attested` is the deny-wins conjunction
/// of every [`ClaimAttestation`], exactly as `ContractVerdict::committed` is
/// `violations.is_empty()`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessContract {
    /// blake3 hex digest binding `CLAUDE.md` + each `rules/*.md` (name + bytes,
    /// in sorted order) — the pin. Any byte or file-set change moves the digest.
    pub digest: String,
    /// Number of constitution files that fed the digest (`CLAUDE.md` if present,
    /// plus each `rules/*.md`).
    pub file_count: usize,
    /// Per-claim attestations — one per evaluated [`ContractClaim`].
    pub claims: Vec<ClaimAttestation>,
    /// Overall verdict: `true` iff EVERY claim passed (deny-wins).
    pub attested: bool,
}

impl HarnessContract {
    /// Pin and attest the constitution rooted at `constitution_root` (the
    /// `~/.claude` directory holding `CLAUDE.md` + `rules/`).
    ///
    /// Computes a deterministic blake3 digest over `CLAUDE.md` followed by each
    /// `rules/*.md` (name + bytes, lexicographically sorted) — so the digest is
    /// a real hash over real file bytes (tamper any byte → digest changes) — and
    /// evaluates the structural [`ContractClaim`]s, returning one
    /// [`ClaimAttestation`] each. Pure over its `&Path` argument and fail-open:
    /// an unreadable file yields a failed claim, never a panic.
    #[must_use]
    pub fn attest(constitution_root: &Path) -> HarnessContract {
        let claude_md = constitution_root.join("CLAUDE.md");
        let claude_bytes = std::fs::read(&claude_md).unwrap_or_default();
        let claude_text = String::from_utf8_lossy(&claude_bytes);

        // Collect rules/*.md sorted by file name for a deterministic digest.
        let rules_dir = constitution_root.join("rules");
        let mut rule_files: Vec<(String, Vec<u8>)> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&rules_dir) {
            let mut names: Vec<String> = entries
                .flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    name.ends_with(".md").then_some(name)
                })
                .collect();
            names.sort();
            for name in names {
                let bytes = std::fs::read(rules_dir.join(&name)).unwrap_or_default();
                rule_files.push((name, bytes));
            }
        }

        // blake3 digest: CLAUDE.md bytes, then each rule (name + bytes) in sorted
        // order. Feeding the name binds the digest to the file SET, so renaming
        // or adding a rule changes the pin, not just editing content.
        let mut hasher = blake3::Hasher::new();
        hasher.update(&claude_bytes);
        for (name, bytes) in &rule_files {
            hasher.update(name.as_bytes());
            hasher.update(bytes);
        }
        let digest = hasher.finalize().to_hex().to_string();
        let file_count = usize::from(!claude_bytes.is_empty()) + rule_files.len();

        let mut claims: Vec<ClaimAttestation> = Vec::new();

        // (1) Constitution present + non-empty.
        let present = !claude_bytes.is_empty();
        claims.push(ClaimAttestation {
            claim: ContractClaim::ConstitutionPresent,
            passed: present,
            evidence: if present {
                format!("CLAUDE.md present ({} bytes)", claude_bytes.len())
            } else {
                format!("CLAUDE.md missing or empty at {}", claude_md.display())
            },
        });

        // (2) Line count under the structural corruption ceiling.
        let line_count = claude_text.lines().count();
        let under_ceiling = line_count <= CLAUDE_MD_STRUCTURAL_CEILING;
        claims.push(ClaimAttestation {
            claim: ContractClaim::LineCountUnderStructuralCeiling {
                limit: CLAUDE_MD_STRUCTURAL_CEILING,
            },
            passed: under_ceiling,
            evidence: format!(
                "CLAUDE.md {line_count} lines vs structural ceiling {CLAUDE_MD_STRUCTURAL_CEILING} (2x REGRA #16 hard limit {CLAUDE_MD_HARD_LIMIT})"
            ),
        });

        // (3) Required REGRA safety markers present.
        for marker in REQUIRED_REGRA_MARKERS {
            let found = claude_text.contains(marker);
            claims.push(ClaimAttestation {
                claim: ContractClaim::RegraMarkerPresent {
                    marker: (*marker).to_owned(),
                },
                passed: found,
                evidence: if found {
                    format!("{marker} present in CLAUDE.md")
                } else {
                    format!("{marker} marker ABSENT from CLAUDE.md")
                },
            });
        }

        // (4) Critical auto-load rule files present.
        let present_rules: std::collections::HashSet<&str> =
            rule_files.iter().map(|(n, _)| n.as_str()).collect();
        for rule in CRITICAL_RULES {
            let found = present_rules.contains(rule);
            claims.push(ClaimAttestation {
                claim: ContractClaim::RuleFilePresent {
                    file: (*rule).to_owned(),
                },
                passed: found,
                evidence: if found {
                    format!("critical rule {rule} present")
                } else {
                    format!("critical rule {rule} ABSENT from rules/")
                },
            });
        }

        let attested = claims.iter().all(|c| c.passed);
        HarnessContract {
            digest,
            file_count,
            claims,
            attested,
        }
    }

    /// Whether every claim passed (the committable/attested verdict). Mirrors
    /// [`super::change_contract::ChangeContract`]'s `is_committed`.
    #[must_use]
    pub fn is_attested(&self) -> bool {
        self.attested
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// Build a fully-attesting constitution fixture under `root`: a CLAUDE.md
    /// containing all required REGRA markers padded to `extra_lines` total lines,
    /// plus every critical rule file.
    fn valid_fixture(root: &Path, total_lines: usize) {
        fs::create_dir_all(root.join("rules")).unwrap();
        let mut body = String::new();
        for marker in REQUIRED_REGRA_MARKERS {
            body.push_str(marker);
            body.push('\n');
        }
        while body.lines().count() < total_lines {
            body.push_str("filler line\n");
        }
        fs::write(root.join("CLAUDE.md"), body).unwrap();
        for rule in CRITICAL_RULES {
            fs::write(root.join("rules").join(rule), format!("# {rule}\n")).unwrap();
        }
    }

    fn tmp() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        // PROCESS-GLOBAL counter — a `thread_local` would reset per test thread,
        // so two tests on different threads would both get suffix 0 and collide
        // (one's remove_dir_all nukes the other's fixture mid-run). The atomic
        // guarantees a unique suffix across ALL threads, with no SystemTime/rand.
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("harness_contract_test_{}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn attest_produces_stable_digest() {
        let root = tmp();
        valid_fixture(&root, 50);
        let a = HarnessContract::attest(&root);
        let b = HarnessContract::attest(&root);
        assert_eq!(a.digest, b.digest, "same inputs must yield the same digest");
        assert_eq!(a.digest.len(), 64, "blake3 hex digest is 64 chars");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn attest_detects_tampered_file() {
        let root = tmp();
        valid_fixture(&root, 50);
        let before = HarnessContract::attest(&root).digest;
        // Append one byte to CLAUDE.md.
        let mut body = fs::read_to_string(root.join("CLAUDE.md")).unwrap();
        body.push('X');
        fs::write(root.join("CLAUDE.md"), body).unwrap();
        let after = HarnessContract::attest(&root).digest;
        assert_ne!(before, after, "a one-byte tamper must change the digest");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn attest_detects_added_rule() {
        let root = tmp();
        valid_fixture(&root, 50);
        let before = HarnessContract::attest(&root);
        fs::write(root.join("rules").join("zz-new-rule.md"), "# new\n").unwrap();
        let after = HarnessContract::attest(&root);
        assert_ne!(
            before.digest, after.digest,
            "adding a rule must change the digest"
        );
        assert_eq!(
            after.file_count,
            before.file_count + 1,
            "file_count increments"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn valid_fixture_attests_true() {
        let root = tmp();
        valid_fixture(&root, 50);
        let c = HarnessContract::attest(&root);
        assert!(
            c.attested,
            "a complete constitution must attest: {:?}",
            c.claims
        );
        assert!(c.claims.iter().all(|cl| cl.passed));
        assert!(c.is_attested());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_constitution_fails_and_blocks_attest() {
        let root = tmp();
        fs::create_dir_all(root.join("rules")).unwrap();
        for rule in CRITICAL_RULES {
            fs::write(root.join("rules").join(rule), "# r\n").unwrap();
        }
        // No CLAUDE.md.
        let c = HarnessContract::attest(&root);
        let present = c
            .claims
            .iter()
            .find(|cl| cl.claim == ContractClaim::ConstitutionPresent)
            .unwrap();
        assert!(
            !present.passed,
            "absent CLAUDE.md must fail ConstitutionPresent"
        );
        assert!(!c.attested, "a missing constitution must block attestation");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn line_count_ceiling_pass_and_fail() {
        // Under the ceiling → passes.
        let under = tmp();
        valid_fixture(&under, CLAUDE_MD_STRUCTURAL_CEILING - 10);
        let cu = HarnessContract::attest(&under);
        assert!(cu.attested, "under-ceiling fixture attests");
        let _ = fs::remove_dir_all(&under);

        // Over the ceiling → that claim fails, attestation blocked.
        let over = tmp();
        valid_fixture(&over, CLAUDE_MD_STRUCTURAL_CEILING + 50);
        let co = HarnessContract::attest(&over);
        let lc = co
            .claims
            .iter()
            .find(|cl| {
                matches!(
                    cl.claim,
                    ContractClaim::LineCountUnderStructuralCeiling { .. }
                )
            })
            .unwrap();
        assert!(!lc.passed, "over-ceiling line count must fail the claim");
        assert!(!co.attested, "a runaway constitution blocks attestation");
        let _ = fs::remove_dir_all(&over);
    }

    #[test]
    fn missing_regra_marker_blocks_attest() {
        let root = tmp();
        fs::create_dir_all(root.join("rules")).unwrap();
        // CLAUDE.md missing REGRA #20.
        fs::write(root.join("CLAUDE.md"), "REGRA #11\nREGRA #19\nbody\n").unwrap();
        for rule in CRITICAL_RULES {
            fs::write(root.join("rules").join(rule), "# r\n").unwrap();
        }
        let c = HarnessContract::attest(&root);
        let missing = c.claims.iter().find(|cl| {
            cl.claim
                == ContractClaim::RegraMarkerPresent {
                    marker: "REGRA #20".to_owned(),
                }
        });
        assert!(
            missing.is_some_and(|cl| !cl.passed),
            "absent REGRA #20 must fail"
        );
        assert!(
            !c.attested,
            "a missing safety REGRA blocks attestation (deny-wins)"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_critical_rule_blocks_attest() {
        let root = tmp();
        fs::create_dir_all(root.join("rules")).unwrap();
        let mut body = String::new();
        for marker in REQUIRED_REGRA_MARKERS {
            body.push_str(marker);
            body.push('\n');
        }
        fs::write(root.join("CLAUDE.md"), body).unwrap();
        // Only the first critical rule present; the rest absent.
        fs::write(root.join("rules").join(CRITICAL_RULES[0]), "# r\n").unwrap();
        let c = HarnessContract::attest(&root);
        assert!(!c.attested, "a missing critical rule blocks attestation");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn serde_roundtrips() {
        let root = tmp();
        valid_fixture(&root, 30);
        let c = HarnessContract::attest(&root);
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("attested"));
        assert!(json.contains("digest"));
        let back: HarnessContract = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back, "HarnessContract must round-trip through JSON");
        let _ = fs::remove_dir_all(&root);
    }
}
