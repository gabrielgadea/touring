//! Snapshot tests for async/await branch-counting in complexity analysis.
//!
//! Rationale: `file_digest_signal` (touring-hooks) summarizes per-symbol
//! cyclomatic complexity. For async code, each `.await` is a potential
//! suspension point — whether the complexity engine counts it as a branch
//! is a load-bearing decision that drives the `hot_fns` list in the
//! digest. Snapshotting guards against silent reclassification when the
//! underlying tree-sitter grammar or the traversal rules change.
//!
//! Review: `cargo insta review -p touring-ast`.

use touring_code::ast::complexity::compute_complexity_for_source;
use touring_code::ast::languages::Lang;

/// Normalize complexity output: sort by symbol name for deterministic diffs,
/// keep the raw `(name, cc)` tuple that the digest downstream reads.
fn normalize(entries: Vec<(String, u16)>) -> Vec<(String, u16)> {
    let mut v = entries;
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

#[test]
fn snapshot_rust_async_await_branches() {
    // Three async fns exercising the full branch set inside async bodies:
    // `.await` in expression position, `?` (Try), conditional.
    let source = r#"
async fn fetch_one(url: &str) -> Result<String, std::io::Error> {
    let body = read(url).await?;
    Ok(body)
}

async fn fetch_all(urls: &[String]) -> Result<Vec<String>, std::io::Error> {
    let mut out = Vec::new();
    for u in urls {
        if u.is_empty() {
            continue;
        }
        let body = fetch_one(u).await?;
        out.push(body);
    }
    Ok(out)
}

async fn read(_u: &str) -> Result<String, std::io::Error> {
    Ok(String::new())
}
"#;
    let entries = compute_complexity_for_source(source, Lang::Rust).expect("complexity ok");
    insta::assert_yaml_snapshot!("rust_async_await_branches", normalize(entries));
}

#[test]
fn snapshot_python_async_await_branches() {
    // Python equivalent — different grammar, same semantic surface area.
    // Differences between the two snapshots document grammar-specific
    // branch counting for auditability.
    let source = r#"
async def fetch_one(url):
    body = await read(url)
    if not body:
        raise ValueError("empty")
    return body

async def fetch_all(urls):
    out = []
    for u in urls:
        if not u:
            continue
        body = await fetch_one(u)
        out.append(body)
    return out

async def read(u):
    return ""
"#;
    let entries = compute_complexity_for_source(source, Lang::Python).expect("complexity ok");
    insta::assert_yaml_snapshot!("python_async_await_branches", normalize(entries));
}
