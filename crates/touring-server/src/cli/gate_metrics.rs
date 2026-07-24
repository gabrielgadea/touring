//! `touring gate-metrics` — L7-B Gamma enrichment gate observability.
//!
//! Queries the daemon for the current snapshot of the enrichment gate counters
//! and prints the JSON payload. Reports how many times `pre_edit` and
//! `pre_write` took the fast-path vs full enrichment, and the ratio.
//!
//! Sprint 6 (D6.4-T2): output is enriched with `wiring_gate_bypassed_count`
//! from `touring_generator::WIRING_GATE_BYPASSED_COUNT`. The counter lives
//! outside `touring-hooks` (where `GateMetricsSnapshot` is defined), so
//! enrichment happens client-side in this handler — `touring-server`
//! depends on both crates and can read the atomic directly.
//!
//! Wave P3-1.3 W4 (2026-06-11): migrated from manual args loop to clap derive.
//! Single flat struct (no subcommand), `--otel` flag preserved (G6).

use super::daemon_query;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "touring gate-metrics",
    bin_name = "touring gate-metrics",
    about = "Gate observability counters; --otel emits OTLP/JSON GenAI shape"
)]
struct GateMetricsCli {
    /// Emit OpenTelemetry OTLP/JSON metrics envelope (GenAI semantic conventions).
    #[arg(long)]
    otel: bool,
}

/// Run the `gate-metrics` CLI subcommand.
///
/// Takes no subcommand. Connects to the daemon socket and issues a
/// `cli-gate-metrics` query, then enriches the JSON with cross-crate counters
/// before printing. With `--otel` (A4), the enriched counters are re-emitted in
/// the OpenTelemetry OTLP/JSON metrics shape using GenAI semantic-convention
/// attribute names (`gen_ai.system` etc.).
///
/// # Errors
///
/// Returns an error if the daemon socket is unreachable or the daemon
/// reports a failure response.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    // `-j`/`--json` accepted for compatibility (clap-derive migration dropped the
    // legacy global flag); gate-metrics emits JSON by default, so strip it.
    let args: Vec<String> = args
        .iter()
        .filter(|a| a.as_str() != "-j" && a.as_str() != "--json")
        .cloned()
        .collect();
    let cli = match GateMetricsCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    let output = daemon_query("cli-gate-metrics", serde_json::json!({}))?;

    // Enrich with WIRING_GATE_BYPASSED_COUNT from touring-generator.
    // The bypass counter is incremented inside `AnalysisGateAdapter::check`
    // when `TOURING_WIRING_GATE_DISABLED=1` is set (Sprint 2.5 audit feature).
    let bypass_count =
        touring_generator::WIRING_GATE_BYPASSED_COUNT.load(std::sync::atomic::Ordering::Relaxed);

    let mut value = serde_json::from_str::<serde_json::Value>(&output)
        .unwrap_or_else(|_| serde_json::json!({ "raw": output }));
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "wiring_gate_bypassed_count".to_string(),
            serde_json::Value::Number(bypass_count.into()),
        );
    }

    if cli.otel {
        let otlp = to_otlp_genai(&value);
        println!(
            "{}",
            serde_json::to_string_pretty(&otlp).unwrap_or_else(|_| output.clone())
        );
    } else {
        println!("{}", serde_json::to_string(&value).unwrap_or(output));
    }
    Ok(())
}

/// Reshape the flat `{counter: u64}` gate-metrics snapshot into the
/// OpenTelemetry **OTLP/JSON** metrics envelope using **GenAI semantic
/// conventions** (A4). Each integer counter becomes a monotonic `Sum` metric
/// named `gen_ai.touring.ceg.<counter>` (unit `1`, cumulative temporality);
/// nested objects (latency histograms) are flattened one level into
/// `<key>.<subkey>` gauges. The resource carries `gen_ai.system=touring` +
/// `service.name=touring-ceg`, the attribute set a GenAI-aware OTLP collector
/// keys on. This is the export *representation*; a network push to a live
/// collector is the `otlp` tracing feature's job (external endpoint).
fn to_otlp_genai(counters: &serde_json::Value) -> serde_json::Value {
    use serde_json::{Value, json};

    // A monotonic cumulative Sum metric for one integer counter (OTLP/JSON).
    let sum_metric = |name: &str, v: i64| -> Value {
        json!({
            "name": format!("gen_ai.touring.ceg.{name}"),
            "unit": "1",
            "sum": {
                "aggregationTemporality": 2, // CUMULATIVE
                "isMonotonic": true,
                "dataPoints": [{ "asInt": v.to_string() }],
            }
        })
    };

    let mut metrics: Vec<Value> = Vec::new();
    if let Some(obj) = counters.as_object() {
        for (k, val) in obj {
            match val {
                Value::Number(n) if n.is_u64() || n.is_i64() => {
                    metrics.push(sum_metric(k, n.as_i64().unwrap_or(0)));
                }
                // Latency objects {p50_us, p99_us, count, ...} → flatten as gauges.
                Value::Object(inner) => {
                    for (sk, sv) in inner {
                        if let Some(iv) = sv.as_i64() {
                            metrics.push(json!({
                                "name": format!("gen_ai.touring.ceg.{k}.{sk}"),
                                "unit": if sk.ends_with("_us") { "us" } else { "1" },
                                "gauge": { "dataPoints": [{ "asInt": iv.to_string() }] }
                            }));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    json!({
        "resourceMetrics": [{
            "resource": {
                "attributes": [
                    { "key": "service.name", "value": { "stringValue": "touring-ceg" } },
                    { "key": "gen_ai.system", "value": { "stringValue": "touring" } },
                    { "key": "gen_ai.operation.name", "value": { "stringValue": "code_agent_harness" } },
                ]
            },
            "scopeMetrics": [{
                "scope": { "name": "touring.ceg.gate_metrics", "version": env!("CARGO_PKG_VERSION") },
                "metrics": metrics,
            }]
        }]
    })
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    GateMetricsCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> GateMetricsCli {
        GateMetricsCli::try_parse_from(args).expect("args should parse")
    }

    #[test]
    fn bare_gate_metrics_no_otel() {
        let cli = parse(&["gate-metrics"]);
        assert!(!cli.otel);
    }

    #[test]
    fn otel_flag_parses() {
        let cli = parse(&["gate-metrics", "--otel"]);
        assert!(cli.otel);
    }

    #[test]
    fn to_otlp_genai_integer_counter() {
        let counters = serde_json::json!({ "pre_edit_fast_path": 42u64 });
        let otlp = to_otlp_genai(&counters);
        let metrics = otlp["resourceMetrics"][0]["scopeMetrics"][0]["metrics"]
            .as_array()
            .unwrap();
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0]["name"], "gen_ai.touring.ceg.pre_edit_fast_path");
    }

    #[test]
    fn to_otlp_genai_nested_histogram() {
        let counters = serde_json::json!({
            "latency": { "p50_us": 100i64, "p99_us": 500i64 }
        });
        let otlp = to_otlp_genai(&counters);
        let metrics = otlp["resourceMetrics"][0]["scopeMetrics"][0]["metrics"]
            .as_array()
            .unwrap();
        assert_eq!(metrics.len(), 2);
        let names: Vec<&str> = metrics
            .iter()
            .map(|m| m["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"gen_ai.touring.ceg.latency.p50_us"));
        assert!(names.contains(&"gen_ai.touring.ceg.latency.p99_us"));
    }

    #[test]
    fn to_otlp_genai_resource_attributes() {
        let counters = serde_json::json!({});
        let otlp = to_otlp_genai(&counters);
        let attrs = otlp["resourceMetrics"][0]["resource"]["attributes"]
            .as_array()
            .unwrap();
        let keys: Vec<&str> = attrs.iter().map(|a| a["key"].as_str().unwrap()).collect();
        assert!(keys.contains(&"gen_ai.system"));
        assert!(keys.contains(&"service.name"));
    }
}
