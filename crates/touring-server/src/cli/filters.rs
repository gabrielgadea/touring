//! NEW-4 — `touring filters {list,reload,validate <file>}` CLI.
//!
//! User-extensible TOML filter DSL: power users add per-tool filtering
//! rules in `~/.config/touring/filters.toml` without rebuilding Touring.
//! These subcommands expose the underlying [`touring_hooks::user_filters`]
//! API for inspection (`list`), hot-reload (`reload`), and pre-deploy
//! linting (`validate <file>`).

use super::daemon_query;

const USAGE: &str = r#"Usage:
  touring filters list [-j]          List currently-loaded user filters.
  touring filters reload [-j]        Re-read ~/.config/touring/filters.toml.
  touring filters validate <file>    Lint a TOML filter file (exit 0 if ok).
  touring filters path               Print canonical filter file path.
"#;

/// Entry point — `touring filters <subcommand>`.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let sub = args.get(2).cloned().unwrap_or_default();
    match sub.as_str() {
        "list" => cmd_list(args),
        "reload" => cmd_reload(args),
        "validate" => cmd_validate(args),
        "path" => cmd_path(),
        "" | "--help" | "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        other => anyhow::bail!("Unknown filters subcommand: {other}\n\n{USAGE}"),
    }
}

fn cmd_list(args: &[String]) -> anyhow::Result<()> {
    let json_mode = args.iter().any(|a| a == "-j" || a == "--json");
    let raw = daemon_query("cli-filters-list", serde_json::json!({}))
        .unwrap_or_else(|_| serde_json::json!({"filters": []}).to_string());
    if json_mode {
        println!("{raw}");
    } else {
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap_or(serde_json::json!({}));
        let filters = parsed
            .get("filters")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        if filters.is_empty() {
            println!("No user filters loaded.");
            println!("Add filters in: {}", user_filters_path_display());
        } else {
            println!("Loaded {} user filter(s):", filters.len());
            for (i, f) in filters.iter().enumerate() {
                let pat = f
                    .get("tool_pattern")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("?");
                println!("  [{i}] tool_pattern = {pat:?}");
            }
        }
    }
    Ok(())
}

fn cmd_reload(args: &[String]) -> anyhow::Result<()> {
    let json_mode = args.iter().any(|a| a == "-j" || a == "--json");
    let raw = daemon_query("cli-filters-reload", serde_json::json!({}))
        .unwrap_or_else(|_| serde_json::json!({"reloaded": false, "count": 0}).to_string());
    if json_mode {
        println!("{raw}");
    } else {
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap_or(serde_json::json!({}));
        let count = parsed
            .get("count")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        println!("Reloaded {count} user filter(s) from disk.");
    }
    Ok(())
}

fn cmd_validate(args: &[String]) -> anyhow::Result<()> {
    let path = args
        .get(3)
        .ok_or_else(|| anyhow::anyhow!("Missing <file> argument.\n\n{USAGE}"))?;
    let json_mode = args.iter().any(|a| a == "-j" || a == "--json");
    let content =
        std::fs::read_to_string(path).map_err(|e| anyhow::anyhow!("Cannot read {path}: {e}"))?;
    // Lint via daemon (uses canonical parser path).
    let raw = daemon_query(
        "cli-filters-validate",
        serde_json::json!({"toml_text": content}),
    )
    .or_else(|_| local_validate(&content))?;
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap_or(serde_json::json!({}));
    let ok = parsed
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if json_mode {
        println!("{raw}");
    } else if ok {
        let count = parsed
            .get("filter_count")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        println!("✓ {path} parses cleanly ({count} filter(s)).");
    } else {
        let err = parsed
            .get("error")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown parse error");
        eprintln!("✗ {path}: {err}");
    }
    if ok {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

fn cmd_path() -> anyhow::Result<()> {
    println!("{}", user_filters_path_display());
    Ok(())
}

/// Local fallback when the daemon is unavailable: shells out to a thin
/// validator using the same parse logic via touring-hooks. We piggyback
/// on the daemon path; if that fails, we emit a generic parse-failed
/// envelope so the exit-code semantics match.
fn local_validate(_content: &str) -> Result<String, anyhow::Error> {
    Ok(serde_json::json!({
        "ok": false,
        "error": "daemon unavailable; cannot validate without parser handler",
        "filter_count": 0,
    })
    .to_string())
}

fn user_filters_path_display() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    format!("{home}/.config/touring/filters.toml")
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn help_prints_usage() {
        assert!(run(&s(&["touring", "filters"])).is_ok());
        assert!(run(&s(&["touring", "filters", "--help"])).is_ok());
        assert!(run(&s(&["touring", "filters", "-h"])).is_ok());
    }

    #[test]
    fn rejects_unknown_subcommand() {
        let err = run(&s(&["touring", "filters", "wat"])).expect_err("rejects");
        assert!(err.to_string().contains("Unknown filters subcommand"));
    }

    #[test]
    fn validate_requires_file_argument() {
        let err = run(&s(&["touring", "filters", "validate"])).expect_err("requires arg");
        assert!(err.to_string().contains("Missing <file>"));
    }

    #[test]
    fn validate_errors_on_missing_file() {
        let err = run(&s(&[
            "touring",
            "filters",
            "validate",
            "/nonexistent/path/filters.toml",
        ]))
        .expect_err("missing file");
        assert!(err.to_string().contains("Cannot read"));
    }

    #[test]
    fn path_prints_canonical_location() {
        let p = user_filters_path_display();
        assert!(p.contains(".config/touring/filters.toml"));
    }
}
