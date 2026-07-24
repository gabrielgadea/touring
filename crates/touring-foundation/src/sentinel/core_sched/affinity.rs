//! Linux `sched_setaffinity` wrapper for CPU affinity pinning.
//!
//! All `unsafe` blocks in the crate are confined to this module. Every unsafe
//! operation is documented with a safety invariant comment.
//!
//! This module is only compiled on Linux (`#[cfg(target_os = "linux")]` is
//! applied at the `core_sched` module level in `mod.rs`).

use crate::sentinel::error::AffinityError;

/// Pin a thread or process to the given CPU set.
///
/// After a successful call the kernel scheduler will only run the target
/// thread on CPUs present in `cpu_ids`.
///
/// # Parameters
///
/// - `tid`: Target thread ID. Pass `0` to affect the calling thread, or use
///   [`current_tid`] to obtain the current thread's TID explicitly.
/// - `cpu_ids`: Slice of logical CPU IDs to allow. Must not be empty, and
///   every ID must fit within the `cpu_set_t` bit-array (< 1024 on x86-64).
///
/// # Errors
///
/// - [`AffinityError::InvalidCoreList`] — `cpu_ids` is empty or contains an
///   out-of-range CPU ID.
/// - [`AffinityError::SetAffinityFailed`] — the `sched_setaffinity(2)` syscall
///   returned `-1`. Common causes:
///   - `EPERM` — not permitted; typical in containers / cgroups with a
///     restricted cpuset.
///   - `ESRCH` — `tid` refers to a thread/process that no longer exists.
///   - `EINVAL` — arguments invalid or the requested mask is empty after
///     intersecting with the cgroup cpuset.
pub fn pin_to_cpus(tid: libc::pid_t, cpu_ids: &[u32]) -> Result<(), AffinityError> {
    if cpu_ids.is_empty() {
        return Err(AffinityError::InvalidCoreList("empty cpu_ids".into()));
    }

    // Safety: `cpu_set_t` is a plain integer bit-array. Zeroing it via
    // `mem::zeroed()` is safe because all-zero is the defined "empty set"
    // representation on Linux (see cpu_set(3)).
    let mut cpuset: libc::cpu_set_t = unsafe { std::mem::zeroed() };

    // Safety: `CPU_ZERO` writes only into the provided `cpu_set_t` reference
    // that we own exclusively. No aliasing or lifetime issues.
    unsafe { libc::CPU_ZERO(&mut cpuset) };

    let max_cpu = 8 * std::mem::size_of::<libc::cpu_set_t>();
    for &cpu in cpu_ids {
        if cpu as usize >= max_cpu {
            return Err(AffinityError::InvalidCoreList(format!(
                "cpu {} out of range (max {})",
                cpu,
                max_cpu - 1
            )));
        }
        // Safety: `cpu` is bounds-checked above. `CPU_SET` sets a single bit
        // in the `cpu_set_t` bit-array; no UB possible given a valid index.
        unsafe { libc::CPU_SET(cpu as usize, &mut cpuset) };
    }

    // Safety: `sched_setaffinity(2)` is a standard POSIX-extension syscall on
    // Linux. We pass:
    //   - `tid`: a caller-supplied thread ID (0 = current thread is valid).
    //   - `size_of::<cpu_set_t>()`: the documented size argument.
    //   - `&cpuset`: a valid, initialised `cpu_set_t` whose lifetime exceeds
    //     the syscall duration.
    // The return value is checked immediately; errno is read only on failure.
    let rc =
        unsafe { libc::sched_setaffinity(tid, std::mem::size_of::<libc::cpu_set_t>(), &cpuset) };

    if rc != 0 {
        // Safety: `__errno_location()` returns a pointer to the calling
        // thread's errno cell. Reading it immediately after a failed syscall
        // is the canonical pattern; no other syscall has run in between.
        let errno = unsafe { *libc::__errno_location() };
        let description = match errno {
            libc::EPERM => "EPERM (permission denied — common in containers/cgroups)",
            libc::ESRCH => "ESRCH (no such process/thread)",
            libc::EINVAL => "EINVAL (invalid arguments or empty mask)",
            libc::EFAULT => "EFAULT (bad pointer)",
            _ => "unknown errno",
        };
        return Err(AffinityError::SetAffinityFailed {
            errno,
            description: description.to_string(),
        });
    }

    Ok(())
}

/// Return the current thread's kernel thread ID (TID) via `gettid(2)`.
///
/// This is the Linux-specific TID, not the POSIX `pthread_self()` handle.
/// Use the returned value as the `tid` argument to [`pin_to_cpus`].
pub fn current_tid() -> libc::pid_t {
    // Safety: `SYS_gettid` is a pure read syscall with no side-effects.
    // The return value always fits in `pid_t` (i32) on Linux.
    unsafe { libc::syscall(libc::SYS_gettid) as libc::pid_t }
}

// ─── Unit Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_tid_returns_positive() {
        let tid = current_tid();
        assert!(tid > 0, "expected positive TID, got {tid}");
    }

    #[test]
    fn pin_to_cpus_empty_returns_invalid_core_list() {
        let result = pin_to_cpus(0, &[]);
        assert!(
            matches!(result, Err(AffinityError::InvalidCoreList(_))),
            "expected InvalidCoreList, got {result:?}"
        );
    }

    #[test]
    fn pin_to_cpus_out_of_range_cpu_returns_invalid_core_list() {
        // cpu 99999 is well outside the cpu_set_t bit-array (max ~1023 on x86-64).
        let result = pin_to_cpus(0, &[99999]);
        assert!(
            matches!(result, Err(AffinityError::InvalidCoreList(_))),
            "expected InvalidCoreList for cpu 99999, got {result:?}"
        );
    }

    /// Live affinity test — skipped in environments that set SKIP_AFFINITY_TESTS=1
    /// (CI containers, cgroup-restricted sandboxes).
    ///
    /// Pins the current thread to cpu0 and cpu1. Expects either `Ok(())` or
    /// `AffinityError::SetAffinityFailed { errno: EPERM, .. }` — both are
    /// acceptable outcomes depending on cgroup cpuset restrictions.
    #[test]
    #[cfg(target_os = "linux")]
    fn pin_to_cpus_live_ok_or_eperm() {
        if std::env::var("SKIP_AFFINITY_TESTS").as_deref() == Ok("1") {
            return;
        }
        let tid = current_tid();
        let result = pin_to_cpus(tid, &[0, 1]);
        match result {
            Ok(()) => {} // success
            Err(AffinityError::SetAffinityFailed { errno, .. }) if errno == libc::EPERM => {
                // Acceptable: container/cgroup restriction.
            }
            Err(e) => panic!("unexpected affinity error: {e:?}"),
        }
    }
}
