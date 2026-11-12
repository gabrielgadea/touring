//! A subtask's pointer to its markdown body, and its triage labels (S-05, 11/11/2026).
//!
//! `touring decompose` is this repo's issue tracker: the DAG indexes a ticket and a markdown file
//! holds its body (`docs/agents/issue-tracker.md`). Until now the body's path was glued to the end
//! of the description and the triage label lived only in a `Triage:` line of the file — invisible
//! to `frontier` and `ready`. These two columns make both first-class and queryable:
//!
//! * `body_file` — the PATH of the body (never its content). A relative path is stored relative to
//!   the project root, so the row survives a checkout moving; a path in the global store, which no
//!   single project owns, is stored absolute.
//! * `labels` — a JSON array in ONE nullable column, ordered and duplicate-free. The labels live
//!   in the subtask's own row (not a side table) so they move with it: a row removed or replaced
//!   takes its labels along instead of orphaning rows elsewhere.
//!
//! Shared by `cli::decompose` (add/update/get) and `wayfinder` (ready/frontier), which is why it
//! sits beside them: one validator and one decoder, because a rule written twice never meets
//! itself.

use crate::knowledge::FileKnowledgeDB;
use rusqlite::{OptionalExtension, params};
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

/// The accepted shape of a label, quoted verbatim in every refusal. [`validate_label`] is the
/// executable form of this pattern, pinned to it over a corpus by a test that compiles it as a
/// regex; the `--help` texts of `touring decompose` quote it too, pinned by a test in
/// `touring-server` through the re-export of this constant.
pub const LABEL_PATTERN: &str = "^[a-z0-9][a-z0-9:_-]{0,63}$";
const LABEL_MAX_LEN: usize = 64;

/// How many times a label rewrite retries after losing a race before it gives up loudly.
const LABEL_REWRITE_ATTEMPTS: usize = 8;

/// `Ok` when `raw` matches [`LABEL_PATTERN`]. Anything else is refused with a message that shows
/// the pattern — a label is a free string, so the pattern is the only documentation a caller gets.
pub(crate) fn validate_label(raw: &str) -> Result<(), String> {
    let mut chars = raw.chars();
    let first_ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    let rest_ok =
        chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, ':' | '_' | '-'));
    if first_ok && rest_ok && raw.len() <= LABEL_MAX_LEN {
        Ok(())
    } else {
        Err(format!(
            "invalid label '{raw}' — a label must match {LABEL_PATTERN} \
             (lowercase letters, digits, ':', '_' and '-'; 1-{LABEL_MAX_LEN} characters; \
             it starts with a letter or digit)"
        ))
    }
}

/// Reads an optional list-of-labels field from a payload: absent or `null` is the empty set, a
/// bare string is one label, an array is each of its strings. Every label is validated; the
/// result is the ordered, duplicate-free set.
pub(crate) fn parse_label_list(
    payload: &serde_json::Value,
    field: &str,
) -> Result<BTreeSet<String>, String> {
    let items: Vec<&serde_json::Value> = match payload.get(field) {
        None | Some(serde_json::Value::Null) => return Ok(BTreeSet::new()),
        Some(serde_json::Value::Array(items)) => items.iter().collect(),
        Some(single @ serde_json::Value::String(_)) => vec![single],
        Some(other) => {
            return Err(format!(
                "{field} must be a list of label strings, got {other}"
            ));
        }
    };
    let mut set = BTreeSet::new();
    for item in items {
        let label = item
            .as_str()
            .ok_or_else(|| format!("{field} must hold only strings, got {item}"))?;
        validate_label(label)?;
        set.insert(label.to_string());
    }
    Ok(set)
}

/// Reads an optional string field; a present value of another type is a loud refusal rather than a
/// silent "not given".
pub(crate) fn optional_str_field<'a>(
    payload: &'a serde_json::Value,
    field: &str,
) -> Result<Option<&'a str>, String> {
    match payload.get(field) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(s)) => Ok(Some(s.as_str())),
        Some(other) => Err(format!("{field} must be a string, got {other}")),
    }
}

/// The column's value for `set`: `None` (SQL NULL) for the empty set, so a row that never had a
/// label and a row whose last label was removed are the same bytes.
pub(crate) fn encode_labels(set: &BTreeSet<String>) -> Option<String> {
    (!set.is_empty())
        .then(|| serde_json::to_string(&set.iter().collect::<Vec<_>>()).ok())
        .flatten()
}

/// The labels a stored cell holds. NULL — every row written before this column existed — and any
/// cell that is not a JSON array of strings read as "no labels".
pub(crate) fn decode_labels(stored: Option<&str>) -> Vec<String> {
    stored
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default()
}

/// What a confirmation read of a subtask's body path and labels found, or why it could not.
pub(crate) type Readback = Result<(Option<String>, Vec<String>), String>;

/// The body path and label set a subtask's row holds right now.
///
/// A failed read is an ERROR, never "no body and no labels": the add that asks has already written,
/// and answering "this row is bare" on a database error is a claim nothing checked (R6.4 review).
/// A row that is missing after a successful write is an error too.
pub(crate) fn stored_meta(db: &FileKnowledgeDB, scoped_id: &str) -> Readback {
    db.conn_ref()
        .query_row(
            "SELECT body_file, labels FROM decomposition_subtasks WHERE subtask_id = ?1",
            params![scoped_id],
            |r| {
                let labels = r.get::<_, Option<String>>(1)?;
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    decode_labels(labels.as_deref()),
                ))
            },
        )
        .map_err(|e| {
            let why = format!("could not read {scoped_id} back to confirm it: {e}");
            tracing::warn!("decompose: {why}");
            why
        })
}

/// Puts a confirmation read on a JSON object: its two fields when it succeeded, and — only when
/// `error_key` names one — the reason when it did not (the fields are left out then, so the object
/// never describes a row nobody read). `None` (nothing was written, so nothing was read) adds nothing.
pub(crate) fn attach_readback(
    target: &mut serde_json::Value,
    readback: Option<&Readback>,
    error_key: Option<&str>,
) {
    match readback {
        Some(Ok((body, labels))) => {
            target["body_file"] = serde_json::json!(body);
            target["labels"] = serde_json::json!(labels);
        }
        Some(Err(why)) => {
            if let Some(key) = error_key {
                target[key] = serde_json::json!(why);
            }
        }
        None => {}
    }
}

/// Conjunctive label filter: the subtask must carry EVERY wanted label. An empty filter keeps all.
pub(crate) fn has_all_labels(have: &[String], want: &BTreeSet<String>) -> bool {
    want.iter().all(|w| have.iter().any(|h| h == w))
}

/// What `add` states about a ticket: the body path as typed, and the label set.
pub(crate) struct AddMeta<'a> {
    pub(crate) body_file: Option<&'a str>,
    pub(crate) labels: BTreeSet<String>,
}

impl<'a> AddMeta<'a> {
    /// Reads `body_file` and `labels` from an `add` payload; any malformed value is a refusal.
    pub(crate) fn from_payload(payload: &'a serde_json::Value) -> Result<Self, String> {
        Ok(Self {
            body_file: optional_str_field(payload, "body_file")?,
            labels: parse_label_list(payload, "labels")?,
        })
    }
}

/// What `update` asks to change about a ticket: a new body path, labels to add, labels to remove.
pub(crate) struct UpdateMeta<'a> {
    pub(crate) body_file: Option<&'a str>,
    add: BTreeSet<String>,
    remove: BTreeSet<String>,
}

impl<'a> UpdateMeta<'a> {
    /// Reads `body_file`, `add_labels` and `remove_labels` from an `update` payload. A label in
    /// BOTH lists is a contradiction, refused rather than resolved by an arbitrary order.
    pub(crate) fn from_payload(payload: &'a serde_json::Value) -> Result<Self, String> {
        let meta = Self {
            body_file: optional_str_field(payload, "body_file")?,
            add: parse_label_list(payload, "add_labels")?,
            remove: parse_label_list(payload, "remove_labels")?,
        };
        if let Some(both) = meta.add.intersection(&meta.remove).next() {
            return Err(format!(
                "label '{both}' is in both the labels to add and the labels to remove"
            ));
        }
        Ok(meta)
    }

    /// Does this update change the label set?
    pub(crate) fn touches_labels(&self) -> bool {
        !self.add.is_empty() || !self.remove.is_empty()
    }

    /// Does this update ask for anything this module owns?
    pub(crate) fn is_empty(&self) -> bool {
        self.body_file.is_none() && !self.touches_labels()
    }

    /// The set after this update: the additions in, the removals out. Removing a label the subtask
    /// never had is a no-op, not an error — the end state is what was asked for.
    pub(crate) fn apply(&self, set: &mut BTreeSet<String>) {
        set.extend(self.add.iter().cloned());
        set.retain(|label| !self.remove.contains(label));
    }
}

/// A tasksfile entry's body path and labels, validated and in the form to STORE. `None` fields mean
/// "the file did not state it": the import keeps what the row already holds.
pub(crate) struct ImportedMeta {
    pub(crate) body_file: Option<String>,
    pub(crate) labels_json: Option<String>,
}

/// Validates what a tasksfile entry carries exactly as `add` does: every label against
/// [`LABEL_PATTERN`], the body path resolved against `project_root` and required to exist now. The
/// first problem is the refusal, named.
pub(crate) fn imported_meta(
    body_file: Option<&str>,
    labels: &[String],
    project_root: &Path,
) -> Result<ImportedMeta, String> {
    let mut set = BTreeSet::new();
    for label in labels {
        validate_label(label)?;
        set.insert(label.clone());
    }
    Ok(ImportedMeta {
        body_file: resolve_optional_body(body_file, project_root, false)?,
        labels_json: encode_labels(&set),
    })
}

/// [`resolve_body_file`] for an optional path: `None` stays `None`.
pub(crate) fn resolve_optional_body(
    raw: Option<&str>,
    project_root: &Path,
    keep_absolute: bool,
) -> Result<Option<String>, String> {
    raw.map(|raw| resolve_body_file(raw, project_root, keep_absolute))
        .transpose()
}

/// Resolves and checks the ticket body at `raw`, returning the form to STORE.
///
/// A relative `raw` is relative to the project root. The file must exist and be a regular file
/// RIGHT NOW — a pointer written to a file that is not there is a dangling pointer from its first
/// second, and the refusal names the RESOLVED path so a wrong base is visible. The stored form is
/// root-relative whenever the file sits under the root (and `keep_absolute` is off), so the row
/// stays valid when the checkout moves; otherwise it is the absolute path.
///
/// `keep_absolute` is for the global store, which no single project owns: a relative path there
/// would have no stable base.
pub(crate) fn resolve_body_file(
    raw: &str,
    project_root: &Path,
    keep_absolute: bool,
) -> Result<String, String> {
    if raw.trim().is_empty() {
        return Err("body_file is empty — pass the path of the ticket's markdown file".to_string());
    }
    let given = Path::new(raw);
    let joined = if given.is_absolute() {
        given.to_path_buf()
    } else {
        project_root.join(given)
    };
    // `.` components are always safe to drop; `..` is left alone — resolving it lexically would
    // check a different file than the OS does whenever a symlink sits in the middle.
    let resolved: PathBuf = joined
        .components()
        .filter(|c| !matches!(c, Component::CurDir))
        .collect();
    match std::fs::metadata(&resolved) {
        Ok(meta) if meta.is_file() => {}
        Ok(_) => {
            return Err(format!(
                "body file {} is not a regular file",
                resolved.display()
            ));
        }
        Err(e) => {
            return Err(format!(
                "body file {} does not exist or cannot be read: {e}",
                resolved.display()
            ));
        }
    }
    let stored = if keep_absolute {
        resolved
    } else {
        relative_to_root(&resolved, project_root).unwrap_or(resolved)
    };
    Ok(stored.to_string_lossy().into_owned())
}

/// `file` relative to `root`, lexically first and through symlinks second (`/tmp` and `/home` are
/// often links, and a root reached by one name must still recognise a file named by another).
fn relative_to_root(file: &Path, root: &Path) -> Option<PathBuf> {
    if let Ok(rel) = file.strip_prefix(root) {
        return Some(rel.to_path_buf());
    }
    let (file, root) = (file.canonicalize().ok()?, root.canonicalize().ok()?);
    file.strip_prefix(&root).ok().map(Path::to_path_buf)
}

/// Rewrites a subtask's label set as `change(current)`, atomically.
///
/// A read-modify-write on a cell is a lost update waiting for two writers, so the write is a
/// compare-and-swap in the shape of the claim (ADR 0005): `UPDATE … WHERE labels IS <what was
/// read>` changes exactly one row, or zero when someone else got there first, in which case the
/// change is re-applied to what is there now. `change` can therefore run more than once.
///
/// `Ok(None)` is "no such subtask" (nothing written); `Ok(Some(labels))` is the set now stored.
pub(crate) fn rewrite_labels(
    db: &FileKnowledgeDB,
    scoped_id: &str,
    mut change: impl FnMut(&mut BTreeSet<String>),
    now: &str,
) -> Result<Option<Vec<String>>, String> {
    for _ in 0..LABEL_REWRITE_ATTEMPTS {
        let stored: Option<Option<String>> = db
            .conn_ref()
            .query_row(
                "SELECT labels FROM decomposition_subtasks WHERE subtask_id = ?1",
                params![scoped_id],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let Some(stored) = stored else {
            return Ok(None);
        };
        let mut set: BTreeSet<String> = decode_labels(stored.as_deref()).into_iter().collect();
        change(&mut set);
        let encoded = encode_labels(&set);
        if encoded == stored {
            return Ok(Some(set.into_iter().collect()));
        }
        let written = db
            .conn_ref()
            .execute(
                "UPDATE decomposition_subtasks SET labels = ?1, updated_at = ?2 \
                 WHERE subtask_id = ?3 AND labels IS ?4",
                params![encoded, now, scoped_id, stored],
            )
            .map_err(|e| e.to_string())?;
        if written == 1 {
            return Ok(Some(set.into_iter().collect()));
        }
    }
    Err(format!(
        "the labels of {scoped_id} kept changing under concurrent writers; gave up after \
         {LABEL_REWRITE_ATTEMPTS} attempts — nothing was written, retry"
    ))
}

/// What [`write_update`] found and left behind.
pub(crate) struct MetaWrite {
    /// The subtask exists (and so the write, if any, reached it).
    pub(crate) row_found: bool,
    /// The label set after the change, when the update touched labels.
    pub(crate) labels: Option<Vec<String>>,
}

/// Applies an `update`'s body path and label changes to one subtask. The labels go first: their
/// rewrite is the only step that can refuse, and it must refuse before anything is written. A
/// subtask that does not exist writes nothing and reports `row_found: false`.
pub(crate) fn write_update(
    db: &FileKnowledgeDB,
    scoped_id: &str,
    meta: &UpdateMeta<'_>,
    body_file: Option<&str>,
    now: &str,
) -> Result<MetaWrite, String> {
    let mut labels = None;
    if meta.touches_labels() {
        match rewrite_labels(db, scoped_id, |set| meta.apply(set), now)? {
            Some(after) => labels = Some(after),
            None => {
                return Ok(MetaWrite {
                    row_found: false,
                    labels: None,
                });
            }
        }
    }
    let mut row_found = labels.is_some();
    if let Some(body) = body_file {
        let rows = db
            .conn_ref()
            .execute(
                "UPDATE decomposition_subtasks SET body_file = ?1, updated_at = ?2 \
                 WHERE subtask_id = ?3",
                params![body, now, scoped_id],
            )
            .map_err(|e| e.to_string())?;
        row_found = rows > 0;
    }
    Ok(MetaWrite { row_found, labels })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_validator_agrees_with_the_documented_pattern() {
        for ok in [
            "a",
            "0",
            "needs-triage",
            "ready:for_agent",
            "a".repeat(64).as_str(),
        ] {
            assert!(validate_label(ok).is_ok(), "{ok} should be accepted");
        }
        for bad in [
            "",
            "-a",
            ":a",
            "_a",
            "A",
            "Ready",
            "a b",
            "a.b",
            "a/b",
            "a\n",
            " a",
            "é",
            "a,b",
            "a".repeat(65).as_str(),
        ] {
            let err = validate_label(bad).expect_err(&format!("{bad:?} should be refused"));
            assert!(
                err.contains(LABEL_PATTERN),
                "the refusal shows the pattern: {err}"
            );
        }
    }

    #[test]
    fn a_label_list_is_ordered_deduplicated_and_validated() {
        let p = serde_json::json!({"labels": ["wontfix", "needs-info", "wontfix"]});
        let got: Vec<String> = parse_label_list(&p, "labels")
            .unwrap()
            .into_iter()
            .collect();
        assert_eq!(got, vec!["needs-info", "wontfix"]);
        assert!(
            parse_label_list(&serde_json::json!({}), "labels")
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            parse_label_list(&serde_json::json!({"labels": "x"}), "labels")
                .unwrap()
                .len(),
            1
        );
        assert!(parse_label_list(&serde_json::json!({"labels": ["ok", "NO"]}), "labels").is_err());
        assert!(parse_label_list(&serde_json::json!({"labels": [1]}), "labels").is_err());
        assert!(parse_label_list(&serde_json::json!({"labels": 7}), "labels").is_err());
    }

    #[test]
    fn the_empty_set_is_stored_as_null_and_null_reads_as_empty() {
        assert_eq!(encode_labels(&BTreeSet::new()), None);
        assert_eq!(decode_labels(None), Vec::<String>::new());
        assert_eq!(decode_labels(Some("not json")), Vec::<String>::new());
        let set: BTreeSet<String> = ["b", "a"].map(String::from).into();
        assert_eq!(encode_labels(&set).as_deref(), Some(r#"["a","b"]"#));
    }

    #[test]
    fn a_relative_body_is_resolved_against_the_root_and_stored_relative() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("tickets")).unwrap();
        std::fs::write(tmp.path().join("tickets/S-05.md"), "# body").unwrap();
        let stored = resolve_body_file("./tickets/S-05.md", tmp.path(), false).unwrap();
        assert_eq!(stored, "tickets/S-05.md");
        let abs = tmp.path().join("tickets/S-05.md");
        let stored = resolve_body_file(abs.to_str().unwrap(), tmp.path(), false).unwrap();
        assert_eq!(
            stored, "tickets/S-05.md",
            "an absolute path under the root is stored relative"
        );
        let stored = resolve_body_file("tickets/S-05.md", tmp.path(), true).unwrap();
        assert_eq!(
            Path::new(&stored),
            abs,
            "the global store keeps the absolute path"
        );
    }

    #[test]
    fn a_missing_or_non_file_body_is_refused_naming_the_resolved_path() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("tickets")).unwrap();
        let err = resolve_body_file("tickets/none.md", tmp.path(), false).unwrap_err();
        assert!(
            err.contains(&tmp.path().join("tickets/none.md").display().to_string()),
            "{err}"
        );
        let err = resolve_body_file("tickets", tmp.path(), false).unwrap_err();
        assert!(err.contains("not a regular file"), "{err}");
        assert!(resolve_body_file("  ", tmp.path(), false).is_err());
    }
}
