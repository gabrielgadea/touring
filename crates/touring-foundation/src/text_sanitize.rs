// touring-quality:allow-secrets — the tests below embed synthetic-secret
// fixtures on purpose (assembled at runtime, never a real credential), the
// detect-secrets / gitleaks `pragma: allowlist secret` convention.
//! Text sanitization shared by every writer and reader of free text that is
//! later replayed to a model as context (memory store/recall, sandbox output).
//!
//! Two independent defences, one module so they can never drift apart:
//!
//! 1. **Secrets** — [`redact_secrets`] (PEM blocks, credential env-var names,
//!    GitHub/OpenAI/AWS/Slack token formats). Moved here from
//!    `touring-ceg::gateway::sandbox_executor` (which re-exports it) so that
//!    crates *below* the CEG — `touring-intelligence`'s `RlmMemory`, the unified
//!    memory write path — can use the ONE redactor instead of growing a second.
//! 2. **Invisible / control characters** — [`strip_invisible`] removes bidi
//!    overrides and isolates, zero-width characters, the Unicode Tag block and
//!    C0 controls. These render as nothing (or reorder the visible text) yet are
//!    read by a model, which makes them a carrier for hidden instructions.
//!
//! [`sanitize_memory_text`] composes both (invisibles first, so a zero-width
//! character spliced into a token cannot hide it from the token patterns) and
//! reports what it changed, so a caller can tell the user instead of silently
//! altering their text.

use std::borrow::Cow;
use std::sync::LazyLock;

use regex::Regex;

/// Token-format secret patterns, matched ANYWHERE in a line. The env-var-name
/// pass in [`redact_secrets`] only catches `KEY=value`; these catch a raw token
/// value with no surrounding key (e.g. a GitHub PAT pasted into an error
/// message, or an OpenAI key in an `Authorization` header).
static SECRET_TOKEN_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"gh[pousr]_[A-Za-z0-9_]{20,}", // GitHub PAT / OAuth / refresh / server / user
        r"sk-[A-Za-z0-9_-]{20,}",       // OpenAI (incl. sk-proj-…)
        r"AKIA[0-9A-Z]{16}",            // AWS access key id
        r"xox[baprs]-[A-Za-z0-9-]{10,}", // Slack bot/app/user/refresh/legacy tokens
    ]
    .iter()
    .filter_map(|p| Regex::new(p).ok())
    .collect()
});

/// Credential env-var names whose `KEY=value` / `KEY: value` value is redacted.
const SECRET_ENV_NEEDLES: &[&str] = &[
    "GH_TOKEN",
    "GITHUB_TOKEN",
    "AWS_SECRET_ACCESS_KEY",
    "AWS_ACCESS_KEY_ID",
    "AWS_SESSION_TOKEN",
    "ANTHROPIC_API_KEY",
    "OPENAI_API_KEY",
    "NPM_TOKEN",
];

/// Redact secret-like material from text before it is stored, logged, or
/// surfaced to the LLM. Three passes:
/// 1. **PEM private-key blocks** — `-----BEGIN … PRIVATE KEY-----` … `-----END …`
///    are dropped wholesale (header, base64 body, and footer).
/// 2. **Env-var name pass** — a line containing a known credential var name
///    (`GITHUB_TOKEN`, `AWS_SECRET_ACCESS_KEY`, …) has its `KEY=value` / `KEY: value`
///    value replaced (catches secrets whose value has no recognizable token format).
/// 3. **Token-format pass** — `SECRET_TOKEN_PATTERNS` redact a raw token
///    value (GitHub/OpenAI/AWS/Slack) anywhere in the remaining text.
///
/// Idempotent: redacting already-redacted text returns it unchanged.
pub fn redact_secrets(s: &str) -> String {
    redact_secrets_counted(s).0
}

/// Passes 2 and 3 over a single line outside a PEM block, with the number of
/// redactions applied. A line whose env-var value is already `[REDACTED]`
/// rebuilds to itself and counts zero.
fn redact_line(line: &str) -> (Cow<'_, str>, usize) {
    let mut redactions = 0usize;
    let mut redacted: Cow<'_, str> = Cow::Borrowed(line);
    if SECRET_ENV_NEEDLES.iter().any(|n| line.contains(n))
        && let Some(eq_idx) = line.find(['=', ':'])
    {
        let replaced = format!("{} [REDACTED]", &line[..=eq_idx]);
        if replaced != line {
            redactions += 1;
        }
        redacted = Cow::Owned(replaced);
    }
    for re in SECRET_TOKEN_PATTERNS.iter() {
        let hits = re.find_iter(&redacted).count();
        if hits > 0 {
            redactions += hits;
            redacted = Cow::Owned(re.replace_all(&redacted, "[REDACTED]").into_owned());
        }
    }
    (redacted, redactions)
}

/// [`redact_secrets`] plus the number of redactions it performed: one per PEM
/// block, one per line whose env-var value was replaced, and one per token
/// match. A line that was already redacted counts zero (the replacement is
/// identical), which is what makes a second pass over clean text report `0`.
pub fn redact_secrets_counted(s: &str) -> (String, usize) {
    let mut out = String::with_capacity(s.len());
    let mut redactions = 0usize;
    let mut in_pem = false;
    for line in s.lines() {
        // Pass 1: PEM private-key block — drop BEGIN, base64 body, and END.
        if line.contains("-----BEGIN") && line.contains("PRIVATE KEY") {
            in_pem = true;
            out.push_str("[REDACTED PEM PRIVATE KEY]\n");
            redactions += 1;
            continue;
        }
        if in_pem {
            if line.contains("-----END") && line.contains("PRIVATE KEY") {
                in_pem = false;
            }
            continue;
        }
        let (redacted, n) = redact_line(line);
        redactions += n;
        out.push_str(&redacted);
        out.push('\n');
    }
    // Preserve final-newline behaviour: if s ended without \n, drop the one
    // we appended at the end of the last iteration.
    if !s.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }
    (out, redactions)
}

/// True for a character that renders as nothing, or reorders the visible
/// text, and so can carry instructions a human reviewer never sees.
///
/// - U+200B..=U+200F — zero-width space/joiners, LRM, RLM
/// - U+202A..=U+202E — bidi embeddings and overrides
/// - U+2066..=U+2069 — bidi isolates
/// - U+E0000..=U+E007F — the Tag block (invisible "ASCII smuggling")
/// - C0 controls except `\n` and `\t`
///
/// U+FEFF is handled by [`strip_invisible`], not here: a byte-order mark at
/// the very start of a text is legitimate, in the middle it is not.
fn is_invisible(c: char) -> bool {
    matches!(
        c,
        '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
            | '\u{E0000}'..='\u{E007F}'
    ) || (c < '\u{20}' && c != '\n' && c != '\t')
}

/// Removes invisible and control characters (see [`is_invisible`]) and any
/// U+FEFF that is not the first character. Returns the cleaned text and how
/// many characters were removed; borrows the input when there is nothing to
/// remove.
pub fn strip_invisible(s: &str) -> (Cow<'_, str>, usize) {
    let dirty = |&(i, c): &(usize, char)| is_invisible(c) || (c == '\u{FEFF}' && i != 0);
    let removed = s.char_indices().filter(dirty).count();
    if removed == 0 {
        return (Cow::Borrowed(s), 0);
    }
    let cleaned: String = s
        .char_indices()
        .filter(|ic| !dirty(ic))
        .map(|(_, c)| c)
        .collect();
    (Cow::Owned(cleaned), removed)
}

/// Outcome of [`sanitize_memory_text`]: the cleaned text and what was altered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sanitized {
    /// The text after invisible-character removal and secret redaction.
    pub text: String,
    /// Number of secret redactions applied (PEM block / env value / token).
    pub secrets: usize,
    /// Number of invisible or control characters removed.
    pub invisible: usize,
}

impl Sanitized {
    /// True when the text was altered in any way.
    pub fn changed(&self) -> bool {
        self.secrets > 0 || self.invisible > 0
    }

    /// `{"secrets": n, "invisible": m}` — the shape the store responses carry.
    /// Always built (zeros when nothing changed): an absent key would be
    /// ambiguous between "nothing to clean" and "an older daemon".
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({ "secrets": self.secrets, "invisible": self.invisible })
    }
}

/// Sanitizes free text headed for (or coming from) model-visible storage:
/// strips invisible/control characters first, then redacts secrets, so a
/// zero-width character spliced into a token cannot hide it from the token
/// patterns. Idempotent.
pub fn sanitize_memory_text(s: &str) -> Sanitized {
    let (stripped, invisible) = strip_invisible(s);
    let (text, secrets) = redact_secrets_counted(&stripped);
    Sanitized {
        text,
        secrets,
        invisible,
    }
}

/// Sanitizes every string leaf of a JSON value in place (object keys are left
/// alone) and returns `(secrets, invisible)` totals. Used on the READ side:
/// recall output is cleaned before it is handed back as context, without
/// rewriting the database.
pub fn sanitize_json_strings(v: &mut serde_json::Value) -> (usize, usize) {
    let add = |acc: (usize, usize), c: (usize, usize)| (acc.0 + c.0, acc.1 + c.1);
    match v {
        serde_json::Value::String(s) => {
            let out = sanitize_memory_text(s);
            let counts = (out.secrets, out.invisible);
            if out.changed() {
                *s = out.text;
            }
            counts
        }
        serde_json::Value::Array(items) => items
            .iter_mut()
            .fold((0, 0), |acc, item| add(acc, sanitize_json_strings(item))),
        serde_json::Value::Object(map) => map
            .values_mut()
            .fold((0, 0), |acc, item| add(acc, sanitize_json_strings(item))),
        _ => (0, 0),
    }
}

/// [`sanitize_json_strings`] over a slice of values (a recall/query result
/// list), returning the summed `(secrets, invisible)`.
pub fn sanitize_json_items(items: &mut [serde_json::Value]) -> (usize, usize) {
    items.iter_mut().fold((0, 0), |acc, item| {
        let c = sanitize_json_strings(item);
        (acc.0 + c.0, acc.1 + c.1)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic OpenAI-shaped key assembled at runtime so no literal in the
    /// source matches the secret detectors.
    fn fake_openai_key() -> String {
        ["sk", "-", "aB3dE6gH9jK2mN5pQ8sTuV1w"].concat()
    }

    /// A synthetic GitHub-PAT-shaped token assembled at runtime.
    fn fake_github_token() -> String {
        ["gh", "p_", "0123456789abcdefghijklmnopqrstuv"].concat()
    }

    #[test]
    fn redacts_token_without_env_name_and_counts_it() {
        let key = fake_openai_key();
        let (out, n) = redact_secrets_counted(&format!("use {key} for the call"));
        assert!(!out.contains(&key), "token leaked: {out}");
        assert!(out.contains("[REDACTED]"));
        assert_eq!(n, 1);
    }

    #[test]
    fn redaction_is_idempotent_and_second_pass_counts_zero() {
        let text = format!(
            "GH_TOKEN={}\nAWS_SECRET_ACCESS_KEY: wxyz\n-----BEGIN RSA PRIVATE KEY-----\nQUJD\n-----END RSA PRIVATE KEY-----\n",
            fake_github_token()
        );
        let (once, n1) = redact_secrets_counted(&text);
        let (twice, n2) = redact_secrets_counted(&once);
        assert_eq!(n1, 3, "env value, env value, PEM block: {once}");
        assert_eq!(once, twice);
        assert_eq!(n2, 0, "second pass must count nothing");
    }

    #[test]
    fn plain_text_is_untouched_including_non_ascii() {
        // Latin accents, an emoji and CJK (escaped: the source stays Latin).
        let s = "Ol\u{e1}, mundo!\n\tA\u{e7}\u{e3}o: \u{65E5}\u{672C}\u{8A9E} e emoji \u{1F642} ok.\nsegunda linha";
        let out = sanitize_memory_text(s);
        assert_eq!(out.text, s);
        assert!(!out.changed());
    }

    #[test]
    fn strips_bidi_zero_width_tag_block_and_c0_controls() {
        let s =
            "a\u{202E}b\u{200B}c\u{2066}d\u{2069}e\u{E0041}f\u{0007}g\u{001B}h\u{FEFF}i\n\tkeep";
        let (out, n) = strip_invisible(s);
        assert_eq!(out, "abcdefghi\n\tkeep");
        assert_eq!(n, 8);
    }

    #[test]
    fn leading_bom_is_kept_but_inner_bom_is_removed() {
        let (out, n) = strip_invisible("\u{FEFF}start\u{FEFF}end");
        assert_eq!(out, "\u{FEFF}startend");
        assert_eq!(n, 1);
    }

    #[test]
    fn zero_width_inside_a_token_does_not_hide_it() {
        let key = fake_openai_key();
        let split = format!("{}\u{200B}{}", &key[..10], &key[10..]);
        let out = sanitize_memory_text(&split);
        assert_eq!(out.invisible, 1);
        assert_eq!(out.secrets, 1);
        assert!(
            !out.text.contains(&key[10..]),
            "token survived: {}",
            out.text
        );
    }

    #[test]
    fn json_strings_are_sanitized_recursively_and_keys_left_alone() {
        let key = fake_openai_key();
        let mut v = serde_json::json!({
            "entries": [{"key": "k", "value": format!("tok {key}")}],
            "note": "a\u{202E}b",
            "n": 3,
        });
        let (secrets, invisible) = sanitize_json_strings(&mut v);
        assert_eq!((secrets, invisible), (1, 1));
        assert_eq!(v["note"], "ab");
        assert!(!v.to_string().contains(&key));
        assert_eq!(v["n"], 3);
    }

    #[test]
    fn json_items_sum_the_counts_over_the_list() {
        let key = fake_openai_key();
        let mut items = vec![
            serde_json::json!({"value": format!("a {key}")}),
            serde_json::json!({"value": "clean"}),
            serde_json::json!({"value": "x\u{200B}y"}),
        ];
        assert_eq!(sanitize_json_items(&mut items), (1, 1));
        assert_eq!(items[1]["value"], "clean");
        assert_eq!(items[2]["value"], "xy");
    }
}
