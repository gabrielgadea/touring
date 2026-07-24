//! Snapshot tests for `ModuleTree::build_from_source` output stability.
//!
//! The module tree drives re-export tracking and import path resolution
//! in wiring analysis — `pub use` chains are the spine of the integration
//! score. Drift here silently breaks fan-in/fan-out accounting.
//!
//! Review: `cargo insta review -p touring-ast`.

use touring_code::ast::languages::Lang;
use touring_code::ast::module_tree::{ModuleNode, ModuleTree};

/// Flatten to (depth, name, is_pub, re_exports) tuples — deterministic and
/// independent of field reorderings inside the serde-derived struct.
fn flatten(node: &ModuleNode, depth: usize, out: &mut Vec<(usize, String, bool, Vec<String>)>) {
    let mut re = node.re_exports.clone();
    re.sort();
    out.push((depth, node.name.clone(), node.is_pub, re));
    let mut children: Vec<&ModuleNode> = node.children.iter().collect();
    children.sort_by(|a, b| a.name.cmp(&b.name));
    for child in children {
        flatten(child, depth + 1, out);
    }
}

#[test]
fn snapshot_rust_module_tree_nested() {
    let source = r#"
pub mod outer {
    pub mod inner {
        pub use crate::foo::Bar;
        pub use crate::foo::Baz as Renamed;
    }
    mod private {}
}

pub use outer::inner::Bar;
"#;
    let tree = ModuleTree::build_from_source_for_lang(source, "lib.rs", Lang::Rust);
    let mut flat = Vec::new();
    flatten(&tree.root, 0, &mut flat);
    insta::assert_yaml_snapshot!("rust_module_tree_nested", flat);
}
