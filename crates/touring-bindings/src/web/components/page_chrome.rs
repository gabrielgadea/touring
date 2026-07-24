//! Shared page chrome — unified hero, KPI strip and panel primitives.
//!
//! Elite W1 (SPEC 2026-06-12 §4.2): `PageHero` gains the composed
//! `stat_denom`/`stat_delta` hero number anatomy (88-140px tabular
//! numeral + "/ 10 000" denominator + mono delta line) and `Panel`
//! gains the optional `num` micro-detail ("01 ·" eyebrow numbering
//! from the elite artboards). Every route renders this same anatomy.

use leptos::prelude::*;

/// Unified page hero — eyebrow, Inter Tight display title (optional
/// italic em segment), sub copy, an actions slot (children) and an
/// optional composed big stat on the right column.
#[component]
pub fn PageHero(
    /// Uppercase eyebrow label above the title.
    eyebrow: &'static str,
    /// Display title (Inter Tight 500, clamp 56-96px).
    title: &'static str,
    /// Optional italic segment appended to the title.
    #[prop(optional)]
    title_em: &'static str,
    /// Supporting copy under the title.
    sub: &'static str,
    /// Optional big reactive stat (right column, 88-140px).
    #[prop(optional)]
    stat: Option<Signal<String>>,
    /// Eyebrow under the stat value.
    #[prop(optional)]
    stat_label: &'static str,
    /// Static denominator rendered after the stat (e.g. "/ 10 000").
    #[prop(optional)]
    stat_denom: &'static str,
    /// Optional mono delta line under the stat ("+212 since session start").
    /// Leading '+' renders pos-green, leading '-' renders neg-rose.
    #[prop(optional)]
    stat_delta: Option<Signal<String>>,
    children: Children,
) -> impl IntoView {
    view! {
        <header class="el-hero">
            <div class="el-hero-main">
                <div class="el-eyebrow">{eyebrow}</div>
                <h1 class="el-hero-title">
                    {title}
                    {(!title_em.is_empty()).then(|| view! {
                        <em class="el-hero-em">{" "}{title_em}</em>
                    })}
                </h1>
                <p class="el-hero-sub">{sub}</p>
                <div class="el-hero-actions">{children()}</div>
            </div>
            {stat.map(|s| view! {
                <div class="el-hero-stat">
                    <div class="el-hero-stat-value">
                        {move || s.get()}
                        {(!stat_denom.is_empty()).then(|| view! {
                            <span class="el-hero-stat-denom">{" "}{stat_denom}</span>
                        })}
                    </div>
                    {stat_delta.map(|d| view! {
                        <div
                            class="el-hero-stat-delta"
                            style=move || format!("color:{}", delta_color(&d.get()))
                        >
                            {move || d.get()}
                        </div>
                    })}
                    <div class="el-eyebrow">{stat_label}</div>
                </div>
            })}
        </header>
    }
}

/// Color token for a delta string by its leading sign.
pub fn delta_color(delta: &str) -> &'static str {
    match delta.trim_start().chars().next() {
        Some('+') => "var(--el-pos)",
        Some('-') | Some('\u{2212}') => "var(--el-neg)",
        _ => "var(--el-fg-4)",
    }
}

/// Four-up KPI strip container — children are [`KpiCell`]s (or custom
/// `.el-kpi-cell` divs for non-textual values like sparklines).
#[component]
pub fn KpiStrip(children: Children) -> impl IntoView {
    view! {
        <section class="el-kpi-strip">{children()}</section>
    }
}

/// Single KPI cell — eyebrow label, reactive value, static sub line.
#[component]
pub fn KpiCell(
    /// Uppercase eyebrow above the value.
    label: &'static str,
    /// Reactive value text.
    value: Signal<String>,
    /// Static sub line under the value.
    sub: &'static str,
) -> impl IntoView {
    view! {
        <div class="el-kpi-cell">
            <div class="el-eyebrow">{label}</div>
            <div class="el-stat el-kpi-value">{move || value.get()}</div>
            <div class="el-kpi-sub">{sub}</div>
        </div>
    }
}

/// Bordered surface panel with an eyebrow label row, an optional panel
/// number ("01 ·" elite micro-detail) and an optional mono command
/// label (e.g. `touring wiring orphans`) on the right.
#[component]
pub fn Panel(
    /// Uppercase eyebrow label.
    eyebrow: &'static str,
    /// Optional elite panel numbering ("01", "02", …).
    #[prop(optional)]
    num: &'static str,
    /// Optional mono CLI-equivalence label on the right.
    #[prop(optional)]
    cmd: &'static str,
    children: Children,
) -> impl IntoView {
    view! {
        <section class="el-panel">
            <div class="el-eyebrow el-panel-label">
                <span>
                    {(!num.is_empty()).then(|| view! {
                        <span class="el-eyebrow-num">{num}" · "</span>
                    })}
                    {eyebrow}
                </span>
                {(!cmd.is_empty()).then(|| view! {
                    <span class="el-cmd-label">{cmd}</span>
                })}
            </div>
            {children()}
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delta_color_by_sign() {
        assert_eq!(delta_color("+212 today"), "var(--el-pos)");
        assert_eq!(delta_color("-34 since W4"), "var(--el-neg)");
        assert_eq!(delta_color("  +1"), "var(--el-pos)");
        assert_eq!(delta_color("flat"), "var(--el-fg-4)");
        assert_eq!(delta_color(""), "var(--el-fg-4)");
    }
}
