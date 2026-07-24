//! Build script — compiles the Cap'n Proto schema into Rust stubs.
//!
//! Only runs when `bind-capnp` feature is enabled. The schema files are
//! mirrors of the canonical source at
//! `~/.claude/tools/holon/schemas/capnp/`.
//! To prevent drift, a separate CI check validates their SHA-256 match.

fn main() {
    // Only compile Cap'n Proto schemas when bind-capnp feature is active.
    // Without this guard, `capnpc` would be required even for default builds.
    #[cfg(feature = "bind-capnp")]
    {
        println!("cargo:rerun-if-changed=schemas/holon-core.capnp");
        println!("cargo:rerun-if-changed=schemas/holon-generator.capnp");
        println!("cargo:rerun-if-changed=build.rs");

        capnpc::CompilerCommand::new()
            .src_prefix("schemas")
            .file("schemas/holon-core.capnp")
            .file("schemas/holon-generator.capnp")
            .run()
            .expect("capnpc compilation of holon schemas failed");
    }
}
