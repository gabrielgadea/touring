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
    if subcommand == "checkout" {
        let dir = checkout_dir(args);
        if let Some(path) = bare_checkout_path(&values, |word| resolves_as_ref(&dir, word)) {
            return Err(format!(
                "REGRA #11 v2: git checkout {path} discards the uncommitted changes of '{path}' \
                 (not a branch or commit there, so git reads it as a path)"
            ));
        }
    }
    Ok(())
}

/// Where a bare checkout's operands are resolved: the last `-C <dir>` before the subcommand,
/// else the process cwd.
fn checkout_dir(args: &[String]) -> std::path::PathBuf {
    let mut dir = std::path::PathBuf::from(".");
    let mut words = args.iter().map(|arg| normalize(arg));
    while let Some(word) = words.next() {
        if word == "-C" {
            if let Some(next) = words.next() {
                dir = dir.join(next);
            }
        } else if !word.starts_with('-') {
            break;
        }
    }
    dir
}

/// A bare `git checkout <word>…` (no `--`): the first word that is a path, if any. Gabriel's
/// decision (10/11/2026) is to copy git — the first operand that resolves as a ref is a ref (a
/// branch named like a file is switched to); every other operand, and every operand after the
/// ref, is a path, and checking a path out discards its uncommitted changes. `-b`/`-B`/`--orphan`
/// create a branch (their operands are a name and a start point); `--pathspec-from-file` is
/// itself a list of paths. Mirror: the bare-checkout loop of client/hooks/block_git.sh.
fn bare_checkout_path(values: &[String], is_ref: impl Fn(&str) -> bool) -> Option<&str> {
    if values.iter().any(|value| value == "--") {
        return None; // judged by checkout_is_destructive
    }
    let mut operands = Vec::new();
    for word in values.iter().map(String::as_str) {
        if word.starts_with("--") {
            if names_long(word, "--pathspec-from-file", 3) {
                return Some(word);
            }
            if names_long(word, "--orphan", 3) {
                return None;
            }
        } else if word.len() > 1 && word.starts_with('-') {
            if cluster_has(word, 'b', &[]) || cluster_has(word, 'B', &[]) {
                return None;
            }
        } else {
            operands.push(word);
        }
    }
    let (first, rest) = operands.split_first()?;
    if !is_ref(first) {
        return Some(first);
    }
    rest.first().copied()
}

/// `-` (the previous branch), a word that resolves as a commit, or a remote branch git would
/// track on checkout. No repository, an empty word or any failure = not a ref (fail-closed).
fn resolves_as_ref(dir: &std::path::Path, word: &str) -> bool {
    if word == "-" {
        return true;
    }
    if word.is_empty() || word.contains(['"', '\'']) {
        return false;
    }
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()
    };
    let commit = format!("{word}^{{commit}}");
    if git(&[
        "rev-parse",
        "--verify",
        "--quiet",
        "--end-of-options",
        &commit,
    ])
    .is_some_and(|out| out.status.success())
    {
        return true;
    }
    let remote = format!("refs/remotes/*/{word}");
    git(&["for-each-ref", "--count=1", "--format=x", &remote])
        .is_some_and(|out| out.status.success() && !out.stdout.is_empty())
}

fn is_destructive(subcommand: &str, values: &[String]) -> bool {
    match subcommand {
        "stash" => !matches!(values.first().map(String::as_str), Some("list" | "show")),
        "reset" => has_any(values, &["--hard", "--merge", "--keep"]),
        "checkout" => checkout_is_destructive(values),
        "switch" => has_flag(values, "--discard-changes"),
        "restore" => restore_is_destructive(values),
        "clean" => !clean_is_dry_run(values),
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

/// The options of a subcommand: its words up to `--` (exclusive), as git reads them.
fn options(values: &[String]) -> impl Iterator<Item = &str> {
    values
        .iter()
        .map(String::as_str)
        .take_while(|value| *value != "--")
}

/// git accepts any unambiguous PREFIX of a long option (`--forc` runs `--force`): `word` names
/// `full` when its part before `=` is a prefix of `full` at least `min` characters long.
/// Mirror: `names_long` in client/hooks/block_git.sh.
fn names_long(word: &str, full: &str, min: usize) -> bool {
    let name = word.split('=').next().unwrap_or(word);
    name.starts_with("--") && name.len() >= min && full.starts_with(name)
}

/// A short-option cluster (`-qf`) holds `letter` before any letter of `valued`, which takes the
/// rest of the cluster as its value (getopt: `-bf` is `-b f`). Mirror: `cluster_has`.
fn cluster_has(word: &str, letter: char, valued: &[char]) -> bool {
    let Some(cluster) = word.strip_prefix('-').filter(|c| !c.starts_with('-')) else {
        return false;
    };
    for ch in cluster.chars() {
        if ch == letter {
            return true;
        }
        if valued.contains(&ch) {
            return false;
        }
    }
    false
}

/// `--` first, `<ref> -- <path>` (a `--` with a word after it), or any option before `--` that
/// forces: `-f` in a cluster (`-b`/`-B` take the rest as a branch name), `--force` or a prefix of
/// it, and a bare `.`. Mirror: the `checkout` patterns and loop of client/hooks/block_git.sh.
fn checkout_is_destructive(values: &[String]) -> bool {
    values.first().is_some_and(|value| value == "--")
        || values.windows(2).any(|pair| pair[0] == "--")
        || options(values).any(|word| {
            word == "." || names_long(word, "--force", 3) || cluster_has(word, 'f', &['b', 'B'])
        })
}

/// A DRY RUN is the only safe `git clean` (fail-closed): a real `-n` (in a cluster, before `-e`,
/// which takes the rest as its pattern) or `--dry-run`/a prefix of it; the word after a bare
/// `-e`/`--exclude` is a pattern, never an option. Any `--no-<option>` (`--no-dry-run`, `--no-d`)
/// undoes an earlier one in git's parser, so a negation is never a dry run (fail-closed).
/// Mirror: the `clean` loop of block_git.sh.
fn clean_is_dry_run(values: &[String]) -> bool {
    let mut value_next = false;
    let mut dry = false;
    for word in options(values) {
        if std::mem::take(&mut value_next) {
            continue;
        }
        if is_negation(word) {
            return false;
        }
        if word.starts_with("--") {
            dry |= names_long(word, "--dry-run", 3);
            value_next = !word.contains('=') && names_long(word, "--exclude", 3);
        } else if let Some(letters) = word.strip_prefix('-').filter(|l| !l.is_empty()) {
            dry |= cluster_has(word, 'n', &['e']);
            value_next = letters.find('e') == Some(letters.len() - 1);
        }
    }
    dry
}

/// `--no-<option>`: git's parser negates (any prefix of) the option, undoing an earlier one.
fn is_negation(word: &str) -> bool {
    word.starts_with("--no-")
}

/// Only an unstage is safe: `--staged`/`-S` and no `--worktree`/`-W` — long prefixes count
/// (`--wor`; `--st` is the shortest `--staged`, `--s` is ambiguous with `--source`), clusters
/// count (`-SW`; `-s` takes the rest as its source), only before `--`, and any `--no-*`
/// negation is destructive (fail-closed: `--staged --no-staged` restores the working tree).
/// Mirror: the `restore` loop of client/hooks/block_git.sh.
fn restore_is_destructive(values: &[String]) -> bool {
    let staged = options(values)
        .any(|word| names_long(word, "--staged", 4) || cluster_has(word, 'S', &['s']));
    let worktree = options(values)
        .any(|word| names_long(word, "--worktree", 3) || cluster_has(word, 'W', &['s']));
    !staged || worktree || options(values).any(is_negation)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    /// A fake repository: `main`, `v1` and `feat` are refs; `feat` is also a file.
    fn fake_ref(word: &str) -> bool {
        matches!(word, "-" | "main" | "v1" | "feat" | "HEAD~1")
    }

    #[test]
    fn bare_checkout_of_a_path_names_the_path() {
        for (line, path) in [
            ("crates/x.py", "crates/x.py"),
            ("*.py", "*.py"),
            ("main crates/x.py", "crates/x.py"),
            ("-q v1 crates/x.py", "crates/x.py"),
            ("nao-existe", "nao-existe"),
            ("--pathspec-from-file=l.txt", "--pathspec-from-file=l.txt"),
        ] {
            assert_eq!(
                bare_checkout_path(&words(line), fake_ref),
                Some(path),
                "{line}"
            );
        }
    }

    #[test]
    fn a_ref_alone_or_a_new_branch_is_not_a_path() {
        for line in [
            "main",
            "feat",
            "-",
            "HEAD~1",
            "-q main",
            "--detach v1",
            "",
            "-b nova crates/x.py",
            "-qB nova",
            "--orphan novo",
            "main -- crates/x.py",
        ] {
            assert_eq!(bare_checkout_path(&words(line), fake_ref), None, "{line}");
        }
    }

    #[test]
    fn checkout_dir_follows_the_last_dash_c() {
        assert_eq!(
            checkout_dir(&words("checkout x")),
            std::path::PathBuf::from(".")
        );
        assert_eq!(
            checkout_dir(&words("-C a -C b checkout x")),
            std::path::PathBuf::from("./a/b")
        );
    }

    #[test]
    fn outside_a_repository_nothing_resolves_as_a_ref() {
        let dir = std::env::temp_dir().join(format!("no-repo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let no_repo = !std::process::Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(["rev-parse", "--git-dir"])
            .output()
            .expect("git runs")
            .status
            .success();
        if no_repo {
            assert!(!resolves_as_ref(&dir, "main"));
            assert!(resolves_as_ref(&dir, "-"));
        }
        let _ = std::fs::remove_dir(&dir);
    }
}
