//! Snapshot test for deeply nested `ModuleTree` + chained re-exports.
//!
//! Rationale: wiring analysis resolves `pub use` chains up to arbitrary
//! depth when computing fan-in for a symbol. Four-level nesting with
//! re-exports at two different depths stresses the recursion boundary
//! that `touring-analysis::wiring::module_reexports` walks. A regression
//! that truncates the chain at depth N would silently drop fan-in edges
//! and misclassify consumers as orphans.
//!
//! Review: `cargo insta review -p touring-ast`.

use touring_code::ast::languages::Lang;
use touring_code::ast::module_tree::{ModuleNode, ModuleTree};

/// Flatten the tree into `(depth, name, is_pub, sorted_re_exports)` tuples.
/// Matches the shape used by `snapshot_module_tree.rs` so diffs between
/// the two snapshots are directly comparable.
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
fn snapshot_rust_module_tree_four_level_chained_reexports() {
    // Four-level nesting: crate -> layer1 -> layer2 -> layer3 -> leaf.
    // `pub use` chains at each depth forward a different symbol,
    // exercising the full re-export pipeline:
    //   - layer1 renames via `as`
    //   - layer2 forwards verbatim
    //   - layer3 re-exports a leaf struct
    //   - crate root lifts the deepest name to the top
    let source = r#"
pub mod layer1 {
    pub mod layer2 {
        pub mod layer3 {
            pub struct Deep;
            pub use super::super::helper::HelperTrait;
        }
        pub use layer3::Deep as Renamed;
    }
    pub use layer2::Renamed;
}

mod helper {
    pub trait HelperTrait {}
}

pub use layer1::Renamed as TopLevel;
pub use layer1::layer2::layer3::Deep;
"#;
    let tree = ModuleTree::build_from_source_for_lang(source, "lib.rs", Lang::Rust);
    let mut flat = Vec::new();
    flatten(&tree.root, 0, &mut flat);
    insta::assert_yaml_snapshot!("rust_module_tree_four_level", flat);
}
