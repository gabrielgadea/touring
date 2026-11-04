// touring-quality:allow-secrets — the REGRA #11 assignment is policy syntax, not a credential.
//! Git's REGRA #11 v2 policy, including code nested in shells and interpreters.
//!
//! `block_git.sh` remains the policy oracle. This module mirrors its destructive
//! forms without judging repository-local hook bypasses: those need filesystem
//! evidence and stay with the shell guard. `GIT_DESTRUCTIVE_OK=1` counts only as
//! an executable assignment, never as text in an echo, a message or a data heredoc.

mod direct;
mod interpreter;
mod nested;
mod text;

use super::vocabulary::GIT_ROUTE;
use super::{Verdict, with_route};

const MAX_DEPTH: usize = 8;
const TOKEN: &str = "GIT_DESTRUCTIVE_OK=1";

/// Judge every executable git occurrence of a shell program.
pub(super) fn judge(command: &str) -> Verdict {
    match scan_program(command, 0, false) {
        Ok(()) => Verdict::Allow,
        Err(reason) => Verdict::Block {
            reason: with_route(reason, Some(GIT_ROUTE)),
        },
    }
}

/// The read-only context one simple command needs. Keeping it in one named
/// object avoids passing the same five positional values down every branch.
#[derive(Clone, Copy)]
struct ScanCtx<'a> {
    previous: Option<&'a [String]>,
    heredocs: &'a [String],
    depth: usize,
    ritual_unlock: bool,
    via_xargs: bool,
}

fn scan_program(command: &str, depth: usize, inherited_ritual_unlock: bool) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err(format!(
            "REGRA #11 v2: nested shells exceed the verification depth ({MAX_DEPTH})"
        ));
    }
    let heredocs = text::heredoc_bodies(command);
    let masked = text::mask_script_heredocs(command);
    for pipeline in super::lexer::shell_pipelines(&masked) {
        let mut previous: Option<&[String]> = None;
        for words in &pipeline {
            let ctx = ScanCtx {
                previous,
                heredocs: &heredocs,
                depth,
                ritual_unlock: inherited_ritual_unlock,
                via_xargs: false,
            };
            nested::scan_command(words, ctx)?;
            previous = Some(words);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIT_PARITY: &str =
        include_str!("../../../tests/fixtures/command_policy/git_parity.jsonl");

    #[test]
    fn block_git_destructive_verdicts_have_parity_in_both_directions() {
        let mut checked = 0;
        for line in GIT_PARITY.lines() {
            let row: serde_json::Value = serde_json::from_str(line).expect("git parity row");
            let command = row["cmd"].as_str().expect("command");
            let expected = row["shell"].as_str().expect("shell verdict") == "block";
            assert_eq!(
                judge(command).is_block(),
                expected,
                "shell={} rust={:?} command={command:?}",
                row["shell"],
                judge(command)
            );
            checked += 1;
        }
        assert_eq!(checked, 279, "the shell-oracle corpus shrank");
    }

    #[test]
    fn direct_destructive_forms_are_blocked_and_reads_are_free() {
        for command in [
            "git reset --hard HEAD",
            "git -C /tmp/repo clean -fd",
            "git push --force-with-lease origin main",
            "git restore .",
            "git stash pop",
            "git rebase main",
            "git branch -D old",
            "git reflog expire --expire=now --all",
            "git gc --prune=now",
        ] {
            assert!(judge(command).is_block(), "{command}");
        }
        for command in [
            "git status",
            "git stash list",
            "git stash show -p stash@{0}",
            "git restore --staged src/lib.rs",
            "git push origin main",
            "echo 'git reset --hard'",
            "git commit -m 'explain git reset --hard'",
        ] {
            assert!(matches!(judge(command), Verdict::Allow), "{command}");
        }
    }

    #[test]
    fn nested_code_is_judged_but_data_is_not() {
        for command in [
            "bash -c 'git reset --hard HEAD'",
            "bash -c \"sh -c 'git clean -fd'\"",
            "eval 'git stash'",
            "env X=1 timeout 60 nice -n 5 hw-slot -- bash -c 'git reset --hard'",
            "echo --hard | xargs git reset",
            "python3 -c \"import subprocess; subprocess.run(['git','reset','--hard'])\"",
            "node -e \"require('child_process').execSync('git reset --hard')\"",
            "echo 'git reset --hard' | sh",
        ] {
            assert!(judge(command).is_block(), "{command}");
        }
        for command in [
            "bash -c 'echo \"git reset --hard\"'",
            "cat <<'EOF'\ngit reset --hard\nEOF",
            "bash script.sh <<'EOF'\ngit reset --hard\nEOF",
            "tee f.txt <<EOF\ngit reset --hard\nEOF",
            "python3 -c \"print('git reset --hard')\"",
            "python3 script.py -c \"import os; os.system('git reset --hard')\"",
            "xargs git status",
        ] {
            assert!(matches!(judge(command), Verdict::Allow), "{command}");
        }
    }

    #[test]
    fn the_token_is_code_not_data() {
        for command in [
            "GIT_DESTRUCTIVE_OK=1 git reset --hard",
            "bash -c 'GIT_DESTRUCTIVE_OK=1 git reset --hard'",
            "env GIT_DESTRUCTIVE_OK=1 bash -c 'git clean -fd'",
        ] {
            assert!(matches!(judge(command), Verdict::Allow), "{command}");
        }
        for command in [
            "echo 'GIT_DESTRUCTIVE_OK=1'; bash -c 'git reset --hard'",
            "bash -c \"echo 'GIT_DESTRUCTIVE_OK=1'; git reset --hard\"",
        ] {
            assert!(judge(command).is_block(), "{command}");
        }
    }
}
