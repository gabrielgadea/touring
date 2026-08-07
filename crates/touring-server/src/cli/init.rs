//! `touring init` — Initialize touring configuration with TOML profiles.
//!
//! Profiles are stored in `~/.claude/touring/profiles/` and selected via
//! `--profile <name>`. The selected profile is written to `~/.claude/touring/touring.toml`.
//!
//! Wave P3-1.3 W7 FINAL (2026-06-11): migrated from manual positional parsing
//! to clap derive. Dispatch contract unchanged — `run` still receives the full
//! argv slice; clap parses `args[1..]`.
//!
//! Subcommands:
//! - `touring init --profile <name>`   — Apply a named profile
//! - `touring init --list-profiles`    — Show all available profiles with descriptions
//! - `touring init --cc-setup`         — Install CC-specific hooks and merge settings
//! - `touring init --rignore-audit`    — Audit .gitignore files for issues

use anyhow::{Result, bail};
use clap::Parser;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

// CC-specific hook script embeddings
const CC_PRE_READ_HOOK: &str = include_str!("../../../hooks/cc-pre-read.sh");
const CC_PRE_WRITE_HOOK: &str = include_str!("../../../hooks/cc-pre-write.sh");
const CC_POST_EDIT_HOOK: &str = include_str!("../../../hooks/cc-post-edit.sh");

// ── CLI struct ───────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name = "touring init",
    bin_name = "touring init",
    about = "Initialize or apply a touring configuration profile",
    disable_help_subcommand = true
)]
struct InitCli {
    /// Apply a named profile (also accepts bare positional name).
    #[arg(long, short = 'p', value_name = "NAME")]
    profile: Option<String>,

    /// Positional profile name (alias for --profile without the flag).
    #[arg(value_name = "PROFILE", conflicts_with = "profile")]
    profile_pos: Option<String>,

    /// List all available profiles with descriptions.
    #[arg(long, short = 'l')]
    list_profiles: bool,

    /// Install CC-specific hooks and merge settings.json.
    #[arg(long, alias = "cc")]
    cc_setup: bool,

    /// Audit .gitignore files for issues (missing artifacts, broad patterns, sensitive files).
    #[arg(long, alias = "rignore")]
    rignore_audit: bool,

    /// Fix issues found by --rignore-audit (backup-first, idempotent).
    #[arg(long)]
    fix: bool,

    /// Bootstrap per-agent shell hook (see --list-agents for names).
    #[arg(long, value_name = "AGENT")]
    agent: Option<String>,

    /// List all supported agent names and their config paths.
    #[arg(long)]
    list_agents: bool,

    /// Preview changes without writing any files (for --agent bootstrap).
    #[arg(long)]
    dry_run: bool,

    /// Fast project bootstrap: autodetect the project (Cargo/pyproject/package.json),
    /// rebuild the Touring index, and run a health check — targets < 60s (Master Plan B-W1.P2).
    #[arg(long)]
    fast: bool,
}

// ── Entry point ──────────────────────────────────────────────────────────────

/// Run the `touring init` subcommand.
pub fn run(args: &[String]) -> Result<()> {
    let cli = InitCli::try_parse_from(args.iter().skip(1)).unwrap_or_else(|e| e.exit());

    if cli.fast {
        return run_fast_init();
    }
    if cli.cc_setup {
        return run_cc_setup();
    }
    if cli.fix {
        return run_rignore_fix();
    }
    if cli.rignore_audit {
        return run_rignore_audit();
    }
    if cli.list_agents {
        return list_all_agents();
    }
    if let Some(agent) = cli.agent {
        return apply_agent_init(&agent, cli.dry_run);
    }
    if cli.list_profiles {
        return list_all_profiles();
    }

    // Resolve profile name: --profile takes precedence over bare positional.
    let profile_name = cli.profile.or(cli.profile_pos);
    if let Some(name) = profile_name {
        apply_profile(&name)?;
    } else {
        print_help();
    }

    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    InitCli::command()
}

/// Project kinds detected at a root by manifest presence (Cargo/pyproject/package.json).
/// Pure + total — backs `touring init --fast` autodetection (Master Plan B.W1.P2.T3).
fn detect_project_kinds(root: &Path) -> Vec<&'static str> {
    let mut kinds = Vec::new();
    if root.join("Cargo.toml").is_file() {
        kinds.push("rust");
    }
    if root.join("pyproject.toml").is_file() || root.join("setup.py").is_file() {
        kinds.push("python");
    }
    if root.join("package.json").is_file() {
        kinds.push("node");
    }
    kinds
}

/// `touring init --fast`: autodetect the project, rebuild the Touring index, and run a
/// health check under a < 60s target (Master Plan B.W1.P2.T3/T4). Decoupled by spawning
/// the running `touring` binary for `index rebuild` and `doctor`, so init never has to
/// construct a `HookRuntime`. Exits non-zero past the 90s CI budget; warns past 60s.
fn run_fast_init() -> Result<()> {
    use std::process::Command;
    let started = std::time::Instant::now();
    let root = project_root();
    let kinds = detect_project_kinds(&root);
    if kinds.is_empty() {
        println!(
            "touring init --fast: no Cargo.toml / pyproject.toml / package.json in {} — \
             indexing as a generic source tree.",
            root.display()
        );
    } else {
        println!(
            "touring init --fast: detected project [{}] at {}",
            kinds.join(", "),
            root.display()
        );
    }

    // Use the running touring binary so we exercise the real client→daemon path.
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("touring"));

    println!("  → [1/2] index rebuild …");
    match Command::new(&exe)
        .args(["index", "rebuild", &root.to_string_lossy()])
        .status()
    {
        Ok(s) if s.success() => println!("        index rebuild: ok"),
        Ok(s) => println!("        index rebuild: exited with {s} (continuing)"),
        Err(e) => {
            println!("        index rebuild: could not spawn ({e}) — is the daemon reachable?")
        }
    }

    println!("  → [2/2] doctor …");
    match Command::new(&exe).args(["doctor", "-j"]).output() {
        Ok(o) if o.status.success() => {
            let txt = String::from_utf8_lossy(&o.stdout);
            let ok = txt.matches("\"status\":\"ok\"").count()
                + txt.matches("\"status\": \"ok\"").count();
            println!("        doctor: {ok} components ok");
        }
        Ok(o) => println!("        doctor: exited with {} (continuing)", o.status),
        Err(e) => println!("        doctor: could not spawn ({e})"),
    }

    let secs = started.elapsed().as_secs_f64();
    println!("touring init --fast: completed in {secs:.1}s");
    if secs > 90.0 {
        bail!(
            "init --fast took {secs:.1}s (> 90s CI budget) — index rebuild is too slow for this workspace"
        );
    }
    if secs > 60.0 {
        println!(
            "⚠ init --fast took {secs:.1}s (> 60s target) — consider scoping the index or warming caches."
        );
    }
    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Path to the project root (where .gitignore files are searched).
fn project_root() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Return the home directory or a fallback path.
fn home_or_default() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Directory where profile TOML files are stored.
fn profiles_dir() -> PathBuf {
    home_or_default()
        .join(".claude")
        .join("touring")
        .join("profiles")
}

/// Path to the active touring.toml configuration file.
fn config_path() -> PathBuf {
    home_or_default()
        .join(".claude")
        .join("touring")
        .join("touring.toml")
}

/// Profile metadata extracted from TOML content.
#[derive(Debug)]
struct Profile {
    name: String,
    description: String,
}

/// Extract name and description from TOML content using simple string matching.
/// This avoids adding a toml crate dependency.
fn parse_profile(content: &str, filename: &str) -> Option<Profile> {
    let name = extract_toml_value(content, "name").unwrap_or_else(|| {
        // Fallback to filename without extension
        filename
            .strip_suffix(".toml")
            .unwrap_or(filename)
            .to_string()
    });
    let description =
        extract_toml_value(content, "description").unwrap_or_else(|| "No description".to_string());

    Some(Profile { name, description })
}

/// Extract a string value from TOML content by looking for `key = "value"`.
fn extract_toml_value(content: &str, key: &str) -> Option<String> {
    let pattern = format!("{} = \"", key);
    let start = content.find(&pattern)?;
    let value_start = start + pattern.len();
    let value_end = content[value_start..].find('"')?;
    Some(content[value_start..value_start + value_end].to_string())
}

/// Load all profiles from the profiles directory.
fn load_profiles() -> Result<Vec<(String, Profile)>> {
    let dir = profiles_dir();
    let mut profiles = Vec::new();

    if !dir.exists() {
        return Ok(profiles);
    }

    for entry in std::fs::read_dir(&dir).map_err(|e| anyhow::anyhow!("read_dir failed: {}", e))? {
        let entry = entry.map_err(|e| anyhow::anyhow!("entry failed: {}", e))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("toml") {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| anyhow::anyhow!("read failed: {}", e))?;
            let filename = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown");
            if let Some(profile) = parse_profile(&content, filename) {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                profiles.push((name, profile));
            }
        }
    }

    profiles.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(profiles)
}

fn print_help() {
    eprintln!(
        "touring init — Initialize or apply a touring configuration profile

USAGE:
  touring init --profile <name>   Apply a profile
  touring init --list-profiles     Show all available profiles
  touring init --cc-setup          Install CC-specific hooks and merge settings

PROFILES:
  recommended  Full features: daemon + tantivy + cognitive + RL + generator
  quickstart   Local-only minimal: no daemon, CLI fallback mode, basic search
  airgapped    Zero network calls, no telemetry
  ci           JSON-only output, no TUI, headless-friendly
"
    );
}

fn list_all_profiles() -> Result<()> {
    let profiles = load_profiles()?;

    if profiles.is_empty() {
        eprintln!("No profiles found in ~/.claude/touring/profiles/");
        eprintln!("Create .toml files there with [name] and [description] tables.");
        return Ok(());
    }

    for (slug, profile) in &profiles {
        let display_name = if profile.name == *slug || profile.name.is_empty() {
            slug.clone()
        } else {
            format!("{} ({})", profile.name, slug)
        };
        println!("{} — {}", display_name, profile.description);
    }

    Ok(())
}

fn apply_profile(name: &str) -> Result<()> {
    let profile_path = profiles_dir().join(format!("{}.toml", name));

    if !profile_path.exists() {
        let available = load_profiles()
            .unwrap_or_default()
            .into_iter()
            .map(|(n, _)| n)
            .collect::<Vec<_>>()
            .join(", ");

        bail!(
            "profile '{}' not found at {}\nAvailable profiles: {}",
            name,
            profile_path.display(),
            available
        );
    }

    let content = std::fs::read_to_string(&profile_path)?;

    // Validate it has name and description
    if extract_toml_value(&content, "name").is_none() {
        bail!("profile '{}' is missing a `name` field", name);
    }

    let config = config_path();

    // Backup existing config if present
    if config.exists() {
        let backup_path = config.with_extension("toml.bak");
        std::fs::copy(&config, &backup_path)
            .map_err(|e| anyhow::anyhow!("backup failed: {}", e))?;
        eprintln!("backed up existing config to {}", backup_path.display());
    }

    // Ensure parent directory exists
    if let Some(parent) = config.parent() {
        std::fs::create_dir_all(parent).map_err(|e| anyhow::anyhow!("create dir failed: {}", e))?;
    }

    std::fs::write(&config, &content).map_err(|e| anyhow::anyhow!("write config failed: {}", e))?;

    println!(
        "{}",
        serde_json::json!({
            "status": "ok",
            "profile": name,
            "config": config.display().to_string()
        })
    );

    Ok(())
}

// ── NEW-5 cross-agent shell hook bootstrap ────────────────────────────────

/// Returns the Sprint-4 supported agents. Order-stable for deterministic CLI output.
fn supported_agents() -> &'static [(&'static str, &'static str)] {
    &[
        ("claude-code", "~/.claude/settings.json (existing path)"),
        ("gemini", "~/.config/gemini/agents.toml"),
        ("codex", "~/.codex/config.json"),
        ("cursor", "~/.cursor/mcp.json"),
        ("cline", "~/.cline/extensions/touring/manifest.json"),
    ]
}

fn list_all_agents() -> Result<()> {
    println!("Supported agents (Sprint 4 scope — 5 of 12 RTK total):\n");
    for (name, path) in supported_agents() {
        println!("  {name:<14}  {path}");
    }
    println!();
    println!("Bootstrap with: touring init --agent <name>");
    println!("Dry run with:  touring init --agent <name> --dry-run");
    Ok(())
}

/// Resolves the touring-rewrite.sh path. Honors `TOURING_REWRITE_PATH` env override
/// for test/dev determinism; falls back to `~/.local/bin/touring-rewrite.sh`.
fn rewrite_script_path() -> PathBuf {
    if let Ok(custom) = std::env::var("TOURING_REWRITE_PATH") {
        return PathBuf::from(custom);
    }
    let mut p = home_or_default();
    p.push(".local/bin/touring-rewrite.sh");
    p
}

/// Apply per-agent init writer — installs the universal `touring-rewrite.sh`
/// hook into the agent's specific config. Idempotent: re-running is a no-op.
fn apply_agent_init(agent: &str, dry_run: bool) -> Result<()> {
    let agents: Vec<&'static str> = supported_agents().iter().map(|(n, _)| *n).collect();
    if !agents.contains(&agent) {
        anyhow::bail!(
            "Unknown agent: {agent}. Available: {}\nRun `touring init --list-agents` for paths.",
            agents.join(", ")
        );
    }
    let rewrite = rewrite_script_path();
    if !rewrite.exists() && !dry_run {
        eprintln!(
            "warning: {} not found — install via `cargo build --release && symlink to ~/.local/bin/`.",
            rewrite.display()
        );
    }
    match agent {
        "claude-code" => init_claude_code(dry_run),
        "gemini" => init_gemini(&rewrite, dry_run),
        "codex" => init_codex(&rewrite, dry_run),
        "cursor" => init_cursor(&rewrite, dry_run),
        "cline" => init_cline(&rewrite, dry_run),
        _ => unreachable!("filtered above"),
    }
}

fn init_claude_code(dry_run: bool) -> Result<()> {
    if dry_run {
        println!("[dry-run] claude-code: existing path preserved (no-op).");
        return Ok(());
    }
    println!("claude-code: existing settings.json path preserved (no changes).");
    println!("Use `touring init --cc-setup` for the canonical Claude-Code installer.");
    Ok(())
}

fn init_gemini(rewrite: &Path, dry_run: bool) -> Result<()> {
    let mut path = home_or_default();
    path.push(".config/gemini/agents.toml");
    let entry = format!(
        "\n# touring-rewrite installed via `touring init --agent gemini`\n[hooks]\npre_tool = {:?}\n",
        rewrite.display().to_string()
    );
    apply_idempotent_config(&path, &entry, "pre_tool = ", dry_run)
}

fn init_codex(rewrite: &Path, dry_run: bool) -> Result<()> {
    let mut path = home_or_default();
    path.push(".codex/config.json");
    let entry = format!(
        "{{\"hooks\": {{\"preToolUse\": \"{}\"}}, \"_touring_managed\": true}}",
        rewrite.display()
    );
    apply_idempotent_config(&path, &entry, "_touring_managed", dry_run)
}

fn init_cursor(rewrite: &Path, dry_run: bool) -> Result<()> {
    let mut path = home_or_default();
    path.push(".cursor/mcp.json");
    let entry = format!(
        "{{\"mcpServers\": {{\"touring\": {{\"command\": \"{}\"}}}}, \"_touring_managed\": true}}",
        rewrite.display()
    );
    apply_idempotent_config(&path, &entry, "\"touring\"", dry_run)
}

fn init_cline(rewrite: &Path, dry_run: bool) -> Result<()> {
    let mut path = home_or_default();
    path.push(".cline/extensions/touring/manifest.json");
    let entry = format!(
        "{{\"name\": \"touring\", \"hook\": \"{}\", \"schema_version\": 1}}",
        rewrite.display()
    );
    apply_idempotent_config(&path, &entry, "\"hook\":", dry_run)
}

/// Idempotent config writer: detect existing entry by `marker` substring,
/// skip with info log if present; otherwise create parent dir + append/write.
fn apply_idempotent_config(path: &Path, entry: &str, marker: &str, dry_run: bool) -> Result<()> {
    if dry_run {
        println!("[dry-run] would write to: {}", path.display());
        println!("[dry-run] entry preview:\n{entry}");
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    if path.exists() {
        let existing = std::fs::read_to_string(path).unwrap_or_default();
        if existing.contains(marker) {
            println!("✓ {} already managed by touring (no-op).", path.display());
            return Ok(());
        }
        // Append non-destructively for TOML; for JSON we overwrite cautiously.
        if path.extension().and_then(|e| e.to_str()) == Some("toml") {
            let merged = format!("{existing}\n{entry}");
            std::fs::write(path, merged)?;
        } else {
            // JSON path: warn user that we keep existing, only annotate.
            eprintln!(
                "warning: {} exists; manual merge recommended.",
                path.display()
            );
            eprintln!("Suggested entry:\n{entry}");
            return Ok(());
        }
    } else {
        std::fs::write(path, entry)?;
    }
    println!("✓ touring hook installed → {}", path.display());
    Ok(())
}

// ── cc-setup ────────────────────────────────────────────────────────────────

/// Merge user settings with CC-specific settings.
/// Reads ~/.claude/settings.json, merges with CC config, writes to cc-settings.json.
fn merge_settings_json() -> Result<String> {
    let settings_path = home_or_default().join(".claude").join("settings.json");
    let cc_path = home_or_default()
        .join(".claude")
        .join("touring")
        .join("cc-settings.json");

    if !settings_path.exists() {
        let minimal = r#"{"hooks":[],"matchers":[]}"#;
        std::fs::create_dir_all(cc_path.parent().expect("cc-settings parent dir"))?;
        std::fs::write(&cc_path, minimal)?;
        return Ok(minimal.to_string());
    }

    let user_settings = std::fs::read_to_string(&settings_path)
        .map_err(|e| anyhow::anyhow!("failed to read settings: {}", e))?;
    let merged = deep_merge_json(&user_settings, r#"{"source":"cc"}"#)?;
    std::fs::create_dir_all(cc_path.parent().expect("cc-settings parent dir"))?;
    std::fs::write(&cc_path, &merged)?;
    Ok(merged)
}

/// Deep merge two JSON values — overlay takes precedence on conflicts.
fn deep_merge_json(base: &str, overlay: &str) -> Result<String> {
    let base_val: serde_json::Value = serde_json::from_str(base)
        .map_err(|e| anyhow::anyhow!("failed to parse settings: {}", e))?;
    let overlay_val: serde_json::Value = serde_json::from_str(overlay)
        .map_err(|e| anyhow::anyhow!("failed to parse overlay: {}", e))?;
    let merged = deep_merge_value(&base_val, &overlay_val);
    serde_json::to_string_pretty(&merged)
        .map_err(|e| anyhow::anyhow!("failed to serialize merged: {}", e))
}

fn deep_merge_value(base: &serde_json::Value, overlay: &serde_json::Value) -> serde_json::Value {
    match (base, overlay) {
        (serde_json::Value::Object(base_obj), serde_json::Value::Object(overlay_obj)) => {
            let mut result = base_obj.clone();
            for (k, v) in overlay_obj {
                result.insert(
                    k.clone(),
                    deep_merge_value(
                        result.get(k.as_str()).unwrap_or(&serde_json::Value::Null),
                        v,
                    ),
                );
            }
            serde_json::Value::Object(result)
        }
        _ => overlay.clone(),
    }
}

/// Install CC-specific hook scripts to ~/.claude/hooks/ (idempotent).
pub fn install_cc_hooks() -> Result<Vec<String>> {
    let hooks_dir = home_or_default().join(".claude").join("hooks");
    std::fs::create_dir_all(&hooks_dir)?;

    let mut installed = Vec::new();
    for (name, content) in [
        ("cc-pre-read.sh", CC_PRE_READ_HOOK),
        ("cc-pre-write.sh", CC_PRE_WRITE_HOOK),
        ("cc-post-edit.sh", CC_POST_EDIT_HOOK),
    ] {
        let path = hooks_dir.join(name);
        if path.exists()
            && let Ok(existing) = std::fs::read_to_string(&path)
            && existing == content
        {
            installed.push(path.to_string_lossy().to_string());
            continue;
        }
        std::fs::write(&path, content)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| anyhow::anyhow!("failed to set permissions: {}", e))?;
        }
        installed.push(path.to_string_lossy().to_string());
    }
    Ok(installed)
}

/// Run CC setup: merge settings and install hooks.
pub fn run_cc_setup() -> Result<()> {
    eprintln!("Running CC setup...");

    match merge_settings_json() {
        Ok(merged) => {
            let cc_path = home_or_default()
                .join(".claude")
                .join("touring")
                .join("cc-settings.json");
            eprintln!("Merged settings written to {}", cc_path.display());
            eprintln!("Merged content: {}", merged);
        }
        Err(e) => {
            eprintln!("Warning: merge failed: {}", e);
        }
    }

    // D42: install_cc_hooks() is idempotent — safe to call once and reuse the result.
    let installed = match install_cc_hooks() {
        Ok(paths) => {
            for path in &paths {
                eprintln!("Installed: {}", path);
            }
            paths.len()
        }
        Err(e) => {
            bail!("hook installation failed: {}", e);
        }
    };

    println!(
        "{}",
        serde_json::json!({
            "status": "ok",
            "cc_setup": true,
            "hooks_installed": installed
        })
    );

    Ok(())
}

// ── rignore audit ────────────────────────────────────────────────────────────

/// Well-known build artifact directories that SHOULD be in .gitignore.
fn expected_build_artifacts() -> &'static [&'static str] {
    &[
        "target/",
        "node_modules/",
        "__pycache__/",
        "*.pyc",
        ".pytest_cache/",
        ".ruff_cache/",
        ".mypy_cache/",
        ".turbo/",
        "dist/",
        "build/",
        ".next/",
        ".nuxt/",
        ".cache/",
    ]
}

/// Sensitive files that should NEVER be ignored.
fn sensitive_patterns() -> &'static [&'static str] {
    &[
        ".env",
        ".env.local",
        ".env.production",
        "*.pem",
        "*.key",
        "*.p12",
        "*.pfx",
        "credentials.json",
        "secrets.yml",
        "secrets.yaml",
    ]
}

/// Patterns that are overly broad and may hide real issues.
fn overly_broad_patterns() -> &'static [&'static str] {
    &["**", "*.log", "*~", ".*"]
}

/// Check if a line is a comment or empty.
fn is_comment_or_empty(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.is_empty() || trimmed.starts_with('#')
}

/// Parse a .gitignore file and return entries with line numbers.
fn parse_gitignore(path: &Path) -> Result<Vec<(usize, String)>> {
    let content = std::fs::read_to_string(path)?;
    Ok(content
        .lines()
        .enumerate()
        .filter(|(_, line)| !is_comment_or_empty(line))
        .map(|(idx, line)| (idx + 1, line.trim().to_string()))
        .collect())
}

/// Audit result types.
#[derive(serde::Serialize)]
struct RignoreAuditResult {
    scanned_files: usize,
    total_entries: usize,
    issues: Vec<RignoreIssue>,
    score: f64,
}

#[derive(serde::Serialize)]
struct RignoreIssue {
    file: String,
    line: Option<usize>,
    entry: String,
    severity: &'static str,
    message: String,
}

/// Find all .gitignore files in a directory tree.
fn find_gitignores(root: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Skip hidden directories
                if path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.starts_with('.'))
                    .unwrap_or(false)
                {
                    continue;
                }
                result.extend(find_gitignores(&path));
            } else if path.file_name().and_then(|s| s.to_str()) == Some(".gitignore") {
                result.push(path);
            }
        }
    }
    result
}

/// Run the rignore audit and output JSON.
fn run_rignore_audit() -> Result<()> {
    let root = project_root();
    let gitignores = find_gitignores(&root);

    let mut total_entries = 0;
    let mut issues = Vec::new();

    for gitignore_path in &gitignores {
        let entries = parse_gitignore(gitignore_path)?;
        total_entries += entries.len();

        for (line_num, entry) in &entries {
            let entry_str = entry.as_str();

            // Check for sensitive files that should NOT be ignored
            for sensitive in sensitive_patterns() {
                if entry_str.contains(sensitive) {
                    issues.push(RignoreIssue {
                        file: gitignore_path.display().to_string(),
                        line: Some(*line_num),
                        entry: entry_str.to_string(),
                        severity: "error",
                        message: format!(
                            "Sensitive pattern '{}' should NEVER be in .gitignore — may expose secrets",
                            sensitive
                        ),
                    });
                }
            }

            // Check for overly broad patterns
            for &broad in overly_broad_patterns() {
                if entry_str == broad {
                    issues.push(RignoreIssue {
                        file: gitignore_path.display().to_string(),
                        line: Some(*line_num),
                        entry: entry_str.to_string(),
                        severity: "warning",
                        message: format!(
                            "Overly broad pattern '{}' may hide real issues or cause confusion",
                            broad
                        ),
                    });
                }
            }
        }

        // Check for missing expected build artifacts
        let content = std::fs::read_to_string(gitignore_path)?;
        let content_lower = content.to_lowercase();

        for artifact in expected_build_artifacts() {
            let artifact_lower = artifact.to_lowercase();
            if !content_lower.contains(&artifact_lower.replace('/', "").replace("*.", "")) {
                // Only warn for common top-level artifacts
                if artifact.trim_end_matches('/').len() < 15 {
                    issues.push(RignoreIssue {
                        file: gitignore_path.display().to_string(),
                        line: None,
                        entry: artifact.to_string(),
                        severity: "info",
                        message: format!(
                            "Expected build artifact '{}' not found in .gitignore — consider adding",
                            artifact
                        ),
                    });
                }
            }
        }
    }

    // Calculate score: penalize errors heavily, warnings moderately, info lightly
    let error_count = issues.iter().filter(|i| i.severity == "error").count();
    let warning_count = issues.iter().filter(|i| i.severity == "warning").count();
    let info_count = issues.iter().filter(|i| i.severity == "info").count();

    let score = if issues.is_empty() {
        1.0
    } else {
        let penalty =
            (error_count as f64 * 0.3) + (warning_count as f64 * 0.1) + (info_count as f64 * 0.02);
        (1.0 - penalty).max(0.0)
    };

    let result = RignoreAuditResult {
        scanned_files: gitignores.len(),
        total_entries,
        issues,
        score,
    };

    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

/// Apply fixes to .gitignore files (backup-first approach).
fn run_rignore_fix() -> Result<()> {
    let dry_run = std::env::var("TOURING_RIGNORE_DRY_RUN").as_deref() == Ok("1");
    let root = project_root();
    let gitignores = find_gitignores(&root);

    if gitignores.is_empty() {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "status": "ok",
                "dry_run": dry_run,
                "message": "No .gitignore files found",
                "files_scanned": 0,
                "fixes_applied": 0
            }))?
        );
        return Ok(());
    }

    let mut fixes = Vec::new();

    for gitignore_path in &gitignores {
        let entries = parse_gitignore(gitignore_path)?;
        let content = std::fs::read_to_string(gitignore_path)?;
        let mut new_lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();

        for (line_num, entry) in &entries {
            let entry_str = entry.as_str();

            for sensitive in sensitive_patterns() {
                if entry_str.contains(sensitive) {
                    fixes.push(RignoreFix {
                        file: gitignore_path.display().to_string(),
                        line: Some(*line_num),
                        action: "remove".to_string(),
                        entry: entry_str.to_string(),
                        reason: format!(
                            "Sensitive pattern '{}' should NOT be in .gitignore",
                            sensitive
                        ),
                    });
                    if let Some(pos) = new_lines.iter().position(|l| l.trim() == *entry) {
                        new_lines[pos] = String::new();
                    }
                }
            }

            for &broad in overly_broad_patterns() {
                if entry_str == broad {
                    fixes.push(RignoreFix {
                        file: gitignore_path.display().to_string(),
                        line: Some(*line_num),
                        action: "remove".to_string(),
                        entry: entry_str.to_string(),
                        reason: format!("Overly broad pattern '{}' may hide real issues", broad),
                    });
                    if let Some(pos) = new_lines.iter().position(|l| l.trim() == *entry) {
                        new_lines[pos] = String::new();
                    }
                }
            }
        }

        if dry_run {
            continue;
        }

        let has_fixes = fixes
            .iter()
            .any(|f| f.file == gitignore_path.display().to_string());
        if !has_fixes {
            continue;
        }

        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("time went backwards")
            .as_secs();
        let backup_path = gitignore_path.with_extension(format!("gitignore.bak.{}", timestamp));
        std::fs::copy(gitignore_path, &backup_path)?;
        eprintln!("backup created: {}", backup_path.display());

        new_lines.retain(|l| !l.trim().is_empty());
        let new_content = new_lines.join("\n");
        std::fs::write(gitignore_path, new_content)?;
        eprintln!("fixed: {}", gitignore_path.display());
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "status": "ok",
            "dry_run": dry_run,
            "files_scanned": gitignores.len(),
            "fixes_applied": if dry_run { 0 } else { fixes.len() },
            "backups_created": if dry_run { 0 } else { fixes.iter().filter(|f| f.action == "remove").count() },
            "fixes": fixes
        }))?
    );

    Ok(())
}

#[derive(serde::Serialize)]
struct RignoreFix {
    file: String,
    line: Option<usize>,
    action: String,
    entry: String,
    reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_toml_value() {
        let content = r#"name = "test-profile"
description = "A test profile""#;
        assert_eq!(
            extract_toml_value(content, "name"),
            Some("test-profile".to_string())
        );
        assert_eq!(
            extract_toml_value(content, "description"),
            Some("A test profile".to_string())
        );
        assert_eq!(extract_toml_value(content, "missing"), None);
    }

    #[test]
    fn test_parse_profile() {
        let content = r#"name = "my-profile"
description = "My description""#;
        let profile = parse_profile(content, "my-profile.toml").unwrap();
        assert_eq!(profile.name, "my-profile");
        assert_eq!(profile.description, "My description");
    }

    #[test]
    fn test_parse_profile_fallback_name() {
        let content = r#"description = "No name here""#;
        let profile = parse_profile(content, "fallback.toml").unwrap();
        assert_eq!(profile.name, "fallback");
        assert_eq!(profile.description, "No name here");
    }

    #[test]
    fn test_load_profiles_from_real_dir() {
        let profiles_dir = profiles_dir();
        if profiles_dir.exists() {
            let result = load_profiles();
            assert!(result.is_ok());
            let profiles = result.unwrap();
            let names: Vec<_> = profiles.iter().map(|(n, _)| n.clone()).collect();
            if names.contains(&"recommended".to_string()) {
                assert!(names.contains(&"quickstart".to_string()));
                assert!(names.contains(&"airgapped".to_string()));
                assert!(names.contains(&"ci".to_string()));
            }
        }
    }

    #[test]
    fn test_config_path_uses_home() {
        let config = config_path();
        assert!(
            config
                .to_string_lossy()
                .contains(".claude/touring/touring.toml")
        );
    }

    #[test]
    fn test_profiles_dir_uses_home() {
        let dir = profiles_dir();
        assert!(dir.to_string_lossy().contains(".claude/touring/profiles"));
    }

    #[test]
    fn init_cli_profile_flag_parses() {
        let result = InitCli::try_parse_from(["init", "--profile", "recommended"]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().profile, Some("recommended".to_string()));
    }

    #[test]
    fn init_cli_list_profiles_flag_parses() {
        let result = InitCli::try_parse_from(["init", "--list-profiles"]);
        assert!(result.is_ok());
        assert!(result.unwrap().list_profiles);
    }

    #[test]
    fn init_cli_cc_setup_flag_parses() {
        let result = InitCli::try_parse_from(["init", "--cc-setup"]);
        assert!(result.is_ok());
        assert!(result.unwrap().cc_setup);
    }

    #[test]
    fn init_cli_agent_flag_parses() {
        let result = InitCli::try_parse_from(["init", "--agent", "gemini", "--dry-run"]);
        assert!(result.is_ok());
        let cli = result.unwrap();
        assert_eq!(cli.agent, Some("gemini".to_string()));
        assert!(cli.dry_run);
    }

    #[test]
    fn init_cli_positional_profile_parses() {
        let result = InitCli::try_parse_from(["init", "quickstart"]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().profile_pos, Some("quickstart".to_string()));
    }

    #[test]
    fn init_cli_rignore_audit_flag_parses() {
        let result = InitCli::try_parse_from(["init", "--rignore-audit"]);
        assert!(result.is_ok());
        assert!(result.unwrap().rignore_audit);
    }

    #[test]
    fn init_cli_fast_flag_parses() {
        let result = InitCli::try_parse_from(["init", "--fast"]);
        assert!(result.is_ok());
        assert!(result.unwrap().fast);
    }

    #[test]
    fn detect_project_kinds_rust_only() {
        let dir = std::env::temp_dir().join(format!("tg_init_fast_rust_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname=\"x\"\n").unwrap();
        let kinds = detect_project_kinds(&dir);
        assert!(kinds.contains(&"rust"));
        assert!(!kinds.contains(&"node"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_project_kinds_polyglot_and_empty() {
        let dir = std::env::temp_dir().join(format!("tg_init_fast_poly_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        assert!(detect_project_kinds(&dir).is_empty());
        std::fs::write(dir.join("pyproject.toml"), "[project]\n").unwrap();
        std::fs::write(dir.join("package.json"), "{}\n").unwrap();
        let kinds = detect_project_kinds(&dir);
        assert!(kinds.contains(&"python"));
        assert!(kinds.contains(&"node"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
