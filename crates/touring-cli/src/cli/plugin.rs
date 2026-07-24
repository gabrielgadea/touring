//! CLI plugin handlers (`cli_plugin_*`) — extracted from cli_handlers.rs (A-W2.P3).

use crate::runtime::HookRuntime;

/// List all registered plugin families and their backends.
pub fn cli_plugin_list(_rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let registry = touring_foundation::plugin::global_registry();
    let plugins = registry.all_plugins();
    let mut families_map: std::collections::HashMap<String, Vec<serde_json::Value>> =
        std::collections::HashMap::new();
    for (family, id, plugin) in plugins {
        let family_str = family.as_str().to_string();
        let entry = serde_json::json!(
            { "id" : id, "backend_type" : std::any::type_name_of_val(plugin.as_ref()), }
        );
        families_map.entry(family_str).or_default().push(entry);
    }
    let result: Vec<serde_json::Value> = families_map
        .into_iter()
        .map(|(family, backends)| serde_json::json!({ "family" : family, "backends" : backends }))
        .collect();
    serde_json::to_string(&result).unwrap_or_else(|_| "[]".to_string())
}
/// Get detailed status for a specific plugin by id.
pub fn cli_plugin_status(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let id: &str = match payload.get("id").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return r#"{"error":"missing plugin id"}"#.to_string(),
    };
    let registry = touring_foundation::plugin::global_registry();
    let plugins = registry.all_plugins();
    for (family, plugin_id, plugin) in plugins {
        if plugin_id == id {
            return serde_json::json!(
                { "id" : plugin_id, "family" : family.as_str(), "backend_type" :
                std::any::type_name_of_val(plugin.as_ref()), "registered" : true, }
            )
            .to_string();
        }
    }
    serde_json::json!({ "id" : id, "registered" : false, "error" : "plugin not found" }).to_string()
}
/// Hot-reload a plugin by id (re-reads config, swaps backend atomically).
pub fn cli_plugin_reload(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let id = match payload.get("id").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return r#"{"error":"missing plugin id"}"#.to_string(),
    };
    let registry = touring_foundation::plugin::global_registry();
    let plugins = registry.all_plugins();
    for (family, plugin_id, _plugin) in plugins {
        if plugin_id == id {
            let static_id: &'static str = Box::leak(id.to_string().into_boxed_str());
            match registry.reload_plugin(family, static_id) {
                Ok(_new_backend) => {
                    return serde_json::json!(
                        { "id" : static_id, "family" : family.as_str(), "reloaded" :
                        true, "note" :
                        "atomic swap completed — next access uses new backend" }
                    )
                    .to_string();
                }
                Err(e) => {
                    return serde_json::json!(
                        { "id" : static_id, "family" : family.as_str(), "reloaded" :
                        false, "error" : e.to_string() }
                    )
                    .to_string();
                }
            }
        }
    }
    serde_json::json!({ "id" : id, "reloaded" : false, "error" : "plugin not found" }).to_string()
}
/// Unregister a plugin from the registry.
pub fn cli_plugin_unregister(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let id = match payload.get("id").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => return r#"{"error":"missing plugin id"}"#.to_string(),
    };
    let registry = touring_foundation::plugin::global_registry();
    let plugins = registry.all_plugins();
    let found_family = plugins.iter().find(|(_, plugin_id, _)| *plugin_id == id);
    match found_family {
        Some((family, _, _)) => {
            let static_id: &'static str = Box::leak(id.into_boxed_str());
            match registry.unregister(*family, static_id) {
                Some(_removed) => {
                    serde_json::json!({ "id" : static_id, "unregistered" : true }).to_string()
                }
                None => serde_json::json!(
                    { "id" : static_id, "unregistered" : false, "error" :
                    "plugin in use or not found" }
                )
                .to_string(),
            }
        }
        None => serde_json::json!(
            { "id" : id, "unregistered" : false, "error" : "plugin not found" }
        )
        .to_string(),
    }
}
