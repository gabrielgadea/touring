//! Line-oriented structure of GitHub Actions workflow text (F4.7).
//!
//! The substring detectors in the parent module answer "does the file mention X";
//! the supply-chain rules need "does THIS job, with THESE permissions, do X". That
//! needs the job/step shape, but not a full YAML parser: the directory-mode reader
//! (`read_artifact_source`) hands F4.7 several workflow files concatenated into one
//! text, which no single-document parser accepts, and fragments (a bare `- uses:`
//! list) must keep scoring. So this module reads the indentation:
//!
//! * [`yaml_comment_regions`] — the byte spans of YAML comments, the only
//!   non-executable text in a workflow. (The detectors used to mask the file with the
//!   *Rust* lexer, whose string and char-literal rules swallow real `uses:` lines:
//!   7 mutable `dtolnay/rust-toolchain` refs in the repository's `ci.yml` were read
//!   as 3.)
//! * [`parse_workflows`] — `permissions`, the `pull_request_target` trigger and the
//!   jobs with their steps, one [`Workflow`] per document of the concatenation.
//!
//! Known limits, by design: block-style YAML only (a flow-style job body is skipped),
//! and a document boundary is a top-level `name:`/`on:`/`run-name:` key after a
//! `jobs:` key. Both fail toward "no structural finding", never toward a false one.

/// One meaningful line: comments stripped, blank lines dropped.
#[derive(Debug, Clone)]
pub(super) struct Ln {
    /// 1-based line number in the source.
    pub(super) no: usize,
    /// Leading spaces.
    pub(super) indent: usize,
    /// Trimmed text, comment removed.
    pub(super) text: String,
}

/// The `permissions:` of a workflow or of a job.
#[derive(Debug, Clone, Default)]
pub(super) struct Perms {
    /// `permissions: write-all`.
    write_all: bool,
    /// `(scope, level)` pairs, lowercase (`("id-token", "write")`).
    entries: Vec<(String, String)>,
    /// 1-based line of the `permissions:` key.
    pub(super) line: usize,
}

impl Perms {
    /// The token can mint an OIDC identity (`id-token: write` or `write-all`).
    pub(super) fn id_token_write(&self) -> bool {
        self.write_all
            || self
                .entries
                .iter()
                .any(|(scope, level)| scope == "id-token" && level == "write")
    }

    /// Any scope is writable.
    pub(super) fn any_write(&self) -> bool {
        self.write_all || self.entries.iter().any(|(_, level)| level == "write")
    }
}

/// One `- uses:` / `- run:` step.
#[derive(Debug, Clone, Default)]
pub(super) struct Step {
    /// 1-based line of the step's dash.
    pub(super) line: usize,
    /// The `uses:` value (`actions/checkout@v4`).
    pub(super) uses: Option<String>,
    /// The `with:` mapping, flat.
    pub(super) with: Vec<(String, String)>,
    /// Every cleaned line of the step, joined — what a `run:` block executes.
    pub(super) text: String,
}

/// One job under `jobs:`.
#[derive(Debug, Clone, Default)]
pub(super) struct Job {
    /// The job key.
    pub(super) id: String,
    /// The job's own `permissions:` (it REPLACES the workflow's, it does not merge).
    pub(super) permissions: Option<Perms>,
    /// The steps, in order.
    pub(super) steps: Vec<Step>,
}

/// One workflow document.
#[derive(Debug, Clone, Default)]
pub(super) struct Workflow {
    /// Top-level `permissions:` — inherited by every job without its own.
    pub(super) permissions: Option<Perms>,
    /// `on:` lists `pull_request_target`.
    pub(super) pull_request_target: bool,
    /// The jobs.
    pub(super) jobs: Vec<Job>,
}

/// A quote only protects a `#` when it OPENS a scalar: after `:`, `-`, `,`, `[`,
/// `{` or at the start of the line.
fn opens_scalar(prev_sig: Option<u8>) -> bool {
    matches!(prev_sig, None | Some(b':' | b'-' | b',' | b'[' | b'{'))
}

/// Offset just past the quoted scalar that opens at `b[open_at]`, or `None` when it
/// never closes (the rest of the line is then inside it).
fn skip_quoted(b: &[u8], open_at: usize) -> Option<usize> {
    let quote = b[open_at];
    let mut i = open_at + 1;
    while i < b.len() {
        match b[i] {
            b'\\' if quote == b'"' => i += 2,
            ch if ch == quote && quote == b'\'' && b.get(i + 1) == Some(&b'\'') => i += 2,
            ch if ch == quote => return Some(i + 1),
            _ => i += 1,
        }
    }
    None
}

/// Byte offset where the YAML comment of `line` starts, if it has one.
///
/// A comment is a `#` at the start of the line or after whitespace. A quote only
/// protects a `#` when it OPENS a scalar ([`opens_scalar`]) -- a plain scalar such
/// as `echo "it's # x"` ends at ` #`, apostrophe or not.
fn comment_start(line: &str) -> Option<usize> {
    let b = line.as_bytes();
    let mut prev_sig: Option<u8> = None;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if matches!(c, b'"' | b'\'') && opens_scalar(prev_sig) {
            let next = skip_quoted(b, i)?;
            prev_sig = Some(c);
            i = next;
            continue;
        }
        if c == b'#' && (i == 0 || b[i - 1].is_ascii_whitespace()) {
            return Some(i);
        }
        if !c.is_ascii_whitespace() {
            prev_sig = Some(c);
        }
        i += 1;
    }
    None
}

/// Byte spans (`start..end`, sorted, disjoint) of the YAML comments in `source`.
pub(super) fn yaml_comment_regions(source: &str) -> Vec<(usize, usize)> {
    let mut regions = Vec::new();
    let mut offset = 0;
    for raw in source.split_inclusive('\n') {
        let line = raw.trim_end_matches(['\n', '\r']);
        if let Some(at) = comment_start(line) {
            regions.push((offset + at, offset + line.len()));
        }
        offset += raw.len();
    }
    regions
}

/// The meaningful lines of `source`.
fn clean_lines(source: &str) -> Vec<Ln> {
    source
        .lines()
        .enumerate()
        .filter_map(|(idx, raw)| {
            let code = match comment_start(raw) {
                Some(at) => &raw[..at],
                None => raw,
            };
            let text = code.trim();
            if text.is_empty() {
                return None;
            }
            Some(Ln {
                no: idx + 1,
                indent: code.len() - code.trim_start().len(),
                text: text.to_string(),
            })
        })
        .collect()
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    for q in ['"', '\''] {
        if s.len() >= 2 && s.starts_with(q) && s.ends_with(q) {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

/// `key: value` split at the first colon that ends the key (`:` then space/end).
fn split_kv(text: &str) -> Option<(String, String)> {
    let b = text.as_bytes();
    let at = (0..b.len())
        .find(|&i| b[i] == b':' && b.get(i + 1).is_none_or(|next| next.is_ascii_whitespace()))?;
    let key = unquote(&text[..at]);
    if key.is_empty() {
        return None;
    }
    Some((key, unquote(&text[at + 1..])))
}

fn is_dash(text: &str) -> bool {
    text == "-" || text.starts_with("- ")
}

/// Index one past the last line that belongs to the block opened at `lines[i]`:
/// the deeper lines, plus a sequence written at the key's own indent
/// (`steps:` followed by `- uses:` at the same column — valid YAML).
fn block_end(lines: &[Ln], i: usize) -> usize {
    let indent = lines[i].indent;
    let mut j = i + 1;
    while j < lines.len()
        && (lines[j].indent > indent || (lines[j].indent == indent && is_dash(&lines[j].text)))
    {
        j += 1;
    }
    j
}

fn parse_perms(block: &[Ln], inline: &str) -> Perms {
    let mut perms = Perms {
        line: block.first().map_or(0, |l| l.no),
        ..Perms::default()
    };
    let value = inline.trim();
    let mut add = |pair: Option<(String, String)>| {
        if let Some((scope, level)) = pair {
            perms
                .entries
                .push((scope.to_ascii_lowercase(), level.to_ascii_lowercase()));
        }
    };
    if value.is_empty() {
        for child in block.iter().skip(1) {
            add(split_kv(&child.text));
        }
    } else if value.starts_with('{') {
        for part in value.trim_matches(['{', '}']).split(',') {
            add(split_kv(part.trim()));
        }
    } else {
        perms.write_all = value.eq_ignore_ascii_case("write-all");
    }
    perms
}

fn parse_map(lines: &[Ln]) -> Vec<(String, String)> {
    let Some(first) = lines.first() else {
        return Vec::new();
    };
    lines
        .iter()
        .filter(|l| l.indent == first.indent)
        .filter_map(|l| split_kv(&l.text))
        .collect()
}

fn parse_step(item: &[Ln]) -> Step {
    let first = &item[0];
    let rest = first.text.trim_start_matches('-').trim_start();
    let mut lines: Vec<Ln> = Vec::with_capacity(item.len());
    if !rest.is_empty() {
        lines.push(Ln {
            no: first.no,
            indent: first.indent + (first.text.len() - rest.len()),
            text: rest.to_string(),
        });
    }
    lines.extend(item.iter().skip(1).cloned());
    let mut step = Step {
        line: first.no,
        text: lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        ..Step::default()
    };
    let Some(key_indent) = lines.first().map(|l| l.indent) else {
        return step;
    };
    let mut k = 0;
    while k < lines.len() {
        if lines[k].indent != key_indent {
            k += 1;
            continue;
        }
        let end = block_end(&lines, k);
        if let Some((key, value)) = split_kv(&lines[k].text) {
            match key.as_str() {
                "uses" => step.uses = Some(value),
                "with" => step.with = parse_map(&lines[k + 1..end]),
                _ => {}
            }
        }
        k = end;
    }
    step
}

fn parse_steps(lines: &[Ln]) -> Vec<Step> {
    let Some(first) = lines.first() else {
        return Vec::new();
    };
    let item_indent = first.indent;
    let mut steps = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].indent == item_indent && is_dash(&lines[i].text) {
            let mut j = i + 1;
            while j < lines.len() && lines[j].indent > item_indent {
                j += 1;
            }
            steps.push(parse_step(&lines[i..j]));
            i = j;
        } else {
            i += 1;
        }
    }
    steps
}

fn parse_jobs(lines: &[Ln]) -> Vec<Job> {
    let Some(first) = lines.first() else {
        return Vec::new();
    };
    let job_indent = first.indent;
    let mut jobs = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let end = block_end(lines, i);
        let header = split_kv(&lines[i].text);
        if lines[i].indent == job_indent
            && let Some((id, value)) = header
            && value.is_empty()
        {
            jobs.push(parse_job(id, &lines[i + 1..end]));
        }
        i = end;
    }
    jobs
}

fn parse_job(id: String, body: &[Ln]) -> Job {
    let mut job = Job {
        id,
        ..Job::default()
    };
    let Some(first) = body.first() else {
        return job;
    };
    let mut k = 0;
    while k < body.len() {
        let end = block_end(body, k);
        if body[k].indent == first.indent
            && let Some((key, value)) = split_kv(&body[k].text)
        {
            match key.as_str() {
                "permissions" => job.permissions = Some(parse_perms(&body[k..end], &value)),
                "steps" => job.steps = parse_steps(&body[k + 1..end]),
                _ => {}
            }
        }
        k = end;
    }
    job
}

/// Whether the `on:` block (inline value or children) mentions `needle`.
fn section_mentions(block: &[Ln], inline: &str, needle: &str) -> bool {
    inline.contains(needle) || block.iter().skip(1).any(|l| l.text.contains(needle))
}

/// The workflow documents in `source` (a file, or several concatenated).
pub(super) fn parse_workflows(source: &str) -> Vec<Workflow> {
    let lines = clean_lines(source);
    let mut docs: Vec<Workflow> = Vec::new();
    let mut current = Workflow::default();
    let mut seen_jobs = false;
    let mut i = 0;
    while i < lines.len() {
        let end = block_end(&lines, i);
        let top = lines[i].indent == 0 && !is_dash(&lines[i].text);
        let Some((key, value)) = top.then(|| split_kv(&lines[i].text)).flatten() else {
            i = end;
            continue;
        };
        if seen_jobs && matches!(key.as_str(), "name" | "on" | "run-name") {
            docs.push(std::mem::take(&mut current));
            seen_jobs = false;
        }
        match key.as_str() {
            "permissions" => current.permissions = Some(parse_perms(&lines[i..end], &value)),
            "on" => {
                current.pull_request_target |=
                    section_mentions(&lines[i..end], &value, "pull_request_target");
            }
            "jobs" => {
                seen_jobs = true;
                current.jobs.extend(parse_jobs(&lines[i + 1..end]));
            }
            _ => {}
        }
        i = end;
    }
    docs.push(current);
    docs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comment_start_respects_scalar_quotes_not_shell_quotes() {
        assert_eq!(comment_start("a: b # c"), Some(5));
        assert_eq!(comment_start("# whole line"), Some(0));
        assert_eq!(comment_start("a: \"x # y\" # z"), Some(11));
        assert_eq!(comment_start("a: 'it''s # y'"), None);
        // A plain scalar ends at ` #` even when a shell quote is open.
        assert_eq!(comment_start("run: echo \"it's # x\""), Some(16));
        // `#` glued to a word is content (a URL fragment), not a comment.
        assert_eq!(comment_start("url: http://h/p#frag"), None);
    }

    #[test]
    fn yaml_comment_regions_cover_only_the_comment() {
        let src = "a: 1 # one\nb: 2\n# three\n";
        let regions = yaml_comment_regions(src);
        assert_eq!(regions, vec![(5, 10), (16, 23)]);
        assert_eq!(&src[regions[0].0..regions[0].1], "# one");
    }

    #[test]
    fn parses_nested_workflow_permissions_and_steps() {
        let wf = parse_workflows(
            r#"name: Release
on:
  push:
    tags: ['v*']
  pull_request_target:
permissions:
  contents: write
  id-token: write
jobs:
  build:
    runs-on: ubuntu-latest
    permissions:
      contents: read
    steps:
      - uses: actions/checkout@v4
        with:
          ref: ${{ github.event.pull_request.head.sha }}
          persist-credentials: false
      - run: cargo build
"#,
        );
        assert_eq!(wf.len(), 1);
        let w = &wf[0];
        assert!(w.pull_request_target);
        assert!(w.permissions.as_ref().is_some_and(Perms::id_token_write));
        assert_eq!(w.jobs.len(), 1);
        let job = &w.jobs[0];
        assert_eq!(job.id, "build");
        assert!(!job.permissions.as_ref().is_some_and(Perms::any_write));
        assert_eq!(job.steps.len(), 2);
        assert_eq!(job.steps[0].uses.as_deref(), Some("actions/checkout@v4"));
        assert!(
            job.steps[0]
                .with
                .contains(&("persist-credentials".into(), "false".into()))
        );
        assert!(job.steps[1].text.contains("cargo build"));
    }

    #[test]
    fn a_sequence_at_the_key_indent_is_still_the_steps_block() {
        let wf = parse_workflows(
            "on: push\njobs:\n  b:\n    steps:\n    - uses: a/b@v1\n    - run: cargo test\n",
        );
        assert_eq!(wf[0].jobs[0].steps.len(), 2);
        assert_eq!(wf[0].jobs[0].steps[0].uses.as_deref(), Some("a/b@v1"));
    }

    #[test]
    fn flow_style_permissions_are_read() {
        let wf = parse_workflows(
            "permissions: { contents: read, id-token: write }\njobs:\n  b:\n    steps:\n      - run: x\n",
        );
        assert!(
            wf[0]
                .permissions
                .as_ref()
                .is_some_and(Perms::id_token_write)
        );
        let all =
            parse_workflows("permissions: write-all\njobs:\n  b:\n    steps:\n      - run: x\n");
        assert!(
            all[0]
                .permissions
                .as_ref()
                .is_some_and(Perms::id_token_write)
        );
        let ro =
            parse_workflows("permissions: read-all\njobs:\n  b:\n    steps:\n      - run: x\n");
        assert!(!ro[0].permissions.as_ref().is_some_and(Perms::any_write));
    }

    #[test]
    fn concatenated_files_do_not_leak_permissions_into_the_next_document() {
        let wf = parse_workflows(
            "name: A\npermissions:\n  id-token: write\njobs:\n  a:\n    steps:\n      - run: x\n\nname: B\njobs:\n  b:\n    steps:\n      - run: y\n",
        );
        assert_eq!(wf.len(), 2);
        assert!(wf[0].permissions.is_some());
        assert!(wf[1].permissions.is_none(), "B declares no permissions");
        assert_eq!(wf[1].jobs[0].id, "b");
    }

    #[test]
    fn a_bare_step_list_has_no_jobs() {
        let wf = parse_workflows("- uses: actions/checkout@v4\n- run: cargo build\n");
        assert!(wf.iter().all(|w| w.jobs.is_empty()));
    }

    #[test]
    fn a_commented_out_permissions_block_is_not_read() {
        let wf = parse_workflows(
            "# permissions:\n#   id-token: write\njobs:\n  b:\n    steps:\n      - run: x\n",
        );
        assert!(wf[0].permissions.is_none());
    }
}
