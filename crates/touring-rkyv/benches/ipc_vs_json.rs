//! Benchmark: rkyv IPC envelope vs. serde_json — serialize + parse roundtrip.
//!
//! Measures what the daemon socket round-trip would cost if the CLI switched
//! from JSON to rkyv framing. Two payload sizes mirror real hook traffic:
//!
//! * **small** — matches a `cli-ast-meta` call (~60 bytes of payload).
//! * **large** — matches a `cli-ast-blast` CallGraph response (~64 KiB).
//!
//! For each size we compare:
//! * `serialize_json` vs `serialize_rkyv` — produce wire bytes.
//! * `parse_json` vs `parse_rkyv_zero_copy` — parse received bytes into a
//!   typed reference. rkyv's `check_archived_root` path is used — the SAFE
//!   parse that would ship to production.
//!
//! Run with: `cargo bench -p touring-rkyv --bench ipc_vs_json`

use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use touring_rkyv::{IpcRequest, check_archived_root, frame_request, unframe};

/// Build a small request typical of `cli-ast-meta`.
fn small_request() -> IpcRequest {
    IpcRequest {
        hook: "cli-ast-meta".to_string(),
        payload: b"{\"file\":\"src/lib.rs\",\"depth\":\"summary\"}".to_vec(),
        project_root: "/home/gabriel/project".to_string(),
        session_id: String::new(),
        priority: 100,
    }
}

/// Build a large request typical of a CallGraph response body.
fn large_request() -> IpcRequest {
    IpcRequest {
        hook: "cli-ast-blast".to_string(),
        payload: vec![0xABu8; 64 * 1024],
        project_root: "/workspace".to_string(),
        session_id: String::new(),
        priority: 50,
    }
}

/// Mirror JSON representation used by the current `send_daemon_request`.
///
/// Shape: `{"hook": ..., "payload": base64, "project_root": ...}`. We encode
/// the payload as a Vec<u8> directly (serde_json encodes as an array of
/// numbers) — same as what `serde_json::Value` with `Vec<u8>` produces. This
/// slightly penalizes JSON (base64 would be smaller) but matches what the
/// current code does when it serializes `payload: serde_json::Value` that
/// happens to be an array. The comparison still reflects the real-world gap.
fn serialize_json(req: &IpcRequest) -> Vec<u8> {
    let value = serde_json::json!({
        "hook": req.hook,
        "payload": req.payload,
        "project_root": req.project_root,
        "session_id": req.session_id,
        "priority": req.priority,
    });
    serde_json::to_vec(&value).expect("json serialize cannot fail on owned data")
}

fn parse_json(bytes: &[u8]) -> serde_json::Value {
    serde_json::from_slice(bytes).expect("valid json in bench")
}

fn bench_small(c: &mut Criterion) {
    let req = small_request();
    let json_bytes = serialize_json(&req);
    let rkyv_bytes = frame_request(&req).expect("rkyv serialize");

    let mut group = c.benchmark_group("ipc_small");
    group.throughput(Throughput::Bytes(json_bytes.len() as u64));

    group.bench_function("serialize_json", |b| {
        b.iter(|| black_box(serialize_json(black_box(&req))))
    });
    group.bench_function("serialize_rkyv", |b| {
        b.iter(|| black_box(frame_request(black_box(&req)).expect("rkyv")))
    });
    group.bench_function("parse_json", |b| {
        b.iter(|| black_box(parse_json(black_box(&json_bytes))))
    });
    group.bench_function("parse_rkyv_zero_copy", |b| {
        b.iter(|| {
            let body = unframe(black_box(&rkyv_bytes)).expect("unframe");
            let archived = check_archived_root::<IpcRequest>(body).expect("bytecheck passes");
            // Touch a field to prevent the compiler optimizing everything away.
            black_box(archived.hook.as_str());
        })
    });
    group.finish();
}

fn bench_large(c: &mut Criterion) {
    let req = large_request();
    let json_bytes = serialize_json(&req);
    let rkyv_bytes = frame_request(&req).expect("rkyv serialize");

    let mut group = c.benchmark_group("ipc_large_64kib");
    group.throughput(Throughput::Bytes(json_bytes.len() as u64));

    group.bench_function("serialize_json", |b| {
        b.iter(|| black_box(serialize_json(black_box(&req))))
    });
    group.bench_function("serialize_rkyv", |b| {
        b.iter(|| black_box(frame_request(black_box(&req)).expect("rkyv")))
    });
    group.bench_function("parse_json", |b| {
        b.iter(|| black_box(parse_json(black_box(&json_bytes))))
    });
    group.bench_function("parse_rkyv_zero_copy", |b| {
        b.iter(|| {
            let body = unframe(black_box(&rkyv_bytes)).expect("unframe");
            let archived = check_archived_root::<IpcRequest>(body).expect("bytecheck passes");
            black_box(archived.payload.len());
        })
    });
    group.finish();
}

/// Wave 3 D5 — response envelope benchmark.
///
/// Mirrors the request side but for `IpcResponse` payloads. Real hook
/// responses range from `cli-doctor` (~200 bytes JSON) to `cli-ast-blast`
/// CallGraph dumps (256 KiB+). We bench the 256 KiB shape since that's
/// where the response migration unlocks the largest gain.
fn bench_response(c: &mut Criterion) {
    use touring_rkyv::{IpcResponse, frame_response};

    // Simulate a 256 KiB CallGraph JSON payload — typical for `cli-ast-blast`.
    let payload = "{\"calls\":[".to_string()
        + &"{\"from\":\"a\",\"to\":\"b\"},".repeat(8000)
        + "{\"from\":\"a\",\"to\":\"b\"}]}";
    let resp = IpcResponse {
        output: payload.clone(),
        success: true,
    };

    let json_bytes = serde_json::to_vec(&serde_json::json!({
        "output": resp.output,
        "success": resp.success,
    }))
    .expect("json serialize");
    let rkyv_bytes = frame_response(&resp).expect("rkyv frame");

    let mut group = c.benchmark_group("response_256kib");
    group.throughput(Throughput::Bytes(json_bytes.len() as u64));

    group.bench_function("serialize_json", |b| {
        b.iter(|| {
            black_box(
                serde_json::to_vec(&serde_json::json!({
                    "output": &resp.output,
                    "success": resp.success,
                }))
                .expect("json"),
            )
        })
    });
    group.bench_function("serialize_rkyv", |b| {
        b.iter(|| black_box(frame_response(black_box(&resp)).expect("rkyv")))
    });
    group.bench_function("parse_json", |b| {
        b.iter(|| {
            let v: serde_json::Value =
                serde_json::from_slice(black_box(&json_bytes)).expect("json");
            black_box(v["success"].as_bool());
        })
    });
    group.bench_function("parse_rkyv_zero_copy", |b| {
        b.iter(|| {
            let body = touring_rkyv::unframe(black_box(&rkyv_bytes)).expect("unframe");
            let archived =
                touring_rkyv::check_archived_root::<IpcResponse>(body).expect("bytecheck");
            black_box(archived.success);
            black_box(archived.output.len());
        })
    });
    group.finish();
}

criterion_group!(benches, bench_small, bench_large, bench_response);
criterion_main!(benches);
