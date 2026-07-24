//! Blast radius consistency test — verifies DependencyCache petgraph implementation.
//!
//! The `DependencyCache` uses a petgraph StableGraph for O(V+E) BFS traversal.
//! This test verifies the correctness of the reverse-BFS blast radius algorithm
//! using synthetic dependency graphs with known structure.

use std::collections::HashSet;
use std::path::PathBuf;
use touring_hooks::dependency_cache::DependencyCache;

fn build_diamond_cache() -> DependencyCache {
    let mut cache = DependencyCache::new();
    cache.add_relation(&PathBuf::from("d.rs"), &PathBuf::from("a.rs"));
    cache.add_relation(&PathBuf::from("a.rs"), &PathBuf::from("b.rs"));
    cache.add_relation(&PathBuf::from("a.rs"), &PathBuf::from("c.rs"));
    cache.add_relation(&PathBuf::from("b.rs"), &PathBuf::from("c.rs"));
    cache
}

fn blast(path: &str, cache: &DependencyCache) -> HashSet<PathBuf> {
    cache
        .blast_radius(&PathBuf::from(path))
        .files()
        .into_iter()
        .collect()
}

#[test]
fn test_blast_radius_leaf_node_c_has_three_dependents() {
    let cache = build_diamond_cache();
    let files = blast("c.rs", &cache);
    assert!(
        !files.contains(&PathBuf::from("c.rs")),
        "c.rs should be excluded"
    );
    assert!(files.contains(&PathBuf::from("b.rs")), "b.rs imports c.rs");
    assert!(
        files.contains(&PathBuf::from("a.rs")),
        "a.rs imports c.rs via b"
    );
    assert!(
        files.contains(&PathBuf::from("d.rs")),
        "d.rs imports c.rs via a and b"
    );
    assert_eq!(files.len(), 3, "3 transitive dependents");
}

#[test]
fn test_blast_radius_intermediate_node_b_has_two_dependents() {
    let cache = build_diamond_cache();
    let files = blast("b.rs", &cache);
    assert!(
        !files.contains(&PathBuf::from("b.rs")),
        "b.rs should be excluded"
    );
    assert!(files.contains(&PathBuf::from("a.rs")), "a.rs imports b.rs");
    assert!(
        files.contains(&PathBuf::from("d.rs")),
        "d.rs imports b.rs via a"
    );
    assert_eq!(files.len(), 2, "2 transitive dependents");
}

#[test]
fn test_blast_radius_root_node_a_has_one_dependent() {
    let cache = build_diamond_cache();
    let files = blast("a.rs", &cache);
    assert!(
        !files.contains(&PathBuf::from("a.rs")),
        "a.rs should be excluded"
    );
    assert!(files.contains(&PathBuf::from("d.rs")), "d.rs imports a.rs");
    assert_eq!(files.len(), 1, "1 direct dependent");
}

#[test]
fn test_blast_radius_isolated_node_has_zero_dependents() {
    let cache = build_diamond_cache();
    let files = blast("x.rs", &cache);
    assert!(files.is_empty(), "isolated node x.rs has no dependents");
}

#[test]
fn test_direct_only_chain() {
    // A -> B -> C: reverse is C <- B <- A.
    let mut cache = DependencyCache::new();
    cache.add_relation(&PathBuf::from("a.rs"), &PathBuf::from("b.rs"));
    cache.add_relation(&PathBuf::from("b.rs"), &PathBuf::from("c.rs"));

    let c_blast = blast("c.rs", &cache);
    assert!(c_blast.contains(&PathBuf::from("b.rs")), "b.rs imports c");
    assert!(
        c_blast.contains(&PathBuf::from("a.rs")),
        "a.rs imports c via b"
    );
    assert_eq!(c_blast.len(), 2, "c.rs has 2 transitive dependents");

    let b_blast = blast("b.rs", &cache);
    assert!(b_blast.contains(&PathBuf::from("a.rs")), "a.rs imports b");
    assert_eq!(b_blast.len(), 1);

    let a_blast = blast("a.rs", &cache);
    assert!(a_blast.is_empty(), "a.rs has no dependents");
}

#[test]
fn test_diamond_graph() {
    // Diamond: A -> B, A -> C, B -> D, C -> D.
    // Reverse: D <- B <- A, D <- C <- A. Dependents of D: {A, B, C}.
    let mut cache = DependencyCache::new();
    cache.add_relation(&PathBuf::from("a.rs"), &PathBuf::from("b.rs"));
    cache.add_relation(&PathBuf::from("a.rs"), &PathBuf::from("c.rs"));
    cache.add_relation(&PathBuf::from("b.rs"), &PathBuf::from("d.rs"));
    cache.add_relation(&PathBuf::from("c.rs"), &PathBuf::from("d.rs"));

    let files = blast("d.rs", &cache);
    assert!(files.contains(&PathBuf::from("a.rs")));
    assert!(files.contains(&PathBuf::from("b.rs")));
    assert!(files.contains(&PathBuf::from("c.rs")));
    assert_eq!(files.len(), 3);
}
