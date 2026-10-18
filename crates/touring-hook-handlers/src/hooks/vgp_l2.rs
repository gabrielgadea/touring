//! Layer-2 (pooled pyright) glue for the VGP shadow hooks — builds the [`SombraExtra`] that the
//! shared `sombra_check` merges into the shadow line (`camadas: [1, 2]`). It lives here and not in
//! touring-code so the dependency graph stays acyclic: touring-hook-handlers → touring-pyright
//! (a crate extracted from touring-lsp with zero touring-* deps — the lsp-bridge's optional
//! touring-hooks edge is what made this wiring impossible before the split).
//!
//! The pool is process-gated: the daemon calls `habilitar_neste_processo()` at startup, so in the
//! daemon this runs for real; in the ephemeral STANDALONE hook `checar` returns `unknown` with a
//! reason and spawns nothing (fail-open — the shadow line carries `camada2` with the motivo).

use touring_code::vgp::sombra::SombraExtra;
use touring_code::vgp::{CheckInput, Finding};
use touring_pyright::pyright::{Severidade, Verdict};

/// Runs the pooled pyright check over the same input layer 1 judged, mapped into vgp terms.
/// `None` when the input is not Python — the shadow line then keeps `camadas: [1]`.
pub(crate) fn extra(input: &CheckInput) -> Option<SombraExtra> {
    if input.lang != "python" {
        return None;
    }
    let overlays: Vec<(String, String)> = input
        .overlays
        .iter()
        .map(|(origem, texto)| (origem.clone(), texto.clone()))
        .collect();
    let l2 = touring_pyright::pool::checar(
        &input.project_root,
        &input.path,
        &input.content,
        input.disk.as_deref(),
        &overlays,
    );
    Some(SombraExtra {
        findings: l2
            .achados
            .iter()
            .map(|a| {
                Finding::new(
                    &a.classe,
                    &a.nome,
                    a.linha as usize,
                    match a.severidade {
                        Severidade::Deny => "deny",
                        Severidade::Warn => "warn",
                    },
                    a.mensagem.clone(),
                    Vec::new(),
                )
            })
            .collect(),
        elapsed_ms: l2.elapsed_ms as f64,
        camada2_motivo: (l2.verdict == Verdict::Unknown).then(|| {
            l2.motivo
                .clone()
                .unwrap_or_else(|| "sem motivo declarado".to_string())
        }),
    })
}
