//! A long-lived registry of warm pyright sessions — one per project root — for the hook daemon.
//!
//! The daemon is one long process and the hook is short: a [`PyrightSession`] costs ~430 ms to start
//! and first analyse, then ~60 ms per check. Keeping it between edits is what makes layer 2 fit the
//! V3 budget; the price is that a forgotten session is a leaked process. So the pool is built around
//! four rules:
//!
//! * **lazy** — nothing is spawned until a root is first checked; a root whose language server cannot
//!   be found or started answers `unknown` and is not retried for 5 minutes (no spawn per edit);
//! * **bounded** — at most `TOURING_VGP_L2_MAX` (default 2) sessions live, least recently used
//!   first out; a session idle longer than `TOURING_VGP_L2_IDLE_S` (default 900) is closed;
//! * **failing closed on state, open on the hook** — a check that cannot be measured (the server
//!   died, the deadline passed) removes the session so the next use recreates it, and the hook gets
//!   an `unknown` with the reason, never an error, a panic or a hang;
//! * **switchable** — `TOURING_VGP_L2_DISABLED=1` answers `unknown` without spawning anything;
//! * **opt-in per process** — [`checar`] answers `unknown` until the process calls
//!   [`habilitar_neste_processo`]. Only the long-lived daemon (and the CLI that asks for
//!   `--camadas 1,2`) does; the ephemeral standalone `touring-hook` fallback would otherwise start a
//!   `pyright-langserver` per call and could leave it orphaned.
//!
//! ## Concurrency
//!
//! The registry lock is held only to look a root up or to count sessions; each root has its own
//! mutex, held for the whole check (the LSP exchange is sequential per session), so a slow root never
//! blocks another. Lock order is always *slot, then registry*, and the registry side only ever
//! `try_lock`s other slots, so it cannot deadlock; a slot that cannot be locked is *in use*, which
//! means "not idle" and "not evictable". A poisoned lock is recovered (`into_inner`), never propagated.
//!
//! Closing a session (`Drop`: polite shutdown, bounded wait, kill — up to ~0.8 s) happens on a
//! detached thread, so it is never on the hook's clock.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, TryLockError};
use std::thread;
use std::time::{Duration, Instant};

use crate::pyright::{L2Resultado, PyrightError, PyrightSession, Verdict};

const ENV_IDLE: &str = "TOURING_VGP_L2_IDLE_S";
const ENV_MAX: &str = "TOURING_VGP_L2_MAX";
const ENV_DISABLED: &str = "TOURING_VGP_L2_DISABLED";
const DEFAULT_IDLE_S: u64 = 900;
const DEFAULT_MAX: usize = 2;
/// How long a root whose server could not be started is left alone.
const DEFAULT_BACKOFF: Duration = Duration::from_secs(300);

/// Tunables, read from the environment on every call so a daemon restart is the only ceremony.
struct Config {
    idle: Duration,
    max: usize,
    disabled: bool,
    backoff: Duration,
}

impl Config {
    /// `get` maps a variable name to its value. Unparsable values fall back to the defaults, and a
    /// cap of 0 becomes 1 (a 0 cap would be a silent kill switch; the real one is `DISABLED`).
    fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Self {
        let idle = get(ENV_IDLE)
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(DEFAULT_IDLE_S);
        let max = get(ENV_MAX)
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(DEFAULT_MAX);
        let disabled = get(ENV_DISABLED)
            .is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true"));
        Self {
            idle: Duration::from_secs(idle),
            max: max.max(1),
            disabled,
            backoff: DEFAULT_BACKOFF,
        }
    }

    fn from_env() -> Self {
        Self::from_lookup(|name| std::env::var(name).ok())
    }
}

/// A live session and when it was last used.
struct Entrada {
    sessao: PyrightSession,
    ultimo_uso: Instant,
}

/// "Do not try to start this root's server again before `ate`", and why it failed.
struct Backoff {
    ate: Instant,
    motivo: String,
}

/// Everything known about one project root. Empty = nothing worth keeping.
#[derive(Default)]
struct Slot {
    entrada: Option<Entrada>,
    backoff: Option<Backoff>,
}

impl Slot {
    fn is_empty(&self) -> bool {
        self.entrada.is_none() && self.backoff.is_none()
    }
}

type Spawner = Box<dyn Fn(&Path) -> Result<PyrightSession, PyrightError> + Send + Sync>;

/// The registry. `spawner` is the only way a session is created, which is what lets the tests count
/// spawns and use a scripted server.
struct Pool {
    slots: Mutex<Slots>,
    spawner: Spawner,
}

/// One edit to check (everything but the project root).
struct Edicao<'a> {
    path: &'a str,
    content: &'a str,
    disk: Option<&'a str>,
    overlays: &'a [(String, String)],
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

/// The registry key: the canonical root, so `./p`, `/abs/p` and a symlink to it share one session.
fn chave(root: &Path) -> PathBuf {
    root.canonicalize()
        .or_else(|_| std::path::absolute(root))
        .unwrap_or_else(|_| root.to_path_buf())
}

/// Closes sessions off the caller's clock: `Drop` waits for the server to leave (and kills it after
/// half a second), which must never be added to a hook call.
fn dispose(sessions: Vec<PyrightSession>) {
    for sessao in sessions {
        let _ = thread::Builder::new()
            .name("lsp-dispose".into())
            .spawn(move || drop(sessao));
    }
}

type Slots = HashMap<PathBuf, Arc<Mutex<Slot>>>;

/// Live sessions of the other roots, and which of them may be evicted (least recently used first).
struct Census {
    live: usize,
    evictable: Vec<(Instant, PathBuf)>,
}

fn census(slots: &Slots, key: &Path) -> Census {
    let mut census = Census {
        live: 0,
        evictable: Vec::new(),
    };
    for (other, slot) in slots.iter().filter(|(other, _)| other.as_path() != key) {
        match try_lock_any(slot) {
            Some(guard) => {
                if let Some(entrada) = &guard.entrada {
                    census.live += 1;
                    census.evictable.push((entrada.ultimo_uso, other.clone()));
                }
            }
            None => census.live += 1, // in use: live, not evictable
        }
    }
    census.evictable.sort_by_key(|(used, _)| *used);
    census
}

/// Evicts from the front of `census.evictable` while the cap is still reached. Returns the live count
/// that remains and the sessions to close.
fn evict_oldest(slots: &Slots, census: Census, max: usize) -> (usize, Vec<PyrightSession>) {
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

fn take_session(slots: &Slots, key: &Path) -> Option<PyrightSession> {
    let mut guard = try_lock_any(slots.get(key)?)?;
    guard.entrada.take().map(|entrada| entrada.sessao)
}

/// Takes the session out of idle slots (and clears expired backoffs); `except` is never touched.
fn collect_stale(
    slots: &Slots,
    idle: Duration,
    now: Instant,
    except: Option<&Path>,
) -> Vec<PyrightSession> {
    let mut stale = Vec::new();
    for (_, slot) in slots
        .iter()
        .filter(|(key, _)| except != Some(key.as_path()))
    {
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

fn take_if_idle(slot: &mut Slot, idle: Duration, now: Instant) -> Option<PyrightSession> {
    let idle_for = now.saturating_duration_since(slot.entrada.as_ref()?.ultimo_uso);
    if idle_for > idle {
        slot.entrada.take().map(|entrada| entrada.sessao)
    } else {
        None
    }
}

/// Forgets slots that hold nothing. `strong_count == 1` under the registry lock means no thread holds
/// the slot (clones are only handed out under that lock), so removing it cannot orphan a session.
fn forget_empty(slots: &mut Slots, except: Option<&Path>) {
    slots.retain(|key, slot| {
        except == Some(key.as_path())
            || Arc::strong_count(slot) > 1
            || !try_lock_any(slot).is_some_and(|guard| guard.is_empty())
    });
}

impl Pool {
    fn new(spawner: Spawner) -> Self {
        Self {
            slots: Mutex::new(HashMap::new()),
            spawner,
        }
    }

    /// The root's slot, created empty on first sight. Held only briefly under the registry lock.
    fn slot_for(&self, key: &Path) -> Arc<Mutex<Slot>> {
        let mut slots = lock(&self.slots);
        Arc::clone(slots.entry(key.to_path_buf()).or_default())
    }

    fn checar(&self, cfg: &Config, now: Instant, root: &Path, edicao: &Edicao<'_>) -> L2Resultado {
        if cfg.disabled {
            return L2Resultado::unknown("camada 2 desligada (TOURING_VGP_L2_DISABLED=1)", 0);
        }
        let started = Instant::now();
        let key = chave(root);
        self.reap(cfg.idle, now, Some(&key));
        let slot = self.slot_for(&key);
        let mut guard = lock(&slot);
        let entrada = match self.ensure(cfg, now, &key, &mut guard) {
            Ok(entrada) => entrada,
            Err(mut unknown) => {
                unknown.elapsed_ms = elapsed_ms(started);
                return unknown;
            }
        };
        entrada.ultimo_uso = now;
        let result =
            entrada
                .sessao
                .check(edicao.path, edicao.content, edicao.disk, edicao.overlays);
        if result.verdict == Verdict::Unknown
            && let Some(broken) = guard.entrada.take()
        {
            // could not be measured (dead server, deadline): never reuse it — the next use recreates it
            dispose(vec![broken.sessao]);
        }
        result
    }

    /// Makes sure `slot` holds a live session, or says why it cannot (backoff, cap, spawn failure).
    fn ensure<'a>(
        &self,
        cfg: &Config,
        now: Instant,
        key: &Path,
        slot: &'a mut Slot,
    ) -> Result<&'a mut Entrada, L2Resultado> {
        if slot.entrada.is_none() {
            if let Some(backoff) = slot.backoff.as_ref().filter(|b| now < b.ate) {
                let wait = backoff.ate.saturating_duration_since(now).as_secs();
                return Err(L2Resultado::unknown(
                    format!("camada 2 em backoff por mais {wait} s: {}", backoff.motivo),
                    0,
                ));
            }
            slot.backoff = None;
            self.make_room(cfg.max, key)?;
            match (self.spawner)(key) {
                Ok(sessao) => {
                    slot.entrada = Some(Entrada {
                        sessao,
                        ultimo_uso: now,
                    });
                }
                Err(e) => {
                    let motivo = e.to_string();
                    slot.backoff = Some(Backoff {
                        ate: now + cfg.backoff,
                        motivo: motivo.clone(),
                    });
                    return Err(L2Resultado::unknown(motivo, 0));
                }
            }
        }
        slot.entrada.as_mut().ok_or_else(|| {
            L2Resultado::unknown("camada 2: estado interno inconsistente (sem sessão)", 0)
        })
    }

    /// Evicts least-recently-used sessions of OTHER roots until one more fits under `max`. A session
    /// that is in use (its slot is locked) counts as live but is never evicted; if that leaves no
    /// room, the caller gets an `unknown` instead of a session beyond the cap.
    fn make_room(&self, max: usize, key: &Path) -> Result<(), L2Resultado> {
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
    /// slots that hold nothing and that nobody else references. `except` (the root about to be used)
    /// is left alone. Returns how many sessions were closed.
    fn reap(&self, idle: Duration, now: Instant, except: Option<&Path>) -> usize {
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

fn elapsed_ms(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn global() -> &'static Pool {
    static POOL: OnceLock<Pool> = OnceLock::new();
    POOL.get_or_init(|| Pool::new(Box::new(PyrightSession::discover)))
}

/// Checks `content` as the file `path` of the project at `project_root` with that root's warm pyright
/// session, creating it on first use (see the module docs for the lifecycle rules).
///
/// Never panics and never returns an error: whatever prevents a measurement — layer disabled, a
/// process that did not opt in ([`habilitar_neste_processo`]), no language server in the venv,
/// backoff, session cap, dead server, deadline — is an
/// [`L2Resultado`] with [`Verdict::Unknown`] and the reason in `motivo`. `disk` and `overlays` are
/// those of [`PyrightSession::check`].
pub fn checar(
    project_root: &Path,
    path: &str,
    content: &str,
    disk: Option<&str>,
    overlays: &[(String, String)],
) -> L2Resultado {
    let edicao = Edicao {
        path,
        content,
        disk,
        overlays,
    };
    checar_com(
        global(),
        habilitado(),
        &Config::from_env(),
        Instant::now(),
        project_root,
        &edicao,
    )
}

/// Whether this process opted in to layer 2 (see [`habilitar_neste_processo`]).
static HABILITADO: AtomicBool = AtomicBool::new(false);

/// Why [`checar`] answers `unknown` in a process that never opted in.
const MOTIVO_NAO_HABILITADO: &str =
    "camada 2 desligada neste processo (só o daemon mantém pyright)";

/// Opts this process in to layer 2. Idempotent. Only the long-lived daemon and the CLI that asks for
/// `--camadas 1,2` call this: any other process (notably the ephemeral standalone `touring-hook`
/// fallback) would start a `pyright-langserver` per call and could leave it orphaned.
pub fn habilitar_neste_processo() {
    HABILITADO.store(true, Ordering::Relaxed);
}

fn habilitado() -> bool {
    HABILITADO.load(Ordering::Relaxed)
}

/// [`checar`] with the opt-in decision injected, so the guard is testable without touching the
/// process-wide flag.
fn checar_com(
    pool: &Pool,
    habilitado: bool,
    cfg: &Config,
    now: Instant,
    root: &Path,
    edicao: &Edicao<'_>,
) -> L2Resultado {
    if !habilitado {
        return L2Resultado::unknown(MOTIVO_NAO_HABILITADO, 0);
    }
    pool.checar(cfg, now, root, edicao)
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
    use crate::pyright::{Verdict, file_uri};
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
}
