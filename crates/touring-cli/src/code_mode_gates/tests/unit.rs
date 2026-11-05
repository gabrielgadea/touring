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
    // inline entrou na rajada como classe PRÓPRIA (aperto 29/08): o
    // remédio é 1:1 (o corpo verbatim), não a fusão R9 que justificava a
    // exclusão histórica
    assert_eq!(
        exec_class_of("python3 -c 'print(1)'"),
        Some("python-inline")
    );
    assert_eq!(
        exec_class_of("python3 - <<'EOF'\nprint(1)\nEOF"),
        Some("python-inline")
    );
    assert_eq!(exec_class_of("python3 - arg1"), Some("python-inline"));
    // mutação marcada fica de fora por construção (P2.3)
    assert_eq!(exec_class_of("python3 setup.py install"), None);
    assert_eq!(exec_class_of("python3 runner.py > out.txt"), None);
    assert_eq!(exec_class_of("python3 -m pip install x"), None);
    // não-executor
    assert_eq!(exec_class_of("cargo test"), None);
    assert_eq!(exec_class_of("grep foo f"), None);
}

#[test]
fn s5_redirect_de_fd_nao_anula_a_classe_exec() {
    use crate::code_mode_gates::exec_class_of;
    // O furo B: `contains(">")` sobre o blob inteiro cegava o G10.
    assert_eq!(
        exec_class_of("cd /tmp && python3 medir.py 2>&1 | head -5"),
        Some("python")
    );
    assert_eq!(
        exec_class_of("python3 medir.py 2>/dev/null"),
        Some("python")
    );
    assert_eq!(exec_class_of("pytest tests/ -q 2>&1"), Some("pytest"));
    // Redirect REAL de saída segue fora — escrita não é rajada exec.
    assert_eq!(exec_class_of("python3 runner.py > out.txt"), None);
    assert_eq!(exec_class_of("python3 runner.py >> log.txt"), None);
}

#[test]
fn s5_assignment_prefixo_e_transparente_para_a_classe() {
    use crate::code_mode_gates::{effective_tokens, exec_class_of};
    // O furo C: segmento assignment-only devolvia tokens vazios e TODA
    // classificação morria ali.
    assert_eq!(
        exec_class_of("P=docs/x\npython3 medir.py --map $P"),
        Some("python")
    );
    assert_eq!(
        effective_tokens("P=docs/x\nR=docs/y\ngrep -n foo arq.rs").first(),
        Some(&"grep")
    );
    // Assignment sozinho continua sem classe (não há verbo).
    assert_eq!(exec_class_of("P=docs/x"), None);
}

#[test]
fn s5_corpo_de_heredoc_nao_alimenta_o_filtro_de_mutacao() {
    use crate::code_mode_gates::{exec_class_of, mutation_scan_view};
    // O corpo é DADO no stdin — um `git ` ou `>` lá dentro não é comando.
    let cmd = "python3 medir.py <<'EOF'\nx = \"git checkout\"\ny = 1 > 0\nEOF";
    assert!(!mutation_scan_view(cmd).contains("git "));
    assert_eq!(exec_class_of(cmd), Some("python"));
    // Herestring não é heredoc — a view não pode engolir o resto.
    assert!(mutation_scan_view("cat <<< 'x' && rm f").contains("rm "));
}

#[test]
fn s5_par_write_run_detecta_escrita_e_execucao_do_mesmo_script() {
    use crate::code_mode_gates::{script_run_targets, script_write_target};
    // A escrita registra o alvo (heredoc típico do loop execute-observe).
    assert_eq!(
        script_write_target("cat > /tmp/s/medir.py <<'PYEOF'\nprint(1)\nPYEOF"),
        Some("/tmp/s/medir.py".to_string())
    );
    assert_eq!(
        script_write_target("cd /proj\ncat > scratch/x.sh <<'EOF'\nls\nEOF"),
        Some("scratch/x.sh".to_string())
    );
    assert_eq!(script_write_target("cat > notas.md <<'EOF'\noi\nEOF"), None);
    // A execução resolve o path mesmo atrás de cd/assignment.
    assert_eq!(
        script_run_targets("cd /proj\n.venv/bin/python3 /tmp/s/medir.py"),
        vec!["/tmp/s/medir.py".to_string()]
    );
    assert_eq!(
        script_run_targets("bash scratch/x.sh"),
        vec!["scratch/x.sh".to_string()]
    );
    // O padrão CANÔNICO do analise (24 dos 60): escreve E executa no MESMO
    // tool_use multi-linha — o run da linha pós-heredoc TEM que aparecer.
    assert_eq!(
        script_run_targets(
            "cat > /tmp/s/perfil.py <<'PYEOF'\nimport json\nPYEOF\n\npython3 /tmp/s/perfil.py"
        ),
        vec!["/tmp/s/perfil.py".to_string()]
    );
    // pytest/módulos nunca casam — o limiar baixo não taxa o caso comum.
    assert!(script_run_targets("python3 -m pytest tests/test_a.py").is_empty());
    assert!(script_run_targets("pytest tests/test_a.py").is_empty());
    // Redirect real no SEGMENTO do run: o remédio --file não reproduziria
    // a escrita — aquele segmento não conta.
    assert!(script_run_targets("python3 /tmp/s/medir.py > out.json").is_empty());
}

#[test]
fn s5_python_inline_tem_remedio_um_para_um() {
    use crate::code_mode_gates::{python_inline_body, python_inline_remedy};
    // heredoc: o corpo verbatim, sem fusão R9
    let cmd = "python3 - <<'PYEOF'\nimport json\nprint(1)\nPYEOF";
    assert_eq!(
        python_inline_body(cmd).as_deref(),
        Some("import json\nprint(1)")
    );
    let r = python_inline_remedy(cmd);
    assert!(r.starts_with("touring run --lang python --code '"), "{r}");
    assert!(r.contains("import json"), "{r}");
    // -c: o literal entre as aspas externas
    assert_eq!(
        python_inline_body("python3 -c 'print(42)'").as_deref(),
        Some("print(42)")
    );
    // inextraível → placeholder honesto, nunca comando quebrado
    assert!(python_inline_remedy("python3 -").contains("verbatim"));
}

/// S213 (25/09/2026) — quem ANEXA a arquivo nunca é inspeção. O remédio da rajada funde
/// os comandos da janela e os reexecuta; `cat x >> triad_hook.rs && cargo test` foi
/// classificado como `cat` e o deny propôs rodá-lo de novo (anexaria o teste duas vezes).
/// Sobrescrever (`>`) segue inspeção — decisão P2.3: reexecutar é idempotente.
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

/// S213b (26/09/2026) — uma SEQUÊNCIA só é inspeção se todo passo inspeciona. A classe
/// vinha do 1º verbo real: `cd t && grep -m1 x Cargo.toml; timeout 1200 cargo clippy …`
/// virou `grep` e o remédio da rajada propôs reexecutar 20 min de clippy (observado ao
/// vivo); com um `touring memory store` no lugar, reexecutaria uma escrita.
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

/// S212 (25/09/2026) — o remédio do G10 NUNCA reexecuta o que já rodou. O ledger traz a
/// rajada inteira em ordem; só o ÚLTIMO comando foi negado — os anteriores JÁ executaram,
/// inclusive escritas (gravar memória, commitar). Reexecutá-los repetia efeitos: observado
/// ao vivo, o deny propôs rodar de novo o script que grava lições. Só o negado vira passo;
/// os executados viajam como comentário de contexto (a 1ª linha, nunca o corpo).
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
    // 30/08: o R9 é BASH — o template python com `import subprocess` era
    // exatamente a capability que o X6 nega sob Sandboxed: o deny do G10
    // emitia uma rota que o gate seguinte barrava (medido pela peer
    // analise-a2 — não havia caminho conforme). Bash é a única lang com o
    // waiver subprocess-only; `--timeout-ms` viaja explícito (E4).
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
        // o comando viaja VERBATIM, com o escape do embrulho shell por
        // cima (o bash externo desfaz na entrega)
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
        "pytest tests/test_a.py -x".to_string(), // duplicado: dedup
    ];
    let intent = exec_burst_intent("pytest", &cmds);
    assert!(intent.starts_with("pytest"), "{intent}");
    // componentes de trabalho (dir + stem), não o caminho inteiro — são
    // eles que casam o propósito do artefato (required_matches do BM25)
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
    // classe solitária quando a rajada não tem alvos
    assert_eq!(exec_burst_intent("python", &[]), "python");
}

#[test]
fn s6_sem_portfolio_ou_sem_match_o_remedio_e_none_e_o_r9_basta() {
    use crate::code_mode_gates::portfolio_remedy_for_burst;
    // fail-open: sem índice legível (ou vazio), o deny sai só com o R9
    let cmds = vec!["pytest tests/xyzzy_inexistente_q123.py".to_string()];
    let _ = portfolio_remedy_for_burst("pytest", &cmds); // nunca panica
}

#[test]
fn s6_prior_art_instanciado_quando_o_portfolio_tem_programa_proximo() {
    // O índice vem do scratch por PARÂMETRO (store::load_from), sem
    // TOURING_PORTFOLIO_DIR: o env é global ao processo e vaza entre
    // testes paralelos (a race medida em 08/10 — ver GateOverrides).
    use crate::code_mode_gates::portfolio_remedy_for_burst_on;
    let dir = monta_portfolio_scratch();
    let index = touring_foundation::portfolio::store::load_from(&dir)
        .expect("índice do scratch carrega");
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
    // O caso medido no transcript de 25/08: laço de inspeção pura.
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
    // Estes quebrariam sob as capabilities do sandbox — ficam de fora por
    // construção, não por sorte. O gate que converte o que não devia é
    // pior que gate nenhum.
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
    // O corpo entra escapado, de modo que o shell externo reentregue o
    // comando original ao sandbox — o rewrite não pode alterar semântica.
    assert!(
        got.contains(r"'\''fn main'\''"),
        "aspas internas escapadas: {got}"
    );
}

/// S3 (27/08/2026) — a calibração deixou de ser por NOME de classe e passou
/// a ser por RAJADA, e a medição é a razão.
///
/// A versão anterior deste teste afirmava o oposto: que `ls`/`wc`/`sed-n`
/// NÃO podiam colapsar porque "são chamada única". Medindo 115 transcripts
/// (`scripts/s3_burst_distribution.py`, janela 300s) isso se mostrou falso
/// justamente para as duas de maior volume — `sed-n` tem 408 chamadas com
/// 81,4% do volume em rajadas ≥2, `ls` tem 338 com 71,3% — enquanto `find`,
/// que a lista negava, é 56,2% isolada e não produziu UMA rajada ≥3.
///
/// O que o teste guarda agora é o invariante que sobrevive à recalibração:
/// toda classe que o gate acompanha é uma que o classificador emite, e o
/// que discrimina não é a lista e sim o limiar de rajada.
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

/// O detector de corpo falha na direção FROUXA: leitura pura classifica;
/// qualquer marca de escrita/subprocesso/DB/dinamismo devolve `None` (o
/// comando fica no G10); não-python nunca classifica.
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

/// Toda classe da lista tem de ser uma que `scan_class_of` realmente emite.
/// Sem isto a lista pode nomear algo que jamais chega ao gate — o modo de
/// falha de `teste-do-componente-nao-e-teste-do-caminho`.
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

/// P2.3 (calibração, 1.312 chamadas reais): `cat > f`/`cat >> f` é
/// heredoc de ESCRITA (T4), não inspeção. Sem a exclusão, o modo `code` —
/// que nega na 1ª chamada — negava 27 escritas (20,6% do que a matriz
/// pegaria). Provado por mutação: remover o braço do redirect em
/// `scan_class_of` reprova os três primeiros casos.
#[test]
fn cat_com_redirect_e_escrita_nao_inspecao() {
    // escrita: primeiro operando já é o redirect
    assert_eq!(scan_class_of("cat > out.txt <<'EOF'"), None);
    assert_eq!(scan_class_of("cat >> log.txt <<'EOF'"), None);
    assert_eq!(scan_class_of("head > out.txt"), None);
    assert_eq!(scan_class_of("tail -n +3 >> out.txt"), None);
    // inspeção preservada
    assert_eq!(scan_class_of("cat README.md"), Some("cat"));
    assert_eq!(scan_class_of("cat -n README.md"), Some("cat"));
    // inspeção cujo RESULTADO vai a arquivo: o operando de leitura vem
    // primeiro — o programa fundido reproduziria o redirect
    assert_eq!(scan_class_of("cat README.md > out.txt"), Some("cat"));
    assert_eq!(scan_class_of("head -5 README.md"), Some("cat"));
}

/// S1 (26/08/2026) — wrappers de execução são transparentes à
/// classificação: `time grep` é `grep`. Antes o 1º token decidia, e
/// `time`/`nice`/`sudo`/`env` anulavam a classe para gates E nudges ao
/// mesmo tempo. Cada assert reprova uma mutação do resolvedor (remover
/// `skip_wrapper` devolve `None` em todos os casos positivos).
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

/// As exclusões pré-S1 atravessam o invólucro: escrita continua escrita,
/// invólucro puro não é classe, e o `-n` do `nice` NÃO vira o `-n` do sed.
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

/// Prefixo `VAR=valor`: o comportamento pré-S1, bit a bit — o bypass
/// `TOURING_GATE_OK=1`/`TOURING_CODE_MODE=<v>` segue transparente.
#[test]
fn s1_prefixo_var_valor_segue_transparente() {
    assert_eq!(scan_class_of("FOO=1 grep x f"), Some("grep"));
    assert_eq!(
        scan_class_of("TOURING_CODE_MODE=native grep x f"),
        Some("grep")
    );
    assert_eq!(scan_class_of("FOO=1"), None);
}

/// Chamada sem classe fan-out não abre turno nem é negada: mutação/build
/// nunca é rajada (o desenho §T3-B é sobre inspeção).
#[test]
fn similaridade_pega_o_fanout_serial_que_os_outros_gates_nao_veem() {
    use crate::code_mode_gates::{G1_SIMILARITY_AT, command_similarity};
    // O caso canônico: mesma inspeção, arquivo diferente. O G6 não pega
    // (não é byte-idêntico) e o G7 não pega (arquivo diferente).
    let s = command_similarity("grep X a.rs", "grep X b.rs");
    assert!(s >= G1_SIMILARITY_AT, "esperado >= 0.5, obtido {s}");

    // Mesma família, faixa diferente — o trabalho repetido é quase todo.
    let s2 = command_similarity("sed -n 1,20p f.rs", "sed -n 40,60p f.rs");
    assert!(s2 >= G1_SIMILARITY_AT, "esperado >= 0.5, obtido {s2}");
}

/// Sem falso positivo: comandos de propósitos distintos não podem ser
/// julgados repetição. Um gate que negasse isto seria pior que nenhum.
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

/// A invariante central: todo comando que ENTRA no corpo entra inteiro.
///
/// Enunciada como asserção positiva sobre TODOS os comandos, não como
/// ausência de um caso conhecido — a lição de `every_derivable_nudge_...`:
/// invariante verificada numa instância recorre na próxima não coberta.
#[test]
fn nenhum_comando_entra_pela_metade() {
    // Excede o orçamento SOZINHO (22 chars × 200 ≈ 4400 > 3000), que é a
    // única forma de forçar a omissão — a primeira versão deste teste usava
    // ×60 (~1334 chars), cabia, e o `omitidos > 0` reprovou o TESTE, não o
    // código: a invariante do "nada pela metade" havia segurado.
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

/// As aspas simples do comando original têm de sobreviver ao embrulho em
/// `--code '<corpo>'`, senão o programa entregue quebra na primeira aspa.
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
    // multilinha: 2 linhas + 5 de contexto de cada lado
    assert_eq!(g3_read_window(p, f, "linha 50\nlinha 51"), Some((45, 12)));
    // perto do começo o offset trava em 1
    assert_eq!(g3_read_window(p, f, "linha 2\n"), Some((1, 7)));
    // caminho relativo ao projeto
    assert_eq!(g3_read_window(p, "grande.rs", "linha 120"), Some((115, 11)));
    // sem o que janelar
    assert_eq!(g3_read_window(p, f, ""), None);
    assert_eq!(g3_read_window(p, f, "não existe"), None);
    assert_eq!(g3_read_window(p, "nao-existe.rs", "linha 120"), None);
    // a janela que é o arquivo inteiro não economiza nada
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
