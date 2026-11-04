//! Inline interpreter programs and the shell commands their exec calls carry.

use std::sync::LazyLock;

use regex::Regex;

use super::text::{backticks, call_region, string_literals};
use super::{MAX_DEPTH, scan_program};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Language {
    Python,
    Perl,
    Ruby,
    JavaScript,
    Php,
    Lua,
    Awk,
}

static PYTHON_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:ipython|pypy|python)[0-9.]*$").expect("python name"));

pub(super) fn language(name: &str) -> Option<Language> {
    if PYTHON_NAME.is_match(name) {
        return Some(Language::Python);
    }
    match name {
        "perl" => Some(Language::Perl),
        "ruby" | "irb" => Some(Language::Ruby),
        "node" | "nodejs" | "bun" | "deno" => Some(Language::JavaScript),
        "php" => Some(Language::Php),
        "lua" | "luajit" => Some(Language::Lua),
        "awk" | "gawk" | "mawk" | "nawk" => Some(Language::Awk),
        _ => None,
    }
}

/// Inline programs and whether the interpreter reads a program from stdin.
/// The first positional word is a script file or module: a later `-c` belongs
/// to that script, not to the interpreter.
pub(super) fn interpreter_invocation(language: Language, args: &[String]) -> (Vec<String>, bool) {
    if language == Language::Awk {
        return awk_invocation(args);
    }
    let mut codes = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        if arg == "-" {
            return (codes, true);
        }
        if arg.contains("<<") {
            return (codes, true);
        }
        if !arg.starts_with('-') || (language == Language::Python && arg == "-m") {
            return (codes, false);
        }
        if let Some(code) = arg
            .strip_prefix("--eval=")
            .or_else(|| arg.strip_prefix("--print="))
        {
            codes.push(code.to_owned());
        } else if inline_flag(language, arg) {
            if let Some(code) = args.get(i + 1) {
                codes.push(code.clone());
                i += 1;
            }
        } else if value_option(language, arg) {
            i += 1;
        }
        i += 1;
    }
    let reads_stdin = codes.is_empty();
    (codes, reads_stdin)
}

fn awk_invocation(args: &[String]) -> (Vec<String>, bool) {
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-F" | "-v" => i += 2,
            "-f" => return (Vec::new(), false),
            arg if arg.starts_with('-') => i += 1,
            _ => return (vec![args[i].clone()], false),
        }
    }
    (Vec::new(), true)
}

fn inline_flag(language: Language, arg: &str) -> bool {
    if !arg.starts_with('-') || arg.starts_with("--") {
        return false;
    }
    let cluster = &arg[1..];
    match language {
        Language::Python => alphabetic(cluster) && cluster.contains('c'),
        Language::Perl => alphabetic(cluster) && (cluster.contains('e') || cluster.contains('E')),
        Language::Ruby | Language::Lua => alphabetic(cluster) && cluster.contains('e'),
        Language::JavaScript => matches!(arg, "-e" | "-p" | "-pe" | "-ep"),
        Language::Php => arg == "-r",
        Language::Awk => false,
    }
}

fn alphabetic(cluster: &str) -> bool {
    !cluster.is_empty() && cluster.chars().all(|c| c.is_ascii_alphabetic())
}

fn value_option(language: Language, arg: &str) -> bool {
    match language {
        Language::Python => matches!(arg, "-W" | "-X" | "-Q"),
        Language::Perl | Language::Ruby => matches!(arg, "-I" | "-r"),
        Language::JavaScript => matches!(arg, "-r" | "--require" | "--import" | "--loader"),
        Language::Php => arg == "-d",
        Language::Lua => arg == "-l",
        Language::Awk => matches!(arg, "-F" | "-v"),
    }
}

static EXEC_CALL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?:os\.(?:system|popen\w*|exec\w*|spawn\w*|posix_spawn\w*)|subprocess\.\w+|commands\.\w+|pty\.spawn|child_process\.\w+|Open3\.\w+|Kernel\.\w+|IO\.popen|io\.popen|os\.execute|execSync|execFileSync|spawnSync|execFile|exec|spawn|system|passthru|shell_exec|proc_open|popen|Popen|check_output|check_call|getoutput|getstatusoutput|run|call)",
    )
    .expect("exec-call pattern")
});

/// Only string literals inside an exec-call region become shell programs.
/// `print('git reset --hard')` and a script's own `-c` argument stay data.
pub(super) fn exec_scan(
    language: Language,
    code: &str,
    depth: usize,
    ritual_unlock: bool,
) -> Result<(), String> {
    if depth + 1 > MAX_DEPTH {
        return Err(format!(
            "REGRA #11 v2: nested interpreter calls exceed the verification depth ({MAX_DEPTH})"
        ));
    }
    for found in EXEC_CALL.find_iter(code) {
        let Some(start) = call_start(code, found.end()) else {
            continue;
        };
        let parens = code[start..].starts_with('(');
        if !parens && !no_paren_call(&code[found.start()..found.end()]) {
            continue;
        }
        let region = call_region(code, start, parens);
        let literals = string_literals(region);
        if literals.is_empty() {
            continue;
        }
        scan_program(&literals.join(" "), depth + 1, ritual_unlock)?;
        for literal in literals {
            if literal.contains(char::is_whitespace) {
                scan_program(&literal, depth + 1, ritual_unlock)?;
            }
        }
    }
    if matches!(language, Language::Perl | Language::Ruby) {
        for literal in backticks(code) {
            scan_program(&literal, depth + 1, ritual_unlock)?;
        }
    }
    Ok(())
}

fn call_start(code: &str, match_end: usize) -> Option<usize> {
    let mut start = match_end;
    while code
        .as_bytes()
        .get(start)
        .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
    {
        start += 1;
    }
    (start < code.len()).then_some(start)
}

fn no_paren_call(call: &str) -> bool {
    let name = call.rsplit('.').next().unwrap_or(call);
    matches!(
        name,
        "system" | "exec" | "spawn" | "passthru" | "shell_exec" | "popen"
    )
}
