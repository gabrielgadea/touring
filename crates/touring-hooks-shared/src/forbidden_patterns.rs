//! P1.3 CEG -- AST-based forbidden-call detector for the ctx_execute sandbox.
//!
//! Provides security-grade pattern sets for 7 languages with ast-grep grammar
//! support, plus documented substring fallback for Go, Shell, Perl and R.
//!
//! # Grammar ABI Status (ast-grep-language 0.36 / ast-grep-core 0.36)
//!
//! | Language   | Method    | Grammar ABI | Notes                                  |
//! |------------|-----------|-------------|----------------------------------------|
//! | JavaScript | AST       | v14 OK      | tree-sitter-javascript                 |
//! | TypeScript | AST       | v14 OK      | tree-sitter-typescript                 |
//! | Python     | AST       | v14 OK      | tree-sitter-python                     |
//! | Ruby       | AST       | v14 OK      | tree-sitter-ruby                       |
//! | Rust       | AST       | v14 OK      | tree-sitter-rust                       |
//! | Php        | AST       | v14 OK      | tree-sitter-php                        |
//! | Elixir     | AST       | v14 OK      | tree-sitter-elixir (P1.3 CEG addition) |
//! | Go         | Substring | v15 PANIC   | B-FUZZ-002: ABI mismatch in 0.36       |
//! | Shell/Bash | Substring | v15 PANIC   | B-FUZZ-001: ABI mismatch in 0.36       |
//! | Perl       | Substring | No grammar  | Not in ast-grep-language 0.36          |
//! | R          | Substring | No grammar  | Not in ast-grep-language 0.36          |
//!
//! Go and Shell use substring fallback until ast-grep is upgraded to 0.38+
//! which ships tree-sitter grammars with ABI v14. Tracked as B-FUZZ-001/002.

use std::time::Duration;

use crate::ast_grep_signal::{PatternEntry, PatternSet, ScanResult, scan_source};
use crate::sandbox_language::SandboxLanguage;
use touring_code::polyglot::Lang;

// -- Budget -------------------------------------------------------------------

/// Scan budget for the forbidden-call scanner. Generous compared to the
/// pre-read budget (30 ms) -- ctx_execute is an async MCP call so
/// latency is less critical than correctness.
const FORBIDDEN_BUDGET: Duration = Duration::from_millis(100);

// -- Forbidden pattern sets (AST-backed languages only) -----------------------

/// JavaScript forbidden patterns.
pub const FORBIDDEN_JS: PatternSet = PatternSet {
    set_id: "forbidden_js_v1",
    language: Lang::JavaScript,
    patterns: &[
        // Bare-identifier calls. `eval` / `Function` are genuine globals.
        PatternEntry {
            id: "eval",
            pattern: "eval($$$)",
            label: "eval (code injection)",
        },
        PatternEntry {
            id: "function_ctor",
            pattern: "new Function($$$)",
            label: "Function constructor (code injection)",
        },
        PatternEntry {
            id: "exec_sync",
            pattern: "execSync($$$)",
            label: "execSync (shell injection)",
        },
        PatternEntry {
            id: "exec_call",
            pattern: "exec($$$)",
            label: "exec (shell injection)",
        },
        PatternEntry {
            id: "spawn_sync",
            pattern: "spawnSync($$$)",
            label: "spawnSync (subprocess)",
        },
        PatternEntry {
            id: "spawn_call",
            pattern: "spawn($$$)",
            label: "spawn (subprocess)",
        },
        PatternEntry {
            id: "write_file_sync",
            pattern: "writeFileSync($$$)",
            label: "writeFileSync (filesystem write)",
        },
        PatternEntry {
            id: "unlink_sync",
            pattern: "unlinkSync($$$)",
            label: "unlinkSync (file deletion)",
        },
        PatternEntry {
            id: "rm_sync",
            pattern: "rmSync($$$)",
            label: "rmSync (recursive delete)",
        },
        PatternEntry {
            id: "deno_command",
            pattern: "new Deno.Command($$$)",
            label: "Deno.Command (subprocess)",
        },
        // Member-call forms — Node APIs are almost always invoked through a
        // receiver (`fs.writeFileSync`, `child_process.execSync`, `cp.spawn`).
        // `$X` matches any receiver node, incl. nested members and `require(..)`.
        PatternEntry {
            id: "exec_sync_member",
            pattern: "$X.execSync($$$)",
            label: "execSync (shell injection)",
        },
        PatternEntry {
            id: "exec_member",
            pattern: "$X.exec($$$)",
            label: "exec (shell injection)",
        },
        PatternEntry {
            id: "spawn_sync_member",
            pattern: "$X.spawnSync($$$)",
            label: "spawnSync (subprocess)",
        },
        PatternEntry {
            id: "spawn_member",
            pattern: "$X.spawn($$$)",
            label: "spawn (subprocess)",
        },
        PatternEntry {
            id: "write_file_sync_member",
            pattern: "$X.writeFileSync($$$)",
            label: "writeFileSync (filesystem write)",
        },
        PatternEntry {
            id: "unlink_sync_member",
            pattern: "$X.unlinkSync($$$)",
            label: "unlinkSync (file deletion)",
        },
        PatternEntry {
            id: "rm_sync_member",
            pattern: "$X.rmSync($$$)",
            label: "rmSync (recursive delete)",
        },
    ],
};

/// TypeScript uses the same patterns as JavaScript (same grammar).
pub const FORBIDDEN_TS: PatternSet = PatternSet {
    set_id: "forbidden_ts_v1",
    language: Lang::TypeScript,
    patterns: FORBIDDEN_JS.patterns,
};

/// Python forbidden patterns.
pub const FORBIDDEN_PYTHON: PatternSet = PatternSet {
    set_id: "forbidden_python_v1",
    language: Lang::Python,
    patterns: &[
        PatternEntry {
            id: "eval",
            pattern: "eval($$$)",
            label: "eval (code injection)",
        },
        PatternEntry {
            id: "exec_call",
            pattern: "exec($$$)",
            label: "exec (code injection)",
        },
        PatternEntry {
            id: "os_system",
            pattern: "os.system($$$)",
            label: "os.system (shell injection)",
        },
        PatternEntry {
            id: "os_popen",
            pattern: "os.popen($$$)",
            label: "os.popen (shell injection)",
        },
        PatternEntry {
            id: "subprocess_run",
            pattern: "subprocess.run($$$)",
            label: "subprocess.run (subprocess)",
        },
        PatternEntry {
            id: "subprocess_call",
            pattern: "subprocess.call($$$)",
            label: "subprocess.call (subprocess)",
        },
        PatternEntry {
            id: "subprocess_popen",
            pattern: "subprocess.Popen($$$)",
            label: "subprocess.Popen (subprocess)",
        },
        PatternEntry {
            id: "subprocess_check",
            pattern: "subprocess.check_output($$$)",
            label: "subprocess.check_output (subprocess)",
        },
        PatternEntry {
            id: "pickle_loads",
            pattern: "pickle.loads($$$)",
            label: "pickle.loads (unsafe deserialization)",
        },
        PatternEntry {
            id: "marshal_loads",
            pattern: "marshal.loads($$$)",
            label: "marshal.loads (unsafe deserialization)",
        },
        PatternEntry {
            id: "os_remove",
            pattern: "os.remove($$$)",
            label: "os.remove (file deletion)",
        },
        PatternEntry {
            id: "os_unlink",
            pattern: "os.unlink($$$)",
            label: "os.unlink (file deletion)",
        },
        PatternEntry {
            id: "shutil_rmtree",
            pattern: "shutil.rmtree($$$)",
            label: "shutil.rmtree (recursive delete)",
        },
        PatternEntry {
            id: "dunder_import",
            pattern: "__import__($$$)",
            label: "__import__ (dynamic import)",
        },
    ],
};

/// Ruby forbidden patterns.
pub const FORBIDDEN_RUBY: PatternSet = PatternSet {
    set_id: "forbidden_ruby_v1",
    language: Lang::Ruby,
    patterns: &[
        PatternEntry {
            id: "eval",
            pattern: "eval($$$)",
            label: "eval (code injection)",
        },
        PatternEntry {
            id: "instance_eval",
            pattern: "$X.instance_eval($$$)",
            label: "instance_eval (code injection)",
        },
        PatternEntry {
            id: "class_eval",
            pattern: "$X.class_eval($$$)",
            label: "class_eval (code injection)",
        },
        PatternEntry {
            id: "system_call",
            pattern: "system($$$)",
            label: "system (shell injection)",
        },
        PatternEntry {
            id: "exec_call",
            pattern: "exec($$$)",
            label: "exec (subprocess replacement)",
        },
        PatternEntry {
            id: "spawn_call",
            pattern: "spawn($$$)",
            label: "spawn (subprocess)",
        },
        PatternEntry {
            id: "file_delete",
            pattern: "File.delete($$$)",
            label: "File.delete (file deletion)",
        },
        PatternEntry {
            id: "fileutils_rm",
            pattern: "FileUtils.rm($$$)",
            label: "FileUtils.rm (file deletion)",
        },
        PatternEntry {
            id: "fu_rm_rf",
            pattern: "FileUtils.rm_rf($$$)",
            label: "FileUtils.rm_rf (recursive delete)",
        },
    ],
};

/// Rust forbidden patterns.
pub const FORBIDDEN_RUST: PatternSet = PatternSet {
    set_id: "forbidden_rust_v1",
    language: Lang::Rust,
    patterns: &[
        PatternEntry {
            id: "command_new",
            pattern: "Command::new($$$)",
            label: "Command::new (subprocess)",
        },
        PatternEntry {
            id: "process_exit",
            pattern: "process::exit($$$)",
            label: "process::exit (process kill)",
        },
        PatternEntry {
            id: "fs_remove_file",
            pattern: "fs::remove_file($$$)",
            label: "fs::remove_file (file deletion)",
        },
        PatternEntry {
            id: "fs_remove_dir",
            pattern: "fs::remove_dir_all($$$)",
            label: "fs::remove_dir_all (recursive delete)",
        },
        PatternEntry {
            id: "mem_transmute",
            pattern: "mem::transmute($$$)",
            label: "mem::transmute (unsafe cast)",
        },
    ],
};

/// PHP forbidden patterns.
pub const FORBIDDEN_PHP: PatternSet = PatternSet {
    set_id: "forbidden_php_v1",
    language: Lang::Php,
    patterns: &[
        PatternEntry {
            id: "eval",
            pattern: "eval($$$)",
            label: "eval (code injection)",
        },
        PatternEntry {
            id: "system",
            pattern: "system($$$)",
            label: "system (shell injection)",
        },
        PatternEntry {
            id: "exec_call",
            pattern: "exec($$$)",
            label: "exec (shell injection)",
        },
        PatternEntry {
            id: "shell_exec",
            pattern: "shell_exec($$$)",
            label: "shell_exec (shell injection)",
        },
        PatternEntry {
            id: "passthru",
            pattern: "passthru($$$)",
            label: "passthru (shell injection)",
        },
        PatternEntry {
            id: "proc_open",
            pattern: "proc_open($$$)",
            label: "proc_open (subprocess)",
        },
        PatternEntry {
            id: "unlink_call",
            pattern: "unlink($$$)",
            label: "unlink (file deletion)",
        },
    ],
};

/// Elixir forbidden patterns.
pub const FORBIDDEN_ELIXIR: PatternSet = PatternSet {
    set_id: "forbidden_elixir_v1",
    language: Lang::Elixir,
    patterns: &[
        PatternEntry {
            id: "system_cmd",
            pattern: "System.cmd($$$)",
            label: "System.cmd (subprocess)",
        },
        PatternEntry {
            id: "system_shell",
            pattern: "System.shell($$$)",
            label: "System.shell (shell injection)",
        },
        PatternEntry {
            id: "code_eval_string",
            pattern: "Code.eval_string($$$)",
            label: "Code.eval_string (code injection)",
        },
        PatternEntry {
            id: "code_eval_file",
            pattern: "Code.eval_file($$$)",
            label: "Code.eval_file (code injection)",
        },
        PatternEntry {
            id: "file_rm",
            pattern: "File.rm($$$)",
            label: "File.rm (file deletion)",
        },
        PatternEntry {
            id: "file_rm_rf_bang",
            pattern: "File.rm_rf!($$$)",
            label: "File.rm_rf! (recursive delete)",
        },
    ],
};

// -- Substring fallbacks for grammar-limited languages ------------------------

/// Go substring fallback.
///
/// Go tree-sitter grammar in ast-grep-language 0.36 uses ABI v15, which panics
/// against ast-grep-core 0.36 (B-FUZZ-002). Use substring detection until
/// ast-grep is upgraded to 0.38+ (ships ABI v14-compatible Go grammar).
pub fn go_forbidden_substring(code: &str) -> Vec<String> {
    const PATTERNS: &[(&str, &str)] = &[
        ("exec.Command(", "exec.Command (subprocess)"),
        ("exec.CommandContext(", "exec.CommandContext (subprocess)"),
        ("os.Remove(", "os.Remove (file deletion)"),
        ("os.RemoveAll(", "os.RemoveAll (recursive delete)"),
        ("unsafe.Pointer(", "unsafe.Pointer (unsafe memory)"),
        ("os.StartProcess(", "os.StartProcess (subprocess)"),
    ];
    PATTERNS
        .iter()
        .filter(|(pat, _)| code.contains(pat))
        .map(|(_, label)| (*label).to_string())
        .collect()
}

/// Shell/Bash substring fallback.
///
/// tree-sitter-bash in ast-grep-language 0.36 uses ABI v15, which panics
/// against ast-grep-core 0.36 (B-FUZZ-001). Use substring detection until
/// ast-grep is upgraded to 0.38+ (ships ABI v14-compatible bash grammar).
pub fn shell_forbidden_substring(code: &str) -> Vec<String> {
    const PATTERNS: &[(&str, &str)] = &[
        ("eval ", "eval (code injection)"),
        ("eval\t", "eval (code injection)"),
        ("$(eval", "eval (code injection)"),
        ("rm -rf", "rm -rf (recursive delete)"),
        ("rm -fr", "rm -fr (recursive delete)"),
        (" dd ", "dd (raw disk write)"),
        ("mkfs", "mkfs (filesystem format)"),
        ("shred ", "shred (file destruction)"),
        ("curl | sh", "curl pipe to sh (remote code execution)"),
        ("wget | sh", "wget pipe to sh (remote code execution)"),
        ("| bash", "pipe to bash (remote code execution)"),
        ("| sh", "pipe to sh (remote code execution)"),
    ];
    PATTERNS
        .iter()
        .filter(|(pat, _)| code.contains(pat))
        .map(|(_, label)| (*label).to_string())
        .collect()
}

/// Perl substring fallback -- no ast-grep grammar in ast-grep-language 0.36.
/// May produce false positives inside string literals/comments.
/// Use the `allow_forbidden` override param to bypass when needed.
pub fn perl_forbidden_substring(code: &str) -> Vec<String> {
    const PATTERNS: &[(&str, &str)] = &[
        ("eval ", "eval (code injection)"),
        ("eval(", "eval (code injection)"),
        ("system(", "system (shell injection)"),
        ("exec(", "exec (subprocess)"),
        ("`", "backtick operator (shell injection)"),
        ("qx{", "qx{} (shell injection)"),
        ("qx/", "qx// (shell injection)"),
        ("unlink(", "unlink (file deletion)"),
        ("File::Path::remove_tree", "remove_tree (recursive delete)"),
    ];
    PATTERNS
        .iter()
        .filter(|(pat, _)| code.contains(pat))
        .map(|(_, label)| (*label).to_string())
        .collect()
}

/// R substring fallback -- no ast-grep grammar in ast-grep-language 0.36.
pub fn r_forbidden_substring(code: &str) -> Vec<String> {
    const PATTERNS: &[(&str, &str)] = &[
        ("eval(", "eval (code injection)"),
        ("parse(text=", "parse+eval (code injection)"),
        ("system(", "system (shell injection)"),
        ("system2(", "system2 (subprocess)"),
        ("shell(", "shell (shell injection)"),
        ("file.remove(", "file.remove (file deletion)"),
        ("unlink(", "unlink (file deletion)"),
        ("unserialize(", "unserialize (unsafe deserialization)"),
    ];
    PATTERNS
        .iter()
        .filter(|(pat, _)| code.contains(pat))
        .map(|(_, label)| (*label).to_string())
        .collect()
}

// -- SandboxLanguage -> polyglot::Lang mapper ---------------------------------

/// Map a [`SandboxLanguage`] to the corresponding [`Lang`] for AST scanning.
///
/// Returns `None` for Go, Shell, Perl, and R (ABI mismatch or no grammar in
/// ast-grep-language 0.36 -- all use substring fallback instead).
pub fn sandbox_lang_to_polyglot(lang: SandboxLanguage) -> Option<Lang> {
    match lang {
        SandboxLanguage::JavaScript => Some(Lang::JavaScript),
        SandboxLanguage::TypeScript => Some(Lang::TypeScript),
        SandboxLanguage::Python => Some(Lang::Python),
        SandboxLanguage::Ruby => Some(Lang::Ruby),
        SandboxLanguage::Rust => Some(Lang::Rust),
        SandboxLanguage::Php => Some(Lang::Php),
        SandboxLanguage::Elixir => Some(Lang::Elixir),
        // Substring fallback (ABI mismatch or no grammar):
        SandboxLanguage::Go
        | SandboxLanguage::Shell
        | SandboxLanguage::Perl
        | SandboxLanguage::R => None,
    }
}

/// Resolve the forbidden [`PatternSet`] for an AST-backed [`SandboxLanguage`].
/// Returns `None` for languages using substring fallback.
fn forbidden_pattern_set(lang: SandboxLanguage) -> Option<&'static PatternSet> {
    match lang {
        SandboxLanguage::JavaScript => Some(&FORBIDDEN_JS),
        SandboxLanguage::TypeScript => Some(&FORBIDDEN_TS),
        SandboxLanguage::Python => Some(&FORBIDDEN_PYTHON),
        SandboxLanguage::Ruby => Some(&FORBIDDEN_RUBY),
        SandboxLanguage::Rust => Some(&FORBIDDEN_RUST),
        SandboxLanguage::Php => Some(&FORBIDDEN_PHP),
        SandboxLanguage::Elixir => Some(&FORBIDDEN_ELIXIR),
        // Substring fallback (ABI mismatch or no grammar):
        SandboxLanguage::Go
        | SandboxLanguage::Shell
        | SandboxLanguage::Perl
        | SandboxLanguage::R => None,
    }
}

// -- Hybrid entry point -------------------------------------------------------

/// Scan `code` for forbidden calls using the appropriate strategy for `lang`.
///
/// This is the P1.3 new symbol: `ast_forbidden_scan`.
///
/// - **7 AST-backed languages** (JS, TS, Python, Ruby, Rust, Php, Elixir):
///   uses `scan_source` with `FORBIDDEN_*` pattern sets. Fail-open: budget
///   overrun returns partial results; grammar errors skip silently.
/// - **Go, Shell**: substring fallback (B-FUZZ-001/002 ABI v15 panic in 0.36).
/// - **Perl, R**: curated substring fallback (no grammar in ast-grep 0.36).
///
/// Returns labels of detected forbidden calls. Empty vec = clean.
pub fn ast_forbidden_scan(lang: SandboxLanguage, code: &str) -> Vec<String> {
    // Dispatch on AST capability: a language that maps to a polyglot `Lang`
    // has an ast-grep grammar (ABI v14 OK) and takes the AST path; the rest
    // (Go/Shell ABI v15 panic, Perl/R no grammar) use the substring fallback.
    if sandbox_lang_to_polyglot(lang).is_some() {
        if let Some(pattern_set) = forbidden_pattern_set(lang) {
            let result: ScanResult = scan_source(code, pattern_set, FORBIDDEN_BUDGET);
            return result
                .matches
                .into_iter()
                .map(|pc| pc.label.to_string())
                .collect();
        }
        return Vec::new();
    }
    // Substring fallback for languages with ABI issues or no grammar.
    match lang {
        SandboxLanguage::Go => go_forbidden_substring(code),
        SandboxLanguage::Shell => shell_forbidden_substring(code),
        SandboxLanguage::Perl => perl_forbidden_substring(code),
        SandboxLanguage::R => r_forbidden_substring(code),
        _ => Vec::new(),
    }
}

// -- Tests --------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // JavaScript
    #[test]
    fn js_detects_eval() {
        let hits = ast_forbidden_scan(SandboxLanguage::JavaScript, r#"eval("x")"#);
        assert!(!hits.is_empty(), "eval must be detected: {:?}", hits);
    }

    #[test]
    fn js_clean_no_false_positive() {
        let hits = ast_forbidden_scan(SandboxLanguage::JavaScript, "const x = 1 + 2;");
        assert!(hits.is_empty(), "clean JS must not flag: {:?}", hits);
    }

    // TypeScript
    #[test]
    fn ts_detects_spawn() {
        let hits = ast_forbidden_scan(SandboxLanguage::TypeScript, "spawn('cmd', [])");
        assert!(!hits.is_empty(), "spawn must be detected in TS: {:?}", hits);
    }

    #[test]
    fn ts_clean_no_false_positive() {
        let hits = ast_forbidden_scan(SandboxLanguage::TypeScript, "const n: number = 42;");
        assert!(hits.is_empty(), "clean TS must not flag: {:?}", hits);
    }

    // Python
    #[test]
    fn python_detects_subprocess_run() {
        let code = "import subprocess\nsubprocess.run(['ls'])";
        let hits = ast_forbidden_scan(SandboxLanguage::Python, code);
        assert!(
            !hits.is_empty(),
            "subprocess.run must be detected: {:?}",
            hits
        );
    }

    #[test]
    fn python_detects_eval() {
        let hits = ast_forbidden_scan(SandboxLanguage::Python, "eval(input())");
        assert!(!hits.is_empty(), "eval must be detected: {:?}", hits);
    }

    #[test]
    fn python_clean_no_false_positive() {
        let hits = ast_forbidden_scan(SandboxLanguage::Python, "def f(x): return x + 1");
        assert!(hits.is_empty(), "clean Python must not flag: {:?}", hits);
    }

    // Ruby
    #[test]
    fn ruby_detects_system() {
        let hits = ast_forbidden_scan(SandboxLanguage::Ruby, "system('id')");
        assert!(!hits.is_empty(), "system must be detected: {:?}", hits);
    }

    #[test]
    fn ruby_clean_no_false_positive() {
        let hits = ast_forbidden_scan(SandboxLanguage::Ruby, "puts 'hello'");
        assert!(hits.is_empty(), "clean Ruby must not flag: {:?}", hits);
    }

    // Go (substring fallback -- B-FUZZ-002)
    #[test]
    fn go_detects_exec_command() {
        let code = "exec.Command(\"ls\").Run()";
        let hits = ast_forbidden_scan(SandboxLanguage::Go, code);
        assert!(
            !hits.is_empty(),
            "exec.Command must be detected via substring: {:?}",
            hits
        );
    }

    #[test]
    fn go_clean_no_false_positive() {
        let code = "fmt.Println(\"hi\")";
        let hits = ast_forbidden_scan(SandboxLanguage::Go, code);
        assert!(hits.is_empty(), "clean Go must not flag: {:?}", hits);
    }

    // Rust
    #[test]
    fn rust_detects_command_new() {
        let code = "fn main() { Command::new(\"ls\").spawn().ok(); }";
        let hits = ast_forbidden_scan(SandboxLanguage::Rust, code);
        assert!(
            !hits.is_empty(),
            "Command::new must be detected: {:?}",
            hits
        );
    }

    #[test]
    fn rust_clean_no_false_positive() {
        let hits = ast_forbidden_scan(SandboxLanguage::Rust, "fn main() { println!(\"hi\"); }");
        assert!(hits.is_empty(), "clean Rust must not flag: {:?}", hits);
    }

    // PHP
    #[test]
    fn php_detects_system() {
        let hits = ast_forbidden_scan(SandboxLanguage::Php, "<?php system('id'); ?>");
        assert!(
            !hits.is_empty(),
            "system must be detected in PHP: {:?}",
            hits
        );
    }

    #[test]
    fn php_clean_no_false_positive() {
        let hits = ast_forbidden_scan(SandboxLanguage::Php, "<?php echo 'hi'; ?>");
        assert!(hits.is_empty(), "clean PHP must not flag: {:?}", hits);
    }

    // Elixir
    #[test]
    fn elixir_detects_system_cmd() {
        let hits = ast_forbidden_scan(SandboxLanguage::Elixir, "System.cmd(\"ls\", [])");
        assert!(!hits.is_empty(), "System.cmd must be detected: {:?}", hits);
    }

    #[test]
    fn elixir_clean_no_false_positive() {
        let hits = ast_forbidden_scan(SandboxLanguage::Elixir, "IO.puts(\"hi\")");
        assert!(hits.is_empty(), "clean Elixir must not flag: {:?}", hits);
    }

    // Shell (substring fallback -- B-FUZZ-001)
    #[test]
    fn shell_detects_eval() {
        // "eval " (with space) triggers substring match.
        let hits = ast_forbidden_scan(SandboxLanguage::Shell, "eval some_cmd");
        assert!(
            !hits.is_empty(),
            "eval must be detected in shell via substring: {:?}",
            hits
        );
    }

    #[test]
    fn shell_detects_rm_rf() {
        let hits = ast_forbidden_scan(SandboxLanguage::Shell, "rm -rf /tmp/test");
        assert!(
            !hits.is_empty(),
            "rm -rf must be detected in shell: {:?}",
            hits
        );
    }

    #[test]
    fn shell_clean_no_false_positive() {
        let hits = ast_forbidden_scan(SandboxLanguage::Shell, "echo hello");
        assert!(hits.is_empty(), "clean shell must not flag: {:?}", hits);
    }

    // Perl substring fallback
    #[test]
    fn perl_fallback_detects_system() {
        let hits = ast_forbidden_scan(SandboxLanguage::Perl, "system('id');");
        assert!(
            !hits.is_empty(),
            "system must be detected in Perl: {:?}",
            hits
        );
    }

    #[test]
    fn perl_fallback_detects_backtick() {
        let code = "my $out = `id`;";
        let hits = ast_forbidden_scan(SandboxLanguage::Perl, code);
        assert!(
            !hits.is_empty(),
            "backtick must be detected in Perl: {:?}",
            hits
        );
    }

    // R substring fallback
    #[test]
    fn r_fallback_detects_system() {
        let hits = ast_forbidden_scan(SandboxLanguage::R, "result <- system('id', intern=TRUE)");
        assert!(!hits.is_empty(), "system must be detected in R: {:?}", hits);
    }

    #[test]
    fn r_clean_no_false_positive() {
        let hits = ast_forbidden_scan(SandboxLanguage::R, "x <- c(1, 2, 3)\nmean(x)");
        assert!(hits.is_empty(), "clean R must not flag: {:?}", hits);
    }

    // Mapper tests
    #[test]
    fn sandbox_lang_to_polyglot_grammar_backed() {
        assert_eq!(
            sandbox_lang_to_polyglot(SandboxLanguage::JavaScript),
            Some(Lang::JavaScript)
        );
        assert_eq!(
            sandbox_lang_to_polyglot(SandboxLanguage::Elixir),
            Some(Lang::Elixir)
        );
    }

    #[test]
    fn sandbox_lang_to_polyglot_substring_returns_none() {
        assert_eq!(sandbox_lang_to_polyglot(SandboxLanguage::Perl), None);
        assert_eq!(sandbox_lang_to_polyglot(SandboxLanguage::R), None);
        assert_eq!(sandbox_lang_to_polyglot(SandboxLanguage::Go), None);
        assert_eq!(sandbox_lang_to_polyglot(SandboxLanguage::Shell), None);
    }
}
