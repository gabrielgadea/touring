# touring-vfs -- Architecture

> **Version**: v30.3.5 | **Updated**: 2026-04-30 | **Tests**: 10+ | **LOC**: ~500

## Overview

Virtual filesystem overlay that sits on top of the real filesystem. Provides in-memory file overlays (for editor edits), stable `FileId` identifiers that survive renames, absolute path types with validation, and an optional file-watcher for change notifications. The core abstraction is a two-layer model: in-memory overlay wins over real FS (upper wins).

## Key Types

| Type | File | Purpose |
|------|------|---------|
| `FileId` | file_id.rs | Opaque u32 identifier stable across file renames |
| `AbsPath` | abs_path.rs | Borrowed absolute path with validation |
| `AbsPathBuf` | abs_path.rs | Owned absolute path (like PathBuf but validated) |
| `Vfs` | vfs.rs | Core virtual filesystem: HashMap<FileId, VfsContent> + path lookups |
| `VfsContent` | vfs.rs | Enum: `Overlay(Bytes)` (in-memory) or `FileSystem` (lazy real-FS) |
| `VfsOverlay` | overlay.rs | Layered overlay stack (upper wins) |
| `FileSet` | file_set.rs | Query by path, FileId, or glob pattern |
| `VfsWatcher` | watcher.rs | File system change notifications via `notify` crate |

## Dependencies

| Crate | Why |
|-------|-----|
| `parking_lot` | RwLock for thread-safe interior mutability (no poisoning) |
| `bytes` | Efficient in-memory file content (`Bytes` copy-on-write) |
| `serde` | Serialization for FileId, AbsPath |
| `thiserror` | Error types: NotFound, AlreadyExists, NotAbsolute, Io |
| `filetime` | Set/compare file modification times for watcher |
| `walkdir` | Directory traversal for glob matching |
| `hashbrown` | Faster HashMap implementation |
| `notify` (opt) | File watcher -- debounced via `notify-debouncer-mini` |
| `notify-debouncer-mini` (opt) | Debouncing layer for watcher events |

## Feature Flags

| Feature | Default | Effect |
|---------|---------|--------|
| `watcher` | No | Enables `VfsWatcher` via `notify` + `notify-debouncer-mini` |

## Key Modules

| Module | Purpose |
|--------|---------|
| `file_id` | `FileId` -- opaque u32 with `INVALID` sentinel and `index()` accessor |
| `abs_path` | `AbsPath` / `AbsPathBuf` -- absolute-path validation (rejects relative) |
| `vfs` | `Vfs` struct with `add_file_system`, `addOverlay`, `read`, `write` |
| `overlay` | `VfsOverlay` layered overlay |
| `file_set` | `FileSet` glob queries over VFS |
| `watcher` | `VfsWatcher` (opt) -- translates `notify` events into VFS updates |

## Invariants

1. Every `FileId` maps to exactly one path in `path_to_id` and one path in `id_to_path` -- the bidirectional map must stay in sync.
2. `VfsContent::FileSystem` is lazy: content is not loaded from disk until `read()` is called.
3. In-memory overlay (`Overlay(Bytes)`) always wins over `FileSystem` for the same `FileId`.
4. `AbsPath::from_absolute()` rejects any path not starting with `/` -- no relative path ever enters the VFS.
5. The `watcher` feature is entirely optional; the rest of the crate has zero `notify` dependencies.

## Tests

10+ tests (unit tests in lib.rs + integration tests in `tests/vfs_tests.rs`). Run with:

```bash
cargo test -p touring-vfs
```