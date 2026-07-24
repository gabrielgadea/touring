//! Theme toggle button component.

use crate::web::theme::Theme;
use leptos::prelude::*;

/// A small button that toggles between dark and light themes.
#[component]
pub fn ThemeToggle() -> impl IntoView {
    let theme = expect_context::<RwSignal<Theme>>();

    view! {
        <button
            class="theme-toggle"
            title=move || format!("Switch to {} mode", match theme.get() {
                Theme::Dark => "light",
                Theme::Light => "dark",
            })
            on:click=move |_| {
                theme.set(match theme.get_untracked() {
                    Theme::Dark => Theme::Light,
                    Theme::Light => Theme::Dark,
                });
            }
        >
            <span class="theme-toggle-icon">
                {move || match theme.get() {
                    Theme::Dark => "☀",
                    Theme::Light => "◐",
                }}
            </span>
        </button>
    }
}
