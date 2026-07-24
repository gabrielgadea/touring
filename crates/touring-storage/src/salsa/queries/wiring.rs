//! Wiring chains query for salsa.
///
/// Uses salsa's Database trait but does not use #[salsa::tracked]
/// since plain types (u32, str) don't implement SalsaStructInDb.
use crate::salsa::ChainTree;
use crate::salsa::FileKey;
use crate::salsa::db::DatabaseImpl;
use crate::salsa::queries::ChainNode;
use rustc_hash::FxHashMap;

/// Compute the wiring chain tree for a symbol.
///
/// This is NOT a #[salsa::tracked] function. Real implementation would
/// consult touring-wiring for actual symbol wiring data.
pub fn wiring_chains_from(_db: &DatabaseImpl, symbol_name: &str) -> ChainTree {
    let mut nodes: Vec<ChainNode> = Vec::new();
    let mut node_index: FxHashMap<String, usize> = FxHashMap::default();

    let root_idx = nodes.len();
    node_index.insert(symbol_name.to_string(), root_idx);
    nodes.push(ChainNode {
        symbol: symbol_name.to_string(),
        file: FileKey::default(),
        children: vec![],
        is_leaf: true,
    });

    ChainTree {
        root: Some(root_idx),
        nodes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_tree_default_is_empty() {
        let tree = ChainTree::default();
        assert!(tree.root.is_none());
        assert!(tree.nodes.is_empty());
    }

    #[test]
    fn wiring_chains_from_simple_symbol() {
        let db = DatabaseImpl::new();
        let tree = wiring_chains_from(&db, "my_function");
        assert!(tree.root.is_some());
        assert_eq!(tree.nodes.len(), 1);
        assert_eq!(tree.nodes[0].symbol, "my_function");
    }
}
