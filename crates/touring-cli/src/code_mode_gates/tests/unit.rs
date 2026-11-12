//! Unit tests of the module's own parts — the pure classifiers, renderers and predicates the
//! chain is built from. They carry no event/verdict semantics (the table in `rows` has those),
//! so they stay with the code they pin, moved verbatim from the old suggester suites.

use super::super::*;
use super::support::*;
use serde_json::json;

#[test]
fn the_deny_delivers_a_similar_block_when_one_exists() {
    let conn = mem_with_body(
        "snippet:varre-pubs",
        "for f in crates/*/src/lib.rs; do grep -n pub $f; done",
    );
    let line = snippet_hint_line(
        &conn,
        "for f in crates/*/src/main.rs; do grep -n pub $f; done",
    )
    .expect("a similar block exists for this program");
    assert!(line.contains("snippet:varre-pubs"), "{line}");
    assert!(line.contains("memory recall"), "{line}");
}

#[test]
fn empty_ladder_is_silent_never_fabricated() {
    let conn = rusqlite::Connection::open_in_memory().expect("mem db");
    conn.execute_batch("CREATE TABLE memory_entries (key TEXT PRIMARY KEY, value TEXT);")
        .expect("t");
    assert!(snippet_hint_line(&conn, "echo hello world").is_none());
}

#[test]
fn missing_db_is_empty_never_an_error() {
    let root = std::path::Path::new("/definitely/not/a/project/root/xyz");
    assert_eq!(snippet_hint_for_program("anything", root), "");
}

#[test]
fn actor_key_separates_subagents_and_defaults_to_anon() {
    let parent = session_key(&json!({"session_id": "s-1"}));
    let child = session_key(&json!({"session_id": "s-1", "agent_id": "agent-x"}));
    assert_ne!(
        parent, child,
        "a subagent must not share its parent's windows"
    );
    assert_eq!(session_key(&json!({})), "anon");
    assert_eq!(session_key(&json!({"session_id": ""})), "anon");
}

/// V0e5 (09/10/2026): `/proc` IS a sandbox read root, yet ANOTHER process's
/// ptrace-guarded entries are not readable from inside — measured on 30.4.94
/// against a live daemon, sandbox × native. Rewritten into the sandbox,
/// `tr '\0' '\n' < /proc/$p/environ` returned rc 0 with an empty answer; such a
/// command must run native. The pid can be a literal, `$p`, `${pid}` or a glob.
#[test]
fn another_process_ptrace_guarded_proc_entry_is_unreachable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    for cmd in [
        "tr '\\0' '\\n' < /proc/$p/environ",
        "readlink /proc/1234/exe",
        "cat /proc/*/maps",
        "ls -l /proc/${pid}/cwd",
        "readlink /proc/$p/fd/3",
        "cat /proc/1234/fdinfo/0",
    ] {
        assert!(
            unreachable_in_sandbox(root, cmd).is_some(),
            "a ptrace-guarded entry of another process was judged reachable: {cmd}"
        );
    }
    // Control: what the sandbox DOES read stays fusible — the same process, the
    // unguarded entries, and the fd directory listing itself.
    for cmd in [
        "head -1 /proc/cpuinfo",
        "cat /proc/self/environ",
        "cat /proc/$p/status",
        "cat /proc/1234/cmdline",
        "ls /proc/$p/fd",
    ] {
        assert!(
            unreachable_in_sandbox(root, cmd).is_none(),
            "a /proc read the sandbox performs was judged unreachable: {cmd}"
        );
    }
}

#[test]
fn g10_exec_class_reconhece_o_executor_e_exclui_o_resto() {
    use crate::code_mode_gates::exec_class_of;
    assert_eq!(
        exec_class_of(".venv/bin/python3 scripts/verifica.py"),
        Some("python")
    );
    assert_eq!(
        exec_class_of("python3 -m pytest tests/test_a.py"),
        Some("python")
    );
    assert_eq!(exec_class_of("python3.11 runner.py"), Some("python"));
    assert_eq!(exec_class_of("pytest tests/test_a.py -x"), Some("pytest"));
    assert_eq!(exec_class_of(".venv/bin/pytest tests/ -q"), Some("pytest"));
    assert_eq!(exec_class_of("time python3 runner.py"), Some("python"));
    assert_eq!(exec_class_of("cd /x && pytest tests/"), Some("pytest"));
    // inline entered the burst as its OWN class (tightening 29/08): the
    // remedy is 1:1 (the verbatim body), not the R9 fusion that justified the
    // historical exclusion
    assert_eq!(
        exec_class_of("python3 -c 'print(1)'"),
        Some("python-inline")
    );
    assert_eq!(
        exec_class_of("python3 - <<'EOF'\nprint(1)\nEOF"),
        Some("python-inline")
    );
    assert_eq!(exec_class_of("python3 - arg1"), Some("python-inline"));
    // marked mutation is left out by construction (P2.3)
    assert_eq!(exec_class_of("python3 setup.py install"), None);
    assert_eq!(exec_class_of("python3 runner.py > out.txt"), None);
    assert_eq!(exec_class_of("python3 -m pip install x"), None);
    // non-executor
    assert_eq!(exec_class_of("cargo test"), None);
    assert_eq!(exec_class_of("grep foo f"), None);
}

#[test]
fn s5_redirect_de_fd_nao_anula_a_classe_exec() {
    use crate::code_mode_gates::exec_class_of;
    // Hole B: `contains(">")` over the whole blob blinded G10.
    assert_eq!(
        exec_class_of("cd /tmp && python3 medir.py 2>&1 | head -5"),
        Some("python")
    );
    assert_eq!(
        exec_class_of("python3 medir.py 2>/dev/null"),
        Some("python")
    );
    assert_eq!(exec_class_of("pytest tests/ -q 2>&1"), Some("pytest"));
    // A REAL output redirect stays out — a write is not an exec burst.
    assert_eq!(exec_class_of("python3 runner.py > out.txt"), None);
    assert_eq!(exec_class_of("python3 runner.py >> log.txt"), None);
}

#[test]
fn s5_assignment_prefixo_e_transparente_para_a_classe() {
    use crate::code_mode_gates::{effective_tokens, exec_class_of};
    // Hole C: an assignment-only segment returned empty tokens and ALL
    // classification died there.
    assert_eq!(
        exec_class_of("P=docs/x\npython3 medir.py --map $P"),
        Some("python")
    );
    assert_eq!(
        effective_tokens("P=docs/x\nR=docs/y\ngrep -n foo arq.rs").first(),
        Some(&"grep")
    );
    // A lone assignment still has no class (there is no verb).
    assert_eq!(exec_class_of("P=docs/x"), None);
}

#[test]
fn s5_corpo_de_heredoc_nao_alimenta_o_filtro_de_mutacao() {
    use crate::code_mode_gates::{exec_class_of, mutation_scan_view};
    // The body is DATA on stdin — a `git ` or `>` inside it is not a command.
    let cmd = "python3 medir.py <<'EOF'\nx = \"git checkout\"\ny = 1 > 0\nEOF";
    assert!(!mutation_scan_view(cmd).contains("git "));
    assert_eq!(exec_class_of(cmd), Some("python"));
    // A herestring is not a heredoc — the view must not swallow the rest.
    assert!(mutation_scan_view("cat <<< 'x' && rm f").contains("rm "));
}

/// Onda 2 (09/11/2026): the heredoc reading is the shared lexer's. A quoted `<<EOF` is
/// no heredoc, a body a shell runs is commands, and `python3 -c 'print(1 << 3)'` holds no
/// heredoc — the `<<` inside the quotes used to end the search with `None`.
#[test]
fn heredoc_do_code_mode_e_o_do_lexer_compartilhado() {
    use crate::code_mode_gates::{mutation_scan_view, python_inline_body};
    assert!(mutation_scan_view("echo \"<<EOF\"\nrm f").contains("rm f"));
    assert!(mutation_scan_view("bash <<'EOF'\nrm f\nEOF").contains("rm f"));
    assert!(!mutation_scan_view("cat <<'EOF' >x\nrm f\nEOF").contains("rm f"));
    assert_eq!(
        python_inline_body("python3 -c 'print(1 << 3)'").as_deref(),
        Some("print(1 << 3)")
    );
    assert_eq!(
        python_inline_body("python3 - <<'PY'\nprint(2)\nPY").as_deref(),
        Some("print(2)")
    );
}

#[test]
fn s5_par_write_run_detecta_escrita_e_execucao_do_mesmo_script() {
    use crate::code_mode_gates::{script_run_targets, script_write_target};
    // The write registers the target (typical heredoc of the execute-observe loop).
    assert_eq!(
        script_write_target("cat > /tmp/s/medir.py <<'PYEOF'\nprint(1)\nPYEOF"),
        Some("/tmp/s/medir.py".to_string())
    );
    assert_eq!(
        script_write_target("cd /proj\ncat > scratch/x.sh <<'EOF'\nls\nEOF"),
        Some("scratch/x.sh".to_string())
    );
    assert_eq!(script_write_target("cat > notas.md <<'EOF'\noi\nEOF"), None);
    // The execution resolves the path even behind cd/assignment.
    assert_eq!(
        script_run_targets("cd /proj\n.venv/bin/python3 /tmp/s/medir.py"),
        vec!["/tmp/s/medir.py".to_string()]
    );
    assert_eq!(
        script_run_targets("bash scratch/x.sh"),
        vec!["scratch/x.sh".to_string()]
    );
    // The CANONICAL pattern of analise (24 of 60): writes AND executes in the SAME
    // multi-line tool_use — the run on the line after the heredoc MUST show up.
    assert_eq!(
        script_run_targets(
            "cat > /tmp/s/perfil.py <<'PYEOF'\nimport json\nPYEOF\n\npython3 /tmp/s/perfil.py"
        ),
        vec!["/tmp/s/perfil.py".to_string()]
    );
    // pytest/modules never match — the low threshold does not tax the common case.
    assert!(script_run_targets("python3 -m pytest tests/test_a.py").is_empty());
    assert!(script_run_targets("pytest tests/test_a.py").is_empty());
    // A real redirect in the run's SEGMENT: the --file remedy would not reproduce
    // the write — that segment does not count.
    assert!(script_run_targets("python3 /tmp/s/medir.py > out.json").is_empty());
}

#[test]
fn s5_python_inline_tem_remedio_um_para_um() {
    use crate::code_mode_gates::{python_inline_body, python_inline_remedy};
    // heredoc: the verbatim body, without R9 fusion
    let cmd = "python3 - <<'PYEOF'\nimport json\nprint(1)\nPYEOF";
    assert_eq!(
        python_inline_body(cmd).as_deref(),
        Some("import json\nprint(1)")
    );
    let r = python_inline_remedy(cmd);
    assert!(r.starts_with("touring run --lang python --code '"), "{r}");
    assert!(r.contains("import json"), "{r}");
    // -c: the literal between the outer quotes
    assert_eq!(
        python_inline_body("python3 -c 'print(42)'").as_deref(),
        Some("print(42)")
    );
    // unextractable → honest placeholder, never a broken command
    assert!(python_inline_remedy("python3 -").contains("verbatim"));
}

/// S213 (25/09/2026) — whoever APPENDS to a file is never inspection. The burst remedy fuses
/// the window's commands and re-executes them; `cat x >> triad_hook.rs && cargo test` was
/// classified as `cat` and the deny proposed running it again (it would append the test twice).
/// Overwriting (`>`) stays inspection — P2.3 decision: re-executing is idempotent.
#[test]
fn a_command_that_writes_a_file_is_never_inspection() {
    use crate::code_mode_gates::scan_class_of;
    for writer in [
        "cat pt3_test.rs >> src/triad_hook.rs && cargo test",
        "grep -n foo src/lib.rs >> achados.txt",
        "head -5 src/lib.rs | tee -a copia.txt",
    ] {
        assert_eq!(
            scan_class_of(writer),
            None,
            "a writer classified as inspection: {writer}"
        );
    }
    // controls: pure reads stay inspection, quoted `>` is data, /dev/null is not a write
    assert_eq!(scan_class_of("cat src/lib.rs"), Some("cat"));
    assert_eq!(scan_class_of("grep -n '>>' src/lib.rs"), Some("grep"));
    assert_eq!(
        scan_class_of("cat README.md > out.txt"),
        Some("cat"),
        "P2.3: overwrite is idempotent"
    );
    assert_eq!(scan_class_of("grep -rn foo src 2> /dev/null"), Some("grep"));
}

/// S213b (26/09/2026) — a SEQUENCE is only inspection if every step inspects. The class
/// came from the 1st real verb: `cd t && grep -m1 x Cargo.toml; timeout 1200 cargo clippy …`
/// became `grep` and the burst remedy proposed re-executing 20 min of clippy (observed
/// live); with a `touring memory store` in its place, it would re-execute a write.
#[test]
fn a_sequence_with_a_step_that_does_work_is_never_inspection() {
    use crate::code_mode_gates::scan_class_of;
    for worker in [
        "cd /x && grep -m1 -n edition Cargo.toml; timeout 1200 cargo clippy --workspace; rustfmt --check a.rs",
        "grep -n foo src/lib.rs && touring memory store k v",
        "cat a.txt\npytest -q tests/",
    ] {
        assert_eq!(
            scan_class_of(worker),
            None,
            "a working sequence classified as inspection: {worker}"
        );
    }
    // controls: sequences of reads — with navigation, echo separators and an
    // assignment — stay inspection, classed by their first reading step
    assert_eq!(
        scan_class_of("cd /x && grep -n foo a.rs; echo ===; cat b.rs"),
        Some("grep")
    );
    assert_eq!(scan_class_of("ls src; wc -l src/lib.rs"), Some("ls"));
    assert_eq!(scan_class_of("P=1\ncat f | head -5"), Some("cat"));
}

/// S212 (25/09/2026) — the G10 remedy NEVER re-executes what already ran. The ledger carries the
/// whole burst in order; only the LAST command was denied — the earlier ones ALREADY executed,
/// including writes (saving memory, committing). Re-executing them repeated effects: observed
/// live, the deny proposed running again the script that saves lessons. Only the denied one becomes a step;
/// the executed ones travel as a context comment (the 1st line, never the body).
#[test]
fn g10_remedy_never_replays_executed_commands() {
    use crate::code_mode_gates::r9_exec_program;
    let executed = vec![
        "python3 grava_licoes.py".to_string(),
        "python3 -c 'import os\nos.remove(\"x\")'".to_string(),
    ];
    let pending = "python3 scout.py";
    let p = r9_exec_program(&executed, pending);
    let body = p
        .strip_prefix("touring run --lang bash --timeout-ms 120000 --code '")
        .and_then(|b| b.strip_suffix('\''))
        .expect("the program shape")
        .replace("'\\''", "'");
    let steps: Vec<&str> = body
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect();
    let runs = |needle: &str| steps.iter().any(|l| l.contains(needle));
    assert!(
        runs("python3 scout.py"),
        "the denied command is the step: {body}"
    );
    assert!(
        !runs("grava_licoes"),
        "an executed command must never run again: {body}"
    );
    assert!(
        !runs("os.remove"),
        "no line of an executed command is a step: {body}"
    );
    assert!(
        body.contains("# já executado"),
        "the executed ones travel as context: {body}"
    );
}

#[test]
fn g10_programa_r9_agrega_as_chamadas_reais_em_bash_executavel() {
    use crate::code_mode_gates::r9_exec_program;
    let cmds = vec![
        "pytest tests/test_a.py -x".to_string(),
        "pytest tests/test_b.py -x".to_string(),
        "python3 -c 'aspas-sobrevivem'".to_string(),
    ];
    let (pending, executed) = cmds.split_last().expect("non-empty");
    let p = r9_exec_program(executed, pending);
    // 30/08: R9 is BASH — the python template with `import subprocess` was
    // exactly the capability that X6 denies under Sandboxed: the G10 deny
    // emitted a route that the next gate barred (measured by the peer
    // analise-a2 — there was no conforming path). Bash is the only lang with the
    // subprocess-only waiver; `--timeout-ms` travels explicitly (E4).
    assert!(
        p.starts_with("touring run --lang bash --timeout-ms 120000 --code '"),
        "{p}"
    );
    assert!(p.ends_with('\''));
    assert!(
        !p.contains("subprocess"),
        "a rota emitida não pode depender da capability que o X6 nega: {p}"
    );
    for c in &cmds {
        // the command travels VERBATIM, with the shell-wrapper escape on
        // top (the outer bash undoes it on delivery)
        let literal = c.replace('\'', "'\\''");
        assert!(p.contains(&literal), "faltou {literal} no programa");
    }
    assert!(p.contains("FALHOU"), "veredito por comando: {p}");
    assert!(p.contains("RESUMO"), "digest agregado: {p}");
}

#[test]
fn s6_intent_carrega_classe_e_alvos_do_trabalho() {
    use crate::code_mode_gates::exec_burst_intent;
    let cmds = vec![
        "pytest tests/test_a.py -x".to_string(),
        "pytest tests/test_b.py -x".to_string(),
        "pytest tests/test_a.py -x".to_string(), // duplicate: dedup
    ];
    let intent = exec_burst_intent("pytest", &cmds);
    assert!(intent.starts_with("pytest"), "{intent}");
    // work components (dir + stem), not the whole path — they are
    // what match the artifact's purpose (BM25's required_matches)
    assert!(intent.contains("tests"), "{intent}");
    assert!(intent.contains("test"), "{intent}");
    assert!(
        !intent.contains("test_a.py"),
        "caminho inteiro dilui: {intent}"
    );
    assert_eq!(
        intent.split(' ').filter(|t| *t == "test").count(),
        1,
        "dedup exato (substring não conta — `pytest` e `tests` contêm `test`): {intent}"
    );
    assert!(
        !intent.contains("-x"),
        "flags não são termo de trabalho: {intent}"
    );
    // lone class when the burst has no targets
    assert_eq!(exec_burst_intent("python", &[]), "python");
}

#[test]
fn s6_sem_portfolio_ou_sem_match_o_remedio_e_none_e_o_r9_basta() {
    use crate::code_mode_gates::portfolio_remedy_for_burst;
    // fail-open: with no readable (or empty) index, the deny comes out with R9 only
    let cmds = vec!["pytest tests/xyzzy_inexistente_q123.py".to_string()];
    let _ = portfolio_remedy_for_burst("pytest", &cmds); // never panics
}

#[test]
fn s6_prior_art_instanciado_quando_o_portfolio_tem_programa_proximo() {
    // The index comes from the scratch by PARAMETER (store::load_from), without
    // TOURING_PORTFOLIO_DIR: the env is global to the process and leaks between
    // parallel tests (the race measured on 08/10 — see GateOverrides).
    use crate::code_mode_gates::portfolio_remedy_for_burst_on;
    let dir = monta_portfolio_scratch();
    let index =
        touring_foundation::portfolio::store::load_from(&dir).expect("índice do scratch carrega");
    let cmds = (1..=3)
        .map(|i| format!("pytest tests/test_{i}.py -x"))
        .collect::<Vec<_>>();
    let remedy = portfolio_remedy_for_burst_on("pytest", &cmds, &index);
    let remedy = remedy.expect("prior art encontrado para a rajada de pytest");
    assert!(
        remedy.contains("python3 ~/x/rodar_suite.py --fast"),
        "{remedy}"
    );
    assert!(remedy.contains("~/x/rodar_suite.py"), "{remedy}");
}

#[test]
fn g8_laco_de_inspecao_vira_uma_varredura() {
    use crate::code_mode_gates::loop_rewrite_candidate;
    // The case measured in the 25/08 transcript: pure inspection loop.
    let cmd = r#"for f in a.rs b.rs; do grep -n "fn main" $f; done"#;
    let got = loop_rewrite_candidate(cmd).expect("laço de inspeção é candidato");
    assert!(got.starts_with("touring run --lang bash --code '"));
    assert!(
        got.contains("for f in a.rs b.rs"),
        "carrega o comando REAL, não placeholder"
    );
}

#[test]
fn g8_nunca_reescreve_laco_com_efeito() {
    use crate::code_mode_gates::loop_rewrite_candidate;
    // These would break under the sandbox capabilities — they stay out by
    // construction, not by luck. A gate that converts what it should not is
    // worse than no gate.
    for cmd in [
        "for p in $(pgrep x); do kill -9 $p; done",
        "for c in a b; do cargo test -p $c; done",
        "for f in *.tmp; do rm $f; done",
        "for d in a b; do git -C $d status; done",
        "for i in 1 2; do echo $i >> saida.txt; done",
        "for f in a b; do python3 -c 'print(1)'; done",
    ] {
        assert!(
            loop_rewrite_candidate(cmd).is_none(),
            "laço com efeito NÃO pode ser reescrito: {cmd}"
        );
    }
}

#[test]
fn g8_ignora_heredoc_aspas_impares_e_o_que_ja_e_code_mode() {
    use crate::code_mode_gates::loop_rewrite_candidate;
    assert!(
        loop_rewrite_candidate("for f in a; do cat <<EOF\n$f\nEOF\ndone").is_none(),
        "heredoc é dado, e o quoting não sobrevive"
    );
    assert!(
        loop_rewrite_candidate(r#"for f in a; do grep 'x $f; done"#).is_none(),
        "aspas simples ímpares quebrariam o --code"
    );
    assert!(
        loop_rewrite_candidate("touring run --lang bash --code 'for f in a; do ls $f; done'")
            .is_none(),
        "já é code mode"
    );
    assert!(
        loop_rewrite_candidate("grep -rn foo crates/").is_none(),
        "sem laço, nada a converter"
    );
}

#[test]
fn g8_escapa_aspas_simples_preservando_o_comando() {
    use crate::code_mode_gates::loop_rewrite_candidate;
    let cmd = r#"for f in a b; do grep -n 'fn main' $f; done"#;
    let got = loop_rewrite_candidate(cmd).expect("candidato");
    // The body enters escaped, so that the outer shell hands the
    // original command back to the sandbox — the rewrite cannot alter semantics.
    assert!(
        got.contains(r"'\''fn main'\''"),
        "aspas internas escapadas: {got}"
    );
}

/// S3 (27/08/2026) — the calibration stopped being by class NAME and became
/// by BURST, and the measurement is the reason.
///
/// The previous version of this test asserted the opposite: that `ls`/`wc`/`sed-n`
/// could NOT collapse because "they are a single call". Measuring 115 transcripts
/// (`scripts/s3_burst_distribution.py`, 300s window) this proved false
/// precisely for the two with the largest volume — `sed-n` has 408 calls with
/// 81.4% of the volume in bursts ≥2, `ls` has 338 with 71.3% — whereas `find`,
/// which the list denied, is 56.2% isolated and did not produce ONE burst ≥3.
///
/// What the test guards now is the invariant that survives recalibration:
/// every class the gate tracks is one that the classifier emits, and what
/// discriminates is not the list but the burst threshold.
#[test]
fn a_calibracao_bate_com_a_medicao() {
    for classe in ["grep", "cat", "find", "ls", "wc", "sed-n"] {
        assert!(
            CODE_MODE_COLLAPSED_CLASSES.contains(&classe),
            "`{classe}` é inspeção reconhecida — o ledger da rajada tem de vê-la, \
             senão o volume fan-out dela fica invisível ao gate"
        );
    }
    assert_eq!(
        INSPECT_BURST_ADVISE_AT, 2,
        "a 1ª tem de passar: cobrar da isolada é taxar o caso comum, a recusa \
         que o próprio DeepSeek documentou"
    );
}

/// The body detector fails in the LOOSE direction: pure reading classifies;
/// any mark of write/subprocess/DB/dynamism returns `None` (the
/// command stays in G10); non-python never classifies.
#[test]
fn o_detector_de_corpo_readonly_falha_na_direcao_frouxa() {
    use crate::code_mode_gates::python_inline_readonly_class;
    let ro = "python3 - <<'EOF'\nimport json\nd = json.load(open('x.json'))\nprint(d)\nEOF";
    assert_eq!(python_inline_readonly_class(ro), Some("python-inline"));
    for w in [
        "python3 -c 'open(\"f\", \"w\").write(\"x\")'",
        "python3 -c 'import subprocess'",
        "python3 -c 'import sqlite3'",
        "python3 -c 'eval(input())'",
        "python3 -c 'import shutil; shutil.rmtree(\"d\")'",
    ] {
        assert_eq!(
            python_inline_readonly_class(w),
            None,
            "escritor tem de ficar fora: {w}"
        );
    }
    assert_eq!(
        python_inline_readonly_class("grep -rn foo src/"),
        None,
        "não-python nunca entra por este classificador"
    );
}

/// Every class in the list has to be one that `scan_class_of` actually emits.
/// Without this the list can name something that never reaches the gate — the failure
/// mode of `teste-do-componente-nao-e-teste-do-caminho`.
#[test]
fn toda_classe_colapsada_e_produzivel_pelo_classificador() {
    let amostras = [
        ("grep", "grep -rn foo src/"),
        ("cat", "cat README.md"),
        ("find", "find . -name x.rs"),
        ("ls", "ls -la src/"),
        ("wc", "wc -l README.md"),
        ("sed-n", "sed -n 1,20p README.md"),
    ];
    for (classe, cmd) in amostras {
        assert_eq!(scan_class_of(cmd), Some(classe), "classificador: {cmd}");
    }
    for classe in CODE_MODE_COLLAPSED_CLASSES {
        assert!(
            amostras.iter().any(|(c, _)| c == classe),
            "classe `{classe}` colapsa sem amostra que a produza"
        );
    }
}

/// P2.3 (calibration, 1,312 real calls): `cat > f`/`cat >> f` is
/// a WRITE heredoc (T4), not inspection. Without the exclusion, the `code` mode —
/// which denies on the 1st call — denied 27 writes (20.6% of what the matrix
/// would catch). Proven by mutation: removing the redirect arm in
/// `scan_class_of` fails the first three cases.
#[test]
fn cat_com_redirect_e_escrita_nao_inspecao() {
    // write: first operand is already the redirect
    assert_eq!(scan_class_of("cat > out.txt <<'EOF'"), None);
    assert_eq!(scan_class_of("cat >> log.txt <<'EOF'"), None);
    assert_eq!(scan_class_of("head > out.txt"), None);
    assert_eq!(scan_class_of("tail -n +3 >> out.txt"), None);
    // inspection preserved
    assert_eq!(scan_class_of("cat README.md"), Some("cat"));
    assert_eq!(scan_class_of("cat -n README.md"), Some("cat"));
    // inspection whose RESULT goes to a file: the read operand comes
    // first — the fused program would reproduce the redirect
    assert_eq!(scan_class_of("cat README.md > out.txt"), Some("cat"));
    assert_eq!(scan_class_of("head -5 README.md"), Some("cat"));
}

/// S1 (26/08/2026) — execution wrappers are transparent to
/// classification: `time grep` is `grep`. Before, the 1st token decided, and
/// `time`/`nice`/`sudo`/`env` nullified the class for gates AND nudges at
/// the same time. Each assert fails a mutation of the resolver (removing the
/// shared lexer's wrapper skip in `command_of` returns `None` in all the positive cases).
#[test]
fn s1_wrappers_resolve_o_verbo_real() {
    assert_eq!(scan_class_of("time grep -rn foo src/"), Some("grep"));
    assert_eq!(scan_class_of("nice -n 5 find . -name x.rs"), Some("find"));
    assert_eq!(scan_class_of("sudo cat README.md"), Some("cat"));
    assert_eq!(scan_class_of("env FOO=1 rg pattern"), Some("grep"));
    assert_eq!(scan_class_of("FOO=1 time grep x"), Some("grep"));
    assert_eq!(scan_class_of("timeout 30 ls -la"), Some("ls"));
    assert_eq!(scan_class_of("stdbuf -o0 cat f"), Some("cat"));
    assert_eq!(scan_class_of("command grep x f"), Some("grep"));
    assert_eq!(scan_class_of("sudo -u root env A=1 wc -l f"), Some("wc"));
    assert_eq!(scan_class_of("time sed -n 5p f"), Some("sed-n"));
    assert_eq!(scan_class_of("chrt -f 10 nice -n 5 grep x f"), Some("grep"));
}

/// The pre-S1 exclusions cross the wrapper: a write is still a write,
/// a bare wrapper is not a class, and the `nice`'s `-n` does NOT become the sed's `-n`.
#[test]
fn s1_wrappers_preservam_as_exclusoes() {
    assert_eq!(scan_class_of("nice cat > out.txt <<'EOF'"), None);
    assert_eq!(scan_class_of("sudo head > out.txt"), None);
    assert_eq!(scan_class_of("nice -n 5 sed 5p f"), None);
    assert_eq!(scan_class_of("time"), None);
    assert_eq!(scan_class_of("env"), None);
    assert_eq!(scan_class_of("sudo"), None);
    assert_eq!(scan_class_of("env FOO=1"), None);
}

/// `VAR=value` prefix: pre-S1 behavior, bit for bit — the bypass
/// `TOURING_GATE_OK=1`/`TOURING_CODE_MODE=<v>` stays transparent.
#[test]
fn s1_prefixo_var_valor_segue_transparente() {
    assert_eq!(scan_class_of("FOO=1 grep x f"), Some("grep"));
    assert_eq!(
        scan_class_of("TOURING_CODE_MODE=native grep x f"),
        Some("grep")
    );
    assert_eq!(scan_class_of("FOO=1"), None);
}

/// A call with no fan-out class neither opens a turn nor is denied: mutation/build
/// is never a burst (the §T3-B design is about inspection).
#[test]
fn similaridade_pega_o_fanout_serial_que_os_outros_gates_nao_veem() {
    use crate::code_mode_gates::{G1_SIMILARITY_AT, command_similarity};
    // The canonical case: same inspection, different file. G6 does not catch it
    // (not byte-identical) and G7 does not catch it (different file).
    let s = command_similarity("grep X a.rs", "grep X b.rs");
    assert!(s >= G1_SIMILARITY_AT, "esperado >= 0.5, obtido {s}");

    // Same family, different range — almost all the work is repeated.
    let s2 = command_similarity("sed -n 1,20p f.rs", "sed -n 40,60p f.rs");
    assert!(s2 >= G1_SIMILARITY_AT, "esperado >= 0.5, obtido {s2}");
}

/// No false positive: commands of distinct purposes cannot be
/// judged a repetition. A gate that denied this would be worse than none.
#[test]
fn comandos_diferentes_nao_sao_similares() {
    use crate::code_mode_gates::{G1_SIMILARITY_AT, command_similarity};
    for (a, b) in [
        ("grep X src/", "cargo test -p touring-cli"),
        ("cat /a/b.rs", "touring doctor -j"),
        ("", "grep X src/"),
    ] {
        let s = command_similarity(a, b);
        assert!(s < G1_SIMILARITY_AT, "{a:?} vs {b:?} não deviam casar: {s}");
    }
}

#[test]
fn ausencia_e_valor_invalido_caem_no_default() {
    let vazio = tmpdir("vazio");
    std::fs::create_dir_all(&vazio).expect("mkdir");
    assert_eq!(project_presentation(&vazio), None);
    let _ = std::fs::remove_dir_all(&vazio);

    let invalido = tmpdir("invalido");
    escreve_config(&invalido, "[code_mode]\nmode = \"agressivo\"\n");
    assert_eq!(project_presentation(&invalido), None);
    let _ = std::fs::remove_dir_all(&invalido);
}

/// The central invariant: every command that ENTERS the body enters whole.
///
/// Stated as a positive assertion over ALL the commands, not as the
/// absence of a known case — the lesson of `every_derivable_nudge_...`:
/// an invariant verified on one instance recurs on the next uncovered one.
#[test]
fn nenhum_comando_entra_pela_metade() {
    // Exceeds the budget ALONE (22 chars × 200 ≈ 4400 > 3000), which is the
    // only way to force the omission — the first version of this test used
    // ×60 (~1334 chars), it fit, and the `omitidos > 0` failed the TEST, not the
    // code: the "nothing halfway" invariant had held.
    let longo = format!("grep -rn \"x\" {}", "crates/touring-ceg/src/".repeat(200));
    let cmds = vec![
        "grep -n foo a.rs".to_string(),
        longo,
        "grep -n bar b.rs".to_string(),
    ];
    let (corpo, omitidos) = fuse_burst_program(&cmds);
    for cmd in &cmds {
        let inteiro = corpo.contains(&cmd.replace('\'', "'\\''"));
        let prefixo: String = cmd.chars().take(24).collect();
        let ausente = !corpo.contains(&prefixo);
        assert!(
            inteiro || ausente,
            "comando entrou pela metade — nem inteiro nem ausente"
        );
    }
    assert!(omitidos > 0, "o comando gigante devia ter sido omitido");
}

#[test]
fn corpo_respeita_o_orcamento() {
    let cmds: Vec<String> = (0..8)
        .map(|i| format!("grep -rn \"pat{i}\" {}", "crates/x/".repeat(50)))
        .collect();
    let (corpo, omitidos) = fuse_burst_program(&cmds);
    assert!(corpo.chars().count() <= G1_BODY_BUDGET);
    assert!(omitidos > 0, "com 8 comandos gigantes algo tem de sobrar");
}

#[test]
fn rajada_normal_entra_inteira_e_nada_e_omitido() {
    let cmds = vec![
        "grep -n alpha a.rs".to_string(),
        "grep -n beta b.rs".to_string(),
        "grep -n gamma c.rs".to_string(),
    ];
    let (corpo, omitidos) = fuse_burst_program(&cmds);
    assert_eq!(omitidos, 0);
    for cmd in &cmds {
        assert!(corpo.contains(cmd), "faltou {cmd}");
    }
    assert_eq!(corpo.lines().count(), 3);
}

/// The single quotes of the original command have to survive the wrapping in
/// `--code '<body>'`, otherwise the delivered program breaks on the first quote.
#[test]
fn aspas_simples_sao_escapadas_para_o_embrulho() {
    let cmds = vec!["grep -rn 'foo bar' src/".to_string()];
    let (corpo, _) = fuse_burst_program(&cmds);
    assert!(corpo.contains("'\\''foo bar'\\''"), "corpo: {corpo}");
    let programa = format!("touring run --lang bash --code '{corpo}'");
    assert!(programa.ends_with('\''));
    assert!(programa.starts_with("touring run --lang bash --code '"));
}

#[test]
fn ready_bypass_line_recusa_o_que_nao_cabe_numa_linha() {
    assert_eq!(
        ready_bypass_line("  cargo test -p x \n").as_deref(),
        Some("TOURING_GATE_OK=1 cargo test -p x")
    );
    assert_eq!(ready_bypass_line("   "), None);
    assert_eq!(ready_bypass_line("a\nb"), None);
    assert_eq!(
        ready_bypass_line(&"y".repeat(READY_LINE_MAX_CHARS + 1)),
        None
    );
    assert!(ready_bypass_line(&"y".repeat(READY_LINE_MAX_CHARS)).is_some());
}

#[test]
fn g3_read_window_cobre_o_old_string_com_contexto() {
    let (tmp, fp) = projeto_com_arquivo_grande();
    let (p, f) = (tmp.path(), fp.to_str().expect("utf8"));
    assert_eq!(g3_read_window(p, f, "linha 120"), Some((115, 11)));
    // multiline: 2 lines + 5 of context on each side
    assert_eq!(g3_read_window(p, f, "linha 50\nlinha 51"), Some((45, 12)));
    // near the start the offset sticks at 1
    assert_eq!(g3_read_window(p, f, "linha 2\n"), Some((1, 7)));
    // path relative to the project
    assert_eq!(g3_read_window(p, "grande.rs", "linha 120"), Some((115, 11)));
    // nothing to window
    assert_eq!(g3_read_window(p, f, ""), None);
    assert_eq!(g3_read_window(p, f, "não existe"), None);
    assert_eq!(g3_read_window(p, "nao-existe.rs", "linha 120"), None);
    // a window that is the whole file saves nothing
    let tudo: String = (1..=200).map(|i| format!("linha {i}\n")).collect();
    assert_eq!(g3_read_window(p, f, &tudo), None);
}

#[test]
fn portfolio_cache_reloads_when_the_index_file_changes() {
    // Audit finding F4: a plain OnceLock never saw `touring portfolio refresh`,
    // so an in-daemon hook asserted stale prior art indefinitely. The cache key
    // is the file's mtime, so this asserts the invalidation path is reached.
    //
    // The index is a REAL file any live daemon hook may rewrite at any moment
    // (`portfolio refresh`), so the premise — "unchanged file" — has to be
    // MEASURED, not assumed. Asserting agreement without checking the mtime
    // made this test fail whenever a hook refreshed the index between the two
    // reads (observed 20/09/2026 under the full suite, green in isolation):
    // it reported a defect where the cache had done exactly its job.
    let mtime_before = portfolio_mtime();
    let first = portfolio_index();
    let second = portfolio_index();
    let mtime_after = portfolio_mtime();

    if mtime_before != mtime_after {
        // The file changed under us: divergence here is the invalidation
        // working, not a bug. Nothing to assert about equality.
        return;
    }

    match (first, second) {
        (Some(a), Some(b)) => assert_eq!(
            a.entries.len(),
            b.entries.len(),
            "two reads of an index whose mtime did not move must agree"
        ),
        (None, None) => {}
        _ => {
            panic!("cache returned inconsistently while the index mtime stayed at {mtime_before:?}")
        }
    }
}

#[test]
fn lista_vazia_devolve_corpo_vazio_sem_omissao() {
    let (corpo, omitidos) = fuse_burst_program(&[]);
    assert!(corpo.is_empty());
    assert_eq!(omitidos, 0);
}

// ── Retro 11/11/2026 (c): os arquivos que um comando Bash LÊ, pelo caminho exato ───────────

/// Arquivos reais num projeto temporário, e os caminhos que `bash_read_targets` devolve para
/// cada forma de leitura — com os controles negativos (o que NÃO conta) ao lado.
#[test]
fn bash_read_targets_names_only_the_exact_files_a_command_reads() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    std::fs::create_dir_all(root.join("src")).expect("src");
    for f in ["src/a.rs", "src/b.rs", "notes.txt"] {
        std::fs::write(root.join(f), "x\n").expect("file");
    }
    let a = root.join("src/a.rs");
    let b = root.join("src/b.rs");
    let at = |cmd: &str| bash_read_targets(root, cmd);

    // as formas que leem o arquivo exato
    let abs = |tail: &str| format!("{tail} {}", a.display());
    for cmd in [
        abs("grep -n needle"),
        abs("rg -n needle"),
        abs("egrep -c needle"),
        abs("cat"),
        abs("head -n 20"),
        abs("tail -n +5"),
        abs("sed -n '1,5p'"),
        format!("sed -n -e '1,5p' {}", a.display()),
        format!("sed --quiet '3p' {}", a.display()),
        "grep -n needle src/a.rs".to_string(),
        format!("cd {}/src && sed -n '1,5p' a.rs", root.display()),
        format!("grep -n 'a b' \"{}\"", a.display()),
        format!("grep -e needle -r {}", a.display()),
        format!("grep -n needle {} | head -3", a.display()),
        format!("TOURING_CODE_MODE=native grep -n needle {}", a.display()),
    ] {
        assert_eq!(at(&cmd), vec![a.clone()], "deveria ler a.rs: {cmd}");
    }
    // vários arquivos de um comando só, em ordem
    assert_eq!(
        at(&format!("head -n 3 {} {}", a.display(), b.display())),
        vec![a.clone(), b.clone()]
    );
    // o `..` e o `.` se resolvem lexicalmente
    assert_eq!(at("cat src/../src/./a.rs"), vec![a.clone()]);

    // controles negativos: nada disto é a leitura de um arquivo exato
    for cmd in [
        format!("grep -rn needle {}/src", root.display()), // diretório
        format!("grep -n needle {}/src/*.rs", root.display()), // glob
        format!("grep -n needle {}/src/ausente.rs", root.display()), // não existe
        format!("ls -l {}", a.display()),                  // não é verbo de leitura
        format!("sed -i 's/x/y/' {}", a.display()),        // escrita no lugar
        format!("sed -n -i '1p' {}", a.display()),         // -i vence o -n
        format!("sed 's/x/y/' {}", a.display()),           // sem -n: não é um recorte
        format!(
            "grep -f {} {}",
            b.display(),
            root.join("notes.txt").display()
        ), // -f: padrões de arquivo; só o alvo conta
        format!("echo {}", a.display()),
        format!("cat > {}", a.display()), // redirecionamento é escrita
        "grep needle -".to_string(),
        "grep needle".to_string(),
    ] {
        let got = at(&cmd);
        assert!(
            !got.contains(&a),
            "NÃO deveria contar a.rs: {cmd} → {got:?}"
        );
    }
    // o -f troca o padrão por um arquivo: o 1º posicional já é um caminho
    assert_eq!(
        at(&format!("grep -f {} {}", b.display(), a.display())),
        vec![a.clone()]
    );
}
