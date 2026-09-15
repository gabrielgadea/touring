//! Signal-layer **vocabulary** — the [`SignalContext`] / [`SignalLayer`] /
//! [`LayerMetrics`] contract shared by every signal producer.
//!
//! Session B F4-pre of the `touring-ceg` extraction (2026-06-10): these types
//! were extracted from `touring-hooks::shared::signal_pipeline` (which keeps
//! the heavy `SignalPipeline` engine and its `touring-analysis` dependencies
//! in the parent) so leaf-side producers like [`crate::ast_grep_signal`] can
//! implement [`SignalLayer`] without reaching back into the parent crate.
//! `signal_pipeline` re-exports them, so every historical
//! `crate::shared::signal_pipeline::{SignalContext, SignalLayer}` path still
//! resolves unchanged.

/// The change a pre-write / pre-edit hook is about to apply (S0, 2026-09-01).
///
/// Until v2 every layer saw `source == ""` at the three live call sites and
/// had no field for the mutation itself — so no layer could analyse the code
/// that WOULD be written, only what was already on disk. This is the payload
/// the TIER-1 layers (ast-grep source risk, secrets entropy, missing imports,
/// antipatterns, Python syntax) read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposedChange<'a> {
    /// `Write`: the whole content the file will have.
    Write {
        /// Full proposed file content.
        content: &'a str,
    },
    /// `Edit`: the exact replacement Claude Code will apply.
    Edit {
        /// Text being replaced.
        old_string: &'a str,
        /// Text that replaces it.
        new_string: &'a str,
    },
}

impl<'a> ProposedChange<'a> {
    /// The text that will exist after the change (the content for a write,
    /// the replacement for an edit) — what a pre-write analyser must look at.
    pub fn new_text(&self) -> &'a str {
        match self {
            ProposedChange::Write { content } => content,
            ProposedChange::Edit { new_string, .. } => new_string,
        }
    }
}

/// Context passed to each signal layer for enrichment.
pub struct SignalContext<'a> {
    /// Relative file path being processed.
    pub file_path: &'a str,
    /// Source content (if available — empty for bash hooks).
    pub source: &'a str,
    /// CILA complexity level (0-6).
    pub cila_level: usize,
    /// Hook name (pre_read, pre_edit, pre_write, pre_bash).
    pub hook_name: &'a str,
    /// Opaque extension data — layers can downcast if needed.
    pub extensions: &'a dyn std::any::Any,
    /// Claude Code tool that triggered the hook (`Write`, `Edit`, `Read`, …);
    /// empty when unknown (S0 v2).
    pub tool_name: &'a str,
    /// The mutation about to happen, when the hook precedes one (S0 v2).
    pub proposed: Option<ProposedChange<'a>>,
}

impl<'a> SignalContext<'a> {
    /// Create a minimal context for testing.
    pub fn new(file_path: &'a str, source: &'a str) -> Self {
        Self {
            file_path,
            source,
            cila_level: 3,
            hook_name: "test",
            extensions: &(),
            tool_name: "",
            proposed: None,
        }
    }

    /// Create context with specific CILA level.
    pub fn with_cila(mut self, level: usize) -> Self {
        self.cila_level = level;
        self
    }

    /// Create context with specific hook name.
    pub fn with_hook(mut self, name: &'a str) -> Self {
        self.hook_name = name;
        self
    }

    /// Record the Claude Code tool behind the hook (S0 v2).
    pub fn with_tool_name(mut self, tool_name: &'a str) -> Self {
        self.tool_name = tool_name;
        self
    }

    /// Attach the proposed mutation (S0 v2).
    pub fn with_proposed(mut self, change: ProposedChange<'a>) -> Self {
        self.proposed = Some(change);
        self
    }

    /// The text a content layer should analyse: the proposed new text when a
    /// non-empty proposal is attached, otherwise the on-disk `source`.
    pub fn analysable_text(&self) -> &'a str {
        match self.proposed {
            Some(change) if !change.new_text().is_empty() => change.new_text(),
            _ => self.source,
        }
    }
}

/// Metadata about a layer's execution.
#[derive(Debug, Clone)]
pub struct LayerMetrics {
    /// Layer name (for observability).
    pub name: &'static str,
    /// Number of signals produced.
    pub signal_count: usize,
    /// Execution time in microseconds.
    pub duration_us: u64,
}

/// A single signal enrichment layer.
///
/// Implement this trait for each type of signal (dependents, gotchas,
/// blast radius, etc.). The pipeline calls `enrich()` on each layer
/// and collects the scored signals.
pub trait SignalLayer: Send + Sync {
    /// Unique name for this layer (used in metrics/logging).
    fn name(&self) -> &'static str;

    /// Produce scored signals for the given context.
    ///
    /// Returns empty vec if this layer has nothing relevant to contribute.
    fn enrich(&self, ctx: &SignalContext<'_>) -> Vec<(f32, String)>;

    /// Whether this layer should run for the given CILA level.
    ///
    /// Default: always run. Override to skip expensive layers at low CILA.
    fn should_run(&self, _cila_level: usize) -> bool {
        true
    }

    /// Roda a camada e devolve o [`LayerMetrics`] dela.
    ///
    /// Cross-audit 04/09/2026 — a medicao existia a mao em DUAS das doze camadas
    /// (`drift.rs` e `scan.rs`), como funcoes livres sem nenhum chamador, e uma
    /// terceira copia vivia no laco do pipeline. Como metodo default do trait a
    /// capacidade passa a valer para TODAS as camadas — a expansao que a REGRA #0
    /// pede, em vez de arranjar um chamador de fachada para duas orfas.
    fn metrics(&self, ctx: &SignalContext<'_>) -> LayerMetrics {
        let start = std::time::Instant::now();
        let signals = self.enrich(ctx);
        LayerMetrics {
            name: self.name(),
            signal_count: signals.len(),
            duration_us: start.elapsed().as_micros() as u64,
        }
    }
}

/// S0 (2026-09-01) — `SignalContext` v2: the layers must be able to analyse
/// the code that WILL be written, not only what is on disk.
#[cfg(test)]
mod signal_context_v2_tests {
    use super::{ProposedChange, SignalContext};

    #[test]
    fn new_keeps_v1_defaults_and_adds_empty_v2_fields() {
        let ctx = SignalContext::new("src/lib.rs", "fn on_disk() {}");
        assert_eq!(ctx.tool_name, "");
        assert!(ctx.proposed.is_none());
        assert_eq!(
            ctx.analysable_text(),
            "fn on_disk() {}",
            "no proposal ⇒ source"
        );
    }

    #[test]
    fn write_proposal_is_what_layers_analyse() {
        let ctx = SignalContext::new("src/new.rs", "")
            .with_tool_name("Write")
            .with_proposed(ProposedChange::Write {
                content: "fn fresh() {}",
            });
        assert_eq!(ctx.tool_name, "Write");
        assert_eq!(ctx.analysable_text(), "fn fresh() {}");
        assert_eq!(ctx.proposed.map(|p| p.new_text()), Some("fn fresh() {}"));
    }

    #[test]
    fn edit_proposal_exposes_old_and_new_and_analyses_the_new_text() {
        let ctx = SignalContext::new("src/lib.rs", "fn a() {}")
            .with_tool_name("Edit")
            .with_proposed(ProposedChange::Edit {
                old_string: "fn a() {}",
                new_string: "fn a() { todo!() }",
            });
        match ctx.proposed {
            Some(ProposedChange::Edit {
                old_string,
                new_string,
            }) => {
                assert_eq!(old_string, "fn a() {}");
                assert_eq!(new_string, "fn a() { todo!() }");
            }
            other => panic!("expected an Edit proposal, got {other:?}"),
        }
        assert_eq!(ctx.analysable_text(), "fn a() { todo!() }");
    }

    #[test]
    fn empty_proposal_falls_back_to_source_never_to_nothing() {
        let ctx = SignalContext::new("x.py", "print(1)")
            .with_proposed(ProposedChange::Write { content: "" });
        assert_eq!(ctx.analysable_text(), "print(1)");
    }
}
