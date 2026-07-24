//! PipelineStages — VGP pipeline visual (SPEC 2026-06-12 §4.3).
//!
//! Connected stage dots (10px done/pending, 14px active with glow)
//! with labels and optional counts. Used by /dashboard, /plans rail
//! and /speculate. Reuses the `.el-stage-dot` token styling.

use leptos::prelude::*;

/// Stage progression state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageState {
    /// Completed (filled accent).
    Done,
    /// Currently executing (14px + glow).
    Active,
    /// Not yet reached (outline).
    Pending,
    /// Failed / blocked (warn fill).
    Warn,
}

impl StageState {
    /// CSS modifier class for `.el-stage-dot`.
    pub fn class(self) -> &'static str {
        match self {
            StageState::Done => "el-stage-dot done",
            StageState::Active => "el-stage-dot active",
            StageState::Pending => "el-stage-dot",
            StageState::Warn => "el-stage-dot warn",
        }
    }
}

/// One pipeline stage.
#[derive(Debug, Clone, PartialEq)]
pub struct Stage {
    /// Short label under the dot (e.g. "VGP", "RENDER", "TDG").
    pub label: &'static str,
    /// Optional count/annotation rendered mono next to the label.
    pub count: Option<String>,
    /// Visual state.
    pub state: StageState,
}

/// Horizontal connected pipeline of stages.
#[component]
pub fn PipelineStages(
    /// Reactive stage list, left to right.
    #[prop(into)]
    stages: Signal<Vec<Stage>>,
) -> impl IntoView {
    view! {
        <div class="el-pipeline">
            {move || {
                let list = stages.get();
                let last = list.len().saturating_sub(1);
                list.into_iter()
                    .enumerate()
                    .map(|(i, s)| view! {
                        <div class="el-pipeline-stage">
                            <span class=s.state.class() aria-hidden="true"></span>
                            <span class="el-pipeline-label">{s.label}</span>
                            {s.count.map(|c| view! {
                                <span class="el-mono el-pipeline-count">{c}</span>
                            })}
                        </div>
                        {(i < last).then(|| view! {
                            <span class="el-pipeline-connector" aria-hidden="true"></span>
                        })}
                    })
                    .collect_view()
            }}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_state_classes_are_distinct() {
        let all = [
            StageState::Done,
            StageState::Active,
            StageState::Pending,
            StageState::Warn,
        ];
        let classes: std::collections::HashSet<_> = all.iter().map(|s| s.class()).collect();
        assert_eq!(classes.len(), 4);
        for s in all {
            assert!(s.class().starts_with("el-stage-dot"));
        }
    }
}
