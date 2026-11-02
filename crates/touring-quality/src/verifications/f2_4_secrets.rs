//! F2.4 — Hardcoded Secrets verifier.
//!
//! Label (17/10/2026): `touring-quality list` used to call this dimension
//! "Cryptographic Issues", but nothing here measures cryptography (algorithms, key
//! sizes, RNG): every signal below is a *credential committed to the source*. The rule
//! file and CLAUDE.md already say "Secrets"; the code now agrees.
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
//!      mixed alphanumeric, Shannon entropy ≥ 4.5 (skips hex hashes & `arn:`/URL/markup,
//!      and — 02/11/2026 — literals that READ as words, numbers and short ids joined by
//!      `- _ . + / | & = ? :`: SEI piece names, NUP keys, query strings, checksum paths).
//!
//! The assignment signals (4) read a comparison (`!=`, `<=`, `>=`) as a comparison, not a
//! binding; a secret name is the target of the delimiter only (another parameter of the
//! same signature is not); and a quoted value is a stand-in — not a credential — when it
//! is a readable placeholder (also numbered/headed: `test-api-key-123`), a ≤4-digit
//! `token` ordinal, or prose assigned to a message holder (`errors.password = "Senha …"`).
//!
//! Strong signal → Fail (0.0 → PreToolUse BLOCK). A bare secret keyword used as a real code
//! identifier with no assigned value → Warn (0.5, surfaced but not blocking). The weak scan runs
//! on a code-only projection (string literals + comments blanked), so a keyword appearing only in
//! prose — a single- or multi-line string literal, or a comment — never fires it. Nothing → Pass (1.0).

use crate::DimId;
use crate::verifications::Verification;
use anyhow::Result;
use std::borrow::Cow;
use std::path::Path;
use std::sync::OnceLock;

/// F2.4 verifier — Hardcoded Secrets.
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
        // Reference-scheme prefixes: memory keys (`mem:lexhub:sei-50500-115208-…`,
        // first hit 2026-08-03 blocking a registry edit) and URNs are pointers to
        // resources, never opaque credentials — same category as `arn:`. A leaked
        // key with such a prefix would still land on the provider-prefix and
        // secret-named paths, so detection holds.
        || v.starts_with("mem:")
        || v.starts_with("urn:")
        || v.contains("://")
        || is_path_shape(v)
        // Code and style syntax: `(`/`)` of a call and `;` of a statement or a CSS
        // declaration never occur in an opaque token (26/09/2026: `expect(…);` in a
        // completion suggestion and `background:#eee;width:200px;…` inside an HTML
        // f-string held the analise hooks tree at 0.0).
        || v.bytes()
            .any(|b| matches!(b, b'<' | b'>' | b'{' | b'}' | b'\\' | b'(' | b')' | b';'))
        || is_predictable_sequence(v)
        || is_email_like(v)
        || is_sri_digest(v)
}

/// A package version or range (`^5.0.1`, `~4.0`, `>=2.0.0-rc.1`, `1.x`, `v3.2.1`):
/// the value of a dependency entry, never a credential. A dependency's NAME may
/// mention a token (`registry-auth-token`) while its value is a semver range
/// (27/09/2026, the kazuba-browser lockfile at F2.4 0.0). Bounded so it waves
/// nothing else through: a bare number needs a range operator (a numeric secret
/// stays a secret), parts are ≤ 8 digits, and the pre-release tail is ≤ 16 chars
/// (`1-<token>` still blocks).
fn is_version_spec(v: &str) -> bool {
    let bare = v.trim_start_matches(['^', '~', '>', '<', '=']);
    let has_op = bare.len() != v.len();
    let bare = bare.strip_prefix('v').unwrap_or(bare);
    let (core, tail) = bare.split_once(['-', '+']).unwrap_or((bare, ""));
    is_version_core(core, has_op) && is_prerelease_tail(tail)
}

/// `MAJOR[.MINOR[.PATCH]]`: each part ≤ 8 digits (`x`/`*` after the first); a
/// single part only after a range operator, so a bare number is never a version.
fn is_version_core(core: &str, has_op: bool) -> bool {
    let parts: Vec<&str> = core.split('.').collect();
    let numeric = |p: &str| (1..=8).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit());
    let arity_ok = parts.len() <= 3 && (parts.len() >= 2 || has_op);
    arity_ok
        && numeric(parts[0])
        && parts[1..]
            .iter()
            .all(|p| numeric(p) || matches!(*p, "x" | "*"))
}

/// A semver pre-release/build tail (`rc.1`, `beta-2`): short, so `1-<token>` is no version.
fn is_prerelease_tail(tail: &str) -> bool {
    tail.len() <= 16
        && tail
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'))
}

/// A Subresource Integrity digest — `sha256-`/`sha384-`/`sha512-` + base64 (W3C
/// SRI; npm lockfiles' `"integrity"`): the hash of a PUBLIC file, published so the
/// browser can verify it, never a credential. Shape, not exact length: a malformed
/// digest is a broken integrity check, not a leaked secret (27/09/2026 — every
/// `package-lock.json` in the analise failed F2.4 at 0.0).
fn is_sri_digest(v: &str) -> bool {
    let Some(rest) = ["sha256-", "sha384-", "sha512-"]
        .iter()
        .find_map(|p| v.strip_prefix(p))
    else {
        return false;
    };
    rest.len() >= 40
        && rest
            .trim_end_matches('=')
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/')
}

/// A filesystem path or a bare filename: a resource location, never an opaque
/// credential (25/09/2026, touring-36, the analise-50 judge). A `?` in the
/// value is a QUERY STRING, never a filesystem name (illegal on Windows,
/// near-never on Linux): `/api?key=<token>` is a credential embedded in a
/// URL and must NOT be spared — the control that keeps this exclusion
/// honest.
fn is_path_shape(v: &str) -> bool {
    if v.contains('?') {
        return false;
    }
    v.starts_with('/')
        || v.starts_with("./")
        || v.starts_with("../")
        || v.starts_with("~/")
        || is_relative_file_path(v)
        || is_relative_dir_path(v)
        || is_known_filename(v)
}

/// A bare filename whose last segment carries a KNOWN document/code
/// extension — `[14]-42780739_Minuta_de_Deliberacao.md` (the analise-50
/// judge, 25/09/2026): a name with a document extension, no `/` anywhere, is
/// a file reference, not a token. A real token never ends in `.md`/`.py` —
/// and if a leak is staged as a filename, the provider-prefix and
/// secret-named paths still see it.
fn is_known_filename(v: &str) -> bool {
    const EXTS: &[&str] = &[
        "md", "markdown", "txt", "pdf", "doc", "docx", "odt", "rtf", "tex", "py", "rs", "js", "ts",
        "tsx", "jsx", "go", "java", "c", "h", "cpp", "sh", "json", "yaml", "yml", "toml", "ini",
        "cfg", "conf", "xml", "csv", "html", "htm", "css", "svg", "png", "jpg", "jpeg", "gif",
        "webp", "xls", "xlsx", "zip", "gz", "tar", "mp3", "mp4", "wav", "sql", "log",
    ];
    let last = v.rsplit('/').next().unwrap_or(v);
    let last = last.split('#').next().unwrap_or(last);
    match last.rsplit_once('.') {
        Some((stem, ext)) => !stem.is_empty() && EXTS.contains(&ext.to_ascii_lowercase().as_str()),
        None => false,
    }
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
/// A `/`-separated value whose LAST segment carries a short alphanumeric file
/// extension (optionally followed by a `#anchor`) — a relative repo path like
/// `docs/plans/15-DIAGNOSTICO-INTEGRACAO.md#7.2`, never an opaque token. Opaque
/// base64 tokens contain `/` but have no dotted short-extension tail; a leaked
/// key inside a path-shaped string still lands on the provider-prefix and
/// secret-named detection paths.
fn is_relative_file_path(v: &str) -> bool {
    if !v.contains('/') {
        return false;
    }
    let last = v.rsplit('/').next().unwrap_or("");
    let stem = last.split('#').next().unwrap_or(last);
    match stem.rsplit_once('.') {
        Some((_, ext)) => {
            (1..=5).contains(&ext.len()) && ext.bytes().all(|b| b.is_ascii_alphanumeric())
        }
        None => false,
    }
}

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
    // `is_readable_phrase` lives HERE and not in `has_non_secret_markers`: that
    // predicate is shared with the NAMED unquoted path (`.env` `DB_PASSWORD=
    // summer-sunset-2024`), where a readable phrase assigned to a secret name is a
    // real, human-chosen credential and must keep blocking (02/11/2026).
    if has_non_secret_markers(v) || is_readable_phrase(v) {
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
    // Cross-audit 03/09/2026 — `$var` e `{placeholder}` ja eram descartados, mas
    // `user:pass` de um exemplo de documentacao nao: `postgres.rs` zerou por causa
    // de uma linha `/// …new("postgres://user:pass@localhost/touring")`. A regra
    // segue lendo a linha CRUA de proposito (uma DSN real num comentario E'
    // vazamento); o que faltava era a mesma disciplina que o resto do detector ja
    // usa para separar credencial de stand-in. `Tr0ub4dor&3` mistura classes e
    // continua bloqueando; `pass` e `changeme` nao.
    !pass.is_empty()
        && !pass.starts_with('$')
        && !pass.contains('{')
        && !pass.contains('}')
        && !is_readable_placeholder(pass)
        && !is_regex_class(userinfo)
}

/// A `userinfo` written as a regex character class — `[^:]+:[^@]+`,
/// `[a-z]*:[A-Za-z0-9]+` — DESCRIBES the credential slot of a DSN: the pattern a
/// validator ships, not a leak (26/09/2026: it zeroed F2.4 of `analise/.claude/hooks`
/// on `secrets_detector.py`'s own connection-string regex).
fn is_regex_class(s: &str) -> bool {
    s.contains("[^") || s.contains("]+") || s.contains("]*") || s.contains("]{")
}

/// The code-vs-literal predicate: ONE source, shared with the OWASP A05
/// config analyzer (`default_or_weak_credentials`), which judges the same
/// class of lines (touring-36, 25/09/2026 — a second copy is how the two
/// dims would drift into disagreeing about the same line). Lives in
/// `touring_foundation::security` because the analyzer crate cannot depend
/// on this one, and neither may depend on the other with an optional
/// feature in the way.
use touring_foundation::security::is_code_expression;

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
    if v.starts_with('$') || has_non_secret_markers(v) || is_code_expression(v) {
        return false;
    }
    let has_digit = v.bytes().any(|b| b.is_ascii_digit());
    let has_alpha = v.bytes().any(|b| b.is_ascii_alphabetic());
    let is_hex = v.len() >= 32 && v.bytes().all(|b| b.is_ascii_hexdigit());
    is_hex || (has_digit && has_alpha && shannon_entropy(v) >= 3.5)
}

/// True when a secret-named RHS is plainly a human-readable placeholder, not a
/// credential: short, digit-free, and made only of letters plus `-`/`_`/`.`
/// separators (`"my-secret-token"`, `"changeme"`, `"your_api_key_here"`).
///
/// Origin 2026-08-02 (cross-audit): the quoted branch of the secret-named path
/// hard-blocked on ANY non-empty literal, so `client.connect(token="my-secret-
/// token")` in a test fixture scored 0.000 and blocked every edit to the file
/// — the fourth false-positive class found in this dim today (after paths,
/// generated sequences and e-mail addresses).
///
/// Deliberately narrow so real credentials keep firing: any digit, any symbol
/// outside `-_.`, or 16+ characters disqualifies the exemption. A real token
/// carries entropy (`ghp_aBcD…`), a real password mixes classes (`P@ssw0rd1`),
/// and a long passphrase still blocks at the 16-character bar.
fn is_readable_placeholder(v: &str) -> bool {
    let v = v.trim();
    if v.is_empty() {
        return false;
    }
    // 02/11/2026 (analise sweep, `test-api-key-123` in kazuba-browser and
    // `X-API-Key: your-api-key-here` in the antt SDK tests): a placeholder-vocabulary
    // word makes a readable phrase of words and SHORT numbers a stand-in even with a
    // digit or a header colon — the vocabulary is "decisive regardless of length",
    // the digit bar only existed to keep opaque tokens out. A token beside the word
    // (`test-4eC39Hq…`) is not a phrase and keeps blocking.
    if is_word_number_phrase(v) && is_placeholder_vocabulary_phrase(v) {
        return true;
    }
    // Letters plus separators only — any digit or other symbol means the value
    // carries entropy or mixes character classes, i.e. a real credential. A
    // SPACE is allowed here because auth values legitimately read as prose
    // (`"Bearer test-token"`); the digit/symbol bar is what keeps real tokens
    // out (`"Bearer eyJhbGciOiJIUzI1NiJ9…"` has digits).
    let wordy = v
        .bytes()
        .all(|b| b.is_ascii_alphabetic() || matches!(b, b'-' | b'_' | b'.' | b' '));
    if !wordy {
        return false;
    }
    // Explicit placeholder vocabulary — these words are how humans SAY "put
    // your value here", so their presence is decisive regardless of length
    // (`your_api_key_here` is 17 chars but obviously not a credential).
    if has_placeholder_word(v) {
        return true;
    }
    // Otherwise fall back to length: short prose-like text is a stand-in; a
    // 16+ character passphrase of words is a real secret and still blocks.
    v.len() < 16
}

/// The vocabulary humans use to say "put your value here" (see
/// [`is_readable_placeholder`]).
const PLACEHOLDER_WORDS: &[&str] = &[
    "your",
    "here",
    "changeme",
    "change-me",
    "example",
    "sample",
    "dummy",
    "fake",
    "test",
    "placeholder",
    "redacted",
    "xxx",
    "todo",
    "fixme",
    "insert",
    "replace",
    "notasecret",
];

/// `v` carries a word of [`PLACEHOLDER_WORDS`] (case-insensitive substring).
fn has_placeholder_word(v: &str) -> bool {
    let lower = v.to_ascii_lowercase();
    PLACEHOLDER_WORDS.iter().any(|w| lower.contains(w))
}

/// Words that name WHAT a credential is (`api`, `key`, `token`…) without being
/// one — the filler of `X-API-Key: your-api-key-here` around the placeholder word.
const CREDENTIAL_NOUNS: &[&str] = &[
    "a",
    "access",
    "account",
    "api",
    "apikey",
    "app",
    "auth",
    "bearer",
    "change",
    "client",
    "credential",
    "credentials",
    "db",
    "header",
    "id",
    "key",
    "keys",
    "me",
    "my",
    "pass",
    "passwd",
    "password",
    "private",
    "pwd",
    "secret",
    "service",
    "string",
    "the",
    "token",
    "user",
    "username",
    "value",
    "x",
];

/// Every alphabetic segment of `v` IS (not merely contains) a word of
/// [`PLACEHOLDER_WORDS`] or [`CREDENTIAL_NOUNS`], and at least one is a placeholder
/// word. A substring hit is not enough (security review of 1305ff40, 02/11/2026):
/// `protest-heaven-sample-castle` holds `test` and `sample` yet is a passphrase, and
/// a single vocabulary segment must not launder the real words around it.
fn is_placeholder_vocabulary_phrase(v: &str) -> bool {
    let mut saw_placeholder = false;
    for seg in v
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
    {
        if seg.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let lower = seg.to_ascii_lowercase();
        if PLACEHOLDER_WORDS.contains(&lower.as_str()) {
            saw_placeholder = true;
        } else if !CREDENTIAL_NOUNS.contains(&lower.as_str()) {
            return false;
        }
    }
    saw_placeholder
}

/// Words (any case) and short numbers (≤4 digits) joined only by ` - _ . :` —
/// `test-api-key-123`, `X-API-Key: your-api-key-here`, `YOUR_API_KEY_1`. A segment
/// that mixes letters and digits (`4eC39HqLyjWD…`) is the shape of a token, so the
/// phrase is refused and the value falls through to the secret reading.
fn is_word_number_phrase(v: &str) -> bool {
    let separator = |b: u8| matches!(b, b' ' | b'-' | b'_' | b'.' | b':');
    v.bytes().all(|b| b.is_ascii_alphanumeric() || separator(b))
        && v.split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|s| !s.is_empty())
            .all(is_plain_word_or_short_number)
}

/// An alphabetic word (any case, ≤24 letters) or a number of ≤4 digits.
fn is_plain_word_or_short_number(s: &str) -> bool {
    if s.bytes().all(|b| b.is_ascii_digit()) {
        s.len() <= 4
    } else {
        s.len() <= 24 && s.bytes().all(|b| b.is_ascii_alphabetic())
    }
}

/// True when a quoted secret-named RHS is a shell expansion read at run time —
/// `TOKEN="$(cat "${TOKEN_FILE}")"`, `KEY="${API_KEY}"`, `` T="`pass show x`" ``.
///
/// The unquoted branch already rejects `$…`; the quoted one split the line at
/// every `"` and read `$(cat ` as a non-empty literal, so a hook reading its token
/// from `~/.config` failed F2.4 in the public touring repository (Canvas D,
/// 15/09/2026). A literal before the expansion (`"ghp_…$X"`) is not exempt, and
/// neither is a credential hash that merely starts with `$` (`"$2b$12$…"`,
/// `"$argon2id$…"`): only `$(`, `${`, a backtick or a whole `$UPPER_NAME` count.
fn is_shell_expansion(rhs: &str) -> bool {
    let value = rhs.trim().trim_end_matches([';', ',']);
    let value = value.strip_prefix('"').unwrap_or(value);
    let value = value.strip_suffix('"').unwrap_or(value);
    if value.starts_with("$(") || value.starts_with("${") || value.starts_with('`') {
        return true;
    }
    value.strip_prefix('$').is_some_and(|name| {
        name.bytes()
            .next()
            .is_some_and(|b| b.is_ascii_uppercase() || b == b'_')
            && name
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
    })
}

/// A relative directory path: 3+ `/`-separated segments, path words, at least
/// one hyphenated segment — `docs/plans/2026-08-11-memory-hashtag-library/tmp`,
/// and now with CAPITALS allowed: `data/processed/process_inventory/
/// malha-paulista-revisao-extraordinaria/converted/SEI_50500.023913_2026_89`
/// (the analise-50 judge, 25/09/2026 — a data path whose process-number tail
/// mixes cases and dots).
///
/// [`is_relative_file_path`] needs a file extension, so a directory scored as a
/// high-entropy literal (length 52, entropy 4.56; Canvas D, 15/09/2026). An
/// opaque token is not made of hyphenated words: base64 carries upper case, and
/// a hex or base36 run has no hyphenated segment — the hyphenated-word
/// requirement (not the lowercase one) is what keeps base64-with-slashes
/// blocking, pinned by the existing test.
fn is_relative_dir_path(v: &str) -> bool {
    let segments: Vec<&str> = v.trim_end_matches('/').split('/').collect();
    segments.len() >= 3
        && segments.iter().all(|s| {
            // A LEADING `@` opens an npm scope (`node_modules/@scope/pkg`, 27/09/2026);
            // base64 never carries `@`, and one mid-segment is still refused.
            let s = s.strip_prefix('@').unwrap_or(s);
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        })
        && segments.iter().any(|s| s.trim_matches('-').contains('-'))
}

/// Separators a READABLE literal may join its segments with. `%`, `$`, `@`, `!`, `*`
/// and the like are deliberately absent: passwords and tokens carry them, names and
/// query strings do not.
const PHRASE_SEPARATORS: &[u8] = b"-_.+/|&=?:#";

/// A literal that READS — words, numbers and short ids joined by [`PHRASE_SEPARATORS`]:
/// a SEI piece name (`[03]-26510579_Anexo.zip__Relatorio__PIA-8041_part_001`), a NUP
/// key (`50500.023913/2026-89|template_instance|reequilibrio`), a query string
/// (`/x/S03032026.zip?version=1.0&t=1&download=true`), a checksum path
/// (`…/MEMORIAL_8.1.1_AS_BUILT_R00.sha256`), a kebab phrase
/// (`the-original-mvt-payload-0123456789`).
///
/// 02/11/2026, the analise sweep (13 literals in 8 files at `high-entropy-literal`):
/// Shannon entropy over the CONCATENATION measures the variety of the field names,
/// not randomness. The honest test is on the SEGMENTS — ≥3 of them, at least two real
/// words, and EVERY one a word, a number or a short id. One opaque segment (mixed
/// case/digits, a base64 run) and the literal is a token again, which is why
/// `/api?key=aB3x…`, `page=1&sess=aB3x…` and `[03]-26510579_aB3x…` keep blocking.
/// The split is on non-alphanumerics, never on `/` alone: a base64 secret with slashes
/// (`wJal…/K7MD…/bPxR…`) has mixed-case segments and fails the same test.
fn is_readable_phrase(v: &str) -> bool {
    let body = strip_index_prefix(v);
    if !body
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || PHRASE_SEPARATORS.contains(&b))
    {
        return false;
    }
    let segments: Vec<&str> = body
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();
    segments.len() >= 3
        && segments.iter().filter(|s| is_word_segment(s)).count() >= 2
        && segments.iter().all(|s| is_readable_segment(s))
}

/// A leading `[NN]` — the ordinal of a SEI piece name (`[03]-26510579_…`) — stripped;
/// any other `[` is left in place, where [`is_readable_phrase`] refuses it.
fn strip_index_prefix(v: &str) -> &str {
    let Some((index, tail)) = v.strip_prefix('[').and_then(|r| r.split_once(']')) else {
        return v;
    };
    if (1..=4).contains(&index.len()) && index.bytes().all(|b| b.is_ascii_digit()) {
        tail
    } else {
        v
    }
}

/// An alphabetic segment shaped like a word: an ACRONYM (`BNDES`) or lower / Capitalized
/// / camelCase / PascalCase (`megaprojects`, `Anexo`, `abrirAtoPublico`) — every
/// uppercase letter opens a lowercase run, which random mixed-case letters rarely do.
fn is_word_segment(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() || b.len() > 24 || !b.iter().all(u8::is_ascii_alphabetic) {
        return false;
    }
    b.iter().all(u8::is_ascii_uppercase)
        || b.iter().enumerate().all(|(i, c)| {
            !c.is_ascii_uppercase() || b.get(i + 1).is_some_and(u8::is_ascii_lowercase)
        })
}

/// A segment of a readable literal: a word, a number (≤14 digits) or a short id of one
/// word run and one digit run (`S03032026`, `MARCO1`, `sha256`, `10mb`).
fn is_readable_segment(s: &str) -> bool {
    let (head, tail, digits_first) = split_runs(s);
    match (digits_first, tail.is_empty()) {
        (true, true) => is_number_run(head),
        (false, true) => is_word_segment(head),
        (false, false) => is_word_segment(head) && is_number_run(tail),
        (true, false) => is_number_run(head) && is_word_segment(tail),
    }
}

/// A digit run short enough to be a number, an ordinal or a code (≤14 digits).
fn is_number_run(n: &str) -> bool {
    !n.is_empty() && n.len() <= 14 && n.bytes().all(|b| b.is_ascii_digit())
}

/// `s` cut where digits turn into non-digits (or back), plus whether it opened with a
/// digit. A third run lands in the tail, which the callers then refuse.
fn split_runs(s: &str) -> (&str, &str, bool) {
    let bytes = s.as_bytes();
    let digits_first = bytes.first().is_some_and(u8::is_ascii_digit);
    let cut = bytes
        .iter()
        .position(|b| b.is_ascii_digit() != digits_first)
        .unwrap_or(bytes.len());
    let (head, tail) = s.split_at(cut);
    (head, tail, digits_first)
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
            .or_else(|| blank_hash_comment(bytes, i, &mut out))
            .unwrap_or_else(|| {
                out.push(bytes[i]);
                1
            });
        i += consumed;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Blank a `#` line comment (Python, shell, YAML, TOML) to end of line.
///
/// A projeção que blanka `//` e deixa `#` é INCONSISTENTE: o detector é
/// multilíngue, e um comentário em Python era lido como CÓDIGO — o `:` de um
/// padrão documentado (`# "token": "…"`) virava delimitador de código, e a
/// regra de atribuição disparava dentro de um COMENTÁRIO (medido em 31/10/2026:
/// o censo do workspace foi de 0,985/0 bloqueios para 0,000/1 blocker por causa
/// de um comentário que DOCUMENTA o padrão).
///
/// O `#` só abre comentário no início da linha (depois de espaços) ou precedido
/// de espaço em branco — a convenção de Python, shell e YAML. `x=1#c` (que o
/// Python aceita) fica como estava: esta é a correção de um furo, não uma
/// mudança de política.
///
/// A varredura de LITERAL continua lendo a linha CRUA (regra 3): um token
/// colado num comentário é vazamento de verdade e continua bloqueando.
fn blank_hash_comment(bytes: &[u8], start: usize, out: &mut Vec<u8>) -> Option<usize> {
    if bytes[start] != b'#' {
        return None;
    }
    if start > 0 && !matches!(bytes[start - 1], b' ' | b'\t') {
        return None;
    }
    let mut j = start;
    while j < bytes.len() && bytes[j] != b'\n' {
        out.push(b' ');
        j += 1;
    }
    Some(j - start)
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
fn first_offending(raw: &str) -> Option<(usize, &'static str)> {
    scan_lines(&mask_documented_placeholders(raw)).0
}

/// Both per-line signals, computed over ONE raw↔projection pairing.
///
/// Returns `(first strong hit as (1-based line, the rule that fired), weak)`.
/// The projection ([`strip_strings_and_comments`]) is byte-length preserving,
/// so a byte index found in a projection line addresses the SAME byte of the
/// raw line — the invariant the delimiter resolution in
/// [`line_strong_rule`] rests on, pinned by
/// `the_code_projection_addresses_the_same_bytes_as_the_raw_source`.
///
/// One pairing site on purpose: [`scan`], [`scan_text`] and
/// [`first_offending`] each ran their own `raw.lines().position(…)` until
/// 04/09/2026, which is how the strong path came to disagree with the weak
/// one about what a string literal is. The rule name rides along since
/// 25/09/2026 (touring-36): the evidence must name `arquivo:linha:regra` of
/// each finding — a line number alone left the author bisecting WHICH of
/// the detector's families fired.
fn scan_lines(raw: &str) -> (Option<(usize, &'static str)>, bool) {
    let code = strip_strings_and_comments(raw);
    let mut codes = code.lines();
    let mut first_strong = None;
    for (idx, line) in raw.lines().enumerate() {
        // Fail-CLOSED if the projection ever runs short: the raw line becomes its
        // own projection, i.e. the pre-fix (strictly more sensitive) reading.
        let code_line = codes.next().unwrap_or(line);
        if let Some(rule) = line_is_strong(line, code_line) {
            first_strong = Some((idx + 1, rule));
            break;
        }
    }
    (first_strong, code.lines().any(names_secret))
}

/// Credentials the vendors publish in their own documentation as examples.
///
/// They are shaped exactly like real credentials (that is the point of an example) and
/// are committed in thousands of READMEs, test fixtures and IaC snippets, so a file
/// that merely shows the AWS docs' sample pair read F2.4 = 0.000 and BLOCKED every edit.
/// An exact-literal list is the narrowest correct exemption: a real key is never one of
/// these strings, and one character more or less (`…EXAMPLEX`) is no longer the sample.
///
/// The literals are assembled from fragments, as the hook-side redactors do: this very
/// file is scanned by the pre-write hook (`scan_text`), which has no own-source
/// allowlist, and a whole sample key here would block the edit that adds it.
fn documented_placeholders() -> &'static [String] {
    static CELL: OnceLock<Vec<String>> = OnceLock::new();
    CELL.get_or_init(|| {
        vec![
            // AWS "IAM user guide / credentials" sample pair.
            ["AKIA", "IOSFODNN7", "EXAMPLE"].concat(),
            ["wJalrXUtnFEMI/K7MDENG/", "bPxRfiCYEXAMPLEKEY"].concat(),
            // AWS sample pair of the older API reference pages.
            ["AKIA", "I44QH8DHB", "EXAMPLE"].concat(),
            ["je7MtGbClwBF/2Zp9Utk/", "h3yCo8nvbEXAMPLEKEY"].concat(),
        ]
    })
}

/// `text` with every documented placeholder ([`documented_placeholders`]) blanked to
/// the same number of `x` bytes, so line numbers and the byte alignment between a raw
/// line and its code projection survive. A literal only counts when it stands alone:
/// neither neighbour may continue the token (`…EXAMPLEX` is another string).
fn mask_documented_placeholders(text: &str) -> Cow<'_, str> {
    let standalone = |s: &str, at: usize, len: usize| {
        let before = s[..at].chars().next_back();
        let after = s[at + len..].chars().next();
        !before.is_some_and(|c| c.is_ascii_alphanumeric())
            && !after.is_some_and(|c| c.is_ascii_alphanumeric())
    };
    let mut masked: Option<String> = None;
    for literal in documented_placeholders() {
        let current = masked.as_deref().unwrap_or(text);
        let hits: Vec<usize> = current
            .match_indices(literal.as_str())
            .map(|(at, _)| at)
            .filter(|&at| standalone(current, at, literal.len()))
            .collect();
        if hits.is_empty() {
            continue;
        }
        let mut next = current.to_string();
        for at in hits {
            next.replace_range(at..at + literal.len(), &"x".repeat(literal.len()));
        }
        masked = Some(next);
    }
    match masked {
        Some(owned) => Cow::Owned(owned),
        None => Cow::Borrowed(text),
    }
}

/// (1) A known provider marker (`ghp_`, `AKIA`, a PEM header, …) appears in `text`.
fn has_strong_marker(text: &str) -> bool {
    STRONG_MARKERS
        .iter()
        .any(|m| marker_carries_a_token(text, m))
}

/// Menor corpo que um token de provedor pode ter depois do prefixo. AWS usa
/// `AKIA` + 16; GitHub, Slack e Stripe usam bem mais. Abaixo disso o que existe
/// no texto e' uma citacao do prefixo, nao uma credencial.
const MIN_TOKEN_BODY: usize = 16;

/// `true` quando `marker` aparece em `text` **seguido de corpo de token**.
///
/// Cross-audit 03/09/2026 — antes disto a regra era `text.contains(marker)` puro,
/// e por isso qualquer prosa que escrevesse o prefixo virava bloqueador P0: no
/// censo de 1.763 arquivos o unico 0.000 foi `transcript_miner.rs`, acusado na
/// linha de um COMENTARIO que citava `` `ghp_...` ``. Um repositorio que documenta
/// os proprios padroes de seguranca nao pode ter um detector que confunde a
/// citacao com o vazamento — e afrouxar seria pior: o comentario continua sendo
/// varrido, porque um token colado num comentario e' vazamento de verdade. O que
/// separa os dois casos e' o corpo, nao o lugar.
///
/// Cabecalhos PEM (`-----BEGIN …`) sao auto-contidos e nao trazem corpo na mesma
/// posicao, entao seguem no teste de contencao simples.
fn marker_carries_a_token(text: &str, marker: &str) -> bool {
    if marker.contains("-----") {
        return text.contains(marker);
    }
    text.match_indices(marker).any(|(at, _)| {
        text[at + marker.len()..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .count()
            >= MIN_TOKEN_BODY
    })
}

/// (1b) A structural provider token that needs no entropy (JWT / SendGrid /
/// OpenAI / Anthropic / Slack-app). Split on whitespace + literal delimiters so a
/// token embedded in code or in a `.env` `KEY=value` line is isolated and
/// shape-checked as a WHOLE token (no bare-substring false positives).
fn has_structural_token(text: &str) -> bool {
    text.split(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '"' | '\'' | '`' | '=' | ',' | '(' | ')' | ';' | '[' | ']'
            )
    })
    .any(|tok| is_jwt(tok) || is_sendgrid(tok) || is_provider_dashkey(tok))
}

/// Every strong signal a SINGLE line can carry on its own — the file-wide
/// predicates applied to one line, plus the per-line paths. Returns the NAME
/// of the rule that fired (the evidence's `regra` in `arquivo:linha:regra`).
///
/// This is what makes a hit LOCATABLE. Both file-wide predicates are evaluated
/// here from the same functions [`scan`] uses, so "the file is dirty" and "this
/// line is why" can never disagree. Before 04/09/2026 there were two rival
/// locators — [`first_offending`] ran the whole `scan` per line (markers
/// included) while `scan_text` ran only [`line_strong_rule`] (markers
/// excluded), so the same `ghp_…` file reported line 3 through one call and
/// `None` through the other.
fn line_is_strong(line: &str, code: &str) -> Option<&'static str> {
    if has_strong_marker(line) {
        return Some("provider-marker");
    }
    if has_structural_token(line) {
        return Some("structural-token");
    }
    line_strong_rule(line, code)
}

fn scan(raw: &str) -> (bool, bool) {
    // Documented sample credentials (the AWS docs pair) are not leaks.
    let masked = mask_documented_placeholders(raw);
    let raw = masked.as_ref();
    // (1)/(1b) file-wide: a marker or a structural token anywhere → strong.
    if has_strong_marker(raw) || has_structural_token(raw) {
        return (true, false);
    }
    // (1c) env-style `NAME=sk-<hex24+>` anywhere in the text — including inside
    // JSON string literals (e.g. an MCP server `args` value like
    // `"GEMINI_API_KEY=sk-<48hex> srv"`). The `sk-` + raw-hex body has entropy
    // ~4.0 bits/char < 4.5 threshold, so the generic high-entropy path misses it;
    // the assignment detector misses it when the `=` lives inside a quoted string.
    if has_env_sk_hex_pattern(raw) {
        return (true, false);
    }
    // (2) per-line strong signals (connection strings, secret-named assignments,
    // generic high-entropy literals) — these MUST see string *values*, so they
    // read the raw source; only the ASSIGNMENT DELIMITER is resolved on the
    // projection. (3) weak: a secret keyword used as a real code identifier
    // (e.g. a bare `let password;`), scanned on the code-only projection so a
    // keyword appearing only in prose — a single- or MULTI-line string literal,
    // or a comment — never fires it. Both come from one pass.
    let (strong_line, weak) = scan_lines(raw);
    if strong_line.is_some() {
        return (true, false);
    }
    (false, weak)
}

/// Detects `UPPERCASE_NAME=sk-<hex24+>` anywhere in `text`, even when the
/// assignment is embedded inside a JSON string literal (e.g. a CLI arg value
/// `"GEMINI_API_KEY=sk-a1b2c3d4e5f6… srv"`).
///
/// Why a dedicated detector:
///   • The `sk-` + lowercase-hex body has entropy ~4.0 bits/char < 4.5, so
///     the generic high-entropy literal path does not fire.
///   • `split_at_code_delimiter` uses the string-blanked projection to locate
///     `=`, so an `=` inside a quoted string is invisible to the assignment
///     detector.
///   • This scanner operates on the RAW text — no projection — so the pattern
///     is found regardless of nesting depth.
///
/// False-positive guard: requires (a) an UPPERCASE word of ≥ 2 chars before
/// `=sk-`, (b) ≥ 24 lowercase hex chars immediately after `sk-`, and (c) no
/// structured `sk-ant-api`/`sk-proj-` prefix (those are caught by
/// `has_structural_token` / `has_strong_marker` first and return before this
/// function is reached — the check here is additive, not overlapping).
fn has_env_sk_hex_pattern(text: &str) -> bool {
    let b = text.as_bytes();
    for (at, _) in text.match_indices("=sk-") {
        // After "=sk-": require ≥ 24 lowercase hex bytes.
        let hex_len = b[at + 4..]
            .iter()
            .take_while(|&&c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
            .count();
        if hex_len < 24 {
            continue;
        }
        // Before "=": require `[A-Z][A-Z0-9_]+` ending right at `at`.
        let word_len = b[..at]
            .iter()
            .rev()
            .take_while(|&&c| matches!(c, b'A'..=b'Z' | b'0'..=b'9' | b'_'))
            .count();
        let starts_upper = at
            .checked_sub(word_len)
            .and_then(|s| b.get(s))
            .is_some_and(|&c| c.is_ascii_uppercase());
        if word_len >= 2 && starts_upper {
            return true;
        }
    }
    false
}

/// File-level opt-out marker for fixtures that embed SAMPLE secrets on purpose
/// (detect-secrets / gitleaks `pragma: allowlist secret` convention).
const ALLOW_SECRETS_PRAGMA: &str = "touring-quality:allow-secrets";

/// Text-level verdict of the F2.4 detector (S2, 2026-09-01).
///
/// The gate's `measure` scores a file on disk; a pre-write hook sees the
/// content BEFORE it exists. This is the same detector over a `&str`, so
/// the hook and the gate cannot drift apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecretScan {
    /// A hardcoded secret (P0): provider marker or token, connection-string
    /// credentials, secret-named assignment, or a high-entropy literal.
    pub strong: bool,
    /// A secret keyword used as a code identifier, without an assigned value.
    pub weak: bool,
    /// 1-based line of the first strong hit when it is line-addressable
    /// (`None` only when nothing hit, or when no single line carries the signal
    /// on its own).
    pub first_line: Option<usize>,
    /// The rule that fired at `first_line` (25/09/2026, touring-36): the
    /// evidence names `arquivo:linha:regra` of each finding, and the rule is
    /// the `regra` — the family the author must look for, never the value.
    pub rule: Option<&'static str>,
    /// The text carries `ALLOW_SECRETS_PRAGMA` — explicitly allowlisted.
    pub allowlisted: bool,
}

/// Run the F2.4 detector over `raw` (the proposed content of a Write/Edit).
pub fn scan_text(raw: &str) -> SecretScan {
    if raw.contains(ALLOW_SECRETS_PRAGMA) {
        return SecretScan {
            strong: false,
            weak: false,
            first_line: None,
            rule: None,
            allowlisted: true,
        };
    }
    let (strong, weak) = scan(raw);
    // Same locator the gate's evidence uses. A marker hit is now LOCATABLE here
    // too: this returned `None` for every `ghp_…` file until 04/09/2026 because
    // it ran a narrower predicate than the gate did, so the pre-write hook could
    // say "secret detected" without saying where.
    let (first_line, rule) = if strong {
        first_offending(raw)
            .map(|(line, rule)| (Some(line), Some(rule)))
            .unwrap_or((None, None))
    } else {
        (None, None)
    };
    SecretScan {
        strong,
        weak,
        first_line,
        rule,
        allowlisted: false,
    }
}

/// Per-line strong-signal check: connection-string credentials, a secret-named
/// assignment (quoted literal OR unquoted hex/high-entropy RHS), or a generic
/// high-entropy quoted literal — returned as the rule that fired. Extracted
/// from [`scan`] to keep its CC low.
fn line_strong_rule(line: &str, code: &str) -> Option<&'static str> {
    // (1c) connection string with inline credentials (`scheme://u:pw@host`).
    if has_connstring_creds(line) {
        return Some("connstring-credentials");
    }
    // (2/2b) secret-named assignment. A Go struct-field declaration with its tag
    // (`Password string \`json:"password,omitempty"\``) names a field and its wire key;
    // nothing is assigned (02/11/2026, analise `sdks/go/configuration.go:54`).
    let go_field = is_go_struct_tag_field(line);
    if !go_field && named_assignment_is_strong(line, code) {
        return Some("secret-named-assignment");
    }
    // (2c) T-3 (31/10/2026): a ligação NESTED no lado direito.
    if !go_field && nested_secret_binding(line, code) {
        return Some("secret-named-assignment");
    }
    // (3) generic high-entropy literal anywhere on the line → strong.
    if extract_quoted(line)
        .iter()
        .any(|l| looks_like_secret_value(l))
    {
        return Some("high-entropy-literal");
    }
    None
}

/// A Go struct field with a struct tag: `Name Type \`key:"value" key2:"value2"\``, an
/// optional `// comment` after it. The tag is metadata for encoders (`json`, `yaml`, `xml`,
/// `db`…); its quoted values are wire names, never assigned secrets. A token-shaped value
/// inside a tag is still caught by the high-entropy rule, which runs regardless.
fn is_go_struct_tag_field(line: &str) -> bool {
    let body = line.split("//").next().unwrap_or(line).trim();
    let Some(tick) = body.find('`') else {
        return false;
    };
    let (decl, tag) = body.split_at(tick);
    let mut parts = decl.split_whitespace();
    let (Some(name), Some(ty), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    let is_ident = |s: &str| {
        s.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    };
    let is_type = |s: &str| {
        s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '*' | '[' | ']'))
    };
    if !(is_ident(name) && is_type(ty)) || !tag.ends_with('`') || tag.len() < 2 {
        return false;
    }
    // Every space-separated pair of the tag is `key:"value"`.
    tag[1..tag.len() - 1].split_whitespace().all(|pair| {
        pair.split_once(':').is_some_and(|(k, v)| {
            is_ident(k) && v.len() >= 2 && v.starts_with('"') && v.ends_with('"')
        })
    })
}

/// Split `line` at its first code-level `=` (else `:`), located in `code` — the
/// string-and-comment-blanked projection of the SAME line, byte-for-byte aligned
/// with it.
///
/// Why the position must come from the projection: until 04/09/2026 the split ran
/// on the raw line, so a delimiter living INSIDE a string literal split it. The
/// line
///
/// ```text
/// for token in line.replace(":", " ").replace("=", " ").split():
/// ```
///
/// split at the `=` inside `"="`, which put `token` on the left and a quoted value
/// on the right and hard-blocked every edit to its file. A census over 592 files
/// (skills + docs + hook crates) found 4 live blockers; 2 were this one idiom.
///
/// Why ONLY the position comes from the projection: a real secret lives inside a
/// literal, and a JSON key names the secret inside one too (`"api_key": "…"`), so
/// `names_secret` and the RHS value keep reading the RAW line.
///
/// Fail-CLOSED on a misaligned projection: fall back to the raw split, which is
/// the strictly more sensitive pre-fix reading.
/// The name an assignment binds: the LHS after its last `,` `(` `{` `[`, so
/// `f(a, password` → `password`, `{"api_key"` → `"api_key"`, `data["token"]` →
/// `"token"]`, while `let token: T` stays whole. Before 29/10/2026 the whole LHS was
/// searched, and a keyword that was only a VALUE earlier on the line blocked the file:
/// `dict(_use(2, "Bash", {"command": "pwd"}), timestamp="…")` named `pwd`.
fn assigned_name(lhs: &str) -> &str {
    // The delimiters are ASCII, so `i + 1` is a char boundary.
    let name = lhs
        .rfind([',', '(', '{', '['])
        .map_or(lhs, |i| &lhs[i + 1..]);
    // 02/11/2026 (analise `apps/web/src/lib/pagina.ts:91`): an ODD number of quotes
    // means the cut fell INSIDE a string literal — `token.href)}" target` out of
    // `"${escapar(token.href)}" target="_blank"` — so its text is not a name. What
    // follows the closing quote is the code, and the code is what names the target.
    // A quoted KEY (`"api_key"`, `"token"] `) closes its quotes and is untouched.
    if name.bytes().filter(|&b| b == b'"').count() % 2 == 1 {
        return name.rsplit('"').next().unwrap_or(name);
    }
    name
}

/// `lhs` with its string literals and comments blanked: the same bytes of the
/// line's code projection. Falls back to `lhs` itself when the projection does not
/// address the same bytes (the stricter reading).
fn lhs_code<'a>(lhs: &'a str, line: &str, code: &'a str) -> &'a str {
    if code.len() == line.len() {
        code.get(..lhs.len()).unwrap_or(lhs)
    } else {
        lhs
    }
}

/// The target a secret-named binding writes to: `None` when `lhs` binds no secret
/// name; `Some(lhs)` for a single target (the stand-in rules read it); `Some("")`
/// for a destructuring, whose literals belong to SEVERAL targets, so no name-specific
/// stand-in applies to them.
///
/// The name right before the delimiter (which may be a quoted key, `{"api_key": …}`)
/// binds a secret. ANY identifier of the LHS binds one only for destructuring — the
/// forms `let (password, other) = …` and `password, other = …` (29/10/2026: the first
/// was a fail-open older than the release of 28/10; the second broke in 20d2b05a,
/// which looked only at the last name). Anywhere else a secret-named identifier
/// earlier on the LHS is ANOTHER parameter or operand, not what the delimiter binds:
/// `def __init__(self, api_key: str, model_name: str = "gpt-4"):` (02/11/2026,
/// analise `src/agents/core/llm_provider_example.py:200`). A keyword that is only a
/// string VALUE inside the LHS (`{"command": "pwd"}`) is blank in the projection.
///
/// `allow_destructuring` is false for the nested reading: a prefix of a right-hand
/// side is never a destructuring pattern (`[{"token"` opens a list, not a target).
fn secret_binding<'a>(
    lhs: &'a str,
    line: &str,
    code: &str,
    allow_destructuring: bool,
) -> Option<&'a str> {
    let named_last = names_secret(assigned_name(lhs));
    let code_lhs = lhs_code(lhs, line, code);
    if allow_destructuring && is_destructuring(code_lhs) {
        return (named_last || names_secret(code_lhs)).then_some("");
    }
    named_last.then_some(lhs)
}

/// (2/2b) The top-level split binds a secret name to a value that is itself strong.
///
/// Prefer the `=` split so a type annotation `let token: T = "…"` is handled by the
/// assignment, not the colon. A quoted RHS with a non-empty literal hard-blocks (unless
/// it is a stand-in, see [`is_stand_in`]); an UNQUOTED RHS blocks only when it is itself
/// a hex / high-entropy token (`.env` / shell `export` / YAML). A line that merely
/// *mentions* a keyword next to strings (a meta tuple, a doc comment,
/// `= std::env::var("…")`) is NOT a blocking assignment.
fn named_assignment_is_strong(line: &str, code: &str) -> bool {
    let Some((lhs, rhs)) = split_at_code_delimiter(line, code) else {
        return false;
    };
    let Some(target) = secret_binding(lhs, line, code, true) else {
        return false;
    };
    let destructuring = target.is_empty();
    let rhs_start = rhs.trim_start();
    if rhs_start.starts_with('"') || (destructuring && rhs_start.starts_with(['(', '['])) {
        // A destructuring RHS lists the values of several targets: all of them count.
        // A single target owns only ITS value (up to the next top-level `,`/`;`/closer).
        let value = if destructuring {
            rhs
        } else {
            bound_value(rhs, code_tail(line, code, rhs))
        };
        quoted_value_is_secret(target, rhs, value)
    } else {
        looks_like_secret_value_named(rhs)
    }
}

/// The projection of `tail` — a suffix of `line` — when `code` addresses the same
/// bytes as `line`; `None` when the projection is misaligned (the stricter reading).
fn code_tail<'a>(line: &str, code: &'a str, tail: &str) -> Option<&'a str> {
    if code.len() != line.len() {
        return None;
    }
    code.get(line.len() - tail.len()..)
}

/// The part of `rhs` that is the VALUE of the name just bound: up to the first
/// top-level `,` / `;` or unmatched closer, located on `rhs_code` — the same bytes
/// with strings and comments blanked, so a delimiter inside a literal never counts.
///
/// 02/11/2026, analise `test_t012_anexo_siglas.py:206`: in `{"token": "SC",
/// "motivo": "unidade da federação"}` the value of `token` is `"SC"`; the literal
/// after the comma is the value of ANOTHER key, which the nested reading judges on
/// its own name. Without a projection (or with one that is misaligned) the whole
/// right-hand side is the value — the stricter reading.
fn bound_value<'a>(rhs: &'a str, rhs_code: Option<&str>) -> &'a str {
    let Some(code) = rhs_code.filter(|c| c.len() == rhs.len()) else {
        return rhs;
    };
    let mut depth = 0usize;
    for (i, b) in code.bytes().enumerate() {
        match b {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' if depth == 0 => return rhs.get(..i).unwrap_or(rhs),
            b')' | b']' | b'}' => depth -= 1,
            b',' | b';' if depth == 0 => return rhs.get(..i).unwrap_or(rhs),
            _ => {}
        }
    }
    rhs
}

/// A quoted right-hand side carries a real value: it is not a shell expansion read at
/// run time, and at least one literal of `value` — the part [`bound_value`] kept — is
/// not a stand-in for the target `lhs` ([`is_stand_in`]). The expansion is judged on the
/// WHOLE `rhs` (`TOKEN="$(cat "${F}")"` closes its string before the `)`).
fn quoted_value_is_secret(lhs: &str, rhs: &str, value: &str) -> bool {
    !is_shell_expansion(rhs)
        && extract_quoted(value)
            .iter()
            .any(|l| !l.is_empty() && !is_stand_in(lhs, l))
}

/// A literal that reads as a stand-in, not a credential, for the target `lhs`:
/// a readable placeholder, a version, a lexical ordinal or a UI message.
fn is_stand_in(lhs: &str, literal: &str) -> bool {
    is_readable_placeholder(literal)
        || is_version_spec(literal)
        || is_lexical_ordinal(lhs, literal)
        || (is_message_holder_target(lhs) && is_prose(literal))
}

/// `"token": "1009"` — a LEXICAL token (a parser/NLP field) carrying an offset, a
/// counter or an id: a bare number of ≤4 digits under the name `token` only
/// (analise `test_medir_lastro_estruturado.py:57`). Bearer tokens are never 4-digit
/// decimals, while a PIN under `password`/`secret` IS a credential, so no other name
/// is spared and a fifth digit ends the exemption.
fn is_lexical_ordinal(lhs: &str, literal: &str) -> bool {
    (1..=4).contains(&literal.len())
        && literal.bytes().all(|b| b.is_ascii_digit())
        && last_identifier(lhs).eq_ignore_ascii_case("token")
}

/// The trailing identifier of `text` (`errors.password ` → `password`,
/// `"token"] ` → `token`): trailing punctuation and spaces are skipped first.
fn last_identifier(text: &str) -> &str {
    let t = text.trim_end_matches(|c: char| !(c.is_ascii_alphanumeric() || c == '_'));
    let start = t
        .bytes()
        .rposition(|b| !is_ident_byte(b))
        .map_or(0, |i| i + 1);
    t.get(start..).unwrap_or("")
}

/// The target is a MESSAGE holder — `errors.password`, `errors["password"]`,
/// `newErrors.password`, `messages.token`: the container is named like a bag of
/// user-facing text. The container must sit right before the member (`.` or `[`), so
/// `f(errors, password` is not one.
fn is_message_holder_target(lhs: &str) -> bool {
    const HOLDERS: &[&str] = &[
        "errors",
        "error",
        "errs",
        "messages",
        "message",
        "msgs",
        "labels",
        "label",
        "hints",
        "hint",
        "placeholders",
        "translations",
        "i18n",
    ];
    let t = lhs.trim_end_matches(|c: char| !(c.is_ascii_alphanumeric() || c == '_'));
    let leaf = last_identifier(t);
    let before = t
        .get(..t.len() - leaf.len())
        .unwrap_or("")
        .trim_end_matches(['"', '\'']);
    let Some(container) = before
        .strip_suffix('.')
        .or_else(|| before.strip_suffix('['))
    else {
        return false;
    };
    let holder = last_identifier(container).to_ascii_lowercase();
    HOLDERS.iter().any(|h| holder.ends_with(h))
}

/// Prose: ≥2 whitespace-separated words, each purely alphabetic (Unicode, with inner
/// `-`/`'`) or a plain number, punctuation allowed around them — `Senha é obrigatória`,
/// `Senha deve ter pelo menos 8 caracteres`. A word that mixes letters and digits
/// (`Tr0ub4dor`) or carries a credential symbol (`&3`) is not prose.
fn is_prose(v: &str) -> bool {
    let word_ok = |w: &str| {
        let core = w.trim_matches(|c: char| {
            matches!(
                c,
                '.' | ',' | ';' | ':' | '!' | '?' | '(' | ')' | '"' | '\''
            )
        });
        !core.is_empty()
            && (core.bytes().all(|b| b.is_ascii_digit())
                || core
                    .chars()
                    .all(|c| c.is_alphabetic() || matches!(c, '-' | '\'' | '’')))
    };
    let mut words = 0usize;
    v.split_whitespace().all(|w| {
        words += 1;
        word_ok(w)
    }) && words >= 2
}

/// O lado direito LIGA um nome secreto? (T-3, 31/10/2026)
///
/// A MESMA leitura do split de topo, aplicada a cada delimitador de código do
/// lado direito: o nome ligado (o predicado de 29/10) E um valor forte.
///
/// `cfg = connect(password="…")` e `creds = {"api_key": "…"}` — o nome do
/// segredo está no lado DIREITO de outra atribuição, e o split de topo punha
/// o LHS em `cfg` / `creds`: o nome do segredo nunca chegava a ser nome.
fn binds_secret_nested(rhs: &str, rhs_code: &str) -> bool {
    for (pos, _) in rhs_code.match_indices(['=', ':']) {
        // `==` e `=>` NÃO são ligação: o nome do lado esquerdo de uma
        // COMPARAÇÃO não liga nada (`v["token"] == "…"` — o `"token"` é uma
        // CHAVE lida, não um binding; com `==` tratado como `=` a linha
        // bloqueava, medido no censo de 31/10/2026).
        //
        // Os DOIS `=` do `==` são pulados: o scanner acha cada caractere, e
        // pular só o primeiro deixava o segundo virar delimitador.
        //
        // 02/11/2026: `!=`, `<=` e `>=` são a mesma classe (comparação) —
        // `is_comparison_eq` é o predicado único, o mesmo do split de topo.
        let bytes = rhs_code.as_bytes();
        if bytes[pos] == b'=' && (bytes.get(pos + 1) == Some(&b'>') || is_comparison_eq(bytes, pos))
        {
            continue;
        }
        let antes = &rhs[..pos];
        let depois = &rhs[pos + 1..];
        // `allow_destructuring = false`: um prefixo do lado direito nunca é padrão
        // de desestruturação, e um nome secreto EARLIER nele é outro operando.
        let Some(target) = secret_binding(antes, rhs, rhs_code, false) else {
            continue;
        };
        let strong = if depois.trim_start().starts_with('"') {
            let value = bound_value(depois, rhs_code.get(pos + 1..));
            quoted_value_is_secret(target, depois, value)
        } else {
            looks_like_secret_value_named(depois)
        };
        if strong {
            return true;
        }
    }
    false
}

/// A linha LIGA um nome secreto no LADO DIREITO de outra atribuição?
///
/// T-3 (31/10/2026): o split de topo corta no PRIMEIRO `=`, e o nome do
/// segredo cai no lado direito — `cfg = connect(password="…")` e
/// `creds = {"api_key": "…"}` nunca chegaram a ser nomes (o gap que o próprio
/// teste de 29/10 registrava).
///
/// A disciplina é a do split de topo (29/10/2026): a POSIÇÃO do delimitador vem
/// da projeção sem strings (um literal não é delimitador, e uma string não é
/// nome); o NOME e o VALOR leem a linha crua (a chave `"api_key"` mora DENTRO
/// de um literal).
fn nested_secret_binding(line: &str, code: &str) -> bool {
    if code.len() != line.len() {
        return false; // projeção desalinhada: sem posição confiável
    }
    let Some((_, rhs)) = split_at_code_delimiter(line, code) else {
        return false;
    };
    let ini = line.len() - rhs.len();
    let Some(rhs_code) = code.get(ini..) else {
        return false;
    };
    binds_secret_nested(rhs, rhs_code)
}

/// A destructuring LHS (strings already blanked): `let (a, b)`, `let [a, b]`,
/// `const [a, b]`, `(a, b)`, or a bare `a, b` outside any call. Its RHS is a
/// tuple or list of the values, so a literal inside `(…)`/`[…]` is the value.
fn is_destructuring(lhs_code: &str) -> bool {
    let mut t = lhs_code.trim();
    for kw in ["let ", "const ", "var ", "mut "] {
        t = t.strip_prefix(kw).map_or(t, str::trim_start);
    }
    t.starts_with(['(', '[']) || (t.contains(',') && !t.contains(['(', '{', '[']))
}

fn split_at_code_delimiter<'a>(line: &'a str, code: &str) -> Option<(&'a str, &'a str)> {
    if code.len() != line.len() {
        return line.split_once('=').or_else(|| line.split_once(':'));
    }
    let at = first_binding_eq(code).or_else(|| code.find(':'))?;
    // `=` and `:` are ASCII, so `at + 1` is a boundary whenever `at` is, and
    // `at < code.len() == line.len()` keeps the tail slice in range.
    line.is_char_boundary(at)
        .then(|| (&line[..at], &line[at + 1..]))
}

/// Byte index of the first `=` of `code` that BINDS — the comparison operators of
/// [`is_comparison_eq`] are skipped. When no `=` binds, the caller falls back to `:`.
fn first_binding_eq(code: &str) -> Option<usize> {
    let bytes = code.as_bytes();
    code.match_indices('=')
        .map(|(at, _)| at)
        .find(|&at| !is_comparison_eq(bytes, at))
}

/// The `=` at byte `at` of `code` belongs to a COMPARISON — `==`, `===`, `!=`, `!==`,
/// `<=`, `>=` — not to an assignment.
///
/// 02/11/2026, analise `scripts/dspy/setup_kimi_dspy.py:77` (`if api_key != "…":`) and
/// `scripts/sei_premium/tests/test_theme.py:86` (`assert theme.token("…") != "#101010"`):
/// the split took the `=` of `!=` as the binding, so the secret NAME on the left of a
/// comparison met the literal it is compared to. Compound assignments (`+=`, `-=`,
/// `|=`, `<<=`, `>>=`) and walrus/short declarations (`:=`) still bind. A `>` stuck to
/// the `=` closes a GENERIC (`Vec<u8>= …`) unless it stands apart (` >= `), so a
/// declaration written without spaces keeps binding (fail-closed).
fn is_comparison_eq(code: &[u8], at: usize) -> bool {
    if code.get(at + 1) == Some(&b'=') {
        return true; // first `=` of `==` / `===`
    }
    let before = |n: usize| at.checked_sub(n).and_then(|i| code.get(i)).copied();
    match before(1) {
        Some(b'!') | Some(b'=') => true,
        Some(b'<') => before(2) != Some(b'<'),
        Some(b'>') => before(2).is_some_and(|c| c.is_ascii_whitespace()),
        _ => false,
    }
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
    // detectors (`touring-foundation::text_sanitize::SECRET_TOKEN_PATTERNS` — moved
    // there from `touring-ceg::gateway::sandbox_executor` on 2026-10-17 and carrying
    // the `touring-quality:allow-secrets` pragma — etc.). Real user code that PREFIXES a value with `ghp_…` / `sk_live_…` etc.
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

    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        if is_detector_own_source(target) {
            return Ok((
                1.0,
                "Hardcoded Secrets: detector own source — markers are definitions, allowlisted (score=1.000)"
                    .to_string(),
            ));
        }
        let raw = crate::verifications::read_security_source(target)?;

        // File-level opt-out for legitimate fixtures that embed SAMPLE secrets to
        // exercise the redactor (detect-secrets / gitleaks `pragma: allowlist
        // secret` convention). Auditable and grep-able — unlike the removed
        // blanket `/tests/` allowlist. A real secret in a test file WITHOUT this
        // marker is now correctly flagged.
        //
        // FILE targets only (cross-audit 14/09/2026): a directory target is read
        // as the concatenation of its files, so one fixture carrying the pragma
        // allowlisted every other file of the directory, real secrets included.
        if target.is_file() && raw.contains(ALLOW_SECRETS_PRAGMA) {
            return Ok((
                1.0,
                "Hardcoded Secrets: file carries `touring-quality:allow-secrets` \
                 pragma (sample-secret fixture, explicitly allowlisted) — score=1.000"
                    .to_string(),
            ));
        }

        // Line-level `# pragma: allowlist secret` suppression (gitleaks /
        // detect-secrets convention). Each annotated line is stripped before
        // the scan so a legitimate fixture can opt a single line out without
        // using the whole-file `ALLOW_SECRETS_PRAGMA` marker.
        const LINE_PRAGMA: &str = "pragma: allowlist secret";
        let filtered_raw;
        let scan_src: &str = if raw.contains(LINE_PRAGMA) {
            filtered_raw = raw
                .lines()
                .filter(|l| !l.contains(LINE_PRAGMA))
                .map(|l| format!("{l}\n"))
                .collect::<String>();
            &filtered_raw
        } else {
            &raw
        };

        let (strong, weak) = scan(scan_src);
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

        // Point at the finding as `arquivo:linha:regra` (25/09/2026,
        // touring-36): this gate scores the WHOLE file, so a hit blocks every
        // subsequent edit to it — without the line AND the rule the author
        // has to bisect by hand to learn what tripped it (three times over
        // on 2026-08-02, all false positives; again on 2026-09-25 with the
        // route-path class, where the line alone did not say WHICH family
        // fired).
        //
        // The location and the rule NAME only: quoting the text — or any
        // slice of it — would print the very credential this dim exists to
        // protect, into terminal scrollback and CI logs. The guard at the
        // bottom of this module plants a synthetic secret and asserts no
        // output format ever echoes it.
        let evidence = match first_offending(&raw) {
            Some((line, rule)) if strong => format!(
                "Hardcoded Secrets: {category} at {}:{line}:{rule} — score={value:.3}",
                target.display()
            ),
            _ => format!("Hardcoded Secrets: {category} — score={value:.3}"),
        };
        Ok((value, evidence))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // `check` (the trait's default) is what the tests exercise, so they still
    // need the wrapper type the verifier itself no longer names.
    use crate::DimScore;
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
    fn test_readable_placeholder_in_secret_named_arg_does_not_block() {
        // Regression 2026-08-02: a test fixture's `token="my-secret-token"`
        // scored 0.000 (P0 BLOCK) and blocked every edit to the file. Short,
        // digit-free, letters-and-separators text is a placeholder, not a
        // credential.
        //
        // The bar here is "does not BLOCK", not "scores 1.0": the keyword is
        // still a real secret-named identifier, so the module's documented
        // weak-scan Warn (0.5) is the CORRECT outcome — surfaced, not blocking.
        // P0 fails below 0.5, so 0.5 clears the gate.
        for line in [
            "client.connect(\"https://x.example.com\", token=\"my-secret-token\")\n",
            "password = \"changeme\"\n",
            "api_key = \"your_api_key_here\"\n",
            // Prose-shaped auth value: the space must not disqualify it.
            "client.connect(url, token=\"Bearer test-token\")\n",
        ] {
            let v = score(line).value;
            assert!(v >= 0.5, "must not P0-BLOCK (got {v}) for: {line}");
        }
    }

    #[test]
    fn test_real_credentials_in_secret_named_arg_still_block() {
        // The twin of the exemption above — each of these must STILL block:
        // a digit disqualifies, a symbol outside `-_.` disqualifies, and 16+
        // characters disqualifies (a long passphrase is a real secret).
        assert_eq!(
            score("password = \"P@ssw0rd123\"\n").value,
            0.0,
            "mixed classes"
        );
        assert_eq!(score("token = \"abc123def456\"\n").value, 0.0, "has digits");
        assert_eq!(
            score("secret = \"correcthorsebatterystaple\"\n").value,
            0.0,
            "26 chars — past the 16-char bar"
        );
    }

    #[test]
    fn test_github_pat_blocks() {
        let s = score("let t = \"ghp_aBcDeF0123456789aBcDeF0123456789aBcD\";\n");
        assert_eq!(s.value, 0.0, "ghp_ token must block");
    }

    /// Cross-audit 03/09/2026 — um marcador de provedor CITADO em prosa nao e' um
    /// segredo. `has_strong_marker` era `text.contains(m)` puro, entao qualquer
    /// documentacao que escrevesse o prefixo virava bloqueador P0: foi assim que o
    /// comentario que EXPLICA esta correcao zerou o arquivo hospedeiro (o unico
    /// 0.000 em 1.763 arquivos do censo). Num repositorio que documenta os proprios
    /// padroes de seguranca, um detector que nao distingue citacao de credencial
    /// custa mais do que rende. O corpo do token e' o que separa os dois.
    /// Cross-audit 03/09/2026 — irmao do teste de prefixo, outra regra. A regra de
    /// connection-string tambem le a linha CRUA (e deve: uma DSN real colada num
    /// comentario e' vazamento). O que faltava era distinguir credencial de
    /// placeholder — disciplina que o arquivo ja tinha em `is_readable_placeholder`
    /// e que esta regra nao consultava. Sintoma: `postgres.rs` zerou por um exemplo
    /// de doc `postgres://user:pass@localhost/touring`.
    #[test]
    fn dsn_de_exemplo_com_credencial_obvia_nao_e_segredo() {
        for doc in [
            "/// let b = PostgresBackend::new(\"postgres://user:pass@localhost/touring\").await?;",
            "// mysql://root:changeme@127.0.0.1/dev",
        ] {
            assert_eq!(
                score(&format!("{doc}\n")).value,
                1.0,
                "exemplo nao bloqueia: {doc}"
            );
        }
    }

    /// O contrapositivo: credencial que mistura classes segue bloqueando.
    #[test]
    fn dsn_com_credencial_real_segue_bloqueando() {
        let s = score("let dsn = \"postgres://admin:Tr0ub4dor&3@db.interno:5432/prod\";\n");
        assert_eq!(s.value, 0.0, "senha com entropia bloqueia");
    }

    #[test]
    fn prefixo_citado_em_prosa_nao_e_segredo() {
        for prosa in [
            "// um literal com prefixo de token (`ghp_...`) faz do arquivo um achado",
            "/// AWS usa o prefixo AKIA para access key id",
            "// o padrao Slack comeca em xoxb- e o GitLab em glpat-",
        ] {
            let s = score(&format!("{prosa}\n"));
            assert_eq!(s.value, 1.0, "citacao nao bloqueia: {prosa}");
        }
    }

    /// O contrapositivo — sem ele o teste acima seria licenca para vazar.
    #[test]
    fn marcador_com_corpo_de_token_segue_bloqueando_ate_em_comentario() {
        // A key SHAPED like a real one. It used to be the AWS documentation's sample
        // (`…EXAMPLE`), which the detector now allowlists on purpose (17/10/2026,
        // `the_aws_documentation_sample_pair_is_not_a_leak`); built at run time.
        let aws = ["AKIA", "Q7ZP3XVB9TQLM2RW"].concat();
        for vazamento in [
            "// deixei aqui: ghp_aBcDeF0123456789aBcDeF0123456789aBcD".to_string(),
            format!("let k = \"{aws}\";"),
            // Split so the source never holds a scanner-shaped Slack token (GitHub push
            // protection blocked this fixture, 18/09/2026); the detector sees it joined.
            concat!(
                "# slack: xox",
                "b-2456789012-3456789012-aBcDeF0123456789aBcD"
            )
            .to_string(),
        ] {
            let s = score(&format!("{vazamento}\n"));
            assert_eq!(s.value, 0.0, "token com corpo bloqueia: {vazamento}");
        }
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
        // regression: AWS detection must still work -- for a real-shaped key, not the
        // documentation's sample (allowlisted, see the 17/10/2026 tests below).
        let key = ["AKIA", "Q7ZP3XVB9TQLM2RW"].concat();
        let s = score(&format!("pub const AWS: &str = \"{key}\";\n"));
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
    fn test_keyword_as_an_earlier_value_is_not_the_assigned_name() {
        // scripts/test_measure_tool_trio.py:139 (29/10/2026): `pwd` is the shell
        // command, the assignment binds `timestamp`.
        let fp = "    dict(_use(2, \"Bash\", {\"command\": \"pwd\"}), timestamp=\"2026-11-01T23:59:59Z\"),\n";
        assert_eq!(scan_text(fp).rule, None, "{fp}");
        // The name right before the delimiter still blocks, in every binding form.
        // T-3 (31/10/2026): o gap que este teste REGISTRAVA — o nome do
        // segredo no lado DIREITO de outra atribuição (`cfg = connect(password=
        // "…")`, `creds = {"api_key": "…"}`) — fecha com a leitura aninhada, e
        // sem reabrir o bypass de destructuring (29/10): o nome ligado é lido
        // pela MESMA projeção sem strings, e o valor pela linha crua.
        for real in [
            "connect(host, password=\"hunter2value\")\n",
            "cfg = connect(password=\"hunter2value\")\n",
            "creds = {\"api_key\": \"hunter2value\"}\n",
            // T-3: um `#` DENTRO de uma string não abre comentário — o resto
            // da linha continua sendo código (e o segredo continua
            // bloqueando).
            "url = \"a#b\"; password = \"hunter2value\"\n",
            "{\"api_key\": \"hunter2value\"}\n",
            "data[\"token\"] = \"hunter2value\"\n",
            "let token: String = \"hunter2value\".into();\n",
            "pwd = \"hunter2value\"\n",
            // Destructuring (29/10/2026, found by touring-c1): the Rust tuple form
            // passed with a Warn, the Python one broke in 20d2b05a.
            "let (password, other) = (\"hunter2value\", \"x\");\n",
            "let (other, password) = (\"x\", \"hunter2value\");\n",
            "password, other = \"hunter2value\", \"x\"\n",
            "const [token, rest] = [\"hunter2value\", \"x\"];\n",
        ] {
            assert_eq!(
                scan_text(real).rule,
                Some("secret-named-assignment"),
                "{real}"
            );
        }
        // A tuple/list RHS is read as values only behind a destructuring LHS.
        for clean in [
            "scopes = [\"read\", \"write\"]\n",
            "let (a, b) = (\"hunter2value\", \"x\");\n",
            // T-3: o nome aninhado SEM valor forte não bloqueia — o mesmo
            // padrão do split de topo (o valor é o que fecha a regra).
            "cfg = connect(password=\"your_password_here\")\n",
            "cfg = connect(timeout=\"30\")\n",
            // `==` é comparação, não ligação: o nome do lado esquerdo de uma
            // comparação não liga nada (o `"token"` é uma chave lida).
            "assert v[\"token\"] == \"adj_minimax.jsonl\"\n",
        ] {
            assert_eq!(scan_text(clean).rule, None, "{clean}");
        }
    }

    /// T-3 (31/10/2026) — um COMENTÁRIO é documento, não código.
    ///
    /// A projeção blankava `//` e `/* */` mas não `#`: um comentário Python que
    /// DOCUMENTA o padrão (`# "token": "…"`) tinha o `:` virado delimitador de
    /// código, e a regra de atribuição disparava dentro do comentário (medido:
    /// o censo do workspace foi de 0,985/0 bloqueios para 0,000/1 blocker).
    ///
    /// Um literal colado num comentário continua sendo vazamento — a varredura
    /// de literal lê a linha CRUA, e o que muda é só a POSIÇÃO dos delimitadores.
    #[test]
    fn a_hash_comment_nao_e_codigo() {
        for clean in [
            "# {\"api_key\": \"hunter2value\"}\n",
            "# token = \"hunter2value\"\n",
            "x = 1  # password = \"hunter2value\"\n",
        ] {
            let s = score(clean);
            assert!(
                s.value > 0.0,
                "um comentário não é código — não pode bloquear: {clean} ({s:?})"
            );
        }
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
        let s = score("    (\"F2.4\", \"secrets\", \"Hardcoded Secrets\"),\n");
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

    #[test]
    fn test_relative_repo_path_no_false_positive() {
        // 2026-08-03 (second hit, same registry edit): a doc path with an anchor
        // is a resource location — high digit density must not read as entropy.
        let s = score(
            "let r = \"docs/legal-normative-infrastructure/explanation/15-DIAGNOSTICO-INTEGRACAO-PROCESS-ANALYSIS.md#7.2\";\n",
        );
        assert_eq!(s.value, 1.0, "relative repo paths must not be entropy FPs");
    }

    #[test]
    fn test_memory_ref_no_false_positive() {
        // 2026-08-03: a long kebab-case memory key blocked a registry edit — a
        // `mem:`/`urn:` reference is a pointer to a resource, never a credential.
        let s = score(
            "let r = \"mem:lexhub:sei-50500-115208-2024-45-inventario-completo-relatorio-diretoria-2026-07-25\";\n",
        );
        assert_eq!(
            s.value, 1.0,
            "memory refs must not be entropy false positives"
        );
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

    /// A Subresource Integrity digest (`sha512-<base64>`) is the hash of a PUBLIC
    /// file, published so the browser can check it — never a credential. Every
    /// `package-lock.json` `"integrity"` line and two SRI attributes in the analise
    /// failed F2.4 at 0.0 (27/09/2026). The fixtures are sha digests of fixed strings.
    #[test]
    fn subresource_integrity_digests_are_not_secrets() {
        let d512 = "whRSHehmd1RgwAg9T8r+qPGyyLUd5jltqBcUqh6MOY6p1BFc7rIf5Xs125V1Nd3TU8oG6Up5YEu01cEyWPYKKw==";
        let d384 = "oA82yqy4rUSVNPL38+ZqPgYvvmkFVLam3lXhIHryXCqfPCJNlz8zXmsluYIZusSW";
        // Control: the bare digest IS high-entropy — without the algorithm prefix the
        // detector must still see a token, so the exemption below is not vacuous.
        assert!(looks_like_secret_value(d512));

        let sri512 = format!("sha512-{d512}");
        let sri384 = format!("sha384-{d384}");
        assert!(!looks_like_secret_value(&sri512));
        assert!(!looks_like_secret_value_named(&sri384));

        let lock =
            format!("{{\n  \"node_modules/x\": {{\n    \"integrity\": \"{sri512}\",\n  }}\n}}\n");
        assert!(
            score(&lock).value > 0.0,
            "an npm lockfile integrity line must not block: {}",
            score(&lock).evidence
        );
        let html =
            format!("<script src=\"https://cdn.example/x.js\" integrity=\"{sri384}\"></script>\n");
        assert!(
            score(&html).value > 0.0,
            "an SRI attribute must not block: {}",
            score(&html).evidence
        );
    }

    /// A nested scoped npm package path is a resource location: the `@` that opens a
    /// scope segment kept it out of `is_relative_dir_path`, and an 81-char lockfile key
    /// at entropy 4.53 blocked F2.4 (27/09/2026, kazuba-browser/package-lock.json).
    #[test]
    fn npm_scoped_package_paths_are_not_secrets() {
        let key = "node_modules/@hookz/qs/node_modules/@wry-context/equality-7b2kq-9xjzv4f";
        assert!(
            shannon_entropy(key) >= 4.5,
            "premise: the key must reach the entropy path"
        );
        assert!(!looks_like_secret_value(key));
        let lock = format!(
            "{{\n  \"packages\": {{\n    \"{key}\": {{\n      \"dev\": true\n    }}\n  }}\n}}\n"
        );
        assert!(
            score(&lock).value > 0.0,
            "a scoped npm package path must not block: {}",
            score(&lock).evidence
        );
        // Control: an `@` in the MIDDLE of a segment opens no scope.
        assert!(!is_relative_dir_path("aB3x/Kq9@mZ7w-r2/yT5u"));
    }

    /// A dependency whose NAME mentions a token (`registry-auth-token`, a real npm
    /// package) has a VERSION RANGE as its value, never a credential — it held the
    /// kazuba-browser lockfile at F2.4 0.0 as a secret-named assignment (27/09/2026).
    #[test]
    fn dependency_version_ranges_are_not_secrets() {
        let lock = "{\n  \"dependencies\": {\n    \"registry-auth-token\": \"^5.0.1\",\n    \"@octokit/auth-token\": \"~4.0.0\"\n  }\n}\n";
        assert!(
            score(lock).value > 0.0,
            "a version range under a token-named dependency must not block: {}",
            score(lock).evidence
        );
        // Control: the same kind of name with a credential-shaped value still blocks.
        let leak = "{\n  \"auth_token\": \"aB3xK9mQ7zR2wY5tE8uI1oP4\"\n}\n";
        assert_eq!(
            score(leak).value,
            0.0,
            "premise: a real-looking token must block"
        );

        for v in ["^5.0.1", "~4.0", ">=2.0.0-rc.1", "1.x", "v3.2.1", "^5"] {
            assert!(is_version_spec(v), "{v} is a version spec");
        }
        // The bounds: a bare number, an over-long part and a token-sized tail are not versions.
        for v in [
            "8392017483",
            "1.123456789",
            "1-aB3xK9mQ7zR2wY5tE8uI1oP4sD6fG",
            "",
            "x.1",
        ] {
            assert!(!is_version_spec(v), "{v:?} must not pass as a version");
        }
    }

    #[test]
    fn evidence_points_at_the_offending_line_without_leaking_it() {
        let src = "fn main() {\n    let cfg = load();\n    let key = \"ghp_aBcDeF0123456789aBcDeF0123456789aBcD\";\n}\n";
        assert_eq!(first_offending(src), Some((3, "provider-marker")));

        let s = score(src);
        assert_eq!(s.value, 0.0, "premise: this must block");
        assert!(
            s.evidence.contains(":3:provider-marker"),
            "evidence must locate the hit as arquivo:linha:regra: {}",
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

    /// 26/09/2026: a DSN *validator* ships the regex of the credential slot — the
    /// userinfo is a character class (`[^:]+:[^@]+`), and the «password» the rule
    /// extracted was `]+:[^@]+`. It zeroed F2.4 of `analise/.claude/hooks` on
    /// `secrets_detector.py`'s own connection-string pattern. A class DESCRIBES a
    /// credential; it is not one.
    #[test]
    fn a_connection_string_regex_is_a_pattern_not_a_credential() {
        assert!(!has_connstring_creds(
            r#"    "dsn": r"(?:postgresql|mysql|mongodb)://[^:]+:[^@]+@[^/]+","#
        ));
        assert!(!has_connstring_creds(
            r"DSN_RE = re.compile(r'redis://[a-z]*:[A-Za-z0-9]+@[\w.]+')"
        ));
        // controls: a real mixed-class password still blocks; a doc stand-in passes
        assert!(has_connstring_creds(
            "postgres://admin:s3cr3tP4ssw0rd@db.example.com:5432/app"
        ));
        assert!(!has_connstring_creds("postgres://user:pass@localhost/db"));
    }

    // ── S2 (2026-09-01): text-level API for the pre-write signal layer ──

    #[test]
    fn scan_text_flags_connection_string_credentials_with_the_line() {
        let raw = "fn main() {\n    let url = \"postgres://admin:s3cr3tP4ssw0rd@db.example.com:5432/app\";\n}\n";
        let s = scan_text(raw);
        assert!(s.strong, "{s:?}");
        assert!(!s.allowlisted);
        assert_eq!(s.first_line, Some(2), "{s:?}");
    }

    #[test]
    fn a_go_struct_tag_names_a_field_it_does_not_assign_a_secret() {
        for clean in [
            // packages/antt-process-analysis/sdks/go/configuration.go:54
            "\tPassword string `json:\"password,omitempty\"`",
            "\tAPIKey   *string `json:\"api_key\" yaml:\"api_key\"` // the key",
            "\tToken []byte `db:\"token\"`",
        ] {
            assert_eq!(rule_of(clean), None, "{clean}");
        }
        for real in [
            // an assignment in Go still counts
            "\tpassword := \"Tr0ub4dor&3xK9\"",
            "\tcfg.Password = \"Tr0ub4dor&3xK9\"",
            // a token inside the tag is still a token
            "\tToken string `default:\"ghp_0123456789abcdef0123456789abcdef0123\"`",
        ] {
            assert!(rule_of(real).is_some(), "{real}");
        }
    }

    #[test]
    fn scan_text_is_clean_for_ordinary_code() {
        let s = scan_text("pub fn add(a: i32, b: i32) -> i32 { a + b }\n");
        assert_eq!(
            s,
            SecretScan {
                strong: false,
                weak: false,
                first_line: None,
                rule: None,
                allowlisted: false
            }
        );
    }

    #[test]
    fn scan_text_honours_the_allow_secrets_pragma() {
        let raw = "// touring-quality:allow-secrets\nlet url = \"postgres://admin:s3cr3tP4ssw0rd@db.example.com/app\";\n";
        let s = scan_text(raw);
        assert!(s.allowlisted && !s.strong, "{s:?}");
    }

    // ── 2026-09-04: the delimiter that decides "this line is an assignment" must
    //    sit at CODE level. Until today the split ran on the RAW line, so an `=`
    //    or `:` living INSIDE a string literal split the line in two, and any
    //    line that (a) names a secret keyword anywhere to its left and (b) quotes
    //    a delimiter hard-blocked every edit to its file. Census over 592 files
    //    (skills + docs + hook crates): 2 of the 4 live blockers were this exact
    //    shape — a loop variable called `token` on a line calling `.replace("=",
    //    " ")`. The file already knew what a string literal is; the strong path
    //    just never asked it.

    #[test]
    fn a_delimiter_inside_a_string_literal_is_not_an_assignment() {
        // Verbatim from ~/.claude/skills/Touring/scripts/pre_edit_gate.py:127,
        // which scored 0.000 and blocked every edit to that file.
        let raw =
            "        for token in line.replace(\":\", \" \").replace(\"=\", \" \").split():\n";
        let s = scan_text(raw);
        assert!(
            !s.strong,
            "a loop variable plus quoted delimiters is not a secret: {s:?}"
        );
    }

    #[test]
    fn a_json_style_colon_assignment_still_blocks() {
        // This colon IS at code level (it separates two literals). The fix must
        // not buy the false positive back by disabling the colon path.
        let raw = "{\"api_key\": \"aB3xQ9zL7pW2mK5t\"}\n";
        assert!(
            scan_text(raw).strong,
            "a quoted secret-named assignment must still block"
        );
    }

    #[test]
    fn an_env_style_unquoted_assignment_still_blocks() {
        let raw = "API_KEY=aB3xQ9zL7pW2mK5tR8vN1c\n";
        assert!(
            scan_text(raw).strong,
            "an unquoted .env assignment must still block"
        );
    }

    #[test]
    fn a_typed_declaration_still_prefers_the_equals_split() {
        let raw = "    let token: String = \"aB3xQ9zL7pW2mK5t\".into();\n";
        assert!(
            scan_text(raw).strong,
            "`let token: T = \"...\"` must still block on the `=`, not the `:`"
        );
    }

    #[test]
    fn the_code_projection_addresses_the_same_bytes_as_the_raw_source() {
        // The delimiter fix rests entirely on this invariant: a byte index found
        // in the projection must address the SAME byte in the raw line. Asserted
        // positively over every literal shape the blanker special-cases, so a
        // future change to the blanker cannot silently misalign the strong path.
        for raw in [
            "let a = \"x=y\";\n",
            "let b = r#\"a:b\"#;\n",
            "// comment with = and :\nlet c = 1;\n",
            "/* block = : */ let d: u8 = 2;\n",
            "let e = \"multi\nline = value\";\n",
            "let f = \"acentuação = ç\";\n",
            "{\"api_key\": \"v\"}\n",
        ] {
            let code = strip_strings_and_comments(raw);
            assert_eq!(
                code.len(),
                raw.len(),
                "projection must preserve byte length for {raw:?}"
            );
            assert_eq!(
                code.lines().count(),
                raw.lines().count(),
                "projection must preserve line count for {raw:?}"
            );
            for (r, c) in raw.lines().zip(code.lines()) {
                assert_eq!(r.len(), c.len(), "line lengths must match: {r:?} vs {c:?}");
            }
        }
    }

    /// Cross-audit 14/09/2026: a directory target is scored as the concatenation
    /// of its files, and the pragma used to be honoured on that concatenation —
    /// one sample-secret fixture allowlisted every other file of the directory.
    #[test]
    fn the_pragma_of_one_fixture_never_allowlists_its_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("fixture.rs"),
            "// touring-quality:allow-secrets\nconst SAMPLE: &str = \"ghp_0123456789abcdef0123456789abcdef0123\";\n",
        )
        .expect("fixture");
        std::fs::write(
            dir.path().join("config.rs"),
            "pub const TOKEN: &str = \"ghp_fedcba9876543210fedcba9876543210fedc\";\n",
        )
        .expect("real secret");
        let (dir_value, evidence) = F2_4_Secrets.measure(dir.path()).expect("measure dir");
        assert!(
            dir_value < 0.5,
            "the real secret beside the fixture is found: {evidence}"
        );
        let (fixture_value, _) = F2_4_Secrets
            .measure(&dir.path().join("fixture.rs"))
            .expect("measure fixture");
        assert_eq!(fixture_value, 1.0, "the fixture itself stays allowlisted");
    }

    /// Canvas D (15/09/2026): two shell scripts in the public touring repository
    /// failed F2.4 with no secret in them. Each exemption sits beside the
    /// credential it must keep catching.
    #[test]
    fn a_shell_expansion_and_a_directory_path_are_not_secrets_but_literals_still_are() {
        for runtime in [
            "TOKEN=\"$(cat \"${TOKEN_FILE}\")\"\n",
            "API_KEY=\"${DEPLOY_API_KEY}\"\n",
            "SECRET=\"$VAULT_SECRET\"\n",
            "TOKEN=\"`pass show ci/token`\"\n",
        ] {
            assert!(!scan_text(runtime).strong, "{runtime}");
        }
        for literal in [
            "TOKEN=\"ghp_aBcDeF0123456789aBcDeF0123456789aBcD\"\n",
            "PASSWORD=\"$2b$12$R9h/cIPz0gi.URNNX3kh2OPST9/PgBkqquzi.Ss7KIUgO2t0jWMUW\"\n",
            "SECRET=\"$vault1\"\n",
            "SECRET=\"Tr0ub4dor&3xK9\"\n",
        ] {
            assert!(scan_text(literal).strong, "{literal}");
        }
        // The real value from `validate_f2_e2e.sh`: past the entropy floor, so the
        // directory-path exemption is what keeps it out.
        let dir = "docs/plans/2026-08-11-memory-hashtag-library/fixture";
        assert!(shannon_entropy(dir) >= 4.5);
        assert!(!scan_text(&format!("D=\"{dir}\"\n")).strong);
        let token = "BLOB=\"aB3xYz9QwErT5uIoP2aSdF6gHjK8lZ/xC4vBn7mQrT1/eW9sZ2kL\"\n";
        assert!(
            scan_text(token).strong,
            "base64 with slashes is still a token"
        );
        // Lowercase and slashed, but no hyphenated word: an opaque token.
        let base36 = "k3j9x2m8q7w1z5v6/b4n0p3r8t2y7u1i9/o4e6a8s0d2f4g6h8";
        assert!(shannon_entropy(base36) >= 4.5);
        assert!(scan_text(&format!("BLOB=\"{base36}\"\n")).strong);
    }

    /// touring-36 (25/09/2026, the analise's backend judge, P0 0.000): a route
    /// path or an identifier RHS is not a credential — only a hardcoded VALUE
    /// is. The decorator's first `=` is the `status_code=` kwarg; "password"
    /// in the route named the LHS; the enum path scored as an assigned
    /// high-entropy value. Negatives on one side, the controls that must
    /// keep failing on the other.
    #[test]
    fn route_paths_and_identifier_rhs_are_not_secrets_but_hardcoded_values_still_are() {
        for negative in [
            // The measured FP (the judge's auth.py:203).
            r#"@router.post("/forgot-password", status_code=status.HTTP_202_ACCEPTED)"#,
            r#"@router.post("/reset-password", status_code=status.HTTP_200_OK)"#,
            "password_reset_token_expire_minutes = 30",
            // An env read is code, never a hardcoded value.
            r#"token = os.environ.get("API_TOKEN")"#,
            r#"let url = std::env::var("DATABASE_URL").unwrap_or_default();"#,
        ] {
            assert!(
                !scan_text(&format!("{negative}\n")).strong,
                "not a secret: {negative}"
            );
        }
        for positive in [
            // A high-entropy literal ASSIGNED to a secret name keeps blocking…
            r#"password = "aB3xK9mQ7zR2wY5tE8uI1oP4""#,
            r#"secret = "aB3xK9mQ7zR2wY5tE8uI1oP4""#,
            // …quoted or unquoted…
            "password = aB3xK9mQ7zR2wY5tE8uI1oP4xC2vB",
            // …a known-prefix key…
            r#"let key = "ghp_0123456789abcdef0123456789abcdef0123";"#,
            r#"client := "sk-ant-api03-aB3xK9mQ7zR2wY5tE8uI1oP4aSdF6gHjK8lZ""#,
            // …and a weak literal password (the analise's test fixtures —
            // detection by design; the pragma is the documented escape).
            r#"password="password123""#,
            // A dotted token (a JWT) unquoted: the named path now skips it as
            // code-shaped — and the STRUCTURAL path still catches it, which is
            // exactly why the exclusion opens no hole.
            "TOKEN=eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJVadQssw5c",
        ] {
            assert!(
                scan_text(&format!("{positive}\n")).strong,
                "still a secret: {positive}"
            );
        }
    }

    /// touring-36 (25/09/2026, the analise-50 judge, scripts/lexhub — files of
    /// 05/09 nobody touched): a literal shaped as a PATH or a FILENAME (a
    /// path separator and/or a known document/code extension, no known key
    /// prefix) is a resource location, not a secret. The entropy came from
    /// the LONG NAME, which is how files are named.
    #[test]
    fn paths_and_filenames_are_not_secrets_but_query_tokens_and_keys_still_are() {
        for negative in [
            // gold_corpus.py:59 — a long data path inside Path(…); the
            // process-number tail mixes cases and dots.
            "MAE_DEFAULT = Path(\n    \"data/processed/process_inventory/malha-paulista-revisao-extraordinaria/converted/SEI_50500.023913_2026_89\"\n)",
            // test_verificar_pleito_mp.py:117 — a document name, no `/`.
            r#"assert "[14]-42780739_Minuta_de_Deliberacao.md" in docs_citados"#,
        ] {
            assert!(
                !scan_text(&format!("{negative}\n")).strong,
                "not a secret: {negative}"
            );
        }
        for positive in [
            // a JWT…
            "TOKEN=eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJVadQssw5c",
            // a known-prefix key…
            r#"let key = "ghp_0123456789abcdef0123456789abcdef0123";"#,
            r#"k = "sk-ant-api03-aB3xK9mQ7zR2wY5tE8uI1oP4aSdF6gHjK8lZ""#,
            // a high-entropy literal with NO `/` and NO extension, assigned to
            // `secret`…
            r#"secret = "aB3xK9mQ7zR2wY5tE8uI1oP4xC2vB""#,
            // …and a PATH carrying a token in the query — a credential
            // embedded in a URL, never a filesystem path (the `?` disqualifies
            // the path shape; this is the control that keeps the exclusion
            // honest).
            r#"url = "/api?key=aB3xK9mQ7zR2wY5tE8uI1oP4xC2vB""#,
        ] {
            assert!(
                scan_text(&format!("{positive}\n")).strong,
                "still a secret: {positive}"
            );
        }
    }

    /// 2026-09-26, analise `.claude/hooks`: two generic-entropy false positives held
    /// the P0 gate at 0.0 — a CSS declaration (the literal between the INNER quotes of
    /// an HTML f-string, so the `<>` markers stayed outside it) and a JS call in a
    /// completion suggestion. Opaque tokens (base64, base64url, hex, provider keys)
    /// never carry `(`, `)` or `;`: code and style syntax, the same category as `<>{}\`.
    #[test]
    fn css_declaration_and_code_call_are_not_opaque_tokens() {
        for benign in [
            r#"f'<div style="background:#eee;width:200px;height:12px;display:inline-block">'"#,
            r#""expect(diagnostico.tarifa_basica2025).toBeCloseTo(0.12345,4);","#,
            // the same call without the `;`: the parentheses alone must spare it
            r#""expect(diagnostico.tarifa_basica2025).toBeCloseTo(0.12345,4)","#,
        ] {
            assert!(
                !scan_text(&format!("{benign}\n")).strong,
                "not a secret: {benign}"
            );
        }
    }

    /// Control: the new markers exempt code and style syntax, not entropy — an opaque
    /// token on the same line as that syntax (in its own literal) still blocks.
    #[test]
    fn opaque_token_beside_code_syntax_still_blocks() {
        assert!(scan_text("call(x); k = \"q7Zp3xVb9TqLm2Rw8sYd4NcF6hJk1GtA\"\n").strong);
    }

    // ── 17/10/2026: documented vendor placeholders + the label ────────────────────
    //
    // Every literal below is assembled at run time: this file is itself scanned by
    // the pre-write hook, which would (rightly) block a whole sample key.

    fn aws_sample_pair() -> (String, String) {
        (
            ["AKIA", "IOSFODNN7", "EXAMPLE"].concat(),
            ["wJalrXUtnFEMI/K7MDENG/", "bPxRfiCYEXAMPLEKEY"].concat(),
        )
    }

    /// The documentation's pair in the layouts it is actually committed in.
    fn aws_sample_layouts() -> Vec<String> {
        let (id, secret) = aws_sample_pair();
        vec![
            format!("[default]\naws_access_key_id = {id}\naws_secret_access_key = {secret}\n"),
            format!("AWS_ACCESS_KEY_ID=\"{id}\"\nAWS_SECRET_ACCESS_KEY=\"{secret}\"\n"),
            format!(
                "c = boto3.client('s3', aws_access_key_id='{id}', aws_secret_access_key='{secret}')\n"
            ),
            format!("export AWS_ACCESS_KEY_ID={id}\nexport AWS_SECRET_ACCESS_KEY={secret}\n"),
            format!("# the docs show {id} / {secret}\nfn main() {{}}\n"),
        ]
    }

    #[test]
    fn the_aws_documentation_sample_pair_is_not_a_leak() {
        for src in aws_sample_layouts() {
            assert_eq!(
                score(&src).value,
                1.0,
                "the gate blocks the sample: {src:?}"
            );
            let text = scan_text(&src);
            assert!(
                !text.strong,
                "the pre-write scan blocks the sample: {src:?} {text:?}"
            );
        }
    }

    #[test]
    fn the_older_aws_sample_pair_is_not_a_leak_either() {
        let id = ["AKIA", "I44QH8DHB", "EXAMPLE"].concat();
        let secret = ["je7MtGbClwBF/2Zp9Utk/", "h3yCo8nvbEXAMPLEKEY"].concat();
        let src = format!("aws_access_key_id = {id}\naws_secret_access_key = \"{secret}\"\n");
        assert_eq!(score(&src).value, 1.0, "{src:?}");
    }

    #[test]
    fn a_real_shaped_aws_key_still_blocks() {
        let id = ["AKIA", "Q7ZP3XVB9TQLM2RW"].concat();
        let secret = ["Zx9Qw3Er7Ty1Ui5Op2As", "8Df4Gh6Jk0Lm3Nb7Vc5X"].concat();
        for src in [
            format!("aws_access_key_id = {id}\n"),
            format!("aws_secret_access_key = \"{secret}\"\n"),
        ] {
            assert_eq!(
                score(&src).value,
                0.0,
                "a real-shaped credential must block: {src:?}"
            );
        }
    }

    #[test]
    fn the_sample_extended_by_one_character_is_another_string() {
        let (id, _) = aws_sample_pair();
        for src in [
            format!("aws_access_key_id = {id}Q\n"),
            format!("aws_access_key_id = Q{id}\n"),
        ] {
            assert!(
                scan_text(&src).strong,
                "not the documented literal: {src:?}"
            );
        }
    }

    #[test]
    fn a_real_key_beside_the_sample_is_still_located_at_its_own_line() {
        let (id, _) = aws_sample_pair();
        let real = ["AKIA", "Q7ZP3XVB9TQLM2RW"].concat();
        let src = format!("aws_access_key_id = {id}\naws_access_key_id = {real}\n");
        let found = scan_text(&src);
        assert!(found.strong, "{found:?}");
        assert_eq!(
            found.first_line,
            Some(2),
            "the sample on line 1 is not the finding"
        );
    }

    #[test]
    fn masking_the_samples_keeps_the_byte_layout() {
        let (id, secret) = aws_sample_pair();
        let src = format!("a = \"{id}\"\nb = '{secret}'\n");
        let masked = mask_documented_placeholders(&src);
        assert_eq!(
            masked.len(),
            src.len(),
            "line numbers and projection alignment"
        );
        assert_eq!(masked.lines().count(), src.lines().count());
        assert!(!masked.contains(&id) && !masked.contains(&secret));
        assert!(
            matches!(
                mask_documented_placeholders("fn main() {}\n"),
                Cow::Borrowed(_)
            ),
            "text without a sample is not copied"
        );
    }

    #[test]
    fn evidence_names_the_dimension_by_what_it_measures() {
        let real = ["AKIA", "Q7ZP3XVB9TQLM2RW"].concat();
        let hit = score(&format!("aws_access_key_id = {real}\n"));
        assert!(
            hit.evidence.starts_with("Hardcoded Secrets:"),
            "{}",
            hit.evidence
        );
        assert!(!hit.evidence.contains("Cryptographic"), "{}", hit.evidence);
        let clean = score("pub fn add(a: i32, b: i32) -> i32 { a + b }\n");
        assert!(
            clean.evidence.starts_with("Hardcoded Secrets:"),
            "{}",
            clean.evidence
        );
    }

    // ── 02/11/2026: false positives measured on the analise repo (F2.4 sweep) ──────
    //
    // Each test pairs the measured case (the line as it stands in the analise,
    // `arquivo:linha:regra` in its comment) with the CONTROL that must keep
    // blocking: the narrowing never touches a real credential.

    fn rule_of(line: &str) -> Option<&'static str> {
        scan_text(&format!("{line}\n")).rule
    }

    /// Class 1 — a comparison is not an assignment. `split_at_code_delimiter`
    /// took the `=` of `!=` / `<=` / `>=` as the binding, so the secret NAME on the
    /// left of a comparison paired with the literal it is compared to.
    #[test]
    fn a_comparison_is_not_an_assignment_but_a_real_assignment_beside_it_still_blocks() {
        for clean in [
            // scripts/dspy/setup_kimi_dspy.py:77 [secret-named-assignment]
            r##"    if api_key != "[NÃO CONFIGURADA]":"##,
            // scripts/sei_premium/tests/test_theme.py:86 — a `token(` CALL, compared
            r##"    assert theme.token("c_primary_soft") != "#101010""##,
            r##"    if token <= "#101010" or secret >= "#202020":"##,
            // nested reading (the name sits on the RHS of another binding)
            r##"    ok = api_key != "[NÃO CONFIGURADA]""##,
        ] {
            assert_eq!(rule_of(clean), None, "comparison, not a binding: {clean}");
        }
        for real in [
            // the control of the brief: a real-shaped assignment AFTER the comparison
            r##"if api_key != "none": api_key = "Tr0ub4dor&3""##,
            r##"assert theme.token("x") != "#101010"; token = "Tr0ub4dor&3""##,
            // `:=`, `+=` and a generic closer stuck to `=` are bindings, not comparisons
            r##"password := "Tr0ub4dor&3""##,
            r##"token += "Tr0ub4dor&3""##,
            r##"let password: Vec<u8>= "Tr0ub4dor&3".into();"##,
        ] {
            assert_eq!(rule_of(real), Some("secret-named-assignment"), "{real}");
        }
    }

    /// Class 2 — the secret name is a PARAMETER of the signature, not the target of
    /// the default that follows. The whole-LHS reading exists for destructuring
    /// only (`let (password, x) = …`, `password, x = …`).
    #[test]
    fn another_parameters_default_is_not_the_secret_names_value_but_its_own_default_still_is() {
        // src/agents/core/llm_provider_example.py:200 [secret-named-assignment]
        let fp = r##"    def __init__(self, api_key: str, model_name: str = "gpt-4"):"##;
        assert_eq!(rule_of(fp), None, "{fp}");
        for real in [
            r##"    def __init__(self, api_key: str = "Tr0ub4dor&3", model_name: str = "gpt-4"):"##,
            r##"    def __init__(self, model_name: str = "gpt-4", api_key: str = "Tr0ub4dor&3"):"##,
            r##"    def __init__(self, model_name: str = "gpt-4", *, password="Tr0ub4dor&3"):"##,
            // destructuring keeps the whole-LHS reading
            r##"let (other, password) = ("x", "Tr0ub4dor&3");"##,
            r##"password, other = "Tr0ub4dor&3", "x""##,
        ] {
            assert_eq!(rule_of(real), Some("secret-named-assignment"), "{real}");
        }
    }

    /// Class 3a — a placeholder VOCABULARY word makes a readable phrase a stand-in
    /// even with a short number or a header prefix: `test-api-key-123`,
    /// `X-API-Key: your-api-key-here`. A token beside the word is not a phrase.
    #[test]
    fn a_numbered_or_headed_placeholder_phrase_is_a_stand_in_but_a_token_beside_the_word_still_blocks()
     {
        for clean in [
            // packages/kazuba-browser/tests/lib/api-client.test.ts:182
            r##"        apiKey: "test-api-key-123","##,
            // packages/antt-process-analysis/sdks/typescript/tests/analyze.test.ts:15
            r##"      apiKey: "X-API-Key: your-api-key-here","##,
            r##"    api_key = "YOUR_API_KEY_1""##,
        ] {
            assert_eq!(rule_of(clean), None, "placeholder phrase: {clean}");
        }
        for real in [
            // the vocabulary word does not exempt a token that sits beside it
            r##"        apiKey: "test-4eC39HqLyjWDarjtT1zdp7dc","##,
            r##"        apiKey: "your-key-Xk9mQ7zR2wY5","##,
            // a phrase WITHOUT the vocabulary is a passphrase — still a secret
            r##"    password = "correct-horse-battery-staple-123""##,
            // a long digit run is a numeric secret, not a short ordinal
            r##"        apiKey: "test-api-key-12345678","##,
            // a vocabulary word INSIDE a real word, or beside real words, launders nothing
            r##"    password = "protest-heaven-sample-castle-12""##,
            r##"    password = "test-orange-violin-harbor-7""##,
        ] {
            assert_eq!(rule_of(real), Some("secret-named-assignment"), "{real}");
        }
    }

    /// Class 3b — a LEXICAL token (`"token": "1009"`: a parser/NLP field carrying an
    /// offset) is a counter, never a bearer credential: a bare ≤4-digit number under
    /// the name `token` only. A PIN under `password` is a credential.
    #[test]
    fn a_lexical_token_ordinal_is_not_a_credential_but_a_numeric_password_still_is() {
        // scripts/process_analysis/tests/test_medir_lastro_estruturado.py:57
        let fp = r##"            "token": "1009","##;
        assert_eq!(rule_of(fp), None, "{fp}");
        for real in [
            r##"            "password": "1009","##,
            r##"            "secret": "1009","##,
            // five digits and a word are no longer an ordinal
            r##"            "token": "10091","##,
            r##"            "token": "aB3xK9mQ","##,
        ] {
            assert_eq!(rule_of(real), Some("secret-named-assignment"), "{real}");
        }
    }

    /// Class 3c — the bound value is the value OF THAT NAME. The literals of the
    /// sibling keys that follow (`"motivo": "unidade da federação"`) are other
    /// bindings, judged on their own by the nested reading.
    #[test]
    fn a_binding_does_not_inherit_the_literals_of_its_sibling_keys() {
        // docs/plans/2026-10-06-geopolitica-logistica-eua/diagnostics/test_t012_anexo_siglas.py:206
        let fp = r##"        "nao_siglas": [{"token": "SC", "motivo": "unidade da federação"}, {"token": "OK"}],"##;
        assert_eq!(rule_of(fp), None, "{fp}");
        for real in [
            // a later sibling that IS a secret name keeps blocking (nested reading)
            r##"        "x": [{"token": "SC", "password": "Tr0ub4dor&3"}],"##,
            r##"        cfg = {"note": "ok", "api_key": "Tr0ub4dor&3"}"##,
            r##"    connect(host="h", token="SC", password="Tr0ub4dor&3")"##,
            // without a comma the alternates belong to the same value
            r##"    token = "changeme" if dev else "Tr0ub4dor&3""##,
            r##"    password = "changeme" "Tr0ub4dor&3""##,
        ] {
            assert_eq!(rule_of(real), Some("secret-named-assignment"), "{real}");
        }
    }

    /// Class 4 — user-facing TEXT: `errors.password = "Senha é obrigatória"` is the
    /// validation message of the password field. The narrowest rule that does not
    /// free a passphrase: the TARGET is a message holder (`errors`, `messages`,
    /// `labels`, …) AND the value is prose (≥2 words, no credential symbols). A
    /// passphrase under a plain name keeps blocking.
    #[test]
    fn a_validation_message_for_the_password_field_is_not_a_credential_but_a_passphrase_still_is() {
        for clean in [
            // frontend/components/auth/LoginForm.tsx:30, SignupForm.tsx:69 and :71
            r##"      errors.password = "Senha é obrigatória";"##,
            r##"      errors.password = "Senha deve ter pelo menos 8 caracteres";"##,
            r##"      newErrors["password"] = "Senha é obrigatória";"##,
            r##"      messages.token = "Token expirado, entre novamente";"##,
        ] {
            assert_eq!(rule_of(clean), None, "UI message: {clean}");
        }
        for real in [
            // the case the rule must NOT free: a passphrase under a plain name
            r##"    password = "uma frase secreta longa""##,
            r##"    form.password = "uma frase secreta longa""##,
            // a message holder with a credential-shaped value is still a credential
            r##"      errors.password = "Tr0ub4dor&3xK9";"##,
            r##"      errors.password = "Tr0ub4dor &3 xK9";"##,
            r##"      errors.password = "ghp_aBcDeF0123456789aBcDeF0123456789aBcD";"##,
        ] {
            assert!(rule_of(real).is_some(), "still a secret: {real}");
        }
    }

    /// Class 5 — a literal that READS: words, numbers and short ids joined by
    /// `- _ . + / | & = ? :` (a SEI piece name, a NUP key, a query string, a
    /// checksum path, a kebab phrase). Shannon entropy over the concatenation
    /// measures the variety of the field names, not randomness. The test is on the
    /// SEGMENTS: one opaque segment (mixed case/digits, a base64 run) and it is a
    /// token again.
    #[test]
    fn a_literal_that_reads_as_words_and_numbers_is_not_opaque_but_a_token_segment_still_blocks() {
        for clean in [
            // scripts/external_apis/tests/test_dou_federal.py:889 — path + query
            r##"        "/629105016/S03032026.zip/uuid-S03032026?version=1.0&t=1&download=true" in u for u in http.baixou_mensal()"##,
            // packages/kazuba-lexhub/tests/test_unidades_antt_e_vocabulario_comum.py:68, :176
            r##"    "_tipo=POR&sgl_orgao=SUINF/ANTT/MI&vlr_ano=2019&seq_at""##,
            r##"    "tion.php?acao=abrirAtoPublico&num_ato=00000002&sgl_tipo=CMS&sgl_orgao=SUROD/ANTT/MT&vlr_ano=2025""##,
            // packages/kazuba-lexhub/tests/test_links_partes.py:28, :53 — SEI piece names
            r##"ANEXO_A = "[03]-26510579_Anexo.zip__Relatorio__PIA-8041_part_001""##,
            r##"        origem = "[07]-26510999_Outro.zip__Relatorio__PIA-8041""##,
            // packages/antt-process-analysis/tests/unit/test_template_instance.py:50 — NUP key
            r##"    assert inst.entity_id == "50500.023913/2026-89|template_instance|reequilibrio""##,
            // analise/relatoria/50500.373544_2023_20/analysis/analyze_kandango_data.py:20
            r##"MULTAS_XLS = RAW / "[023]-22029359_Historico_de_multas___KANDANGO" / "03.233.439.xlsx""##,
            // scripts/rail-ap-elite-50500-021563/W31/deep_economic_analysis.py:163
            r##"        "source": "Flyvbjerg_2014_megaprojects_database_+_BNDES_rail_concession_audit","##,
            // scripts/vlt_campina/generate_plan.py:1416, :2343, :3108, :3180, :4178
            r##"                    "analise/vlt_campina/manual/01_levantamento/MEMORIAL_8.1.1_AS_BUILT_R00.sha256", ".sha256", "output""##,
            r##"                _io("analise/vlt_campina/manual/02_urbanistico/COMPATIBILIZACAO_8.2_BCF.bcfzip", ".bcfzip", "output"),"##,
            r##"                _io("analise/vlt_campina/handoff/VLT_EstacaoVelha_MARCO_1_R00.zip.sha256", ".sha256", "output"),"##,
            r##"                _io("analise/vlt_campina/quality/PEER_REVIEW_CERTIFICATE_MARCO1.sha256", ".sha256", "output"),"##,
            // packages/kazuba-geo-engine/src/io/pmtiles.rs:254 — a kebab phrase
            r##"        let original = b"the-original-mvt-payload-0123456789".to_vec();"##,
        ] {
            assert_eq!(rule_of(clean), None, "reads as words and numbers: {clean}");
        }
        for real in [
            r##"let k = "q7Zp3xVb9TqLm2Rw8sYd4NcF6hJk1GtA";"##,
            r##"BLOB="aB3xYz9QwErT5uIoP2aSdF6gHjK8lZ/xC4vBn7mQrT1/eW9sZ2kL""##,
            r##"url = "/api?key=aB3xK9mQ7zR2wY5tE8uI1oP4xC2vB""##,
            // a query string whose VALUE is the token
            r##"u = "page=1&limit=50&sess=aB3xK9mQ7zR2wY5tE8uI1oP4xC2vB""##,
            // readable fields followed by an opaque segment
            r##"id = "[03]-26510579_aB3xK9mQ7zR2wY5tE8uI1oP4xC2vB""##,
            r##"id = "docs_v2_aB3xK9mQ7zR2wY5tE8uI1oP4xC2vB_final""##,
        ] {
            assert_eq!(rule_of(real), Some("high-entropy-literal"), "{real}");
        }
    }

    /// The operator table behind class 1: every `=` of a comparison is told apart from
    /// the `=` of an assignment, compound or not.
    #[test]
    fn is_comparison_eq_tells_comparisons_from_assignments() {
        let marks = |s: &str| -> Vec<bool> {
            s.match_indices('=')
                .map(|(at, _)| is_comparison_eq(s.as_bytes(), at))
                .collect()
        };
        assert_eq!(marks("a == b"), [true, true]);
        assert_eq!(marks("a === b"), [true, true, true]);
        assert_eq!(marks("a != b"), [true]);
        assert_eq!(marks("a !== b"), [true, true]);
        assert_eq!(marks("a <= b"), [true]);
        assert_eq!(marks("a >= b"), [true]);
        for binding in [
            "a = b",
            "a += b",
            "a -= b",
            "a |= b",
            "a <<= b",
            "a >>= b",
            "a := b",
            "let v: Vec<u8>= b",
        ] {
            assert!(
                marks(binding).iter().all(|&c| !c),
                "an assignment, not a comparison: {binding}"
            );
        }
        // the first binding `=` skips the comparisons before it
        assert_eq!(first_binding_eq("if a != b: c = d"), Some(13));
        assert_eq!(first_binding_eq("if a != b:"), None);
    }

    /// Found by the census of the whole analise (not in the brief's list): the name
    /// slice cut the line at a `(` INSIDE a template string, so `token.href)}" target`
    /// became the "assigned name" and `token` named a secret. An odd number of quotes
    /// means the cut fell inside a literal: the name is what follows its closing quote.
    #[test]
    fn a_secret_word_inside_a_literal_cut_by_the_name_slice_is_not_the_assigned_name() {
        // apps/web/src/lib/pagina.ts:91 [secret-named-assignment]
        let fp = r##"        return `<a href="${escapar(token.href)}" target="_blank" rel="noopener noreferrer">${texto}</a>`;"##;
        assert_eq!(rule_of(fp), None, "{fp}");
        for real in [
            // the same literal followed by a real `password=` attribute
            r##"        return `<a href="${escapar(token.href)}" password="Tr0ub4dor&3">`;"##,
            r##"        creds = {"x-token": "Tr0ub4dor&3"}"##,
            r##"        cfg = f("${esc(token.href)}", password="Tr0ub4dor&3")"##,
        ] {
            assert_eq!(rule_of(real), Some("secret-named-assignment"), "{real}");
        }
    }
}
