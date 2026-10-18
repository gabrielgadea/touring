//! A minimal synchronous LSP client over stdio — the transport of VGP layer 2.
//!
//! Why hand-rolled, std + `serde_json` only (no `tower-lsp`, no `tokio`): a hook needs one
//! `didOpen → publishDiagnostics → didClose` round-trip per edit against a language server that
//! stays alive between edits. An async runtime would be a second scheduler inside a short-lived
//! synchronous caller, and the hook crates that consume this one must not inherit a runtime (nor the
//! `touring-lsp` editor bridge's dependencies) just to ask a type checker one question.
//!
//! Three design points, each one learned from the Python probe (`sonda_camada2_pyright.py`):
//!
//! 1. **Reading has a deadline.** A dedicated reader thread owns the child's stdout and forwards
//!    parsed frames over an `mpsc` channel, so every wait is a `recv_timeout` that can never block
//!    past the caller's deadline (a blocking `read` on a pipe cannot be interrupted).
//! 2. **Matching is by `(uri, version)`.** Every `didOpen` carries a fresh, strictly increasing
//!    version and only the `publishDiagnostics` of that exact version counts. pyright 1.1.408 has no
//!    pull diagnostics and publishes a *cleanup* (`version: null`, no diagnostics) ~50 ms after each
//!    `didClose`; accepting "the first publication for the uri" made a reopened file read the previous
//!    open's cleanup as its own result.
//! 3. **The server's requests are answered.** pyright asks `workspace/configuration` and waits for
//!    the reply forever; any request we do not understand gets `result: null` instead of silence.

use std::io::{self, BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use thiserror::Error;

/// How long `shutdown` waits for the server's reply before sending `exit` anyway.
const SHUTDOWN_REPLY_WAIT: Duration = Duration::from_millis(300);
/// How long the process gets to exit by itself after `exit` + stdin EOF before it is killed.
const EXIT_GRACE: Duration = Duration::from_millis(500);
/// Upper bound for one frame body; a corrupt `Content-Length` must not allocate gigabytes.
const MAX_FRAME_BYTES: usize = 64 * 1024 * 1024;

/// Errors of the LSP client.
#[derive(Debug, Error)]
pub enum ClientError {
    /// The server process could not be started.
    #[error("failed to spawn language server: {0}")]
    Spawn(io::Error),
    /// Reading from or writing to the server failed.
    #[error("I/O with language server: {0}")]
    Io(io::Error),
    /// The server closed its end of the connection (it exited or crashed).
    #[error("language server closed the connection")]
    Closed,
    /// A frame violated the wire format (missing/oversized `Content-Length`, truncated body).
    #[error("malformed LSP frame: {0}")]
    Protocol(String),
    /// A frame body was not valid JSON.
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// The caller's deadline passed before the awaited message arrived.
    #[error("timed out waiting for {0}")]
    Timeout(String),
    /// The server answered a request with a JSON-RPC `error`.
    #[error("language server error for `{method}`: {message}")]
    ServerError {
        /// The request method that failed.
        method: String,
        /// The server's error message.
        message: String,
    },
}

impl From<io::Error> for ClientError {
    /// A broken pipe means the peer is gone: report it as [`ClientError::Closed`], not as noise.
    fn from(e: io::Error) -> Self {
        match e.kind() {
            io::ErrorKind::BrokenPipe | io::ErrorKind::UnexpectedEof => Self::Closed,
            _ => Self::Io(e),
        }
    }
}

// ── framing ───────────────────────────────────────────────────────────────

/// Writes one `Content-Length`-framed JSON message. The length counts **bytes**, not characters.
pub fn write_frame<W: Write>(w: &mut W, msg: &Value) -> Result<(), ClientError> {
    let body = serde_json::to_vec(msg)?;
    let mut frame = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    frame.extend_from_slice(&body);
    w.write_all(&frame)?;
    w.flush()?;
    Ok(())
}

/// Reads one frame. `Ok(None)` is a clean EOF *between* frames; an EOF inside a frame is a
/// [`ClientError::Protocol`] error (the peer died mid-message).
pub fn read_frame<R: BufRead>(r: &mut R) -> Result<Option<Value>, ClientError> {
    let Some(len) = read_headers(r)? else {
        return Ok(None);
    };
    if len > MAX_FRAME_BYTES {
        return Err(ClientError::Protocol(format!(
            "Content-Length {len} exceeds the {MAX_FRAME_BYTES}-byte cap"
        )));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body).map_err(|e| match e.kind() {
        io::ErrorKind::UnexpectedEof => ClientError::Protocol("truncated frame body".into()),
        _ => e.into(),
    })?;
    Ok(Some(serde_json::from_slice(&body)?))
}

/// Reads header lines up to the blank line and returns the `Content-Length` (header name matched
/// case-insensitively, other headers ignored). `Ok(None)` = EOF before any header byte.
fn read_headers<R: BufRead>(r: &mut R) -> Result<Option<usize>, ClientError> {
    let mut length = None;
    let mut seen_any = false;
    loop {
        let mut line = Vec::new();
        if r.read_until(b'\n', &mut line)? == 0 {
            return if seen_any {
                Err(ClientError::Protocol("EOF inside the header block".into()))
            } else {
                Ok(None)
            };
        }
        seen_any = true;
        let text = String::from_utf8_lossy(&line);
        let text = text.trim();
        if text.is_empty() {
            break;
        }
        if let Some(n) = content_length(text)? {
            length = Some(n);
        }
    }
    length
        .map(Some)
        .ok_or_else(|| ClientError::Protocol("missing Content-Length header".into()))
}

/// The value of a `Content-Length` header line, `None` for any other header.
fn content_length(header: &str) -> Result<Option<usize>, ClientError> {
    let Some((name, value)) = header.split_once(':') else {
        return Ok(None);
    };
    if !name.trim().eq_ignore_ascii_case("content-length") {
        return Ok(None);
    }
    value
        .trim()
        .parse::<usize>()
        .map(Some)
        .map_err(|_| ClientError::Protocol(format!("bad Content-Length value `{}`", value.trim())))
}

// ── message classification (pure) ──────────────────────────────────────────

/// A message with both `method` and `id` is a *request from the server* and demands a reply.
fn is_server_request(m: &Value) -> bool {
    m.get("method").is_some() && m.get("id").is_some()
}

/// A message without `method` whose `id` equals ours is the response to our request.
fn is_response_to(m: &Value, id: u64) -> bool {
    m.get("method").is_none() && m.get("id").and_then(Value::as_u64) == Some(id)
}

/// Is `m` the `publishDiagnostics` of exactly `(uri, version)`? A publication of another version,
/// of another uri, or with a null/absent version (pyright's post-`didClose` cleanup) is not.
pub(crate) fn is_publish_for(m: &Value, uri: &str, version: u64) -> bool {
    m.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics")
        && m.pointer("/params/version").and_then(Value::as_u64) == Some(version)
        && m.pointer("/params/uri")
            .and_then(Value::as_str)
            .is_some_and(|published| same_uri(published, uri))
}

/// URI equality that tolerates a server re-encoding the path (`%20` vs space, `%C3%A7` vs `ç`).
pub(crate) fn same_uri(a: &str, b: &str) -> bool {
    a == b || percent_decode(a) == percent_decode(b)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match (bytes[i], hex_pair(bytes.get(i + 1..i + 3))) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (c, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_pair(pair: Option<&[u8]>) -> Option<u8> {
    let pair = pair?;
    let hi = char::from(pair[0]).to_digit(16)?;
    let lo = char::from(pair[1]).to_digit(16)?;
    u8::try_from(hi * 16 + lo).ok()
}

/// The `result` to send back for a request the *server* made to us.
///
/// `workspace/configuration` is answered item by item from `sections` (`null` for a section we do
/// not declare); every other request — `client/registerCapability`, `window/workDoneProgress/create`,
/// … — gets `null`, which is the "nothing to report" reply and keeps the server from waiting.
pub fn answer_server_request(msg: &Value, sections: &Map<String, Value>) -> Value {
    if msg.get("method").and_then(Value::as_str) != Some("workspace/configuration") {
        return Value::Null;
    }
    let items = msg
        .pointer("/params/items")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    Value::Array(
        items
            .iter()
            .map(|item| {
                let section = item.get("section").and_then(Value::as_str).unwrap_or("");
                sections.get(section).cloned().unwrap_or(Value::Null)
            })
            .collect(),
    )
}

// ── the client ─────────────────────────────────────────────────────────────

/// What the reader thread forwards.
enum Incoming {
    Message(Value),
    Eof,
    Failure(String),
}

fn reader_loop(stdout: ChildStdout, tx: Sender<Incoming>) {
    let mut reader = BufReader::new(stdout);
    loop {
        let item = match read_frame(&mut reader) {
            Ok(Some(msg)) => Incoming::Message(msg),
            Ok(None) => Incoming::Eof,
            Err(e) => Incoming::Failure(e.to_string()),
        };
        let last = !matches!(item, Incoming::Message(_));
        if tx.send(item).is_err() || last {
            return;
        }
    }
}

/// A synchronous LSP client bound to one server process.
///
/// Dropping it performs the polite shutdown (`shutdown` → `exit` → stdin EOF) and kills the process
/// if it does not leave within half a second, so a session never leaks a language server.
pub struct LspClient {
    child: Child,
    stdin: Option<ChildStdin>,
    inbox: Receiver<Incoming>,
    sections: Map<String, Value>,
    next_id: u64,
    next_version: u64,
    finished: bool,
}

impl LspClient {
    /// Starts `command` with piped stdin/stdout and a discarded stderr, and begins reading its
    /// output. `sections` answers the server's `workspace/configuration` requests (section name →
    /// value). The caller owns `current_dir`, arguments and environment of the command.
    pub fn spawn(mut command: Command, sections: Map<String, Value>) -> Result<Self, ClientError> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn().map_err(ClientError::Spawn)?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let (tx, inbox) = mpsc::channel();
        // Built before the thread starts: if anything below fails, `Drop` reaps the child.
        let client = Self {
            child,
            stdin,
            inbox,
            sections,
            next_id: 0,
            next_version: 0,
            finished: false,
        };
        let stdout =
            stdout.ok_or_else(|| ClientError::Protocol("server stdout was not piped".into()))?;
        thread::Builder::new()
            .name("lsp-reader".into())
            .spawn(move || reader_loop(stdout, tx))
            .map_err(ClientError::Spawn)?;
        Ok(client)
    }

    /// The server's OS process id.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Performs the `initialize` handshake (`initialize` request, then the `initialized` notice) with
    /// `root_uri` as the single workspace folder. Returns the server's `initialize` result.
    pub fn initialize(
        &mut self,
        root_uri: &str,
        root_name: &str,
        capabilities: Value,
        deadline: Instant,
    ) -> Result<Value, ClientError> {
        let params = json!({
            "processId": Value::Null,
            "rootUri": root_uri,
            "workspaceFolders": [{"uri": root_uri, "name": root_name}],
            "capabilities": capabilities,
        });
        let result = self.request("initialize", params, deadline)?;
        self.notify("initialized", json!({}))?;
        Ok(result)
    }

    /// Sends a request and waits for its response, answering the server's own requests meanwhile.
    pub fn request(
        &mut self,
        method: &str,
        params: Value,
        deadline: Instant,
    ) -> Result<Value, ClientError> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&rpc_message(Some(id), method, params))?;
        let reply = self
            .pump(deadline, |m| is_response_to(m, id).then(|| m.clone()))?
            .ok_or_else(|| ClientError::Timeout(format!("the response to `{method}`")))?;
        match reply.get("error") {
            Some(err) if !err.is_null() => Err(ClientError::ServerError {
                method: method.to_string(),
                message: err
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            }),
            _ => Ok(reply.get("result").cloned().unwrap_or(Value::Null)),
        }
    }

    /// Sends a notification (no response expected). A null `params` is omitted from the wire.
    pub fn notify(&mut self, method: &str, params: Value) -> Result<(), ClientError> {
        self.send(&rpc_message(None, method, params))
    }

    /// `textDocument/didOpen` with a **fresh, strictly increasing version**, which is returned: it is
    /// the key [`LspClient::wait_publish`] matches on. Versions are never reused, even after a close.
    pub fn open(&mut self, uri: &str, language_id: &str, text: &str) -> Result<u64, ClientError> {
        self.next_version += 1;
        let version = self.next_version;
        self.notify(
            "textDocument/didOpen",
            json!({"textDocument": {
                "uri": uri, "languageId": language_id, "version": version, "text": text,
            }}),
        )?;
        Ok(version)
    }

    /// `textDocument/didClose`.
    pub fn close(&mut self, uri: &str) -> Result<(), ClientError> {
        self.notify(
            "textDocument/didClose",
            json!({"textDocument": {"uri": uri}}),
        )
    }

    /// Waits for the `publishDiagnostics` of exactly `(uri, version)` and returns its diagnostics.
    ///
    /// Publications of another version or with a null version are skipped (see the module docs), so
    /// this never returns the cleanup of an earlier open. [`ClientError::Timeout`] when `deadline`
    /// passes first; [`ClientError::Closed`] when the server dies while we wait.
    pub fn wait_publish(
        &mut self,
        uri: &str,
        version: u64,
        deadline: Instant,
    ) -> Result<Vec<Value>, ClientError> {
        self.pump(deadline, |m| {
            is_publish_for(m, uri, version).then(|| diagnostics_of(m))
        })?
        .ok_or_else(|| {
            ClientError::Timeout(format!("publishDiagnostics for {uri} (version {version})"))
        })
    }

    /// Polite shutdown, then a bounded wait, then `kill`. Idempotent; also run by `Drop`.
    pub fn shutdown(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        if self.stdin.is_some() {
            let reply_by = Instant::now() + SHUTDOWN_REPLY_WAIT;
            let _ = self.request("shutdown", Value::Null, reply_by);
            let _ = self.notify("exit", Value::Null);
        }
        self.stdin = None; // EOF: a server that ignored `exit` still sees its input close
        self.reap();
    }

    fn reap(&mut self) {
        let give_up = Instant::now() + EXIT_GRACE;
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if Instant::now() < give_up => thread::sleep(Duration::from_millis(10)),
                _ => break,
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    fn send(&mut self, msg: &Value) -> Result<(), ClientError> {
        let stdin = self.stdin.as_mut().ok_or(ClientError::Closed)?;
        write_frame(stdin, msg)
    }

    /// Feeds incoming messages to `accept` until it returns `Some`, answering the server's requests
    /// on the way. `Ok(None)` = the deadline passed. A queued message is always examined first, so a
    /// deadline that has already passed still collects what has already arrived.
    fn pump<T>(
        &mut self,
        deadline: Instant,
        mut accept: impl FnMut(&Value) -> Option<T>,
    ) -> Result<Option<T>, ClientError> {
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let Some(msg) = self.next_message(remaining)? else {
                return Ok(None);
            };
            if is_server_request(&msg) {
                self.reply_to(&msg)?;
            } else if let Some(found) = accept(&msg) {
                return Ok(Some(found));
            }
        }
    }

    /// One message from the reader thread, `Ok(None)` when `wait` elapses with nothing queued.
    fn next_message(&mut self, wait: Duration) -> Result<Option<Value>, ClientError> {
        match self.inbox.recv_timeout(wait) {
            Ok(Incoming::Message(msg)) => Ok(Some(msg)),
            Ok(Incoming::Eof) | Err(RecvTimeoutError::Disconnected) => Err(ClientError::Closed),
            Ok(Incoming::Failure(why)) => Err(ClientError::Protocol(why)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
        }
    }

    fn reply_to(&mut self, request: &Value) -> Result<(), ClientError> {
        let result = answer_server_request(request, &self.sections);
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        self.send(&json!({"jsonrpc": "2.0", "id": id, "result": result}))
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn rpc_message(id: Option<u64>, method: &str, params: Value) -> Value {
    let mut msg = json!({"jsonrpc": "2.0", "method": method});
    if let Some(id) = id {
        msg["id"] = json!(id);
    }
    if !params.is_null() {
        msg["params"] = params;
    }
    msg
}

fn diagnostics_of(publish: &Value) -> Vec<Value> {
    publish
        .pointer("/params/diagnostics")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;
    use std::path::Path;
    use std::time::Duration;

    // ── framing ────────────────────────────────────────────────────────────

    fn frame_bytes(v: &Value) -> Vec<u8> {
        let mut buf = Vec::new();
        write_frame(&mut buf, v).expect("write to a Vec cannot fail");
        buf
    }

    #[test]
    fn framing_round_trip_in_buffer() {
        let msg = json!({"jsonrpc":"2.0","id":7,"method":"x","params":{"a":[1,2,3],"s":"ação"}});
        let bytes = frame_bytes(&msg);
        assert!(bytes.starts_with(b"Content-Length: "));
        let back = read_frame(&mut Cursor::new(bytes)).unwrap().unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn framing_counts_bytes_not_chars() {
        let msg = json!({"s":"ação — ✓"});
        let body = serde_json::to_vec(&msg).unwrap();
        assert!(
            body.len() > "ação — ✓".chars().count() + 6,
            "fixture must be multi-byte"
        );
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        assert!(frame_bytes(&msg).starts_with(header.as_bytes()));
    }

    #[test]
    fn framing_reads_consecutive_frames_then_eof_is_none() {
        let mut buf = frame_bytes(&json!({"n":1}));
        buf.extend(frame_bytes(&json!({"n":2})));
        let mut cur = Cursor::new(buf);
        assert_eq!(read_frame(&mut cur).unwrap(), Some(json!({"n":1})));
        assert_eq!(read_frame(&mut cur).unwrap(), Some(json!({"n":2})));
        assert_eq!(read_frame(&mut cur).unwrap(), None);
    }

    #[test]
    fn framing_accepts_any_header_case_and_extra_headers() {
        let body = br#"{"ok":true}"#;
        let mut raw = format!(
            "content-type: application/vscode-jsonrpc; charset=utf-8\r\nCONTENT-LENGTH: {}\r\n\r\n",
            body.len()
        )
        .into_bytes();
        raw.extend_from_slice(body);
        assert_eq!(
            read_frame(&mut Cursor::new(raw)).unwrap(),
            Some(json!({"ok":true}))
        );
    }

    #[test]
    fn framing_rejects_missing_length_truncated_body_and_oversize() {
        let missing = b"X-Other: 1\r\n\r\n{}".to_vec();
        assert!(matches!(
            read_frame(&mut Cursor::new(missing)),
            Err(ClientError::Protocol(_))
        ));
        let truncated = b"Content-Length: 50\r\n\r\n{\"a\":1}".to_vec();
        assert!(matches!(
            read_frame(&mut Cursor::new(truncated)),
            Err(ClientError::Protocol(_))
        ));
        let huge = b"Content-Length: 99999999999\r\n\r\n".to_vec();
        assert!(matches!(
            read_frame(&mut Cursor::new(huge)),
            Err(ClientError::Protocol(_))
        ));
    }

    // ── answers to server requests ─────────────────────────────────────────

    #[test]
    fn configuration_request_answers_each_section_and_null_for_unknown() {
        let mut sections = Map::new();
        sections.insert("python".into(), json!({"pythonPath":"/p/python"}));
        sections.insert(
            "python.analysis".into(),
            json!({"typeCheckingMode":"standard"}),
        );
        let msg = json!({"jsonrpc":"2.0","id":5,"method":"workspace/configuration",
            "params":{"items":[{"section":"python"},{"section":"python.analysis"},{"section":"outra"},{}]}});
        assert_eq!(
            answer_server_request(&msg, &sections),
            json!([{"pythonPath":"/p/python"},{"typeCheckingMode":"standard"},null,null])
        );
    }

    #[test]
    fn any_other_server_request_answers_null() {
        let sections = Map::new();
        for method in [
            "client/registerCapability",
            "window/workDoneProgress/create",
        ] {
            let msg = json!({"jsonrpc":"2.0","id":1,"method":method,"params":{}});
            assert_eq!(answer_server_request(&msg, &sections), Value::Null);
        }
    }

    // ── uri / publish matching (pure) ──────────────────────────────────────

    #[test]
    fn same_uri_ignores_percent_encoding_differences() {
        assert!(same_uri("file:///a/b.py", "file:///a/b.py"));
        assert!(same_uri("file:///a/b%20c.py", "file:///a/b c.py"));
        assert!(same_uri("file:///caf%C3%A9.py", "file:///café.py"));
        assert!(!same_uri("file:///a/b.py", "file:///a/c.py"));
    }

    #[test]
    fn publish_match_requires_uri_and_exact_version() {
        let uri = "file:///p/a.py";
        let publish = |ver: Value, u: &str| {
            json!({"jsonrpc":"2.0","method":"textDocument/publishDiagnostics",
                "params":{"uri":u,"version":ver,"diagnostics":[]}})
        };
        assert!(is_publish_for(&publish(json!(3), uri), uri, 3));
        assert!(
            !is_publish_for(&publish(Value::Null, uri), uri, 3),
            "version null (the didClose cleanup) must be ignored"
        );
        assert!(!is_publish_for(&publish(json!(2), uri), uri, 3));
        assert!(!is_publish_for(
            &publish(json!(3), "file:///p/b.py"),
            uri,
            3
        ));
        let other_kind = json!({"method":"window/logMessage","params":{"uri":uri,"version":3}});
        assert!(!is_publish_for(&other_kind, uri, 3));
        let no_version = json!({"method":"textDocument/publishDiagnostics",
            "params":{"uri":uri,"diagnostics":[]}});
        assert!(!is_publish_for(&no_version, uri, 3));
    }

    // ── fake server (python3 -c) ───────────────────────────────────────────

    /// A scripted LSP server. Modes (argv[1]): `ok` (default) publishes, per `didOpen`, a null-version
    /// cleanup, a stale version, another uri, THEN the right version, then a trailing cleanup (the late
    /// `didClose` artefact); `silencioso` never publishes; `morre` exits on `didOpen`; `surdo` hangs on
    /// `shutdown` so the client must kill it. On `initialized` it asks `workspace/configuration` and
    /// echoes the client's answer inside the right publication (`x_config`).
    const FAKE_SERVER: &str = r##"
import sys, json, time
mode = sys.argv[1] if len(sys.argv) > 1 else "ok"
rin, out = sys.stdin.buffer, sys.stdout.buffer
pending = []

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
    out.write(b"Content-Length: %d\r\n\r\n" % len(b) + b)
    out.flush()

def pub(uri, ver, diags):
    send({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics",
          "params": {"uri": uri, "version": ver, "diagnostics": diags}})

config = None
while True:
    m = pending.pop(0) if pending else read()
    meth = m.get("method")
    if meth == "initialize":
        send({"jsonrpc": "2.0", "id": m["id"], "result": {"capabilities": {}}})
    elif meth == "initialized":
        send({"jsonrpc": "2.0", "id": 900, "method": "workspace/configuration",
              "params": {"items": [{"section": "python"}, {"section": "outra"}]}})
        while True:
            r = read()
            if r.get("method") is None and r.get("id") == 900:
                config = r.get("result")
                break
            pending.append(r)
    elif meth == "shutdown":
        if mode == "surdo":
            time.sleep(3600)
        send({"jsonrpc": "2.0", "id": m["id"], "result": None})
    elif meth == "exit":
        sys.exit(0)
    elif meth == "textDocument/didOpen":
        td = m["params"]["textDocument"]
        uri, v = td["uri"], td["version"]
        if mode == "silencioso":
            continue
        if mode == "morre":
            sys.exit(0)
        pub(uri, None, [])
        pub(uri, v - 1, [{"message": "obsoleto"}])
        pub(uri + "_outro", v, [{"message": "outra uri"}])
        pub(uri, v, [{"code": "reportUndefinedVariable", "message": "\"fantasma\" is not defined",
                      "x_config": config, "x_text": td["text"]}])
        pub(uri, None, [])
"##;

    fn fake(mode: &str) -> LspClient {
        let mut cmd = Command::new("python3");
        cmd.arg("-c").arg(FAKE_SERVER).arg(mode);
        let mut sections = Map::new();
        sections.insert("python".into(), json!({"pythonPath":"/x/python"}));
        let mut client = LspClient::spawn(cmd, sections)
            .expect("python3 must be on PATH to run the fake LSP server");
        client
            .initialize(
                "file:///proj",
                "proj",
                json!({}),
                Instant::now() + Duration::from_secs(10),
            )
            .expect("fake server initialize");
        client
    }

    fn in_secs(n: u64) -> Instant {
        Instant::now() + Duration::from_secs(n)
    }

    #[test]
    fn wait_publish_skips_null_stale_and_other_uri_and_returns_the_right_version() {
        let mut c = fake("ok");
        let uri = "file:///proj/a.py";
        let v = c.open(uri, "python", "x = 1\n").unwrap();
        let diags = c.wait_publish(uri, v, in_secs(10)).unwrap();
        assert_eq!(
            diags.len(),
            1,
            "the null-version cleanup (0 diagnostics) must not win"
        );
        assert_eq!(diags[0]["message"], "\"fantasma\" is not defined");
        c.close(uri).unwrap();
    }

    #[test]
    fn reopening_the_same_uri_never_returns_the_previous_cleanup() {
        let mut c = fake("ok");
        let uri = "file:///proj/a.py";
        let v1 = c.open(uri, "python", "primeiro\n").unwrap();
        let first = c.wait_publish(uri, v1, in_secs(10)).unwrap();
        c.close(uri).unwrap();
        let v2 = c.open(uri, "python", "segundo\n").unwrap();
        let second = c.wait_publish(uri, v2, in_secs(10)).unwrap();
        assert_eq!(first[0]["x_text"], "primeiro\n");
        assert_eq!(
            second.len(),
            1,
            "the trailing cleanup of open #1 must be skipped"
        );
        assert_eq!(
            second[0]["x_text"], "segundo\n",
            "result must be the one of the 2nd content"
        );
    }

    #[test]
    fn open_versions_are_strictly_monotonic() {
        let mut c = fake("ok");
        let uri = "file:///proj/a.py";
        let versions: Vec<u64> = (0..4)
            .map(|_| c.open(uri, "python", "x\n").unwrap())
            .collect();
        assert!(versions.windows(2).all(|w| w[1] > w[0]), "{versions:?}");
        assert_eq!(versions[0], 1);
    }

    #[test]
    fn configuration_requests_are_answered_with_the_declared_sections() {
        let mut c = fake("ok");
        let uri = "file:///proj/a.py";
        let v = c.open(uri, "python", "x\n").unwrap();
        let diags = c.wait_publish(uri, v, in_secs(10)).unwrap();
        assert_eq!(
            diags[0]["x_config"],
            json!([{"pythonPath":"/x/python"}, null])
        );
    }

    #[test]
    fn wait_publish_times_out_when_the_server_stays_silent() {
        let mut c = fake("silencioso");
        let uri = "file:///proj/a.py";
        let v = c.open(uri, "python", "x\n").unwrap();
        let t0 = Instant::now();
        let err = c
            .wait_publish(uri, v, Instant::now() + Duration::from_millis(300))
            .unwrap_err();
        let waited = t0.elapsed();
        assert!(matches!(err, ClientError::Timeout(_)), "{err:?}");
        assert!(
            waited >= Duration::from_millis(250),
            "returned too early: {waited:?}"
        );
        assert!(
            waited < Duration::from_millis(1500),
            "blocked past the deadline: {waited:?}"
        );
    }

    #[test]
    fn wait_publish_reports_closed_when_the_server_dies() {
        let mut c = fake("morre");
        let uri = "file:///proj/a.py";
        let v = c.open(uri, "python", "x\n").unwrap();
        let err = c.wait_publish(uri, v, in_secs(10)).unwrap_err();
        assert!(matches!(err, ClientError::Closed), "{err:?}");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn drop_kills_and_reaps_a_server_that_ignores_shutdown() {
        let c = fake("surdo");
        let pid = c.pid();
        assert!(Path::new(&format!("/proc/{pid}")).exists());
        let t0 = Instant::now();
        drop(c);
        assert!(
            t0.elapsed() < Duration::from_secs(3),
            "drop blocked: {:?}",
            t0.elapsed()
        );
        assert!(
            !Path::new(&format!("/proc/{pid}")).exists(),
            "server {pid} must be killed and reaped"
        );
    }
}
