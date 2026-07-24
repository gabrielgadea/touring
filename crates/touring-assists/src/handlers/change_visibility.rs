//! `change_visibility` assist — cycle pub visibility level.
//!
//! Detects the current visibility modifier (pub, pub(crate), pub(super),
//! pub(in path::to::module)) and cycles to the next level in the hierarchy.

use crate::{AssistContext, AssistHandler, AssistId, Assists, LazySourceChange};

/// Stable identifier for the change-visibility assist.
pub const CHANGE_VISIBILITY_ID: AssistId = "change_visibility";

/// Visibility level in the hierarchy order for cycling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VisibilityLevel {
    None,
    Pub,
    PubCrate,
    PubSuper,
    PubIn,
}

impl VisibilityLevel {
    fn next(self) -> Self {
        match self {
            VisibilityLevel::None => VisibilityLevel::Pub,
            VisibilityLevel::Pub => VisibilityLevel::PubCrate,
            VisibilityLevel::PubCrate => VisibilityLevel::PubSuper,
            VisibilityLevel::PubSuper => VisibilityLevel::PubIn,
            VisibilityLevel::PubIn => VisibilityLevel::Pub,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            VisibilityLevel::None => "",
            VisibilityLevel::Pub => "pub",
            VisibilityLevel::PubCrate => "pub(crate)",
            VisibilityLevel::PubSuper => "pub(super)",
            VisibilityLevel::PubIn => "pub(in path)",
        }
    }
}

/// Extract the visibility level and the rest of the item from input string.
fn parse_visibility(input: &str) -> (VisibilityLevel, &str) {
    let input = input.trim();
    if !input.starts_with("pub") {
        return (VisibilityLevel::None, input);
    }

    // pub(in path...) — must check before pub(crate) and pub(super)
    if input.starts_with("pub(in ")
        && let Some(pos) = input.find(')')
    {
        let rest = input[pos + 1..].trim();
        return (VisibilityLevel::PubIn, rest);
    }

    if let Some(after) = input.strip_prefix("pub(crate)") {
        return (VisibilityLevel::PubCrate, after.trim_start());
    }

    if let Some(after) = input.strip_prefix("pub(super)") {
        return (VisibilityLevel::PubSuper, after.trim_start());
    }

    if let Some(stripped) = input.strip_prefix("pub") {
        let rest = stripped.trim_start();
        if rest.is_empty() {
            (VisibilityLevel::Pub, "")
        } else {
            // pub followed by something else (like pub fn, pub struct)
            (VisibilityLevel::Pub, rest)
        }
    } else {
        (VisibilityLevel::None, input)
    }
}

/// Handler that cycles the selected item's visibility to the next level in the hierarchy.
pub const CHANGE_VISIBILITY: AssistHandler = |assists: &mut Assists, ctx: &AssistContext| {
    let selected = ctx.selected_text().to_string();
    if selected.trim().is_empty() {
        return Some(());
    }

    let file_id = ctx.file_id;
    let range = ctx.range.clone();
    let label = format!(
        "Change visibility: {}",
        selected
            .lines()
            .next()
            .unwrap_or("...")
            .chars()
            .take(25)
            .collect::<String>()
    );

    let lazy = LazySourceChange::new(move || {
        let (level, rest) = parse_visibility(&selected);
        let new_level = level.next();
        let new_vis = new_level.as_str();

        let insert = if new_vis.is_empty() {
            rest.to_string()
        } else {
            format!("{} {}", new_vis, rest)
        };

        super::make_replace(file_id, range.clone(), insert)
    });

    super::add_refactor_assist(assists, ctx, CHANGE_VISIBILITY_ID, label, lazy);

    Some(())
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_visibility_handler_registered() {
        assert_eq!(CHANGE_VISIBILITY_ID, "change_visibility");
    }

    #[test]
    fn parse_visibility_pub_none() {
        assert_eq!(
            parse_visibility("fn foo"),
            (VisibilityLevel::None, "fn foo")
        );
        assert_eq!(
            parse_visibility("  struct Bar  "),
            (VisibilityLevel::None, "struct Bar")
        );
    }

    #[test]
    fn parse_visibility_pub() {
        assert_eq!(
            parse_visibility("pub fn foo"),
            (VisibilityLevel::Pub, "fn foo")
        );
        assert_eq!(
            parse_visibility("pub struct Bar"),
            (VisibilityLevel::Pub, "struct Bar")
        );
    }

    #[test]
    fn parse_visibility_pub_crate() {
        let (lvl, rest) = parse_visibility("pub(crate) fn foo");
        assert_eq!(lvl, VisibilityLevel::PubCrate);
        assert_eq!(rest, "fn foo");

        let (lvl, rest) = parse_visibility("pub(crate) struct Bar");
        assert_eq!(lvl, VisibilityLevel::PubCrate);
        assert_eq!(rest, "struct Bar");
    }

    #[test]
    fn parse_visibility_pub_super() {
        let (lvl, rest) = parse_visibility("pub(super) fn foo");
        assert_eq!(lvl, VisibilityLevel::PubSuper);
        assert_eq!(rest, "fn foo");
    }

    #[test]
    fn parse_visibility_pub_in() {
        let (lvl, rest) = parse_visibility("pub(in crate::path) fn foo");
        assert_eq!(lvl, VisibilityLevel::PubIn);
        assert_eq!(rest, "fn foo");
    }

    #[test]
    fn visibility_level_next_cycles() {
        assert_eq!(VisibilityLevel::None.next(), VisibilityLevel::Pub);
        assert_eq!(VisibilityLevel::Pub.next(), VisibilityLevel::PubCrate);
        assert_eq!(VisibilityLevel::PubCrate.next(), VisibilityLevel::PubSuper);
        assert_eq!(VisibilityLevel::PubSuper.next(), VisibilityLevel::PubIn);
        assert_eq!(VisibilityLevel::PubIn.next(), VisibilityLevel::Pub);
    }

    #[test]
    fn visibility_level_as_str() {
        assert_eq!(VisibilityLevel::None.as_str(), "");
        assert_eq!(VisibilityLevel::Pub.as_str(), "pub");
        assert_eq!(VisibilityLevel::PubCrate.as_str(), "pub(crate)");
        assert_eq!(VisibilityLevel::PubSuper.as_str(), "pub(super)");
        assert_eq!(VisibilityLevel::PubIn.as_str(), "pub(in path)");
    }
}
