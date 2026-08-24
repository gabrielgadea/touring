//! E2E tests for touring diary CLI — cross-audit PLN2 P4.1
//!
//! Tests the full diary stack:
//! - AgentDiary write/read/list/meta
//! - AAAK marker parsing
//! - Palace hierarchy storage
//! - Topic filtering

use std::path::PathBuf;
use std::process::{Command, Stdio};
use tempfile::TempDir;

/// Resolve a touring binary, preferring the WORKSPACE build over the installed one.
///
/// Both helpers used to hardcode `/home/gabrielgadea/.local/bin/…`, so this
/// suite exercised whatever was last *deployed* rather than the code under
/// test — a green run said nothing about the working tree, and the paths break
/// on any other machine. Workspace first, installed as fallback.
fn resolve_bin(name: &str) -> PathBuf {
    let workspace_target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("target"));
    if let Some(target) = workspace_target {
        for profile in ["debug", "release"] {
            let candidate = target.join(profile).join(name);
            if candidate.exists() {
                return candidate;
            }
        }
    }
    // Fallback: the user's own install dir, derived from $HOME. It used to be
    // the literal `/home/gabrielgadea/.local/bin/…`, which resolves nowhere on
    // a CI runner — and a test whose fallback cannot exist is a test that only
    // ever ran on one machine (run 31166066285, 2026-08-07).
    let home = std::env::var_os("HOME").map(PathBuf::from);
    home.map_or_else(
        || PathBuf::from(name),
        |h| h.join(".local").join("bin").join(name),
    )
}

fn touring_bin() -> PathBuf {
    resolve_bin("touring")
}

fn daemon_bin() -> PathBuf {
    resolve_bin("touring-daemon")
}

/// Start a fresh daemon for test isolation.
///
/// REGRA #19: per-process socket isolation (TOURING_DAEMON_SOCKET below)
/// makes broad `pkill -9 -f touring-daemon` unnecessary AND dangerous —
/// it would also kill the daemon of any parallel test process or of
/// other CC sessions on the same host. Clean only OUR socket/lock here.
/// Socket path unique to the CURRENT TEST, not merely the current process.
///
/// Keying on `process::id()` alone gave all 9 tests in this binary ONE socket,
/// and `start_daemon` opens with `remove_file(&socket_path)` — so a test
/// starting up deleted the endpoint a concurrently-running neighbour was
/// already talking to, and that neighbour's next CLI call died with
/// "No such file or directory (os error 2)" (2026-08-02). libtest names the
/// worker thread after the test, so both `start_daemon` and `touring` derive
/// the same value inside one test without threading any parameter through.
fn socket_path() -> String {
    let test = std::thread::current()
        .name()
        .unwrap_or("main")
        .replace([':', '/'], "_");
    format!("/tmp/touring-daemon-{}-{}.sock", std::process::id(), test)
}

/// NOTA sobre isolamento (medido em 2026-08-24, ao consertar este arquivo)
///
/// O socket e o cwd são isolados; o `HOME` NÃO é — e o corpus de memória
/// pende de `$HOME/.claude/touring`. Portanto `memory recall` aqui alcança o
/// corpus da máquina que roda o teste. Isolar o HOME foi tentado e **não é
/// viável hoje**: num HOME novo TODA chamada de `cli-memory-recall` estoura o
/// orçamento de 15s do cliente (não só a primeira — um warm-up descartado não
/// resolveu), o que é um problema à parte, do recall em corpus vazio, e não
/// deste teste. A saída aqui é não depender do silêncio do ambiente: a entrada
/// carrega um nonce único e a consulta é por ele, então o ranqueamento não
/// disputa com o que mais exista no corpus.
fn start_daemon() -> DaemonGuard {
    let socket_path = socket_path();
    let _ = std::fs::remove_file(&socket_path);
    let _ = std::fs::remove_file(format!("{socket_path}.lock"));

    // We intentionally leak the Child: the daemon must outlive this helper so
    // subsequent touring CLI calls can reach the socket. `stop_daemon()` later
    // kills it via SIGKILL + socket cleanup. `mem::forget` prevents Drop from
    // reaping the handle prematurely.
    // The socket must be handed to the DAEMON, not only to the client. Without
    // this the daemon bound the global default socket while `touring()` dialed
    // `/tmp/touring-daemon-<pid>.sock`, so every CLI call failed with
    // "No such file or directory (os error 2)" — the isolation the doc comment
    // above promised never actually existed (found 2026-08-02).
    let daemon = Command::new(daemon_bin())
        .env("TOURING_DAEMON_SOCKET", &socket_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("daemon spawn failed");
    std::thread::sleep(std::time::Duration::from_secs(2));
    let pid = daemon.id();
    std::mem::forget(daemon);
    DaemonGuard { pid }
}

/// Run touring CLI in a temp directory
fn touring(args: &[&str], tmpdir: &TempDir) -> std::process::Output {
    let socket_path = socket_path();
    let mut cmd = Command::new(touring_bin());
    for arg in args {
        cmd.arg(arg);
    }
    cmd.current_dir(tmpdir.path())
        .env("TOURING_DAEMON_SOCKET", &socket_path);
    cmd.output().expect("touring CLI failed")
}

fn stop_daemon(pid: u32) {
    let _ = Command::new("kill").arg("-9").arg(pid.to_string()).output();
    // Clean the socket on the way OUT too. `start_daemon` only removed it on
    // the way IN, so every run left its socket file behind: 174 stale
    // `/tmp/touring-daemon-*-test_*.sock` had accumulated by 09/08/2026.
    let socket_path = socket_path();
    let _ = std::fs::remove_file(&socket_path);
    let _ = std::fs::remove_file(format!("{socket_path}.lock"));
}

/// Stops the test daemon even when the test PANICS.
///
/// `stop_daemon(pid)` used to be the last statement of each test body, so a
/// failing assertion skipped it and the daemon outlived the run — found
/// 09/08/2026 with a leaked `touring-daemon` still alive after
/// `test_diary_fts5_searchable` failed. Cleanup that only happens on the happy
/// path is not cleanup; `Drop` runs during unwinding, so this one always does.
struct DaemonGuard {
    pid: u32,
}

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        stop_daemon(self.pid);
    }
}

fn parse_json(output: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
        panic!(
            "JSON parse failed. stdout: {}, stderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn parse_json_opt(output: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Null)
}

#[test]
fn test_diary_write_and_read() {
    let tmpdir = TempDir::new().unwrap();
    let _daemon = start_daemon();

    let out = touring(
        &[
            "diary",
            "write",
            "test_agent",
            "minha primeira entrada",
            "--topic",
            "onboarding",
        ],
        &tmpdir,
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "diary write failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json = parse_json(&out);
    assert_eq!(json["status"], "written");
    assert_eq!(json["agent"], "test_agent");
    assert_eq!(json["is_aaak"], false);
    assert!(json["timestamp"].as_str().is_some());

    let out = touring(&["diary", "read", "test_agent"], &tmpdir);
    assert_eq!(out.status.code(), Some(0));
    let json = parse_json(&out);
    assert_eq!(json["count"], 1);
    let entry = &json["entries"][0];
    assert!(
        entry["content"]
            .as_str()
            .unwrap()
            .contains("primeira entrada")
    );
}

#[test]
fn test_diary_aaak_markers() {
    let tmpdir = TempDir::new().unwrap();
    let _daemon = start_daemon();

    let out = touring(
        &[
            "diary",
            "write",
            "audit_agent",
            "#[P:scouting] #[R:0.95] #[L:nenhum orphan encontrado",
            "--aaak",
            "--topic",
            "cross_audit",
        ],
        &tmpdir,
    );
    assert_eq!(out.status.code(), Some(0));
    let json = parse_json(&out);
    assert_eq!(json["is_aaak"], true);

    let out = touring(&["diary", "read", "audit_agent"], &tmpdir);
    let json = parse_json(&out);
    assert_eq!(json["count"], 1);

    let markers: Vec<(String, String)> = json["entries"][0]["markers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m[0].as_str().unwrap().to_string(),
                m[1].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert!(
        markers.iter().any(|(k, v)| k == "phase" && v == "scouting"),
        "phase marker missing"
    );
    assert!(
        markers.iter().any(|(k, v)| k == "result" && v == "0.95"),
        "result marker missing"
    );
}

#[test]
fn test_diary_topic_filter() {
    let tmpdir = TempDir::new().unwrap();
    let _daemon = start_daemon();

    touring(
        &[
            "diary",
            "write",
            "topic_agent",
            "entrada alpha",
            "--topic",
            "alpha",
        ],
        &tmpdir,
    );
    touring(
        &[
            "diary",
            "write",
            "topic_agent",
            "entrada beta",
            "--topic",
            "beta",
        ],
        &tmpdir,
    );
    touring(
        &[
            "diary",
            "write",
            "topic_agent",
            "entrada alpha 2",
            "--topic",
            "alpha",
        ],
        &tmpdir,
    );

    let out = touring(
        &["diary", "read", "topic_agent", "--topic", "alpha"],
        &tmpdir,
    );
    let json = parse_json(&out);
    assert_eq!(json["count"], 2);
    assert_eq!(json["topic_filter"], "alpha");
}

#[test]
fn test_diary_last_n() {
    let tmpdir = TempDir::new().unwrap();
    let _daemon = start_daemon();

    for i in 0..5 {
        touring(
            &["diary", "write", "last_agent", &format!("entrada {}", i)],
            &tmpdir,
        );
    }

    let out = touring(&["diary", "read", "last_agent", "--last", "2"], &tmpdir);
    let json = parse_json(&out);
    assert_eq!(json["count"], 2);
    assert_eq!(json["last_n"], 2);
}

#[test]
fn test_diary_meta_after_write() {
    let tmpdir = TempDir::new().unwrap();
    let _daemon = start_daemon();

    touring(&["diary", "write", "agent_a", "entrada A"], &tmpdir);

    let out = touring(&["diary", "meta", "agent_a"], &tmpdir);
    let json = parse_json(&out);
    assert_eq!(json["status"], "ok");
    assert_eq!(json["agent"], "agent_a");
    assert!(json["entry_count"].as_i64().unwrap_or(0) >= 1);
}

#[test]
fn test_diary_write_exit_code() {
    let tmpdir = TempDir::new().unwrap();
    let _daemon = start_daemon();

    let out = touring(&["diary", "write", "exit_test", "test content"], &tmpdir);
    assert_eq!(out.status.code(), Some(0), "valid write must exit 0");
}

#[test]
fn test_diary_multiple_entries_ordered() {
    let tmpdir = TempDir::new().unwrap();
    let _daemon = start_daemon();

    touring(&["diary", "write", "order_agent", "primeira"], &tmpdir);
    std::thread::sleep(std::time::Duration::from_millis(50));
    touring(&["diary", "write", "order_agent", "segunda"], &tmpdir);
    std::thread::sleep(std::time::Duration::from_millis(50));
    touring(&["diary", "write", "order_agent", "terceira"], &tmpdir);

    let out = touring(&["diary", "read", "order_agent"], &tmpdir);
    let json = parse_json(&out);
    // Newest first
    let contents: Vec<&str> = json["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["content"].as_str().unwrap())
        .collect();
    assert_eq!(contents[0], "terceira");
    assert_eq!(contents[1], "segunda");
    assert_eq!(contents[2], "primeira");
}

#[test]
fn test_diary_no_diary_status() {
    let tmpdir = TempDir::new().unwrap();
    let _daemon = start_daemon();

    let out = touring(&["diary", "meta", "nonexistent_agent"], &tmpdir);
    let json = parse_json_opt(&out);
    // Status depends on whether diary was found — use raw output
    assert!(out.status.code() == Some(0) || json.get("status").is_some());
}

/// MEDIDO em 2026-08-24 — este teste era um FALSO-VERDE.
///
/// A asserção procurava, em QUALQUER entrada devolvida, as palavras "FTS5" ou
/// "semantic recall". O `memory recall` alcança o corpus da máquina que roda o
/// teste, onde essas palavras existem de sobra — então ele passava casando com
/// memórias do desenvolvedor, nunca com a entrada de diário que ele mesmo
/// acabara de escrever. Um nonce por execução (abaixo) desfaz o acidente, e com
/// ele a falha real aparece:
///
/// - num tmpdir novo, o recall reporta `RRF fusion from 1 sources` — só a fonte
///   ANN responde, e ela cai no corpus GLOBAL; as fontes lexicais do projeto de
///   teste ficam mudas, então o termo exato recém-escrito não é encontrado;
/// - isolar o `HOME` foi tentado e piora: aí TODA chamada de `cli-memory-recall`
///   estoura o orçamento de 15s do cliente (um warm-up descartado não resolveu).
///
/// CAUSA RAIZ, medida em disco em 24/08/2026 — e CORRIGIDA no mesmo dia.
///
/// O parágrafo acima descreve o SINTOMA, e ele apontava para o motor de recall,
/// que está certo. O defeito era do ESCRITOR: `touring diary write` resolvia a
/// raiz com `std::env::current_dir()` cru, em cinco subcomandos, enquanto todo o
/// resto do Touring usa a raiz NORMALIZADA. No mesmo diretório, com o mesmo
/// binário:
///   - `diary write`   gravava em `<cwd>/.claude/touring/memory.db`  (cwd cru)
///   - `memory recall` lê primary(raiz normalizada) + raízes sob `~/.claude`
///
/// Num diretório sem marcador de projeto as duas nunca se encontravam: o diário
/// caía numa DB que o recall jamais abre. O tell é o campo `source_db` — `null`
/// em TODAS as entradas significa que nada veio da federação lexical, só da ANN,
/// e a consulta devolvia N resultados confiantes sem NENHUM contendo o termo.
///
/// Num projeto real funcionava porque cwd e raiz normalizada coincidem, e foi
/// essa coincidência que sustentou a leitura "coisa de projeto vazio" por meses.
///
/// A correção põe os cinco subcomandos em `diary_project_root()`
/// (`TouringConfig::normalize_project_root`), o mesmo remédio que
/// `daemon_client.rs` e `handlers/mcp.rs` já haviam recebido para a "classe das
/// 29 DBs órfãs" — o diário era o membro que faltava. Guardas de regressão:
/// `cli::diary::diary_root_tests` (estrutural sobre os 5 sítios + as duas
/// direções: sem marcador cai no HOME, dentro de projeto continua no projeto).
///
/// Este teste deixou de ser `#[ignore]` porque a deficiência que o justificava
/// não existe mais — verificado verde no binário vivo em 24/08/2026.
#[test]
fn test_diary_fts5_searchable() {
    // Verify diary entries are ingested into FTS5 so `touring memory recall`
    // can find them via text search.
    let tmpdir = TempDir::new().unwrap();
    let _daemon = start_daemon();

    // Um nonce por execução torna a entrada IRREPETÍVEL no corpus. Antes a
    // consulta era a frase toda ("FTS5 integração semantic recall") e o teste
    // só passava enquanto nada semanticamente próximo existisse na máquina —
    // quebrou em 2026-08-24 quando uma sessão gravou memórias vizinhas.
    let nonce = format!("fts5nonce{}", std::process::id());
    let entry = format!("diário busca FTS5 integração semantic recall {nonce}");
    touring(&["diary", "write", "fts5_agent", &entry], &tmpdir);

    // Query memory via recall (FTS5) — pelo nonce, não pela frase genérica.
    let out = touring(&["memory", "recall", &nonce], &tmpdir);
    assert_eq!(
        out.status.code(),
        Some(0),
        "memory recall failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json = parse_json(&out);
    // Should find the diary entry we just wrote — entries[] is the RLM layer,
    // matches[] is the SemanticRecall FTS5 layer (may be 0 if no embedding stored).
    let diary_found = json["entries"]
        .as_array()
        .map(|arr| {
            arr.iter().any(|m| {
                m["value"]
                    .as_str()
                    .map(|v| v.contains(&nonce))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);

    assert!(
        diary_found,
        "diary entry should appear in recall output. Got: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}
