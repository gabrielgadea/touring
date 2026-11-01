//! Python injection sinks the shared CWE registry cannot see (S51, plan 2026-09-25).
//!
//! The `touring-offensive` registry matches PAYLOADS — it also serves the concolic /
//! input path — so a code-shaped sink with no payload in it scored 1.000 on F2.1:
//! `eval(user_value)`, `exec(user_value)` and SQL assembled inside the `execute` call
//! (measured 25/09/2026, one sink per file). This is the code judge's own detector,
//! Python-only:
//!
//! - **CWE-95** `eval(`/`exec(` whose first argument is dynamic — anything but a plain
//!   string literal that is not concatenated. `model.eval()`, `ast.literal_eval(x)`,
//!   `def eval(self)` and `eval("1 + 1")` are not sinks.
//! - **CWE-89** SQL assembled in an `execute`/`executemany`/`executescript` call — an
//!   f-string that interpolates, or a literal followed by `%`, `+` or `.format(` — when
//!   the literal carries an SQL keyword (`pool.execute(f"job-{x}")` is not SQL). A bare
//!   variable argument is not judged: its value is unknown here.
//!
//! Spans start at the CALL token, outside the literal, so the region filter in
//! [`super::security::SecurityAnalyzer::analyze`] still drops a sink quoted in a
//! comment, a string or a docstring.

use touring_offensive::vuln::VulnMatch;

/// Pattern name of a dynamic `eval`/`exec` (distinct from the registry's names, so the
/// one-finding-per-pattern dedupe never merges it with a registry match).
pub(crate) const EVAL_INJECTION: &str = "EvalInjection";
/// Pattern name of SQL assembled by string interpolation inside the call.
pub(crate) const SQL_INTERPOLATION: &str = "SQLi-interpolated";

const EVAL_SEVERITY: f32 = 9.3;
const SQL_SEVERITY: f32 = 9.8;
const SQL_KEYWORDS: [&str; 12] = [
    "SELECT", "INSERT", "UPDATE", "DELETE", "CREATE", "DROP", "ALTER", "REPLACE", "MERGE", "WITH",
    "PRAGMA", "TRUNCATE",
];

/// Every Python injection sink in `source` (see the module doc for what counts).
///
/// A call token quoted inside a string (`doc = "exec(x)"`) is text, not a call: only
/// tokens that survive the code mask unchanged count. The analyzer's region filter
/// covers comments and docstrings but keeps strings visible for the XSS registry.
pub(crate) fn detect(source: &str) -> Vec<VulnMatch> {
    let mask = super::code_regions::python_code_mask(source);
    let mut found = eval_exec_sinks(source);
    found.extend(sql_interpolation_sinks(source));
    found.retain(|m| mask.get(m.span.0) == source.as_bytes().get(m.span.0));
    found.retain(|m| !opted_out_on_line(source, m.span.0, rule_codes(&m.pattern_name)));
    found
}

/// The ruff/bandit codes an author already writes for the same rule.
fn rule_codes(pattern_name: &str) -> &'static [&'static str] {
    if pattern_name == EVAL_INJECTION {
        &["S102", "S307"]
    } else {
        &["S608"]
    }
}

/// A NAMED line-level opt-out on the call's line: `# noqa: <code>` with one of `codes`,
/// or `# nosec`. A bare `# noqa` — routinely written for unrelated lint — never
/// silences a security finding. Measured 25/09/2026: 2 of the 12 new analise findings
/// were `exec(...)  # noqa: S102 — <reason>`, a justified exception already on record.
fn opted_out_on_line(source: &str, at: usize, codes: &[&str]) -> bool {
    let start = source[..at].rfind('\n').map_or(0, |i| i + 1);
    let end = source[at..].find('\n').map_or(source.len(), |i| at + i);
    let line = &source[start..end];
    let Some(hash) = line.find('#') else {
        return false;
    };
    let comment = &line[hash..];
    comment.contains("nosec")
        || (comment.contains("noqa") && codes.iter().any(|code| comment.contains(code)))
}

fn eval_exec_sinks(source: &str) -> Vec<VulnMatch> {
    ["eval", "exec"]
        .into_iter()
        .flat_map(|name| call_sites(source, name).map(move |site| (name, site)))
        .filter(|(_, (_, arg_at))| argument_is_dynamic(&source[*arg_at..]))
        .map(|(name, (at, _))| {
            VulnMatch::new(
                EVAL_INJECTION.into(),
                (at, at + name.len()),
                EVAL_SEVERITY,
                95,
            )
        })
        .collect()
}

fn sql_interpolation_sinks(source: &str) -> Vec<VulnMatch> {
    execute_sites(source)
        .filter(|(_, arg_at)| builds_sql(&source[*arg_at..]))
        .map(|(at, _)| {
            VulnMatch::new(
                SQL_INTERPOLATION.into(),
                (at, at + "execute".len()),
                SQL_SEVERITY,
                89,
            )
        })
        .collect()
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// `(name offset, offset after "(")` of each standalone call to `name`: not an attribute
/// (`x.eval(`), not part of a longer identifier (`literal_eval(`, `execute(`), not a `def`.
fn call_sites<'a>(source: &'a str, name: &'a str) -> impl Iterator<Item = (usize, usize)> + 'a {
    source.match_indices(name).filter_map(move |(at, _)| {
        let before = at.checked_sub(1).map(|i| source.as_bytes()[i]);
        if before.is_some_and(|b| is_ident_byte(b) || b == b'.') || preceded_by_def(source, at) {
            return None;
        }
        open_paren_after(source, at + name.len()).map(|arg_at| (at, arg_at))
    })
}

/// `(offset of "execute", offset after "(")` for `.execute(`, `.executemany(`, `.executescript(`.
fn execute_sites(source: &str) -> impl Iterator<Item = (usize, usize)> + '_ {
    source
        .match_indices(".execute")
        .filter_map(move |(dot, pattern)| {
            let mut end = dot + pattern.len();
            let rest = &source[end..];
            if rest.starts_with("many") {
                end += "many".len();
            } else if rest.starts_with("script") {
                end += "script".len();
            }
            open_paren_after(source, end).map(|arg_at| (dot + 1, arg_at))
        })
}

/// Offset just after the `(` that follows `from` (spaces/tabs allowed); None otherwise.
fn open_paren_after(source: &str, from: usize) -> Option<usize> {
    let rest = &source[from..];
    let trimmed = rest.trim_start_matches([' ', '\t']);
    trimmed
        .starts_with('(')
        .then(|| from + (rest.len() - trimmed.len()) + 1)
}

fn preceded_by_def(source: &str, at: usize) -> bool {
    let head = source[..at].trim_end_matches([' ', '\t']);
    head.strip_suffix("def")
        .is_some_and(|h| h.as_bytes().last().is_none_or(|b| !is_ident_byte(*b)))
}

/// A string literal at the start of `arg`: its body, whether it is an f-string, and the
/// offset just after the closing quote.
struct Literal<'a> {
    body: &'a str,
    is_fstring: bool,
    end: usize,
}

fn parse_literal(arg: &str) -> Option<Literal<'_>> {
    let prefix_len = arg.bytes().take_while(u8::is_ascii_alphabetic).count();
    let prefix = &arg[..prefix_len];
    if prefix_len > 2 || !prefix.chars().all(|c| "rRbBuUfF".contains(c)) {
        return None;
    }
    let rest = &arg[prefix_len..];
    let quote = ["\"\"\"", "'''", "\"", "'"]
        .into_iter()
        .find(|q| rest.starts_with(q))?;
    let body_at = prefix_len + quote.len();
    let close = closing_quote(&arg[body_at..], quote)?;
    Some(Literal {
        body: &arg[body_at..body_at + close],
        is_fstring: prefix.contains(['f', 'F']),
        end: body_at + close + quote.len(),
    })
}

/// Offset of the closing `quote` in `body`, honouring backslash escapes; a single-quoted
/// literal never spans a newline.
fn closing_quote(body: &str, quote: &str) -> Option<usize> {
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'\n' if quote.len() == 1 => return None,
            _ if bytes[i..].starts_with(quote.as_bytes()) => return Some(i),
            _ => i += 1,
        }
    }
    None
}

fn interpolates(literal: &Literal<'_>) -> bool {
    literal.is_fstring && literal.body.replace("{{", "").contains('{')
}

/// A plain literal whose value is built at runtime: `"…" % x`, `"…" + x`, `"…".format(x)`.
fn combined_after(arg: &str, literal: &Literal<'_>) -> bool {
    let after = arg[literal.end..].trim_start();
    after.starts_with('%') || after.starts_with('+') || after.starts_with(".format(")
}

/// The first argument of `eval(`/`exec(` is anything but a constant string.
fn argument_is_dynamic(arg: &str) -> bool {
    let arg = arg.trim_start();
    if arg.starts_with(')') {
        return false;
    }
    match parse_literal(arg) {
        Some(literal) => interpolates(&literal) || combined_after(arg, &literal),
        None => true,
    }
}

/// The first argument of `execute(` assembles SQL from a literal at runtime.
fn builds_sql(arg: &str) -> bool {
    let arg = arg.trim_start();
    parse_literal(arg).is_some_and(|literal| {
        (interpolates(&literal) || combined_after(arg, &literal)) && has_sql_keyword(literal.body)
    })
}

fn has_sql_keyword(text: &str) -> bool {
    text.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .any(|word| SQL_KEYWORDS.iter().any(|k| word.eq_ignore_ascii_case(k)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(source: &str) -> Vec<String> {
        detect(source).into_iter().map(|m| m.pattern_name).collect()
    }

    #[test]
    fn dynamic_eval_and_exec_are_sinks() {
        for src in [
            "eval(input())",
            "exec(code)",
            "x = eval(f\"{a} + 1\")",
            "eval(\"1+\" + a)",
        ] {
            assert_eq!(names(src), [EVAL_INJECTION], "{src}");
        }
    }

    #[test]
    fn eval_lookalikes_are_not_sinks() {
        for src in [
            "model.eval()",
            "ast.literal_eval(x)",
            "def eval(self, x):",
            "eval(\"1 + 1\")",
            "evaluate(x)",
            "executor(x)",
            "eval()",
        ] {
            assert!(names(src).is_empty(), "{src}: {:?}", names(src));
        }
    }

    #[test]
    fn sql_built_in_the_execute_call_is_a_sink() {
        for src in [
            "cur.execute(f\"SELECT * FROM t WHERE a = '{x}'\")",
            "cur.execute(\"SELECT * FROM t WHERE a = '%s'\" % x)",
            "cur.execute(\"DELETE FROM t WHERE a = '\" + x + \"'\")",
            "cur.execute(\"UPDATE t SET a = '{}'\".format(x))",
            "cur.executemany(f\"\"\"\n  INSERT INTO t VALUES ('{x}')\n\"\"\", rows)",
            "cur.executescript(rf'DROP TABLE {name}')",
        ] {
            assert_eq!(names(src), [SQL_INTERPOLATION], "{src}");
        }
    }

    #[test]
    fn execute_without_assembled_sql_is_not_a_sink() {
        for src in [
            "cur.execute(\"SELECT * FROM t WHERE a = ?\", (x,))",
            "cur.execute(f\"SELECT 1\")",
            "cur.execute(f\"SELECT {{literal braces}}\")",
            "pool.execute(f\"job-{x}\")",
            "cur.execute(query)",
            "cur.execute(\"SELECT 1\")",
        ] {
            assert!(names(src).is_empty(), "{src}: {:?}", names(src));
        }
    }

    #[test]
    fn span_starts_at_the_call_token_outside_the_literal() {
        let src = "cur.execute(f\"SELECT {x}\")";
        let hit = &detect(src)[0];
        assert_eq!(&src[hit.span.0..hit.span.1], "execute");
        let src = "  eval(user)";
        assert_eq!(detect(src)[0].span, (2, 6));
    }

    #[test]
    fn a_sink_quoted_inside_a_string_is_not_a_sink() {
        for src in [
            "doc = \"exec(usuario); conn.execute(f'SELECT {usuario}')\"",
            "msg = '''\n  eval(user)\n'''",
            "x = f\"call eval(y) later\"",
        ] {
            assert!(names(src).is_empty(), "{src}: {:?}", names(src));
        }
        // Control: the real sink next to a quoted one is still found.
        assert_eq!(names("s = \"eval(a)\"; eval(b)"), [EVAL_INJECTION]);
    }

    #[test]
    fn a_named_ruff_or_bandit_opt_out_on_the_call_line_is_honoured() {
        for src in [
            "exec(code, ns)  # noqa: S102 — the profiler instruments its own code",
            "eval(expr)  # noqa: S307",
            "cur.execute(f\"SELECT * FROM [{t}]\")  # noqa: S608 — t comes from sqlite_master",
            "eval(expr)  # nosec",
            "cur = conn.execute(  # noqa: S608\n    f\"SELECT {x}\"\n)",
        ] {
            assert!(names(src).is_empty(), "{src}: {:?}", names(src));
        }
        // A bare `# noqa` or another rule's code never silences a security finding.
        for src in [
            "exec(code)  # noqa",
            "exec(code)  # noqa: E501",
            "eval(x)  # noqa: S608",
        ] {
            assert_eq!(names(src), [EVAL_INJECTION], "{src}");
        }
    }

    #[test]
    fn non_ascii_text_around_a_sink_never_panics() {
        let src = "msg = \"ação — ç\\\"é\"\ncur.execute(f\"SELECT {x} -- ç\")\neval(ñ)";
        assert_eq!(detect(src).len(), 2);
    }
}
