//! RAWRUN (11/11/2026) — o que volta ao modelo quando o hook REESCREVE um Bash em `touring run`.
//!
//! Caso real (sessão Claude Code neste workspace, `[code_mode] mode = "code"`): a ferramenta Bash
//! recebeu `touring decompose --help … | head -60; for s in create ticket claim; do …; done`; o G8
//! (laço de inspeção) a reescreveu para `touring run --lang bash --timeout-ms … --code '…'`, e o
//! modelo recebeu o ENVELOPE JSON do run (`run_id`, `ceg_advisory`, `forbidden_calls`, o stdout
//! como UMA string escapada) precedido do log `INFO ceg::enforcement`. O programa não tinha
//! mudado; a apresentação, sim — mais tokens, pior leitura, e um guarda a jusante casou a chave
//! `forbidden_calls` do próprio envelope.
//!
//! Este teste fecha o laço pelas duas pontas, como o `g10_trusted_route_e2e`: o hook REAL
//! (`cli_suggester::run`) devolve o comando reescrito, e o binário `touring` REAL o executa.
//! O que se afirma: reescrita => texto cru no stdout, stderr cru no stderr, exit do programa;
//! chamada explícita (sem a marca) => o envelope de sempre.

use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};
use touring_hooks::cli_suggester;
use touring_hooks::runtime::HookRuntime;

const TOURING: &str = env!("CARGO_BIN_EXE_touring");

/// A forma da linha do caso real: `cd <proj> && <inspeção> 2>&1 | head; for … do …; done`.
///
/// O caso real chamava `touring decompose --help`; aqui o programa é `echo "Usage: …"` porque o
/// sandbox só executa o que está sob a raiz do projeto (Landlock) e o projeto deste teste é um
/// tmpdir hermético — o binário de teste fica em `target/`, fora dele. O que o teste mede é a
/// APRESENTAÇÃO do resultado, e ela não depende de qual inspeção o laço faz.
fn linha_real(proj: &Path) -> String {
    format!(
        "cd {} && echo \"Usage: touring decompose\" 2>&1 | head -60; for s in create ticket claim; do echo \"=== $s\"; echo \"Usage: touring decompose $s\" 2>&1 | head -30; done",
        proj.display()
    )
}

fn projeto() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(tmp.path().join(".claude/data")).expect("mkdir .claude/data");
    std::fs::create_dir_all(tmp.path().join(".touring")).expect("mkdir .touring");
    std::fs::write(
        tmp.path().join(".touring/touring.toml"),
        "[code_mode]\nmode = \"code\"\n",
    )
    .expect("touring.toml");
    tmp
}

/// O hook REAL sobre uma chamada Bash: o `command` do `updatedInput`, quando há reescrita.
fn reescrita(rt: &HookRuntime, sessao: &str, cmd: &str) -> Option<String> {
    let saida = cli_suggester::run(
        rt,
        &json!({
            "tool_name": "Bash",
            "session_id": sessao,
            "tool_input": { "command": cmd }
        }),
    );
    let v: Value = serde_json::from_str(&saida).ok()?;
    let h = v.get("hookSpecificOutput")?;
    if h.get("permissionDecision")?.as_str() != Some("allow") {
        return None;
    }
    h.get("updatedInput")?
        .get("command")?
        .as_str()
        .map(str::to_owned)
}

/// Roda `comando` num shell, com `touring` no PATH, dentro do projeto e SEM daemon. Devolve
/// `(exit, stdout, stderr)` SEPARADOS — a separação é o que o teste mede.
fn roda(projeto: &Path, comando: &str) -> (i32, String, String) {
    let bin_dir = Path::new(TOURING).parent().expect("dir do binário");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    // HOME temporário: o journal do run vem do HOME (T-2, 31/10/2026) — nunca o do usuário.
    let home = projeto.join("home-temp");
    std::fs::create_dir_all(&home).expect("HOME temporário");
    let out = Command::new("bash")
        .arg("-c")
        .arg(comando)
        .current_dir(projeto)
        .env("HOME", &home)
        .env("PATH", path)
        .env("TOURING_DAEMON_SOCKET", projeto.join("sem-daemon.sock"))
        .env("TOURING_DAEMON_SOCK", projeto.join("sem-daemon.sock"))
        .env_remove("CLAUDE_PROJECT_DIR")
        .env_remove("TOURING_PROJECT_ROOT")
        .env_remove("RUST_LOG")
        .env("TOURING_REPEAT_GATE", "off")
        .output()
        .expect("bash");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// O stdout é TEXTO do programa: não é um objeto JSON e não carrega as chaves do envelope.
fn assert_texto_cru(stdout: &str) {
    assert!(
        serde_json::from_str::<Value>(stdout)
            .map(|v| v.is_object())
            .unwrap_or(false)
            .eq(&false),
        "o stdout não pode ser um envelope JSON: {stdout}"
    );
    for chave in [
        "\"run_id\"",
        "\"forbidden_calls\"",
        "\"ceg_advisory\"",
        "\"duration_ms\"",
    ] {
        assert!(
            !stdout.contains(chave),
            "o stdout vazou a chave do envelope {chave}: {stdout}"
        );
    }
    assert!(
        !stdout.contains("ceg::enforcement"),
        "o stdout vazou o log de enforcement: {stdout}"
    );
}

#[test]
fn a_linha_real_reescrita_volta_como_texto_cru() {
    let tmp = projeto();
    let rt = HookRuntime::new(tmp.path()).expect("hook runtime");
    let cmd = linha_real(tmp.path());

    let reescrito = reescrita(&rt, "rawrun-e2e-real", &cmd).expect("o G8 reescreve o laço");
    assert!(
        reescrito.starts_with("TOURING_RUN_RAW=1 touring run --lang bash "),
        "a reescrita é um `touring run` marcado como substituição: {reescrito}"
    );

    let (rc, stdout, stderr) = roda(tmp.path(), &reescrito);
    assert_eq!(rc, 0, "o programa saiu 0: stdout={stdout} stderr={stderr}");
    assert_texto_cru(&stdout);
    for marca in ["=== create", "=== ticket", "=== claim", "Usage:"] {
        assert!(
            stdout.contains(marca),
            "o stdout traz o texto real do programa ({marca}): {stdout}"
        );
    }
    // Quebras de linha REAIS, não `\n` escapados dentro de uma string JSON.
    assert!(
        stdout.lines().count() > 3,
        "o texto chega em linhas reais: {stdout:?}"
    );
    assert!(
        !stderr.contains("ceg::enforcement") && !stderr.contains("INFO"),
        "o log INFO ceg::enforcement não vai ao modelo: {stderr}"
    );
}

#[test]
fn o_exit_do_programa_e_o_stderr_cru_atravessam_a_reescrita() {
    let tmp = projeto();
    let rt = HookRuntime::new(tmp.path()).expect("hook runtime");
    let cmd = "for s in a b; do echo \"=== $s\"; echo \"erro-$s\" >&2; done; exit 3";

    let reescrito = reescrita(&rt, "rawrun-e2e-exit", cmd).expect("o G8 reescreve o laço");
    let (rc, stdout, stderr) = roda(tmp.path(), &reescrito);

    assert_eq!(
        rc, 3,
        "o exit do programa é o do processo: {stdout} {stderr}"
    );
    assert_eq!(stdout, "=== a\n=== b\n", "stdout cru, byte a byte");
    assert!(
        stderr.contains("erro-a\nerro-b\n"),
        "o stderr do programa vai ao stderr: {stderr:?}"
    );
    assert!(!stderr.contains("ceg::enforcement"), "{stderr}");
}

#[test]
fn a_chamada_explicita_mantem_o_envelope_json() {
    let tmp = projeto();
    let (rc, stdout, _stderr) = roda(
        tmp.path(),
        "touring run --lang bash --code 'echo ola; exit 4'",
    );

    assert_eq!(rc, 4, "o exit do programa já era propagado: {stdout}");
    let v: Value = serde_json::from_str(stdout.trim_start_matches(|c| c != '{'))
        .unwrap_or_else(|e| panic!("a chamada explícita devolve o envelope JSON ({e}): {stdout}"));
    assert_eq!(v["exit_code"], 4);
    assert_eq!(v["stdout"], "ola\n");
    assert!(v["run_id"].as_str().is_some_and(|s| s.starts_with("run-")));
    assert!(v.get("forbidden_calls").is_some());
}

/// A variável e a flag são UMA chave; `--brief` explícito vence a variável do ambiente.
#[test]
fn a_variavel_do_ambiente_equivale_a_flag_e_o_brief_explicito_vence() {
    let tmp = projeto();
    let (rc, stdout, stderr) = roda(
        tmp.path(),
        "TOURING_RUN_RAW=1 touring run --lang bash --code 'echo ola; exit 5'",
    );
    assert_eq!(rc, 5, "{stdout} {stderr}");
    assert_eq!(stdout, "ola\n", "texto cru, sem envelope");
    assert!(!stderr.contains("ceg::enforcement"), "{stderr}");

    let (rc, stdout, _stderr) = roda(
        tmp.path(),
        "TOURING_RUN_RAW=1 touring run --brief --lang bash --code 'echo ola'",
    );
    assert_eq!(rc, 0);
    let v: Value = serde_json::from_str(stdout.trim_start_matches(|c| c != '{'))
        .unwrap_or_else(|e| panic!("o --brief explícito mantém o envelope ({e}): {stdout}"));
    assert_eq!(v["stdout"], "ola\n");
}

/// `--raw` muda só a FORMA da saída: a política do CEG é a mesma. Um deny (rede) sai igual com
/// e sem a marca — nada roda, stdout vazio, a causa numa linha no stderr, exit 1.
#[test]
fn a_marca_nao_relaxa_a_politica_do_ceg() {
    let tmp = projeto();
    let corpo = "curl -s http://127.0.0.1:1/ ; echo executou";
    let (rc_plain, out_plain, err_plain) = roda(
        tmp.path(),
        &format!("touring run --lang bash --code '{corpo}'"),
    );
    let (rc_raw, out_raw, err_raw) = roda(
        tmp.path(),
        &format!("TOURING_RUN_RAW=1 touring run --lang bash --code '{corpo}'"),
    );
    assert_eq!(rc_plain, 1, "{err_plain}");
    assert_eq!(
        rc_raw, rc_plain,
        "o exit do deny não muda sob a marca: {err_raw}"
    );
    assert!(
        out_plain.is_empty() && out_raw.is_empty(),
        "nada rodou: {out_plain:?} {out_raw:?}"
    );
    for err in [&err_plain, &err_raw] {
        assert!(
            err.contains("denied the network capability 'curl'"),
            "a causa do deny sai no stderr: {err}"
        );
    }
}

#[test]
fn a_saida_truncada_e_dita_numa_linha_curta_no_stderr() {
    let tmp = projeto();
    let (rc, stdout, stderr) = roda(
        tmp.path(),
        "touring run --raw --lang python --code 'print(\"a\" * 20000)'",
    );

    assert_eq!(rc, 0, "{stderr}");
    assert_texto_cru(&stdout);
    assert!(
        stdout.starts_with("aaaa") && stdout.len() < 20_000,
        "stdout é o inline cru, cortado no teto: {} bytes",
        stdout.len()
    );
    let aviso: Vec<&str> = stderr
        .lines()
        .filter(|l| l.contains("[touring run run-"))
        .collect();
    assert_eq!(aviso.len(), 1, "UMA linha que nomeia o run_id: {stderr}");
    assert!(aviso[0].contains("stdout truncated"), "{}", aviso[0]);
}

/// R4.1 — o exit do modo cru é o do COMANDO que o gate substituiu: timeout como `timeout(1)`
/// (124), morte por sinal n como o shell (128+n). O envelope mantém o `exit_code` de sempre.
#[test]
fn o_modo_cru_traduz_timeout_e_sinal_para_o_exit_do_shell() {
    let tmp = projeto();
    let (rc, stdout, stderr) = roda(
        tmp.path(),
        "touring run --raw --lang bash --timeout-ms 1000 --code 'sleep 20'",
    );
    assert_eq!(
        rc, 124,
        "timeout => 124 como o timeout(1): {stdout} {stderr}"
    );
    assert!(
        stderr
            .lines()
            .any(|l| l.starts_with("[touring run run-") && l.contains("timeout")),
        "a linha nomeia a causa e o run_id: {stderr}"
    );

    let (rc, _stdout, stderr) = roda(
        tmp.path(),
        "touring run --raw --lang python --code 'import os, signal; os.kill(os.getpid(), signal.SIGKILL)'",
    );
    assert_eq!(rc, 137, "SIGKILL (9) => 128+9: {stderr}");
    assert!(
        stderr
            .lines()
            .any(|l| l.starts_with("[touring run run-") && l.contains("SIGKILL")),
        "a linha nomeia a causa: {stderr}"
    );

    // o envelope NÃO muda: o mesmo timeout, sem a marca, segue com o exit de sempre.
    let (rc, stdout, _stderr) = roda(
        tmp.path(),
        "touring run --lang bash --timeout-ms 1000 --code 'sleep 20'",
    );
    assert_eq!(rc, 254, "o exit do envelope é o to_wire(-2) de sempre");
    let v: Value = serde_json::from_str(stdout.trim_start_matches(|c| c != '{'))
        .unwrap_or_else(|e| panic!("o envelope segue JSON ({e}): {stdout}"));
    assert_eq!(v["exit_code"], -2);
}

/// R4.2 — o advisory do CEG (deny de subprocesso dispensado) vai para o journal, com o mesmo
/// run_id, em TODOS os modos; o modo cru não imprime nada sobre ele.
#[test]
fn o_advisory_do_ceg_vai_para_o_journal_em_todos_os_modos() {
    let tmp = projeto();
    let journal = tmp
        .path()
        .join("home-temp/.claude/touring/run_journal.jsonl");
    let corpo = "echo a | sed s/a/b/";

    let (rc, stdout, stderr) = roda(
        tmp.path(),
        &format!("TOURING_RUN_RAW=1 touring run --lang bash --code '{corpo}'"),
    );
    assert_eq!(rc, 0, "{stderr}");
    assert_eq!(stdout, "b\n");
    assert!(
        !stdout.contains("advisory") && !stderr.contains("advisory") && !stderr.contains("X6"),
        "o modo cru não diz nada do advisory: {stdout:?} {stderr:?}"
    );
    let linhas: Vec<Value> = std::fs::read_to_string(&journal)
        .expect("o run foi para o journal")
        .lines()
        .map(|l| serde_json::from_str(l).expect("linha do journal"))
        .collect();
    assert_eq!(linhas.len(), 1, "{linhas:?}");
    let advisory = &linhas[0]["ceg_advisory"];
    assert!(
        advisory["reason"]
            .as_str()
            .is_some_and(|r| r.contains("'sed'")),
        "o journal carrega o advisory do run cru: {}",
        linhas[0]
    );
    assert!(advisory["composite"].is_number(), "{advisory}");

    // modo envelope: o MESMO run_id no envelope e no journal, com o advisory nos dois.
    let (rc, stdout, _stderr) = roda(
        tmp.path(),
        &format!("touring run --lang bash --code '{corpo}; true'"),
    );
    assert_eq!(rc, 0);
    let env: Value = serde_json::from_str(stdout.trim_start_matches(|c| c != '{'))
        .unwrap_or_else(|e| panic!("envelope ({e}): {stdout}"));
    assert!(env["ceg_advisory"]["reason"].is_string(), "{env}");
    let ultima: Value = serde_json::from_str(
        std::fs::read_to_string(&journal)
            .expect("journal")
            .lines()
            .last()
            .expect("linha"),
    )
    .expect("json");
    assert_eq!(ultima["run_id"], env["run_id"]);
    assert_eq!(
        ultima["ceg_advisory"]["reason"],
        env["ceg_advisory"]["reason"]
    );
}

/// R4.3 — um deny do CEG aborta antes de existir run_id; no modo cru ainda sai UMA linha curta
/// que nomeia o fato, com exit diferente de zero e sem o log ERROR com timestamp do caminho de erro.
#[test]
fn o_deny_do_ceg_no_modo_cru_e_uma_linha_curta_sem_run_id() {
    let tmp = projeto();
    let (rc, stdout, stderr) = roda(
        tmp.path(),
        "TOURING_RUN_RAW=1 touring run --lang bash --code 'curl -s http://127.0.0.1:1/ ; echo executou'",
    );
    assert_ne!(rc, 0, "{stderr}");
    assert!(stdout.is_empty(), "nada rodou nem foi impresso: {stdout:?}");
    let linhas: Vec<&str> = stderr.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(linhas.len(), 1, "UMA linha no stderr: {stderr}");
    assert!(
        linhas[0].starts_with("[touring run] denied: ")
            && linhas[0].contains("denied the network capability 'curl'"),
        "a linha nomeia o fato: {}",
        linhas[0]
    );
    assert!(
        !stderr.contains("ERROR"),
        "sem o log de erro do dispatcher: {stderr}"
    );
}

/// A nota (b) do review — o deny do repeat_gate também é um deny SEM run_id: no modo cru sai
/// UMA linha `[touring run] denied: …` que ensina a rota, stdout vazio e o mesmo exit (2);
/// o modo envelope segue devolvendo o JSON `repeat_blocked`.
#[test]
fn o_deny_do_repeat_gate_no_modo_cru_e_uma_linha_que_ensina_a_rota() {
    let tmp = projeto();
    // 6 linhas não vazias: abaixo disso o SimHash é ruidoso demais para bloquear.
    let corpo = "a=1\nb=2\nc=3\nd=4\necho $a $b $c $d\necho fim";
    let programa = format!("'{}'", corpo.replace("\\n", "\n"));
    let com_gate = |marca: &str| {
        format!("{marca}TOURING_REPEAT_GATE=block touring run --lang bash --code {programa}")
    };

    // a 1ª execução corre e fica no journal
    let (rc, stdout, stderr) = roda(tmp.path(), &com_gate("TOURING_RUN_RAW=1 "));
    assert_eq!(rc, 0, "{stderr}");
    assert_eq!(stdout, "1 2 3 4\nfim\n");

    // a 2ª, igual, é negada: UMA linha, stdout vazio, exit 2
    let (rc, stdout, stderr) = roda(tmp.path(), &com_gate("TOURING_RUN_RAW=1 "));
    assert_eq!(
        rc, 2,
        "o exit do deny por repetição não muda: {stdout} {stderr}"
    );
    assert!(stdout.is_empty(), "nada de JSON no stdout: {stdout:?}");
    let linhas: Vec<&str> = stderr.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(linhas.len(), 1, "UMA linha no stderr: {stderr}");
    assert!(
        linhas[0].starts_with("[touring run] denied: ")
            && linhas[0].contains("run-")
            && linhas[0].contains("--file")
            && linhas[0].contains("--allow-repeat"),
        "a linha nomeia o run anterior e ensina a rota: {}",
        linhas[0]
    );

    // o modo envelope NÃO muda: o mesmo deny, sem a marca, segue JSON no stdout com exit 2
    let (rc, stdout, _stderr) = roda(tmp.path(), &com_gate(""));
    assert_eq!(rc, 2);
    let v: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("o envelope segue JSON ({e}): {stdout}"));
    assert!(v["repeat_blocked"]["best_run_id"].is_string(), "{v}");
}
