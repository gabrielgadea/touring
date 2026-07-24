//! `generate_impl` assist — generate impl block for type.
//!
//! Parses the selected type (struct, enum, trait, union) and generates
//! an impl block with actual fields and methods from the type definition.

use crate::{AssistContext, AssistHandler, AssistId, Assists, LazySourceChange};
use syn::Item;

/// Stable identifier for the generate-impl assist.
pub const GENERATE_IMPL_ID: AssistId = "generate_impl";

/// Parse a type and extract structural info: (name, fields, variants)
fn parse_type_for_impl(selected: &str) -> Option<(String, Vec<String>, Vec<String>)> {
    // Try parsing as full Item first
    if let Ok(Item::Struct(s)) = syn::parse_str::<Item>(selected) {
        let name = s.ident.to_string();
        let fields: Vec<String> = s
            .fields
            .iter()
            .filter_map(|f| f.ident.as_ref().map(|i| i.to_string()))
            .collect();
        return Some((name, fields, vec![]));
    }
    if let Ok(Item::Enum(e)) = syn::parse_str::<Item>(selected) {
        let name = e.ident.to_string();
        let variants: Vec<String> = e.variants.iter().map(|v| v.ident.to_string()).collect();
        return Some((name, vec![], variants));
    }
    if let Ok(Item::Trait(t)) = syn::parse_str::<Item>(selected) {
        let name = t.ident.to_string();
        let methods: Vec<String> = t
            .items
            .iter()
            .filter_map(|i| {
                if let syn::TraitItem::Fn(f) = i {
                    Some(f.sig.ident.to_string())
                } else {
                    None
                }
            })
            .collect();
        return Some((name, methods, vec![])); // methods in fields slot, variants empty
    }
    // Try to extract from partial struct/enum
    let trimmed = selected.trim();
    if trimmed.starts_with("struct ") {
        let name = trimmed
            .split_whitespace()
            .nth(1)?
            .trim_matches('{')
            .trim()
            .to_string();
        let fields: Vec<String> = trimmed
            .lines()
            .skip(1)
            .filter_map(|l| {
                let l = l.trim();
                if l.starts_with("pub ") || l.starts_with("field ") || l.starts_with("//") {
                    None
                } else {
                    l.split(':')
                        .next()
                        .map(|f| f.trim().to_string())
                        .filter(|f| !f.is_empty())
                }
            })
            .collect();
        Some((name, fields, vec![]))
    } else if trimmed.starts_with("enum ") {
        let name = trimmed.split_whitespace().nth(1)?.trim().to_string();
        Some((name, vec![], vec![]))
    } else {
        None
    }
}

/// Generate impl block content from parsed type info.
fn build_impl_block(
    type_name: &str,
    fields: &[String],
    variants: &[String],
    is_trait_impl: bool,
) -> String {
    if !fields.is_empty() && !is_trait_impl {
        // Struct impl: generate field accessors
        let param_str = fields
            .iter()
            .map(|f| format!("{}: /* type */", f))
            .collect::<Vec<_>>()
            .join(", ");
        let field_init = fields.to_vec().join(", ");
        let mut impl_body = format!("impl {} {{\n", type_name);
        impl_body.push_str("    /// Creates a new instance.\n");
        impl_body.push_str(&format!(
            "    pub fn new({}) -> Self {{\n        Self {{ {} }}\n    }}\n",
            param_str, field_init
        ));
        for field in fields {
            impl_body.push_str(&format!("    /// Get `{}`.\n", field));
            impl_body.push_str(&format!(
                "    pub fn {}(&self) -> &{{ /* type */ }} {{\n        &self.{}\n    }}\n\n",
                field, field
            ));
        }
        impl_body.push('}');
        impl_body
    } else if !variants.is_empty() {
        // Enum impl: generate variant handlers
        let mut impl_body = format!("impl {} {{\n", type_name);
        for variant in variants {
            impl_body.push_str(&format!("    /// Handle `{}` variant.\n", variant));
            impl_body.push_str(&format!(
                "    pub fn {}(&self) -> &str {{\n        match self {{}}\n    }}\n\n",
                variant
            ));
        }
        impl_body.push('}');
        impl_body
    } else if is_trait_impl {
        // Trait impl: stub methods
        let mut impl_body = format!("impl {} for Self {{\n", type_name);
        for method in fields {
            impl_body.push_str(&format!(
                "    fn {}(&mut self) {{ /* TODO: implement */ }}\n\n",
                method
            ));
        }
        impl_body.push('}');
        impl_body
    } else {
        format!("impl {} {{\n    // TODO: add methods\n}}\n", type_name)
    }
}

/// Handler that generates an `impl` block for the selected type from its fields and variants.
pub const GENERATE_IMPL: AssistHandler = |assists: &mut Assists, ctx: &AssistContext| {
    let selected = ctx.selected_text().to_string();
    if selected.trim().is_empty() {
        return Some(());
    }

    // Try to parse the selected type to extract structural info
    let parsed = parse_type_for_impl(&selected);
    let (type_name, fields, variants) = parsed
        .as_ref()
        .map(|(n, f, v)| (n.clone(), f.clone(), v.clone()))
        .unwrap_or_else(|| {
            let trimmed = selected.trim();
            let name = if trimmed.starts_with("struct ") {
                trimmed
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("Type")
                    .trim_matches('{')
                    .trim()
                    .to_string()
            } else if trimmed.starts_with("enum ") {
                trimmed
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("Type")
                    .trim()
                    .to_string()
            } else {
                trimmed
                    .lines()
                    .next()
                    .unwrap_or("Type")
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("Type")
                    .to_string()
            };
            (name, vec![], vec![])
        });

    let file_id = ctx.file_id;
    let range = ctx.range.clone();
    let is_trait = parsed
        .as_ref()
        .map(|(_, f, v)| v.is_empty() && !f.is_empty() && selected.trim().starts_with("trait "))
        .unwrap_or(false);
    let label = format!("Generate impl for: {}", type_name);

    let lazy = LazySourceChange::new(move || {
        let impl_content = build_impl_block(&type_name, &fields, &variants, is_trait);
        super::make_replace(file_id, range.clone(), impl_content)
    });

    super::add_refactor_assist(assists, ctx, GENERATE_IMPL_ID, label, lazy);

    Some(())
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_impl_handler_registered() {
        assert_eq!(GENERATE_IMPL_ID, "generate_impl");
    }

    #[test]
    fn parse_struct_with_fields() {
        let input = "struct Point { x: f64, y: f64 }";
        let result = parse_type_for_impl(input);
        assert!(result.is_some());
        let (name, fields, variants) = result.unwrap();
        assert_eq!(name, "Point");
        assert_eq!(fields, vec!["x", "y"]);
        assert!(variants.is_empty());
    }

    #[test]
    fn parse_enum_with_variants() {
        let input = "enum Color { Red, Green, Blue }";
        let result = parse_type_for_impl(input);
        assert!(result.is_some());
        let (name, fields, variants) = result.unwrap();
        assert_eq!(name, "Color");
        assert!(fields.is_empty());
        assert_eq!(variants, vec!["Red", "Green", "Blue"]);
    }

    #[test]
    fn parse_trait_with_methods() {
        let input = "trait Drawable { fn draw(&self); fn erase(&mut self); }";
        let result = parse_type_for_impl(input);
        assert!(result.is_some());
        let (name, fields, variants) = result.unwrap();
        assert_eq!(name, "Drawable");
        assert_eq!(fields, vec!["draw", "erase"]);
        assert!(variants.is_empty());
    }

    #[test]
    fn build_impl_block_struct() {
        let result = build_impl_block("Point", &["x".to_string(), "y".to_string()], &[], false);
        assert!(result.contains("impl Point"));
        assert!(result.contains("pub fn new("));
        assert!(result.contains("pub fn x("));
        assert!(result.contains("pub fn y("));
        assert!(!result.contains("todo!()"));
    }

    #[test]
    fn build_impl_block_enum() {
        let result = build_impl_block(
            "Color",
            &[],
            &["Red".to_string(), "Green".to_string()],
            false,
        );
        assert!(result.contains("impl Color"));
        assert!(result.contains("pub fn Red("));
        assert!(result.contains("pub fn Green("));
        assert!(!result.contains("todo!()"));
    }

    #[test]
    fn build_impl_block_trait() {
        let result = build_impl_block("Drawable", &["draw".to_string()], &[], true);
        assert!(result.contains("impl Drawable for Self"));
        assert!(result.contains("fn draw("));
        assert!(!result.contains("todo!()"));
    }
}
