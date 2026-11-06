//! The session's recent history: the ledgers the gates read and write, and the keys that
//! scope them (project root, actor, class, file).

use moka::sync::Cache;
use serde_json::Value;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use super::spec::*;

/// A sessão entra na chave (30/08/2026): os ledgers vivem no daemon, um
/// processo só para N sessões CC — chaveado por (projeto, tag), um deny nesta
/// sessão carregava comandos de OUTRA (medido pela peer `analise-a2`: paths de
/// scratchpad alheios no remédio) e inflava a contagem de quem não fez a
/// rajada. O custo: a rajada distribuída entre 2 sessões deixa de somar — o
/// desenho certo, cada sessão responde pelo próprio loop.
pub(super) fn write_run_key(project_root: &Path, session: &str, tag: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    project_root.hash(&mut h);
    session.hash(&mut h);
    tag.hash(&mut h);
    h.finish()
}

pub(super) type ExecBurstEntry = (u32, Vec<String>);

/// Sessão na chave pela mesma razão de [`write_run_key`]: o ledger é do
/// daemon, e sem ela a janela vazava entre sessões CC do mesmo projeto.
pub(super) fn inspect_burst_key(project_root: &Path, session: &str, class: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "\u{2}s3-inspect\u{2}".hash(&mut hasher);
    project_root.hash(&mut hasher);
    session.hash(&mut hasher);
    class.hash(&mut hasher);
    hasher.finish()
}

/// Sessão na chave pela mesma razão de [`write_run_key`].
pub(super) fn exec_burst_key(project_root: &Path, session: &str, class: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "\u{2}g10-exec\u{2}".hash(&mut hasher);
    project_root.hash(&mut hasher);
    session.hash(&mut hasher);
    class.hash(&mut hasher);
    hasher.finish()
}

/// Contas do orçamento de bypass de um escopo, dentro da janela.
///
/// São DUAS contas porque as duas regras são distintas, e uma só as confundia:
/// medido na estreia, um bypass separado por um comando neutro era negado como
/// "dois seguidos", que não era verdade.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct BypassBudget {
    /// Bypasses colados um no outro; qualquer outro comando zera.
    pub(crate) strict_run: u32,
    /// Bypasses desde o último `touring run`; só a rota sancionada zera.
    pub(crate) since_route: u32,
}

/// Chave do ledger: o escopo, como nos demais gates deste módulo.
pub(super) fn bypass_key(project_root: &Path, actor: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "\u{2}code-mode-bypass\u{2}".hash(&mut hasher);
    project_root.hash(&mut hasher);
    actor.hash(&mut hasher);
    hasher.finish()
}

/// W1 S-1.2 — ledger de repetição exata: input-hash → (repetições, época de
/// mutação do projeto na primeira vista). TTL espelha o anti-spam (300s).
/// G6 ledger key: the session AND the input. A blind retry is the same session repeating
/// what it already holds in context; another session running the same command has never
/// seen the result. Keyed by input alone, fresh sessions were denied as "retries" of each
/// other (26/09/2026 — the contract suite, one session per call, within 300 s).
pub(super) fn g6_key(
    project_root: &Path,
    session: &str,
    tool_name: &str,
    tool_input: &Value,
) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    project_root.hash(&mut hasher);
    session.hash(&mut hasher);
    tool_name.hash(&mut hasher);
    tool_input.to_string().hash(&mut hasher);
    hasher.finish()
}

/// W2 — ledger da rajada por (projeto, classe): contagem + os comandos REAIS
/// acumulados (cap 8) — eles viram o corpo do programa no remédio do deny.
/// TTL = a janela do contador legado (o mesmo sinal, agora com memória).
/// Entrada da rajada: `(contagem, comandos, época de mutação da última inserção)`.
///
/// A época entrou junto com a calibração por similaridade: sem ela, uma
/// releitura DEPOIS de um Edit era negada como repetição — e reler o que
/// acabou de mudar é legítimo, não redundância. É a mesma proteção que o G6 já
/// tinha (`mutation_epoch`), agora estendida ao gatilho novo. O teste
/// `g6_mutacao_no_meio_reseta_a_repeticao` pegou a regressão.
pub(super) type BurstEntry = (u32, Vec<String>, u64);

pub(super) fn burst_key(project_root: &Path, actor: &str, class: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "\u{2}g1-burst\u{2}".hash(&mut hasher);
    project_root.hash(&mut hasher);
    actor.hash(&mut hasher);
    class.hash(&mut hasher);
    hasher.finish()
}

pub(super) fn g3_read_key(project_root: &Path, session: &str, file: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "\u{2}g3-read\u{2}".hash(&mut hasher);
    project_root.hash(&mut hasher);
    session.hash(&mut hasher);
    file.hash(&mut hasher);
    hasher.finish()
}

pub(super) fn g7_key(project_root: &Path, actor: &str, file: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "\u{2}g7-target\u{2}".hash(&mut hasher);
    project_root.hash(&mut hasher);
    actor.hash(&mut hasher);
    file.hash(&mut hasher);
    hasher.finish()
}

/// 30.4.97 (G7G3G9) — (mtime, tamanho) de um arquivo: o que o harness do Claude Code compara no
/// «File has been modified since read, either by the user or by a linter. Read it again before
/// attempting to write it.»
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct FileStamp {
    pub(super) mtime_ns: u128,
    pub(super) len: u64,
}

/// Every ledger the gates remember a session by. ONE instance serves the daemon for its whole
/// life (`GateState::global`); a test builds its own (`GateState::new`) and so shares nothing
/// with its neighbours — the isolation the old process-global statics had to fake with unique
/// project roots and `#[serial]` markers.
///
/// Each field is the former `fn name() -> &'static Cache<…>` accessor, with the same capacity
/// and the same window; the windows are named in [`super::spec`].
pub(crate) struct GateState {
    /// G6 — byte-identical repeats: (repeats, mutation epoch at first sight).
    pub(super) g6_seen: Cache<u64, (u32, u64)>,
    /// Per-project mutation epoch (every Edit/Write advances it).
    pub(super) mutation_epoch: Cache<String, u64>,
    /// G9 deny awaiting its follow-through (the next Edit/Write of the session).
    pub(super) pending_g9: Cache<String, ()>,
    /// G2 deny awaiting `pipefail` on the next Bash.
    pub(super) pending_g2: Cache<String, ()>,
    /// G1 burst per (project, actor, class).
    pub(super) burst_ledger: Cache<u64, BurstEntry>,
    /// G1 deny awaiting its continuation check (carries the denied class).
    pub(super) pending_g1: Cache<String, String>,
    /// G3 — Edits without a recent Read, per session.
    pub(super) g3_streak: Cache<String, u32>,
    /// G3 — (project, session, file) read recently.
    pub(super) g3_read_files: Cache<u64, ()>,
    /// G7 — re-reads of the same file.
    pub(super) g7_seen: Cache<u64, u32>,
    /// G7 — the (mtime, len) the last read saw.
    pub(super) g7_stamps: Cache<u64, FileStamp>,
    /// M3 — distinct files read per session.
    pub(super) m3_distinct_reads: Cache<String, u32>,
    /// G4 — the session located something (Grep/Glob) recently.
    pub(super) g4_last_locate: Cache<String, ()>,
    /// G5 — the streak of Edit/Write of the session.
    pub(super) g5_edit_streak: Cache<String, u32>,
    /// S5 — scripts written in the window, per (project, session, path).
    pub(super) written_scripts_ledger: Cache<u64, ()>,
    /// S5 — the write→run pairs counted (with the scripts they named).
    pub(super) write_run_pair_ledger: Cache<u64, (u32, Vec<String>)>,
    /// G10 — serial calls of one executor class.
    pub(super) exec_burst_ledger: Cache<u64, ExecBurstEntry>,
    /// S3 — inspections of one class in the window.
    pub(super) inspect_burst_ledger: Cache<u64, ExecBurstEntry>,
    /// G10 deny awaiting its follow-through.
    pub(super) pending_g10: Cache<String, ()>,
    /// G11 — the bypass budget.
    pub(super) bypass_ledger: Cache<u64, BypassBudget>,
}

fn ttl(secs: u64) -> Duration {
    Duration::from_secs(secs)
}

impl GateState {
    /// A fresh, empty store.
    pub(crate) fn new() -> Self {
        Self {
            g6_seen: Cache::builder()
                .max_capacity(CACHE_MAX_CAPACITY)
                .time_to_live(ttl(SUGGESTION_TTL_SECS))
                .build(),
            mutation_epoch: Cache::builder()
                .max_capacity(CONFORMAL_TAU_MAX_PROJECTS)
                .build(),
            pending_g9: Cache::builder()
                .max_capacity(SESSION_LEDGER_CAPACITY)
                .time_to_live(ttl(PENDING_DENY_TTL_SECS))
                .build(),
            pending_g2: Cache::builder()
                .max_capacity(SESSION_LEDGER_CAPACITY)
                .time_to_live(ttl(PENDING_DENY_TTL_SECS))
                .build(),
            burst_ledger: Cache::builder()
                .max_capacity(CACHE_MAX_CAPACITY)
                .time_to_live(ttl(CODE_MODE_WINDOW_SECS))
                .build(),
            pending_g1: Cache::builder()
                .max_capacity(SESSION_LEDGER_CAPACITY)
                .time_to_live(ttl(PENDING_DENY_TTL_SECS))
                .build(),
            g3_streak: Cache::builder()
                .max_capacity(SESSION_LEDGER_CAPACITY)
                .time_to_live(ttl(SESSION_STATE_TTL_SECS))
                .build(),
            g3_read_files: Cache::builder()
                .max_capacity(CACHE_MAX_CAPACITY)
                .time_to_live(ttl(SESSION_STATE_TTL_SECS))
                .build(),
            g7_seen: Cache::builder()
                .max_capacity(CACHE_MAX_CAPACITY)
                .time_to_live(ttl(SESSION_STATE_TTL_SECS))
                .build(),
            g7_stamps: Cache::builder()
                .max_capacity(CACHE_MAX_CAPACITY)
                .time_to_live(ttl(SESSION_STATE_TTL_SECS))
                .build(),
            m3_distinct_reads: Cache::builder()
                .max_capacity(SESSION_LEDGER_CAPACITY)
                .time_to_live(ttl(SESSION_STATE_TTL_SECS))
                .build(),
            g4_last_locate: Cache::builder()
                .max_capacity(SESSION_LEDGER_CAPACITY)
                .time_to_live(ttl(G4_LOCATE_TTL_SECS))
                .build(),
            g5_edit_streak: Cache::builder()
                .max_capacity(SESSION_LEDGER_CAPACITY)
                .time_to_live(ttl(SESSION_STATE_TTL_SECS))
                .build(),
            written_scripts_ledger: Cache::builder()
                .max_capacity(CACHE_MAX_CAPACITY)
                .time_to_live(ttl(WRITE_RUN_WINDOW_SECS))
                .build(),
            write_run_pair_ledger: Cache::builder()
                .max_capacity(CACHE_MAX_CAPACITY)
                .time_to_live(ttl(WRITE_RUN_WINDOW_SECS))
                .build(),
            exec_burst_ledger: Cache::builder()
                .max_capacity(CACHE_MAX_CAPACITY)
                .time_to_live(ttl(EXEC_BURST_WINDOW_SECS))
                .build(),
            inspect_burst_ledger: Cache::builder()
                .max_capacity(CACHE_MAX_CAPACITY)
                .time_to_live(ttl(INSPECT_BURST_WINDOW_SECS))
                .build(),
            pending_g10: Cache::builder()
                .max_capacity(SESSION_LEDGER_CAPACITY)
                .time_to_live(ttl(PENDING_DENY_TTL_SECS))
                .build(),
            bypass_ledger: Cache::builder()
                .max_capacity(BYPASS_LEDGER_CAPACITY)
                .time_to_live(ttl(G11_WINDOW_SECS))
                .build(),
        }
    }

    /// The daemon's store: one per process, created on first use.
    pub(crate) fn global() -> &'static Self {
        static GLOBAL: OnceLock<GateState> = OnceLock::new();
        GLOBAL.get_or_init(Self::new)
    }
}

impl Default for GateState {
    fn default() -> Self {
        Self::new()
    }
}
