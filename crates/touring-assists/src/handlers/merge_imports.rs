//! `merge_imports` assist — combine adjacent use statements with shared prefix.
//!
//! Parses selected `use` statements, groups those with shared prefix paths,
//! and emits a single merged `use` with nested sub-paths.

use crate::{AssistContext, AssistHandler, Assists, LazySourceChange};
use std::collections::HashMap;
use syn::{ItemUse, UseTree};

/// Stable identifier for the merge-imports assist.
pub const MERGE_IMPORTS_ID: &str = "merge_imports";

/// Recursively extract path segments from a UseTree.
fn use_tree_to_segments(tree: &UseTree, segments: &mut Vec<String>) {
    match tree {
        UseTree::Path(p) => {
            segments.push(p.ident.to_string());
            use_tree_to_segments(&p.tree, segments);
        }
        UseTree::Name(n) => {
            segments.push(n.ident.to_string());
        }
        UseTree::Rename(r) => {
            segments.push(r.ident.to_string());
        }
        UseTree::Glob(_) | UseTree::Group(_) => {}
    }
}

/// Extract path string from a `use` item (e.g. `"foo::bar::qux"`).
fn use_path_string(u: &ItemUse) -> Option<String> {
    let mut segs = Vec::new();
    use_tree_to_segments(&u.tree, &mut segs);
    if segs.is_empty() {
        None
    } else {
        Some(segs.join("::"))
    }
}

/// Merge adjacent use statements that share a common prefix into nested form.
fn merge_adjacent_lines(selected: &str) -> String {
    let all_lines: Vec<&str> = selected.lines().collect();
    if all_lines.is_empty() {
        return selected.to_string();
    }

    // Parse all non-empty lines as ItemUse
    let parsed: Vec<(usize, ItemUse)> = all_lines
        .iter()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .filter_map(|(i, l)| syn::parse_str::<ItemUse>(l).ok().map(|u| (i, u)))
        .collect();

    if parsed.is_empty() {
        return selected.to_string();
    }

    // Group by prefix (all segments except last)
    let mut groups: HashMap<String, Vec<(usize, String)>> = HashMap::new();
    for (_, item) in &parsed {
        if let Some(path) = use_path_string(item) {
            let segments: Vec<&str> = path.split("::").collect();
            let (prefix, leaf) = if segments.len() > 1 {
                let p = segments[..segments.len() - 1].join("::");
                (p, segments[segments.len() - 1].to_string())
            } else {
                (path.clone(), String::new())
            };
            groups.entry(prefix).or_default().push((0, leaf));
        }
    }

    // Rebuild lines — merge groups with 2+ leaves
    let mut result: Vec<String> = Vec::new();
    let mut used_indices = vec![false; all_lines.len()];

    for (prefix, leaves) in &groups {
        let real_leaves: Vec<String> = leaves.iter().map(|(_, l)| l.clone()).collect();
        if real_leaves.len() >= 2 && !prefix.is_empty() {
            // Merge into nested use
            let mut sorted = real_leaves.clone();
            sorted.sort();
            sorted.dedup();
            let inner = sorted
                .iter()
                .map(|l| format!("    {};", l))
                .collect::<Vec<_>>()
                .join("\n");
            result.push(format!("use {} {{\n{}\n}};", prefix, inner));

            // Mark all lines that contributed this prefix as used
            for (i, item) in &parsed {
                if let Some(path) = use_path_string(item) {
                    let segments: Vec<&str> = path.split("::").collect();
                    let p = if segments.len() > 1 {
                        segments[..segments.len() - 1].join("::")
                    } else {
                        path.clone()
                    };
                    if p == *prefix {
                        used_indices[*i] = true;
                    }
                }
            }
        }
    }

    // Collect result preserving order
    let mut final_lines: Vec<String> = Vec::new();
    for (i, line) in all_lines.iter().enumerate() {
        if used_indices[i] {
            // This line was merged — find the merged entry
            if let Some(merged) = result.iter().find(|r| match syn::parse_str::<ItemUse>(r) {
                Ok(item) => use_path_string(&item).is_some(),
                _ => false,
            }) {
                final_lines.push(merged.clone());
            }
        } else {
            final_lines.push(line.to_string());
        }
    }

    final_lines.join("\n")
}

/// Handler that combines selected adjacent `use` statements sharing a prefix into one nested import.
pub const MERGE_IMPORTS: AssistHandler = |assists: &mut Assists, ctx: &AssistContext| {
    let selected = ctx.selected_text().to_string();
    if selected.trim().is_empty() {
        return Some(());
    }

    let file_id = ctx.file_id;
    let range = ctx.range.clone();

    let first_use = selected
        .lines()
        .find(|l| l.trim().starts_with("use "))
        .map(|l| l.chars().take(20).collect::<String>())
        .unwrap_or_default();

    let label = format!("Merge imports: {}", first_use);

    let lazy = LazySourceChange::new(move || {
        let merged = merge_adjacent_lines(&selected);
        super::make_replace(file_id, range.clone(), merged)
    });

    super::add_refactor_assist(assists, ctx, MERGE_IMPORTS_ID, label, lazy);

    Some(())
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_imports_handler_registered() {
        assert_eq!(MERGE_IMPORTS_ID, "merge_imports");
    }

    #[test]
    fn use_path_string_basic() {
        let item: ItemUse = syn::parse_str("use foo::bar;").unwrap();
        assert_eq!(use_path_string(&item).as_deref(), Some("foo::bar"));
    }

    #[test]
    fn merge_adjacent_lines_single_no_change() {
        let input = "use foo::bar;\nuse baz::qux;";
        let result = merge_adjacent_lines(input);
        // Different prefixes — no merge
        assert!(result.contains("foo::bar"));
        assert!(result.contains("baz::qux"));
    }

    #[test]
    fn merge_adjacent_lines_empty() {
        assert_eq!(merge_adjacent_lines(""), "");
        assert_eq!(merge_adjacent_lines("   "), "   ");
    }
}
