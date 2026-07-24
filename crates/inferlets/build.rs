//! Build script for inferlets.
//!
//! Copies the compiled WASM inferlet binary to `wasm_bytes/` for embedding.
//!
//! Usage (after building WASM):
//! ```bash
//! cargo build --target wasm32-wasip1 --release -p inferlets
//! # build.rs copies the .wasm file to wasm_bytes/ on next `cargo` invocation
//! ```

use std::env;
use std::path::Path;

fn main() {
    // Use bare wasm target (no WASI imports) so the binary works
    // with the WasmRunner allowlist that only permits env::log and env::get_config.
    let target = "wasm32-unknown-unknown";
    let profile = if env::var("PROFILE").unwrap_or_default() == "release" {
        "release"
    } else {
        "dev"
    };

    let wasm_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("wasm_bytes");
    std::fs::create_dir_all(&wasm_dir).ok();

    // The cdylib for "inferlets" outputs as `inferlets.wasm`
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(target)
        .join(profile)
        .join("inferlets.wasm");

    let dst = wasm_dir.join("libinferlets.wasm");

    if src.exists() {
        if let Err(e) = std::fs::copy(&src, &dst) {
            eprintln!("inferlets build: failed to copy {:?}: {}", src, e);
        } else {
            println!("cargo:rustc-env=INFERLET_WASM={}", dst.display());
        }
    } else {
        eprintln!(
            "inferlets build: source not found at {:?}. \
             Run `cargo build --target wasm32-wasip1 --release -p inferlets` first.",
            src
        );
    }

    println!("cargo:rustc-env=INFERLETS_WASM_DIR={}", wasm_dir.display());
}
