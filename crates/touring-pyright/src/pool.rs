//! A long-lived registry of warm layer-2 sessions — one per `(project root, verifier)` — for the hook
//! daemon.
//!
//! The daemon is one long process and the hook is short: a [`crate::pyright::PyrightSession`] costs
//! ~430 ms to start and first analyse, then ~60 ms per check; a rust-analyzer workspace load or a
//! tsserver program build costs seconds. Keeping the session between edits is what makes layer 2 fit
//! the V3 budget; the price is that a forgotten session is a leaked process. So the pool is built
//! around these rules, the same for every [`Servidor`]:
//!
//! * **lazy** — nothing is spawned until a root is first checked; a root whose verifier cannot be
//!   found or started answers `unknown` and is not retried for 5 minutes (no spawn per edit);
//! * **never on the hook's clock when slow** — a verifier that needs seconds to get ready
//!   ([`Arranque::SegundoPlano`]: rust-analyzer, tsserver, deno) warms up on a detached thread, primed
//!   with the first edit; meanwhile the hook gets `unknown: warming up` and nobody blocks;
//! * **bounded** — at most `TOURING_VGP_L2_MAX` (default 2) sessions of each verifier live (rust-analyzer:
//!   `TOURING_VGP_L2_MAX_RA`, default 1 — a loaded workspace is heavy), least recently used first out;
//!   a session idle longer than `TOURING_VGP_L2_IDLE_S` (default 900) is closed; a warm-up that has
//!   not produced a usable session after `TOURING_VGP_L2_WARMUP_S` (default 180) is given up;
//! * **failing closed on state, open on the hook** — a check that cannot be measured (the server
//!   died, the deadline passed) removes the session so the next use recreates it, and the hook gets
//!   an `unknown` with the reason, never an error, a panic or a hang. An `unknown` that is about the
//!   *file* (rust-analyzer: not part of any crate) keeps the warm session;
//! * **switchable** — `TOURING_VGP_L2_DISABLED=1` answers `unknown` without spawning anything, and
//!   `TOURING_VGP_L2_OFF=rust-analyzer,deno` turns single verifiers off;
//! * **opt-in per process** — [`checar`] and its siblings answer `unknown` until the process calls
//!   [`habilitar_neste_processo`]. Only the long-lived daemon (and the CLI that asks for
//!   `--camadas 1,2`) does; the ephemeral standalone `touring-hook` fallback would otherwise start a
//!   language server per call and could leave it orphaned.
//!
//! ## Concurrency
//!
//! The registry lock is held only to look a slot up or to count sessions; each slot has its own
//! mutex, held for the whole check (the exchange with a server is sequential per session), so a slow
//! slot never blocks another — in particular a rust-analyzer warm-up never blocks a pyright check of the
//! same project. Lock order is always *slot, then registry*, and the registry side only ever
//! `try_lock`s other slots, so it cannot deadlock; a slot that cannot be locked is *in use*, which
//! means "not idle" and "not evictable". A poisoned lock is recovered (`into_inner`), never propagated.
//! A warming slot holds no lock while it works (it would stall every hook): it is marked, counted as
//! live, never evicted, and the warm-up thread takes the lock only to install the result.
//!
//! Closing a session (`Drop`: polite shutdown, bounded wait, kill — up to ~0.8 s) happens on a
//! detached thread, so it is never on the hook's clock.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, TryLockError};
use std::thread;
use std::time::{Duration, Instant};

use crate::achado::L2Resultado;
use crate::servidor::{Alvo, Ambiente, Arranque, Edicao, Servidor, Sessao};

const ENV_IDLE: &str = "TOURING_VGP_L2_IDLE_S";
const ENV_MAX: &str = "TOURING_VGP_L2_MAX";
const ENV_MAX_RA: &str = "TOURING_VGP_L2_MAX_RA";
const ENV_DISABLED: &str = "TOURING_VGP_L2_DISABLED";
const ENV_OFF: &str = "TOURING_VGP_L2_OFF";
const ENV_WARMUP: &str = "TOURING_VGP_L2_WARMUP_S";
const DEFAULT_IDLE_S: u64 = 900;
const DEFAULT_MAX: usize = 2;
const DEFAULT_MAX_RA: usize = 1;
const DEFAULT_WARMUP_S: u64 = 180;
/// How long a root whose server could not be started is left alone.
const DEFAULT_BACKOFF: Duration = Duration::from_secs(300);

/// Tunables, read from the environment on every call so a daemon restart is the only ceremony.
struct Config {
    idle: Duration,
    max: usize,
    max_ra: usize,
    disabled: bool,
    off: Vec<Servidor>,
    backoff: Duration,
    aquecimento: Duration,
    l2_budget: Option<Duration>,
}

impl Config {
    /// `get` maps a variable name to its value. Unparsable values fall back to the defaults, and a
    /// cap of 0 becomes 1 (a 0 cap would be a silent kill switch; the real one is `DISABLED`).
    fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Self {
        let number = |name: &str| get(name).and_then(|v| v.trim().parse::<u64>().ok());
        let idle = number(ENV_IDLE).unwrap_or(DEFAULT_IDLE_S);
        let max = get(ENV_MAX)
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(DEFAULT_MAX);
        let max_ra = get(ENV_MAX_RA)
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(DEFAULT_MAX_RA);
        let disabled = get(ENV_DISABLED)
            .is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true"));
        let off = get(ENV_OFF)
            .map(|v| v.split(',').filter_map(Servidor::do_nome).collect())
            .unwrap_or_default();
        Self {
            idle: Duration::from_secs(idle),
            max: max.max(1),
            max_ra: max_ra.max(1),
            disabled,
            off,
            backoff: DEFAULT_BACKOFF,
            aquecimento: Duration::from_secs(number(ENV_WARMUP).unwrap_or(DEFAULT_WARMUP_S).max(1)),
            l2_budget: Some(Duration::from_millis(crate::pyright::l2_budget_ms_com(
                &get,
            ))),
        }
    }

    fn from_env() -> Self {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// The cap of live sessions of one verifier.
    fn teto(&self, servidor: Servidor) -> usize {
        if servidor == Servidor::RustAnalyzer {
            self.max_ra
        } else {
            self.max
        }
    }
}

/// A live session and when it was last used.
struct Entrada {
    sessao: Box<dyn Sessao>,
    ultimo_uso: Instant,
}

/// "Do not try to start this root's server again before `ate`", and why it failed.
struct Backoff {
    ate: Instant,
    motivo: String,
}

/// A session being prepared on a background thread since `desde`.
struct Aquecimento {
    desde: Instant,
}

/// Everything known about one `(root, verifier)`. Empty = nothing worth keeping.
#[derive(Default)]
struct Slot {
    entrada: Option<Entrada>,
    backoff: Option<Backoff>,
    aquecendo: Option<Aquecimento>,
}

impl Slot {
    fn is_empty(&self) -> bool {
        self.entrada.is_none() && self.backoff.is_none() && self.aquecendo.is_none()
    }
}

/// What the registry is keyed by: which verifier, for which project, and — where one project can
/// hold several independent installs — which one (the directory whose `node_modules` provides the
/// `tsserver`, the Cargo workspace root). The sub-key is also what makes a backoff end by itself when
/// the situation changes (a `typescript` installed in a closer package is a different key).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Chave {
    servidor: Servidor,
    raiz: PathBuf,
    sub: PathBuf,
}

/// How sessions are created. `abrir` is the only way a session comes to exist, which is what lets the
/// tests count spawns and use a scripted server; `pre_checar` is the cheap, pure part of discovery that
/// a background-started verifier answers *before* any thread is launched, so that the commonest
/// absences (no `Cargo.toml`, no `node_modules/typescript`, no `deno.json`) reach the hook at once.
trait Abridor: Send + Sync {
    /// Is there anything to start? `Err` = why the layer is absent here. Must not spawn a server.
    fn pre_checar(&self, _alvo: &Alvo<'_>) -> Result<(), String> {
        Ok(())
    }

    /// Discover → spawn → initialize. `Err` = why the layer is absent.
    fn abrir(&self, alvo: &Alvo<'_>) -> Result<Box<dyn Sessao>, String>;
}

type Spawner = Arc<dyn Abridor>;

/// The production [`Abridor`]: every [`Servidor`] discovers its verifier in the project.
struct Despachante {
    ambiente: Ambiente,
}

impl Abridor for Despachante {
    fn pre_checar(&self, alvo: &Alvo<'_>) -> Result<(), String> {
        alvo.servidor.pre_checar(alvo, &self.ambiente)
    }

    fn abrir(&self, alvo: &Alvo<'_>) -> Result<Box<dyn Sessao>, String> {
        alvo.servidor.abrir(alvo, &self.ambiente)
    }
}

/// A closure over the project root, boxed: what the tests script.
#[cfg(test)]
type FabricaDeTeste<S, E> = Box<dyn Fn(&Path) -> Result<S, E> + Send + Sync>;

/// Adapts a closure over the project root (what the tests script) to an [`Abridor`].
#[cfg(test)]
struct AbridorDeTeste<F>(F);

#[cfg(test)]
impl<S, E, F> Abridor for AbridorDeTeste<F>
where
    S: Sessao + 'static,
    E: std::fmt::Display,
    F: Fn(&Path) -> Result<S, E> + Send + Sync,
{
    fn abrir(&self, alvo: &Alvo<'_>) -> Result<Box<dyn Sessao>, String> {
        (self.0)(alvo.raiz)
            .map(|sessao| Box::new(sessao) as Box<dyn Sessao>)
            .map_err(|e| e.to_string())
    }
}

/// The registry. `spawner` is the only way a session is created, which is what lets the tests count
/// spawns and use a scripted server.
struct Pool {
    slots: Mutex<Slots>,
    spawner: Spawner,
}

/// An [`Edicao`] that owns its data, for the thread that warms a session up with the first edit.
struct EdicaoPropria {
    path: String,
    content: String,
    disk: Option<String>,
    overlays: Vec<(String, String)>,
}

impl EdicaoPropria {
    fn de(edicao: &Edicao<'_>) -> Self {
        Self {
            path: edicao.path.to_string(),
            content: edicao.content.to_string(),
            disk: edicao.disk.map(str::to_string),
            overlays: edicao.overlays.to_vec(),
        }
    }

    fn como_edicao(&self) -> Edicao<'_> {
        Edicao {
            path: &self.path,
            content: &self.content,
            disk: self.disk.as_deref(),
            overlays: &self.overlays,
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `None` = locked by someone else (in use); a poisoned lock is recovered like any other.
fn try_lock_any<T>(mutex: &Mutex<T>) -> Option<MutexGuard<'_, T>> {
    match mutex.try_lock() {
        Ok(guard) => Some(guard),
        Err(TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    }
}

/// The registry key of a root: the canonical path, so `./p`, `/abs/p` and a symlink to it share one session.
fn chave(root: &Path) -> PathBuf {
    root.canonicalize()
        .or_else(|_| std::path::absolute(root))
        .unwrap_or_else(|_| root.to_path_buf())
}

/// Closes sessions off the caller's clock: `Drop` waits for the server to leave (and kills it after
/// half a second), which must never be added to a hook call.
fn dispose(sessions: Vec<Box<dyn Sessao>>) {
    for sessao in sessions {
        let _ = thread::Builder::new()
            .name("lsp-dispose".into())
            .spawn(move || drop(sessao));
    }
}

type Slots = HashMap<Chave, Arc<Mutex<Slot>>>;

/// Live sessions of the other slots of the same verifier, and which of them may be evicted (least
/// recently used first).
struct Census {
    live: usize,
    evictable: Vec<(Instant, Chave)>,
}

fn census(slots: &Slots, key: &Chave) -> Census {
    let mut census = Census {
        live: 0,
        evictable: Vec::new(),
    };
    let same_kind = |other: &&Chave| other.servidor == key.servidor && *other != key;
    for (other, slot) in slots.iter().filter(|(other, _)| same_kind(other)) {
        match presenca(slot) {
            Presenca::Ocupado | Presenca::Aquecendo => census.live += 1,
            Presenca::Viva(ultimo_uso) => {
                census.live += 1;
                census.evictable.push((ultimo_uso, other.clone()));
            }
            Presenca::Vazio => {}
        }
    }
    census.evictable.sort_by_key(|(used, _)| *used);
    census
}

/// What a slot holds, as the cap sees it.
enum Presenca {
    /// Locked by a check: live, not evictable.
    Ocupado,
    /// A session being prepared: live, not evictable.
    Aquecendo,
    /// A warm session last used at the instant: live, evictable.
    Viva(Instant),
    Vazio,
}

fn presenca(slot: &Mutex<Slot>) -> Presenca {
    let Some(guard) = try_lock_any(slot) else {
        return Presenca::Ocupado;
    };
    match (&guard.entrada, &guard.aquecendo) {
        (Some(entrada), _) => Presenca::Viva(entrada.ultimo_uso),
        (None, Some(_)) => Presenca::Aquecendo,
        (None, None) => Presenca::Vazio,
    }
}

/// Evicts from the front of `census.evictable` while the cap is still reached. Returns the live count
/// that remains and the sessions to close.
fn evict_oldest(slots: &Slots, census: Census, max: usize) -> (usize, Vec<Box<dyn Sessao>>) {
    let mut live = census.live;
    let mut evicted = Vec::new();
    for (_, other) in census.evictable {
        if live < max {
            break;
        }
        if let Some(sessao) = take_session(slots, &other) {
            evicted.push(sessao);
            live -= 1;
        }
    }
    (live, evicted)
}

fn take_session(slots: &Slots, key: &Chave) -> Option<Box<dyn Sessao>> {
    let mut guard = try_lock_any(slots.get(key)?)?;
    guard.entrada.take().map(|entrada| entrada.sessao)
}

/// Takes the session out of idle slots (and clears expired backoffs); `except` is never touched.
fn collect_stale(
    slots: &Slots,
    idle: Duration,
    now: Instant,
    except: Option<&Chave>,
) -> Vec<Box<dyn Sessao>> {
    let mut stale = Vec::new();
    for (_, slot) in slots.iter().filter(|(key, _)| except != Some(*key)) {
        let Some(mut guard) = try_lock_any(slot) else {
            continue; // in use is the opposite of idle
        };
        if let Some(sessao) = take_if_idle(&mut guard, idle, now) {
            stale.push(sessao);
        }
        if guard.backoff.as_ref().is_some_and(|b| now >= b.ate) {
            guard.backoff = None;
        }
    }
    stale
}

fn take_if_idle(slot: &mut Slot, idle: Duration, now: Instant) -> Option<Box<dyn Sessao>> {
    let idle_for = now.saturating_duration_since(slot.entrada.as_ref()?.ultimo_uso);
    if idle_for > idle {
        slot.entrada.take().map(|entrada| entrada.sessao)
    } else {
        None
    }
}

/// Forgets slots that hold nothing. `strong_count == 1` under the registry lock means no thread holds
/// the slot (clones are only handed out under that lock, and a warm-up thread owns one), so removing
/// it cannot orphan a session.
fn forget_empty(slots: &mut Slots, except: Option<&Chave>) {
    slots.retain(|key, slot| {
        except == Some(key)
            || Arc::strong_count(slot) > 1
            || !try_lock_any(slot).is_some_and(|guard| guard.is_empty())
    });
}

impl Pool {
    fn com_spawner(spawner: Spawner) -> Self {
        Self {
            slots: Mutex::new(HashMap::new()),
            spawner,
        }
    }

    /// A pool whose spawner serves every verifier from the project root alone — what the tests use
    /// to count spawns and to script a server.
    #[cfg(test)]
    fn new<S, E>(spawner: FabricaDeTeste<S, E>) -> Self
    where
        S: Sessao + 'static,
        E: std::fmt::Display + 'static,
    {
        Self::com_spawner(Arc::new(AbridorDeTeste(spawner)))
    }

    /// The slot of `key`, created empty on first sight. Held only briefly under the registry lock.
    fn slot_da(&self, key: &Chave) -> Arc<Mutex<Slot>> {
        let mut slots = lock(&self.slots);
        Arc::clone(slots.entry(key.clone()).or_default())
    }

    /// The pyright slot of a (canonical) root — the pre-generalisation shape, for the tests.
    #[cfg(test)]
    fn slot_for(&self, root: &Path) -> Arc<Mutex<Slot>> {
        self.slot_da(&Chave {
            servidor: Servidor::Pyright,
            raiz: root.to_path_buf(),
            sub: PathBuf::new(),
        })
    }

    /// [`Pool::checar_servidor`] for pyright (the shape every caller had before the pool was generic).
    #[cfg(test)]
    fn checar(&self, cfg: &Config, now: Instant, root: &Path, edicao: &Edicao<'_>) -> L2Resultado {
        self.checar_servidor(cfg, now, Servidor::Pyright, root, edicao)
    }

    fn checar_servidor(
        &self,
        cfg: &Config,
        now: Instant,
        servidor: Servidor,
        root: &Path,
        edicao: &Edicao<'_>,
    ) -> L2Resultado {
        if cfg.disabled {
            return L2Resultado::unknown("camada 2 desligada (TOURING_VGP_L2_DISABLED=1)", 0);
        }
        if cfg.off.contains(&servidor) {
            return L2Resultado::unknown(
                format!(
                    "camada 2 ({}) desligada ({ENV_OFF} lista {})",
                    servidor.nome(),
                    servidor.nome()
                ),
                0,
            );
        }
        let started = Instant::now();
        let raiz = chave(root);
        let key = Chave {
            servidor,
            sub: servidor.subchave(&raiz, edicao.path),
            raiz,
        };
        self.reap(cfg.idle, now, Some(&key));
        let slot = self.slot_da(&key);
        let mut guard = lock(&slot);
        if let Err(mut unknown) = self.ensure(cfg, now, &key, &slot, &mut guard, edicao) {
            unknown.elapsed_ms = elapsed_ms(started);
            return unknown;
        }
        let Some(entrada) = guard.entrada.as_mut() else {
            return L2Resultado::unknown("camada 2: estado interno inconsistente (sem sessão)", 0);
        };
        entrada.ultimo_uso = now;
        let verificado = entrada.sessao.verificar(edicao, cfg.l2_budget);
        if !verificado.saudavel
            && let Some(broken) = guard.entrada.take()
        {
            // could not be measured (dead server, deadline): never reuse it — the next use recreates it
            dispose(vec![broken.sessao]);
        }
        verificado.resultado
    }

    /// Makes sure `slot` holds a live session, or says why it does not (backoff, cap, spawn failure,
    /// still warming up).
    fn ensure(
        &self,
        cfg: &Config,
        now: Instant,
        key: &Chave,
        slot_arc: &Arc<Mutex<Slot>>,
        slot: &mut Slot,
        edicao: &Edicao<'_>,
    ) -> Result<(), L2Resultado> {
        if slot.entrada.is_some() {
            return Ok(());
        }
        if let Some(aquecimento) = &slot.aquecendo {
            let secs = now.saturating_duration_since(aquecimento.desde).as_secs();
            return Err(L2Resultado::unknown(
                format!(
                    "camada 2 ({}) aquecendo há {secs} s: a primeira edição prepara o verificador do projeto",
                    key.servidor.nome()
                ),
                0,
            ));
        }
        if let Some(backoff) = slot.backoff.as_ref().filter(|b| now < b.ate) {
            let wait = backoff.ate.saturating_duration_since(now).as_secs();
            return Err(L2Resultado::unknown(
                format!("camada 2 em backoff por mais {wait} s: {}", backoff.motivo),
                0,
            ));
        }
        slot.backoff = None;
        self.make_room(cfg.teto(key.servidor), key)?;
        match key.servidor.arranque() {
            Arranque::Sincrono => self.abrir_aqui(cfg, now, key, slot, edicao),
            Arranque::SegundoPlano => {
                self.abrir_em_segundo_plano(cfg, now, key, slot_arc, slot, edicao)
            }
        }
    }

    /// Cheap servers: spawn inside the check, under the slot lock.
    fn abrir_aqui(
        &self,
        cfg: &Config,
        now: Instant,
        key: &Chave,
        slot: &mut Slot,
        edicao: &Edicao<'_>,
    ) -> Result<(), L2Resultado> {
        let alvo = Alvo {
            servidor: key.servidor,
            raiz: &key.raiz,
            arquivo: edicao.path,
        };
        match self.spawner.abrir(&alvo) {
            Ok(sessao) => {
                slot.entrada = Some(Entrada {
                    sessao,
                    ultimo_uso: now,
                });
                Ok(())
            }
            Err(motivo) => Err(registrar_ausencia(cfg, now, slot, motivo)),
        }
    }

    /// Slow servers: mark the slot, hand the first edit to a thread that opens and primes the session,
    /// and answer `unknown: warming up` now.
    fn abrir_em_segundo_plano(
        &self,
        cfg: &Config,
        now: Instant,
        key: &Chave,
        slot_arc: &Arc<Mutex<Slot>>,
        slot: &mut Slot,
        edicao: &Edicao<'_>,
    ) -> Result<(), L2Resultado> {
        let alvo = Alvo {
            servidor: key.servidor,
            raiz: &key.raiz,
            arquivo: edicao.path,
        };
        if let Err(motivo) = self.spawner.pre_checar(&alvo) {
            return Err(registrar_ausencia(cfg, now, slot, motivo));
        }
        let tarefa = Aquecer {
            spawner: Arc::clone(&self.spawner),
            slot: Arc::clone(slot_arc),
            key: key.clone(),
            edicao: EdicaoPropria::de(edicao),
            prazo: cfg.aquecimento,
            backoff: cfg.backoff,
        };
        slot.aquecendo = Some(Aquecimento { desde: now });
        let lancada = thread::Builder::new()
            .name("l2-aquecimento".into())
            .spawn(move || tarefa.executar());
        if let Err(e) = lancada {
            slot.aquecendo = None;
            let motivo = format!("camada 2: não foi possível iniciar o aquecimento: {e}");
            return Err(registrar_ausencia(cfg, now, slot, motivo));
        }
        Err(L2Resultado::unknown(
            format!(
                "camada 2 ({}) aquecendo: a primeira edição prepara o verificador do projeto",
                key.servidor.nome()
            ),
            0,
        ))
    }

    /// Evicts least-recently-used sessions of OTHER slots of the same verifier until one more fits
    /// under `max`. A session that is in use (its slot is locked) or warming counts as live but is
    /// never evicted; if that leaves no room, the caller gets an `unknown` instead of a session
    /// beyond the cap.
    fn make_room(&self, max: usize, key: &Chave) -> Result<(), L2Resultado> {
        let (live, evicted) = {
            let slots = lock(&self.slots);
            evict_oldest(&slots, census(&slots, key), max)
        };
        dispose(evicted);
        if live < max {
            Ok(())
        } else {
            Err(L2Resultado::unknown(
                format!("camada 2: teto de {max} sessão(ões) viva(s) atingido e todas em uso"),
                0,
            ))
        }
    }

    /// Closes every session idle for more than `idle` at `now`, forgets expired backoffs, and drops
    /// slots that hold nothing and that nobody else references. `except` (the slot about to be used)
    /// is left alone. Returns how many sessions were closed.
    fn reap(&self, idle: Duration, now: Instant, except: Option<&Chave>) -> usize {
        let stale = {
            let mut slots = lock(&self.slots);
            let stale = collect_stale(&slots, idle, now, except);
            forget_empty(&mut slots, except);
            stale
        };
        let closed = stale.len();
        dispose(stale);
        closed
    }

    fn reap_idle(&self, cfg: &Config, now: Instant) -> usize {
        self.reap(cfg.idle, now, None)
    }

    #[cfg(test)]
    fn vivas(&self) -> usize {
        lock(&self.slots)
            .values()
            .filter(|slot| try_lock_any(slot).is_some_and(|guard| guard.entrada.is_some()))
            .count()
    }

    #[cfg(test)]
    fn slots_len(&self) -> usize {
        lock(&self.slots).len()
    }
}

/// The work of a warm-up thread: open the session, prime it with the first edit, install it.
struct Aquecer {
    spawner: Spawner,
    slot: Arc<Mutex<Slot>>,
    key: Chave,
    edicao: EdicaoPropria,
    prazo: Duration,
    backoff: Duration,
}

impl Aquecer {
    fn executar(self) {
        // a panic in a spawner must not leave the slot "warming" for ever: it ends as a backoff
        let aberta =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.abrir_e_aquecer()))
                .unwrap_or_else(|_| Err("o aquecimento terminou em pânico".to_string()));
        let mut guard = lock(&self.slot);
        guard.aquecendo = None;
        match aberta {
            Ok(sessao) => {
                guard.entrada = Some(Entrada {
                    sessao,
                    ultimo_uso: Instant::now(),
                });
            }
            Err(motivo) => {
                guard.backoff = Some(Backoff {
                    ate: Instant::now() + self.backoff,
                    motivo,
                });
            }
        }
    }

    fn abrir_e_aquecer(&self) -> Result<Box<dyn Sessao>, String> {
        let alvo = Alvo {
            servidor: self.key.servidor,
            raiz: &self.key.raiz,
            arquivo: &self.edicao.path,
        };
        let mut sessao = self.spawner.abrir(&alvo)?;
        sessao.aquecer(&self.edicao.como_edicao(), self.prazo)?;
        Ok(sessao)
    }
}

/// A verifier that cannot be had here: remember why (the next calls get it from the backoff without
/// trying again) and tell the caller now.
fn registrar_ausencia(cfg: &Config, now: Instant, slot: &mut Slot, motivo: String) -> L2Resultado {
    slot.backoff = Some(Backoff {
        ate: now + cfg.backoff,
        motivo: motivo.clone(),
    });
    L2Resultado::unknown(motivo, 0)
}

fn elapsed_ms(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn global() -> &'static Pool {
    static POOL: OnceLock<Pool> = OnceLock::new();
    POOL.get_or_init(|| {
        Pool::com_spawner(Arc::new(Despachante {
            ambiente: Ambiente::do_processo(),
        }))
    })
}

/// Checks `content` as the file `path` of the project at `project_root` with that root's warm pyright
/// session, creating it on first use (see the module docs for the lifecycle rules).
///
/// Never panics and never returns an error: whatever prevents a measurement — layer disabled, a
/// process that did not opt in ([`habilitar_neste_processo`]), no language server in the venv,
/// backoff, session cap, dead server, deadline — is an
/// [`L2Resultado`] with [`crate::achado::Verdict::Unknown`] and the reason in `motivo`. `disk` and
/// `overlays` are those of [`crate::pyright::PyrightSession::check`].
pub fn checar(
    project_root: &Path,
    path: &str,
    content: &str,
    disk: Option<&str>,
    overlays: &[(String, String)],
) -> L2Resultado {
    checar_em(
        Servidor::Pyright,
        project_root,
        path,
        content,
        disk,
        overlays,
    )
}

/// [`checar`] for any [`Servidor`]: the same lifecycle, caps, backoff and opt-in, with the verifier
/// the project provides (`rust-analyzer`, the `tsserver` of its `node_modules/typescript`, `deno lsp`).
/// An absent verifier is an `unknown` with the reason; the first edit of a slow verifier only warms it
/// up (`unknown: aquecendo`) and later edits are measured.
pub fn checar_em(
    servidor: Servidor,
    project_root: &Path,
    path: &str,
    content: &str,
    disk: Option<&str>,
    overlays: &[(String, String)],
) -> L2Resultado {
    checar_em_com_orcamento(servidor, project_root, path, content, disk, overlays, None)
}

/// [`checar_em`] with an explicit per-call budget, used by the CLI flag; absent means the env/default.
fn checar_em_com_orcamento(
    servidor: Servidor,
    project_root: &Path,
    path: &str,
    content: &str,
    disk: Option<&str>,
    overlays: &[(String, String)],
    orcamento: Option<Duration>,
) -> L2Resultado {
    let edicao = Edicao {
        path,
        content,
        disk,
        overlays,
    };
    let mut cfg = Config::from_env();
    if let Some(orcamento) = orcamento {
        cfg.l2_budget = Some(orcamento);
    }
    checar_com_servidor(
        global(),
        habilitado(),
        &cfg,
        Instant::now(),
        servidor,
        project_root,
        &edicao,
    )
}

/// [`checar_em`] for a layer-1 `lang` (`python`, `rust`, `typescript`, `tsx`, `javascript`): picks the
/// verifier with [`Servidor::para_lang`]. A language without a layer 2 (`html`, `css`, unknown) is an
/// `unknown` saying so, and layer 1 stands alone.
pub fn checar_lang(
    lang: &str,
    project_root: &Path,
    path: &str,
    content: &str,
    disk: Option<&str>,
    overlays: &[(String, String)],
) -> L2Resultado {
    match Servidor::para_lang(lang, project_root, path) {
        Some(servidor) => checar_em(servidor, project_root, path, content, disk, overlays),
        None => L2Resultado::unknown(
            format!("camada 2: não há verificador para a linguagem `{lang}`"),
            0,
        ),
    }
}

/// [`checar_lang`] with an explicit per-call budget in milliseconds.
pub fn checar_lang_com_orcamento(
    lang: &str,
    project_root: &Path,
    path: &str,
    content: &str,
    disk: Option<&str>,
    overlays: &[(String, String)],
    orcamento_ms: u64,
) -> L2Resultado {
    match Servidor::para_lang(lang, project_root, path) {
        Some(servidor) => checar_em_com_orcamento(
            servidor,
            project_root,
            path,
            content,
            disk,
            overlays,
            Some(Duration::from_millis(orcamento_ms)),
        ),
        None => L2Resultado::unknown(
            format!("camada 2: não há verificador para a linguagem `{lang}`"),
            0,
        ),
    }
}

/// Whether this process opted in to layer 2 (see [`habilitar_neste_processo`]).
static HABILITADO: AtomicBool = AtomicBool::new(false);

/// Why a check answers `unknown` in a process that never opted in.
fn motivo_nao_habilitado(servidor: Servidor) -> String {
    format!(
        "camada 2 desligada neste processo (só o daemon mantém {})",
        servidor.nome()
    )
}

/// Opts this process in to layer 2. Idempotent. Only the long-lived daemon and the CLI that asks for
/// `--camadas 1,2` call this: any other process (notably the ephemeral standalone `touring-hook`
/// fallback) would start a language server per call and could leave it orphaned.
pub fn habilitar_neste_processo() {
    HABILITADO.store(true, Ordering::Relaxed);
}

fn habilitado() -> bool {
    HABILITADO.load(Ordering::Relaxed)
}

/// [`checar_com_servidor`] for pyright, with the opt-in decision injected, so the guard is testable
/// without touching the process-wide flag.
#[cfg(test)]
fn checar_com(
    pool: &Pool,
    habilitado: bool,
    cfg: &Config,
    now: Instant,
    root: &Path,
    edicao: &Edicao<'_>,
) -> L2Resultado {
    checar_com_servidor(pool, habilitado, cfg, now, Servidor::Pyright, root, edicao)
}

fn checar_com_servidor(
    pool: &Pool,
    habilitado: bool,
    cfg: &Config,
    now: Instant,
    servidor: Servidor,
    root: &Path,
    edicao: &Edicao<'_>,
) -> L2Resultado {
    if !habilitado {
        return L2Resultado::unknown(motivo_nao_habilitado(servidor), 0);
    }
    pool.checar_servidor(cfg, now, servidor, root, edicao)
}

/// Closes every session idle for more than `TOURING_VGP_L2_IDLE_S` (default 900) at `now` and
/// returns how many were closed. `checar` already does this for the other roots on every call; the
/// daemon can also call it on a timer so an abandoned root's server does not outlive its use.
pub fn reap_idle(now: Instant) -> usize {
    global().reap_idle(&Config::from_env(), now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::LspClient;
    use crate::pyright::{PyrightError, PyrightSession, Verdict, file_uri};
    use serde_json::{Map, json};
    use std::fs;
    use std::process::Command;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    // ── helpers ────────────────────────────────────────────────────────────

    /// A unique scratch tree removed on drop (no external dep, panic-safe).
    struct TempTree {
        root: PathBuf,
    }

    impl TempTree {
        fn new(tag: &str) -> Self {
            static N: AtomicUsize = AtomicUsize::new(0);
            let root = std::env::temp_dir().join(format!(
                "touring_pyright_pool_{tag}_{}_{}",
                std::process::id(),
                N.fetch_add(1, Ordering::SeqCst)
            ));
            fs::create_dir_all(&root).unwrap();
            Self {
                root: root.canonicalize().unwrap(),
            }
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    /// A scripted stand-in for pyright: `# undef N` lines become `reportUndefinedVariable`; `# mata`
    /// makes the server exit on `didOpen`; `# silencio` makes it publish nothing. `didClose` publishes
    /// the null-version cleanup, like pyright 1.1.408.
    const FAKE: &str = r##"
import sys, json, re
rin, out = sys.stdin.buffer, sys.stdout.buffer

def read():
    n = 0
    while True:
        line = rin.readline()
        if not line:
            sys.exit(0)
        line = line.strip()
        if not line:
            break
        if line.lower().startswith(b"content-length:"):
            n = int(line.split(b":")[1])
    return json.loads(rin.read(n))

def send(o):
    b = json.dumps(o).encode()
    out.write(b"Content-Length: %d\r\n\r\n" % len(b) + b)
    out.flush()

def pub(uri, ver, diags):
    send({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics",
          "params": {"uri": uri, "version": ver, "diagnostics": diags}})

while True:
    m = read()
    meth = m.get("method")
    if meth == "initialize":
        send({"jsonrpc": "2.0", "id": m["id"], "result": {"capabilities": {}}})
    elif meth == "shutdown":
        send({"jsonrpc": "2.0", "id": m["id"], "result": None})
    elif meth == "exit":
        sys.exit(0)
    elif meth == "textDocument/didOpen":
        td = m["params"]["textDocument"]
        text = td["text"]
        if "# mata" in text:
            sys.exit(0)
        if "# silencio" in text:
            continue
        diags = []
        for i, line in enumerate(text.split("\n")):
            hit = re.match(r"# undef (\S+)", line)
            if hit:
                diags.append({"code": "reportUndefinedVariable",
                              "message": '"%s" is not defined' % hit.group(1),
                              "range": {"start": {"line": i, "character": 0},
                                        "end": {"line": i, "character": 1}}})
        pub(td["uri"], td["version"], diags)
    elif meth == "textDocument/didClose":
        pub(m["params"]["textDocument"]["uri"], None, [])
"##;

    /// Counts what the spawner is asked to do and remembers every server pid per root.
    #[derive(Default)]
    struct Probe {
        attempts: AtomicUsize,
        spawns: AtomicUsize,
        fail: AtomicBool,
        pids: Mutex<Vec<(PathBuf, u32)>>,
    }

    impl Probe {
        fn pid_of(&self, root: &Path) -> u32 {
            let pids = self.pids.lock().unwrap();
            pids.iter()
                .rev()
                .find(|(r, _)| r == root)
                .map(|(_, pid)| *pid)
                .expect("a server was spawned for this root")
        }
    }

    fn pool(probe: &Arc<Probe>, budget_ms: u64) -> Pool {
        let probe = Arc::clone(probe);
        Pool::new(Box::new(move |root| {
            probe.attempts.fetch_add(1, Ordering::SeqCst);
            if probe.fail.load(Ordering::SeqCst) {
                return Err(PyrightError::NaoEncontrado {
                    raiz: root.display().to_string(),
                    procurados: "fake".into(),
                });
            }
            let mut cmd = Command::new("python3");
            cmd.arg("-c").arg(FAKE);
            let mut client = LspClient::spawn(cmd, Map::new())?;
            probe.spawns.fetch_add(1, Ordering::SeqCst);
            probe
                .pids
                .lock()
                .unwrap()
                .push((root.to_path_buf(), client.pid()));
            client.initialize(
                &file_uri(root),
                "p",
                json!({}),
                Instant::now() + Duration::from_secs(10),
            )?;
            Ok(PyrightSession::from_client(
                client,
                root.to_path_buf(),
                Duration::from_millis(budget_ms),
            ))
        }))
    }

    fn cfg() -> Config {
        Config::from_lookup(|_| None)
    }

    fn check(pool: &Pool, cfg: &Config, now: Instant, root: &Path, text: &str) -> L2Resultado {
        let edicao = Edicao {
            path: "<string>",
            content: text,
            disk: None,
            overlays: &[],
        };
        pool.checar(cfg, now, root, &edicao)
    }

    fn at(base: Instant, secs: u64) -> Instant {
        base + Duration::from_secs(secs)
    }

    /// Polls until the process is gone (disposal runs on a background thread).
    fn wait_gone(pid: u32) -> bool {
        let proc_dir = PathBuf::from(format!("/proc/{pid}"));
        for _ in 0..100 {
            if !proc_dir.exists() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    const BAD: &str = "# undef fantasma\n";

    // ── config ─────────────────────────────────────────────────────────────

    #[test]
    fn config_defaults_overrides_and_garbage() {
        let d = Config::from_lookup(|_| None);
        assert_eq!(
            (d.idle, d.max, d.disabled, d.backoff),
            (Duration::from_secs(900), 2, false, Duration::from_secs(300))
        );
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            Config::from_lookup(move |k| {
                pairs
                    .iter()
                    .find(|(name, _)| *name == k)
                    .map(|(_, v)| (*v).to_string())
            })
        };
        let c = env(&[
            ("TOURING_VGP_L2_IDLE_S", "60"),
            ("TOURING_VGP_L2_MAX", "5"),
            ("TOURING_VGP_L2_DISABLED", "1"),
        ]);
        assert_eq!(
            (c.idle, c.max, c.disabled),
            (Duration::from_secs(60), 5, true)
        );
        // garbage falls back to the defaults; a cap of 0 would disable the layer silently → at least 1
        let g = env(&[
            ("TOURING_VGP_L2_IDLE_S", "abc"),
            ("TOURING_VGP_L2_MAX", "-3"),
            ("TOURING_VGP_L2_DISABLED", "talvez"),
        ]);
        assert_eq!(
            (g.idle, g.max, g.disabled),
            (Duration::from_secs(900), 2, false)
        );
        assert_eq!(env(&[("TOURING_VGP_L2_MAX", "0")]).max, 1);
        assert!(env(&[("TOURING_VGP_L2_DISABLED", "true")]).disabled);
        assert!(!env(&[("TOURING_VGP_L2_DISABLED", "0")]).disabled);
    }

    // ── reuse ──────────────────────────────────────────────────────────────

    #[test]
    fn one_spawn_serves_many_checks_of_the_same_root() {
        let tree = TempTree::new("reuse");
        let probe = Arc::new(Probe::default());
        let pool = pool(&probe, 5000);
        let now = Instant::now();
        for i in 0..6 {
            let r = check(&pool, &cfg(), at(now, i), &tree.root, BAD);
            assert_eq!(r.verdict, Verdict::Deny, "check #{i}: {r:?}");
        }
        assert_eq!(
            probe.spawns.load(Ordering::SeqCst),
            1,
            "N checks must share ONE server"
        );
        assert_eq!(pool.vivas(), 1);
    }

    #[test]
    fn concurrent_calls_on_one_root_share_one_server() {
        let tree = TempTree::new("concurrent");
        let probe = Arc::new(Probe::default());
        let pool = pool(&probe, 5000);
        let now = Instant::now();
        std::thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| {
                    for _ in 0..5 {
                        let r = check(&pool, &cfg(), now, &tree.root, BAD);
                        assert_eq!(r.verdict, Verdict::Deny, "{r:?}");
                    }
                });
            }
        });
        assert_eq!(probe.spawns.load(Ordering::SeqCst), 1);
    }

    // ── discover failure → backoff ─────────────────────────────────────────

    #[test]
    fn failed_discover_is_unknown_and_backs_off_for_five_minutes() {
        let tree = TempTree::new("backoff");
        let probe = Arc::new(Probe::default());
        probe.fail.store(true, Ordering::SeqCst);
        let pool = pool(&probe, 5000);
        let t0 = Instant::now();

        let r = check(&pool, &cfg(), t0, &tree.root, BAD);
        assert_eq!(r.verdict, Verdict::Unknown);
        assert!(
            r.motivo.as_deref().is_some_and(|m| m.contains("fake")),
            "{r:?}"
        );
        assert_eq!(probe.attempts.load(Ordering::SeqCst), 1);

        for secs in [10, 299] {
            let r = check(&pool, &cfg(), at(t0, secs), &tree.root, BAD);
            assert_eq!(r.verdict, Verdict::Unknown);
            let motivo = r.motivo.expect("unknown carries its reason");
            assert!(
                motivo.contains("backoff") && motivo.contains("fake"),
                "{motivo}"
            );
        }
        assert_eq!(
            probe.attempts.load(Ordering::SeqCst),
            1,
            "inside the backoff there must be NO spawn attempt"
        );

        // backoff over: tries again, fails again, a new window starts
        let r = check(&pool, &cfg(), at(t0, 301), &tree.root, BAD);
        assert_eq!(r.verdict, Verdict::Unknown);
        assert_eq!(probe.attempts.load(Ordering::SeqCst), 2);
        check(&pool, &cfg(), at(t0, 400), &tree.root, BAD);
        assert_eq!(probe.attempts.load(Ordering::SeqCst), 2);

        // the project got a venv meanwhile → recovers on the first attempt after the window
        probe.fail.store(false, Ordering::SeqCst);
        let r = check(&pool, &cfg(), at(t0, 602), &tree.root, BAD);
        assert_eq!(r.verdict, Verdict::Deny, "{r:?}");
        assert_eq!(probe.attempts.load(Ordering::SeqCst), 3);
        assert_eq!(probe.spawns.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn backoff_is_per_root() {
        let a = TempTree::new("bo_a");
        let b = TempTree::new("bo_b");
        let probe = Arc::new(Probe::default());
        probe.fail.store(true, Ordering::SeqCst);
        let pool = pool(&probe, 5000);
        let t0 = Instant::now();
        check(&pool, &cfg(), t0, &a.root, BAD);
        probe.fail.store(false, Ordering::SeqCst);
        let r = check(&pool, &cfg(), at(t0, 1), &b.root, BAD);
        assert_eq!(
            r.verdict,
            Verdict::Deny,
            "root B must not inherit A's backoff: {r:?}"
        );
    }

    // ── idleness ───────────────────────────────────────────────────────────

    #[test]
    fn idle_session_is_reaped_with_an_injected_clock() {
        let tree = TempTree::new("idle");
        let probe = Arc::new(Probe::default());
        let pool = pool(&probe, 5000);
        let t0 = Instant::now();
        check(&pool, &cfg(), t0, &tree.root, BAD);
        let pid = probe.pid_of(&tree.root);

        assert_eq!(
            pool.reap_idle(&cfg(), at(t0, 899)),
            0,
            "899 s idle is under the 900 s limit"
        );
        assert_eq!(
            pool.reap_idle(&cfg(), at(t0, 900)),
            0,
            "exactly the limit is not \"more than\" the limit"
        );
        assert_eq!(pool.vivas(), 1);
        assert_eq!(pool.reap_idle(&cfg(), at(t0, 901)), 1);
        assert_eq!(pool.vivas(), 0);
        assert!(
            wait_gone(pid),
            "reaped server {pid} must not outlive its session"
        );

        let r = check(&pool, &cfg(), at(t0, 902), &tree.root, BAD);
        assert_eq!(r.verdict, Verdict::Deny);
        assert_eq!(
            probe.spawns.load(Ordering::SeqCst),
            2,
            "a reaped root is recreated on demand"
        );
    }

    #[test]
    fn idle_sessions_are_also_reaped_by_the_next_call_of_any_root() {
        let a = TempTree::new("idle_a");
        let b = TempTree::new("idle_b");
        let probe = Arc::new(Probe::default());
        let pool = pool(&probe, 5000);
        let t0 = Instant::now();
        check(&pool, &cfg(), t0, &a.root, BAD);
        let pid_a = probe.pid_of(&a.root);

        check(&pool, &cfg(), at(t0, 1000), &b.root, BAD);
        assert_eq!(
            pool.vivas(),
            1,
            "A idle for 1000 s must be gone, only B alive"
        );
        assert!(wait_gone(pid_a));
        assert_eq!(probe.spawns.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn the_root_being_called_is_never_reaped_as_idle_right_before_use() {
        let tree = TempTree::new("idle_same");
        let probe = Arc::new(Probe::default());
        let pool = pool(&probe, 5000);
        let t0 = Instant::now();
        check(&pool, &cfg(), t0, &tree.root, BAD);
        let r = check(&pool, &cfg(), at(t0, 5000), &tree.root, BAD);
        assert_eq!(r.verdict, Verdict::Deny);
        assert_eq!(
            probe.spawns.load(Ordering::SeqCst),
            1,
            "a warm session about to be used is not killed and respawned"
        );
    }

    #[test]
    fn reap_forgets_expired_backoff_and_empty_slots() {
        let tree = TempTree::new("forget");
        let probe = Arc::new(Probe::default());
        probe.fail.store(true, Ordering::SeqCst);
        let pool = pool(&probe, 5000);
        let t0 = Instant::now();
        check(&pool, &cfg(), t0, &tree.root, BAD);
        assert_eq!(pool.slots_len(), 1);
        pool.reap_idle(&cfg(), at(t0, 100));
        assert_eq!(pool.slots_len(), 1, "an active backoff must be remembered");
        pool.reap_idle(&cfg(), at(t0, 301));
        assert_eq!(
            pool.slots_len(),
            0,
            "an expired backoff leaves nothing behind"
        );
    }

    // ── cap ────────────────────────────────────────────────────────────────

    #[test]
    fn the_third_root_evicts_the_least_recently_used_session() {
        let (a, b, c) = (
            TempTree::new("cap_a"),
            TempTree::new("cap_b"),
            TempTree::new("cap_c"),
        );
        let probe = Arc::new(Probe::default());
        let pool = pool(&probe, 5000);
        let t0 = Instant::now();
        check(&pool, &cfg(), at(t0, 0), &a.root, BAD);
        check(&pool, &cfg(), at(t0, 1), &b.root, BAD);
        // A is touched again, so B is now the least recently used
        check(&pool, &cfg(), at(t0, 2), &a.root, BAD);
        let (pid_a, pid_b) = (probe.pid_of(&a.root), probe.pid_of(&b.root));

        let r = check(&pool, &cfg(), at(t0, 3), &c.root, BAD);
        assert_eq!(r.verdict, Verdict::Deny, "{r:?}");
        assert_eq!(pool.vivas(), 2, "the cap of 2 must hold");
        assert!(wait_gone(pid_b), "B (least recent) must be evicted");
        assert!(
            Path::new(&format!("/proc/{pid_a}")).exists(),
            "A (touched) must survive"
        );
        assert_eq!(probe.spawns.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn the_cap_follows_the_configuration() {
        let (a, b) = (TempTree::new("cap1_a"), TempTree::new("cap1_b"));
        let probe = Arc::new(Probe::default());
        let pool = pool(&probe, 5000);
        let one = Config::from_lookup(|k| (k == "TOURING_VGP_L2_MAX").then(|| "1".to_string()));
        let t0 = Instant::now();
        check(&pool, &one, at(t0, 0), &a.root, BAD);
        check(&pool, &one, at(t0, 1), &b.root, BAD);
        assert_eq!(pool.vivas(), 1);
    }

    #[test]
    fn a_busy_session_is_never_evicted_and_the_cap_still_holds() {
        let (a, b) = (TempTree::new("busy_a"), TempTree::new("busy_b"));
        let probe = Arc::new(Probe::default());
        let pool = pool(&probe, 5000);
        let one = Config::from_lookup(|k| (k == "TOURING_VGP_L2_MAX").then(|| "1".to_string()));
        let t0 = Instant::now();
        check(&pool, &one, t0, &a.root, BAD);
        let slot_a = pool.slot_for(&a.root);
        let _in_use = slot_a.lock().unwrap(); // A is "checking" right now

        let r = check(&pool, &one, at(t0, 1), &b.root, BAD);
        assert_eq!(r.verdict, Verdict::Unknown, "{r:?}");
        assert!(
            r.motivo.as_deref().is_some_and(|m| m.contains("teto")),
            "{r:?}"
        );
        assert_eq!(
            probe.spawns.load(Ordering::SeqCst),
            1,
            "no spawn beyond the cap"
        );
    }

    #[test]
    fn a_busy_root_does_not_block_other_roots() {
        let (a, b) = (TempTree::new("noblock_a"), TempTree::new("noblock_b"));
        let probe = Arc::new(Probe::default());
        let pool = pool(&probe, 5000);
        let t0 = Instant::now();
        check(&pool, &cfg(), t0, &a.root, BAD);
        let slot_a = pool.slot_for(&a.root);
        let _in_use = slot_a.lock().unwrap();
        let r = check(&pool, &cfg(), at(t0, 1), &b.root, BAD);
        assert_eq!(
            r.verdict,
            Verdict::Deny,
            "B must proceed while A is locked: {r:?}"
        );
    }

    // ── kill switch ────────────────────────────────────────────────────────

    #[test]
    fn kill_switch_is_unknown_without_any_spawn() {
        let tree = TempTree::new("kill");
        let probe = Arc::new(Probe::default());
        let pool = pool(&probe, 5000);
        let off =
            Config::from_lookup(|k| (k == "TOURING_VGP_L2_DISABLED").then(|| "1".to_string()));
        let r = check(&pool, &off, Instant::now(), &tree.root, BAD);
        assert_eq!(r.verdict, Verdict::Unknown);
        assert!(
            r.motivo
                .as_deref()
                .is_some_and(|m| m.contains("camada 2 desligada")),
            "{r:?}"
        );
        assert_eq!(probe.attempts.load(Ordering::SeqCst), 0);
        assert_eq!(pool.slots_len(), 0);
    }

    // ── fail closed on state, open on the hook ─────────────────────────────

    #[test]
    fn a_dead_server_drops_the_entry_and_the_next_use_recreates_it() {
        let tree = TempTree::new("dead");
        let probe = Arc::new(Probe::default());
        let pool = pool(&probe, 5000);
        let now = Instant::now();
        assert_eq!(
            check(&pool, &cfg(), now, &tree.root, BAD).verdict,
            Verdict::Deny
        );
        let first = probe.pid_of(&tree.root);

        let r = check(&pool, &cfg(), at(now, 1), &tree.root, "# mata\n");
        assert_eq!(r.verdict, Verdict::Unknown, "{r:?}");
        assert!(r.motivo.is_some());
        assert_eq!(pool.vivas(), 0, "a failed session must not stay registered");
        assert!(wait_gone(first));

        let r = check(&pool, &cfg(), at(now, 2), &tree.root, BAD);
        assert_eq!(r.verdict, Verdict::Deny, "{r:?}");
        assert_eq!(probe.spawns.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_deadline_overrun_drops_the_entry_and_returns_the_reason() {
        let tree = TempTree::new("deadline");
        let probe = Arc::new(Probe::default());
        let pool = pool(&probe, 300);
        let now = Instant::now();
        check(&pool, &cfg(), now, &tree.root, BAD);
        let first = probe.pid_of(&tree.root);

        let started = Instant::now();
        let r = check(&pool, &cfg(), at(now, 1), &tree.root, "# silencio\n");
        assert!(
            started.elapsed() < Duration::from_millis(1800),
            "the hook must not hang"
        );
        assert_eq!(r.verdict, Verdict::Unknown, "{r:?}");
        assert!(
            r.motivo.as_deref().is_some_and(|m| m.contains("prazo")),
            "{r:?}"
        );
        assert_eq!(pool.vivas(), 0);
        assert!(wait_gone(first));
        assert_eq!(
            check(&pool, &cfg(), at(now, 2), &tree.root, BAD).verdict,
            Verdict::Deny
        );
        assert_eq!(probe.spawns.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_poisoned_slot_lock_is_recovered_not_propagated() {
        let tree = TempTree::new("poison");
        let probe = Arc::new(Probe::default());
        let pool = pool(&probe, 5000);
        let now = Instant::now();
        check(&pool, &cfg(), now, &tree.root, BAD);

        let slot = pool.slot_for(&tree.root);
        let victim = Arc::clone(&slot);
        let _ = std::thread::spawn(move || {
            let _guard = victim.lock().unwrap();
            panic!("poisoning the slot on purpose");
        })
        .join();
        assert!(slot.is_poisoned(), "fixture must actually poison the lock");

        let r = check(&pool, &cfg(), at(now, 1), &tree.root, BAD);
        assert_eq!(r.verdict, Verdict::Deny, "{r:?}");
        assert_eq!(
            probe.spawns.load(Ordering::SeqCst),
            1,
            "the warm session survives the poisoning"
        );
        assert_eq!(pool.vivas(), 1);
    }

    // ── opt-in per process ─────────────────────────────────────────────────

    #[test]
    fn a_process_that_did_not_opt_in_never_spawns() {
        let tree = TempTree::new("optin");
        let probe = Arc::new(Probe::default());
        let pool = pool(&probe, 5000);
        let edicao = Edicao {
            path: "<string>",
            content: BAD,
            disk: None,
            overlays: &[],
        };
        for i in 0..3 {
            let r = checar_com(&pool, false, &cfg(), Instant::now(), &tree.root, &edicao);
            assert_eq!(r.verdict, Verdict::Unknown, "call #{i}: {r:?}");
            assert_eq!(
                r.motivo.as_deref(),
                Some("camada 2 desligada neste processo (só o daemon mantém pyright)")
            );
        }
        assert_eq!(
            probe.attempts.load(Ordering::SeqCst),
            0,
            "not even a spawn attempt"
        );
        assert_eq!(probe.spawns.load(Ordering::SeqCst), 0);
        assert_eq!(pool.slots_len(), 0, "nothing registered either");

        // control: the very same pool and call, opted in, does work
        let r = checar_com(&pool, true, &cfg(), Instant::now(), &tree.root, &edicao);
        assert_eq!(r.verdict, Verdict::Deny, "{r:?}");
        assert_eq!(probe.spawns.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn opting_in_is_idempotent() {
        habilitar_neste_processo();
        habilitar_neste_processo();
        assert!(habilitado());
    }

    // ── public API (global pool, real discover) ────────────────────────────

    #[test]
    fn public_checar_without_a_venv_is_unknown_then_backs_off() {
        habilitar_neste_processo();
        let tree = TempTree::new("public");
        let first = checar(&tree.root, "<string>", BAD, None, &[]);
        assert_eq!(first.verdict, Verdict::Unknown, "{first:?}");
        assert!(
            first
                .motivo
                .as_deref()
                .is_some_and(|m| m.contains("pyright-langserver")),
            "{first:?}"
        );
        let second = checar(&tree.root, "<string>", BAD, None, &[]);
        assert!(
            second
                .motivo
                .as_deref()
                .is_some_and(|m| m.contains("backoff")),
            "second call must hit the backoff, not retry discovery: {second:?}"
        );
        assert_eq!(reap_idle(Instant::now()), 0);
    }

    // ── generalisation: several verifiers, background warm-up, per-verifier caps ──

    use crate::servidor::Verificado;
    use std::sync::atomic::AtomicU64;

    /// A session without a process: answers what the test scripts and counts what it is asked.
    struct SessaoFalsa {
        verificacoes: Arc<AtomicUsize>,
        saudavel: bool,
        verdict: Verdict,
        aquecer_ms: u64,
    }

    impl Sessao for SessaoFalsa {
        fn verificar(&mut self, _: &Edicao<'_>, _: Option<Duration>) -> Verificado {
            self.verificacoes.fetch_add(1, Ordering::SeqCst);
            let resultado = if self.verdict == Verdict::Unknown {
                L2Resultado::unknown("sobre o arquivo", 0)
            } else {
                L2Resultado::from_achados(Vec::new(), 0)
            };
            Verificado {
                resultado,
                saudavel: self.saudavel,
            }
        }

        fn aquecer(&mut self, _: &Edicao<'_>, _: Duration) -> Result<(), String> {
            std::thread::sleep(Duration::from_millis(self.aquecer_ms));
            Ok(())
        }
    }

    /// An [`Abridor`] that counts what it is asked, with scripted slowness and failures.
    #[derive(Default)]
    struct AbridorFalso {
        pre_checagens: AtomicUsize,
        aberturas: AtomicUsize,
        verificacoes: Arc<AtomicUsize>,
        abrir_ms: AtomicU64,
        aquecer_ms: AtomicU64,
        pre_checar_falha: Mutex<Option<String>>,
        abrir_falha: Mutex<Option<String>>,
        insaudavel: AtomicBool,
        sobre_o_arquivo: AtomicBool,
        panica: AtomicBool,
    }

    impl Abridor for AbridorFalso {
        fn pre_checar(&self, _: &Alvo<'_>) -> Result<(), String> {
            self.pre_checagens.fetch_add(1, Ordering::SeqCst);
            lock(&self.pre_checar_falha).clone().map_or(Ok(()), Err)
        }

        fn abrir(&self, _: &Alvo<'_>) -> Result<Box<dyn Sessao>, String> {
            self.aberturas.fetch_add(1, Ordering::SeqCst);
            assert!(
                !self.panica.load(Ordering::SeqCst),
                "abridor de teste em pânico"
            );
            std::thread::sleep(Duration::from_millis(self.abrir_ms.load(Ordering::SeqCst)));
            if let Some(motivo) = lock(&self.abrir_falha).clone() {
                return Err(motivo);
            }
            Ok(Box::new(SessaoFalsa {
                verificacoes: Arc::clone(&self.verificacoes),
                saudavel: !self.insaudavel.load(Ordering::SeqCst),
                verdict: if self.sobre_o_arquivo.load(Ordering::SeqCst) {
                    Verdict::Unknown
                } else {
                    Verdict::Allow
                },
                aquecer_ms: self.aquecer_ms.load(Ordering::SeqCst),
            }))
        }
    }

    fn pool_falso() -> (Arc<AbridorFalso>, Pool) {
        let abridor = Arc::new(AbridorFalso::default());
        let spawner: Spawner = Arc::clone(&abridor) as Spawner;
        (abridor, Pool::com_spawner(spawner))
    }

    fn checar_em_falso(
        pool: &Pool,
        cfg: &Config,
        now: Instant,
        servidor: Servidor,
        root: &Path,
        path: &str,
    ) -> L2Resultado {
        let edicao = Edicao {
            path,
            content: "x\n",
            disk: None,
            overlays: &[],
        };
        pool.checar_servidor(cfg, now, servidor, root, &edicao)
    }

    /// Polls `checar` until it stops answering "aquecendo" (the warm-up thread installs the session).
    fn ate_aquecer(pool: &Pool, servidor: Servidor, root: &Path, path: &str) -> L2Resultado {
        let limite = Instant::now() + Duration::from_secs(10);
        loop {
            let r = checar_em_falso(pool, &cfg(), Instant::now(), servidor, root, path);
            let aquecendo = r.motivo.as_deref().is_some_and(|m| m.contains("aquecendo"));
            if !aquecendo || Instant::now() > limite {
                return r;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn slow_verifiers_warm_up_in_the_background_and_the_hook_never_waits() {
        let tree = TempTree::new("warm");
        let (abridor, pool) = pool_falso();
        abridor.abrir_ms.store(500, Ordering::SeqCst);
        let t0 = Instant::now();
        let primeira = checar_em_falso(
            &pool,
            &cfg(),
            t0,
            Servidor::RustAnalyzer,
            &tree.root,
            "src/lib.rs",
        );
        assert!(
            t0.elapsed() < Duration::from_millis(250),
            "the first call must return at once, not wait for the 500 ms start: {:?}",
            t0.elapsed()
        );
        assert_eq!(primeira.verdict, Verdict::Unknown);
        assert!(
            primeira
                .motivo
                .as_deref()
                .is_some_and(|m| m.contains("rust-analyzer") && m.contains("aquecendo")),
            "{primeira:?}"
        );
        let durante = checar_em_falso(
            &pool,
            &cfg(),
            Instant::now(),
            Servidor::RustAnalyzer,
            &tree.root,
            "src/lib.rs",
        );
        assert!(
            durante
                .motivo
                .as_deref()
                .is_some_and(|m| m.contains("aquecendo")),
            "still warming: {durante:?}"
        );

        let pronta = ate_aquecer(&pool, Servidor::RustAnalyzer, &tree.root, "src/lib.rs");
        assert_eq!(pronta.verdict, Verdict::Allow, "{pronta:?}");
        assert_eq!(abridor.aberturas.load(Ordering::SeqCst), 1);
        assert_eq!(pool.vivas(), 1);
    }

    #[test]
    fn a_failed_warm_up_backs_off_and_reports_why() {
        let tree = TempTree::new("warmfail");
        let (abridor, pool) = pool_falso();
        *lock(&abridor.abrir_falha) = Some("sem rust-analyzer no toolchain".to_string());
        let primeira = checar_em_falso(
            &pool,
            &cfg(),
            Instant::now(),
            Servidor::RustAnalyzer,
            &tree.root,
            "src/lib.rs",
        );
        assert!(
            primeira
                .motivo
                .as_deref()
                .is_some_and(|m| m.contains("aquecendo")),
            "{primeira:?}"
        );
        let depois = ate_aquecer(&pool, Servidor::RustAnalyzer, &tree.root, "src/lib.rs");
        assert_eq!(depois.verdict, Verdict::Unknown);
        let motivo = depois.motivo.expect("reason");
        assert!(
            motivo.contains("backoff") && motivo.contains("sem rust-analyzer no toolchain"),
            "{motivo}"
        );
        for _ in 0..5 {
            checar_em_falso(
                &pool,
                &cfg(),
                Instant::now(),
                Servidor::RustAnalyzer,
                &tree.root,
                "src/lib.rs",
            );
        }
        assert_eq!(
            abridor.aberturas.load(Ordering::SeqCst),
            1,
            "no start inside the backoff"
        );
    }

    #[test]
    fn a_panicking_warm_up_ends_in_a_backoff_not_in_a_slot_stuck_warming() {
        let tree = TempTree::new("warmpanic");
        let (abridor, pool) = pool_falso();
        abridor.panica.store(true, Ordering::SeqCst);
        let primeira = checar_em_falso(
            &pool,
            &cfg(),
            Instant::now(),
            Servidor::RustAnalyzer,
            &tree.root,
            "src/lib.rs",
        );
        assert!(
            primeira
                .motivo
                .as_deref()
                .is_some_and(|m| m.contains("aquecendo"))
        );
        let depois = ate_aquecer(&pool, Servidor::RustAnalyzer, &tree.root, "src/lib.rs");
        let motivo = depois.motivo.expect("reason");
        assert!(
            motivo.contains("backoff") && motivo.contains("pânico"),
            "the slot must not stay `aquecendo` for ever: {motivo}"
        );
    }

    #[test]
    fn a_failed_pre_check_reaches_the_hook_at_once_and_starts_nothing() {
        let tree = TempTree::new("precheck");
        let (abridor, pool) = pool_falso();
        *lock(&abridor.pre_checar_falha) =
            Some("nenhum Cargo.toml entre o arquivo e a raiz".to_string());
        let r = checar_em_falso(
            &pool,
            &cfg(),
            Instant::now(),
            Servidor::RustAnalyzer,
            &tree.root,
            "src/lib.rs",
        );
        assert_eq!(r.verdict, Verdict::Unknown);
        assert_eq!(
            r.motivo.as_deref(),
            Some("nenhum Cargo.toml entre o arquivo e a raiz")
        );
        assert_eq!(
            abridor.aberturas.load(Ordering::SeqCst),
            0,
            "no thread, no process"
        );
        let de_novo = checar_em_falso(
            &pool,
            &cfg(),
            Instant::now(),
            Servidor::RustAnalyzer,
            &tree.root,
            "src/lib.rs",
        );
        assert!(
            de_novo.motivo.is_some_and(|m| m.contains("backoff")),
            "remembered for 5 minutes"
        );
        assert_eq!(
            abridor.pre_checagens.load(Ordering::SeqCst),
            1,
            "not asked again inside the backoff"
        );
    }

    #[test]
    fn pyright_keeps_its_synchronous_start() {
        let tree = TempTree::new("pysync");
        let (abridor, pool) = pool_falso();
        let r = checar_em_falso(
            &pool,
            &cfg(),
            Instant::now(),
            Servidor::Pyright,
            &tree.root,
            "a.py",
        );
        assert_eq!(
            r.verdict,
            Verdict::Allow,
            "measured on the very first call: {r:?}"
        );
        assert_eq!(
            abridor.pre_checagens.load(Ordering::SeqCst),
            0,
            "pyright discovers inside its own start"
        );
    }

    #[test]
    fn a_warming_session_counts_against_the_cap_and_is_never_evicted() {
        let (a, b) = (TempTree::new("warmcap_a"), TempTree::new("warmcap_b"));
        let (abridor, pool) = pool_falso();
        abridor.abrir_ms.store(400, Ordering::SeqCst);
        // rust-analyzer's cap defaults to 1
        checar_em_falso(
            &pool,
            &cfg(),
            Instant::now(),
            Servidor::RustAnalyzer,
            &a.root,
            "src/lib.rs",
        );
        let r = checar_em_falso(
            &pool,
            &cfg(),
            Instant::now(),
            Servidor::RustAnalyzer,
            &b.root,
            "src/lib.rs",
        );
        assert_eq!(r.verdict, Verdict::Unknown);
        assert!(
            r.motivo.as_deref().is_some_and(|m| m.contains("teto")),
            "{r:?}"
        );
        ate_aquecer(&pool, Servidor::RustAnalyzer, &a.root, "src/lib.rs");
        assert_eq!(
            abridor.aberturas.load(Ordering::SeqCst),
            1,
            "the second root did not start a workspace"
        );
    }

    #[test]
    fn the_cap_is_per_verifier_so_one_language_does_not_evict_another() {
        let tree = TempTree::new("kindcap");
        let (_abridor, pool) = pool_falso();
        let one = Config::from_lookup(|k| (k == "TOURING_VGP_L2_MAX").then(|| "1".to_string()));
        let t0 = Instant::now();
        assert_eq!(
            checar_em_falso(&pool, &one, t0, Servidor::Pyright, &tree.root, "a.py").verdict,
            Verdict::Allow
        );
        checar_em_falso(&pool, &one, t0, Servidor::Tsserver, &tree.root, "a.ts");
        checar_em_falso(&pool, &one, t0, Servidor::DenoLsp, &tree.root, "a.ts");
        let _ = ate_aquecer(&pool, Servidor::Tsserver, &tree.root, "a.ts");
        let _ = ate_aquecer(&pool, Servidor::DenoLsp, &tree.root, "a.ts");
        assert_eq!(
            pool.vivas(),
            3,
            "pyright + tsserver + deno of the same root coexist under MAX=1"
        );
    }

    #[test]
    fn rust_analyzers_cap_has_its_own_knob_and_a_zero_becomes_one() {
        let c = Config::from_lookup(|k| (k == "TOURING_VGP_L2_MAX_RA").then(|| "3".to_string()));
        assert_eq!(
            (c.teto(Servidor::RustAnalyzer), c.teto(Servidor::Pyright)),
            (3, 2)
        );
        let zero = Config::from_lookup(|k| (k == "TOURING_VGP_L2_MAX_RA").then(|| "0".to_string()));
        assert_eq!(zero.teto(Servidor::RustAnalyzer), 1);
        let padrao = Config::from_lookup(|_| None);
        assert_eq!(
            padrao.teto(Servidor::RustAnalyzer),
            1,
            "a loaded workspace is heavy: one by default"
        );
        assert_eq!(padrao.aquecimento, Duration::from_secs(180));
    }

    #[test]
    fn an_unknown_about_the_file_keeps_the_warm_session_and_one_about_the_session_drops_it() {
        let tree = TempTree::new("health");
        let (abridor, pool) = pool_falso();
        abridor.sobre_o_arquivo.store(true, Ordering::SeqCst);
        let _ = checar_em_falso(
            &pool,
            &cfg(),
            Instant::now(),
            Servidor::Pyright,
            &tree.root,
            "a.py",
        );
        let r = checar_em_falso(
            &pool,
            &cfg(),
            Instant::now(),
            Servidor::Pyright,
            &tree.root,
            "a.py",
        );
        assert_eq!(r.verdict, Verdict::Unknown);
        assert_eq!(
            pool.vivas(),
            1,
            "a healthy session survives an unknown about the file"
        );
        assert_eq!(abridor.aberturas.load(Ordering::SeqCst), 1);

        abridor.insaudavel.store(true, Ordering::SeqCst);
        let tree2 = TempTree::new("health2");
        let _ = checar_em_falso(
            &pool,
            &cfg(),
            Instant::now(),
            Servidor::Pyright,
            &tree2.root,
            "a.py",
        );
        let _ = checar_em_falso(
            &pool,
            &cfg(),
            Instant::now(),
            Servidor::Pyright,
            &tree2.root,
            "a.py",
        );
        assert_eq!(
            pool.vivas(),
            1,
            "the unhealthy one was dropped (only tree 1's session remains)"
        );
    }

    #[test]
    fn a_single_verifier_can_be_switched_off_by_name() {
        let tree = TempTree::new("off");
        let (abridor, pool) = pool_falso();
        let off = Config::from_lookup(|k| {
            (k == "TOURING_VGP_L2_OFF").then(|| "rust-analyzer, Deno,lixo".to_string())
        });
        assert_eq!(
            off.off,
            vec![Servidor::RustAnalyzer, Servidor::DenoLsp],
            "garbage names are ignored"
        );
        let ra = checar_em_falso(
            &pool,
            &off,
            Instant::now(),
            Servidor::RustAnalyzer,
            &tree.root,
            "src/lib.rs",
        );
        assert_eq!(ra.verdict, Verdict::Unknown);
        assert!(
            ra.motivo
                .as_deref()
                .is_some_and(|m| m.contains("rust-analyzer") && m.contains("TOURING_VGP_L2_OFF")),
            "{ra:?}"
        );
        assert_eq!(abridor.pre_checagens.load(Ordering::SeqCst), 0);
        let py = checar_em_falso(
            &pool,
            &off,
            Instant::now(),
            Servidor::Pyright,
            &tree.root,
            "a.py",
        );
        assert_eq!(py.verdict, Verdict::Allow, "the others are untouched");
    }

    #[test]
    fn slots_are_keyed_by_verifier_and_by_the_package_that_provides_the_typescript() {
        let tree = TempTree::new("keys");
        for pacote in ["web", "api"] {
            let dir = tree.root.join(pacote).join("node_modules/typescript/lib");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("tsserver.js"), "").unwrap();
        }
        let (abridor, pool) = pool_falso();
        let t0 = Instant::now();
        for (servidor, path) in [
            (Servidor::Tsserver, "web/src/a.ts"),
            (Servidor::Tsserver, "api/src/b.ts"),
            (Servidor::Tsserver, "web/src/c.ts"),
            (Servidor::Pyright, "a.py"),
        ] {
            checar_em_falso(&pool, &cfg(), t0, servidor, &tree.root, path);
        }
        let _ = ate_aquecer(&pool, Servidor::Tsserver, &tree.root, "web/src/a.ts");
        let _ = ate_aquecer(&pool, Servidor::Tsserver, &tree.root, "api/src/b.ts");
        assert_eq!(
            abridor.aberturas.load(Ordering::SeqCst),
            3,
            "web and api have their own TypeScript (2 tsserver) + 1 pyright; web/c.ts reuses web's"
        );
    }

    #[test]
    fn without_opting_in_every_verifier_is_unknown_with_its_own_name() {
        let tree = TempTree::new("optin_all");
        let (abridor, pool) = pool_falso();
        let edicao = Edicao {
            path: "src/lib.rs",
            content: "x\n",
            disk: None,
            overlays: &[],
        };
        for servidor in Servidor::TODOS {
            let r = checar_com_servidor(
                &pool,
                false,
                &cfg(),
                Instant::now(),
                servidor,
                &tree.root,
                &edicao,
            );
            assert_eq!(r.verdict, Verdict::Unknown);
            assert_eq!(
                r.motivo,
                Some(format!(
                    "camada 2 desligada neste processo (só o daemon mantém {})",
                    servidor.nome()
                ))
            );
        }
        assert_eq!(
            abridor.pre_checagens.load(Ordering::SeqCst) + abridor.aberturas.load(Ordering::SeqCst),
            0
        );
    }

    #[test]
    fn l2_budget_env_pequeno_classifica_timeout() {
        assert_eq!(cfg().l2_budget, Some(Duration::from_millis(1000)));
        let cfg = Config::from_lookup(|name| {
            (name == crate::pyright::L2_BUDGET_ENV).then_some("1".into())
        });
        let pool = pool(&Arc::new(Probe::default()), 5000);
        let tree = TempTree::new("l2budget");
        let r = check(&pool, &cfg, Instant::now(), &tree.root, "# silencio\n");
        assert_eq!(r.verdict, Verdict::Unknown);
        assert!(
            r.motivo.as_deref().is_some_and(|m| m.contains("prazo")),
            "{r:?}"
        );
    }

    #[test]
    fn a_language_without_a_verifier_is_unknown_and_says_so() {
        let tree = TempTree::new("nolang");
        for lang in ["html", "css", "cobol"] {
            let r = checar_lang(lang, &tree.root, "a.x", "x\n", None, &[]);
            assert_eq!(r.verdict, Verdict::Unknown, "{lang}");
            assert!(
                r.motivo
                    .is_some_and(|m| m.contains("não há verificador") && m.contains(lang)),
                "{lang}"
            );
        }
    }
}
