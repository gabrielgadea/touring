//! SIMD-accelerated antipattern detection for 8 languages.
//!
//! Uses `memchr::memmem` for O(n) byte-level scanning.

use memchr::memmem;
use std::path::Path;

/// The `print(` offsets that are debug residue, not the program speaking.
///
/// In a command-line script every `print` is the output of the program, and a
/// `print(…, file=sys.stderr)` anywhere is a diagnostic channel. `cidata_gen.py`,
/// `cidata_iso.py` and `hooks_runnable.py` print their JSON result and their
/// `FAIL …` message, and were the entire antipattern count of the touring
/// workspace (Canvas E, 15/09/2026).
fn debug_prints(source: &str, offsets: Vec<usize>, cli_script: bool) -> Vec<usize> {
    if cli_script || is_cli_script_source(source) {
        return Vec::new();
    }
    offsets
        .into_iter()
        .filter(|off| !prints_to_stderr(source, *off))
        .collect()
}

/// Whether the module, read alone, is a command-line program (S28, 25/09/2026): the
/// `__main__` guard (Canvas E), a shebang, or a module-level `print(` — top-level
/// statements only run when the file is executed, so their output is the program's.
fn is_cli_script_source(source: &str) -> bool {
    has_main_guard(source)
        || source.starts_with("#!")
        || source.lines().any(|l| l.starts_with("print("))
}

/// Whether the module is a command-line entry point (`if __name__ == "__main__"`).
fn has_main_guard(source: &str) -> bool {
    source.contains("if __name__ ==")
}

/// A Rust file that is test code as a whole (S218): under `tests/` or `benches/`, or a
/// `tests.rs` / `*_tests.rs` module whose `#[cfg(test)]` gate lives in its PARENT file.
fn is_rust_test_file(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name == "tests.rs"
        || name.ends_with("_tests.rs")
        || path
            .components()
            .any(|c| matches!(c.as_os_str().to_str(), Some("tests" | "benches")))
}

/// Whether `path` lives under a `scripts/` directory — the conventional home of
/// command-line programs, whose `print` is output (S28).
pub fn is_script_path(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == "scripts")
}

/// Whether the `print(` at `offset` writes to stderr — a diagnostic channel, not
/// a debug leftover.
fn prints_to_stderr(source: &str, offset: usize) -> bool {
    let line_start = source[..offset].rfind('\n').map_or(0, |p| p + 1);
    let line_end = source[offset..]
        .find('\n')
        .map_or(source.len(), |p| offset + p);
    source[line_start..line_end].contains("file=sys.stderr")
}

/// One antipattern rule found in a source (S28): the rule's message (a stable key across
/// runs), how often it occurs, and the 1-indexed line of its first occurrence.
///
/// This is the structured view consumers COMPARE (the post-edit/post-write block gate
/// diffs before × after); [`detect_antipatterns`] is its formatted view for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AntipatternCount {
    /// The rule's message (stable key).
    pub message: &'static str,
    /// Occurrences in the source.
    pub count: usize,
    /// 1-indexed line of the first occurrence.
    pub first_line: usize,
}

/// Offsets of `pattern` in `bytes`; with a `code_mask`, only those whose first byte
/// survived the mask — i.e. that start in CODE, not inside a literal or a comment.
fn occurrences(bytes: &[u8], pattern: &[u8], code_mask: Option<&[u8]>) -> Vec<usize> {
    memmem::find_iter(bytes, pattern)
        .filter(|&off| code_mask.is_none_or(|mask| mask.get(off) == bytes.get(off)))
        .collect()
}

/// Count every antipattern rule of `lang` in `source`, sorted by message. `path`, when
/// known, lets directory conventions apply (a `scripts/` file is a program).
pub fn count_antipatterns(source: &str, lang: &str, path: Option<&Path>) -> Vec<AntipatternCount> {
    // S218 (25/09/2026): Rust TEST code is not production — `panic!()` in the `mod tests`
    // of hook_response.rs was reported as "explicit panic in production code".
    if lang == "rust" && path.is_some_and(is_rust_test_file) {
        return Vec::new();
    }
    let cli_script = path.is_some_and(is_script_path);
    let bytes = source.as_bytes();
    let mut counts: Vec<AntipatternCount> = Vec::new();
    let mut rust_test_regions: Option<Vec<(usize, usize)>> = None;
    // Every Rust and Python pattern is a CODE token, so an occurrence inside a string
    // literal or a comment is quoted text — the pattern table itself (`b".unwrap()"`)
    // and a message saying "except: fallback" were reported (26/09/2026). Not for
    // TypeScript, whose `@ts-ignore` lives in a comment by definition.
    let code_mask =
        matches!(lang, "rust" | "python").then(|| super::code_regions::code_mask(source, lang));
    for (pattern, message) in patterns_for(lang) {
        let mut offsets = occurrences(bytes, pattern, code_mask.as_deref());
        if lang == "python" && pattern == b"print(" {
            offsets = debug_prints(source, offsets, cli_script);
        }
        if lang == "rust" && !offsets.is_empty() {
            let regions = rust_test_regions
                .get_or_insert_with(|| super::code_regions::non_executable_regions(source, "rust"));
            offsets.retain(|off| !super::code_regions::offset_suppressed(*off, regions));
        }
        let Some(&first) = offsets.first() else {
            continue;
        };
        // First occurrence line: count \n bytes before the first offset + 1 (1-indexed)
        let first_line = bytes
            .get(..first)
            .map_or(1, |s| s.iter().filter(|&&b| b == b'\n').count() + 1);
        counts.push(AntipatternCount {
            message,
            count: offsets.len(),
            first_line,
        });
    }
    counts.sort_by(|a, b| {
        a.message
            .cmp(b.message)
            .then(a.first_line.cmp(&b.first_line))
    });
    counts.dedup_by(|a, b| a.message == b.message);
    counts
}

/// Detect antipatterns in source code for the given language.
///
/// Returns a deduplicated list of `(warning_message, first_line_number)` tuples.
/// Line numbers are 1-indexed and indicate the first occurrence of each pattern.
pub fn detect_antipatterns(source: &str, lang: &str) -> Vec<(String, usize)> {
    render(lang, count_antipatterns(source, lang, None))
}

/// [`detect_antipatterns`] for a file at `path`, so directory conventions apply (S28).
pub fn detect_antipatterns_at(source: &str, lang: &str, path: &Path) -> Vec<(String, usize)> {
    render(lang, count_antipatterns(source, lang, Some(path)))
}

/// The formatted view of the counts: `[lang] message (Nx)` with the first line.
fn render(lang: &str, counts: Vec<AntipatternCount>) -> Vec<(String, usize)> {
    counts
        .into_iter()
        .map(|c| {
            (
                format!("[{lang}] {} ({}x)", c.message, c.count),
                c.first_line,
            )
        })
        .collect()
}

/// The rule table of `lang`: `(byte pattern, message)`.
fn patterns_for(lang: &str) -> Vec<(&'static [u8], &'static str)> {
    match lang {
        "rust" => vec![
            (
                b".unwrap()",
                "`.unwrap()` \u{2014} use `?` or `.expect(\"reason\")`",
            ),
            (b"todo!()", "`todo!()` \u{2014} incomplete implementation"),
            (b"unimplemented!()", "`unimplemented!()` \u{2014} stub code"),
            (
                b"panic!(",
                "`panic!()` \u{2014} explicit panic in production code",
            ),
            (
                b"unsafe {",
                "`unsafe {}` \u{2014} document invariants or encapsulate in safe wrapper",
            ),
            (
                b"mem::forget(",
                "`mem::forget()` \u{2014} prefer `ManuallyDrop` or RAII",
            ),
            (
                b"#[allow(dead_code)]",
                "`#[allow(dead_code)]` \u{2014} remove unused code instead of suppressing",
            ),
            (
                b"as *mut ",
                "`as *mut` \u{2014} raw pointer cast, prefer safe abstractions",
            ),
            (
                b"transmute(",
                "`transmute()` \u{2014} extremely unsafe, use safer transmutation",
            ),
        ],
        "python" => vec![
            (
                b"except:",
                "bare `except:` \u{2014} catch specific exceptions",
            ),
            (
                b"print(",
                "`print()` \u{2014} debug output left in production code",
            ),
            (
                b"import *",
                "`import *` \u{2014} wildcard import pollutes namespace",
            ),
            (
                b"\nglobal ",
                "`global` \u{2014} mutable global state harms testability",
            ),
        ],
        "typescript" | "ts" => vec![
            (b": any", "`: any` \u{2014} avoid untyped values"),
            (b"<any>", "`<any>` \u{2014} avoid type assertion to any"),
            (b"as any", "`as any` \u{2014} avoid casting to any"),
            (
                b"console.log(",
                "`console.log()` \u{2014} debug output left in code",
            ),
            (
                b"@ts-ignore",
                "`@ts-ignore` \u{2014} suppresses type errors, fix the type instead",
            ),
            (
                b"@ts-nocheck",
                "`@ts-nocheck` \u{2014} disables type checking for the file",
            ),
        ],
        "javascript" | "js" => vec![
            (
                b"== null",
                "`== null` \u{2014} use strict equality `=== null`",
            ),
            (b"var ", "`var` \u{2014} use `let` or `const`"),
            (
                b"console.log(",
                "`console.log()` \u{2014} debug output left in code",
            ),
        ],
        "go" => vec![
            (
                b"panic(",
                "`panic()` \u{2014} avoid panics in production Go code",
            ),
            (
                b"_ = err",
                "`_ = err` \u{2014} silently discards error, handle it explicitly",
            ),
            (
                b"log.Fatal(",
                "`log.Fatal()` \u{2014} calls os.Exit(), prevents deferred cleanup",
            ),
        ],
        "c" => vec![
            (
                b"gets(",
                "`gets()` \u{2014} buffer overflow risk, use `fgets()`",
            ),
            (
                b"sprintf(",
                "`sprintf()` \u{2014} buffer overflow risk, use `snprintf()`",
            ),
            (
                b"strcpy(",
                "`strcpy()` \u{2014} buffer overflow risk, use `strncpy()`",
            ),
            (
                b"strtok(",
                "`strtok()` \u{2014} not thread-safe, use `strtok_r()`",
            ),
        ],
        "cpp" | "c++" => vec![
            (b"gets(", "`gets()` \u{2014} buffer overflow risk"),
            (b"sprintf(", "`sprintf()` \u{2014} use `snprintf()`"),
            (b"NULL", "`NULL` \u{2014} use `nullptr` in modern C++"),
        ],
        "java" => vec![
            (
                b"catch(Exception ",
                "`catch(Exception)` \u{2014} catch specific exceptions",
            ),
            (
                b"catch (Exception ",
                "`catch (Exception)` \u{2014} catch specific exceptions",
            ),
            (
                b"System.out.println(",
                "`System.out.println()` \u{2014} use a logger instead",
            ),
            (
                b"e.printStackTrace(",
                "`e.printStackTrace()` \u{2014} use structured logging instead",
            ),
        ],
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_test_code_is_not_production() {
        let flags = |c: &[AntipatternCount]| {
            c.iter()
                .any(|a| a.message.contains("panic!()") || a.message.contains(".unwrap()"))
        };
        let module = "pub fn f() -> u8 { 1 }\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {\n        let x: Option<u8> = None;\n        x.unwrap();\n        panic!(\"boom\");\n    }\n}\n";
        assert!(
            !flags(&count_antipatterns(module, "rust", None)),
            "cfg(test) module"
        );
        let prod = "pub fn f() {\n    let x: Option<u8> = None;\n    x.unwrap();\n    panic!(\"boom\");\n}\n";
        assert!(
            flags(&count_antipatterns(prod, "rust", None)),
            "control: production counts"
        );
        for test_path in [
            "crates/x/tests/it.rs",
            "crates/x/src/wiring_tests.rs",
            "crates/x/src/tests.rs",
        ] {
            assert!(
                count_antipatterns(prod, "rust", Some(Path::new(test_path))).is_empty(),
                "{test_path}"
            );
        }
        assert!(flags(&count_antipatterns(
            prod,
            "rust",
            Some(Path::new("crates/x/src/lib.rs"))
        )));
    }

    /// S28 (plan 2026-09-25): a command-line script's `print` is its output. Four markers,
    /// each enough alone: the `__main__` guard (Canvas E), a shebang, a module-level
    /// `print(` (top-level statements only run as a program), a `scripts/` directory.
    #[test]
    fn print_in_cli_script_is_not_an_issue() {
        let flags_print = |w: &[(String, usize)]| w.iter().any(|(m, _)| m.contains("print()"));
        let shebang = "#!/usr/bin/env python3\nimport sys\n\n\ndef run(x):\n    print(x)\n";
        assert!(
            !flags_print(&detect_antipatterns(shebang, "python")),
            "shebang marks a program"
        );
        let flat = "import json\nfor k in (1, 2):\n    print(k)\nprint(json.dumps({}))\n";
        assert!(
            !flags_print(&detect_antipatterns(flat, "python")),
            "module-level print marks a program"
        );
        let lib = "import sys\n\n\ndef helper(x):\n    print(x)\n    return x\n";
        assert!(
            flags_print(&detect_antipatterns(lib, "python")),
            "control: a library still reports"
        );
        assert!(
            !flags_print(&detect_antipatterns_at(
                lib,
                "python",
                Path::new("repo/scripts/tool/helper.py")
            )),
            "a scripts/ directory marks programs"
        );
        assert!(
            flags_print(&detect_antipatterns_at(
                lib,
                "python",
                Path::new("repo/src/helper.py")
            )),
            "control: outside scripts/ the library rule holds"
        );
        let bare = "#!/usr/bin/env python3\ntry:\n    pass\nexcept:\n    pass\n";
        assert!(
            detect_antipatterns(bare, "python")
                .iter()
                .any(|(m, _)| m.contains("except:")),
            "a script's real antipatterns are still reported"
        );
    }

    /// S28: the block gate reads COUNTS, never re-parses the formatted text — the old gate
    /// parsed `ANTIPATTERN [Nx]: …`, a form no producer emits, and never fired.
    #[test]
    fn counts_are_structured_and_match_the_formatted_view() {
        let src = "fn a() { x.unwrap(); y.unwrap(); todo!(); }";
        let counts = count_antipatterns(src, "rust", None);
        let unwraps = counts
            .iter()
            .find(|c| c.message.contains(".unwrap()"))
            .expect("unwrap rule");
        assert_eq!(unwraps.count, 2);
        let todos = counts
            .iter()
            .find(|c| c.message.contains("todo!()"))
            .expect("todo rule");
        assert_eq!(todos.count, 1);
        let formatted = detect_antipatterns(src, "rust");
        assert_eq!(
            formatted.len(),
            counts.len(),
            "one formatted line per counted rule"
        );
        assert!(
            formatted
                .iter()
                .any(|(m, _)| m.ends_with("(2x)") && m.contains(".unwrap()"))
        );
    }

    #[test]
    fn test_rust_unwrap_detected() {
        let source = "fn main() { let x = foo().unwrap(); }";
        let w = detect_antipatterns(source, "rust");
        assert_eq!(w.len(), 1);
        let first = w.first().expect("one unwrap pattern");
        assert!(first.0.contains(".unwrap()"));
        assert_eq!(first.1, 1);
    }

    #[test]
    fn test_rust_multiple_patterns() {
        let source = "fn main() { todo!(); foo().unwrap(); }";
        let w = detect_antipatterns(source, "rust");
        assert_eq!(w.len(), 2);
    }

    #[test]
    fn test_rust_clean_code() {
        let source = "fn main() -> Result<(), Error> { let x = foo()?; Ok(()) }";
        let w = detect_antipatterns(source, "rust");
        assert!(w.is_empty());
    }

    /// Canvas E (15/09/2026): the three antipatterns of the whole touring workspace
    /// were the `print` calls of three CLI scripts printing their own output.
    #[test]
    fn print_is_output_in_a_cli_script_and_debug_residue_in_a_library() {
        let cli = "import json, sys\n\n\ndef main():\n    print(json.dumps({\"ok\": True}))\n    print(\"FAIL missing tool\", file=sys.stderr)\n    try:\n        pass\n    except:\n        pass\n    return 0\n\n\nif __name__ == \"__main__\":\n    sys.exit(main())\n";
        let w = detect_antipatterns(cli, "python");
        assert!(!w.iter().any(|(m, _)| m.contains("print()")), "{w:?}");
        assert!(
            w.iter().any(|(m, _)| m.contains("except:")),
            "the bare except of the same script is still reported: {w:?}"
        );

        let library = "import sys\n\n\ndef helper(x):\n    print(x)\n    return x\n";
        let w = detect_antipatterns(library, "python");
        assert!(w.iter().any(|(m, _)| m.contains("print()")), "{w:?}");

        let diagnostics =
            "import sys\n\n\ndef helper(x):\n    print(\"warn\", file=sys.stderr)\n    return x\n";
        assert!(
            !detect_antipatterns(diagnostics, "python")
                .iter()
                .any(|(m, _)| m.contains("print()")),
            "stderr is a diagnostic channel"
        );
        let mixed = "import sys\n\n\ndef helper(x):\n    print(\"warn\", file=sys.stderr)\n    print(x)\n    return x\n";
        assert!(
            detect_antipatterns(mixed, "python")
                .iter()
                .any(|(m, _)| m.contains("print()")),
            "the plain print beside it is still reported"
        );
    }

    #[test]
    fn test_python_bare_except() {
        let source = "try:\n    foo()\nexcept:\n    pass";
        let w = detect_antipatterns(source, "python");
        assert_eq!(w.len(), 1);
        let first = w.first().expect("one except pattern");
        assert_eq!(first.1, 3, "except: is on line 3");
    }

    /// 26/09/2026: a pattern inside a string literal or a comment is quoted text —
    /// `"… except: fallback"` in a message was reported as a bare except, the same
    /// way the Rust branch already skips literals and comments.
    #[test]
    fn python_patterns_inside_strings_and_comments_are_not_code() {
        let source = "MSG = \"try: fast except: fallback\"  # except: see above\nHINT = 'print(x) to debug'\n";
        let w = detect_antipatterns(source, "python");
        assert!(w.is_empty(), "quoted patterns are not code: {w:?}");
    }

    /// The Rust half: the pattern TABLE of this very module (`b".unwrap()"`,
    /// `"panic!("`) was reported as production `.unwrap()`/`panic!()` by the
    /// post-edit hook; the real call beside it must still be counted.
    #[test]
    fn rust_patterns_inside_literals_and_comments_are_not_code() {
        let quoted = "const P: &[u8] = b\".unwrap()\";\nconst M: &str = \"panic!(boom)\"; // todo!() later\nconst R: &str = r#\"transmute(\"#;\n";
        assert!(
            detect_antipatterns(quoted, "rust").is_empty(),
            "quoted patterns are not code: {:?}",
            detect_antipatterns(quoted, "rust")
        );
        let real = format!("{quoted}fn f(x: Option<u8>) -> u8 {{ x.unwrap() }}\n");
        let w = detect_antipatterns(&real, "rust");
        assert_eq!(w.len(), 1, "only the real call: {w:?}");
        assert_eq!(w[0].1, 4, "the real `.unwrap()` is on line 4");
    }

    #[test]
    fn test_typescript_any() {
        let source = "const x: any = 5; const y = z as any;";
        let w = detect_antipatterns(source, "typescript");
        assert!(w.len() >= 2);
    }

    #[test]
    fn test_go_panic() {
        let source = "func main() { panic(\"oops\") }";
        let w = detect_antipatterns(source, "go");
        assert_eq!(w.len(), 1);
    }

    #[test]
    fn test_c_gets() {
        let source = "char buf[10]; gets(buf);";
        let w = detect_antipatterns(source, "c");
        assert!(w.len() >= 1);
    }

    #[test]
    fn test_java_catch_exception() {
        let source = "try { } catch(Exception e) { }";
        let w = detect_antipatterns(source, "java");
        assert_eq!(w.len(), 1);
    }

    #[test]
    fn test_unknown_language() {
        let w = detect_antipatterns("anything here", "brainfuck");
        assert!(w.is_empty());
    }

    #[test]
    fn test_empty_source() {
        let w = detect_antipatterns("", "rust");
        assert!(w.is_empty());
    }

    #[test]
    fn test_count_multiple_occurrences() {
        let source = "a.unwrap(); b.unwrap(); c.unwrap();";
        let w = detect_antipatterns(source, "rust");
        assert_eq!(w.len(), 1);
        let first = w.first().expect("one pattern aggregated");
        assert!(first.0.contains("3x"));
    }

    #[test]
    fn test_line_number_first_occurrence() {
        let source = "fn a() {}\nfn b() {}\nfn c() { todo!(); }";
        let w = detect_antipatterns(source, "rust");
        let todo_entry = w.iter().find(|(msg, _)| msg.contains("todo!"));
        assert!(todo_entry.is_some(), "todo! should be detected");
        assert_eq!(todo_entry.unwrap().1, 3, "todo!() is on line 3");
    }

    #[test]
    fn test_line_number_multiple_occurrences_reports_first() {
        // Two unwraps: line 2 and line 5 — must report line 2
        let source = "fn a() {}\nfoo.unwrap();\nfn b() {}\nfn c() {}\nbar.unwrap();";
        let w = detect_antipatterns(source, "rust");
        let entry = w.iter().find(|(msg, _)| msg.contains(".unwrap()"));
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().1, 2, "first unwrap is on line 2");
        assert!(
            entry.unwrap().0.contains("2x"),
            "should count both occurrences"
        );
    }

    #[test]
    fn test_line_number_line_1() {
        let source = "foo.unwrap(); // line 1\nbar();";
        let w = detect_antipatterns(source, "rust");
        let entry = w.iter().find(|(msg, _)| msg.contains(".unwrap()"));
        assert!(entry.is_some());
        assert_eq!(
            entry.unwrap().1,
            1,
            "unwrap on first line should report line 1"
        );
    }
}
