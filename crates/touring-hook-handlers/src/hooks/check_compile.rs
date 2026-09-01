//! F-A1 (wave signal-layer-tier-ab, 01/09/2026) — the check-compile signal.
//!
//! P9 Verify-After was MEASURED at 17%: most edit bursts end without a
//! build/test (the G5 advisory said so twice in the very session that built
//! this). This module closes that loop by affordance, not persuasion (D8):
//! a post-edit/post-write on a Rust file spawns a DETACHED
//! `cargo check -p <crate> --message-format=short` whose verdict lands in a
//! state file, and the NEXT hook event injects the pending verdict as one
//! dense line (STR — the summary that decides, never the full output).
//!
//! Contract: the spawn path is milliseconds (fork + detach — the check runs
//! on its own clock, sharing the workspace target dir and waiting on cargo's
//! own lock when the user is building); the delivery read is <1ms; every arm
//! is fail-soft — a broken state file never touches the hook response.

use std::path::{Path, PathBuf};

/// Debounce window: a second check for the same crate within this many
/// seconds is skipped (edit bursts must not queue N redundant checks).
pub const DEBOUNCE_SECS: u64 = 30;

/// Walk up from `file` to the nearest `Cargo.toml` carrying a `[package]`
/// section and return the package name. `None` outside a crate (or for a
/// workspace-root manifest, which has no `[package]`).
pub fn resolve_crate_name(file: &Path) -> Option<String> {
    let mut dir = if file.is_dir() { file } else { file.parent()? };
    loop {
        let manifest = dir.join("Cargo.toml");
        if manifest.is_file()
            && let Ok(body) = std::fs::read_to_string(&manifest)
            && let Some(name) = package_name_from_manifest(&body)
        {
            return Some(name);
        }
        dir = dir.parent()?;
    }
}

/// Extract `name = "…"` from the `[package]` section of a manifest body.
/// Deliberately a line-scanner, not a TOML parser: the two fields it needs
/// are stable and a parser dependency here would outweigh them.
fn package_name_from_manifest(body: &str) -> Option<String> {
    let mut in_package = false;
    for line in body.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_package = t == "[package]";
            continue;
        }
        if in_package
            && let Some(rest) = t.strip_prefix("name")
        {
            let rest = rest.trim_start();
            if let Some(rest) = rest.strip_prefix('=') {
                let v = rest.trim().trim_matches('"');
                // Security: the name is interpolated into a shell line by
                // `spawn_check_for`, and the manifest is DATA from whatever
                // repo the user edits. Only Cargo's own package grammar
                // passes — anything else (quotes, `;`, spaces, unicode) is
                // rejected here, so injection-shaped names never reach bash.
                if !v.is_empty()
                    && v.chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                {
                    return Some(v.to_string());
                }
                return None;
            }
        }
    }
    None
}

/// `true` when no check for `crate_name` was marked inside the debounce
/// window. A missing or corrupt state file always allows (fail-open).
pub fn should_spawn(state_path: &Path, crate_name: &str, now_unix: u64, window_secs: u64) -> bool {
    let Ok(body) = std::fs::read_to_string(state_path) else {
        return true;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) else {
        return true;
    };
    match v.get(crate_name).and_then(serde_json::Value::as_u64) {
        Some(last) => now_unix.saturating_sub(last) >= window_secs,
        None => true,
    }
}

/// Record the spawn moment for the debounce window (best-effort).
pub fn mark_spawned(state_path: &Path, crate_name: &str, now_unix: u64) {
    let mut v = std::fs::read_to_string(state_path)
        .ok()
        .and_then(|b| serde_json::from_str::<serde_json::Value>(&b).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(map) = v.as_object_mut() {
        map.insert(crate_name.to_string(), serde_json::json!(now_unix));
    }
    if let Some(parent) = state_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(state_path, v.to_string());
}

/// The one-line verdict of the most recent FINISHED check — reading CONSUMES
/// it (the file is renamed `.delivered`), so a verdict is injected exactly
/// once. Format on disk: line 1 = `ok <crate>` | `fail <crate>`, remaining
/// lines = the tail of cargo's short output.
pub fn take_pending_verdict(verdict_path: &Path) -> Option<String> {
    let body = std::fs::read_to_string(verdict_path).ok()?;
    let mut lines = body.lines();
    let head = lines.next()?;
    let (status, krate) = head.split_once(' ')?;
    let summary = match status {
        "ok" => format!("check({krate}): OK"),
        _ => {
            let first = lines.next().unwrap_or("").trim();
            format!("check({krate}): FALHOU — {first}")
        }
    };
    // Consume: a verdict speaks once. Rename keeps the evidence on disk.
    let delivered = verdict_path.with_extension("delivered");
    let _ = std::fs::rename(verdict_path, delivered);
    Some(summary)
}

/// Fire-and-forget: resolve the crate, debounce, spawn a detached shell that
/// runs the check and writes the verdict file. Returns `true` when a check
/// was actually spawned. Non-`.rs` files and files outside a crate no-op.
pub fn spawn_check_for(file: &Path, state_dir: &Path) -> bool {
    if file.extension().and_then(|e| e.to_str()) != Some("rs") {
        return false;
    }
    let Some(krate) = resolve_crate_name(file) else {
        return false;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let state = state_dir.join("check_spawn.json");
    if !should_spawn(&state, &krate, now, DEBOUNCE_SECS) {
        return false;
    }
    mark_spawned(&state, &krate, now);
    let verdict = state_dir.join("check_verdict.txt");
    let cwd = file.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    // The subshell reparents to init (`& disown`) and writes atomically via
    // rename; the intermediate bash exits in milliseconds and is reaped by a
    // throwaway thread so the long-running daemon never accumulates zombies.
    let script = format!(
        "( out=$(cargo check -p {krate} --message-format=short 2>&1 | tail -4); rc=$?; \
         {{ [ $rc -eq 0 ] && echo 'ok {krate}' || echo 'fail {krate}'; printf '%s\\n' \"$out\"; }} \
         > {v}.tmp && mv {v}.tmp {v} ) & disown",
        krate = krate,
        v = verdict.display(),
    );
    match std::process::Command::new("bash")
        .arg("-c")
        .arg(script)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            true
        }
        Err(e) => {
            tracing::warn!("check_compile: spawn failed: {e}");
            false
        }
    }
}

/// Resolve the default state dir (`~/.claude/touring`) — the same root the
/// signal mirror lives in. `None` without HOME.
pub fn default_state_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".claude/touring"))
}

/// Post-edit/post-write glue: spawn a (debounced) check for the edited file
/// and deliver any PENDING verdict as one dense context line. `Allow`
/// becomes `Context` when a verdict is due; an existing `Context` gets the
/// verdict prepended; any other response passes through untouched. Every
/// arm is fail-soft — a broken state file never changes the hook decision.
pub fn attach_check_signal(
    file_path: &str,
    response: super::runtime::HookResponse,
    state_dir: &Path,
) -> super::runtime::HookResponse {
    use super::runtime::HookResponse;
    let _ = spawn_check_for(Path::new(file_path), state_dir);
    let Some(verdict) = take_pending_verdict(&state_dir.join("check_verdict.txt")) else {
        return response;
    };
    match response {
        HookResponse::Context { context, event_name } => HookResponse::Context {
            context: format!("⚙ {verdict} | {context}"),
            event_name,
        },
        HookResponse::Allow => HookResponse::Context {
            context: format!("⚙ {verdict}"),
            event_name: Some("PostToolUse".to_string()),
        },
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_verdict_upgrades_allow_to_context() {
        let d = tmp();
        std::fs::write(d.join("check_verdict.txt"), "ok minha-crate\n").expect("verdict");
        let out = attach_check_signal(
            "README.md",
            super::super::runtime::HookResponse::Allow,
            &d,
        );
        match out {
            super::super::runtime::HookResponse::Context { context, .. } => {
                assert!(context.contains("check(minha-crate): OK"), "{context}");
            }
            _ => panic!("a due verdict must surface as Context"),
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn no_verdict_leaves_the_response_untouched() {
        let d = tmp();
        let out = attach_check_signal(
            "README.md",
            super::super::runtime::HookResponse::Allow,
            &d,
        );
        assert!(
            matches!(out, super::super::runtime::HookResponse::Allow),
            "silence stays silent"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "check-compile-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&d).expect("tmp dir");
        d
    }

    #[test]
    fn resolves_the_nearest_package_manifest() {
        let d = tmp();
        std::fs::create_dir_all(d.join("src")).expect("src");
        std::fs::write(
            d.join("Cargo.toml"),
            "[package]\nname = \"minha-crate\"\nversion = \"0.1.0\"\n",
        )
        .expect("manifest");
        let f = d.join("src/lib.rs");
        std::fs::write(&f, "").expect("file");
        assert_eq!(resolve_crate_name(&f).as_deref(), Some("minha-crate"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn workspace_manifest_without_package_is_skipped_upward() {
        let d = tmp();
        // workspace root: no [package]; nested crate: has one.
        std::fs::write(d.join("Cargo.toml"), "[workspace]\nmembers=[\"a\"]\n").expect("ws");
        std::fs::create_dir_all(d.join("a/src")).expect("a/src");
        std::fs::write(d.join("a/Cargo.toml"), "[package]\nname = \"a\"\n").expect("a");
        let f = d.join("a/src/main.rs");
        std::fs::write(&f, "").expect("file");
        assert_eq!(resolve_crate_name(&f).as_deref(), Some("a"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn debounce_blocks_inside_window_and_reopens_after() {
        let d = tmp();
        let state = d.join("check_spawn.json");
        assert!(should_spawn(&state, "x", 1000, 30), "missing state is open");
        mark_spawned(&state, "x", 1000);
        assert!(!should_spawn(&state, "x", 1010, 30), "inside window blocks");
        assert!(should_spawn(&state, "y", 1010, 30), "other crate unaffected");
        assert!(should_spawn(&state, "x", 1031, 30), "window elapsed reopens");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn verdict_is_delivered_exactly_once() {
        let d = tmp();
        let v = d.join("check_verdict.txt");
        std::fs::write(&v, "fail minha-crate\nerror[E0308]: mismatched types\n").expect("verdict");
        let msg = take_pending_verdict(&v).expect("first read delivers");
        assert!(msg.contains("FALHOU"), "failure verdict names itself: {msg}");
        assert!(msg.contains("E0308"), "the first error line travels: {msg}");
        assert!(take_pending_verdict(&v).is_none(), "second read is silent");
        assert!(
            v.with_extension("delivered").exists(),
            "the evidence stays on disk"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Security (commit review 01/09) — the manifest is attacker-shaped data
    /// (any cloned repo); a name outside Cargo's grammar must never survive
    /// the parse, because `spawn_check_for` interpolates it into a shell line.
    #[test]
    fn injection_shaped_package_names_are_rejected() {
        for evil in [
            "x; rm -rf $HOME",
            "x\"; touch /tmp/pwn; \"",
            "x`id`",
            "x$(id)",
            "nome com espaço",
        ] {
            let body = format!("[package]\nname = \"{evil}\"\n");
            assert_eq!(
                package_name_from_manifest(&body),
                None,
                "must reject: {evil}"
            );
        }
        assert_eq!(
            package_name_from_manifest("[package]\nname = \"ok_name-123\"\n").as_deref(),
            Some("ok_name-123"),
            "the legitimate grammar still passes"
        );
    }

    #[test]
    fn non_rust_files_never_spawn() {
        let d = tmp();
        let f = d.join("README.md");
        std::fs::write(&f, "").expect("file");
        assert!(!spawn_check_for(&f, &d), "markdown must not trigger cargo");
        let _ = std::fs::remove_dir_all(&d);
    }
}
