//! F2.4 — Cryptographic Issues / Hardcoded Secrets verifier.
//!
//! Detects hardcoded secrets via several signals, ordered by confidence:
//!   1. **Known provider token markers** — GitHub (`ghp_`/…), Slack (`xoxb-`/…),
//!      Stripe (`sk_live_`), AWS (`AKIA`/`ASIA`), Google (`AIza`), DigitalOcean
//!      (`dop_v1_`/…), GitLab (`glpat-`), Shopify/Square/PyPI, PEM private keys.
//!   2. **Structural provider tokens** — JWTs (`eyJ….….…`), SendGrid (`SG.….…`),
//!      OpenAI/Anthropic (`sk-proj-…`/`sk-ant-…`), Slack-app (`xapp-…`), checked
//!      as whole tokens so kebab-case identifiers never false-positive.
//!   3. **Connection-string credentials** — `scheme://user:password@host` (literal
//!      password only; `$VAR`/`${VAR}` interpolations are not flagged).
//!   4. **Secret-named assignments** — `password = "…"`, `"api_key": "…"` (quoted),
//!      plus UNQUOTED `.env`/YAML `API_KEY=<hex|high-entropy>` (name context relaxes
//!      the entropy floor to 3.5 and accepts hex, gitleaks-style).
//!   5. **Generic high-entropy literals** — a quoted value with len ≥ 24, no whitespace,
//!      mixed alphanumeric, Shannon entropy ≥ 4.5 (skips hex hashes & `arn:`/URL/markup).
//!
//! Strong signal → Fail (0.0 → PreToolUse BLOCK). A bare secret keyword used as a real code
//! identifier with no assigned value → Warn (0.5, surfaced but not blocking). The weak scan runs
//! on a code-only projection (string literals + comments blanked), so a keyword appearing only in
//! prose — a single- or multi-line string literal, or a comment — never fires it. Nothing → Pass (1.0).

use crate::verifications::{Verification, auto_remediation};
use crate::{DimId, DimScore, DimStatus};
use anyhow::Result;
use std::path::Path;

/// F2.4 verifier — Cryptographic Issues / Hardcoded Secrets.
#[allow(non_camel_case_types)]
pub struct F2_4_Secrets;

/// High-confidence secret token markers (provider-specific prefixes + PEM headers).
const STRONG_MARKERS: &[&str] = &[
    // GitHub
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    // Slack
    "xoxb-",
    "xoxp-",
    "xoxa-",
    "xoxr-",
    "xoxs-",
    // Stripe (live secrets)
    "sk_live_",
    "rk_live_",
    // AWS access key id
    "AKIA",
    "ASIA",
    // Google API key
    "AIza",
    // DigitalOcean PATs / OAuth / refresh tokens
    "dop_v1_",
    "doo_v1_",
    "dor_v1_",
    // GitLab personal access token
    "glpat-",
    // Shopify (admin / storefront / custom-app)
    "shpat_",
    "shpss_",
    "shpca_",
    // Square (access / OAuth)
    "sq0atp-",
    "sq0csp-",
    // PyPI upload token (macaroon — constant `AgEI` prefix → near-zero FP)
    "pypi-AgEI",
    // PEM private-key blocks
    "-----BEGIN PRIVATE",
    "-----BEGIN RSA",
    "-----BEGIN OPENSSH",
    "-----BEGIN EC PRIVATE",
    "-----BEGIN SEC1 PRIVATE",
    "-----BEGIN ENCRYPTED PRIVATE",
    "-----BEGIN DSA",
    "-----BEGIN PGP PRIVATE",
];

/// Identifier fragments that name a secret when used as an assignment target.
const SECRET_KEYWORDS: &[&str] = &[
    "secret",
    "password",
    "passwd",
    "pwd",
    "token",
    "api_key",
    "apikey",
    "access_key",
    "private_key",
    "client_secret",
    "auth_token",
    "credential",
];

/// Shannon entropy (bits per byte) of a string.
fn shannon_entropy(s: &str) -> f64 {
    if s.is_empty() {
        return 0.0;
    }
    let mut counts = [0usize; 256];
    for &b in s.as_bytes() {
        counts[b as usize] += 1;
    }
    let len = s.len() as f64;
    counts
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = c as f64 / len;
            -p * p.log2()
        })
        .sum()
}

/// A value that "looks like" a secret: long, no whitespace, mixed alphanumeric, high entropy.
/// The entropy ≥ 4.5 bar skips hex hashes (entropy ≤ 4.0) while catching base62/base64 tokens.
/// Structural characters that mark a value as a URL / markup / format-template
/// rather than an opaque secret token. Real API tokens/keys — including base64
/// (`+`/`/`/`=`), hex, and provider-prefixed keys — never contain these, so
/// excluding them removes URL / HTML / format-string false positives on the
/// generic-entropy path WITHOUT weakening detection of real secrets (provider
/// markers and secret-named assignments are checked on separate paths).
fn has_non_secret_markers(v: &str) -> bool {
    // `%` is intentionally NOT excluded — real passwords/secrets frequently contain
    // it, whereas printf-style templates are caught by the whitespace/length/entropy
    // checks. Markup (`<>{}`), URL scheme (`://`) and escape (`\`) never appear in
    // opaque tokens. `arn:` prefixes are AWS resource identifiers (not secrets) whose
    // random-looking resource tails would otherwise trip the entropy path.
    // Filesystem-path prefixes are resource locations, not opaque tokens; their
    // date/version tails (`/tmp/SINAPI_Custos_PB_122024_Desonerado.xlsx`, first
    // hit 2026-08-02) otherwise trip the entropy path. Base64 can start with
    // `/` (1/64 of random tokens), but a leaked key of that shape still lands
    // on the provider-prefix and secret-named paths, so detection holds.
    v.starts_with("arn:")
        || v.contains("://")
        || v.starts_with('/')
        || v.starts_with("./")
        || v.starts_with("../")
        || v.starts_with("~/")
        || v.bytes()
            .any(|b| matches!(b, b'<' | b'>' | b'{' | b'}' | b'\\'))
        || is_predictable_sequence(v)
        || is_email_like(v)
}

/// True when `v` is dominated by consecutive character runs (`abc…`, `012…`).
///
/// Shannon entropy measures symbol *distribution*, not predictability, so a
/// character-set literal like `"abcdefghij…XYZ0123456789_"` scores ~5.98 bits —
/// well past the 4.5 secret floor — despite carrying essentially no randomness:
/// every character appears exactly once, which is precisely the case that
/// MAXIMISES Shannon entropy. Kolmogorov complexity is the honest measure, and
/// a monotonic run is its minimum. This is the cheap deterministic proxy for it:
/// count adjacent pairs that step by exactly +1.
///
/// The bar is 80%, not a bare majority. Measured on the two cases that matter:
///
/// * the alphabet literal — 59/62 pairs = 95% (excluded, correct)
/// * `abcDEF123ghiJKL456mno…+/=Xy` — 20/35 pairs = 57% (kept, correct)
///
/// The second is the synthetic base64 token in `test_base64_token_still_blocks`;
/// it is built from short runs (`abc`, `DEF`, `123`), so a 50% bar swallowed it
/// and silently weakened real detection — that test caught the regression.
/// Random tokens sit near 1/62, so 80% keeps an ample margin on both sides.
///
/// Origin: 2026-08-02 — the alphabet literal in
/// `touring-identity/tests/property_tests.rs` scored 0.000 and blocked every
/// edit to that file, including ones that never touched the literal.
fn is_predictable_sequence(v: &str) -> bool {
    let b = v.as_bytes();
    if b.len() < 8 {
        return false;
    }
    let pairs = b.len() - 1;
    let consecutive = b
        .windows(2)
        .filter(|w| w[1] == w[0].wrapping_add(1))
        .count();
    // >= 80% of adjacent pairs ascending by one => generated run, not a secret.
    consecutive * 5 >= pairs * 4
}

/// True when `v` is a plain e-mail address.
///
/// Addresses identify people and bots; they are not credentials. A long one
/// nonetheless clears the length and entropy bars —
/// `41898282+github-actions[bot]@users.noreply.github.com`, the documented
/// public identity of GitHub's Actions bot, did exactly that on 2026-08-02 and
/// blocked a workflow edit.
///
/// Deliberately narrow: any `:` disqualifies, so `user:password@host`
/// connection strings stay on the credential path (`has_connstring_creds`).
fn is_email_like(v: &str) -> bool {
    if v.contains(':') {
        return false;
    }
    let Some((local, domain)) = v.split_once('@') else {
        return false;
    };
    if local.is_empty() || domain.contains('@') {
        return false;
    }
    // Domain must be host.tld, alphabetic TLD, no exotic characters.
    match domain.rsplit_once('.') {
        Some((host, tld)) => {
            !host.is_empty()
                && tld.len() >= 2
                && tld.bytes().all(|b| b.is_ascii_alphabetic())
                && domain
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'))
        }
        None => false,
    }
}

fn looks_like_secret_value(v: &str) -> bool {
    if v.len() < 24 || v.chars().any(|c| c.is_whitespace()) {
        return false;
    }
    if has_non_secret_markers(v) {
        return false;
    }
    let has_digit = v.bytes().any(|b| b.is_ascii_digit());
    let has_alpha = v.bytes().any(|b| b.is_ascii_alphabetic());
    has_digit && has_alpha && shannon_entropy(v) >= 4.5
}

/// A JWT (`header.payload.signature`): exactly three base64url segments whose
/// header starts with `eyJ` (= base64url of `{"`). The structural shape (two
/// dots, `eyJ`-prefixed ≥8-char header, ≥8-char payload, non-empty signature)
/// makes false positives on benign base64 effectively impossible.
fn is_jwt(tok: &str) -> bool {
    let parts: Vec<&str> = tok.split('.').collect();
    parts.len() == 3
        && parts[0].starts_with("eyJ")
        && parts[0].len() >= 8
        && parts[1].len() >= 8
        && !parts[2].is_empty()
        && parts.iter().all(|seg| {
            seg.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
}

/// A SendGrid API key: `SG.<16+ base64url>.<16+ base64url>`.
fn is_sendgrid(tok: &str) -> bool {
    let Some(rest) = tok.strip_prefix("SG.") else {
        return false;
    };
    let parts: Vec<&str> = rest.split('.').collect();
    parts.len() == 2
        && parts[0].len() >= 16
        && parts[1].len() >= 16
        && parts.iter().all(|seg| {
            seg.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
}

/// OpenAI (`sk-proj-…`, `sk-ant-api…`) / Anthropic (`sk-ant-…`) / Slack-app
/// (`xapp-…`) keys. Checked as a whole token (prefix + long opaque tail) rather
/// than a bare substring so kebab-case identifiers like `task-proj-x` never match.
fn is_provider_dashkey(tok: &str) -> bool {
    const PREFIXES: &[&str] = &["sk-ant-api", "sk-proj-", "sk-ant-", "xapp-"];
    PREFIXES.iter().any(|pre| {
        tok.strip_prefix(pre).is_some_and(|rest| {
            rest.len() >= 16
                && rest
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
    })
}

/// A URL authority carrying inline credentials — `scheme://user:password@host`.
/// The password must be a literal (not a `$VAR` / `${VAR}` interpolation), so
/// env-substituted connection strings are not flagged.
fn has_connstring_creds(line: &str) -> bool {
    let Some(pos) = line.find("://") else {
        return false;
    };
    let after = &line[pos + 3..];
    let end = after
        .find(|c: char| {
            c == '/' || c == '?' || c == '#' || c.is_whitespace() || c == '"' || c == '\''
        })
        .unwrap_or(after.len());
    let authority = &after[..end];
    let Some(at) = authority.find('@') else {
        return false;
    };
    let userinfo = &authority[..at];
    let Some(colon) = userinfo.find(':') else {
        return false;
    };
    let pass = &userinfo[colon + 1..];
    !pass.is_empty() && !pass.starts_with('$') && !pass.contains('{') && !pass.contains('}')
}

/// Value check used when the assignment target NAMES a secret. Following gitleaks'
/// keyword-prefilter pattern, the naming context raises confidence enough to relax
/// the generic floor: accept hex tokens (≥32) and lower the entropy bar to 3.5,
/// while still rejecting env-var references (`$…`), markup, and short values. Used
/// only for UNQUOTED right-hand sides (the quoted case is already a hard block).
fn looks_like_secret_value_named(v: &str) -> bool {
    let v = v.trim().trim_end_matches([';', ',']);
    if v.len() < 12 || v.chars().any(|c| c.is_whitespace()) {
        return false;
    }
    if v.starts_with('$') || has_non_secret_markers(v) {
        return false;
    }
    let has_digit = v.bytes().any(|b| b.is_ascii_digit());
    let has_alpha = v.bytes().any(|b| b.is_ascii_alphabetic());
    let is_hex = v.len() >= 32 && v.bytes().all(|b| b.is_ascii_hexdigit());
    is_hex || (has_digit && has_alpha && shannon_entropy(v) >= 3.5)
}

/// Double-quoted string literals on a line (odd-indexed `"`-split segments).
fn extract_quoted(line: &str) -> Vec<&str> {
    line.split('"').skip(1).step_by(2).collect()
}

/// True if `text` contains a secret-naming keyword (case-insensitive,
/// word-boundary aware). Bare substring matches like `Tokenizer`,
/// `code_tokenizer`, or `tokens.contains(&...)` no longer fire — the
/// keyword must appear as a standalone identifier (`secret`, `password`,
/// `token`, …), not as a sub-component of a larger name.
fn names_secret(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    SECRET_KEYWORDS.iter().any(|k| {
        // Match the keyword surrounded by non-identifier characters (start
        // of line, whitespace, punctuation, or string-literal boundaries).
        // This keeps `secret_key = "abc"` flagged while removing FP on
        // `code_tokenizer`, `tokens.contains(...)`, `Tokenizer::new()`.
        let bytes = lower.as_bytes();
        let k_bytes = k.as_bytes();
        let mut start = 0usize;
        while let Some(pos) = lower[start..].find(k) {
            let abs = start + pos;
            let end = abs + k_bytes.len();
            let before_ok = abs == 0 || !is_ident_byte(bytes[abs - 1]);
            let after_ok = end == bytes.len() || !is_ident_byte(bytes[end]);
            if before_ok && after_ok {
                return true;
            }
            start = abs + 1;
        }
        false
    })
}

/// True if `b` is a Rust/Python identifier continuation byte
/// (`[a-zA-Z0-9_]`).
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Project `src` to a "code-only" form for the **weak** signal: the contents of
/// string literals (normal `"…"` with `\"` escapes and `\`-newline continuations,
/// and raw `r"…"` / `r#"…"#` / `br#"…"#` strings) and of comments (`//` line and
/// nesting-aware `/* … */` block) are blanked to spaces, with newlines preserved so
/// line structure is kept. The weak keyword scan runs on THIS projection, so a
/// keyword that appears only in prose — a single- **or multi-line** string literal
/// (e.g. a `"… token compression …"` doc string) or a comment — can never fire the
/// weak path; only a keyword used as a real code identifier survives.
///
/// The strong path still scans the raw source (it MUST see string *values* — a real
/// hardcoded secret lives inside a string), so this projection never weakens secret
/// detection; it only removes false positives from the non-blocking 0.5 signal.
///
/// Char literals are not special-cased: a `'"'` char literal — vanishingly rare in
/// real code — could momentarily mis-track the following string. Acceptable for a
/// heuristic feeding only the weak path.
fn strip_strings_and_comments(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        // Try each span kind in precedence order; the first match blanks the span
        // and returns how many bytes it consumed. None of them matching → the byte
        // is real code, emitted verbatim.
        let consumed = blank_line_comment(bytes, i, &mut out)
            .or_else(|| blank_block_comment(bytes, i, &mut out))
            .or_else(|| blank_raw_string(bytes, i, &mut out))
            .or_else(|| blank_normal_string(bytes, i, &mut out))
            .unwrap_or_else(|| {
                out.push(bytes[i]);
                1
            });
        i += consumed;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Blank a `//` line comment to end of line; `None` if `start` is not `//`.
fn blank_line_comment(bytes: &[u8], start: usize, out: &mut Vec<u8>) -> Option<usize> {
    if !(bytes[start] == b'/' && bytes.get(start + 1) == Some(&b'/')) {
        return None;
    }
    let mut j = start;
    while j < bytes.len() && bytes[j] != b'\n' {
        out.push(b' ');
        j += 1;
    }
    Some(j - start)
}

/// Blank a nesting-aware `/* … */` block comment (Rust allows `/* /* */ */`);
/// `None` if `start` is not `/*`.
fn blank_block_comment(bytes: &[u8], start: usize, out: &mut Vec<u8>) -> Option<usize> {
    if !(bytes[start] == b'/' && bytes.get(start + 1) == Some(&b'*')) {
        return None;
    }
    let mut depth = 1usize;
    out.extend_from_slice(b"  ");
    let mut j = start + 2;
    while j < bytes.len() && depth > 0 {
        if bytes[j] == b'/' && bytes.get(j + 1) == Some(&b'*') {
            depth += 1;
            out.extend_from_slice(b"  ");
            j += 2;
        } else if bytes[j] == b'*' && bytes.get(j + 1) == Some(&b'/') {
            depth -= 1;
            out.extend_from_slice(b"  ");
            j += 2;
        } else {
            out.push(if bytes[j] == b'\n' { b'\n' } else { b' ' });
            j += 1;
        }
    }
    Some(j - start)
}

/// Blank a normal `"…"` string literal (with `\` escapes / `\`-newline
/// continuation); `None` if `start` is not an opening double quote.
fn blank_normal_string(bytes: &[u8], start: usize, out: &mut Vec<u8>) -> Option<usize> {
    if bytes[start] != b'"' {
        return None;
    }
    out.push(b' ');
    let mut j = start + 1;
    while j < bytes.len() {
        match bytes[j] {
            b'\\' => {
                out.push(b' ');
                if j + 1 < bytes.len() {
                    out.push(if bytes[j + 1] == b'\n' { b'\n' } else { b' ' });
                    j += 2;
                } else {
                    j += 1;
                }
            }
            b'"' => {
                out.push(b' ');
                j += 1;
                break;
            }
            b'\n' => {
                out.push(b'\n');
                j += 1;
            }
            _ => {
                out.push(b' ');
                j += 1;
            }
        }
    }
    Some(j - start)
}

/// If `bytes[start..]` opens a raw string (`r"…"`, `r#"…"#`, `br##"…"##`, …), blank
/// its full extent (spaces, newlines preserved) into `out` and return the bytes
/// consumed; otherwise return `None` (e.g. the `r` of `return`, or a raw identifier
/// `r#type`).
fn blank_raw_string(bytes: &[u8], start: usize, out: &mut Vec<u8>) -> Option<usize> {
    let mut j = start;
    if bytes.get(j) == Some(&b'b') {
        j += 1; // optional byte-string prefix `b`
    }
    if bytes.get(j) != Some(&b'r') {
        return None;
    }
    j += 1;
    let hashes = {
        let h0 = j;
        while bytes.get(j) == Some(&b'#') {
            j += 1;
        }
        j - h0
    };
    if bytes.get(j) != Some(&b'"') {
        return None; // not a raw-string opener (e.g. `return`, `r#ident`)
    }
    j += 1; // consume opening quote
    while j < bytes.len() {
        if bytes[j] == b'"' {
            let mut k = j + 1;
            let mut cnt = 0usize;
            while cnt < hashes && bytes.get(k) == Some(&b'#') {
                k += 1;
                cnt += 1;
            }
            if cnt == hashes {
                j = k;
                break;
            }
        }
        j += 1;
    }
    let end = j.min(bytes.len());
    for &c in &bytes[start..end] {
        out.push(if c == b'\n' { b'\n' } else { b' ' });
    }
    Some(end - start)
}

/// Scan content, returning (strong, weak) secret signals.
/// 1-based line number of the first line that trips the STRONG scan.
///
/// Re-runs [`scan`] per line rather than duplicating any detection logic, so the
/// two can never disagree. Only called once a hit is already known, so the extra
/// pass costs nothing on the common (clean) path.
///
/// Multi-line secrets (PEM blocks) still resolve, because their marker
/// (`-----BEGIN … PRIVATE`) lives on a single line.
///
/// Returns `None` when the hit is not attributable to one line on its own — the
/// caller then omits the location rather than guessing.
fn first_offending_line(raw: &str) -> Option<usize> {
    raw.lines().position(|line| scan(line).0).map(|idx| idx + 1)
}

fn scan(raw: &str) -> (bool, bool) {
    // (1) provider markers anywhere in the file → strong.
    if STRONG_MARKERS.iter().any(|m| raw.contains(m)) {
        return (true, false);
    }
    // (1b) structural provider tokens that need no entropy (JWT / SendGrid /
    // OpenAI / Anthropic / Slack-app). Split on whitespace + literal delimiters
    // so a token embedded in code or a `.env`/`KEY=value` line is isolated and
    // shape-checked as a whole token (no bare-substring false positives).
    if raw
        .split(|c: char| {
            c.is_whitespace()
                || matches!(
                    c,
                    '"' | '\'' | '`' | '=' | ',' | '(' | ')' | ';' | '[' | ']'
                )
        })
        .any(|tok| is_jwt(tok) || is_sendgrid(tok) || is_provider_dashkey(tok))
    {
        return (true, false);
    }
    // (2) per-line strong signals (connection strings, secret-named assignments,
    // generic high-entropy literals) — these MUST see string *values*, so they
    // scan the raw source.
    for line in raw.lines() {
        if line_has_strong_secret(line) {
            return (true, false);
        }
    }
    // (3) weak: a secret keyword used as a real code identifier (e.g. a bare
    // `let password;` declaration). Scanned on the code-only projection (string
    // literals + comments blanked) so a keyword appearing only in prose — a
    // single- or MULTI-line string literal, or a comment — never fires it.
    let weak = strip_strings_and_comments(raw).lines().any(names_secret);
    (false, weak)
}

/// Per-line strong-signal check: connection-string credentials, a secret-named
/// assignment (quoted literal OR unquoted hex/high-entropy RHS), or a generic
/// high-entropy quoted literal. Extracted from [`scan`] to keep its CC low.
fn line_has_strong_secret(line: &str) -> bool {
    // (1c) connection string with inline credentials (`scheme://u:pw@host`).
    if has_connstring_creds(line) {
        return true;
    }
    // (2/2b) secret-named assignment. Prefer the `=` split so a type annotation
    // `let token: T = "…"` is handled by the assignment, not the colon. A quoted
    // RHS with a non-empty literal hard-blocks; an UNQUOTED RHS blocks only when
    // it is itself a hex / high-entropy token (`.env` / shell `export` / YAML).
    // A line that merely *mentions* a keyword next to strings (a meta tuple, a
    // doc comment, `= std::env::var("…")`) is NOT a blocking assignment.
    let delim = if line.contains('=') {
        line.split_once('=')
    } else {
        line.split_once(':')
    };
    if let Some((lhs, rhs)) = delim
        && names_secret(lhs)
    {
        let strong = if rhs.trim_start().starts_with('"') {
            extract_quoted(rhs).iter().any(|l| !l.is_empty())
        } else {
            looks_like_secret_value_named(rhs)
        };
        if strong {
            return true;
        }
    }
    // (3) generic high-entropy literal anywhere on the line → strong.
    extract_quoted(line)
        .iter()
        .any(|l| looks_like_secret_value(l))
}

/// The secrets detector's OWN source defines provider markers (`"ghp_"`, `"AKIA"`,
/// …) as data and carries secret-shaped test fixtures; scanning it would always
/// self-flag (F2.4 = 0.0). Treat the detector's own verification source + tests as
/// an allowlisted path (gitleaks-style D17 #3) so the crate stays editable and its
/// own files don't surface a false P0 in audits / the PreToolUse BLOCK hook.
fn is_detector_own_source(target: &Path) -> bool {
    if target.file_name().and_then(|n| n.to_str()) == Some("f2_4_secrets.rs") {
        return true;
    }
    // Canonicalize so a relative path (`src/lib.rs` run from inside the crate) is
    // matched just like the absolute path the BLOCK hook passes; fall back to the
    // raw path when the file does not exist (e.g. unit tests).
    let canonical = target.canonicalize();
    let p = canonical
        .as_deref()
        .map(|c| c.to_string_lossy())
        .unwrap_or_else(|_| target.to_string_lossy());
    // The whole detector crate source legitimately defines markers AND describes
    // the security dimensions ("secrets"/"token"/… as dim names in lib.rs/META).
    if p.contains("touring-quality/src") || p.contains("touring-quality/tests") {
        return true;
    }
    // 2026-06-25 (Wave W7.5 fix): any source file that **defines** secret-detection
    // patterns (regex strings describing what the engine looks for) cannot itself
    // be "leaking" those patterns — they are the operational definitions. This
    // matches the `is_detector_own_source` allowlist used by the F2.4 verifier
    // for its own crate, extended to other crates that ship their own secret
    // detectors (`touring-ceg::gateway::sandbox_executor::SECRET_TOKEN_PATTERNS`,
    // etc.). Real user code that PREFIXES a value with `ghp_…` / `sk_live_…` etc.
    // still scores 0.0 — the matcher is fine-grained enough to ignore raw regex
    // source like `r"gh[pousr]_[A-Za-z0-9_]{20,}"` which never contains a literal
    // `ghp_`/`gho_`/`ghu_`/`ghs_`/`ghr_` token.
    if p.contains("touring-ceg/src/gateway/sandbox_executor.rs") {
        return true;
    }
    // 2026-06-25: cache/policy configuration files often embed sample secret-
    // named identifiers (e.g. `Cache<String, _>`) that the engine's word-
    // boundary filter cannot disambiguate from real secrets without
    // AST-level inspection. Allowlist the two known Moka-policy files.
    if p.contains("moka_policies.rs") || p.ends_with("/moka_policies.rs") {
        return true;
    }
    // W2 (2026-07-02): the blanket `/tests/` + `/benches/` allowlist was
    // REMOVED. It was a fail-open — a real secret accidentally committed to any
    // test directory in the workspace passed silently (gitleaks/trufflehog do
    // NOT exempt tests by default). Legitimate fixtures that embed sample
    // secrets now opt out explicitly with a file-level pragma
    // (`touring-quality:allow-secrets`), checked in `F2_4_Secrets::check`. This
    // is the detect-secrets / gitleaks `pragma: allowlist secret` convention:
    // narrow, auditable, and grep-able — not a directory-wide blind spot.
    false
}

impl Verification for F2_4_Secrets {
    fn id(&self) -> DimId {
        DimId::F2_4
    }

    fn check(&self, target: &Path) -> Result<DimScore> {
        if is_detector_own_source(target) {
            return Ok(DimScore {
                value: 1.0,
                status: DimStatus::Pass,
                evidence:
                    "Cryptographic Issues: detector own source — markers are definitions, allowlisted (score=1.000)"
                        .to_string(),
                suggestions: vec![auto_remediation(self.id(), target, DimStatus::Pass)],
                latency_ms: 0,
            });
        }
        let raw = crate::verifications::read_target_source(target)?;

        // File-level opt-out for legitimate fixtures that embed SAMPLE secrets to
        // exercise the redactor (detect-secrets / gitleaks `pragma: allowlist
        // secret` convention). Auditable and grep-able — unlike the removed
        // blanket `/tests/` allowlist. A real secret in a test file WITHOUT this
        // marker is now correctly flagged.
        if raw.contains("touring-quality:allow-secrets") {
            return Ok(DimScore {
                value: 1.0,
                status: DimStatus::Pass,
                evidence: "Cryptographic Issues: file carries `touring-quality:allow-secrets` \
                           pragma (sample-secret fixture, explicitly allowlisted) — score=1.000"
                    .to_string(),
                suggestions: vec![auto_remediation(self.id(), target, DimStatus::Pass)],
                latency_ms: 0,
            });
        }

        let (strong, weak) = scan(&raw);
        let (value, category): (f32, &str) = if strong {
            (
                0.0,
                "hardcoded secret detected (token prefix / secret-named assignment / high-entropy literal)",
            )
        } else if weak {
            (0.5, "secret-related keyword present (no assigned value)")
        } else {
            (1.0, "no hardcoded secret detected")
        };

        // Point at the offending line. This gate scores the WHOLE file, so a hit
        // blocks every subsequent edit to it — without a line number the author
        // has to bisect by hand to learn what tripped it (three times over on
        // 2026-08-02, all false positives).
        //
        // The line NUMBER only: quoting the text would print the very credential
        // this dim exists to protect, into terminal scrollback and CI logs.
        let location = first_offending_line(&raw)
            .map(|line| format!(" at line {line}"))
            .unwrap_or_default();
        let evidence = format!("Cryptographic Issues: {category}{location} — score={value:.3}");
        Ok(crate::verifications::finish(
            self.id(),
            value,
            evidence,
            target,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn score(content: &str) -> DimScore {
        let mut f = NamedTempFile::new().expect("create temp");
        f.write_all(content.as_bytes()).expect("write");
        F2_4_Secrets.check(f.path()).expect("check")
    }

    #[test]
    fn test_secrets_returns_valid_score() {
        let s = score("fn example() {}\n");
        assert!((0.0..=1.0).contains(&s.value), "out of range: {}", s.value);
    }

    #[test]
    fn test_secrets_empty_file() {
        let s = score("");
        assert!((0.0..=1.0).contains(&s.value));
    }

    #[test]
    fn test_clean_code_passes() {
        let s = score("pub fn add(a: i32, b: i32) -> i32 { a + b }\n");
        assert_eq!(s.value, 1.0, "clean code should pass");
    }

    #[test]
    fn test_filesystem_path_with_datey_tail_passes() {
        // Regression 2026-08-02: a test-fixture path whose date/version tail has
        // high Shannon entropy is a resource location, not a leaked credential.
        let s = score(
            "let r = extract_referencia(\"/tmp/SINAPI_Custos_PB_122024_Desonerado.xlsx\");\n",
        );
        assert_eq!(s.value, 1.0, "filesystem path must not read as a secret");
    }

    #[test]
    fn test_generic_base64_literal_still_blocks() {
        // Sanity twin for the path exemption: an opaque high-entropy token that
        // does NOT start with a path prefix must keep tripping the generic path.
        let s = score("let k = \"q7Zp3xVb9TqLm2Rw8sYd4NcF6hJk1GtA\";\n");
        assert_eq!(s.value, 0.0, "opaque high-entropy literal must still block");
    }

    #[test]
    fn test_github_pat_blocks() {
        let s = score("let t = \"ghp_aBcDeF0123456789aBcDeF0123456789aBcD\";\n");
        assert_eq!(s.value, 0.0, "ghp_ token must block");
    }

    #[test]
    fn test_github_oauth_blocks() {
        let s = score("const T: &str = \"gho_16C7e42F292c6912E7710c838347Ae178B4a\";\n");
        assert_eq!(s.value, 0.0, "gho_ token must block");
    }

    #[test]
    fn test_slack_token_blocks() {
        let s = score("let s = \"xoxb-1234567890-ABCDEFabcdef1234567890ab\";\n");
        assert_eq!(s.value, 0.0, "xoxb- token must block");
    }

    #[test]
    fn test_stripe_live_blocks() {
        let s = score("let k = \"sk_live_aBcDeF0123456789AbCdEf01\";\n");
        assert_eq!(s.value, 0.0, "sk_live_ key must block");
    }

    #[test]
    fn test_aws_key_still_blocks() {
        // regression: AWS detection must still work.
        let s = score("pub const AWS: &str = \"AKIAIOSFODNN7EXAMPLE\";\n");
        assert_eq!(s.value, 0.0, "AKIA key must still block");
    }

    #[test]
    fn test_generic_high_entropy_blocks() {
        // non-secret-named var, but high-entropy mixed-case token → generic entropy path.
        let s = score("let blob = \"a1B2c3D4e5F6g7H8i9J0kLmNoPqRsTuV\";\n");
        assert_eq!(s.value, 0.0, "high-entropy literal must block");
    }

    #[test]
    fn test_secret_named_assignment_blocks() {
        let s = score("let password = \"hunter2value\";\n");
        assert_eq!(s.value, 0.0, "secret-named assignment must block");
    }

    #[test]
    fn test_bare_keyword_warns_not_blocks() {
        // A code line that *names* a secret (no assigned value) → Warn (0.5),
        // must NOT block (< 0.5). Regression: the comment filter must NOT
        // suppress real code-line weak signals.
        // Post word-boundary fix (2026-06-25): compound identifiers
        // (`new_password`, `api_key`) are NOT weak — they are real Rust idiom
        // naming patterns. Use a clear keyword here so the weak signal
        // still triggers.
        let s = score("fn rotate() { let password: String; }\n");
        assert_eq!(s.value, 0.5, "bare keyword in code should warn, not block");
        assert!(
            s.value >= 0.5,
            "warn must not trip the <0.5 BLOCK threshold"
        );
    }

    #[test]
    fn test_hex_hash_no_false_positive() {
        // pure-hex hash (entropy ≤ 4.0) with non-secret var → must NOT block.
        let s = score("const H: &str = \"abcdef0123456789abcdef0123456789abcdef01\";\n");
        assert_eq!(s.value, 1.0, "hex hash must not be a false positive");
    }

    #[test]
    fn test_sentence_no_false_positive() {
        // long string with spaces, non-secret var → must NOT block.
        let s = score("let msg = \"the quick brown fox jumps over the lazy dog\";\n");
        assert_eq!(s.value, 1.0, "prose literal must not be a false positive");
    }

    #[test]
    fn test_url_literal_no_false_positive() {
        // 26-char URL with a digit (2000) — the `://` marker must exclude it from
        // the generic-entropy path (Finding A regression).
        let s = score("const NS: &str = \"http://www.w3.org/2000/svg\";\n");
        assert_eq!(s.value, 1.0, "URL literal must not be a false positive");
    }

    #[test]
    fn test_html_format_template_no_false_positive() {
        // HTML / format-string template with <>{} markers must not trip entropy.
        let s = score("let row = \"<td>{}</td><td>{:.3}</td><td>{}</td><td>{}</td>\";\n");
        assert_eq!(
            s.value, 1.0,
            "HTML/format template must not be a false positive"
        );
    }

    #[test]
    fn test_base64_token_still_blocks() {
        // base64-shaped secret with `+` `/` `=` (but no `://`) must STILL block —
        // proves the URL/markup exclusion is precise and does not weaken base64.
        // Deliberately built from short runs (`abc`, `DEF`, `123` → 57% ascending
        // pairs): it sits just under the 80% `is_predictable_sequence` bar and is
        // the sentinel that catches any tightening of that exemption (it DID
        // catch the 50% bar swallowing it, 2026-08-02).
        let s = score("let blob = \"abcDEF123ghiJKL456mnoPQR789stu+/=Xy\";\n");
        assert_eq!(
            s.value, 0.0,
            "base64-shaped high-entropy token must still block"
        );
    }

    #[test]
    fn test_env_var_pattern_no_false_block() {
        // the SECURE pattern (load from env, not hardcoded) must NOT hard-block —
        // the quoted "DB_PASSWORD" is an env var NAME inside a call, not a value.
        let s = score("    let password = std::env::var(\"DB_PASSWORD\")?;\n");
        assert!(
            s.value >= 0.5,
            "env::var pattern must not hard-block (got {})",
            s.value
        );
    }

    #[test]
    fn test_type_annotated_secret_still_blocks() {
        // a type annotation must not let a direct hardcoded literal slip past path-2.
        let s = score("    let token: &str = \"opaqueSecretValue1234567\";\n");
        assert_eq!(
            s.value, 0.0,
            "type-annotated hardcoded literal must still block"
        );
    }

    #[test]
    fn test_percent_token_still_blocks() {
        // `%` is no longer excluded — a high-entropy token containing it must block.
        let s = score("    let blob = \"aB3cD5%eF7gH9iJ1kL3mN5pQ7rS9tU\";\n");
        assert_eq!(s.value, 0.0, "high-entropy token with `%` must still block");
    }

    #[test]
    fn test_pem_encrypted_and_sec1_block() {
        assert_eq!(
            score("const K: &str = \"-----BEGIN ENCRYPTED PRIVATE KEY-----\";\n").value,
            0.0,
            "ENCRYPTED PRIVATE KEY header must block"
        );
        assert_eq!(
            score("const K: &str = \"-----BEGIN SEC1 PRIVATE KEY-----\";\n").value,
            0.0,
            "SEC1 PRIVATE KEY header must block"
        );
    }

    #[test]
    fn test_meta_tuple_does_not_block() {
        // a meta tuple / call that merely names the "secrets" dimension next to
        // string literals is not an assignment — it may warn (soft) but must NOT
        // hard-block (<0.5). Regression for the lib.rs META_TABLE self-flag.
        let s = score("    (\"F2.4\", \"secrets\", \"Cryptographic Issues\"),\n");
        assert!(
            s.value >= 0.5,
            "meta tuple must not hard-block (got {})",
            s.value
        );
    }

    #[test]
    fn test_detector_own_source_allowlisted() {
        // The detector's own source/tests are allowlisted (Finding B self-skip):
        // the path is matched before any file read, so it need not exist on disk.
        let by_name = F2_4_Secrets
            .check(std::path::Path::new("/anywhere/f2_4_secrets.rs"))
            .expect("check");
        assert_eq!(
            by_name.value, 1.0,
            "detector own file (by name) must be allowlisted"
        );
        let by_path = F2_4_Secrets
            .check(std::path::Path::new(
                "/repo/touring-quality/src/verifications/other.rs",
            ))
            .expect("check");
        assert_eq!(
            by_path.value, 1.0,
            "detector verifications dir must be allowlisted"
        );
    }

    // ── Backlog hardening (2026-06-20): JWT / conn-strings / new providers /
    //    named-unquoted env-style / ARN allowlist / entropy-with-name-context ──

    #[test]
    fn test_jwt_blocks() {
        // header.payload.signature, header base64url-encodes `{"alg":"HS256",…}`.
        let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N";
        assert_eq!(
            score(&format!("let t = \"{jwt}\";\n")).value,
            0.0,
            "JWT literal must block"
        );
    }

    #[test]
    fn test_jwt_unquoted_env_blocks() {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJ1c2VyIjoiYWRtaW4ifQ.Qm9ndXNTaWduYXR1cmVYWVo";
        assert_eq!(
            score(&format!("ACCESS_TOKEN={jwt}\n")).value,
            0.0,
            "unquoted JWT must block"
        );
    }

    #[test]
    fn test_dotted_version_not_jwt() {
        // three dot-separated identifiers that are NOT a JWT (no `eyJ` header).
        let s = score("const V: &str = \"1.2.3\";\n");
        assert_eq!(
            s.value, 1.0,
            "dotted version must not be a JWT false positive"
        );
    }

    #[test]
    fn test_connstring_creds_block() {
        let s = score("DATABASE_URL=postgres://admin:s3cr3tP4ssw0rd@db.internal:5432/app\n");
        assert_eq!(s.value, 0.0, "conn-string with inline password must block");
    }

    #[test]
    fn test_connstring_env_ref_no_false_block() {
        // password is a `${VAR}` interpolation → NOT a hardcoded secret.
        let s = score("url = \"postgres://user:${DB_PASS}@host:5432/db\"\n");
        assert!(
            s.value >= 0.5,
            "env-interpolated conn-string must not hard-block (got {})",
            s.value
        );
    }

    #[test]
    fn test_plain_url_no_false_block() {
        // scheme://host with no `user:pass@` authority → not a credential.
        let s = score("const API: &str = \"https://api.example.com:8443/v1/users\";\n");
        assert_eq!(s.value, 1.0, "plain URL must not be a false positive");
    }

    #[test]
    fn test_digitalocean_token_blocks() {
        let s = score(
            "let t = \"dop_v1_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\";\n",
        );
        assert_eq!(s.value, 0.0, "dop_v1_ token must block");
    }

    #[test]
    fn test_gitlab_pat_blocks() {
        let s = score("GITLAB = \"glpat-abcDEF1234567890xyzA\"\n");
        assert_eq!(s.value, 0.0, "glpat- token must block");
    }

    #[test]
    fn test_openai_anthropic_blocks() {
        assert_eq!(
            score("k = \"sk-proj-abcDEF1234567890ghiJKL\"\n").value,
            0.0,
            "sk-proj- key must block"
        );
        assert_eq!(
            score("k = \"sk-ant-api03-abcDEF1234567890ghiJKL\"\n").value,
            0.0,
            "sk-ant-api key must block"
        );
    }

    #[test]
    fn test_kebab_identifier_not_provider_key() {
        // `task-proj-runner` must NOT match `sk-proj-` (whole-token prefix, not substring).
        let s = score("let task_label = \"run-task-proj-runner-fast\";\n");
        assert_eq!(
            s.value, 1.0,
            "kebab identifier must not false-positive as provider key"
        );
    }

    #[test]
    fn test_sendgrid_blocks() {
        let s = score(
            "SENDGRID = \"SG.aBcDeFgHiJkLmNoPqRsTuV.wXyZ0123456789aBcDeFgHiJkLmNoPqRsTuVwXyZ012\"\n",
        );
        assert_eq!(s.value, 0.0, "SendGrid SG. key must block");
    }

    #[test]
    fn test_named_unquoted_hex_blocks() {
        // `.env`-style secret-named assignment with an unquoted hex token.
        let s = score("API_KEY=deadbeefdeadbeefdeadbeefdeadbeef\n");
        assert_eq!(s.value, 0.0, "named unquoted hex secret must block");
    }

    #[test]
    fn test_named_unquoted_placeholder_no_block() {
        // short / low-entropy placeholder default → warn, must NOT hard-block.
        let s = score("password=changeme\n");
        assert!(
            s.value >= 0.5,
            "placeholder default must not hard-block (got {})",
            s.value
        );
    }

    #[test]
    fn test_arn_no_false_positive() {
        // AWS ARN with a random-looking resource tail → identifier, not a secret.
        let s = score("let r = \"arn:aws:iam::123456789012:role/AppRoleX9k2Lm4Qp7Zt\";\n");
        assert_eq!(s.value, 1.0, "ARN must not be an entropy false positive");
    }

    // ── 2026-06-24: comment-line filter — doc comments discussing the concept
    //    ("token" as a lexical term, "password" as a D-name) must NOT fire weak.
    //    Regression for the mod.rs audit false-positive.

    #[test]
    fn test_doc_comment_with_keyword_passes() {
        // doc comment mentions "token" + "password" + "secret" as concepts
        // (no assignment exists on the line) → must be 1.0 Pass, not 0.5 Warn.
        let s = score(
            "/// Polyglot database-performance analysis (D20): N+1 (a DB-execution token —\n\
             /// auth-handling code (token/cookie/login/password) without `test_auth*`.\n\
             /// operand vocabulary and total token counts.\n",
        );
        assert_eq!(
            s.value, 1.0,
            "doc comments discussing the concept must not fire weak (got {})",
            s.value
        );
    }

    #[test]
    fn test_single_line_doc_comment_with_keyword_passes() {
        let s = score("/// the `token` is `//`, `/*`, `*/`, `*`, or `#` for Python-family).\n");
        assert_eq!(
            s.value, 1.0,
            "single doc comment with keyword must pass (got {})",
            s.value
        );
    }

    #[test]
    fn test_block_comment_with_keyword_passes() {
        let s = score(
            "/*\n \
             * This module handles token, password, and secret detection.\n \
             * The keyword api_key is part of the public API.\n \
             */\n",
        );
        assert_eq!(
            s.value, 1.0,
            "block comment with keyword must pass (got {})",
            s.value
        );
    }

    #[test]
    fn test_inner_doc_comment_with_keyword_passes() {
        // `//!` (inner doc) also starts with `//` → must be skipped.
        let s = score("//! Helper for credential rotation: token + password lifecycle.\n");
        assert_eq!(
            s.value, 1.0,
            "inner doc comment with keyword must pass (got {})",
            s.value
        );
    }

    #[test]
    fn test_code_line_with_keyword_still_warns() {
        // A real code line that *names* a secret (no assignment) still fires weak.
        // Regression guard: the comment filter must not over-suppress.
        // Post word-boundary fix (2026-06-25): keywords embedded inside compound
        // identifiers (`new_token`, `old_password`) no longer fire — they are
        // the kinds of compound names that real Rust code uses, and gitleaks
        // itself only flags a *whole-token* match. Use a clear keyword here
        // so the weak signal still triggers.
        let s = score("fn rotate() { let token: String; let password: String; }\n");
        assert_eq!(
            s.value, 0.5,
            "code line naming a secret (no assignment) must still warn (got {})",
            s.value
        );
    }

    #[test]
    fn test_mixed_comment_and_code_keyword_warns() {
        // A doc comment line is benign, but a sibling code line that names a
        // keyword still triggers weak (the source as a whole is suspect).
        let s = score(
            "/// API for token management.\n\
             fn rotate() { let password: String; }\n",
        );
        assert_eq!(
            s.value, 0.5,
            "code line in same file as doc comment must still warn (got {})",
            s.value
        );
    }

    // ── 2026-06-28: weak-path precision — a secret keyword appearing only in a
    //    string literal (single- OR multi-line) or a trailing comment is prose,
    //    not an identifier naming a secret. Regression for the cli_suggester.rs
    //    F2.4 false positive ("…token compression…" in a `\`-continued doc string).

    #[test]
    fn test_keyword_in_string_literal_no_weak() {
        let s = score("    let label = \"Signal-to-Token Ratio\";\n");
        assert_eq!(
            s.value, 1.0,
            "keyword inside a string literal must not fire weak (got {})",
            s.value
        );
    }

    #[test]
    fn test_keyword_in_multiline_string_no_weak() {
        // Faithful to the cli_suggester.rs FP: a `\`-continued multi-line string
        // whose final line carries "token". Line-by-line quote parity would mis-
        // read the unbalanced closing line; the file-wide projection must not.
        let s = score(
            "    let reason = \"one touring run executes the whole loop \\\n\
             code-mode WITHOUT MCP, 30-200x token compression (CodeAct).\";\n",
        );
        assert_eq!(
            s.value, 1.0,
            "keyword in a multi-line string literal must not fire weak (got {})",
            s.value
        );
    }

    #[test]
    fn test_keyword_in_trailing_comment_no_weak() {
        let s = score("    let n = bucket.size(); // token bucket rate limiter\n");
        assert_eq!(
            s.value, 1.0,
            "keyword in a trailing comment must not fire weak (got {})",
            s.value
        );
    }

    #[test]
    fn test_keyword_in_url_string_no_weak() {
        // The `//` of the URL scheme is inside the string (blanked first), so it
        // is not mistaken for a comment, and "token" in the path does not fire.
        let s = score("    let u = \"https://api.example.com/v1/token\";\n");
        assert_eq!(
            s.value, 1.0,
            "keyword in a URL string must not fire weak (got {})",
            s.value
        );
    }

    #[test]
    fn test_keyword_in_raw_string_no_weak() {
        // Raw string `r#"…"#` carrying the keyword → prose, blanked, no weak.
        let s = score("    let code = r#\"export AUTH from token store\"#;\n");
        assert_eq!(
            s.value, 1.0,
            "keyword in a raw string must not fire weak (got {})",
            s.value
        );
    }

    #[test]
    fn test_real_declaration_still_warns_with_unrelated_string() {
        // Guard: the identifier `token` is real code (outside the string); the
        // string is unrelated prose. The projection keeps the identifier, so the
        // weak signal still fires — precision must not cost the true positive.
        let s = score("    let token = lookup(\"some label\");\n");
        assert_eq!(
            s.value, 0.5,
            "a real secret-named declaration must still warn even with an unrelated string (got {})",
            s.value
        );
    }

    /// W2 (2026-07-02): the blanket `/tests/` + `/benches/` allowlist was
    /// removed — a real secret in a test directory must now be flagged, and
    /// legitimate fixtures opt out via the `touring-quality:allow-secrets`
    /// file-level pragma (gitleaks/detect-secrets convention).
    #[test]
    fn test_tests_dir_no_longer_blanket_allowlisted_pragma_opts_out() {
        use std::io::Write;

        // A /tests/ path is NOT self-allowlisted anymore (was `true` pre-W2).
        assert!(
            !is_detector_own_source(std::path::Path::new("/x/crates/foo/tests/bar.rs")),
            "arbitrary /tests/ files must no longer be blanket-allowlisted"
        );
        // The detector's own source stays allowlisted (embeds the vocabulary).
        assert!(is_detector_own_source(std::path::Path::new(
            "/x/touring-analysis/src/quality/f2_4_secrets.rs"
        )));

        let secret = "let t = \"ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789\";\n";

        // Secret WITHOUT pragma → BLOCK (0.0), even if the temp path is under a
        // test-like dir (NamedTempFile lives in the system temp dir).
        let mut f_bad = tempfile::Builder::new().suffix(".rs").tempfile().unwrap();
        f_bad.write_all(secret.as_bytes()).unwrap();
        let bad = F2_4_Secrets.check(f_bad.path()).unwrap();
        assert_eq!(bad.value, 0.0, "hardcoded secret without pragma must BLOCK");

        // Same secret WITH the pragma → allowlisted (1.0).
        let mut f_ok = tempfile::Builder::new().suffix(".rs").tempfile().unwrap();
        f_ok.write_all(b"// touring-quality:allow-secrets\n")
            .unwrap();
        f_ok.write_all(secret.as_bytes()).unwrap();
        let ok = F2_4_Secrets.check(f_ok.path()).unwrap();
        assert_eq!(
            ok.value, 1.0,
            "pragma must allowlist a sample-secret fixture"
        );
    }

    // ── FP guards added 2026-08-02 ──────────────────────────────────────────
    // Each of these blocked a real, unrelated edit because the gate scores the
    // WHOLE FILE: one stale false positive freezes every future change to it.

    #[test]
    fn predictable_runs_are_not_secrets() {
        // The exact literal that blocked touring-identity/tests/property_tests.rs:
        // a character-set alphabet. Every char appears once, which MAXIMISES
        // Shannon entropy (~5.98 bits) while carrying no randomness at all.
        let alphabet = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_";
        assert!(
            shannon_entropy(alphabet) >= 4.5,
            "premise: entropy clears the floor"
        );
        assert!(is_predictable_sequence(alphabet));
        assert!(!looks_like_secret_value(alphabet));

        assert!(is_predictable_sequence("abcdefghijklmnop"));
        assert!(is_predictable_sequence("0123456789012345"));
        // Too short to judge — stays out of the heuristic.
        assert!(!is_predictable_sequence("abcdef"));

        // REGRESSION GUARD: this token is built from short runs (abc/DEF/123)
        // and sits at 57% consecutive pairs. A 50% bar swallowed it, silently
        // weakening real detection — caught by test_base64_token_still_blocks.
        // The 80% bar keeps it on the secret path.
        assert!(!is_predictable_sequence(
            "abcDEF123ghiJKL456mnoPQR789stu+/=Xy"
        ));
    }

    #[test]
    fn real_tokens_survive_the_run_heuristic() {
        // Regression guard: the fix must not blunt detection. Random-looking
        // tokens have ~1/62 of pairs consecutive, far under the 50% bar.
        assert!(!is_predictable_sequence("aB3xK9mQ7zR2wY5tE8uI1oP4"));
        assert!(!is_predictable_sequence("dGhpcyBpcyBhIHNlY3JldCB0b2tlbg"));
        // A short ascending tail does not make the whole token predictable.
        assert!(!is_predictable_sequence("K9mQ7zR2wY5tE8uI1oP4abcde"));
        assert!(looks_like_secret_value("aB3xK9mQ7zR2wY5tE8uI1oP4"));
    }

    #[test]
    fn email_addresses_are_not_secrets() {
        // GitHub's Actions bot identity — the numeric part is its PUBLIC id,
        // documented by GitHub. Blocked a workflow edit on 2026-08-02.
        let bot = "41898282+github-actions[bot]@users.noreply.github.com";
        assert!(is_email_like(bot));
        assert!(!looks_like_secret_value(bot));

        assert!(is_email_like("someone.long.name@subdomain.example.org"));
        assert!(!is_email_like("not-an-email-at-all"));
        assert!(!is_email_like("@nolocal.com"));
        assert!(!is_email_like("missing-tld@example"));
    }

    #[test]
    fn evidence_points_at_the_offending_line_without_leaking_it() {
        let src = "fn main() {\n    let cfg = load();\n    let key = \"ghp_aBcDeF0123456789aBcDeF0123456789aBcD\";\n}\n";
        assert_eq!(first_offending_line(src), Some(3));

        let s = score(src);
        assert_eq!(s.value, 0.0, "premise: this must block");
        assert!(
            s.evidence.contains("at line 3"),
            "evidence must locate the hit: {}",
            s.evidence
        );
        // The whole point of the redaction: the gate must not print the secret
        // it is protecting into scrollback or CI logs.
        assert!(
            !s.evidence.contains("ghp_"),
            "evidence must NEVER echo the secret: {}",
            s.evidence
        );
    }

    #[test]
    fn clean_file_reports_no_location() {
        let s = score("fn main() { println!(\"hello\"); }\n");
        assert_eq!(s.value, 1.0);
        assert!(!s.evidence.contains("at line"));
    }

    #[test]
    fn connection_string_credentials_still_detected() {
        // The email exclusion must NOT swallow `user:password@host`: the colon
        // disqualifies it, so it stays on the credential path.
        assert!(!is_email_like("admin:hunter2@db.internal.example.com"));
        assert!(has_connstring_creds(
            "postgres://admin:s3cr3tP4ssw0rd@db.example.com:5432/app"
        ));
    }
}
