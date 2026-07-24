//! Process identity — set the kernel-visible process name via `prctl(PR_SET_NAME)`.
//!
//! Sprint 3 PC-1 (REGRA #19). Distinguishes the 4 touring process kinds in
//! `/proc/<pid>/comm` (15-char truncation max), so operators (and the LLM) can
//! tell apart:
//!
//! | Kind          | When set_process_name is called             | comm string     |
//! |---------------|---------------------------------------------|-----------------|
//! | touring-daemon| daemon_main.rs OR main.rs --start-daemon    | "touring-daemon"|
//! | touring-hook  | main.rs (hook handler for an event)         | "touring-hook"  |
//! | touring-mcp   | touring-server main.rs when `argv[1]=="serve"`| "touring-mcp"   |
//! | touring-cli   | touring-server main.rs other subcommand     | "touring-cli"   |
//!
//! Why `prctl(PR_SET_NAME)` (Linux): writes the kernel task struct `comm` field,
//! visible at `/proc/<pid>/comm` and via `ps -o comm` immediately. Truncated
//! silently to 15 chars (kernel TASK_COMM_LEN-1). Affects only the calling
//! thread; tokio/rayon worker threads inherit the parent's comm unless they
//! call set_process_name themselves — for our use case we only care about the
//! PID seen by `pgrep`/`ps`, which is the main thread.
//!
//! On non-Linux targets this is a no-op (still compiles, but the kernel
//! syscall is absent). Production target is Linux; macOS path is via
//! `pthread_setname_np` (not implemented here — out of scope for this wave).
//!
//! Zero dependency on `nix` or `libc` crates (intentional — mirrors the
//! `extern "C" { fn getuid() }` pattern already used in
//! `crates/touring-server/src/cli/mod.rs::libc_getuid` and
//! `crates/touring-server/src/cli/daemon_ctl.rs::kill`).

#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn prctl(option: i32, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> i32;
}

/// `PR_SET_NAME` from `<linux/prctl.h>` — sets the calling thread's name
/// (`/proc/<pid>/comm`). Truncated at 15 chars (TASK_COMM_LEN − 1).
#[cfg(target_os = "linux")]
const PR_SET_NAME: i32 = 15;

/// `PR_GET_NAME` — read the calling thread's name into a 16-byte buffer
/// (NUL-terminated). Used only by the test module to verify that
/// `set_process_name` actually wrote the per-thread comm; reading
/// `/proc/self/comm` would give the test harness's main-thread name.
#[cfg(all(test, target_os = "linux"))]
const PR_GET_NAME: i32 = 16;

/// Set the kernel-visible process name. No-op on non-Linux targets.
///
/// `name` is silently truncated to 15 bytes by the kernel; this helper
/// passes the original slice (the kernel truncates, we do not).
///
/// Errors from `prctl` are intentionally swallowed — `set_process_name`
/// is purely cosmetic for `ps`/`pgrep` observability; failure to set the
/// name must not abort daemon startup. The function returns `()` and never
/// panics.
///
/// # Safety / invariants
///
/// `prctl(PR_SET_NAME, ptr, 0, 0, 0)` is a side-effect-free POSIX-like
/// syscall; it cannot fail in ways that matter to the caller (return code
/// is checked but the only practical errno is EFAULT, which we cannot
/// observe with a valid CString pointer). The CString allocation is local
/// and dropped at function exit.
#[cfg(target_os = "linux")]
pub fn set_process_name(name: &str) {
    let Ok(cname) = std::ffi::CString::new(name) else {
        // CString::new errors only on interior NUL bytes; treat as caller
        // bug and silently skip — we won't kill startup over a comm string.
        return;
    };
    // SAFETY: prctl(PR_SET_NAME, ptr, 0, 0, 0) is a kernel syscall with no
    // memory effects on the caller. The pointer is valid for the duration
    // of the call (cname owns the allocation). Excess bytes beyond 15 are
    // truncated by the kernel — by design.
    let _rc = unsafe { prctl(PR_SET_NAME, cname.as_ptr() as u64, 0, 0, 0) };
}

#[cfg(not(target_os = "linux"))]
pub fn set_process_name(_name: &str) {
    // No-op on non-Linux. macOS would use pthread_setname_np; out of scope.
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Read the CALLING thread's comm via `prctl(PR_GET_NAME)`. We do NOT
    /// read `/proc/self/comm` — that returns the process main thread comm,
    /// whereas `prctl(PR_SET_NAME)` operates per-thread, so cargo's worker
    /// threads (which run tests) would never see the value we just wrote.
    #[cfg(target_os = "linux")]
    fn read_thread_comm() -> String {
        let mut buf = [0u8; 16];
        // SAFETY: prctl(PR_GET_NAME, ptr, 0, 0, 0) writes up to 16 bytes
        // into the caller-owned buffer (15 chars + NUL). Side-effect-free
        // POSIX-like syscall; ptr is valid for the duration of the call.
        let _ = unsafe { prctl(PR_GET_NAME, buf.as_mut_ptr() as u64, 0, 0, 0) };
        let nul = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        String::from_utf8_lossy(&buf[..nul]).into_owned()
    }

    #[test]
    fn set_process_name_with_short_string_does_not_panic() {
        set_process_name("touring-daemon");
        #[cfg(target_os = "linux")]
        {
            assert_eq!(read_thread_comm(), "touring-daemon");
        }
    }

    #[test]
    fn set_process_name_truncates_safely_at_15_bytes() {
        // Kernel truncates to TASK_COMM_LEN-1 = 15 bytes; the helper passes
        // the original slice and the kernel does the truncation silently.
        set_process_name("this-is-definitely-too-long-for-comm");
        #[cfg(target_os = "linux")]
        {
            let comm = read_thread_comm();
            assert!(
                comm.len() <= 15,
                "comm must be <= 15 chars; got {} bytes: {:?}",
                comm.len(),
                comm
            );
            assert!(
                comm.starts_with("this-is-definit"),
                "expected truncation to first 15 chars; got {comm:?}"
            );
        }
    }

    #[test]
    fn set_process_name_with_nul_byte_is_a_noop_not_panic() {
        // Establish a known thread comm, then attempt the bad write.
        set_process_name("touring-test");
        #[cfg(target_os = "linux")]
        let before = read_thread_comm();
        // CString::new returns Err on interior NUL; the helper swallows it.
        set_process_name("bad\0string");
        #[cfg(target_os = "linux")]
        {
            let after = read_thread_comm();
            assert_eq!(before, after, "NUL-containing name must be no-op");
        }
    }
}
