//! Wave 5 (2026-04-18) — side-by-side declarative hook registration.
//!
//! The canonical hook table is `touring_hooks::hook_registry::ALL_DAEMON_HOOK_NAMES`
//! — a hand-maintained list of 138 entries that the daemon dispatch
//! path consults on every incoming request. Maintaining that list by
//! hand is correct and stable, but adding a new hook requires three
//! synchronized edits (handler impl, registry entry, test count).
//!
//! The `inventory` crate offers an alternative: a declarative
//! `submit!` macro that collects entries at **link time**, across all
//! crates in the final binary. This module exposes the plumbing so
//! future hooks can opt into the plugin-style pattern without
//! refactoring the existing registry.
//!
//! # Invariants
//!
//! 1. `ALL_DAEMON_HOOK_NAMES` remains the **single source of truth**
//!    for the daemon dispatch table. Inventory entries are
//!    observability/discovery only — they do not replace dispatch.
//! 2. `collect_inventory_hooks()` is called once during bootstrap to
//!    assert the inventory list is consistent with the manual table
//!    (every inventory entry must exist in the manual table, but not
//!    every manual entry needs an inventory counterpart).
//! 3. Availability is gated behind the `inventory-registry` feature
//!    so a build with `--no-default-features --features all-hooks,...`
//!    continues to compile without pulling in `inventory`.
//!
//! # Usage
//!
//! ```ignore
//! // Register a new hook declaratively — any module can do this:
//! #[cfg(feature = "inventory-registry")]
//! inventory::submit! {
//!     touring_hooks::inventory_registry::HookEntry {
//!         name: "cli-my-new-handler",
//!         description: "Does a new thing",
//!         category: "cli",
//!     }
//! }
//! ```

use serde::{Deserialize, Serialize};

/// A single inventory entry for a hook handler.
///
/// Intentionally small: the critical metadata is `name` (matches the
/// string passed on the wire) + `category` (for grouping in `touring
/// status -j`) + `description` (surface in help text and docs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookEntry {
    /// Hook name as wired in the daemon dispatch table. MUST match
    /// one entry in `touring_hooks::hook_registry::ALL_DAEMON_HOOK_NAMES`.
    pub name: &'static str,
    /// One-line description surfaced to `touring --help` and docs.
    pub description: &'static str,
    /// Category bucket — e.g. "cli", "lifecycle", "inferlet", "analysis".
    pub category: &'static str,
}

#[cfg(feature = "inventory-registry")]
inventory::collect!(HookEntry);

/// Return every `HookEntry` registered via `inventory::submit!` across
/// the linked binary. Returns an empty Vec when the feature is off.
#[must_use]
pub fn collect_inventory_hooks() -> Vec<&'static HookEntry> {
    #[cfg(feature = "inventory-registry")]
    {
        inventory::iter::<HookEntry>.into_iter().collect()
    }
    #[cfg(not(feature = "inventory-registry"))]
    {
        Vec::new()
    }
}

// Sample registration — provides a smoke-test entry so the inventory
// machinery has at least one element to iterate even before consumer
// crates start using it. Remove once real hooks adopt the pattern.
// NOTE: plain `//` comments (not `///`) because rustdoc does not
// generate documentation for macro invocations and `-D unused-doc-comments`
// would otherwise block the build.
#[cfg(feature = "inventory-registry")]
inventory::submit! {
    HookEntry {
        name: "__inventory_smoketest",
        description: "Wave 5 smoketest — replace with real hook registrations",
        category: "internal",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "inventory-registry")]
    fn inventory_collects_smoketest_entry() {
        let entries = collect_inventory_hooks();
        assert!(
            entries.iter().any(|e| e.name == "__inventory_smoketest"),
            "inventory must contain the Wave 5 smoketest entry; got {:?}",
            entries.iter().map(|e| e.name).collect::<Vec<_>>()
        );
    }

    #[test]
    #[cfg(not(feature = "inventory-registry"))]
    fn collection_is_empty_when_feature_disabled() {
        assert!(collect_inventory_hooks().is_empty());
    }

    #[test]
    fn hook_entry_round_trips_through_serde() {
        let entry = HookEntry {
            name: "cli-test",
            description: "desc",
            category: "cli",
        };
        let json = serde_json::to_string(&entry).expect("serialize");
        // Deserialize into an owned variant (can't reuse `&'static str`
        // for deser — that's fine, the on-wire representation owns).
        #[derive(Deserialize)]
        struct OwnedEntry {
            name: String,
            description: String,
            category: String,
        }
        let back: OwnedEntry = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.name, "cli-test");
        assert_eq!(back.category, "cli");
        assert_eq!(back.description, "desc");
    }
}
