//! End-to-end integration test for the rkyv IPC path.
//!
//! Validates that the daemon's `handle_connection_async` peek-byte dispatch
//! correctly routes rkyv-framed requests through `check_archived_root` into
//! a `DaemonRequest` that downstream dispatch can consume.
//!
//! # Why a mock daemon
//!
//! Spinning up the real `touring-daemon` binary here would pull in 200+
//! crates of runtime state (SQLite, Tantivy, actor threads). The goal of
//! THIS test is protocol correctness — a mock server that mimics the
//! peek-byte branch is sufficient and runs in milliseconds.
//!
//! Full-daemon integration is covered by `cli_handlers_e2e.rs`; this file
//! pins the wire-level contract.
//!
//! Gated by `#[cfg(feature = "rkyv-ipc")]` so the JSON-only default build
//! is unaffected.

#![cfg(feature = "rkyv-ipc")]

use std::path::PathBuf;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use touring_rkyv::{
    IPC_FRAME_HEADER_LEN, IPC_MAGIC, IpcRequest, IpcResponse, check_archived_root, frame_request,
    frame_response, unframe,
};

/// Mock peek-byte dispatcher — mirrors the real logic in
/// `touring_hooks::daemon::handle_connection_async` but without the
/// `HookRuntime` dependency. Echo-serves: whatever `hook` name arrives,
/// it returns `{"output": "echo:<hook>", "success": true}`.
async fn mock_daemon(listener: UnixListener) {
    // Single accept then exit — the test only sends one request.
    let (stream, _) = listener.accept().await.expect("accept");
    let (reader, mut writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(reader);

    let first = match reader.fill_buf().await {
        Ok(buf) if !buf.is_empty() => buf[0],
        _ => return,
    };

    let response_json = match first {
        b'R' => handle_rkyv(&mut reader).await,
        _ => handle_json(&mut reader).await,
    };

    writer
        .write_all(response_json.as_bytes())
        .await
        .expect("write");
    writer.write_all(b"\n").await.expect("nl");
    writer.flush().await.expect("flush");
}

async fn handle_rkyv<R: tokio::io::AsyncRead + Unpin>(reader: &mut BufReader<R>) -> String {
    let mut header = [0u8; IPC_FRAME_HEADER_LEN];
    reader.read_exact(&mut header).await.expect("header");
    assert_eq!(&header[..4], &IPC_MAGIC, "magic mismatch");
    let body_len = u32::from_le_bytes(header[4..8].try_into().expect("slice")) as usize;

    let mut body = vec![0u8; body_len];
    reader.read_exact(&mut body).await.expect("body");

    let archived = check_archived_root::<IpcRequest>(&body).expect("bytecheck");
    format!(
        "{{\"output\":\"echo:{}\",\"success\":true}}",
        archived.hook.as_str()
    )
}

/// Mock daemon that mirrors the inbound protocol on the response side
/// (Wave 3 D4): rkyv request → rkyv response, JSON request → JSON response.
async fn mock_daemon_dual_response(listener: UnixListener) {
    let (stream, _) = listener.accept().await.expect("accept");
    let (reader, mut writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(reader);

    let first = match reader.fill_buf().await {
        Ok(buf) if !buf.is_empty() => buf[0],
        _ => return,
    };

    if first == b'R' {
        // Read+validate inbound rkyv request, then emit rkyv response frame.
        let mut header = [0u8; IPC_FRAME_HEADER_LEN];
        reader.read_exact(&mut header).await.expect("header");
        let body_len = u32::from_le_bytes(header[4..8].try_into().expect("slice")) as usize;
        let mut body = vec![0u8; body_len];
        reader.read_exact(&mut body).await.expect("body");
        let archived = check_archived_root::<IpcRequest>(&body).expect("bytecheck");

        let resp = IpcResponse {
            output: format!("echo:{}", archived.hook.as_str()),
            success: true,
        };
        let frame = frame_response(&resp).expect("frame_response");
        writer.write_all(&frame).await.expect("write rkyv resp");
    } else {
        let json_str = handle_json(&mut reader).await;
        writer
            .write_all(json_str.as_bytes())
            .await
            .expect("write json resp");
        writer.write_all(b"\n").await.expect("nl");
    }
    writer.flush().await.expect("flush");
}

async fn handle_json<R: tokio::io::AsyncRead + Unpin>(reader: &mut BufReader<R>) -> String {
    let mut line = String::new();
    reader.read_line(&mut line).await.expect("line");
    let value: serde_json::Value = serde_json::from_str(line.trim()).expect("valid json");
    format!(
        "{{\"output\":\"echo:{}\",\"success\":true}}",
        value["hook"].as_str().unwrap_or("?")
    )
}

fn socket_path(dir: &TempDir, name: &str) -> PathBuf {
    dir.path().join(name)
}

#[tokio::test]
async fn rkyv_framed_request_reaches_mock_daemon() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sock = socket_path(&dir, "rkyv_ok.sock");
    let listener = UnixListener::bind(&sock).expect("bind");

    let server = tokio::spawn(mock_daemon(listener));

    // Client: emit rkyv-framed request over the socket.
    let mut client = UnixStream::connect(&sock).await.expect("connect");
    let req = IpcRequest {
        hook: "cli-ast-meta".to_string(),
        payload: br#"{"file":"src/lib.rs"}"#.to_vec(),
        project_root: "/tmp/project".to_string(),
        session_id: String::new(),
        priority: 100,
    };
    let frame = frame_request(&req).expect("frame");
    client.write_all(&frame).await.expect("write frame");
    client.shutdown().await.ok();

    // Read server response and verify it confirms the rkyv parse worked.
    let mut resp = String::new();
    client.read_to_string(&mut resp).await.expect("read resp");
    server.await.expect("server join");

    let resp: serde_json::Value = serde_json::from_str(resp.trim()).expect("resp json");
    assert_eq!(resp["success"], true);
    assert_eq!(resp["output"], "echo:cli-ast-meta");
}

#[tokio::test]
async fn rkyv_response_roundtrips_through_socket() {
    // Wave 3 D4: prove the response side of the rkyv envelope reaches the
    // client intact via real UnixStream.
    let dir = tempfile::tempdir().expect("tempdir");
    let sock = socket_path(&dir, "rkyv_resp.sock");
    let listener = UnixListener::bind(&sock).expect("bind");
    let server = tokio::spawn(mock_daemon_dual_response(listener));

    let mut client = UnixStream::connect(&sock).await.expect("connect");
    let req = IpcRequest {
        hook: "cli-doctor".to_string(),
        payload: Vec::new(),
        project_root: "/tmp/project".to_string(),
        session_id: String::new(),
        priority: 0,
    };
    let frame = frame_request(&req).expect("frame");
    client.write_all(&frame).await.expect("write");
    client.shutdown().await.ok();

    let mut resp_bytes = Vec::new();
    client.read_to_end(&mut resp_bytes).await.expect("read");
    server.await.expect("server join");

    // Client-side: parse rkyv response.
    assert_eq!(
        &resp_bytes[..4],
        &IPC_MAGIC,
        "response should be rkyv-framed"
    );
    let body = unframe(&resp_bytes).expect("unframe response");
    let archived = check_archived_root::<IpcResponse>(body).expect("bytecheck");
    assert!(archived.success);
    assert_eq!(archived.output.as_str(), "echo:cli-doctor");
}

#[tokio::test]
async fn json_request_still_works_on_same_dispatcher() {
    // Prove the peek-byte branch correctly routes legacy `{` lines.
    let dir = tempfile::tempdir().expect("tempdir");
    let sock = socket_path(&dir, "json_ok.sock");
    let listener = UnixListener::bind(&sock).expect("bind");

    let server = tokio::spawn(mock_daemon(listener));

    let mut client = UnixStream::connect(&sock).await.expect("connect");
    let line = br#"{"hook":"cli-doctor","payload":{},"project_root":"/"}
"#;
    client.write_all(line).await.expect("write json");
    client.shutdown().await.ok();

    let mut resp = String::new();
    client.read_to_string(&mut resp).await.expect("read resp");
    server.await.expect("server join");

    let resp: serde_json::Value = serde_json::from_str(resp.trim()).expect("resp json");
    assert_eq!(resp["success"], true);
    assert_eq!(resp["output"], "echo:cli-doctor");
}
