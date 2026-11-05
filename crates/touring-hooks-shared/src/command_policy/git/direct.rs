//! The direct `git <subcommand> …` forms of the REGRA #11 v2 policy.

use super::super::lexer::flag_matches;

pub(super) fn scan_git_invocation(
    args: &[String],
    via_xargs: bool,
    ritual_unlock: bool,
) -> Result<(), String> {
    if ritual_unlock {
        return Ok(());
    }
    let Some((subcommand, rest)) = split_git_subcommand(args) else {
        if via_xargs {
            return Err(
                "REGRA #11 v2: `xargs git` receives an invisible subcommand on stdin".into(),
            );
        }
        return Ok(());
    };
    let subcommand = normalize(subcommand);
    let values: Vec<String> = rest.iter().map(|arg| normalize(arg)).collect();
    if via_xargs && xargs_risky(&subcommand) && !xargs_safe(&subcommand, &values) {
        return Err(format!(
            "REGRA #11 v2: `xargs git {subcommand}` receives invisible arguments on stdin"
        ));
    }
    if is_destructive(&subcommand, &values) {
        return Err(format!(
            "REGRA #11 v2: git {subcommand} is destructive and requires the ritual"
        ));
    }
    Ok(())
}

fn is_destructive(subcommand: &str, values: &[String]) -> bool {
    match subcommand {
        "stash" => !matches!(values.first().map(String::as_str), Some("list" | "show")),
        "reset" => has_any(values, &["--hard", "--merge", "--keep"]),
        "checkout" => values
            .first()
            .is_some_and(|value| matches!(value.as_str(), "-f" | "--force" | "--" | ".")),
        "switch" => has_flag(values, "--discard-changes"),
        "restore" => restore_is_destructive(values),
        "clean" => has_short_flag(values, "-f") || has_short_flag(values, "-d"),
        _ => history_or_remote_is_destructive(subcommand, values),
    }
}

fn history_or_remote_is_destructive(subcommand: &str, values: &[String]) -> bool {
    match subcommand {
        "push" => push_is_destructive(values),
        "rebase" | "filter-branch" | "filter-repo" => true,
        "branch" => has_flag(values, "-D"),
        "reflog" => values.first().is_some_and(|value| value == "expire"),
        "gc" => has_flag(values, "--prune=now"),
        _ => false,
    }
}

fn has_flag(values: &[String], flag: &str) -> bool {
    values.iter().any(|value| value == flag)
}

fn has_any(values: &[String], flags: &[&str]) -> bool {
    flags.iter().any(|flag| has_flag(values, flag))
}

fn has_short_flag(values: &[String], flag: &str) -> bool {
    values
        .iter()
        .take_while(|value| value.as_str() != "--")
        .any(|value| flag_matches(flag, value))
}

fn restore_is_destructive(values: &[String]) -> bool {
    let staged = has_flag(values, "--staged") || has_short_flag(values, "-S");
    let worktree = has_flag(values, "--worktree") || has_short_flag(values, "-W");
    !staged || worktree
}

fn push_is_destructive(values: &[String]) -> bool {
    has_any(values, &["--force", "--force-with-lease", "-d"])
        || has_short_flag(values, "-f")
        || values.iter().any(|value| {
            value.starts_with("--force-with-lease=")
                || value.starts_with("--force=")
                || value.starts_with("--delete")
                || value.starts_with(':')
        })
}

fn split_git_subcommand(args: &[String]) -> Option<(&str, &[String])> {
    const VALUE_OPTIONS: &[&str] = &[
        "-C",
        "-c",
        "--git-dir",
        "--work-tree",
        "--namespace",
        "--exec-path",
        "--super-prefix",
        "--config-env",
    ];
    let mut i = 0;
    while i < args.len() {
        let arg = normalize(&args[i]);
        if VALUE_OPTIONS.contains(&arg.as_str()) {
            i += 2;
        } else if arg.starts_with('-') {
            i += 1;
        } else {
            return Some((&args[i], &args[i + 1..]));
        }
    }
    None
}

fn xargs_risky(subcommand: &str) -> bool {
    matches!(
        subcommand,
        "reset"
            | "clean"
            | "checkout"
            | "switch"
            | "restore"
            | "stash"
            | "rebase"
            | "filter-branch"
            | "filter-repo"
    )
}

fn xargs_safe(subcommand: &str, values: &[String]) -> bool {
    match subcommand {
        "stash" => matches!(values.first().map(String::as_str), Some("list" | "show")),
        "restore" => {
            let staged = values
                .iter()
                .any(|value| matches!(value.as_str(), "--staged" | "-S"));
            let worktree = values
                .iter()
                .any(|value| matches!(value.as_str(), "--worktree" | "-W"));
            staged && !worktree
        }
        _ => false,
    }
}

fn normalize(arg: &str) -> String {
    arg.strip_prefix('$').unwrap_or(arg).to_owned()
}
