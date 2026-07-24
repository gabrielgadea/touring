## DESIGN RULE (2026-03-29): Centralize unsafe extern "C" FFI in lib.rs

### Problem
`extern "C" { fn getuid() -> u32; }` was duplicated in both `ipc.rs` and `circuit_breaker.rs`.
Each duplication is an independent unsafe declaration that must be audited separately.

### Fix Applied
Consolidated into a single `pub(crate)` function in `lib.rs`:

```rust
/// Returns the current process UID via libc getuid().
///
/// SAFETY: getuid() is always safe to call — it never fails, has no
/// preconditions, and is async-signal-safe per POSIX.
pub(crate) fn current_uid() -> u32 {
    extern "C" {
        fn getuid() -> u32;
    }
    unsafe { getuid() }
}
```

Callers updated to use `crate::current_uid()` — no more `unsafe` blocks at call sites.

### Rule
**Any `unsafe extern "C"` call duplicated across ≥2 modules MUST be consolidated into `pub(crate) fn` in `lib.rs`.**

Rationale:
- Unsafe code has smaller audit surface when centralized — 1 declaration to review vs N duplicates
- Call sites become safe Rust — no `unsafe` block needed
- SAFETY documentation lives in one place
- Reduces the chance of diverging declarations (e.g. different return types)

### Scope
This applies to the `touring-hooks` crate and any other crate with multiple FFI consumers.
