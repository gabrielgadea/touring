//! Ordem dos gates com o VGP em modo `ativo` (30.4.97, coordenadora): o deny aplicável vence o
//! `[VGP · aviso]`, qualquer que seja a ordem de registro dos gates.
//!
//! `pre_bash` e `pre_edit_prevention` devolviam o `Context` do VGP na hora e o resto da cadeia
//! — o `bash_ast_validator`/`PreToolValidator` (comando perigoso) e o `check_deny_gate` S210
//! (edição que quebra um arquivo que parseia) — nunca era avaliado. Medido nos handlers
//! ORIGINAIS (fase 8, `scratchpad/logs/p8/red_e2e.log`, 10/10/2026): o stdout dos dois casos
//! era só `{"hookSpecificOutput":{"additionalContext":"[VGP · aviso] …"}}`, sem
//! `permissionDecision`, ou seja, o `rm -rf` passava. Este arquivo dirige o
//! binário REAL do hook (daemon dispensado, `[vgp] modo = "ativo"` no projeto) com um caso REAL
//! de `warn` do motor: símbolo do PROJETO inexistente fora de `/tmp` (o motor nega tudo em
//! `/tmp/` e em código inline; um atributo inexistente num `str` é `warn` mesmo inline).

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

const AVISO: &str = "[VGP · aviso]";

/// Projeto FORA de `/tmp` (o `warn` de projeto só existe em arquivo do projeto) com o VGP ativo.
fn projeto() -> tempfile::TempDir {
    let dir = tempfile::Builder::new()
        .prefix("vgp-ordem-")
        .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
        .expect("tempdir fora de /tmp");
    assert!(
        !dir.path().starts_with("/tmp"),
        "o motor nega tudo em /tmp: o projeto tem de morar fora dele ({})",
        dir.path().display()
    );
    let raiz = dir.path();
    std::fs::create_dir_all(raiz.join(".touring")).expect("mkdir .touring");
    std::fs::write(
        raiz.join(".touring/touring.toml"),
        "[vgp]\nmodo = \"ativo\"\n",
    )
    .expect("touring.toml");
    std::fs::create_dir_all(raiz.join("pkg")).expect("mkdir pkg");
    std::fs::write(raiz.join("pkg/__init__.py"), "").expect("__init__");
    std::fs::write(raiz.join("pkg/mod.py"), "def real():\n    return 1\n").expect("mod.py");
    dir
}

/// Roda o `touring-hook <subcommand>` real; devolve o stdout (JSON do hook).
fn roda(subcommand: &str, payload: &serde_json::Value, raiz: &Path) -> String {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_touring-hook"));
    cmd.arg(subcommand)
        .current_dir(raiz)
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", raiz)
        .env("CLAUDE_PROJECT_DIR", raiz)
        .env("TOURING_NO_DAEMON", "1")
        .env("TOURING_VGP_SOMBRA_FILE", raiz.join("sombra.jsonl"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn touring-hook");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(payload.to_string().as_bytes())
        .expect("write payload");
    let out = child.wait_with_output().expect("wait touring-hook");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn json(stdout: &str) -> serde_json::Value {
    serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("stdout do hook não é JSON ({e}): {stdout:?}"))
}

fn decisao(v: &serde_json::Value) -> Option<&str> {
    v.pointer("/hookSpecificOutput/permissionDecision")
        .and_then(serde_json::Value::as_str)
}

fn razao(v: &serde_json::Value) -> &str {
    v.pointer("/hookSpecificOutput/permissionDecisionReason")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
}

fn contexto(v: &serde_json::Value) -> &str {
    v.pointer("/hookSpecificOutput/additionalContext")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
}

/// A prova de que o motor FALOU: a linha de sombra do projeto registra `ativo` + `warn`. Sem ela
/// o teste seria vácuo (o aviso poderia nunca ter existido).
fn motor_avisou(raiz: &Path, hook: &str) {
    let sombra = std::fs::read_to_string(raiz.join("sombra.jsonl")).unwrap_or_default();
    let linha = sombra
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .find(|v| v["hook"] == hook)
        .unwrap_or_else(|| panic!("o motor não gravou linha de `{hook}` em:\n{sombra}"));
    assert_eq!(linha["modo"], "ativo", "{linha}");
    assert_eq!(
        linha["verdict"], "warn",
        "o caso tem de ser um warn REAL: {linha}"
    );
}

/// Python inline com um `warn` do motor (atributo inexistente num `str`) E um comando perigoso
/// na mesma linha. O `rm -rf` tem de ser negado, com o aviso do VGP no texto do deny.
#[test]
fn bash_aviso_do_vgp_nao_esconde_o_bloqueio_do_comando_perigoso() {
    let dir = projeto();
    let payload = serde_json::json!({
        "session_id": "g97-vgp-bash",
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": {"command": "python3 -c 'x = \"abc\"; x.inexistente()' && rm -rf /tmp/work"}
    });
    let out = roda("pre-bash", &payload, dir.path());
    motor_avisou(dir.path(), "pre-bash");
    let v = json(&out);
    assert_eq!(
        decisao(&v),
        Some("deny"),
        "o deny aplicável vence o aviso do VGP (stdout: {out})"
    );
    assert!(
        razao(&v).contains("rm -rf"),
        "o bloqueio é o do validador: {}",
        razao(&v)
    );
    assert!(
        razao(&v).contains(AVISO),
        "o aviso vai junto no texto: {}",
        razao(&v)
    );
}

/// Controle positivo: o mesmo comando perigoso, sem python inline, é negado sem texto do VGP —
/// o gate existe neste ambiente, e a diferença do caso acima é só a ordem.
#[test]
fn bash_controle_sem_python_o_bloqueio_existe_sem_texto_do_vgp() {
    let dir = projeto();
    let payload = serde_json::json!({
        "session_id": "g97-vgp-bash-controle",
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": {"command": "rm -rf /tmp/work"}
    });
    let v = json(&roda("pre-bash", &payload, dir.path()));
    assert_eq!(decisao(&v), Some("deny"), "{v}");
    assert!(!razao(&v).contains("VGP"), "{}", razao(&v));
}

/// Controle: python inline com o mesmo `warn` e comando inofensivo segue só com o aviso.
#[test]
fn bash_controle_inofensivo_segue_com_o_aviso() {
    let dir = projeto();
    let payload = serde_json::json!({
        "session_id": "g97-vgp-bash-inofensivo",
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": {"command": "python3 -c 'x = \"abc\"; x.inexistente()'"}
    });
    let v = json(&roda("pre-bash", &payload, dir.path()));
    motor_avisou(dir.path(), "pre-bash");
    assert_eq!(decisao(&v), None, "só o aviso, nunca deny: {v}");
    assert!(contexto(&v).contains(AVISO), "{v}");
}

/// Um `.py` do projeto com um `warn` REAL do motor (atributo inexistente de função do projeto)
/// e uma edição que o quebra sintaticamente: o deny do S210 tem de vencer o aviso.
#[test]
fn edit_aviso_do_vgp_nao_esconde_o_deny_de_sintaxe() {
    let dir = projeto();
    let alvo = dir.path().join("uses.py");
    std::fs::write(&alvo, "from pkg.mod import real\n\nreal.inexistente()\n").expect("uses.py");
    let payload = serde_json::json!({
        "session_id": "g97-vgp-edit",
        "hook_event_name": "PreToolUse",
        "tool_name": "Edit",
        "tool_input": {
            "file_path": alvo.to_str().expect("utf8"),
            "old_string": "real.inexistente()\n",
            "new_string": "real.inexistente()\nx = (\n"
        }
    });
    let out = roda("pre-edit-prevention", &payload, dir.path());
    motor_avisou(dir.path(), "pre-edit");
    let v = json(&out);
    assert_eq!(
        decisao(&v),
        Some("deny"),
        "o S210 vence o aviso do VGP (stdout: {out})"
    );
    assert!(
        razao(&v).contains(AVISO),
        "o aviso vai junto no texto: {}",
        razao(&v)
    );
}

/// Controle: a mesma edição, sem quebra de sintaxe, segue só com o aviso.
#[test]
fn edit_controle_valida_segue_com_o_aviso() {
    let dir = projeto();
    let alvo = dir.path().join("uses.py");
    std::fs::write(&alvo, "from pkg.mod import real\n\nreal.inexistente()\n").expect("uses.py");
    let payload = serde_json::json!({
        "session_id": "g97-vgp-edit-valida",
        "hook_event_name": "PreToolUse",
        "tool_name": "Edit",
        "tool_input": {
            "file_path": alvo.to_str().expect("utf8"),
            "old_string": "real.inexistente()\n",
            "new_string": "real.inexistente()\nx = 1\n"
        }
    });
    let v = json(&roda("pre-edit-prevention", &payload, dir.path()));
    motor_avisou(dir.path(), "pre-edit");
    assert_eq!(decisao(&v), None, "só o aviso, nunca deny: {v}");
    assert!(contexto(&v).contains(AVISO), "{v}");
}

/// Write: o `warn` do VGP vai junto; o pipeline continua (a cadeia não é encerrada pelo aviso).
#[test]
fn write_aviso_do_vgp_segue_junto_do_pipeline() {
    let dir = projeto();
    let alvo = dir.path().join("novo.py");
    let payload = serde_json::json!({
        "session_id": "g97-vgp-write",
        "hook_event_name": "PreToolUse",
        "tool_name": "Write",
        "tool_input": {
            "file_path": alvo.to_str().expect("utf8"),
            "content": "from pkg.mod import real\n\nreal.inexistente()\n"
        }
    });
    let v = json(&roda("pre-write", &payload, dir.path()));
    motor_avisou(dir.path(), "pre-write");
    assert_eq!(decisao(&v), None, "só contexto, nunca deny: {v}");
    assert!(contexto(&v).contains(AVISO), "{v}");
}
