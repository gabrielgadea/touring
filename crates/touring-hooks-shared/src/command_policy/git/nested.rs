//! Shells, launchers and the programs they hand to another execution layer.

use super::super::lexer::is_shell;
use super::direct::scan_git_invocation;
use super::interpreter::{Language, exec_scan, interpreter_invocation, language};
use super::text::substitutions;
use super::{ScanCtx, TOKEN, scan_program};

pub(super) fn scan_command(words: &[String], ctx: ScanCtx<'_>) -> Result<(), String> {
    let (ritual_unlock, rest) = leading_ritual_unlock(words, ctx.ritual_unlock);
    if rest.is_empty() {
        return Ok(());
    }
    for substitution in substitutions(&rest.join(" ")) {
        scan_program(&substitution, ctx.depth + 1, ritual_unlock)?;
    }
    let ctx = ScanCtx {
        ritual_unlock,
        ..ctx
    };
    let name = basename(&rest[0]);
    let args = &rest[1..];
    if is_wrapper(name) {
        scan_wrapper(name, args, ctx)
    } else {
        scan_named(name, args, ctx)
    }
}

fn scan_wrapper(name: &str, args: &[String], ctx: ScanCtx<'_>) -> Result<(), String> {
    let mut ritual_unlock = ctx.ritual_unlock;
    let mut sudo_shell = false;
    let mut i = 0;
    while i < args.len() {
        match wrapper_action(name, args, i, ctx, &mut ritual_unlock, &mut sudo_shell)? {
            WrapperAction::Next(width) => i += width,
            WrapperAction::Stop => break,
            WrapperAction::Done => return Ok(()),
        }
    }
    scan_wrapper_rest(
        name,
        &args[i.min(args.len())..],
        ctx,
        ritual_unlock,
        sudo_shell,
    )
}

enum WrapperAction {
    Next(usize),
    Stop,
    Done,
}

fn wrapper_action(
    name: &str,
    args: &[String],
    i: usize,
    ctx: ScanCtx<'_>,
    ritual_unlock: &mut bool,
    sudo_shell: &mut bool,
) -> Result<WrapperAction, String> {
    let (value_options, positional) = wrapper_spec(name);
    let arg = args[i].as_str();
    if arg == "--" {
        return Ok(WrapperAction::Next(1));
    }
    if let Some(action) = wrapper_special_action(name, args, i, ctx, ritual_unlock, sudo_shell)? {
        return Ok(action);
    }
    if value_options.contains(&arg) {
        return Ok(WrapperAction::Next(2));
    }
    if arg.starts_with('-') {
        return Ok(WrapperAction::Next(1));
    }
    if positional > 0 && arg.starts_with(|c: char| c.is_ascii_digit()) {
        return Ok(WrapperAction::Next(1));
    }
    Ok(WrapperAction::Stop)
}

fn wrapper_special_action(
    name: &str,
    args: &[String],
    i: usize,
    ctx: ScanCtx<'_>,
    ritual_unlock: &mut bool,
    sudo_shell: &mut bool,
) -> Result<Option<WrapperAction>, String> {
    let arg = args[i].as_str();
    if name == "command" && matches!(arg, "-v" | "-V") {
        return Ok(Some(WrapperAction::Done));
    }
    if name == "env" && matches!(arg, "-S" | "--split-string") {
        if let Some(code) = args.get(i + 1) {
            scan_program(code, ctx.depth + 1, *ritual_unlock)?;
        }
        return Ok(Some(WrapperAction::Done));
    }
    if name == "env" && is_assignment(arg) {
        *ritual_unlock |= arg == TOKEN;
        return Ok(Some(WrapperAction::Next(1)));
    }
    if name == "sudo" && matches!(arg, "-s" | "-i" | "--shell" | "--login") {
        *sudo_shell = true;
        return Ok(Some(WrapperAction::Next(1)));
    }
    Ok(None)
}

fn scan_wrapper_rest(
    name: &str,
    rest: &[String],
    ctx: ScanCtx<'_>,
    ritual_unlock: bool,
    sudo_shell: bool,
) -> Result<(), String> {
    if sudo_shell {
        if !rest.is_empty() {
            scan_program(&rest.join(" "), ctx.depth + 1, ritual_unlock)?;
        }
        return Ok(());
    }
    if rest.is_empty() {
        return Ok(());
    }
    scan_command(
        rest,
        ScanCtx {
            ritual_unlock,
            via_xargs: ctx.via_xargs || name == "xargs",
            ..ctx
        },
    )
}

fn scan_named(name: &str, args: &[String], ctx: ScanCtx<'_>) -> Result<(), String> {
    if name == "git" {
        return scan_git_invocation(args, ctx.via_xargs, ctx.ritual_unlock);
    }
    if is_shell(name) {
        return scan_shell(args, ctx);
    }
    if name == "eval" {
        if !args.is_empty() {
            scan_program(&args.join(" "), ctx.depth + 1, ctx.ritual_unlock)?;
        }
        return Ok(());
    }
    if matches!(name, "su" | "flock" | "script") {
        return scan_string_runner(args, ctx);
    }
    if name == "find" {
        return scan_find(args, ctx);
    }
    if name == "touring" {
        return scan_touring(args, ctx);
    }
    if name == "watch" {
        return scan_watch(args, ctx);
    }
    if let Some(language) = language(name) {
        return scan_interpreter(language, args, ctx);
    }
    Ok(())
}

fn scan_shell(args: &[String], ctx: ScanCtx<'_>) -> Result<(), String> {
    let (payload, reads_stdin) = shell_invocation(args);
    if let Some(code) = payload {
        scan_program(&code, ctx.depth + 1, ctx.ritual_unlock)?;
    } else if reads_stdin {
        scan_stdin(args, ctx, None)?;
    }
    Ok(())
}

fn scan_string_runner(args: &[String], ctx: ScanCtx<'_>) -> Result<(), String> {
    for (i, arg) in args.iter().enumerate() {
        if matches!(arg.as_str(), "-c" | "--command") {
            if let Some(code) = args.get(i + 1) {
                scan_program(code, ctx.depth + 1, ctx.ritual_unlock)?;
            }
        } else if let Some(code) = arg.strip_prefix("--command=") {
            scan_program(code, ctx.depth + 1, ctx.ritual_unlock)?;
        }
    }
    Ok(())
}

fn scan_find(args: &[String], ctx: ScanCtx<'_>) -> Result<(), String> {
    let mut i = 0;
    while i < args.len() {
        if matches!(args[i].as_str(), "-exec" | "-execdir" | "-ok" | "-okdir") {
            let start = i + 1;
            let mut end = start;
            while end < args.len() && !matches!(args[end].as_str(), ";" | "+") {
                end += 1;
            }
            if start < end {
                scan_command(
                    &args[start..end],
                    ScanCtx {
                        previous: None,
                        heredocs: &[],
                        ..ctx
                    },
                )?;
            }
            i = end;
        }
        i += 1;
    }
    Ok(())
}

fn scan_touring(args: &[String], ctx: ScanCtx<'_>) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("exec") => scan_touring_exec(&args[1..], ctx),
        Some("run") => scan_touring_run(&args[1..], ctx),
        _ => Ok(()),
    }
}

fn scan_touring_exec(args: &[String], ctx: ScanCtx<'_>) -> Result<(), String> {
    let program: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .filter(|arg| !arg.starts_with('-'))
        .collect();
    if !program.is_empty() {
        scan_program(&program.join(" "), ctx.depth + 1, ctx.ritual_unlock)?;
    }
    Ok(())
}

fn scan_touring_run(args: &[String], ctx: ScanCtx<'_>) -> Result<(), String> {
    let lang = option_value(args, "--lang");
    let Some(code) = option_value(args, "--code") else {
        return Ok(());
    };
    match lang.as_deref() {
        None => {
            scan_program(&code, ctx.depth + 1, ctx.ritual_unlock)?;
            exec_scan(Language::Python, &code, ctx.depth, ctx.ritual_unlock)?;
        }
        Some("bash" | "sh" | "shell" | "zsh") => {
            scan_program(&code, ctx.depth + 1, ctx.ritual_unlock)?;
        }
        Some(other) => {
            if let Some(language) = language(other) {
                exec_scan(language, &code, ctx.depth, ctx.ritual_unlock)?;
            }
        }
    }
    Ok(())
}

fn scan_watch(args: &[String], ctx: ScanCtx<'_>) -> Result<(), String> {
    let program: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .filter(|arg| !arg.starts_with('-'))
        .collect();
    if !program.is_empty() {
        scan_program(&program.join(" "), ctx.depth + 1, ctx.ritual_unlock)?;
    }
    Ok(())
}

fn scan_interpreter(language: Language, args: &[String], ctx: ScanCtx<'_>) -> Result<(), String> {
    let (codes, reads_stdin) = interpreter_invocation(language, args);
    let had_codes = !codes.is_empty();
    for code in codes {
        exec_scan(language, &code, ctx.depth, ctx.ritual_unlock)?;
    }
    if reads_stdin && !had_codes {
        scan_stdin(args, ctx, Some(language))?;
    }
    Ok(())
}

fn scan_stdin(args: &[String], ctx: ScanCtx<'_>, language: Option<Language>) -> Result<(), String> {
    scan_heredoc_stdin(args, ctx, language)?;
    for (i, arg) in args.iter().enumerate() {
        if arg.contains("<<<") {
            if let Some(code) = args.get(i + 1) {
                run_stdin_code(code, ctx, language)?;
            }
        } else if arg == "<<"
            && args.get(i + 1).is_some_and(|next| next == "<")
            && let Some(code) = args.get(i + 2)
        {
            run_stdin_code(code, ctx, language)?;
        }
    }
    scan_previous_stdin(ctx, language)
}

fn scan_heredoc_stdin(
    args: &[String],
    ctx: ScanCtx<'_>,
    language: Option<Language>,
) -> Result<(), String> {
    let previous_has_heredoc = ctx
        .previous
        .is_some_and(|words| words.iter().any(|word| word.contains("<<")));
    let reads_heredoc = args.is_empty() || (args.len() == 1 && args[0] == "-");
    if reads_heredoc || previous_has_heredoc || args.iter().any(|arg| arg.contains("<<")) {
        for body in ctx.heredocs {
            run_stdin_code(body, ctx, language)?;
        }
    }
    Ok(())
}

fn scan_previous_stdin(ctx: ScanCtx<'_>, language: Option<Language>) -> Result<(), String> {
    let Some(previous) = ctx.previous else {
        return Ok(());
    };
    let (_, words) = leading_ritual_unlock(previous, false);
    if !words
        .first()
        .is_some_and(|word| matches!(basename(word), "echo" | "printf"))
    {
        return Ok(());
    }
    let values = &words[1..];
    if !values.is_empty() {
        run_stdin_code(&values.join(" "), ctx, language)?;
    }
    for value in values {
        if value.contains(char::is_whitespace) {
            run_stdin_code(value, ctx, language)?;
        }
    }
    Ok(())
}

fn run_stdin_code(code: &str, ctx: ScanCtx<'_>, language: Option<Language>) -> Result<(), String> {
    if let Some(language) = language {
        exec_scan(language, code, ctx.depth, ctx.ritual_unlock)
    } else {
        scan_program(code, ctx.depth + 1, ctx.ritual_unlock)
    }
}

pub(super) fn shell_invocation(args: &[String]) -> (Option<String>, bool) {
    let mut c_seen = false;
    let mut stdin_flag = false;
    let mut positional = false;
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        if arg == "--" {
            if let Some(next) = args.get(i + 1) {
                if c_seen {
                    return (Some(next.clone()), false);
                }
                positional = next != "-";
            }
            break;
        }
        if arg.contains("<<") {
            return (None, true);
        }
        match shell_arg_kind(arg) {
            ShellArgKind::ValueOption => i += 2,
            ShellArgKind::LongOption => i += 1,
            ShellArgKind::Cluster => {
                c_seen |= arg[1..].contains('c');
                stdin_flag |= arg[1..].contains('s');
                i += 1;
            }
            ShellArgKind::Positional => {
                if c_seen {
                    return (Some(arg.to_owned()), false);
                }
                positional = arg != "-";
                break;
            }
        }
    }
    (None, !c_seen && (stdin_flag || !positional))
}

enum ShellArgKind {
    ValueOption,
    LongOption,
    Cluster,
    Positional,
}

fn shell_arg_kind(arg: &str) -> ShellArgKind {
    if matches!(arg, "-o" | "+o" | "-O" | "+O" | "--rcfile" | "--init-file") {
        return ShellArgKind::ValueOption;
    }
    if arg.starts_with("--") {
        return ShellArgKind::LongOption;
    }
    if arg.len() > 1 && (arg.starts_with('-') || arg.starts_with('+')) {
        return ShellArgKind::Cluster;
    }
    ShellArgKind::Positional
}

fn leading_ritual_unlock(words: &[String], inherited: bool) -> (bool, &[String]) {
    let mut ritual_unlock = inherited;
    let mut i = 0;
    while i < words.len() && is_assignment(&words[i]) {
        ritual_unlock |= words[i] == TOKEN;
        i += 1;
    }
    (ritual_unlock, &words[i..])
}

fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

fn basename(word: &str) -> &str {
    word.rsplit('/').next().unwrap_or(word)
}

fn is_wrapper(name: &str) -> bool {
    matches!(
        name,
        "env"
            | "timeout"
            | "nice"
            | "nohup"
            | "command"
            | "builtin"
            | "exec"
            | "sudo"
            | "doas"
            | "time"
            | "setsid"
            | "ionice"
            | "stdbuf"
            | "busybox"
            | "hw-slot"
            | "xargs"
    )
}

fn wrapper_spec(name: &str) -> (&'static [&'static str], usize) {
    match name {
        "env" => (&["-u", "--unset", "-C", "--chdir", "-a", "--argv0"], 0),
        "timeout" => (&["-s", "--signal", "-k", "--kill-after"], 1),
        "nice" => (&["-n", "--adjustment"], 0),
        "exec" => (&["-a"], 0),
        "sudo" => (
            &[
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
            ],
            0,
        ),
        "doas" => (&["-u", "-C"], 0),
        "time" => (&["-f", "-o", "--format", "--output"], 0),
        "ionice" => (&["-c", "-n", "-p", "-P", "-u", "--class", "--classdata"], 0),
        "stdbuf" => (&["-i", "-o", "-e"], 0),
        "hw-slot" => (
            &[
                "--class",
                "--wait",
                "--max-temp",
                "--min-mem-gb",
                "--min-vram-gb",
                "--abort-temp",
                "--label",
            ],
            0,
        ),
        "xargs" => (
            &[
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
            0,
        ),
        _ => (&[], 0),
    }
}

fn option_value(args: &[String], flag: &str) -> Option<String> {
    args.iter().enumerate().find_map(|(i, arg)| {
        if arg == flag {
            args.get(i + 1).cloned()
        } else {
            arg.strip_prefix(&format!("{flag}=")).map(str::to_owned)
        }
    })
}
