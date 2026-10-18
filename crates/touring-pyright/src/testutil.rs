//! Test-only helpers shared by the unit tests of several modules.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A unique scratch tree removed on drop (no external dependency, panic-safe).
pub(crate) struct Arvore {
    pub(crate) raiz: PathBuf,
}

impl Arvore {
    pub(crate) fn nova(tag: &str) -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let raiz = std::env::temp_dir().join(format!(
            "touring_pyright_t_{tag}_{}_{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&raiz).expect("create the scratch tree");
        Self {
            raiz: raiz.canonicalize().expect("canonicalize the scratch tree"),
        }
    }

    /// Writes `corpo` at `rel` (creating the directories) and returns the absolute path.
    pub(crate) fn arquivo(&self, rel: &str, corpo: &str) -> PathBuf {
        let path = self.raiz.join(rel);
        fs::create_dir_all(path.parent().expect("a file has a parent")).expect("create dirs");
        fs::write(&path, corpo).expect("write the fixture file");
        path
    }

    /// Like [`Arvore::arquivo`] and marks the file executable (a fake binary).
    #[cfg(unix)]
    pub(crate) fn executavel(&self, rel: &str, corpo: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = self.arquivo(rel, corpo);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod +x");
        path
    }
}

impl Drop for Arvore {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.raiz);
    }
}

/// A scripted LSP server that advertises `diagnosticProvider` and, like rust-analyzer, reports
/// `experimental/serverStatus`: `quiescent: false` right after `initialized`, `true` 300 ms later
/// (`nunca` never gets quiet; `oscila` flips false/true/false/true; `morre` exits on `didOpen`;
/// `saude_erro` reports `health: error` — a workspace that failed to load, quiet and empty;
/// `saude_aviso` reports `health: warning` — a workspace loaded without its dependencies). A
/// pull sent BEFORE it is quiet gets an EMPTY `full` report (the trap measured on rust-analyzer 1.98),
/// except in `cancela` mode, which answers `ServerCancelled` to the first three pulls, and
/// `sempre_cancela`, which never answers anything else. After it is quiet, `# undef N` lines of the
/// opened text become `E0425` diagnostics carrying the name in `x_nome`.
pub(crate) const FAKE_PULL: &str = r##"
import sys, json, time, threading
mode = sys.argv[1] if len(sys.argv) > 1 else "ok"
rin, out = sys.stdin.buffer, sys.stdout.buffer
lock = threading.Lock()
docs = {}
state = {"quiet": False, "pulls": 0}

def read():
    n = 0
    while True:
        line = rin.readline()
        if not line:
            sys.exit(0)
        line = line.strip()
        if not line:
            break
        if line.lower().startswith(b"content-length:"):
            n = int(line.split(b":")[1])
    return json.loads(rin.read(n))

def send(o):
    b = json.dumps(o).encode()
    with lock:
        out.write(b"Content-Length: %d\r\n\r\n" % len(b) + b)
        out.flush()

def status(q):
    health, message = "ok", None
    if mode == "saude_erro":
        health, message = "error", "falha ao carregar o workspace"
    elif mode == "saude_aviso":
        health, message = "warning", "Failed to read Cargo metadata with dependencies for `/p`"
    send({"jsonrpc": "2.0", "method": "experimental/serverStatus",
          "params": {"health": health, "quiescent": q, "message": message}})

def become_quiet():
    time.sleep(0.3)
    state["quiet"] = True
    status(True)
    if mode == "oscila":
        time.sleep(0.1)
        state["quiet"] = False
        status(False)
        time.sleep(0.1)
        state["quiet"] = True
        status(True)

def items(text):
    res = []
    for i, line in enumerate(text.split("\n")):
        if line.startswith("# undef "):
            nome = line.split()[2]
            res.append({"code": "E0425", "source": "rust-analyzer", "message": "no such value in this scope",
                        "x_nome": nome,
                        "range": {"start": {"line": i, "character": 8}, "end": {"line": i, "character": 8 + len(nome)}}})
    return res

while True:
    m = read()
    meth = m.get("method")
    if meth == "initialize":
        send({"jsonrpc": "2.0", "id": m["id"],
              "result": {"capabilities": {"diagnosticProvider": {"identifier": "fake", "interFileDependencies": True}}}})
    elif meth == "initialized":
        status(False)
        if mode != "nunca":
            threading.Thread(target=become_quiet, daemon=True).start()
    elif meth == "textDocument/didOpen":
        td = m["params"]["textDocument"]
        if mode == "morre":
            sys.exit(0)
        docs[td["uri"]] = td["text"]
    elif meth == "textDocument/diagnostic":
        state["pulls"] += 1
        uri = m["params"]["textDocument"]["uri"]
        if mode == "sempre_cancela" or (mode == "cancela" and state["pulls"] <= 3):
            send({"jsonrpc": "2.0", "id": m["id"],
                  "error": {"code": -32802, "message": "server cancelled the request", "data": {"retriggerRequest": True}}})
        elif mode == "unchanged":
            send({"jsonrpc": "2.0", "id": m["id"], "result": {"kind": "unchanged", "resultId": "x"}})
        else:
            its = items(docs.get(uri, "")) if state["quiet"] else []
            send({"jsonrpc": "2.0", "id": m["id"], "result": {"kind": "full", "resultId": "r", "items": its}})
    elif meth == "shutdown":
        send({"jsonrpc": "2.0", "id": m["id"], "result": None})
    elif meth == "exit":
        sys.exit(0)
"##;
