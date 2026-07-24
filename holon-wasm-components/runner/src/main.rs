//! THSF Fase 4 host runner — loads a WebAssembly Component built against
//! the `holon:core@0.1.0` WIT package and drives its `capabilities`
//! interface using wasmtime's low-level component API (no bindgen!).
//!
//! The component exports `holon:core/capabilities@0.1.0` with two
//! functions:
//!   - `list-capabilities() -> list<string>`
//!   - `invoke(request: invoke-request) -> result<invoke-response, invoke-error>`
//!
//! We call these via typed function handles — more verbose than bindgen
//! but stable across wasmtime 42/43/44 and requires zero WIT path
//! resolution at host-build time.
//!
//! Usage::
//!
//!     holon-wasm-runner <component.wasm> list
//!     holon-wasm-runner <component.wasm> invoke <capability> [<json-args>]
//!
//! Exit codes: 0 success; 1 runtime error; 2 usage error.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use wasmtime::component::{Component, Linker, ResourceTable, Val};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

// ---------------------------------------------------------------------------
// Host state — holds WasiCtx + ResourceTable for the WASI 0.2 linker.
// ---------------------------------------------------------------------------

struct HostState {
    wasi: WasiCtx,
    table: ResourceTable,
}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

// Fully-qualified exported instance name in the WIT package.
const CAPABILITIES_INSTANCE: &str = "holon:core/capabilities@0.1.0";

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

enum Cmd {
    List,
    Invoke { capability: String, args: Vec<u8> },
}

struct Args {
    component_path: PathBuf,
    cmd: Cmd,
}

fn parse_args() -> Result<Args, String> {
    let argv: Vec<String> = env::args().collect();
    let prog = argv.first().cloned().unwrap_or_else(|| "runner".into());
    if argv.len() < 3 {
        return Err(format!(
            "usage: {prog} <component.wasm> (list | invoke <capability> [<json-args>])"
        ));
    }
    let component_path = PathBuf::from(&argv[1]);
    let cmd = match argv.get(2).map(String::as_str) {
        Some("list") => Cmd::List,
        Some("invoke") => {
            let capability = argv
                .get(3)
                .cloned()
                .ok_or_else(|| "invoke: capability name required".to_string())?;
            let args = argv
                .get(4)
                .cloned()
                .unwrap_or_else(|| "{}".to_string())
                .into_bytes();
            Cmd::Invoke { capability, args }
        }
        other => return Err(format!("unknown command: {other:?}")),
    };
    Ok(Args { component_path, cmd })
}

// ---------------------------------------------------------------------------
// Runtime driver
// ---------------------------------------------------------------------------

fn run(args: Args) -> anyhow::Result<String> {
    let mut config = Config::new();
    config.wasm_component_model(true);
    let engine = Engine::new(&config)?;
    let component = Component::from_file(&engine, &args.component_path)?;

    let mut linker: Linker<HostState> = Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;

    let state = HostState {
        wasi: WasiCtxBuilder::new().inherit_stdio().build(),
        table: ResourceTable::new(),
    };
    let mut store: Store<HostState> = Store::new(&engine, state);
    let instance = linker.instantiate(&mut store, &component)?;

    // Resolve the exported `capabilities` instance.
    let (_, caps_export_idx) = instance
        .get_export(&mut store, None, CAPABILITIES_INSTANCE)
        .ok_or_else(|| {
            anyhow::anyhow!("component does not export `{CAPABILITIES_INSTANCE}`")
        })?;

    match args.cmd {
        Cmd::List => {
            let (_, list_fn_idx) = instance
                .get_export(&mut store, Some(&caps_export_idx), "list-capabilities")
                .ok_or_else(|| anyhow::anyhow!("export `list-capabilities` missing"))?;
            let func = instance
                .get_func(&mut store, list_fn_idx)
                .ok_or_else(|| anyhow::anyhow!("list-capabilities is not a function"))?;
            let mut results = vec![Val::String(String::new())];
            func.call(&mut store, &[], &mut results)?;
            func.post_return(&mut store)?;
            Ok(val_to_json(results.first()))
        }
        Cmd::Invoke { capability, args } => {
            let (_, invoke_fn_idx) = instance
                .get_export(&mut store, Some(&caps_export_idx), "invoke")
                .ok_or_else(|| anyhow::anyhow!("export `invoke` missing"))?;
            let func = instance
                .get_func(&mut store, invoke_fn_idx)
                .ok_or_else(|| anyhow::anyhow!("invoke is not a function"))?;
            let request_val = build_invoke_request_val(&capability, &args);
            let mut results = vec![Val::Bool(false)];
            func.call(&mut store, &[request_val], &mut results)?;
            func.post_return(&mut store)?;
            Ok(val_to_json(results.first()))
        }
    }
}

fn build_invoke_request_val(capability: &str, args: &[u8]) -> Val {
    Val::Record(vec![
        ("capability".to_string(), Val::String(capability.to_string())),
        (
            "args".to_string(),
            Val::List(args.iter().map(|b| Val::U8(*b)).collect()),
        ),
        (
            "requester".to_string(),
            Val::String("holon-wasm-runner".to_string()),
        ),
        ("timeout-ms".to_string(), Val::U32(30_000)),
    ])
}

fn val_to_json(val: Option<&Val>) -> String {
    match val {
        None => "null".to_string(),
        Some(v) => serde_json::to_string(&val_to_serde(v))
            .unwrap_or_else(|_| format!("{v:?}")),
    }
}

fn val_to_serde(v: &Val) -> serde_json::Value {
    match v {
        Val::Bool(b) => serde_json::Value::Bool(*b),
        Val::S8(n) => serde_json::json!(*n),
        Val::U8(n) => serde_json::json!(*n),
        Val::S16(n) => serde_json::json!(*n),
        Val::U16(n) => serde_json::json!(*n),
        Val::S32(n) => serde_json::json!(*n),
        Val::U32(n) => serde_json::json!(*n),
        Val::S64(n) => serde_json::json!(*n),
        Val::U64(n) => serde_json::json!(*n),
        Val::Float32(n) => serde_json::json!(*n),
        Val::Float64(n) => serde_json::json!(*n),
        Val::Char(c) => serde_json::Value::String(c.to_string()),
        Val::String(s) => serde_json::Value::String(s.clone()),
        Val::List(items) => serde_json::Value::Array(items.iter().map(val_to_serde).collect()),
        Val::Record(fields) => {
            let mut map = serde_json::Map::new();
            for (k, v) in fields {
                map.insert(k.clone(), val_to_serde(v));
            }
            serde_json::Value::Object(map)
        }
        Val::Tuple(items) => serde_json::Value::Array(items.iter().map(val_to_serde).collect()),
        Val::Variant(case, payload) => {
            let mut map = serde_json::Map::new();
            map.insert(
                "case".to_string(),
                serde_json::Value::String(case.clone()),
            );
            if let Some(p) = payload.as_deref() {
                map.insert("payload".to_string(), val_to_serde(p));
            }
            serde_json::Value::Object(map)
        }
        Val::Enum(name) => serde_json::Value::String(name.clone()),
        Val::Option(inner) => match inner.as_deref() {
            Some(v) => val_to_serde(v),
            None => serde_json::Value::Null,
        },
        Val::Result(res) => match res {
            Ok(inner) => {
                let mut map = serde_json::Map::new();
                map.insert(
                    "ok".to_string(),
                    inner.as_deref().map(val_to_serde).unwrap_or(serde_json::Value::Null),
                );
                serde_json::Value::Object(map)
            }
            Err(inner) => {
                let mut map = serde_json::Map::new();
                map.insert(
                    "err".to_string(),
                    inner.as_deref().map(val_to_serde).unwrap_or(serde_json::Value::Null),
                );
                serde_json::Value::Object(map)
            }
        },
        Val::Flags(flags) => serde_json::Value::Array(
            flags.iter().map(|f| serde_json::Value::String(f.clone())).collect(),
        ),
        Val::Resource(_) => serde_json::Value::String("<resource>".to_string()),
        // WASI 0.3 async variants — not used by the minimal spec-version
        // component; rendered as opaque strings.
        _ => serde_json::Value::String("<unsupported-variant>".to_string()),
    }
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(2);
        }
    };
    match run(args) {
        Ok(out) => {
            println!("{out}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("holon-wasm-runner FAILED: {e:#}");
            ExitCode::from(1)
        }
    }
}
