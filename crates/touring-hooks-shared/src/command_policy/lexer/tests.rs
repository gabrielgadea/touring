// touring-quality:allow-attack-fixture — destructive command strings are lexer INPUT here, never executed
//! Unit tests of the shell lexer: how a line is split into pipelines, commands and words.

use super::*;

#[test]
fn the_lexer_reads_quotes_escapes_comments_and_separators() {
    let lines = shell_pipelines("a 'b c' \"d\\\"e\" f\\ g # h i\nj && k || l; m | n");
    assert_eq!(
        lines,
        vec![
            vec![vec!["a", "b c", "d\"e", "f g"]],
            vec![vec!["j"]],
            vec![vec!["k"]],
            vec![vec!["l"]],
            vec![vec!["m"], vec!["n"]],
        ]
    );
}

#[test]
fn heredoc_body_words_do_not_reach_the_lexer_output() {
    assert_eq!(
        shell_pipelines("python3 - <<'EOF'\ndel x\nEOF\nls"),
        vec![vec![vec!["python3", "-"]], vec![vec!["ls"]]]
    );
    // `<<` with nothing to delimit, and `<<<`, stay words.
    assert_eq!(
        shell_pipelines("cat <<-\nls"),
        vec![vec![vec!["cat", "<<-"]], vec![vec!["ls"]]]
    );
    assert_eq!(
        shell_pipelines("cat <<\nls"),
        vec![vec![vec!["cat", "<<"]], vec![vec!["ls"]]]
    );
    assert_eq!(
        shell_pipelines("bash <<< 'rm -rf /'"),
        vec![vec![vec!["bash", "<<<", "rm -rf /"]]]
    );
    // A body kept as commands comes back where the shell would run it…
    assert_eq!(
        shell_pipelines("cat <<EOF\n$(date)\ndel x\nEOF\nls"),
        vec![
            vec![vec!["cat"]],
            vec![vec!["$(date)"]],
            vec![vec!["del", "x"]],
            vec![vec!["ls"]],
        ]
    );
    // …or after the commands of the text, when only the end tells it is code.
    assert_eq!(
        shell_pipelines("bash <<'EOF'\ndel x\nEOF\nls"),
        vec![vec![vec!["bash"]], vec![vec!["ls"]], vec![vec!["del", "x"]]]
    );
}

#[test]
fn heredoc_the_programs_that_run_text() {
    // Spelled out, not read from `TEXT_RUNNERS`: a name dropped from the
    // list must fail here.
    for name in [
        "bash", "sh", "zsh", "dash", "ksh", "ash", "fish", "mksh", "rbash", "csh", "tcsh", "eval",
        "source", "ssh", "su", "sudo", "doas", "at", "batch",
    ] {
        assert!(may_run_text(&[name.to_string()]), "{name}");
    }
    for word in [
        ".",
        "bin/bash",
        "$(bash",
        "(zsh",
        "echo bash)",
        "`sh",
        "echo hi; bash",
    ] {
        assert!(may_run_text(&[word.to_string()]), "{word}");
    }
    for word in [
        "python3",
        "cat",
        "-",
        "./run.sh",
        "../x",
        "ssh-keygen",
        "bash-completion.sh",
        "--batch",
    ] {
        assert!(!may_run_text(&[word.to_string()]), "{word}");
    }
}

#[test]
fn heredoc_delimiter_words() {
    let read = |text: &str| {
        let mut chars = text.chars().peekable();
        let word = read_delimiter(&mut chars);
        (word, chars.collect::<String>())
    };
    let word = |text: &str, quoted: bool| (text.to_string(), quoted);
    // Quotes and backslashes are removed, and protect what they enclose.
    assert_eq!(read("EOF"), (word("EOF", false), String::new()));
    assert_eq!(read("'E F'"), (word("E F", true), String::new()));
    assert_eq!(read("E\\ F"), (word("E F", true), String::new()));
    assert_eq!(read("\"E F\""), (word("E F", true), String::new()));
    assert_eq!(read("\"EF\" tail"), (word("EF", true), " tail".to_string()));
    assert_eq!(read("E'O'\"F\""), (word("EOF", true), String::new()));
    // Inside `"…"` a backslash escapes `$`, a backtick, `"` and `\` — and is
    // itself before anything else.
    assert_eq!(read("\"E\\\"F\""), (word("E\"F", true), String::new()));
    assert_eq!(read("\"E\\$F\""), (word("E$F", true), String::new()));
    assert_eq!(read("\"E\\`F\""), (word("E`F", true), String::new()));
    assert_eq!(read("\"E\\\\F\""), (word("E\\F", true), String::new()));
    assert_eq!(read("\"E\\xF\""), (word("E\\xF", true), String::new()));
    // Unquoted, the word ends at the shell's metacharacters and blanks.
    for stop in [";", "&", "|", "<", ">", ")", " ", "\t", "\n"] {
        let (found, rest) = read(&format!("EOF{stop}rest"));
        assert_eq!(found, word("EOF", false), "{stop:?}");
        assert_eq!(rest, format!("{stop}rest"), "{stop:?}");
    }
}

#[test]
fn heredoc_files_a_command_writes() {
    let words = |text: &str| -> Vec<String> { text.split(' ').map(str::to_string).collect() };
    assert_eq!(written_paths(&words("cat > ./a.sh")), ["a.sh"]);
    assert_eq!(written_paths(&words("cat >> a.sh")), ["a.sh"]);
    assert_eq!(written_paths(&words("cat >a.sh")), ["a.sh"]);
    assert_eq!(written_paths(&words("tee -a a.sh b.sh")), ["a.sh", "b.sh"]);
    assert_eq!(written_paths(&words("sudo tee x")), ["x"]);
    assert!(written_paths(&words("cat a.sh")).is_empty());
    assert!(written_paths(&words("echo >")).is_empty());
}
