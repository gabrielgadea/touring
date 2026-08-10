//! The compounding loop — verdicts recorded, then fed back as evidence.
//!
//! A static index answers the same way forever. What makes the portfolio an
//! organism is the pheromone: each time an intent is served, the agent records
//! which artifact it chose and how ([`Verdict`]), and the next answer carries
//! that history as [`Evidence`]. This is the ACO feedback the workspace already
//! names as a pillar — *consultar → executar → observar → registrar → reforçar*.
//!
//! Storage is an append-only JSONL beside the index. Append-only because a
//! verdict is a historical fact: superseding one is a new record, never an
//! edit of the old.

use std::collections::HashMap;
use std::io::Write as _;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::{CapabilityEntry, Verdict};

/// File name of the append-only verdict log, kept next to `index.json`.
pub const VERDICT_LOG: &str = "verdicts.jsonl";

/// One recorded decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerdictRecord {
    /// The intent that was served.
    pub intent: String,
    /// Id of the artifact the verdict is about; `None` for `create_new` with no
    /// prior art in play.
    pub artifact_id: Option<String>,
    /// What was decided.
    pub verdict: Verdict,
    /// Why — free text from the agent. Required by the CLI, so the log never
    /// degenerates into unexplained choices.
    pub rationale: String,
    /// Reward in `[0,1]`, when the outcome was measured.
    pub reward: Option<f64>,
    /// Append timestamp (`epoch:<secs>`).
    pub at: String,
}

/// Append one verdict to the log in `dir`.
///
/// # Errors
/// Returns an error if the directory cannot be created or the log cannot be
/// opened or written.
pub fn record(dir: &Path, rec: &VerdictRecord) -> Result<()> {
    std::fs::create_dir_all(dir)
        .with_context(|| format!("creating portfolio dir {}", dir.display()))?;
    let path = dir.join(VERDICT_LOG);
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("opening {}", path.display()))?;
    let line = serde_json::to_string(rec).context("serializing verdict")?;
    writeln!(f, "{line}").with_context(|| format!("appending to {}", path.display()))?;
    Ok(())
}

/// Read every verdict recorded in `dir`, oldest first.
///
/// Malformed lines are skipped rather than failing the read: a corrupt tail
/// must never make the portfolio unusable.
///
/// # Errors
/// Returns an error only when the log exists but cannot be read.
pub fn history(dir: &Path) -> Result<Vec<VerdictRecord>> {
    let path = dir.join(VERDICT_LOG);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    Ok(text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<VerdictRecord>(l).ok())
        .collect())
}

/// Fold the verdict history into a map of `artifact_id → (latest verdict, reward)`.
///
/// Later records win, so the map reflects the most recent decision about each
/// artifact.
#[must_use]
pub fn latest_by_artifact(records: &[VerdictRecord]) -> HashMap<String, (Verdict, Option<f64>)> {
    let mut map = HashMap::new();
    for r in records {
        if let Some(id) = &r.artifact_id {
            map.insert(id.clone(), (r.verdict, r.reward));
        }
    }
    map
}

/// Stamp entries with the verdict history so answers carry it as evidence.
///
/// Called after mining: the index itself stays a pure function of the corpus,
/// and history is layered on top.
pub fn apply_history(entries: &mut [CapabilityEntry], records: &[VerdictRecord]) {
    let latest = latest_by_artifact(records);
    for e in entries.iter_mut() {
        if let Some((verdict, reward)) = latest.get(&e.id) {
            e.evidence.prior_verdict = Some(*verdict);
            e.evidence.reward = *reward;
        }
    }
}

/// Current timestamp in the format stored in [`VerdictRecord::at`].
#[must_use]
pub fn now_stamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or_else(|_| "unknown".to_string(), |d| format!("epoch:{}", d.as_secs()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portfolio::{CapabilityKind, Evidence};
    use std::path::PathBuf;

    struct ScopedDir(PathBuf);

    impl ScopedDir {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("portfolio-fb-{tag}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).expect("mkdir");
            Self(dir)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for ScopedDir {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    fn rec(id: &str, v: Verdict, reward: Option<f64>) -> VerdictRecord {
        VerdictRecord {
            intent: "gerar PDF profissional".to_string(),
            artifact_id: Some(id.to_string()),
            verdict: v,
            rationale: "porque sim".to_string(),
            reward,
            at: now_stamp(),
        }
    }

    fn entry(id: &str) -> CapabilityEntry {
        CapabilityEntry {
            id: id.to_string(),
            display_path: "~/a.py".to_string(),
            kind: CapabilityKind::Script,
            name: "a".to_string(),
            purpose: "Generate a professional PDF".to_string(),
            language: "python".to_string(),
            entry_point: None,
            provenance: "skill:x".to_string(),
            keywords: vec![],
            evidence: Evidence::default(),
            purpose_inherited: false,
        }
    }

    #[test]
    fn record_then_history_roundtrips_in_order() {
        let s = ScopedDir::new("roundtrip");
        record(s.path(), &rec("script:~/a.py", Verdict::Reuse, Some(0.9))).expect("rec 1");
        record(s.path(), &rec("script:~/b.py", Verdict::Supersede, None)).expect("rec 2");
        let h = history(s.path()).expect("history");
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].verdict, Verdict::Reuse);
        assert_eq!(h[1].verdict, Verdict::Supersede);
    }

    #[test]
    fn absent_log_yields_empty_history() {
        let s = ScopedDir::new("absent");
        assert!(history(s.path()).expect("history").is_empty());
    }

    #[test]
    fn corrupt_line_is_skipped_not_fatal() {
        let s = ScopedDir::new("corrupt");
        record(s.path(), &rec("script:~/a.py", Verdict::Reuse, None)).expect("rec");
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(s.path().join(VERDICT_LOG))
            .expect("open");
        writeln!(f, "{{ not json").expect("write junk");
        drop(f);
        let h = history(s.path()).expect("history must survive a corrupt tail");
        assert_eq!(h.len(), 1);
    }

    #[test]
    fn later_verdict_supersedes_the_earlier_one() {
        let s = ScopedDir::new("latest");
        record(s.path(), &rec("script:~/a.py", Verdict::Reuse, Some(0.9))).expect("rec 1");
        record(s.path(), &rec("script:~/a.py", Verdict::Supersede, Some(0.1))).expect("rec 2");
        let latest = latest_by_artifact(&history(s.path()).expect("history"));
        let (v, r) = latest.get("script:~/a.py").expect("entry");
        assert_eq!(*v, Verdict::Supersede, "append-only, latest wins");
        assert_eq!(*r, Some(0.1));
    }

    #[test]
    fn history_lands_on_the_entry_as_evidence() {
        let mut entries = vec![entry("script:~/a.py"), entry("script:~/z.py")];
        let records = vec![rec("script:~/a.py", Verdict::Extend, Some(0.7))];
        apply_history(&mut entries, &records);
        assert_eq!(entries[0].evidence.prior_verdict, Some(Verdict::Extend));
        assert_eq!(entries[0].evidence.reward, Some(0.7));
        assert_eq!(entries[1].evidence.prior_verdict, None, "untouched entry stays blank");
        // And the blank one still SAYS it is blank.
        assert!(entries[1].evidence.summary().contains("nunca escolhido"));
    }

    #[test]
    fn create_new_without_an_artifact_is_recordable() {
        let s = ScopedDir::new("createnew");
        let r = VerdictRecord {
            intent: "algo inédito".to_string(),
            artifact_id: None,
            verdict: Verdict::CreateNew,
            rationale: "nada no portfólio cobre isto".to_string(),
            reward: None,
            at: now_stamp(),
        };
        record(s.path(), &r).expect("record");
        let h = history(s.path()).expect("history");
        assert_eq!(h[0].artifact_id, None);
        assert!(latest_by_artifact(&h).is_empty(), "no artifact → no evidence stamp");
    }
}
