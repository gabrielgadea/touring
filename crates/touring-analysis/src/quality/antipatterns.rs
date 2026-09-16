//! SIMD-accelerated antipattern detection for 8 languages.
//!
//! Uses `memchr::memmem` for O(n) byte-level scanning.

use memchr::memmem;

/// The `print(` offsets that are debug residue, not the program speaking.
///
/// In a command-line script every `print` is the output of the program, and a
/// `print(…, file=sys.stderr)` anywhere is a diagnostic channel. `cidata_gen.py`,
/// `cidata_iso.py` and `hooks_runnable.py` print their JSON result and their
/// `FAIL …` message, and were the entire antipattern count of the touring
/// workspace (Canvas E, 15/09/2026).
fn debug_prints(source: &str, offsets: Vec<usize>) -> Vec<usize> {
    if has_main_guard(source) {
        return Vec::new();
    }
    offsets
        .into_iter()
        .filter(|off| !prints_to_stderr(source, *off))
        .collect()
}

/// Whether the module is a command-line entry point (`if __name__ == "__main__"`).
fn has_main_guard(source: &str) -> bool {
    source.contains("if __name__ ==")
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

/// Detect antipatterns in source code for the given language.
///
/// Returns a deduplicated list of `(warning_message, first_line_number)` tuples.
/// Line numbers are 1-indexed and indicate the first occurrence of each pattern.
pub fn detect_antipatterns(source: &str, lang: &str) -> Vec<(String, usize)> {
    let patterns: Vec<(&[u8], &str)> = match lang {
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
    };

    let bytes = source.as_bytes();
    let mut warnings: Vec<(String, usize)> = Vec::new();

    for (pattern, message) in &patterns {
        let mut all_offsets: Vec<usize> = memmem::find_iter(bytes, *pattern).collect();
        if lang == "python" && *pattern == b"print(" {
            all_offsets = debug_prints(source, all_offsets);
        }
        if all_offsets.is_empty() {
            continue;
        }
        let count = all_offsets.len();
        // First occurrence line: count \n bytes before first offset + 1 (1-indexed)
        let first_line = all_offsets
            .first()
            .copied()
            .map(|off| {
                bytes
                    .get(..off)
                    .map_or(1, |s| s.iter().filter(|&&b| b == b'\n').count() + 1)
            })
            .unwrap_or(1);
        warnings.push((format!("[{lang}] {message} ({count}x)"), first_line));
    }

    // Sort by (message, line) so dedup keeps the entry with the lowest line number
    // for each distinct message.
    warnings.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    warnings.dedup_by(|a, b| a.0 == b.0);
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let diagnostics = "import sys\n\n\ndef helper(x):\n    print(\"warn\", file=sys.stderr)\n    return x\n";
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
