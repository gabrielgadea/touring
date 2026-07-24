//! Plugin registry with lock-free runtime swap support.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use arc_swap::ArcSwap;

use crate::plugin::error::PluginError;
use crate::plugin::r#trait::{PluginFamily, ProviderPlugin};

/// Lock-free plugin registry using `ArcSwap` for hot-path swaps.
///
/// Two-level pattern:
/// - Top-level: `ArcSwap<HashMap<Family, Arc<ArcSwap<FamilyMap>>>>` — swapped atomically
/// - Per-family: `ArcSwap<HashMap<&str, Arc<Box<dyn PP>>>>` — swapped per-family
///
/// `ArcSwapAny::clone()` copies the `Arc` wrapper, which is O(1). No mutex on any path.
type FamilyMap = HashMap<&'static str, Arc<Box<dyn ProviderPlugin>>>;
type FamilyArcSwap = ArcSwap<FamilyMap>;
type PluginFamilyMap = HashMap<PluginFamily, Arc<FamilyArcSwap>>;
/// Tuple type for (family, plugin_id, plugin).
type PluginEntry = (PluginFamily, &'static str, Arc<Box<dyn ProviderPlugin>>);

/// Process-wide registry of provider plugins grouped by family.
///
/// Reads are lock-free via `ArcSwap::load`. Updates are
/// per-family: each entry is itself wrapped in `Arc<ArcSwap<_>>`,
/// so a hot-swap only invalidates readers of that family.
pub struct PluginRegistry {
    /// Top-level: swapped atomically. Each family value is Arc<ArcSwap<..>>,
    /// making per-family updates a single ArcSwap::store() without top-level mutation.
    inner: ArcSwap<PluginFamilyMap>,
}

/// Global registry instance — populated at daemon startup via `populate_global_registry`.
static GLOBAL_REGISTRY: OnceLock<PluginRegistry> = OnceLock::new();

/// Returns the global plugin registry, creating it on first call.
pub fn global_registry() -> &'static PluginRegistry {
    GLOBAL_REGISTRY.get_or_init(PluginRegistry::new)
}

/// Populates the global registry with initial plugin providers.
/// Called once at daemon startup. Idempotent — subsequent calls are no-ops.
pub fn populate_global_registry() {
    let _ = GLOBAL_REGISTRY.get_or_init(PluginRegistry::new);
}

impl PluginRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self {
            inner: ArcSwap::new(Arc::new(HashMap::new())),
        }
    }

    /// Registers a plugin, replacing any existing plugin with the same family+ID.
    pub fn register(&self, plugin: Box<dyn ProviderPlugin>) {
        let family = plugin.family();
        let id = plugin.id();

        let new_inner = {
            let old = self.inner.load();
            let mut new_map = HashMap::new();

            // Copy all families — Arc::clone is O(1) pointer copy
            for (f, arc_aswap) in old.iter() {
                new_map.insert(*f, arc_aswap.clone());
            }

            // Build new family map with plugin inserted
            let new_plugins = {
                let mut m = HashMap::new();
                // Bind the Guard so its lifetime spans the loop; the previous
                // `.map(|a| a.load().as_ref())` returned a reference into a
                // dropped temporary (E0515).
                if let Some(arc_aswap) = new_map.get(&family) {
                    let guard = arc_aswap.load();
                    for (k, v) in guard.iter() {
                        m.insert(*k, v.clone());
                    }
                }
                m.insert(id, Arc::new(plugin));
                m
            };

            new_map.insert(family, Arc::new(ArcSwap::new(Arc::new(new_plugins))));
            new_map
        };

        self.inner.store(Arc::new(new_inner));
    }

    /// Unregisters a plugin by family and ID.
    pub fn unregister(
        &self,
        family: PluginFamily,
        id: &'static str,
    ) -> Option<Box<dyn ProviderPlugin>> {
        let old = self.inner.load();
        let arc_aswap = old.get(&family)?;

        let old_plugins = arc_aswap.load();
        let mut new_plugins = HashMap::new();
        let mut removed = None;

        for (k, v) in old_plugins.iter() {
            if *k == id {
                // Arc::try_unwrap succeeds only when this Arc has a single refcount.
                // If the plugin is still referenced elsewhere, we skip removal.
                if let Ok(plugin) = Arc::try_unwrap(v.clone()) {
                    removed = Some(plugin);
                }
            } else {
                new_plugins.insert(*k, v.clone());
            }
        }

        if removed.is_some() {
            // Move new_plugins into a single Arc once, then clone the Arc per
            // iteration (Arc::clone is O(1) — fixes E0382 move-in-loop).
            let plugins_arc = Arc::new(new_plugins);
            let new_inner = {
                let mut new_map = HashMap::new();
                for (f, arc_aswap) in old.iter() {
                    if *f == family {
                        new_map.insert(*f, Arc::new(ArcSwap::new(plugins_arc.clone())));
                    } else {
                        new_map.insert(*f, arc_aswap.clone());
                    }
                }
                new_map
            };
            self.inner.store(Arc::new(new_inner));
        }
        removed
    }

    /// Returns the currently active plugin for a given family and ID.
    pub fn get(
        &self,
        family: PluginFamily,
        id: &'static str,
    ) -> Option<Arc<Box<dyn ProviderPlugin>>> {
        let old = self.inner.load();
        let arc_aswap = old.get(&family)?;
        arc_aswap.load().get(id).cloned()
    }

    /// Returns the backend for the given family and plugin ID.
    pub fn backend(
        &self,
        family: PluginFamily,
        id: &'static str,
    ) -> Result<Arc<dyn std::any::Any + Send + Sync + 'static>, PluginError> {
        let plugin = self
            .get(family, id)
            .ok_or_else(|| PluginError::PluginNotFound(format!("{}:{}", family.as_str(), id)))?;
        plugin.backend()
    }

    /// Returns all registered family keys.
    pub fn families(&self) -> Vec<PluginFamily> {
        self.inner.load().keys().copied().collect()
    }

    /// Returns all registered plugins as a vector of (family, id, plugin).
    pub fn all_plugins(&self) -> Vec<PluginEntry> {
        let inner = self.inner.load();
        let mut result = Vec::new();
        for (family, arc_swap) in inner.iter() {
            let guard = arc_swap.load();
            for (id, plugin) in guard.iter() {
                result.push((*family, *id, plugin.clone()));
            }
        }
        result
    }

    /// Reloads a plugin by family and ID, producing a fresh backend via the
    /// existing plugin's `backend()` method and performing an atomic
    /// ArcSwap::store() to make it available on the next read.
    ///
    /// Returns the new backend arc on success, or an error if the plugin was not
    /// found or its backend could not be produced.
    ///
    /// # Lock-Free Design
    /// This method builds a new family map with the reloaded plugin and then
    /// calls `inner.store()` once — a single ArcSwap::store() that replaces the
    /// entire top-level arc. Readers see the new map on their next access with
    /// zero contention.
    pub fn reload_plugin(
        &self,
        family: PluginFamily,
        id: &'static str,
    ) -> Result<Arc<dyn std::any::Any + Send + Sync + 'static>, PluginError> {
        let old = self.inner.load();
        let _ = old
            .get(&family)
            .ok_or_else(|| PluginError::PluginNotFound(format!("{}:{}", family.as_str(), id)))?;

        // Build a new family map with a fresh backend for the target plugin.
        // All other families/plugins are cloned (O(1) Arc::clone) so no
        // allocations on the hot read path.
        let new_inner = {
            let mut new_map = HashMap::new();
            for (f, arc_swap) in old.iter() {
                if *f == family {
                    // Rebuild the family's ArcSwap with the updated plugins map.
                    let family_plugins = {
                        let mut m = HashMap::new();
                        let guard = arc_swap.load();
                        for (k, v) in guard.iter() {
                            m.insert(*k, v.clone());
                        }
                        // Inject the fresh backend plugin.
                        if let Some(plugin_arc) = m.get(id) {
                            let fresh = (*plugin_arc).backend()?;
                            // try_unwrap succeeds only when this Arc has a single refcount.
                            // If external refs exist, we cannot move out — reload fails gracefully.
                            if let Ok(plugin_box) = Arc::try_unwrap(plugin_arc.clone()) {
                                let new_plugin = plugin_box.with_fresh_backend(fresh.clone())?;
                                m.insert(id, Arc::new(new_plugin));
                            }
                        }
                        m
                    };
                    new_map.insert(*f, Arc::new(ArcSwap::new(Arc::new(family_plugins))));
                } else {
                    new_map.insert(*f, arc_swap.clone());
                }
            }
            new_map
        };

        self.inner.store(Arc::new(new_inner));

        // Return the fresh backend for caller confirmation.
        self.backend(family, id)
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}
