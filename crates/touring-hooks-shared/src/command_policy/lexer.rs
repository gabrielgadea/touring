//! How a shell line is READ: its pipelines, simple commands and words.
//!
//! The lexical reading of the policy judges words, never raw text, so the
//! reading itself is part of the policy: change how a quote, a comment or a
//! heredoc is read and the same command changes verdict. It lives here, beside
//! the vocabulary it serves, and the lexer unit tests live with it.
//!
//! [`shell_pipelines`] and [`command_of`] are also the reading other hooks need
//! (the VGP execution gate asks what program a command line runs), so both are
//! re-exported from `super`.

/// Whether a command WORD is the option `flag`: `--force` is that word (or
/// `--force=<v>`), and `-f` is a short option cluster holding the letter — `-f`,
/// `-rf`, `-fd` — never a long option, a path, or a word that merely contains
/// the two characters (`touring-foundation`, `my-file.txt`).
pub(super) fn flag_matches(flag: &str, word: &str) -> bool {
    if flag.starts_with("--") {
        return word == flag
            || word
                .strip_prefix(flag)
                .is_some_and(|rest| rest.starts_with('='));
    }
    let Some(letter) = flag.strip_prefix('-') else {
        return word == flag;
    };
    let Some(cluster) = word.strip_prefix('-') else {
        return false;
    };
    !cluster.is_empty()
        && !cluster.starts_with('-')
        && cluster.chars().all(|c| c.is_ascii_alphabetic())
        && cluster.contains(letter)
}

/// The words that can be options: those before a `--`, which ends them
/// (`rm -- -f` removes a file named `-f`).
pub(super) fn option_words(args: &[String]) -> impl Iterator<Item = &str> {
    args.iter()
        .map(String::as_str)
        .take_while(|word| *word != "--")
}

/// The pipelines of a shell line, each as its simple commands, each as its
/// words — read the way a shell reads them, short of expansion: quotes group and
/// are removed, a backslash escapes, `#` at the start of a word opens a comment
/// to the end of the line; `|` ends a command within a pipeline, and `;`, `&`,
/// `&&`, `||` and newline end the pipeline. A flag inside quotes or a comment is
/// text, not a flag. No subshells, no `$(…)` parsing — the structural
/// reading (`super::structural`) runs before this layer.
///
/// A heredoc body (`<<[-]WORD` up to the delimiter line) is the stdin of the
/// command, not commands of the shell: it is left out of the result, unless the
/// text may run it as code — see [`ShellLexer::read_heredoc_bodies`].
pub fn shell_pipelines(command: &str) -> Vec<Vec<Vec<String>>> {
    lex(command, true)
}

/// [`shell_pipelines`], with `heredocs` choosing whether a `<<WORD` body is read
/// as one. Without it every line is a command, as before heredocs were read —
/// how a body that may be code is judged, and never recursively: a body kept
/// as commands is lexed once more, flat.
fn lex(command: &str, heredocs: bool) -> Vec<Vec<Vec<String>>> {
    let mut lexer = ShellLexer {
        heredocs,
        ..ShellLexer::default()
    };
    let mut chars = command.chars().peekable();
    while let Some(c) = chars.next() {
        lexer.feed(c, &mut chars);
    }
    lexer.finish()
}

type Chars<'a> = std::iter::Peekable<std::str::Chars<'a>>;

/// The state of [`shell_pipelines`]: the word, the command and the pipeline being
/// built, each closed into the next level up.
#[derive(Default)]
struct ShellLexer {
    pipelines: Vec<Vec<Vec<String>>>,
    commands: Vec<Vec<String>>,
    words: Vec<String>,
    word: String,
    /// A word is open — even an empty one (`''` is a word).
    in_word: bool,
    /// Heredoc operators of the line being read; their bodies start at its newline.
    pending: Vec<PendingHeredoc>,
    /// Bodies proven data: put back as commands only if [`Self::runs_text`].
    data_bodies: Vec<String>,
    /// Some command of the text is a shell or hands its input to one
    /// ([`may_run_text`]): a body written by one command may be run by another.
    runs_text: bool,
    /// The files the commands write (`> f`, `>> f`, `tee f`) and the programs
    /// they run, to see a script written by one and run by another.
    written: Vec<String>,
    executed: Vec<String>,
    /// Read `<<WORD` as a heredoc (see [`lex`]).
    heredocs: bool,
}

/// A `<<[-]WORD` read on the current line, its body not read yet.
struct PendingHeredoc {
    /// The delimiter word, quotes removed.
    delimiter: String,
    /// `<<-`: tabs at the start of a line are ignored when it is compared with
    /// the delimiter.
    strip_tabs: bool,
    /// The delimiter was quoted somewhere: the body is not expanded.
    quoted: bool,
}

impl ShellLexer {
    fn feed(&mut self, c: char, chars: &mut Chars<'_>) {
        match c {
            '\'' => {
                self.in_word = true;
                // `take_while` consumes the closing quote too.
                self.word.extend(chars.by_ref().take_while(|&q| q != '\''));
            }
            '"' => self.double_quoted(chars),
            '\\' => self.escape(chars),
            '#' if !self.in_word => {
                // A comment runs to the end of the line, which ends the pipeline.
                let _ = chars.by_ref().find(|&q| q == '\n');
                self.end_pipeline();
                self.read_heredoc_bodies(chars);
            }
            '<' | '\n' if self.heredoc_event(c, chars) => {}
            '|' if chars.peek() != Some(&'|') => self.end_command(),
            ';' | '&' | '|' | '\n' => {
                self.end_pipeline();
                if matches!(c, '&' | '|') && chars.peek() == Some(&c) {
                    chars.next();
                }
            }
            c if c.is_whitespace() => self.end_word(),
            _ => {
                self.in_word = true;
                self.word.push(c);
            }
        }
    }

    /// A backslash escapes the next character; before a newline it joins the lines.
    fn escape(&mut self, chars: &mut Chars<'_>) {
        self.in_word = true;
        if let Some(escaped) = chars.next()
            && escaped != '\n'
        {
            self.word.push(escaped);
        }
    }

    /// Inside `"…"`, a backslash still escapes the next character.
    fn double_quoted(&mut self, chars: &mut Chars<'_>) {
        self.in_word = true;
        while let Some(q) = chars.next() {
            match q {
                '"' => break,
                '\\' => self.word.extend(chars.next()),
                _ => self.word.push(q),
            }
        }
    }

    fn end_word(&mut self) {
        if std::mem::take(&mut self.in_word) {
            self.words.push(std::mem::take(&mut self.word));
        }
    }

    fn end_command(&mut self) {
        self.end_word();
        if !self.words.is_empty() {
            self.runs_text |= may_run_text(&self.words);
            self.written.extend(written_paths(&self.words));
            self.executed
                .extend(command_of(&self.words).map(|(program, _)| plain_path(program)));
            self.commands.push(std::mem::take(&mut self.words));
        }
    }

    fn end_pipeline(&mut self) {
        self.end_command();
        if !self.commands.is_empty() {
            self.pipelines.push(std::mem::take(&mut self.commands));
        }
    }

    fn finish(mut self) -> Vec<Vec<Vec<String>>> {
        self.end_pipeline();
        if self.runs_text || self.runs_written_file() {
            for body in std::mem::take(&mut self.data_bodies) {
                self.pipelines.extend(lex(&body, false));
            }
        }
        self.pipelines
    }

    /// A command of the text runs a file another one writes: the body of
    /// `cat > run.sh <<EOF` is a script when `./run.sh` follows.
    fn runs_written_file(&self) -> bool {
        self.written.iter().any(|path| self.executed.contains(path))
    }

    /// The two places where a heredoc touches the line: its operator, and the
    /// newline after which its body starts. `false` when `c` is neither.
    ///
    /// Only an operator that starts a word counts; any other `<<` (`1<<2`) stays
    /// in its word and what follows it stays commands.
    fn heredoc_event(&mut self, c: char, chars: &mut Chars<'_>) -> bool {
        if !self.heredocs {
            return false;
        }
        match c {
            '<' if !self.in_word && chars.peek() == Some(&'<') => self.heredoc_operator(chars),
            '\n' if !self.pending.is_empty() => {
                self.end_pipeline();
                self.read_heredoc_bodies(chars);
            }
            _ => return false,
        }
        true
    }

    /// `<<[-]WORD`, the first `<` already read: queue the heredoc, whose body
    /// starts at the next newline. `<<` with no delimiter is not a heredoc and
    /// stays a word, as before — which is also how `<<<` (a here-string) falls
    /// out: the delimiter reader stops at the third `<`.
    fn heredoc_operator(&mut self, chars: &mut Chars<'_>) {
        chars.next(); // the second `<`
        let strip_tabs = chars.next_if_eq(&'-').is_some();
        while chars.next_if(|c| matches!(c, ' ' | '\t')).is_some() {}
        let (delimiter, quoted) = read_delimiter(chars);
        if delimiter.is_empty() {
            self.in_word = true;
            self.word.push_str(if strip_tabs { "<<-" } else { "<<" });
        } else {
            self.pending.push(PendingHeredoc {
                delimiter,
                strip_tabs,
                quoted,
            });
        }
    }

    /// The line with heredoc operators is over: read their bodies, in order.
    ///
    /// A body is the stdin of its command, so DATA — unless the shell would run
    /// it, and what cannot be proven data stays commands (the validator only
    /// ever refuses more, never less, than it did before heredocs were read):
    /// - no delimiter line: the shell reads to the end of the text;
    /// - an unquoted delimiter expands the body, and `$(…)` or a backtick in it
    ///   runs;
    /// - some command of the text is a shell ([`may_run_text`]): decided at
    ///   [`Self::finish`], since `cat > run.sh <<EOF` is followed by `bash run.sh`.
    fn read_heredoc_bodies(&mut self, chars: &mut Chars<'_>) {
        for heredoc in std::mem::take(&mut self.pending) {
            let (body, closed) = heredoc_body(chars, &heredoc);
            if !closed || (!heredoc.quoted && (body.contains("$(") || body.contains('`'))) {
                self.pipelines.extend(lex(&body, false));
            } else {
                self.data_bodies.push(body);
            }
        }
    }
}

/// The delimiter word of a heredoc, quotes removed as the shell does, and
/// whether any quoting was there.
fn read_delimiter(chars: &mut Chars<'_>) -> (String, bool) {
    let mut word = String::new();
    let mut quoted = false;
    while let Some(c) = chars.next_if(|c| !c.is_whitespace() && !";&|<>)".contains(*c)) {
        if matches!(c, '\'' | '"' | '\\') {
            quoted = true;
            unquote_into(c, chars, &mut word);
        } else {
            word.push(c);
        }
    }
    (word, quoted)
}

/// The text a quote (or backslash) `open` protects, quotes removed.
fn unquote_into(open: char, chars: &mut Chars<'_>, word: &mut String) {
    match open {
        '\'' => word.extend(chars.by_ref().take_while(|&q| q != '\'')),
        '\\' => word.extend(chars.next()),
        _ => {
            while let Some(q) = chars.next() {
                match q {
                    '"' => break,
                    // Inside `"…"` a backslash escapes only these four.
                    '\\' => word.push(
                        chars
                            .next_if(|e| matches!(e, '$' | '`' | '"' | '\\'))
                            .unwrap_or('\\'),
                    ),
                    _ => word.push(q),
                }
            }
        }
    }
}

/// The lines of a heredoc body up to its delimiter line (consumed, not part of
/// the body), and whether that line was there.
fn heredoc_body(chars: &mut Chars<'_>, heredoc: &PendingHeredoc) -> (String, bool) {
    let mut body = String::new();
    loop {
        let mut line = String::new();
        let mut newline = false;
        for c in chars.by_ref() {
            if c == '\n' {
                newline = true;
                break;
            }
            line.push(c);
        }
        let compared = if heredoc.strip_tabs {
            line.trim_start_matches('\t')
        } else {
            &line
        };
        if compared == heredoc.delimiter {
            return (body, true);
        }
        body.push_str(&line);
        if !newline {
            return (body, false);
        }
        body.push('\n');
    }
}

/// `./run.sh` and `run.sh` are the same file.
fn plain_path(path: &str) -> String {
    path.strip_prefix("./").unwrap_or(path).to_string()
}

/// The files a command writes: the target of `>` and `>>`, spaced or glued to
/// the operator, and the operands of `tee`. (`>&2` yields `&2`, which no
/// command is called.)
fn written_paths(words: &[String]) -> Vec<String> {
    let mut paths = Vec::new();
    for (i, word) in words.iter().enumerate() {
        match word.strip_prefix(">>").or_else(|| word.strip_prefix('>')) {
            Some("") => paths.extend(words.get(i + 1).map(|path| plain_path(path))),
            Some(path) => paths.push(plain_path(path)),
            None => {}
        }
    }
    if let Some(("tee", operands)) = command_of(words) {
        let files = operands.iter().filter(|operand| !operand.starts_with('-'));
        paths.extend(files.map(|file| plain_path(file)));
    }
    paths
}

/// What takes text as shell code: the shells, what runs on a shell elsewhere
/// (`ssh`, `su`, `sudo`, `doas`) and what reads a script (`eval`, `source`,
/// `at`, `batch`) — and `.`, matched as a whole word.
const TEXT_RUNNERS: &[&str] = &[
    "bash", "sh", "zsh", "dash", "ksh", "ash", "fish", "mksh", "rbash", "csh", "tcsh", "eval",
    "source", "ssh", "su", "sudo", "doas", "at", "batch",
];

/// `true` when a command with these words may run text as shell code — in any
/// word, since `docker exec -i app bash`, `y=$(bash` and `/bin/sh` do.
fn may_run_text(words: &[String]) -> bool {
    words.iter().any(|word| {
        word == "."
            || word
                .split(|c: char| c.is_whitespace() || "/()`".contains(c))
                .any(|part| TEXT_RUNNERS.contains(&part))
    })
}

/// Wrappers that only change HOW a command runs, each with its options that take
/// a SEPARATE value: in `sudo -u root git push -f`, `root` is the value of `-u`,
/// not the command (read as the command, the push went unjudged).
const WRAPPERS: &[(&str, &[&str])] = &[
    (
        "sudo",
        &["-u", "-g", "-C", "-D", "-p", "-r", "-t", "-U", "-T"],
    ),
    ("env", &["-u", "-C"]),
    ("timeout", &["-s", "-k"]),
    ("time", &["-f", "-o"]),
    ("exec", &["-a"]),
    ("nohup", &[]),
    ("command", &[]),
];

/// The command word of a simple command and its arguments, past what only
/// changes HOW it runs: leading `NAME=value` assignments and the [`WRAPPERS`]
/// (with their own options, and `timeout`'s duration).
pub fn command_of(words: &[String]) -> Option<(&str, &[String])> {
    let mut i = 0;
    while let Some(word) = words.get(i).map(String::as_str) {
        if is_assignment(word) {
            i += 1;
        } else if let Some((_, takes_value)) = WRAPPERS.iter().find(|(name, _)| *name == word) {
            i = past_wrapper_options(words, i + 1, word, takes_value);
        } else {
            return Some((word, words.get(i + 1..).unwrap_or_default()));
        }
    }
    None
}

/// `NAME=value`: a shell variable assignment, not a command.
fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

/// The index of the first word after `wrapper`'s own options, starting at `i`.
fn past_wrapper_options(
    words: &[String],
    mut i: usize,
    wrapper: &str,
    takes_value: &[&str],
) -> usize {
    while let Some(next) = words.get(i) {
        let is_duration = wrapper == "timeout" && next.starts_with(|c: char| c.is_ascii_digit());
        if takes_value.contains(&next.as_str()) {
            i += 2;
        } else if next.starts_with('-') || is_duration {
            i += 1;
        } else {
            break;
        }
    }
    i
}

#[cfg(test)]
mod tests;
