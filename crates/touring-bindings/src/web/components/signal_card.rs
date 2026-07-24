//! Hero card — the single highest-density Sentrux summary on the dashboard.

use crate::web::models::quality::WorkspaceQualitySignal;
use leptos::prelude::*;

/// Hero card — single highest-density Sentrux summary on the dashboard.
///
/// Displays composite score, normalized value, dominant bottleneck, and
/// raw counters (cycles, functions, edges). Bottleneck pill links to
/// `/quality?focus=<axis>`; cycles link to `/wiring`.
#[component]
pub fn SignalCard(
    /// Reactive [`WorkspaceQualitySignal`] driving the card content.
    #[prop(into)]
    signal: Signal<WorkspaceQualitySignal>,
) -> impl IntoView {
    let score = move || signal.get().signal_0_10000;
    let normalized = move || signal.get().signal_normalized;
    let bottleneck = move || signal.get().bottleneck.clone();
    let cycles = move || signal.get().raw.cycle_count;
    let fns = move || signal.get().raw.total_functions;
    let edges = move || signal.get().raw.total_edges;

    let bottleneck_class = move || match bottleneck().as_str() {
        "modularity" => "bn-modularity",
        "acyclicity" => "bn-acyclicity",
        "depth" => "bn-depth",
        "equality" => "bn-equality",
        "redundancy" => "bn-redundancy",
        _ => "bn-tied",
    };

    let aria_summary = move || {
        format!(
            "Sentrux quality signal {} of 10000, normalized {:.3}, bottleneck {}",
            score(),
            normalized(),
            bottleneck(),
        )
    };
    let cycles_href = move || {
        let _ = cycles();
        "/wiring".to_string()
    };
    let bottleneck_href = move || format!("/quality?focus={}", bottleneck());

    view! {
        <section class="signal-card" role="region" aria-label="Sentrux signal hero card" aria-live="polite">
            <a class="hero-link" href="/quality" aria-label=aria_summary>
                <div class="hero">
                    <div class="score-big">{move || score().to_string()}</div>
                    <div class="score-unit">"/ 10000"</div>
                </div>
            </a>
            <div class="meta">
                <span class="normalized">{move || format!("{:.3} norm", normalized())}</span>
                <a class={move || format!("bottleneck {}", bottleneck_class())}
                   href=bottleneck_href
                   aria-label=move || format!("focus on bottleneck {}", bottleneck())>
                    "bottleneck: " {bottleneck}
                </a>
            </div>
            <div class="raw">
                <a class="raw-link" href=cycles_href title="dependency cycles" aria-label="open wiring detail">
                    "⟳ "{cycles}
                </a>
                <span title="total functions">"ƒ "{fns}</span>
                <span title="total edges">"→ "{edges}</span>
            </div>
        </section>
    }
}
