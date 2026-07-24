//! THSF Fase 4 — `blast-radius` capability component.
//!
//! Computes the *transitive* set of files that depend on a target file
//! given a pre-serialised dependency graph. Pure function — no
//! filesystem, no network, no clock. Input and output are JSON bytes to
//! keep the wire shape language-agnostic.
//!
//! Input JSON::
//!
//!     {
//!       "graph": { "a.rs": ["b.rs", "c.rs"], "b.rs": ["c.rs"], ... },
//!       "target": "c.rs"
//!     }
//!
//! The `graph` maps a file path to its *direct dependents* (reverse
//! adjacency list). Reachability from `target` counts everyone who
//! (directly or transitively) depends on it.
//!
//! Output JSON::
//!
//!     {
//!       "target": "c.rs",
//!       "blast_radius": 2,
//!       "dependents": ["a.rs", "b.rs"]
//!     }

use std::collections::{BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Bindings
// ---------------------------------------------------------------------------

wit_bindgen::generate!({
    path: "../../crates/touring-wasm/wit/holon-core.wit",
    world: "holon-component",
});

use exports::holon::core::capabilities::{Guest, InvokeError, InvokeRequest, InvokeResponse};

const CAPABILITY: &str = "blast-radius";

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct BlastInput {
    graph: std::collections::BTreeMap<String, Vec<String>>,
    target: String,
}

#[derive(Serialize)]
struct BlastOutput<'a> {
    target: &'a str,
    blast_radius: usize,
    dependents: Vec<String>,
}

// ---------------------------------------------------------------------------
// Core algorithm — BFS over reverse dependency edges.
// ---------------------------------------------------------------------------

fn compute_blast_radius(input: &BlastInput) -> BlastOutput<'_> {
    let mut visited: BTreeSet<String> = BTreeSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    queue.push_back(input.target.clone());
    visited.insert(input.target.clone());

    while let Some(current) = queue.pop_front() {
        if let Some(dependents) = input.graph.get(&current) {
            for dep in dependents {
                if !visited.contains(dep) {
                    visited.insert(dep.clone());
                    queue.push_back(dep.clone());
                }
            }
        }
    }

    // Remove the target itself from the dependents set.
    visited.remove(&input.target);
    let mut dependents: Vec<String> = visited.into_iter().collect();
    dependents.sort();

    BlastOutput {
        target: &input.target,
        blast_radius: dependents.len(),
        dependents,
    }
}

// ---------------------------------------------------------------------------
// Guest implementation
// ---------------------------------------------------------------------------

struct Component;

impl Guest for Component {
    fn list_capabilities() -> Vec<String> {
        vec![CAPABILITY.to_string()]
    }

    fn invoke(request: InvokeRequest) -> Result<InvokeResponse, InvokeError> {
        if request.capability != CAPABILITY {
            return Err(InvokeError::UnknownCapability(request.capability));
        }

        let input: BlastInput = serde_json::from_slice(&request.args)
            .map_err(|e| InvokeError::InvalidArgs(format!("deserialise BlastInput: {e}")))?;
        let output = compute_blast_radius(&input);
        let stdout = serde_json::to_vec(&output)
            .map_err(|e| InvokeError::Internal(format!("serialise BlastOutput: {e}")))?;

        Ok(InvokeResponse {
            exit_code: 0,
            stdout,
            stderr: Vec::new(),
            duration_ms: 0,
            logged: false,
        })
    }
}

export!(Component);
