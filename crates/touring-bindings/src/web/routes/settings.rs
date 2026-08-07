//! /settings — system preferences. Elite W2 (SPEC 2026-06-12 §5.17):
//! PageHero + numbered panels over the `.el-*`/`.pg-settings-*` class set.
//!
//! Real interactions only — nothing fake:
//! - THEME swatches drive the SAME `RwSignal<Theme>` context that
//!   `ThemeToggle` consumes (provided in `app.rs`); each click also calls
//!   `apply_theme()` (data-theme flip on `<html>` + localStorage persist,
//!   key `touring-web-theme`). Unavailable palettes (Nord, Solar) render
//!   as honest disabled swatches labelled "soon".
//! - BEHAVIOUR configures the global `RefreshBus` cadence (10/30/60/120s,
//!   0 = paused) and persists it to localStorage key `touring.refresh_secs`;
//!   on mount the persisted value is restored if valid.
//! - WORKSPACE renders live facts: project root + daemon status from
//!   `WorkspaceCtx`, index + bandit counters from `/api/status`
//!   (`touring status -j`; live shape captured 2026-06-12:
//!   `/index/{file_count,symbol_count}`, `/learning/{bandit_type,arm_count}`).
//! - ABOUT reads the real binary version from `/api/health`
//!   (`touring doctor -j` — ARRAY of `{name, status, detail}`; the
//!   `binary_version` entry carries `detail: "touring 30.0.0"`).
//!
//! CSS prefix: `pg-settings-*` (settings.css) over the shared `.el-*` system.

use leptos::prelude::*;
use leptos_meta::Title;
use serde_json::Value;

use crate::web::components::sidebar::NavItem;
use crate::web::components::{Icon, PageHero, Panel};
use crate::web::ctx::{use_refresh_bus, use_workspace};
use crate::web::services::{fetch_health, fetch_status};
use crate::web::theme::{Theme, apply_theme};

/// localStorage key for the RefreshBus cadence preference.
const REFRESH_STORAGE_KEY: &str = "touring.refresh_secs";

/// Refresh cadences offered by the UI, in display order (0 = paused).
const REFRESH_OPTIONS: [u32; 5] = [10, 30, 60, 120, 0];

/// Parse a persisted refresh-interval string. Only the cadences offered
/// by the UI (10/30/60/120/0) are valid — anything else is rejected so a
/// corrupted localStorage entry can never wedge the RefreshBus.
pub fn parse_refresh_secs(raw: &str) -> Option<u32> {
    let v = raw.trim().parse::<u32>().ok()?;
    REFRESH_OPTIONS.contains(&v).then_some(v)
}

/// Button/status label for a refresh cadence — "Paused" for 0, "30s" otherwise.
pub fn refresh_label(secs: u32) -> String {
    if secs == 0 {
        "Paused".to_string()
    } else {
        format!("{secs}s")
    }
}

/// Extract the touring binary version from the `/api/health` payload —
/// an ARRAY of doctor components; the entry with `name == "binary_version"`
/// carries the version string in its `detail` field.
pub fn binary_version(health: &Value) -> Option<String> {
    health.as_array()?.iter().find_map(|c| {
        if c.get("name").and_then(Value::as_str) == Some("binary_version") {
            c.get("detail").and_then(Value::as_str).map(str::to_string)
        } else {
            None
        }
    })
}

/// Index line from `/api/status` — "3176 files · 71436 symbols".
pub fn index_summary(status: &Value) -> String {
    let files = status.pointer("/index/file_count").and_then(Value::as_u64);
    let symbols = status
        .pointer("/index/symbol_count")
        .and_then(Value::as_u64);
    match (files, symbols) {
        (Some(f), Some(s)) => format!("{f} files · {s} symbols"),
        _ => "index unavailable".to_string(),
    }
}

/// Bandit line from `/api/status` — "linucb · 8 arms".
pub fn bandit_summary(status: &Value) -> String {
    let bandit = status
        .pointer("/learning/bandit_type")
        .and_then(Value::as_str);
    let arms = status
        .pointer("/learning/arm_count")
        .and_then(Value::as_u64);
    match (bandit, arms) {
        (Some(b), Some(a)) => format!("{b} · {a} arms"),
        _ => "learning unavailable".to_string(),
    }
}

/// Defensive localStorage handle — `None` outside a browser context.
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}

/// Read + validate the persisted refresh cadence from localStorage.
fn load_refresh_secs() -> Option<u32> {
    let raw = local_storage()?
        .get_item(REFRESH_STORAGE_KEY)
        .ok()
        .flatten()?;
    parse_refresh_secs(&raw)
}

/// Persist the refresh cadence to localStorage (best-effort, fail-open).
fn persist_refresh_secs(secs: u32) {
    if let Some(storage) = local_storage() {
        let _ = storage.set_item(REFRESH_STORAGE_KEY, &secs.to_string());
    }
}

/// `/settings` — theme swatches, RefreshBus cadence, live workspace
/// facts, the full route map and stack/about info (Elite W2 §5.17).
#[component]
pub fn SettingsPage() -> impl IntoView {
    // Same context ThemeToggle consumes — set() triggers the app.rs Effect;
    // apply_theme() is called directly too so the flip is immediate.
    let theme = expect_context::<RwSignal<Theme>>();
    let workspace = use_workspace();
    let bus = use_refresh_bus();

    // On mount, restore the persisted refresh cadence when valid.
    Effect::new(move |prev: Option<()>| {
        if prev.is_some() {
            return;
        }
        if let Some(secs) = load_refresh_secs()
            && bus.interval_secs.get_untracked() != secs
        {
            bus.interval_secs.set(secs);
        }
    });

    // Live counters from /api/status (`touring status -j`), on the bus tick.
    let status = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move { fetch_status().await.ok() }
    });
    // Doctor components from /api/health — version is static per daemon run.
    let health = LocalResource::new(move || async move { fetch_health().await.ok() });

    view! {
        <div class="page pg-settings">
            <Title text="Touring — Settings"/>
            <div class="el-main-editorial">
                <PageHero
                    eyebrow="SYSTEM · PREFERENCES"
                    title="Settings"
                    sub="Theme, refresh cadence and live workspace facts — every control on this page drives real app state and persists across sessions."
                >
                    <a class="el-btn" href="/health">"Health"</a>
                </PageHero>

                // ── 01 · THEME — functional swatches over the real signal ──
                <Panel num="01" eyebrow="THEME">
                    <div class="pg-settings-swatches">
                        <button
                            type="button"
                            class="el-swatch pg-settings-swatch"
                            class:active=move || theme.get() == Theme::Dark
                            on:click=move |_| {
                                theme.set(Theme::Dark);
                                apply_theme(Theme::Dark);
                            }
                        >
                            <div class="el-swatch-bands">
                                <div style="background:#0a0a0d"></div>
                                <div style="background:#111114"></div>
                                <div style="background:#5eead4"></div>
                                <div style="background:#a78bfa"></div>
                                <div style="background:#fafafa"></div>
                            </div>
                            <div class="pg-settings-swatch-name">"Obsidian"</div>
                            <div class="pg-settings-swatch-sub el-mono">"dark"</div>
                        </button>
                        <button
                            type="button"
                            class="el-swatch pg-settings-swatch"
                            class:active=move || theme.get() == Theme::Light
                            on:click=move |_| {
                                theme.set(Theme::Light);
                                apply_theme(Theme::Light);
                            }
                        >
                            <div class="el-swatch-bands">
                                <div style="background:#fafaf8"></div>
                                <div style="background:#ffffff"></div>
                                <div style="background:#0d9488"></div>
                                <div style="background:#8b5cf6"></div>
                                <div style="background:#16161a"></div>
                            </div>
                            <div class="pg-settings-swatch-name">"Daylight"</div>
                            <div class="pg-settings-swatch-sub el-mono">"light"</div>
                        </button>
                        // Honest placeholders — palettes not implemented yet.
                        <button type="button" class="el-swatch disabled pg-settings-swatch" disabled=true>
                            <div class="el-swatch-bands">
                                <div style="background:#2e3440"></div>
                                <div style="background:#3b4252"></div>
                                <div style="background:#88c0d0"></div>
                                <div style="background:#b48ead"></div>
                                <div style="background:#eceff4"></div>
                            </div>
                            <div class="pg-settings-swatch-name">"Nord"</div>
                            <div class="pg-settings-swatch-sub el-mono">"soon"</div>
                        </button>
                        <button type="button" class="el-swatch disabled pg-settings-swatch" disabled=true>
                            <div class="el-swatch-bands">
                                <div style="background:#002b36"></div>
                                <div style="background:#073642"></div>
                                <div style="background:#2aa198"></div>
                                <div style="background:#b58900"></div>
                                <div style="background:#fdf6e3"></div>
                            </div>
                            <div class="pg-settings-swatch-name">"Solar"</div>
                            <div class="pg-settings-swatch-sub el-mono">"soon"</div>
                        </button>
                    </div>
                </Panel>

                // ── 02 · BEHAVIOUR — RefreshBus cadence, persisted ─────────
                <Panel num="02" eyebrow="BEHAVIOUR">
                    <p class="pg-settings-note">
                        "Refresh interval — feeds the global RefreshBus every page subscribes to: one tick re-fetches all live panels app-wide. Paused stops all polling."
                    </p>
                    <div class="pg-settings-refresh" role="group" aria-label="refresh interval">
                        {REFRESH_OPTIONS
                            .into_iter()
                            .map(|secs| {
                                let label = refresh_label(secs);
                                view! {
                                    <button
                                        type="button"
                                        class="el-btn"
                                        class:el-btn-primary=move || bus.interval_secs.get() == secs
                                        on:click=move |_| {
                                            bus.interval_secs.set(secs);
                                            persist_refresh_secs(secs);
                                        }
                                    >
                                        {label}
                                    </button>
                                }
                            })
                            .collect_view()}
                    </div>
                    <p class="pg-settings-current el-mono">
                        {move || format!("current: {} · persisted to localStorage", refresh_label(bus.interval_secs.get()))}
                    </p>
                </Panel>

                // ── 03 · WORKSPACE — live, read-only facts ─────────────────
                <Panel num="03" eyebrow="WORKSPACE" cmd="touring status -j">
                    <div class="el-row pg-settings-row">
                        <span class="pg-settings-key">"Project root"</span>
                        <span class="el-mono pg-settings-val">
                            {move || workspace.project_path.get()}
                        </span>
                    </div>
                    <div class="el-row pg-settings-row">
                        <span class="pg-settings-key">"Daemon"</span>
                        <span class="pg-settings-daemon">
                            <span
                                class="el-pulse"
                                style=move || format!("background:{}", workspace.daemon_status.get().color())
                                aria-hidden="true"
                            ></span>
                            <span>{move || workspace.daemon_status.get().label()}</span>
                        </span>
                    </div>
                    <Suspense fallback=|| view! { <div class="el-skeleton pg-settings-skel" aria-label="loading"></div> }>
                        {move || status.get().map(|opt| {
                            let v = opt.unwrap_or(Value::Null);
                            let index = index_summary(&v);
                            let bandit = bandit_summary(&v);
                            view! {
                                <div class="el-row pg-settings-row">
                                    <span class="pg-settings-key">"Index"</span>
                                    <span class="el-mono pg-settings-val">{index}</span>
                                </div>
                                <div class="el-row pg-settings-row">
                                    <span class="pg-settings-key">"Bandit"</span>
                                    <span>
                                        <span class="el-tag el-tag-on">{bandit}</span>
                                    </span>
                                </div>
                            }
                        })}
                    </Suspense>
                </Panel>

                // ── 04 · ROUTES — real navigation map from NavItem ─────────
                <Panel num="04" eyebrow="ROUTES">
                    <div class="pg-settings-routes">
                        {NavItem::iter()
                            .map(|item| {
                                let route = item.route();
                                view! {
                                    <a class="el-result-row" href=route>
                                        <Icon name=item.icon()/>
                                        <span>{item.label()}</span>
                                        <span class="hint el-mono">{route}</span>
                                    </a>
                                }
                            })
                            .collect_view()}
                    </div>
                </Panel>

                // ── 05 · ABOUT — real binary version + design stamp ────────
                <Panel num="05" eyebrow="ABOUT" cmd="touring doctor -j">
                    <Suspense fallback=|| view! { <div class="el-skeleton pg-settings-skel" aria-label="loading"></div> }>
                        {move || health.get().map(|opt| {
                            let version = opt
                                .as_ref()
                                .and_then(binary_version)
                                .unwrap_or_else(|| "version unavailable".to_string());
                            view! {
                                <div class="el-row pg-settings-row">
                                    <span class="pg-settings-key">"Touring"</span>
                                    <span class="el-mono pg-settings-val">{version}</span>
                                </div>
                            }
                        })}
                    </Suspense>
                    <div class="el-row pg-settings-row">
                        <span class="pg-settings-key">"Design"</span>
                        <span class="pg-settings-val">"Elite design system · SPEC 2026-06-12"</span>
                    </div>
                    <div class="pg-settings-about-actions">
                        <a class="el-btn el-btn-ghost" href="/dashboard">"Back to overview"</a>
                    </div>
                </Panel>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_refresh_secs_accepts_offered_cadences() {
        assert_eq!(parse_refresh_secs("0"), Some(0));
        assert_eq!(parse_refresh_secs("10"), Some(10));
        assert_eq!(parse_refresh_secs("30"), Some(30));
        assert_eq!(parse_refresh_secs("60"), Some(60));
        assert_eq!(parse_refresh_secs("120"), Some(120));
        assert_eq!(parse_refresh_secs(" 30 "), Some(30));
    }

    #[test]
    fn parse_refresh_secs_rejects_unknown_values() {
        assert_eq!(parse_refresh_secs("15"), None);
        assert_eq!(parse_refresh_secs("-1"), None);
        assert_eq!(parse_refresh_secs("abc"), None);
        assert_eq!(parse_refresh_secs(""), None);
        assert_eq!(parse_refresh_secs("4294967296"), None); // u32 overflow
    }

    #[test]
    fn refresh_label_paused_for_zero() {
        assert_eq!(refresh_label(0), "Paused");
        assert_eq!(refresh_label(30), "30s");
        assert_eq!(refresh_label(120), "120s");
    }

    #[test]
    fn binary_version_reads_detail_from_health_array() {
        let h = serde_json::json!([
            {"name": "daemon_socket", "status": "ok", "detail": "/tmp/sock"},
            {"name": "binary_version", "status": "ok", "detail": "touring 30.0.0"}
        ]);
        assert_eq!(binary_version(&h), Some("touring 30.0.0".to_string()));
    }

    #[test]
    fn binary_version_none_on_missing_or_malformed() {
        assert_eq!(
            binary_version(&serde_json::json!([{"name": "other"}])),
            None
        );
        assert_eq!(binary_version(&serde_json::json!({"error": "boom"})), None);
        assert_eq!(binary_version(&Value::Null), None);
    }

    #[test]
    fn index_summary_formats_live_status_shape() {
        let v = serde_json::json!({"index": {"file_count": 3176, "symbol_count": 71436}});
        assert_eq!(index_summary(&v), "3176 files · 71436 symbols");
        assert_eq!(index_summary(&Value::Null), "index unavailable");
    }

    #[test]
    fn bandit_summary_formats_learning_state() {
        let v = serde_json::json!({"learning": {"bandit_type": "linucb", "arm_count": 8}});
        assert_eq!(bandit_summary(&v), "linucb · 8 arms");
        assert_eq!(
            bandit_summary(&serde_json::json!({})),
            "learning unavailable"
        );
    }
}
