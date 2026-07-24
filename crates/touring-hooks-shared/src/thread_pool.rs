//! Dedicated rayon thread pool for hook computation.
//!
//! Isolates CPU-bound AST analysis from the tokio async runtime
//! to prevent hook computation from starving socket I/O.

use std::sync::OnceLock;

static HOOK_POOL: OnceLock<rayon::ThreadPool> = OnceLock::new();

/// Get or create the dedicated hook thread pool.
///
/// Uses `min(4, num_cpus)` threads to avoid overwhelming the system.
pub fn hook_pool() -> &'static rayon::ThreadPool {
    HOOK_POOL.get_or_init(|| {
        let num_threads = num_cpus::get().clamp(1, 4);
        rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .thread_name(|i| format!("hook-worker-{i}"))
            .build()
            .expect("failed to create hook thread pool")
    })
}

/// Execute a closure on the dedicated hook pool.
///
/// Use this for CPU-bound signal computation (AST analysis,
/// blast radius, etc.) to avoid starving the daemon's tokio runtime.
pub fn with_hook_pool<F, R>(f: F) -> R
where
    F: FnOnce() -> R + Send,
    R: Send,
{
    hook_pool().install(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_pool_creation() {
        let pool = hook_pool();
        let threads = pool.current_num_threads();
        assert!(threads >= 1 && threads <= 4);
    }

    #[test]
    fn test_with_hook_pool_executes() {
        let result = with_hook_pool(|| 42);
        assert_eq!(result, 42);
    }

    #[test]
    fn test_with_hook_pool_rayon_join() {
        let (a, b) = with_hook_pool(|| rayon::join(|| 10, || 20));
        assert_eq!(a, 10);
        assert_eq!(b, 20);
    }
}
