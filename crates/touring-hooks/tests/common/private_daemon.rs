//! Daemon privado para testes E2E que invocam o binário `touring`.
//!
//! **Por que existe.** Os testes E2E que rodam `touring <hook>` falavam com o
//! daemon GLOBAL (`/tmp/touring-daemon-1000.sock`) — o mesmo que atende as
//! sessões vivas do Claude Code. Sob a suíte completa (várias crates em
//! paralelo, máquina saturada) o daemon não respondia a tempo, o cliente saía
//! com stdout vazio e o teste quebrava no parse do JSON. O sintoma acusava o
//! código sob teste; a causa era a competição por um recurso compartilhado.
//!
//! Isolar o daemon torna esses testes **determinísticos** e, de quebra, impede
//! que a suíte polua o estado do daemon que o usuário está usando.
//!
//! Extraído de `wave24_hook_integration_e2e.rs` em 03/08/2026, quando
//! `e2e_diagnostic_rfc100.rs` reproduziu exatamente a mesma flakiness
//! (`b310_path_wired_when_predictive_blast_injects_symbols`, verde isolado e
//! vermelho na suíte completa). Duas cópias do mesmo helper seriam a assimetria
//! que a REGRA #0 manda evitar.

use std::io::Write;
use std::process::{Command, Stdio};

/// Localiza um binário do workspace, preferindo `release` a `debug`.
///
/// Devolve `None` quando não foi compilado — os testes tratam isso como SKIP,
/// não como falha (o harness E2E roda em debug invocando o binário de release).
///
/// A busca cobre três raízes porque `cargo llvm-cov` redireciona o build para
/// `target/llvm-cov-target/`: um binário de um `cargo build` comum fica
/// invisível dentro de uma execução de cobertura — exatamente como o job de
/// cobertura falhou em 02/08/2026. Um `CARGO_TARGET_DIR` explícito vence pelo
/// mesmo motivo.
#[must_use]
pub fn locate_binary(name: &str) -> Option<std::path::PathBuf> {
    let workspace_target = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("target"))?;
    let roots = [
        std::env::var_os("CARGO_TARGET_DIR").map(std::path::PathBuf::from),
        Some(workspace_target.join("llvm-cov-target")),
        Some(workspace_target),
    ];
    roots
        .into_iter()
        .flatten()
        .flat_map(|root| {
            ["release", "debug"]
                .map(|profile| root.join(profile).join(name))
                .into_iter()
        })
        .find(|candidate| candidate.exists())
}

/// Daemon `touring` próprio deste processo de teste, num socket exclusivo.
///
/// O `Drop` sinaliza pelo **PID que este processo criou** — nunca por nome
/// (REGRA #19: matar por nome derruba bridges MCP e handlers de outras sessões).
pub struct PrivateDaemon {
    pid: u32,
    socket: String,
}

impl PrivateDaemon {
    /// Sobe um daemon num socket único para (processo, `tag`). `None` quando o
    /// binário não foi compilado — um skip, não uma falha.
    #[must_use]
    pub fn start(tag: &str) -> Option<Self> {
        let bin = locate_binary("touring-daemon")?;
        let socket = format!("/tmp/touring-e2e-{tag}-{}.sock", std::process::id());
        let _ = std::fs::remove_file(&socket);
        let _ = std::fs::remove_file(format!("{socket}.lock"));

        let child = Command::new(&bin)
            .env("TOURING_DAEMON_SOCKET", &socket)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let pid = child.id();
        std::mem::forget(child); // colhido pelo Drop, não pelo handle pai

        // Prontidão medida por IDA E VOLTA real, nunca pela existência do arquivo.
        //
        // Até 07/08/2026 esta espera era `Path::new(&socket).exists()`. O bind cria
        // o arquivo MUITO antes de o daemon conseguir SERVIR (abrir DBs, montar
        // índice), então o teste seguia cedo demais e o cliente recebia
        // `success=false` com payload vazio — o que quebrou
        // `b310_path_wired_when_predictive_blast_injects_symbols` na suíte completa
        // (verde em 7,7s isolado, vermelho em 15-21s sob carga). É exatamente a
        // corrida espúria que a REGRA #19 descreve para o daemon global: socket
        // ligado ≠ daemon pronto.
        //
        // `doctor -j` é o probe canônico e barato; só o veredito `daemon_health ==
        // ok` conta. Enquanto não vier, o daemon não está pronto — por construção,
        // não por tempo de espera arbitrário.
        for _ in 0..200 {
            if std::path::Path::new(&socket).exists() && daemon_answers(&socket) {
                return Some(Self { pid, socket });
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        eprintln!(
            "PrivateDaemon({tag}): daemon não ficou pronto em 10s — teste será pulado, \
             não reprovado (socket={socket})"
        );
        let _ = Command::new("kill").arg(pid.to_string()).output();
        None
    }

    /// Socket deste daemon, para passar a `run_with_stdin`.
    #[must_use]
    pub fn socket(&self) -> &str {
        &self.socket
    }
}

impl Drop for PrivateDaemon {
    fn drop(&mut self) {
        // Sinaliza pelo PID que criamos — jamais por nome (REGRA #19).
        let _ = Command::new("kill").arg(self.pid.to_string()).output();
        let _ = std::fs::remove_file(&self.socket);
        let _ = std::fs::remove_file(format!("{}.lock", self.socket));
    }
}

/// O daemon deste socket já RESPONDE? (não apenas "o socket existe")
///
/// Lê o veredito de `touring doctor -j`: só `daemon_health.status == "ok"` conta.
/// Qualquer outra coisa — erro de conexão, JSON inválido, binário ausente — é
/// "ainda não pronto", que é o que o chamador precisa saber.
fn daemon_answers(socket: &str) -> bool {
    let Some((stdout, _stderr, code)) =
        run_with_stdin("touring", &["doctor", "-j"], "", Some(socket))
    else {
        return false;
    };
    if code != 0 {
        return false;
    }
    serde_json::from_str::<serde_json::Value>(&stdout)
        .ok()
        .and_then(|v| {
            v.as_array().map(|checks| {
                checks.iter().any(|c| {
                    c.get("name").and_then(serde_json::Value::as_str) == Some("daemon_health")
                        && c.get("status").and_then(serde_json::Value::as_str) == Some("ok")
                })
            })
        })
        .unwrap_or(false)
}

/// Roda `bin_name args…` com `stdin_payload` na entrada padrão.
///
/// `socket = Some(..)` aponta o cliente para um [`PrivateDaemon`]; `None` deixa
/// o cliente resolver o daemon normalmente (use apenas quando o teste tem como
/// alvo justamente essa resolução).
#[must_use]
pub fn run_with_stdin(
    bin_name: &str,
    args: &[&str],
    stdin_payload: &str,
    socket: Option<&str>,
) -> Option<(String, String, i32)> {
    let bin = locate_binary(bin_name)?;
    let mut cmd = Command::new(&bin);
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(sock) = socket {
        cmd.env("TOURING_DAEMON_SOCKET", sock);
    }
    let mut child = cmd.spawn().ok()?;

    if let Some(mut sin) = child.stdin.take() {
        sin.write_all(stdin_payload.as_bytes()).ok()?;
    }
    let out = child.wait_with_output().ok()?;
    Some((
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    ))
}
