//! `auto_wire` assist — suggest wire targets for orphan pub symbols.
//!
//! Invokes `touring wiring suggest` to get ranked wiring suggestions for
//! the selected orphan symbol, then generates a comment block with the
//! ranked consumers and confidence scores.

use std::process::Command;

use crate::{AssistContext, AssistHandler, AssistId, Assists, LazySourceChange};
use serde::Deserialize;

/// auto_wire assist ID.
pub const AUTO_WIRE_ID: AssistId = "auto_wire";

#[derive(Debug, Deserialize)]
struct WiringSuggestResponse {
    // serde ignores the response's `count`/`orphan_symbol` keys (not consumed here);
    // only `suggestions` is read. Dropping the unused fields removes the dead_code
    // suppression instead of hiding it (REGRA #0).
    suggestions: Vec<WiringSuggestionItem>,
}

#[derive(Debug, Deserialize)]
struct WiringSuggestionItem {
    suggested_consumer: String,
    confidence: f64,
    reason: String,
}

/// Run `touring wiring suggest <symbol>` and parse JSON output.
fn get_wiring_suggestions(symbol: &str) -> Vec<(String, f64, String)> {
    let output = match Command::new("touring")
        .args(["wiring", "suggest", symbol])
        .output()
    {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };
    if !output.status.success() {
        return Vec::new();
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let response: WiringSuggestResponse = match serde_json::from_str(&stdout) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    response
        .suggestions
        .into_iter()
        .map(|s| (s.suggested_consumer, s.confidence, s.reason))
        .collect()
}

/// Handler that emits a ranked comment block of wiring targets for the selected orphan symbol.
pub const AUTO_WIRE: AssistHandler = |assists: &mut Assists, ctx: &AssistContext| {
    let selected = ctx.selected_text().to_string();
    if selected.trim().is_empty() {
        return Some(());
    }

    let symbol = selected.trim().to_string();
    let suggestions = get_wiring_suggestions(&symbol);

    let file_id = ctx.file_id;
    let range = ctx.range.clone();

    let label = if let Some(first) = suggestions.first() {
        format!(
            "Wire orphan symbol: {} → {} (confidence={:.2})",
            symbol, first.0, first.1
        )
    } else {
        format!("Wire orphan symbol: {} (no suggestions)", symbol)
    };

    let lazy = LazySourceChange::new(move || {
        let mut insert = String::new();

        if suggestions.is_empty() {
            insert.push_str(&format!(
                "// auto_wire: {} — no wiring suggestions found (symbol may already be wired)\n",
                symbol
            ));
        } else {
            insert.push_str(&format!(
                "// auto_wire: {} wired to {} consumer(s)\n",
                symbol,
                suggestions.len()
            ));
            insert.push_str("// Suggestions (ranked by confidence):\n");
            for (consumer, conf, reason) in &suggestions {
                insert.push_str(&format!(
                    "///   • {}  [confidence={:.2}, reason={}]\n",
                    consumer, conf, reason
                ));
            }
            insert.push_str(&format!(
                "// ---\n// Placeholder: connect `{}` to one of the above consumers\n",
                symbol
            ));
        }

        super::make_replace(file_id, range.clone(), insert)
    });

    super::add_refactor_assist(assists, ctx, AUTO_WIRE_ID, label, lazy);

    Some(())
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_wire_handler_registered() {
        assert_eq!(AUTO_WIRE_ID, "auto_wire");
    }
}
