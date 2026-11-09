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
    lexer.read(command);
    lexer.finish()
}

/// A shell line read for a judge that knows which commands take a heredoc as
/// INPUT to a program they already have (`bash -c CODE <<EOF`, `bash s.sh <<EOF`):
/// the [`shell_pipelines`] of the line, except that such a body is dropped instead
/// of possibly read as commands, and every heredoc body of the line, in order (a
/// shell reading its stdin runs them). Same reader as [`shell_pipelines`] —
/// quotes, here-strings (`<<<`) and `<<-` are read once, here.
pub(super) fn read_with_script_input(
    command: &str,
    script_input: fn(&[String]) -> bool,
) -> (Vec<Vec<Vec<String>>>, Vec<String>) {
    let mut lexer = ShellLexer {
        heredocs: true,
        script_input: Some(script_input),
        ..ShellLexer::default()
    };
    lexer.read(command);
    let bodies = std::mem::take(&mut lexer.bodies);
    (lexer.finish(), bodies)
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
    /// The code `env -S CODE` hands to a fresh split ([`split_string_code`]):
    /// read as commands of the line once the line is over.
    split_codes: Vec<String>,
    /// Read `<<WORD` as a heredoc (see [`lex`]).
    heredocs: bool,
    /// Commands whose heredoc is input to a program they already have: such a
    /// body is never read as commands (see [`read_with_script_input`]).
    script_input: Option<fn(&[String]) -> bool>,
    /// Every heredoc body read, in order.
    bodies: Vec<String>,
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
    /// The words of the command the operator belongs to, set when that command
    /// ends (before the newline that starts the body).
    owner: Option<Vec<String>>,
}

impl ShellLexer {
    fn read(&mut self, command: &str) {
        let mut chars = command.chars().peekable();
        while let Some(c) = chars.next() {
            self.feed(c, &mut chars);
        }
    }

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
            for heredoc in self.pending.iter_mut().filter(|h| h.owner.is_none()) {
                heredoc.owner = Some(self.words.clone());
            }
            self.runs_text |= may_run_text(&self.words);
            self.written.extend(written_paths(&self.words));
            self.executed
                .extend(command_of(&self.words).map(|(program, _)| plain_path(program)));
            self.split_codes.extend(split_string_code(&self.words));
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
        for code in std::mem::take(&mut self.split_codes) {
            self.pipelines.extend(lex(&code, self.heredocs));
        }
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
                owner: None,
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
            if self.script_input.is_some() {
                self.bodies.push(body.clone());
            }
            let is_script_input = self
                .script_input
                .zip(heredoc.owner.as_deref())
                .is_some_and(|(script_input, owner)| script_input(owner));
            if !closed || (!heredoc.quoted && (body.contains("$(") || body.contains('`'))) {
                self.pipelines.extend(lex(&body, false));
            } else if !is_script_input {
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

/// The shells, by program basename — the one list every reader of "is this a shell?" asks
/// ([`is_shell`]). It used to be spelled three times (here, `git/nested.rs`, `git/text.rs`),
/// so a shell added to one reading would have been missed by the others.
const SHELLS: &[&str] = &[
    "bash", "sh", "zsh", "dash", "ksh", "ash", "fish", "mksh", "rbash", "csh", "tcsh",
];

/// `true` when `name` (a program basename) is a shell.
pub(super) fn is_shell(name: &str) -> bool {
    SHELLS.contains(&name)
}

/// What else takes text as shell code besides the shells: what runs on a shell elsewhere
/// (`ssh`, `su`, `sudo`, `doas`) and what reads a script (`eval`, `source`, `at`,
/// `batch`) — and `.`, matched as a whole word in [`may_run_text`].
const OTHER_TEXT_RUNNERS: &[&str] = &["eval", "source", "ssh", "su", "sudo", "doas", "at", "batch"];

/// `true` when a command with these words may run text as shell code — in any
/// word, since `docker exec -i app bash`, `y=$(bash` and `/bin/sh` do.
fn may_run_text(words: &[String]) -> bool {
    words.iter().any(|word| {
        word == "."
            || word
                .split(|c: char| c.is_whitespace() || "/()`".contains(c))
                .any(|part| is_shell(part) || OTHER_TEXT_RUNNERS.contains(&part))
    })
}

/// A program whose job is to run the program named by its arguments, changing
/// only HOW it runs (see "Wrapper" in `GLOSSARY.md`).
pub(super) struct Wrapper {
    /// The program basename.
    pub(super) name: &'static str,
    /// Options that take a SEPARATE value: in `sudo -u root git push -f`, `root`
    /// is the value of `-u`, not the command (read as the command, the push went
    /// unjudged).
    pub(super) value_options: &'static [&'static str],
    /// Operands before the program, whatever their spelling: `timeout`'s
    /// duration (`5`, `.5`, `1.5m`), `chrt`'s priority, `taskset`'s mask.
    pub(super) positionals: usize,
}

/// The ONE table of wrappers. It is the union of what every reader of a shell
/// line used to know on its own (six tables that disagreed: `nice chmod 000 f`
/// was refused by one gate and let through by another). Reading through more
/// wrappers only ever reveals the program, never hides it.
const WRAPPERS: &[Wrapper] = &[
    Wrapper {
        name: "sudo",
        value_options: &[
            "-u",
            "-g",
            "-C",
            "-D",
            "-h",
            "-p",
            "-r",
            "-t",
            "-T",
            "-U",
            "--user",
            "--group",
            "--chdir",
            "--host",
            "--prompt",
            "--role",
            "--type",
            "--close-from",
            "--command-timeout",
            "--other-user",
        ],
        positionals: 0,
    },
    Wrapper {
        name: "doas",
        value_options: &["-u", "-C"],
        positionals: 0,
    },
    Wrapper {
        name: "env",
        value_options: &[
            "-u",
            "--unset",
            "-C",
            "--chdir",
            "-a",
            "--argv0",
            // `env -S CODE` runs CODE through a fresh split: the lexer reads it
            // as commands of the line (`split_string_code`), so here it is
            // consumed as the value it is.
            "-S",
            "--split-string",
        ],
        positionals: 0,
    },
    Wrapper {
        name: "timeout",
        value_options: &["-s", "--signal", "-k", "--kill-after"],
        positionals: 1,
    },
    Wrapper {
        name: "nice",
        value_options: &["-n", "--adjustment"],
        positionals: 0,
    },
    Wrapper {
        name: "ionice",
        value_options: &["-c", "-n", "-p", "-P", "-u", "--class", "--classdata"],
        positionals: 0,
    },
    Wrapper {
        name: "chrt",
        value_options: &["-T", "-P", "-D"],
        positionals: 1,
    },
    Wrapper {
        name: "taskset",
        value_options: &[],
        positionals: 1,
    },
    Wrapper {
        name: "stdbuf",
        value_options: &["-i", "-o", "-e"],
        positionals: 0,
    },
    Wrapper {
        name: "time",
        value_options: &["-f", "-o", "--format", "--output"],
        positionals: 0,
    },
    Wrapper {
        name: "exec",
        value_options: &["-a"],
        positionals: 0,
    },
    Wrapper {
        name: "nohup",
        value_options: &[],
        positionals: 0,
    },
    Wrapper {
        name: "setsid",
        value_options: &[],
        positionals: 0,
    },
    Wrapper {
        name: "command",
        value_options: &[],
        positionals: 0,
    },
    Wrapper {
        name: "builtin",
        value_options: &[],
        positionals: 0,
    },
    Wrapper {
        name: "busybox",
        value_options: &[],
        positionals: 0,
    },
    Wrapper {
        name: "hw-slot",
        value_options: &[
            "--class",
            "--wait",
            "--max-temp",
            "--min-mem-gb",
            "--min-vram-gb",
            "--abort-temp",
            "--label",
        ],
        positionals: 0,
    },
    Wrapper {
        name: "xargs",
        value_options: &[
            "-a",
            "--arg-file",
            "-d",
            "--delimiter",
            "-E",
            "-I",
            "-L",
            "-n",
            "--max-args",
            "--max-lines",
            "-P",
            "--max-procs",
            "-s",
            "--max-chars",
            "--process-slot-var",
        ],
        positionals: 0,
    },
];

/// The wrapper `word` names, by basename (`/usr/bin/sudo` is `sudo`).
pub(super) fn wrapper(word: &str) -> Option<&'static Wrapper> {
    let name = basename(word);
    WRAPPERS.iter().find(|wrapper| wrapper.name == name)
}

/// The last path component of a program word.
pub(super) fn basename(word: &str) -> &str {
    word.rsplit('/').next().unwrap_or(word)
}

/// The EFFECTIVE PROGRAM of a simple command (see `GLOSSARY.md`) and its
/// arguments: the word past leading assignments (`A=1`, `A+=1`) and past every
/// [`Wrapper`] with its options, positional operands and `--`. A wrapper that
/// runs nothing (`command -v rm`) is itself the program. `None` when only
/// assignments and wrappers are there.
///
/// This is the one answer to "which program does this command run" — every
/// gate that judges or classifies a command asks it, never a table of its own.
pub fn command_of(words: &[String]) -> Option<(&str, &[String])> {
    let mut i = 0;
    while let Some(word) = words.get(i).map(String::as_str) {
        if is_assignment(word) {
            i += 1;
        } else if let Some(wrapper) = wrapper(word) {
            match past_wrapper_operands(words, i + 1, wrapper) {
                Some(next) => i = next,
                None => return Some((word, words.get(i + 1..).unwrap_or_default())),
            }
        } else {
            return Some((word, words.get(i + 1..).unwrap_or_default()));
        }
    }
    None
}

/// `NAME=value` or `NAME+=value`: a shell variable assignment, not a command.
/// The shell runs `A+=1 chmod 000 f` as `chmod`, so `+=` is an assignment too.
pub fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        let name = name.strip_suffix('+').unwrap_or(name);
        name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

/// The index of the first word after `wrapper`'s own options and operands,
/// starting at `i`; `None` when the wrapper runs no program (`command -v`).
fn past_wrapper_operands(words: &[String], mut i: usize, wrapper: &Wrapper) -> Option<usize> {
    while let Some(word) = words.get(i) {
        match option_width(wrapper, word)? {
            0 => break,
            width => i += width,
        }
    }
    if words.get(i).is_some_and(|word| word == "--") {
        i += 1;
    }
    Some(i + wrapper.positionals)
}

/// How many words the option `word` of `wrapper` spans: 2 for an option with a
/// separate value, 1 for any other option (or `env`'s `NAME=value`), 0 when
/// `word` is no option (`--` included: it ends them). `None` when the option
/// means the wrapper runs nothing (`command -v`).
fn option_width(wrapper: &Wrapper, word: &str) -> Option<usize> {
    if wrapper.name == "command" && matches!(word, "-v" | "-V") {
        return None;
    }
    let width = if word == "--" {
        0
    } else if takes_separate_value(wrapper, word) {
        2
    } else if (word.len() > 1 && word.starts_with('-'))
        || (wrapper.name == "env" && is_assignment(word))
    {
        1
    } else {
        0
    };
    Some(width)
}

/// The code `env -S CODE` (`--split-string`, `--split-string=CODE`, `-SCODE`)
/// hands to a fresh split, when this simple command is one: the shell then
/// runs CODE as a command line of its own.
fn split_string_code(words: &[String]) -> Option<String> {
    let env_at = words.iter().position(|word| basename(word) == "env")?;
    let args = &words[env_at + 1..];
    args.iter()
        .enumerate()
        .find_map(|(i, arg)| split_string_option(arg, args.get(i + 1)))
        .map(|(code, _)| code.to_owned())
}

/// `env`'s split-string option in `arg`, in every spelling, with the code it carries
/// and how many words it spans: `--split-string CODE` and `-S CODE` (2), the glued
/// `--split-string=CODE` and `-SCODE` (1), and `S` anywhere in a short-option
/// cluster — `-iS CODE`, `-iSCODE` — which is the same option (read only at the
/// start of the cluster, `env -iS 'rm -rf x'` was one "program" named `rm -rf x`).
/// The one reader of this option, for the lexer and for the git judge.
pub(super) fn split_string_option<'a>(
    arg: &'a str,
    next: Option<&'a String>,
) -> Option<(&'a str, usize)> {
    if arg == "--split-string" {
        return next.map(|code| (code.as_str(), 2));
    }
    if let Some(code) = arg.strip_prefix("--split-string=") {
        return Some((code, 1));
    }
    if !is_split_cluster(arg) {
        return None;
    }
    let after = &arg[arg.find('S')? + 1..];
    if after.is_empty() {
        next.map(|code| (code.as_str(), 2))
    } else {
        Some((after, 1))
    }
}

/// The option `word` of `wrapper` takes the NEXT word as its value: a listed value
/// option, or an `env` cluster ending in `S` (`-iS CODE`).
fn takes_separate_value(wrapper: &Wrapper, word: &str) -> bool {
    wrapper.value_options.contains(&word)
        || (wrapper.name == "env" && is_split_cluster(word) && word.ends_with('S'))
}

/// A single-dash option cluster holding `S` (`-S`, `-iS`, `-iSCODE`).
fn is_split_cluster(arg: &str) -> bool {
    arg.strip_prefix('-')
        .is_some_and(|cluster| !cluster.starts_with('-') && cluster.contains('S'))
}

#[cfg(test)]
mod tests;
