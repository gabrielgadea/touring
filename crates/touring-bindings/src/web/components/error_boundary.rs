//! Error boundary component for UI error handling.
//!
//! Wraps route content and displays user-friendly error messages.
//! Includes a retry button to re-trigger the failed operation.

use leptos::prelude::*;
use thiserror::Error;

/// Errors that can occur in the web UI.
#[derive(Debug, Error, Clone)]
pub enum UiError {
    /// Request to the touring daemon failed.
    #[error("Request failed: {0}")]
    Request(String),

    /// The daemon is not responding.
    #[error("Daemon unavailable")]
    DaemonUnavailable,

    /// Failed to parse the server response.
    #[error("Invalid response: {0}")]
    ParseError(String),
}

/// Error banner component that displays a `UiError` with a retry action.
/// The retry callback should clear the error signal and re-trigger the operation.
#[component]
pub fn ErrorBoundary(
    /// Signal holding the current error, if any.
    error: RwSignal<Option<UiError>>,
    /// Callback to retry the failed operation — clears the error and re-invokes.
    on_retry: impl Fn() + 'static,
) -> impl IntoView {
    view! {
        <>
            {error.get().map(|e| {
                let msg = e.to_string();
                view! {
                    <div class="error-banner">
                        <span>{msg}</span>
                        <button on:click=move |_| {
                            error.set(None);
                            on_retry();
                        }>
                            "Retry"
                        </button>
                    </div>
                }
            })}
        </>
    }
}
