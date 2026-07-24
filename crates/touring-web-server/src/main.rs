//! touring-web-server — binary entrypoint.
//!
//! All logic lives in the library (`src/lib.rs`) so integration tests
//! under `tests/` can drive the router directly without a TCP bind.
//! This shim simply boots the tokio runtime and delegates to
//! [`touring_web_server::run`].

#[tokio::main]
async fn main() {
    touring_web_server::run().await;
}
