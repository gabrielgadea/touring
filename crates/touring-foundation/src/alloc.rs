//! Global memory allocator using mimalloc.
//!
//! This module MUST be loaded before any other allocator usage.
//! Rust only allows one global allocator per program.
//!
//! Gated behind the `mimalloc-allocator` feature (on by default). Disable
//! when a downstream binary needs to own the global allocator — e.g. the
//! `touring` binary with `--features dhat-heap` for heap profiling.

#[cfg(feature = "mimalloc-allocator")]
use mimalloc::MiMalloc;

#[cfg(feature = "mimalloc-allocator")]
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;
