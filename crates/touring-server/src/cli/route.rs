//! `touring route` — RGAO task routing (C7). Computes a CILA level + routing
//! topology + recommended TACO phases from a 5-component task-scope vector
//! `c = (depth, files, symbols, cognitive, coupling)`, replacing the trivial
//! `u8 → CilaLevel` mapping with a computed heuristic.
//!
//! The orchestrator computes the vector from a task's scope (blast depth, files,
//! symbols from `ast`/`wiring`/`index`; cognitive + coupling from
//! file-knowledge) and passes it via flags. Pure backend in
//! [`touring_server_reasoning::reasoning::route`].

use crate::reasoning::route::{RouteVector, route};

/// Parse the value following `flag` in `args`, defaulting to `default` when the
/// flag is absent or its value does not parse.
fn flag_value<T: std::str::FromStr>(args: &[String], flag: &str, default: T) -> T {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

/// Entry point for the `route` subcommand. Reads the 5-vector from
/// `--depth/--files/--symbols/--cognitive/--coupling` flags; `-j`/`--json`
/// selects JSON output. Always exits `Ok` — routing is advisory.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let json = args.iter().any(|a| a == "-j" || a == "--json");

    let vector = RouteVector {
        depth: flag_value(args, "--depth", 0),
        files: flag_value(args, "--files", 1),
        symbols: flag_value(args, "--symbols", 0),
        cognitive: flag_value(args, "--cognitive", 0.0),
        coupling: flag_value(args, "--coupling", 0.0),
    };
    let result = route(&vector);

    if json {
        println!("{}", serde_json::to_string(&result)?);
    } else {
        println!(
            "level={:?}  mode={:?}  parallelism={}  composite={:.3}",
            result.level, result.routing_mode, result.max_parallelism, result.composite
        );
        println!("phases: {}", result.phases.join(" → "));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn flag_value_parses_present_flag() {
        let a = args(&["touring", "route", "--depth", "7"]);
        assert_eq!(flag_value(&a, "--depth", 0usize), 7);
    }

    #[test]
    fn flag_value_defaults_when_absent() {
        let a = args(&["touring", "route", "--files", "3"]);
        assert_eq!(flag_value(&a, "--depth", 0usize), 0);
        assert_eq!(flag_value(&a, "--files", 1usize), 3);
    }

    #[test]
    fn flag_value_defaults_on_unparseable() {
        let a = args(&["touring", "route", "--cognitive", "notanumber"]);
        assert!((flag_value(&a, "--cognitive", 0.0f64)).abs() < f64::EPSILON);
    }

    #[test]
    fn flag_value_parses_float() {
        let a = args(&["touring", "route", "--coupling", "0.75"]);
        assert!((flag_value(&a, "--coupling", 0.0f64) - 0.75).abs() < 1e-9);
    }
}
