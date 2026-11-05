//! The precedence of the presentation, as a table:
//! **command prefix → hook env → `TOURING_CODE_ONLY` alias → project `touring.toml` → armed
//! policy → default `both`**.
//!
//! Every level reaches [`resolve_presentation`] as a VALUE (`CodeModeEnv` for the two env
//! levels, a temp `touring.toml` for the project), so no row reads the test process's own
//! environment — the 15 tests the hermeticity matrix (08/10) found red under
//! `TOURING_CODE_MODE=native` read it by accident.

use crate::code_mode_gates::mode::{
    code_gates_disabled_from, project_presentation, resolve_presentation,
};
use crate::code_mode_gates::{CodeModeEnv, CodeModePresentation};
use CodeModePresentation::{Both, Code, Native};

/// One precedence case: the project file, the hook env, the alias, the command, the answer.
struct Row {
    name: &'static str,
    /// Body of `<root>/.touring/touring.toml`; `None` = no file at all.
    toml: Option<&'static str>,
    env_mode: Option<&'static str>,
    alias: Option<&'static str>,
    cmd: &'static str,
    want: CodeModePresentation,
}

const CODE_PROJECT: Option<&str> = Some("[code_mode]\nmode = \"code\"\n");

const ROWS: &[Row] = &[
    // ── the prefix is the outermost level ────────────────────────────────────
    Row {
        name: "prefix native relaxes a code project",
        toml: CODE_PROJECT,
        env_mode: None,
        alias: None,
        cmd: "TOURING_CODE_MODE=native grep foo src/",
        want: Native,
    },
    Row {
        name: "prefix both relaxes a code project",
        toml: CODE_PROJECT,
        env_mode: None,
        alias: None,
        cmd: "TOURING_CODE_MODE=both grep foo src/",
        want: Both,
    },
    Row {
        name: "no prefix: the project decides",
        toml: CODE_PROJECT,
        env_mode: None,
        alias: None,
        cmd: "grep foo src/",
        want: Code,
    },
    Row {
        name: "env beats the project",
        toml: CODE_PROJECT,
        env_mode: Some("native"),
        alias: None,
        cmd: "grep foo src/",
        want: Native,
    },
    Row {
        name: "prefix beats the env",
        toml: CODE_PROJECT,
        env_mode: Some("native"),
        alias: None,
        cmd: "TOURING_CODE_MODE=both grep foo src/",
        want: Both,
    },
    Row {
        name: "prefix tightens a project with no configuration",
        toml: None,
        env_mode: None,
        alias: None,
        cmd: "TOURING_CODE_MODE=code grep foo",
        want: Code,
    },
    // ── an invalid prefix does not silence the next levels ───────────────────
    Row {
        name: "invalid prefix falls to the project, never to an unrequested collapse",
        toml: CODE_PROJECT,
        env_mode: None,
        alias: None,
        cmd: "TOURING_CODE_MODE=explode grep foo",
        want: Code,
    },
    Row {
        name: "invalid prefix yields to the injected env",
        toml: CODE_PROJECT,
        env_mode: Some("native"),
        alias: None,
        cmd: "TOURING_CODE_MODE=explode grep foo",
        want: Native,
    },
    // ── what counts as a prefix ──────────────────────────────────────────────
    Row {
        name: "prefix after other assignments",
        toml: None,
        env_mode: None,
        alias: None,
        cmd: "FOO=1 TOURING_CODE_MODE=native grep foo",
        want: Native,
    },
    Row {
        name: "prefix value in quotes",
        toml: None,
        env_mode: None,
        alias: None,
        cmd: "TOURING_CODE_MODE=\"native\" grep foo",
        want: Native,
    },
    Row {
        name: "the variable in the middle of the command is not a prefix",
        toml: None,
        env_mode: None,
        alias: None,
        cmd: "echo x TOURING_CODE_MODE=native",
        want: Both,
    },
    Row {
        name: "the env applies exactly where the middle of the command does not",
        toml: None,
        env_mode: Some("code"),
        alias: None,
        cmd: "echo x TOURING_CODE_MODE=native",
        want: Code,
    },
    // ── defaults and the env levels ──────────────────────────────────────────
    Row {
        name: "no file, no env: the default",
        toml: None,
        env_mode: None,
        alias: None,
        cmd: "grep x",
        want: Both,
    },
    Row {
        name: "injected env beats the default (native)",
        toml: None,
        env_mode: Some("native"),
        alias: None,
        cmd: "grep x",
        want: Native,
    },
    Row {
        name: "injected env beats the default (code)",
        toml: None,
        env_mode: Some("code"),
        alias: None,
        cmd: "grep x",
        want: Code,
    },
    Row {
        name: "the legacy alias means code",
        toml: None,
        env_mode: None,
        alias: Some("1"),
        cmd: "grep x",
        want: Code,
    },
    Row {
        name: "the alias only counts with the value 1",
        toml: None,
        env_mode: None,
        alias: Some("0"),
        cmd: "grep x",
        want: Both,
    },
    Row {
        name: "the env mode beats the alias",
        toml: None,
        env_mode: Some("native"),
        alias: Some("1"),
        cmd: "grep x",
        want: Native,
    },
    Row {
        name: "an invalid env mode lets the alias through",
        toml: None,
        env_mode: Some("explode"),
        alias: Some("1"),
        cmd: "grep x",
        want: Code,
    },
    Row {
        name: "the alias beats the project",
        toml: Some("[code_mode]\nmode = \"native\"\n"),
        env_mode: None,
        alias: Some("1"),
        cmd: "grep x",
        want: Code,
    },
    // ── the project file ─────────────────────────────────────────────────────
    Row {
        name: "[code_mode] among other sections",
        toml: Some("[daemon]\nper_project = true\n\n[code_mode]\nmode = \"code\"\n"),
        env_mode: None,
        alias: None,
        cmd: "grep x",
        want: Code,
    },
    Row {
        name: "native project",
        toml: Some("[code_mode]\nmode = \"native\"\n"),
        env_mode: None,
        alias: None,
        cmd: "grep x",
        want: Native,
    },
    Row {
        name: "a `mode` key of another section decides nothing",
        toml: Some("[toolchain]\nmode = \"code\"\nchannel = \"dev\"\n"),
        env_mode: None,
        alias: None,
        cmd: "grep x",
        want: Both,
    },
];

fn project_with(toml: Option<&str>) -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("project root");
    if let Some(body) = toml {
        let dir = root.path().join(".touring");
        std::fs::create_dir_all(&dir).expect("mkdir .touring");
        std::fs::write(dir.join("touring.toml"), body).expect("write touring.toml");
    }
    root
}

#[test]
fn the_precedence_of_the_presentation_holds_row_by_row() {
    for row in ROWS {
        let root = project_with(row.toml);
        let env = CodeModeEnv {
            mode: row.env_mode.map(str::to_owned),
            code_only_alias: row.alias.map(str::to_owned),
            arm_armed: false,
        };
        assert_eq!(
            resolve_presentation(root.path(), row.cmd, &env),
            row.want,
            "{}",
            row.name
        );
    }
}

#[test]
fn the_project_section_is_read_as_the_project_level_and_only_that() {
    let root = project_with(Some("[daemon]\nper_project = true\n\n[code_mode]\nmode = \"code\"\n"));
    assert_eq!(project_presentation(root.path()), Some(Code));
    let other = project_with(Some("[toolchain]\nmode = \"code\"\nchannel = \"dev\"\n"));
    assert_eq!(
        project_presentation(other.path()),
        None,
        "only [code_mode] decides"
    );
}

/// The human kill switch reads exactly `1`: a typo or a truthy-looking word must not turn the
/// gates off (and an unset variable must leave them on).
#[test]
fn the_kill_switch_is_exactly_the_value_one() {
    assert!(code_gates_disabled_from(Some("1")));
    for off in [None, Some(""), Some("0"), Some("true"), Some("yes"), Some("01"), Some(" 1")] {
        assert!(!code_gates_disabled_from(off), "{off:?} must not disable the gates");
    }
}
