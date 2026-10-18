//! HOOKCWD — a raiz do `touring-hook` quando `CLAUDE_PROJECT_DIR` não existe.
//!
//! Caso real (smoke do `analise`, 10/10/2026, 30.4.96): numa chamada MANUAL do hook, sem
//! `CLAUDE_PROJECT_DIR`, a raiz saía de `TOURING_PROJECT_ROOT` — que o settings global fixa em
//! `~/projects/touring` (caso A) — ou de uma subida até o `~/.claude/touring` global (caso B). O
//! daemon PINADO no `analise` recusa em 0 ms com `{"error":"this daemon is pinned to … the
//! request names …"}`, o trace gravava `exit_reason: daemon-json` — IGUAL ao sucesso — e o
//! Claude Code engole o erro calado.
//!
//! Dois contratos:
//! (a) o `cwd` do payload entra na precedência da raiz: `CLAUDE_PROJECT_DIR` > `cwd` do payload >
//!     `TOURING_PROJECT_ROOT` > a subida estrutural;
//! (b) a recusa do daemon tem `exit_reason` PRÓPRIO (`daemon-error`), distinto do sucesso
//!     (`daemon-json`) — a medição distingue recusa de sucesso.
//!
//! O daemon é REAL e pinado de verdade: o socket mora em `<raiz>/.touring/`, que é a regra
//! de `pin_for_socket`. Sem o binário compilado o teste falha ALTO — um skip aqui esconderia
//! exatamente o defeito que ele existe para pegar.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

const HOOK_BIN: &str = env!("CARGO_BIN_EXE_touring-hook");
const DAEMON_BIN: &str = env!("CARGO_BIN_EXE_touring-daemon");

/// Daemon pinado numa raiz: socket em `<raiz>/.touring/d.sock` (a regra do pin). Encerrado pelo
/// PID que ESTE teste criou — jamais por nome (REGRA #19).
struct DaemonPinado {
    child: Child,
    socket: PathBuf,
}

impl DaemonPinado {
    fn sobe(raiz: &Path) -> Self {
        std::fs::create_dir_all(raiz.join(".touring")).expect("mkdir .touring");
        let socket = raiz.join(".touring/d.sock");
        let child = Command::new(DAEMON_BIN)
            .current_dir(raiz)
            .env("TOURING_DAEMON_SOCKET", &socket)
            .env("TOURING_DAEMON_SOCK", &socket)
            .env("TOURING_PROJECT_ROOT", raiz)
            .env("CLAUDE_PROJECT_DIR", raiz)
            .env("TOURING_IDLE_TIMEOUT_SECS", "120")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn touring-daemon");
        let d = Self { child, socket };
        d.espera_pronto();
        d
    }

    /// Pronto = responde ao `__health__` por ida e volta real (socket ligado ≠ daemon pronto).
    fn espera_pronto(&self) {
        let req = json!({
            "hook": "__health__", "payload": null, "project_root": "/",
            "session_id": null, "priority": 0, "origin": null
        })
        .to_string();
        for _ in 0..300 {
            if let Ok(mut s) = UnixStream::connect(&self.socket) {
                let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(2)));
                if s.write_all(req.as_bytes()).is_ok() && s.write_all(b"\n").is_ok() {
                    let mut linha = String::new();
                    if BufReader::new(s).read_line(&mut linha).is_ok() && linha.contains("healthy")
                    {
                        return;
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        panic!(
            "o daemon pinado não ficou pronto em 15s (socket {:?})",
            self.socket
        );
    }
}

impl Drop for DaemonPinado {
    fn drop(&mut self) {
        let _ = Command::new("kill")
            .arg(self.child.id().to_string())
            .output();
        let _ = self.child.wait();
    }
}

/// O que um `touring-hook cli-suggest` deixou: stdout e a linha de trace da invocação.
struct Saida {
    stdout: String,
    stderr: String,
    exit_reason: String,
    rc: i32,
}

/// Roda o hook REAL com ambiente CONTROLADO (nada herdado, exceto o PATH).
fn roda_hook(
    socket: &Path,
    cwd_do_processo: &Path,
    home: &Path,
    env_extra: &[(&str, &Path)],
    payload: &Value,
) -> Saida {
    let trace = tempfile::NamedTempFile::new().expect("trace");
    let mut cmd = Command::new(HOOK_BIN);
    cmd.arg("cli-suggest")
        .current_dir(cwd_do_processo)
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", home)
        .env("TOURING_DAEMON_SOCKET", socket)
        .env("TOURING_DAEMON_SOCK", socket)
        .env("TOURING_HOOK_TRACE_FILE", trace.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env_extra {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().expect("spawn touring-hook");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(payload.to_string().as_bytes())
        .expect("payload");
    let out = child.wait_with_output().expect("wait");
    let linhas = std::fs::read_to_string(trace.path()).unwrap_or_default();
    let ultima: Value = linhas
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .and_then(|l| serde_json::from_str(l).ok())
        .unwrap_or(Value::Null);
    Saida {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        exit_reason: ultima["exit_reason"]
            .as_str()
            .unwrap_or("<sem trace>")
            .to_string(),
        rc: out.status.code().unwrap_or(-1),
    }
}

/// Uma sessão NOVA por chamada: o G6 nega a repetição byte-idêntica dentro da mesma sessão, e
/// este teste mede a RAIZ, não o G6.
fn payload(cwd: Option<&Path>) -> Value {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let mut p = json!({
        "tool_name": "Bash",
        "session_id": format!("g97-hookcwd-{n}"),
        "tool_input": { "command": "ls" }
    });
    if let Some(c) = cwd {
        p["cwd"] = json!(c);
    }
    p
}

/// A recusa do daemon pinado NÃO aparece no stdout: o hook devolve `{}` (o schema do Claude Code
/// não admite a chave `error`) e despeja o JSON de erro no stderr — é por isso que o CC a engole
/// calado. O detector olha os DOIS canais.
fn recusado(s: &Saida) -> bool {
    s.stdout.contains("this daemon is pinned") || s.stderr.contains("this daemon is pinned")
}

#[test]
fn a_raiz_do_hook_segue_o_cwd_do_payload_e_a_recusa_tem_exit_reason_proprio() {
    let base = tempfile::tempdir().expect("tempdir");
    // `analise` = o projeto cujo daemon está PINADO; `touring` = o projeto que o settings global
    // fixa em TOURING_PROJECT_ROOT; `casa` = um HOME falso com o marcador global `.claude/touring`.
    let analise = base.path().join("analise");
    let touring = base.path().join("touring");
    let casa = base.path().join("casa");
    for d in [
        analise.join("sub/dir"),
        touring.join(".touring"),
        touring.join("sub"),
        casa.join(".claude/touring"),
        casa.join("trabalho"),
    ] {
        std::fs::create_dir_all(d).expect("mkdir");
    }
    let daemon = DaemonPinado::sobe(&analise);
    let sock = daemon.socket.clone();
    let sub = analise.join("sub/dir");
    let touring_sub = touring.join("sub");
    let fantasma = base.path().join("fantasma/inexistente");

    let mut falhas: Vec<String> = Vec::new();
    let mut confere = |nome: &str, ok: bool, s: &Saida, esperado: &str| {
        if !ok {
            falhas.push(format!(
                "{nome}: esperado {esperado}; rc={} exit_reason={} stdout={} stderr={}",
                s.rc,
                s.exit_reason,
                s.stdout.chars().take(120).collect::<String>(),
                s.stderr.chars().take(260).collect::<String>()
            ));
        }
    };

    // Controle 0 — CLAUDE_PROJECT_DIR continua PRIMEIRO, mesmo com um cwd de payload de fora.
    let s = roda_hook(
        &sock,
        &touring,
        &casa,
        &[("CLAUDE_PROJECT_DIR", analise.as_path())],
        &payload(Some(touring_sub.as_path())),
    );
    confere(
        "C0 CLAUDE_PROJECT_DIR vence o cwd do payload",
        !recusado(&s) && s.rc == 0,
        &s,
        "admitido",
    );

    // Caso A — TOURING_PROJECT_ROOT global aponta para OUTRO projeto; o cwd do payload é do
    // projeto pinado: o cwd da sessão (específico) vence a env global.
    let s = roda_hook(
        &sock,
        &touring,
        &casa,
        &[("TOURING_PROJECT_ROOT", touring.as_path())],
        &payload(Some(sub.as_path())),
    );
    confere(
        "A cwd do payload vence TOURING_PROJECT_ROOT",
        !recusado(&s) && s.rc == 0,
        &s,
        "admitido",
    );
    // (b) controle positivo: o SUCESSO segue `daemon-json`.
    confere(
        "A/b o sucesso continua daemon-json",
        s.exit_reason == "daemon-json",
        &s,
        "exit_reason=daemon-json",
    );

    // Caso B — sem nenhuma env, o processo roda em ~/trabalho (HOME falso); a subida estrutural
    // chegaria ao marcador global `~/.claude/touring`. O cwd do payload manda.
    let s = roda_hook(
        &sock,
        &casa.join("trabalho"),
        &casa,
        &[],
        &payload(Some(sub.as_path())),
    );
    confere(
        "B cwd do payload vence a subida até ~/.claude/touring",
        !recusado(&s) && s.rc == 0,
        &s,
        "admitido",
    );

    // Sem cwd no payload a cadeia antiga vale (nada regride): TOURING_PROJECT_ROOT do projeto certo.
    let s = roda_hook(
        &sock,
        &touring,
        &casa,
        &[("TOURING_PROJECT_ROOT", analise.as_path())],
        &payload(None),
    );
    confere(
        "controle: sem cwd de payload, TOURING_PROJECT_ROOT ainda manda",
        !recusado(&s) && s.rc == 0,
        &s,
        "admitido",
    );

    // cwd de payload que NÃO existe é ignorado (nunca vira raiz): cai na env.
    let s = roda_hook(
        &sock,
        &touring,
        &casa,
        &[("TOURING_PROJECT_ROOT", analise.as_path())],
        &payload(Some(fantasma.as_path())),
    );
    confere(
        "controle: cwd inexistente é ignorado",
        !recusado(&s) && s.rc == 0,
        &s,
        "admitido",
    );

    // Caso C — o cwd do payload é de FORA da árvore pinada: a recusa é legítima, e o trace a
    // distingue do sucesso. O hook segue saindo 0 (hooks nunca bloqueiam o CC).
    let s = roda_hook(
        &sock,
        &touring,
        &casa,
        &[],
        &payload(Some(touring_sub.as_path())),
    );
    confere(
        "C a recusa legítima do daemon pinado",
        recusado(&s) && s.rc == 0,
        &s,
        "recusa + rc 0",
    );
    confere(
        "C/b a recusa tem exit_reason próprio",
        s.exit_reason == "daemon-error",
        &s,
        "exit_reason=daemon-error",
    );

    drop(daemon);
    assert!(
        falhas.is_empty(),
        "{} caso(s) reprovado(s):\n  {}",
        falhas.len(),
        falhas.join("\n  ")
    );
}
