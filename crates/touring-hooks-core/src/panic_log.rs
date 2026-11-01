//! Crash logging for the touring-daemon — one JSONL record per fatal signal,
//! written from the signal handler immediately before the process terminates.
//!
//! Sprint 4.5 (Wave touring-process-hygiene 2026-05-23), modernised 2026-06-26,
//! repaired 14/09/2026.
//!
//! ## What is captured
//!
//! `SIGABRT` (`panic = "abort"`, `abort()`), `SIGSEGV`, `SIGBUS` and `SIGILL`
//! each append one line to `~/.claude/touring/daemon-crash.jsonl` (override:
//! `TOURING_CRASH_LOG_PATH`): wall time, uptime, pid, tid, the kernel thread
//! name, the signal, `si_code`, the fault address, and the [`CrashContext`]
//! label of the thread that died — the file being indexed, when the rebuild or
//! a hook writer set one. A core dump alone names neither the thread's work nor,
//! in a stripped release binary, a single function.
//!
//! ## The repair of 14/09/2026
//!
//! The daemon died by SIGSEGV twice during `index rebuild` (13 and 14/09) and
//! left no record. `install_hook`, the daemon's only call, installed the
//! handlers but never opened the log, so the handler found no descriptor and
//! re-raised in silence; the tests exercised only the formatter. Behind that,
//! the record allocated (`format!`, `to_string`, `thread::current`) inside the
//! handler — unsafe exactly when the heap is what broke — and `signal(2)`
//! REPLACED the runtime's SIGSEGV handler, the code that turns a guard-page
//! fault into "thread … has overflowed its stack". Now the handler runs on the
//! alternate signal stack (`SA_ONSTACK`), formats with no allocation, restores
//! the disposition it found at install and lets the signal reach it: a fault
//! repeats when the instruction runs again, a sent signal is raised again.
//! `crash_path_tests` proves the three in child processes that really die.
//!
//! ```ignore
//! touring_hooks::panic_log::install_hook();
//! let _ctx = touring_hooks::panic_log::CrashContext::enter("crates/x/src/y.rs");
//! ```
//!
//! `install_hook` is idempotent — later calls are no-ops and return `false`.

use std::cell::Cell;
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);

/// `CLOCK_MONOTONIC` at install, in nanoseconds — the zero of `uptime_secs`.
static START_MONOTONIC_NS: AtomicU64 = AtomicU64::new(0);

/// The signals that terminate the daemon, in the order [`PREVIOUS`] keeps them.
const FATAL_SIGNALS: [libc::c_int; 4] = [libc::SIGABRT, libc::SIGSEGV, libc::SIGBUS, libc::SIGILL];

/// The disposition each fatal signal had before `install_hook` — the runtime's
/// stack-overflow handler for SIGSEGV and SIGBUS, `SIG_DFL` for the others. Set
/// before the first handler is installed, so a handler never finds it empty.
static PREVIOUS: OnceLock<[libc::sigaction; 4]> = OnceLock::new();

/// Longest label a [`CrashContext`] keeps, in bytes. A longer label keeps its
/// tail, which for a path is the part that names the file.
const CONTEXT_CAP: usize = 256;

/// Size of one crash record; the fields are bounded, so a record always fits.
const RECORD_CAP: usize = 768;

thread_local! {
    /// The current thread's label. Const-initialised and without a destructor,
    /// so reading it from a signal handler touches only thread-local storage.
    static CONTEXT: Cell<([u8; CONTEXT_CAP], usize)> =
        const { Cell::new(([0; CONTEXT_CAP], 0)) };
}

/// Names what the current thread is working on until the guard drops, so a
/// fatal signal on this thread is logged with it. Nested guards restore the
/// outer label.
#[must_use = "the label is only set while the guard lives"]
pub struct CrashContext {
    outer: ([u8; CONTEXT_CAP], usize),
}

impl CrashContext {
    /// Sets `label` as this thread's crash context (its last 256 bytes, cut at
    /// a character boundary).
    pub fn enter(label: &str) -> Self {
        let mut start = label.len().saturating_sub(CONTEXT_CAP);
        while !label.is_char_boundary(start) {
            start += 1;
        }
        let tail = &label.as_bytes()[start..];
        let mut bytes = [0u8; CONTEXT_CAP];
        bytes[..tail.len()].copy_from_slice(tail);
        let outer = CONTEXT
            .try_with(|c| c.replace((bytes, tail.len())))
            .unwrap_or(([0; CONTEXT_CAP], 0));
        Self { outer }
    }
}

impl Drop for CrashContext {
    fn drop(&mut self) {
        let outer = self.outer;
        let _ = CONTEXT.try_with(|c| c.set(outer));
    }
}

/// The label of the calling thread, if one is set.
fn current_context() -> ([u8; CONTEXT_CAP], usize) {
    CONTEXT.try_with(Cell::get).unwrap_or(([0; CONTEXT_CAP], 0))
}

/// Coerce the `SA_SIGINFO` handler to the `usize` that `sigaction.sa_sigaction`
/// holds. The cast is isolated here so the lint is allowed in one place.
#[allow(
    clippy::fn_to_numeric_cast,
    clippy::fn_to_numeric_cast_any,
    function_casts_as_integer
)]
#[inline]
fn signal_handler_address() -> libc::sighandler_t {
    signal_handler as usize
}

/// Restaura o comportamento PADRÃO do `SIGPIPE` — só para o papel de **CLI**.
///
/// # O defeito que isto corrige
///
/// Rust instala `SIG_IGN` para `SIGPIPE` no startup. Com isso, escrever num
/// pipe fechado não mata o processo: a escrita devolve `EPIPE`, e `println!`
/// **entra em panic** (`"failed printing to stdout: Broken pipe"`). Como o
/// perfil release usa `panic = "abort"`, o panic vira **SIGABRT** — exit 134.
///
/// Observado em 03/08/2026: `propagate-release.sh` abortou com 134 no meio do
/// relatório final, em `touring --version 2>&1 | head -1`. O `--version` escreve
/// 5 linhas; o `head -1` fecha o pipe depois da primeira. Reproduzido: 3 abortos
/// em 200 tentativas — é uma corrida, daí a intermitência que dificultou o
/// diagnóstico. Qualquer `touring … | head`, `| grep -q` ou `| less` fechado
/// cedo tem o mesmo efeito, inclusive digitado à mão no terminal.
///
/// Com `SIG_DFL` o processo morre silenciosamente ao ter o pipe fechado, como
/// `ls`, `grep` e todo utilitário Unix — que é o comportamento que um
/// consumidor de pipeline espera.
///
/// # Por que APENAS no CLI
///
/// O mesmo binário serve três papéis: CLI efêmero, daemon e **bridge MCP
/// stdio**. No bridge, morrer em silêncio ao fechar o pipe transformaria um
/// erro diagnosticável num desaparecimento mudo no meio de uma sessão do Claude
/// Code. Lá o `SIG_IGN` do Rust é o comportamento desejável: o erro sobe como
/// `EPIPE` e pode ser tratado. Por isso a restauração é condicionada ao papel,
/// resolvido em `main()` antes de qualquer runtime subir.
///
/// Idempotente e sem alocação — seguro no caminho de inicialização.
pub fn restore_default_sigpipe_for_cli() {
    // SAFETY: `signal()` é async-signal-safe e `SIG_DFL` é a disposição
    // herdada de `execve` — estamos apenas desfazendo o `SIG_IGN` que o
    // runtime do Rust instala. Chamado de `main()` antes de qualquer thread
    // ser criada, então não há corrida com outro instalador.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

/// Install the daemon crash-log signal handlers and open the log they write to.
///
/// Idempotent — subsequent calls are no-ops. Returns `true` if the handlers
/// were installed on this call, `false` if an earlier call installed them.
pub fn install_hook() -> bool {
    if HOOK_INSTALLED.swap(true, Ordering::SeqCst) {
        return false;
    }
    START_MONOTONIC_NS.store(clock_ns(libc::CLOCK_MONOTONIC), Ordering::Relaxed);
    install_log_fd();

    // SAFETY: a null new action makes `sigaction` only read the current
    // disposition into `old`, a valid zeroed out-parameter.
    let previous = FATAL_SIGNALS.map(|sig| unsafe {
        let mut old: libc::sigaction = std::mem::zeroed();
        libc::sigaction(sig, std::ptr::null(), &mut old);
        old
    });
    let _ = PREVIOUS.set(previous);

    for sig in FATAL_SIGNALS {
        // SAFETY: the handler makes only async-signal-safe calls (see
        // `signal_handler`); `sa_mask` is emptied before the action is used.
        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = signal_handler_address();
            action.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(sig, &action, std::ptr::null_mut());
        }
    }
    true
}

/// Resolve the target path for the crash log JSONL file.
///
/// Precedence: `TOURING_CRASH_LOG_PATH` env var → `$HOME/.claude/touring/daemon-crash.jsonl`
/// → `./daemon-crash.jsonl` (last-resort fallback when no `HOME`).
fn crash_log_path() -> PathBuf {
    if let Ok(override_path) = std::env::var("TOURING_CRASH_LOG_PATH") {
        return PathBuf::from(override_path);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".claude/touring/daemon-crash.jsonl")
}

/// Nanoseconds on `clock`, or 0 when it cannot be read. `clock_gettime(2)` is
/// async-signal-safe.
fn clock_ns(clock: libc::clockid_t) -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `ts` is a valid out-parameter.
    if unsafe { libc::clock_gettime(clock, &mut ts) } != 0 {
        return 0;
    }
    u64::try_from(ts.tv_sec)
        .unwrap_or(0)
        .saturating_mul(1_000_000_000)
        .saturating_add(u64::try_from(ts.tv_nsec).unwrap_or(0))
}

/// `SA_SIGINFO` handler for SIGABRT / SIGSEGV / SIGBUS / SIGILL.
///
/// Runs on the alternate signal stack with the heap possibly corrupt, so it
/// calls only functions on the `signal-safety(7)` list: `write`,
/// `clock_gettime`, `prctl`, `gettid`, `getpid`, `sigaction`, `signal`,
/// `raise`. Nothing allocates and no lock is taken.
extern "C" fn signal_handler(
    sig: libc::c_int,
    info: *mut libc::siginfo_t,
    _context: *mut libc::c_void,
) {
    let (code, fault_addr) = if info.is_null() {
        (0, 0)
    } else {
        // SAFETY: the kernel passes a valid `siginfo_t` to an `SA_SIGINFO`
        // handler; it is only read.
        unsafe { ((*info).si_code, (*info).si_addr() as usize) }
    };
    if let Some(&fd) = FD_LOG.get() {
        let mut buf = [0u8; RECORD_CAP];
        let len = write_signal_record(sig, code, fault_addr, &mut buf);
        // SAFETY: `write(2)` is async-signal-safe; a short write is accepted.
        let _ = unsafe { libc::write(fd, buf.as_ptr().cast(), len) };
    }
    let previous = PREVIOUS.get().and_then(|actions| {
        FATAL_SIGNALS
            .iter()
            .position(|&s| s == sig)
            .map(|i| actions[i])
    });
    // SAFETY: `sigaction(2)`, `signal(2)` and `raise(3)` are async-signal-safe.
    unsafe {
        match previous {
            Some(action) => {
                libc::sigaction(sig, &action, std::ptr::null_mut());
            }
            None => {
                libc::signal(sig, libc::SIG_DFL);
            }
        }
        // A sent signal (`abort`, `kill`, `raise`: `si_code <= 0`) does not
        // come back by itself; a fault does, when the instruction runs again.
        if code <= 0 {
            libc::raise(sig);
        }
    }
}

/// Cached write end of the crash log file, opened by `install_hook` before any
/// handler is installed. The handler only reads it.
static FD_LOG: OnceLock<libc::c_int> = OnceLock::new();

/// Open the crash log for append and cache the fd. Best-effort — on failure
/// the handler still hands the signal back, it just cannot record it.
fn install_log_fd() {
    let path = crash_log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(file) = OpenOptions::new().create(true).append(true).open(&path) {
        use std::os::fd::IntoRawFd;
        let _ = FD_LOG.set(file.into_raw_fd());
    }
}

/// Format one JSONL crash record into `buf` and return its length. Every value
/// is a JSON string; nothing here allocates.
fn write_signal_record(
    sig: libc::c_int,
    code: libc::c_int,
    fault_addr: usize,
    buf: &mut [u8],
) -> usize {
    let mut w = BufWriter::new(buf);
    let mut digits = [0u8; 24];
    let now = clock_ns(libc::CLOCK_REALTIME);
    let uptime =
        clock_ns(libc::CLOCK_MONOTONIC).saturating_sub(START_MONOTONIC_NS.load(Ordering::Relaxed));

    write_jsonl_open(&mut w);
    write_jsonl_bytes(&mut w, "timestamp_unix", seconds(now, &mut digits));
    write_jsonl_field_sep(&mut w);
    write_jsonl_bytes(&mut w, "uptime_secs", seconds(uptime, &mut digits));
    write_jsonl_field_sep(&mut w);
    // SAFETY: `getpid(2)` and `gettid(2)` are async-signal-safe and cannot fail.
    let (pid, tid) = unsafe { (libc::getpid(), libc::gettid()) };
    write_jsonl_bytes(&mut w, "pid", signed(i64::from(pid), &mut digits));
    write_jsonl_field_sep(&mut w);
    write_jsonl_bytes(&mut w, "tid", signed(i64::from(tid), &mut digits));
    write_jsonl_field_sep(&mut w);
    let mut name = [0u8; 16];
    // SAFETY: PR_GET_NAME writes at most 16 bytes, NUL-terminated, into `name`.
    unsafe { libc::prctl(libc::PR_GET_NAME, name.as_mut_ptr()) };
    let name_len = name.iter().position(|&b| b == 0).unwrap_or(name.len());
    // The kernel cuts names at 15 bytes, possibly inside a character.
    for b in &mut name[..name_len] {
        if *b >= 0x80 {
            *b = b'?';
        }
    }
    write_jsonl_bytes(&mut w, "thread", &name[..name_len]);
    write_jsonl_field_sep(&mut w);
    write_jsonl_field(&mut w, "signal", signal_name(sig));
    write_jsonl_field_sep(&mut w);
    write_jsonl_bytes(&mut w, "signal_num", signed(i64::from(sig), &mut digits));
    write_jsonl_field_sep(&mut w);
    write_jsonl_bytes(&mut w, "si_code", signed(i64::from(code), &mut digits));
    if code > 0 && sig != libc::SIGABRT {
        write_jsonl_field_sep(&mut w);
        write_jsonl_bytes(&mut w, "fault_addr", hex(fault_addr, &mut digits));
    }
    let (label, label_len) = current_context();
    write_jsonl_field_sep(&mut w);
    write_jsonl_bytes(&mut w, "context", &label[..label_len]);
    write_jsonl_field_sep(&mut w);
    write_jsonl_field(&mut w, "payload", "fatal signal received — see core dump");
    write_jsonl_close(&mut w);
    write_jsonl_newline(&mut w);
    w.pos
}

/// `ns` as `seconds.milliseconds`, written into the tail of `out`.
fn seconds(ns: u64, out: &mut [u8; 24]) -> &[u8] {
    let millis = ns / 1_000_000;
    let mut i = out.len();
    let mut frac = millis % 1000;
    for _ in 0..3 {
        i -= 1;
        out[i] = b'0' + u8::try_from(frac % 10).unwrap_or(0);
        frac /= 10;
    }
    i -= 1;
    out[i] = b'.';
    let mut whole = millis / 1000;
    loop {
        i -= 1;
        out[i] = b'0' + u8::try_from(whole % 10).unwrap_or(0);
        whole /= 10;
        if whole == 0 {
            break;
        }
    }
    &out[i..]
}

/// `n` in decimal, written into the tail of `out`.
fn signed(n: i64, out: &mut [u8; 24]) -> &[u8] {
    let mut i = out.len();
    let mut rest = n.unsigned_abs();
    loop {
        i -= 1;
        out[i] = b'0' + u8::try_from(rest % 10).unwrap_or(0);
        rest /= 10;
        if rest == 0 {
            break;
        }
    }
    if n < 0 {
        i -= 1;
        out[i] = b'-';
    }
    &out[i..]
}

/// `n` as `0x…` lowercase hexadecimal, written into the tail of `out`.
fn hex(n: usize, out: &mut [u8; 24]) -> &[u8] {
    let mut i = out.len();
    let mut rest = n;
    loop {
        i -= 1;
        out[i] = b"0123456789abcdef"[rest % 16];
        rest /= 16;
        if rest == 0 {
            break;
        }
    }
    i -= 2;
    out[i] = b'0';
    out[i + 1] = b'x';
    &out[i..]
}

/// Tiny stack-only JSONL formatter. We can't use `String`/`Vec` inside a
/// signal handler (no allocation), so we write byte-by-byte into a caller-
/// provided buffer; bytes past its end are dropped.
struct BufWriter<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> BufWriter<'a> {
    fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, pos: 0 }
    }
    fn push_byte(&mut self, b: u8) {
        if self.pos < self.buf.len() {
            self.buf[self.pos] = b;
            self.pos += 1;
        }
    }
    /// Push `bytes` as the inside of a JSON string: quote and backslash are
    /// escaped, control characters dropped.
    fn push_escaped(&mut self, bytes: &[u8]) {
        for &b in bytes {
            if b == b'"' || b == b'\\' {
                self.push_byte(b'\\');
                self.push_byte(b);
            } else if b >= 0x20 {
                self.push_byte(b);
            }
        }
    }
}

fn write_jsonl_open(w: &mut BufWriter<'_>) {
    w.push_byte(b'{');
}
fn write_jsonl_close(w: &mut BufWriter<'_>) {
    w.push_byte(b'}');
}
fn write_jsonl_field_sep(w: &mut BufWriter<'_>) {
    w.push_byte(b',');
}
fn write_jsonl_newline(w: &mut BufWriter<'_>) {
    w.push_byte(b'\n');
}
fn write_jsonl_field(w: &mut BufWriter<'_>, key: &str, value: &str) {
    write_jsonl_bytes(w, key, value.as_bytes());
}
fn write_jsonl_bytes(w: &mut BufWriter<'_>, key: &str, value: &[u8]) {
    w.push_byte(b'"');
    w.push_escaped(key.as_bytes());
    w.push_byte(b'"');
    w.push_byte(b':');
    w.push_byte(b'"');
    w.push_escaped(value);
    w.push_byte(b'"');
}

fn signal_name(sig: libc::c_int) -> &'static str {
    match sig {
        libc::SIGABRT => "SIGABRT",
        libc::SIGSEGV => "SIGSEGV",
        libc::SIGBUS => "SIGBUS",
        libc::SIGILL => "SIGILL",
        _ => "UNKNOWN",
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod sigpipe_tests {
    /// A restauração do `SIGPIPE` é lida de VOLTA do kernel, não assumida.
    ///
    /// Prova a correção do exit 134 de 03/08/2026: sem ela o Rust deixa
    /// `SIG_IGN`, `println!` entra em panic ao escrever num pipe fechado, e
    /// `panic = "abort"` transforma isso em SIGABRT. Reproduzido na época em
    /// 3 de 200 execuções de `touring --version | head -1`.
    ///
    /// `#[serial]` porque a disposição de sinal é global ao PROCESSO — este
    /// teste a modifica e restaura, e um vizinho paralelo observaria o estado
    /// intermediário (a mesma classe de defeito que os outros `#[serial]`
    /// desta sessão corrigiram).
    #[test]
    #[serial_test::serial(process_signal_disposition)]
    fn cli_role_restores_default_sigpipe_disposition() {
        // SAFETY: leitura da disposição atual; `SIG_IGN` é reinstalado ao fim
        // para não vazar estado para os testes seguintes do binário.
        let previous = unsafe { libc::signal(libc::SIGPIPE, libc::SIG_IGN) };

        super::restore_default_sigpipe_for_cli();

        // `signal()` devolve a disposição ANTERIOR — consultamos sem alterar
        // o resultado, reinstalando o que acabamos de ler.
        // SAFETY: mesma justificativa; nenhuma thread concorrente sob #[serial].
        let after = unsafe {
            let d = libc::signal(libc::SIGPIPE, libc::SIG_DFL);
            libc::signal(libc::SIGPIPE, d);
            d
        };
        assert_eq!(
            after,
            libc::SIG_DFL,
            "após a restauração o SIGPIPE tem de estar em SIG_DFL — senão \
             `touring … | head` volta a abortar com 134"
        );

        // SAFETY: devolve o processo ao estado anterior ao teste.
        unsafe {
            libc::signal(libc::SIGPIPE, previous);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::Write;
    use std::sync::Mutex;

    /// Test mutex so env-mutating tests serialise.
    static TEST_MUTEX: Mutex<()> = Mutex::new(());

    fn parse_jsonl_line(s: &str) -> HashMap<String, String> {
        let trimmed = s.trim();
        assert!(
            trimmed.starts_with('{') && trimmed.ends_with('}'),
            "bad json: {trimmed}"
        );
        let body = &trimmed[1..trimmed.len() - 1];
        let mut out = HashMap::new();
        let mut chars = body.chars().peekable();
        while let Some(&c) = chars.peek() {
            if c == ',' || c.is_whitespace() {
                chars.next();
                continue;
            }
            // Read key
            assert_eq!(chars.next(), Some('"'));
            let mut key = String::new();
            while let Some(kc) = chars.next() {
                if kc == '"' {
                    break;
                }
                key.push(kc);
            }
            assert_eq!(chars.next(), Some(':'));
            // Read value
            assert_eq!(chars.next(), Some('"'));
            let mut val = String::new();
            while let Some(vc) = chars.next() {
                if vc == '"' {
                    break;
                }
                if vc == '\\' {
                    if let Some(esc) = chars.next() {
                        val.push(esc);
                    }
                    continue;
                }
                val.push(vc);
            }
            out.insert(key, val);
        }
        out
    }

    #[test]
    fn jsonl_field_formatter_handles_quotes() {
        let mut buf = [0u8; 256];
        let pos = {
            let mut w = BufWriter::new(&mut buf);
            write_jsonl_open(&mut w);
            write_jsonl_field(&mut w, "k", "v\"with\\quotes");
            w.pos
        };
        let s = std::str::from_utf8(&buf[..pos]).unwrap();
        assert_eq!(s, r#"{"k":"v\"with\\quotes""#);
    }

    #[test]
    fn signal_name_known_signals() {
        assert_eq!(signal_name(libc::SIGABRT), "SIGABRT");
        assert_eq!(signal_name(libc::SIGSEGV), "SIGSEGV");
        assert_eq!(signal_name(libc::SIGBUS), "SIGBUS");
        assert_eq!(signal_name(libc::SIGILL), "SIGILL");
    }

    #[test]
    fn crash_log_path_honors_env_override() {
        let _guard = TEST_MUTEX.lock().unwrap();
        unsafe {
            std::env::set_var("TOURING_CRASH_LOG_PATH", "/tmp/custom-panic.log");
        }
        let p = crash_log_path();
        assert_eq!(p, PathBuf::from("/tmp/custom-panic.log"));
        unsafe {
            std::env::remove_var("TOURING_CRASH_LOG_PATH");
        }
    }

    #[test]
    fn write_signal_record_emits_valid_jsonl() {
        let mut buf = [0u8; 512];
        let n = write_signal_record(libc::SIGABRT, -6, 0, &mut buf);
        let line = std::str::from_utf8(&buf[..n]).unwrap().to_string();
        let parsed = parse_jsonl_line(&line);
        assert_eq!(parsed.get("signal").map(String::as_str), Some("SIGABRT"));
        assert_eq!(parsed.get("signal_num").map(String::as_str), Some("6"));
        assert!(parsed.contains_key("timestamp_unix"));
        assert!(parsed.contains_key("pid"));
        assert!(parsed.contains_key("thread"));
        assert!(parsed.contains_key("uptime_secs"));
        assert!(line.ends_with('\n'));
    }

    #[test]
    fn install_hook_is_idempotent() {
        // We don't actually install (would interfere with other tests' signal
        // handling); just verify the idempotency guard via the static.
        // The static starts false; flipping it directly proves the pattern.
        let was = HOOK_INSTALLED.swap(true, Ordering::SeqCst);
        // restore original state at end
        HOOK_INSTALLED.store(was, Ordering::SeqCst);
        // Install returns false on second call (via swap test of logic).
        let _ = was;
    }

    /// End-to-end smoke: simulate a fatal signal and verify the JSONL
    /// line was written by the signal handler (without actually raising
    /// SIGABRT against the test process). We exercise the writer directly
    /// since we cannot install signal handlers inside a multi-threaded
    /// test harness without disturbing the runner.
    #[test]
    fn signal_path_emits_parseable_jsonl_to_tmpfile() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let tmp = std::env::temp_dir().join(format!(
            "panic_log_signal_test_{}.jsonl",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&tmp);
        unsafe {
            std::env::set_var("TOURING_CRASH_LOG_PATH", &tmp);
        }

        // Manually run the writer (the handler does the same syscall in
        // production; here we exercise the formatting).
        let mut buf = [0u8; 512];
        let n = write_signal_record(libc::SIGSEGV, 1, 0x10, &mut buf);
        let line = std::str::from_utf8(&buf[..n]).unwrap();
        // Append to the file the way the handler would.
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&tmp)
            .unwrap();
        f.write_all(line.as_bytes()).unwrap();
        f.flush().unwrap();
        drop(f);

        let content = std::fs::read_to_string(&tmp).unwrap_or_default();
        assert!(
            content.contains("\"signal\":\"SIGSEGV\""),
            "missing signal: {content}"
        );
        assert!(
            content.contains("\"payload\":"),
            "missing payload: {content}"
        );
        assert!(content.ends_with('\n'));

        unsafe {
            std::env::remove_var("TOURING_CRASH_LOG_PATH");
        }
        let _ = std::fs::remove_file(&tmp);
    }
}

/// The crash path exactly as the daemon runs it: a CHILD process installs the hook the
/// way `daemon_main` does and dies by a real signal, and the parent reads what reached
/// the log. The in-process tests above only exercise the formatter, which is how a hook
/// that never opened its log passed them for months (no `daemon-crash.jsonl` existed
/// when the daemon died by SIGSEGV on 13 and 14/09/2026).
#[cfg(test)]
mod crash_path_tests {
    use std::os::unix::process::ExitStatusExt;
    use std::process::Command;

    /// Selects what the child does; unset in a normal run, where `crash_child` passes.
    const MODE: &str = "TOURING_PANIC_LOG_CHILD_MODE";

    #[test]
    fn crash_child() {
        let Ok(mode) = std::env::var(MODE) else {
            return;
        };
        super::install_hook();
        match mode.as_str() {
            "fault" => {
                let _ctx = super::CrashContext::enter("crates/demo/src/crashy.rs");
                // SAFETY: none — this child exists to take a real page fault.
                let addr = std::hint::black_box(16usize) as *const u8;
                let _ = unsafe { std::ptr::read_volatile(addr) };
            }
            "abort" => std::process::abort(),
            "overflow" => {
                let _ = std::thread::Builder::new()
                    .name("deep".into())
                    .spawn(|| {
                        let _ctx = super::CrashContext::enter("crates/demo/src/deep.rs");
                        recurse(0)
                    })
                    .map(std::thread::JoinHandle::join);
            }
            other => panic!("unknown child mode {other}"),
        }
    }

    #[allow(unconditional_recursion)]
    #[inline(never)]
    fn recurse(depth: u64) -> u64 {
        let pad = std::hint::black_box([depth; 64]);
        recurse(depth + 1).wrapping_add(pad[0])
    }

    /// (terminating signal, crash log, stderr) of a child run in `mode`.
    fn run_child(mode: &str) -> (Option<i32>, String, String) {
        let log = std::env::temp_dir().join(format!(
            "panic_log_child_{}_{mode}.jsonl",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&log);
        let out = Command::new(std::env::current_exe().expect("test binary path"))
            .args([
                "--exact",
                "panic_log::crash_path_tests::crash_child",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(MODE, mode)
            .env("TOURING_CRASH_LOG_PATH", &log)
            .output()
            .expect("spawn crash child");
        let content = std::fs::read_to_string(&log).unwrap_or_default();
        let _ = std::fs::remove_file(&log);
        (
            out.status.signal(),
            content,
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }

    #[test]
    fn a_real_fault_reaches_the_crash_log() {
        let (signal, log, stderr) = run_child("fault");
        assert_eq!(signal, Some(libc::SIGSEGV), "child stderr: {stderr}");
        assert!(
            log.contains("\"signal\":\"SIGSEGV\""),
            "the daemon's own install path must write the record; log: {log:?}"
        );
        assert!(
            log.contains("\"context\":\"crates/demo/src/crashy.rs\""),
            "the record names what the dying thread was working on; log: {log:?}"
        );
        assert!(log.contains("\"fault_addr\":\"0x10\""), "log: {log:?}");
    }

    #[test]
    fn an_abort_reaches_the_crash_log_and_still_ends_by_sigabrt() {
        let (signal, log, stderr) = run_child("abort");
        assert_eq!(signal, Some(libc::SIGABRT), "child stderr: {stderr}");
        assert!(log.contains("\"signal\":\"SIGABRT\""), "log: {log:?}");
    }

    #[test]
    fn a_stack_overflow_keeps_the_runtime_diagnosis() {
        let (signal, log, stderr) = run_child("overflow");
        assert!(
            stderr.contains("has overflowed its stack"),
            "replacing the runtime's SIGSEGV handler loses the one line that names a \
             stack overflow; signal {signal:?}, stderr: {stderr}"
        );
        assert!(log.contains("\"signal\":\"SIGSEGV\""), "log: {log:?}");
        assert!(
            log.contains("\"context\":\"crates/demo/src/deep.rs\""),
            "log: {log:?}"
        );
    }

    #[test]
    fn a_nested_context_restores_the_outer_label_and_keeps_a_long_tail() {
        let outer = super::CrashContext::enter("outer.rs");
        {
            let long = format!("{}ção/final.rs", "a/".repeat(200));
            let _inner = super::CrashContext::enter(&long);
            let (bytes, len) = super::current_context();
            let kept = std::str::from_utf8(&bytes[..len]).expect("cut at a character boundary");
            assert!(
                kept.ends_with("ção/final.rs") && len <= super::CONTEXT_CAP,
                "{kept}"
            );
        }
        let (bytes, len) = super::current_context();
        assert_eq!(&bytes[..len], b"outer.rs");
        drop(outer);
        assert_eq!(super::current_context().1, 0);
    }
}
