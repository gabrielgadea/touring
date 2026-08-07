//! Tantivy Stream Actor — real-time event-sourced upsert pipeline.
//!
//! Provides a non-blocking `try_send_symbol` API that hooks (post_edit,
//! post_write) call to enqueue `SymbolDoc` updates without stalling the
//! hook request path. A background tokio task drains the channel, batching
//! upserts and committing every `COMMIT_BATCH` docs or `COMMIT_INTERVAL`
//! (whichever comes first).
//!
//! # Backpressure
//!
//! The channel is bounded at `CHANNEL_CAP`. When the actor is falling behind,
//! `try_send` returns an error and the caller falls back to the synchronous
//! direct-upsert path (recording a `tantivy_stream_backpressure_drop` metric).
//! This keeps hook latency bounded regardless of indexing throughput.
//!
//! # Lifecycle
//!
//! 1. `spawn_stream_actor()` — called once from `run_daemon_async()` within
//!    the tokio context. Safe to call multiple times (subsequent calls no-op).
//! 2. `try_send_symbol(doc)` — called from hook handlers (any thread context).
//!    Uses `Sender::try_send` which is sync and does not block.
//! 3. On daemon shutdown the channel sender is dropped (daemon exits), which
//!    closes the receiver and causes the actor loop to drain and exit cleanly.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use tokio::sync::mpsc;

use crate::shared::gate_metrics;
use crate::tantivy_index::SymbolDoc;

/// Bounded channel capacity. Under load the actor commits at ~250ms/batch
/// (5k docs), so 2048 entries = ~8s of burst headroom before dropping.
const CHANNEL_CAP: usize = 2_048;

/// Flush after this many docs regardless of timer.
const COMMIT_BATCH: usize = 5_000;

/// Flush on a recurring timer even when buffer < COMMIT_BATCH.
const COMMIT_INTERVAL: Duration = Duration::from_secs(2);

/// Sender do stream — `None` até `spawn_stream_actor` ser chamado.
///
/// O payload carrega a **raiz do projeto** junto do documento (F3, 03/08/2026).
/// Antes era só `SymbolDoc`: `post_edit`/`post_write` TÊM `runtime.project_root`
/// e o descartavam aqui, na fronteira do canal, de modo que o actor drenava
/// documentos de todos os projetos para um índice único. A raiz não estava
/// apenas "não passada adiante" — estava **apagada**, e era esse o mecanismo
/// que produzia a contaminação cross-project e a eviction silenciosa
/// (ver `identical_relative_coordinates_collapse_to_one_document`).
static STREAM_TX: OnceLock<mpsc::Sender<(PathBuf, SymbolDoc)>> = OnceLock::new();

/// Attempt to enqueue a [`SymbolDoc`] for async Tantivy indexing.
///
/// Returns `true` when the doc was accepted into the channel (fast path).
/// Returns `false` when backpressure drops it — the caller should fall back
/// to synchronous `upsert_symbol` to preserve index freshness.
///
/// # eBPF Backpressure
///
/// When the global circuit breaker is open (e.g. triggered by kernel-level memory
/// pressure from eBPF `MemoryPressureSignal`: page faults greater than 1k or L3 cache misses
/// greater than 10k), the function immediately drops the document and returns `false`. This
/// prevents the Tantivy indexer from amplifying memory pressure during a kernel-level
/// anomaly. The caller's synchronous fallback path also respects the circuit.
///
/// This function is synchronous and safe to call from any thread, including
/// non-tokio project actor threads.
pub fn try_send_symbol(project_root: PathBuf, doc: SymbolDoc) -> bool {
    // eBPF backpressure gate: shed indexing load when the circuit is open.
    if crate::circuit_breaker::is_open() {
        gate_metrics::record_tantivy_stream_backpressure_drop();
        return false;
    }
    let Some(tx) = STREAM_TX.get() else {
        return false;
    };
    match tx.try_send((project_root, doc)) {
        Ok(()) => {
            gate_metrics::record_tantivy_stream_enqueued();
            true
        }
        Err(_) => {
            gate_metrics::record_tantivy_stream_backpressure_drop();
            false
        }
    }
}

/// Returns `true` if the stream actor is running (sender is initialized).
pub fn is_active() -> bool {
    STREAM_TX.get().is_some()
}

/// Spawn the background actor that drains the stream channel.
///
/// Must be called from within a tokio runtime context (e.g. `run_daemon_async`).
/// Subsequent calls are silently ignored — the `OnceLock` ensures single init.
///
/// A guarda `global_tantivy().is_none()` foi **removida** em 03/08/2026: com
/// índices por projeto, a pergunta "o tantivy está disponível?" não tem mais uma
/// resposta única no boot — a disponibilidade é por raiz, e nenhuma raiz é
/// conhecida antes do primeiro documento chegar. O actor ocioso custa uma task e
/// um ticker de 2s; quando nenhum índice resolve, `flush_buffer` registra e
/// descarta. Manter a guarda faria o daemon decidir pelo índice LEGADO se vale
/// indexar os projetos — exatamente o acoplamento que esta fase desfaz.
pub fn spawn_stream_actor() {
    // Idempotent guard — OnceLock already set means actor is live.
    if STREAM_TX.get().is_some() {
        return;
    }
    let (tx, rx) = mpsc::channel(CHANNEL_CAP);
    // Race-safe: if two callers reach this concurrently, the loser's tx
    // is dropped, closing the channel — that's fine, winner's actor runs.
    if STREAM_TX.set(tx).is_err() {
        return;
    }
    tokio::spawn(run_actor(rx));
    tracing::info!(
        channel_cap = CHANNEL_CAP,
        commit_batch = COMMIT_BATCH,
        commit_interval_secs = COMMIT_INTERVAL.as_secs(),
        "tantivy stream actor spawned"
    );
}

/// Actor loop — drains the channel, batches, and commits to the Tantivy index.
async fn run_actor(mut rx: mpsc::Receiver<(PathBuf, SymbolDoc)>) {
    let mut buffer: Vec<(PathBuf, SymbolDoc)> = Vec::with_capacity(COMMIT_BATCH);
    let mut ticker = tokio::time::interval(COMMIT_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            biased;
            doc = rx.recv() => {
                match doc {
                    Some(d) => {
                        buffer.push(d);
                        if buffer.len() >= COMMIT_BATCH {
                            flush_buffer(&mut buffer);
                        }
                    }
                    None => {
                        // Channel closed (daemon shutting down) — drain remaining.
                        if !buffer.is_empty() {
                            flush_buffer(&mut buffer);
                        }
                        tracing::debug!("tantivy stream actor channel closed, exiting");
                        break;
                    }
                }
            }
            _ = ticker.tick() => {
                if !buffer.is_empty() {
                    flush_buffer(&mut buffer);
                }
            }
        }
    }
}

/// Descarrega o buffer nos índices dos respectivos projetos e commita.
///
/// Chamado da task do actor — nunca das threads de hook.
///
/// **Agrupa por raiz antes de escrever.** Isso não é otimização: `commit()` é
/// por índice, então um buffer heterogêneo escrito documento a documento faria
/// um commit (com fsync) por documento em vez de um por projeto. O agrupamento
/// também é o que torna a partição real — cada documento vai para o índice do
/// SEU projeto, e não para um índice compartilhado onde coordenadas relativas
/// iguais se sobrescrevem.
fn flush_buffer(buffer: &mut Vec<(PathBuf, SymbolDoc)>) {
    let mut by_root: HashMap<PathBuf, Vec<SymbolDoc>> = HashMap::new();
    for (root, doc) in buffer.drain(..) {
        by_root.entry(root).or_default().push(doc);
    }

    let mut flushed = 0u64;
    for (root, docs) in by_root {
        let Some(idx) = crate::tantivy_index::tantivy_for(Some(&root)) else {
            // Contabilizado, não só logado: um `tracing::debug!` sozinho fazia os
            // documentos sumirem da observabilidade (achado do cross-audit
            // 2026-08-03). Com o contador, `enqueued` fecha contra
            // `flush_docs + backpressure_drop + index_unavailable_drop`.
            gate_metrics::record_tantivy_stream_index_unavailable_drop(docs.len() as u64);
            tracing::warn!(
                root = %root.display(),
                docs = docs.len(),
                "tantivy stream: índice do projeto indisponível — lote descartado"
            );
            continue;
        };
        for doc in &docs {
            if let Err(e) = idx.upsert_symbol(doc) {
                tracing::debug!("tantivy stream: upsert error: {e}");
            }
        }
        // Um commit por índice — o custo do fsync é pago uma vez por projeto.
        if let Err(e) = idx.commit() {
            tracing::debug!(root = %root.display(), "tantivy stream: commit error: {e}");
            continue;
        }
        flushed += docs.len() as u64;
    }

    if flushed > 0 {
        gate_metrics::record_tantivy_stream_flush(flushed);
        tracing::debug!(docs = flushed, "tantivy stream: flushed batch");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::gate_metrics;

    #[test]
    fn try_send_returns_false_when_actor_not_started() {
        // Without spawn_stream_actor, STREAM_TX is None → returns false.
        // Note: this test is order-sensitive — if another test ran spawn first
        // this would be true. The actor is guarded by OnceLock so we just
        // verify the return is consistent with is_active().
        let doc = SymbolDoc {
            symbol_name: "test".to_string(),
            file_path: "test.rs".to_string(),
            symbol_kind: "fn".to_string(),
            module_path: None,
            docstring: None,
            line_number: 1,
            language: "rust".to_string(),
            visibility: None,
            crate_name: None,
            blake3_hash: None,
            import_count: None,
            export_count: None,
            cognitive_score: None,
            functional_signature: None,
            community_id: None,
        };
        let active = is_active();
        let sent = try_send_symbol(PathBuf::from("/tmp/projeto-de-teste"), doc);
        // If not active, must return false. If active (actor already running
        // from a parallel test), may return true — both are correct.
        if !active {
            assert!(
                !sent,
                "try_send must return false when actor is not running"
            );
        }
    }

    #[test]
    fn stream_metrics_record_functions_compile_and_run() {
        let before_enq = gate_metrics::global()
            .tantivy_stream_enqueued_count
            .load(std::sync::atomic::Ordering::Relaxed);
        gate_metrics::record_tantivy_stream_enqueued();
        let after_enq = gate_metrics::global()
            .tantivy_stream_enqueued_count
            .load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(after_enq, before_enq + 1);

        let before_drop = gate_metrics::global()
            .tantivy_stream_backpressure_drop_count
            .load(std::sync::atomic::Ordering::Relaxed);
        gate_metrics::record_tantivy_stream_backpressure_drop();
        let after_drop = gate_metrics::global()
            .tantivy_stream_backpressure_drop_count
            .load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(after_drop, before_drop + 1);

        let before_flush = gate_metrics::global()
            .tantivy_stream_flush_count
            .load(std::sync::atomic::Ordering::Relaxed);
        let before_docs = gate_metrics::global()
            .tantivy_stream_flush_docs_count
            .load(std::sync::atomic::Ordering::Relaxed);
        gate_metrics::record_tantivy_stream_flush(42);
        let after_flush = gate_metrics::global()
            .tantivy_stream_flush_count
            .load(std::sync::atomic::Ordering::Relaxed);
        let after_docs = gate_metrics::global()
            .tantivy_stream_flush_docs_count
            .load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(after_flush, before_flush + 1);
        assert_eq!(after_docs, before_docs + 42);
    }

    /// A contabilidade do stream FECHA: cada doc enfileirado termina em
    /// exatamente um de três destinos — flushed, backpressure-drop ou
    /// index-unavailable-drop. Antes deste contador o terceiro destino era
    /// invisível e a soma não fechava (cross-audit 03/08/2026).
    #[test]
    fn the_index_unavailable_drop_is_counted_and_exposed() {
        let before = gate_metrics::global()
            .tantivy_stream_index_unavailable_drop_count
            .load(std::sync::atomic::Ordering::Relaxed);
        gate_metrics::record_tantivy_stream_index_unavailable_drop(7);
        let after = gate_metrics::global()
            .tantivy_stream_index_unavailable_drop_count
            .load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(after, before + 7, "o contador soma o LOTE, não uma ocorrência");

        let json = serde_json::to_string(&gate_metrics::GateMetricsSnapshot::capture())
            .expect("snapshot serializável");
        assert!(
            json.contains("tantivy_stream_index_unavailable_drop_count"),
            "o descarte tem de ser observável em `touring gate-metrics -j`, \
             senão o documento some sem rastro"
        );
    }

    #[test]
    fn gate_metrics_snapshot_includes_stream_fields() {
        let snap = gate_metrics::GateMetricsSnapshot::capture();
        // Fields must serialize without error (they have #[serde(default)]).
        let json = serde_json::to_string(&snap).expect("snapshot serializes");
        assert!(json.contains("tantivy_stream_enqueued_count"));
        assert!(json.contains("tantivy_stream_backpressure_drop_count"));
        assert!(json.contains("tantivy_stream_flush_count"));
        assert!(json.contains("tantivy_stream_flush_docs_count"));
    }
}

/// O flush por raiz (F3, 03/08/2026).
///
/// Antes o canal carregava só `SymbolDoc` e o actor drenava tudo para um índice
/// único — era esse o mecanismo que misturava projetos e fazia coordenadas
/// relativas iguais se sobrescreverem. Aqui provamos o contrário: dois projetos
/// com o MESMO caminho relativo terminam em índices distintos, cada um com o seu.
#[cfg(all(test, feature = "tantivy-fts"))]
mod per_project_flush_tests {
    use super::*;
    use crate::tantivy_index::SymbolDoc;

    fn doc(name: &str, crate_name: &str) -> SymbolDoc {
        SymbolDoc {
            symbol_name: name.to_string(),
            // MESMO caminho relativo nos dois projetos — a coordenada que colide.
            file_path: "src/lib.rs".to_string(),
            symbol_kind: "fn".to_string(),
            module_path: None,
            docstring: None,
            line_number: 1,
            language: "rust".to_string(),
            visibility: None,
            crate_name: Some(crate_name.to_string()),
            blake3_hash: None,
            import_count: None,
            export_count: None,
            cognitive_score: None,
            functional_signature: None,
            community_id: None,
        }
    }

    #[test]
    fn flush_routes_each_document_to_its_own_project_index() {
        let a = tempfile::TempDir::new().expect("tempdir A");
        let b = tempfile::TempDir::new().expect("tempdir B");
        // Marcador real: sem ele `normalize_project_root` sobe até $HOME e as
        // duas raízes de teste colapsariam no MESMO diretório — o teste passaria
        // por acidente sem provar nada.
        std::fs::create_dir_all(a.path().join(".git")).expect("marcador A");
        std::fs::create_dir_all(b.path().join(".git")).expect("marcador B");

        let mut buffer = vec![
            (a.path().to_path_buf(), doc("compartilhado", "projeto-a")),
            (b.path().to_path_buf(), doc("compartilhado", "projeto-b")),
        ];
        flush_buffer(&mut buffer);
        assert!(buffer.is_empty(), "o flush drena o buffer");

        let idx_a = crate::tantivy_index::tantivy_for(Some(a.path())).expect("índice A");
        let idx_b = crate::tantivy_index::tantivy_for(Some(b.path())).expect("índice B");
        let hits_a = idx_a.search("compartilhado", 10).expect("busca A");
        let hits_b = idx_b.search("compartilhado", 10).expect("busca B");

        assert_eq!(hits_a.len(), 1, "A recebeu exatamente o seu documento");
        assert_eq!(hits_b.len(), 1, "B recebeu exatamente o seu documento");
        assert_eq!(hits_a[0].crate_name.as_deref(), Some("projeto-a"));
        assert_eq!(
            hits_b[0].crate_name.as_deref(),
            Some("projeto-b"),
            "nenhum projeto sobrescreveu o outro, apesar do caminho relativo idêntico"
        );
    }
}
