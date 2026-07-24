//! `touring inferlet install` — install a WASM inferlet from a local manifest.
//!
//! Usage: `touring inferlet install <name> --manifest <path>`
//!
//! `<name>`            — inferlet name (used in install path)
//! `--manifest <path>` — path to InferletManifest TOML file
//!
//! Resolution logic:
//!   - If `wasm_uri` starts with `file://` or is a plain path → read from disk
//!   - If `wasm_uri` is a remote URL (http/https) → fetch via reqwest
//!   - If no `wasm_uri` field → error (must have one or the other)
//!
//! After loading bytes, validates `wasm_sha256` then saves:
//!   `~/.claude/touring/inferlets/<name>/<version>/inferlet.wasm`
//!   `~/.claude/touring/inferlets/<name>/<version>/inferlet.manifest.toml`

use std::path::PathBuf;

use inferlets::InferletManifest;
use sha2::{Digest, Sha256};

/// Run the `inferlet list` subcommand.
///
/// Scans `~/.claude/touring/inferlets/` (or `$TOURING_INFERLET_HOME`) for
/// installed inferlets and reports each one with its metadata.
///
/// # Output
///
/// Without `--json`: prints a human-readable table.
///
/// With `--json`: prints a machine-readable JSON array.
pub fn list(args: &[String]) -> anyhow::Result<()> {
    let json_mode = args.iter().any(|a| a == "--json" || a == "-j");

    let base = std::env::var("TOURING_INFERLET_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home)
                .join(".claude")
                .join("touring")
                .join("inferlets")
        });

    if !base.exists() {
        if json_mode {
            println!("{{\"inferlets\": [], \"count\": 0}}");
        } else {
            println!(
                "No inferlets installed (directory does not exist: {})",
                base.display()
            );
        }
        return Ok(());
    }

    let mut entries: Vec<InferletEntry> = Vec::new();

    // Scan <base>/<name>/<version>/
    for name_entry in std::fs::read_dir(&base)? {
        let name_entry = name_entry?;
        let name_path = name_entry.path();
        if !name_path.is_dir() {
            continue;
        }
        let name = name_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        for version_entry in std::fs::read_dir(&name_path)? {
            let version_entry = version_entry?;
            let version_path = version_entry.path();
            if !version_path.is_dir() {
                continue;
            }
            let version = version_path
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or("")
                .to_string();

            let manifest_path = version_path.join("inferlet.manifest.toml");
            if !manifest_path.exists() {
                continue;
            }

            match InferletManifest::load_from_path(&manifest_path) {
                Ok(manifest) => {
                    // Sanity-check that on-disk path layout matches manifest
                    // metadata; mismatches are common after manual moves and
                    // would otherwise silently mis-route inferlet lookups.
                    if !name.is_empty() && manifest.name != name {
                        eprintln!(
                            "Warning: inferlet path/manifest name mismatch — \
                             path={name} manifest={} (continuing with manifest)",
                            manifest.name
                        );
                    }
                    if !version.is_empty() && manifest.version != version {
                        eprintln!(
                            "Warning: inferlet path/manifest version mismatch — \
                             path={version} manifest={} (continuing with manifest)",
                            manifest.version
                        );
                    }
                    entries.push(InferletEntry {
                        name: manifest.name,
                        version: manifest.version,
                        description: manifest.description,
                        tags: manifest.metadata.tags,
                        author: manifest.metadata.author,
                        installed_path: version_path.to_string_lossy().to_string(),
                    });
                }
                Err(e) => {
                    // Skip invalid manifests but keep scanning
                    eprintln!("Warning: skipping {}: {e}", manifest_path.display());
                }
            }
        }
    }

    if json_mode {
        let count = entries.len();
        let result = serde_json::json!({
            "inferlets": entries,
            "count": count
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&result).expect("JSON serialization should never fail")
        );
    } else {
        if entries.is_empty() {
            println!("No inferlets installed.");
        } else {
            println!(
                "{:<25} {:<12} {:<40} {:<20}",
                "NAME", "VERSION", "DESCRIPTION", "AUTHOR"
            );
            println!("{}", "-".repeat(100));
            for e in &entries {
                let tags = if e.tags.is_empty() {
                    String::new()
                } else {
                    e.tags.join(", ")
                };
                println!(
                    "{:<25} {:<12} {:<40} {:<20}",
                    e.name,
                    e.version,
                    if e.description.len() > 38 {
                        format!("{}...", &e.description[..35])
                    } else {
                        e.description.clone()
                    },
                    e.author.as_deref().unwrap_or("-"),
                );
                if !tags.is_empty() {
                    println!("  tags: {tags}");
                }
            }
            println!("\nTotal: {} inferlet(s)", entries.len());
        }
    }

    Ok(())
}

/// One row in the `list` output.
#[derive(Debug, serde::Serialize)]
struct InferletEntry {
    name: String,
    version: String,
    description: String,
    tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    author: Option<String>,
    installed_path: String,
}

/// Run the `inferlet install` subcommand.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    // Args: [0]="touring", [1]="inferlet", [2]="install", [3]=<name>, [4..]=flags
    let name = args.get(3).ok_or_else(|| {
        anyhow::anyhow!(
            "Usage: touring inferlet install <name> --manifest <path>\n\
             Example: touring inferlet install pattern --manifest ./pattern.toml"
        )
    })?;

    let manifest_path = extract_flag_value(args, "manifest").ok_or_else(|| {
        anyhow::anyhow!(
            "Usage: touring inferlet install <name> --manifest <path>\n\
                 The --manifest flag is required."
        )
    })?;

    let manifest_path = PathBuf::from(manifest_path);
    if !manifest_path.exists() {
        anyhow::bail!("Manifest file not found: {}", manifest_path.display());
    }

    // ── 1. Load manifest ────────────────────────────────────────────────
    let manifest = InferletManifest::load_from_path(&manifest_path)
        .map_err(|e| anyhow::anyhow!("Failed to load manifest: {e}"))?;

    // Override name from manifest with CLI name (CLI takes precedence)
    let name = name.trim();
    let version = &manifest.version;

    // ── 2. Resolve wasm_bytes ───────────────────────────────────────────
    let wasm_bytes = resolve_wasm(&manifest).map_err(|e| anyhow::anyhow!("{e}"))?;

    // ── 3. Validate sha256 ──────────────────────────────────────────────
    let computed = Sha256::digest(&wasm_bytes);
    let computed_hex = format!("{:x}", computed);
    if computed_hex != manifest.wasm_sha256 {
        anyhow::bail!(
            "SHA-256 mismatch:\n  expected: {}\n  computed: {}",
            manifest.wasm_sha256,
            computed_hex
        );
    }

    // ── 4. Compute install path ─────────────────────────────────────────
    let install_dir = install_dir(name, version)?;
    std::fs::create_dir_all(&install_dir).map_err(|e| {
        anyhow::anyhow!(
            "Failed to create install directory {}: {}",
            install_dir.display(),
            e
        )
    })?;

    // ── 5. Write wasm file ──────────────────────────────────────────────
    let wasm_path = install_dir.join("inferlet.wasm");
    std::fs::write(&wasm_path, &wasm_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to write wasm file {}: {}", wasm_path.display(), e))?;

    // ── 6. Write manifest alongside ────────────────────────────────────
    let manifest_path_on_disk = install_dir.join("inferlet.manifest.toml");
    let manifest_toml = toml::to_string_pretty(&manifest)
        .map_err(|e| anyhow::anyhow!("Failed to serialize manifest: {e}"))?;
    std::fs::write(&manifest_path_on_disk, manifest_toml).map_err(|e| {
        anyhow::anyhow!(
            "Failed to write manifest file {}: {}",
            manifest_path_on_disk.display(),
            e
        )
    })?;

    // ── 7. Emit JSON ────────────────────────────────────────────────────
    let result = serde_json::json!({
        "status": "installed",
        "name": name,
        "version": version,
        "wasm_sha256": computed_hex,
        "path": wasm_path.to_string_lossy(),
    });

    println!(
        "{}",
        serde_json::to_string_pretty(&result).expect("JSON serialization should never fail")
    );
    Ok(())
}

/// Resolve wasm bytes from manifest's `wasm_uri` field.
fn resolve_wasm(manifest: &InferletManifest) -> anyhow::Result<Vec<u8>> {
    let uri = manifest
        .wasm_uri
        .as_ref()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Manifest has no `wasm_uri` field. \
                 Either set wasm_uri to a local file path or a remote URL."
            )
        })?
        .trim();

    if let Some(path) = uri.strip_prefix("file://") {
        // Strip file:// prefix
        read_local_wasm(PathBuf::from(path))
    } else if uri.starts_with('/') || uri.starts_with('.') {
        // Plain path (absolute or relative)
        read_local_wasm(PathBuf::from(uri))
    } else if uri.starts_with("http://") || uri.starts_with("https://") {
        fetch_remote_wasm(uri)
    } else {
        // Treat as plain path
        read_local_wasm(PathBuf::from(uri))
    }
}

/// Read wasm bytes from local filesystem.
fn read_local_wasm(path: PathBuf) -> anyhow::Result<Vec<u8>> {
    if !path.exists() {
        anyhow::bail!("Local wasm file not found: {}", path.display());
    }
    std::fs::read(&path)
        .map_err(|e| anyhow::anyhow!("Failed to read wasm file {}: {}", path.display(), e))
}

/// Fetch wasm bytes from a remote URL via reqwest.
fn fetch_remote_wasm(url: &str) -> anyhow::Result<Vec<u8>> {
    // reqwest is available via touring-server dependency
    let response = reqwest::blocking::get(url)
        .map_err(|e| anyhow::anyhow!("Failed to fetch wasm from {}: {}", url, e))?;
    if !response.status().is_success() {
        anyhow::bail!(
            "HTTP fetch failed for {}: status {}",
            url,
            response.status()
        );
    }
    response
        .bytes()
        .map_err(|e| anyhow::anyhow!("Failed to read bytes from {}: {}", url, e))
        .map(|b| b.to_vec())
}

/// Compute the install directory path.
fn install_dir(name: &str, version: &str) -> anyhow::Result<PathBuf> {
    let base = std::env::var("TOURING_INFERLET_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home)
                .join(".claude")
                .join("touring")
                .join("inferlets")
        });
    Ok(base.join(name).join(version))
}

/// Extract flag value from args, allowing both `--flag value` and `--flag=value`.
fn extract_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    // Check for --flag=value
    for arg in args.iter() {
        if let Some(val) = arg.strip_prefix(&format!("--{flag}=")) {
            return Some(val);
        }
    }
    // Check for --flag value
    for (i, arg) in args.iter().enumerate() {
        if arg == &format!("--{flag}") {
            return args.get(i + 1).map(|s| s.as_str());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_flag_value_eq() {
        let args = vec![
            "touring".into(),
            "inferlet".into(),
            "install".into(),
            "foo".into(),
            "--manifest=./bar.toml".into(),
        ];
        assert_eq!(extract_flag_value(&args, "manifest"), Some("./bar.toml"));
    }

    #[test]
    fn test_extract_flag_value_separate() {
        let args = vec![
            "touring".into(),
            "inferlet".into(),
            "install".into(),
            "foo".into(),
            "--manifest".into(),
            "./bar.toml".into(),
        ];
        assert_eq!(extract_flag_value(&args, "manifest"), Some("./bar.toml"));
    }

    #[test]
    fn test_extract_flag_value_missing() {
        let args = vec![
            "touring".into(),
            "inferlet".into(),
            "install".into(),
            "foo".into(),
        ];
        assert_eq!(extract_flag_value(&args, "manifest"), None);
    }

    /// Serializes the two env-mutating tests below so they cannot race on the
    /// shared `TOURING_INFERLET_HOME` process var under parallel execution
    /// (fix 2026-06-29: `custom`'s `set_var` was bleeding into `default`'s read).
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_install_dir_default() {
        let _guard = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Without TOURING_INFERLET_HOME, uses $HOME/.claude/touring/inferlets.
        // Defensive remove: another (serialized) test may have set it.
        unsafe { std::env::remove_var("TOURING_INFERLET_HOME") };
        let dir =
            install_dir("pattern", "1.0.0").expect("install_dir should not fail with valid input");
        assert!(dir.to_string_lossy().ends_with("inferlets/pattern/1.0.0"));
    }

    #[test]
    fn test_install_dir_custom() {
        let _guard = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("TOURING_INFERLET_HOME", "/tmp/inferlets-test") };
        let dir =
            install_dir("pattern", "1.0.0").expect("install_dir should not fail with valid input");
        assert_eq!(dir.to_string_lossy(), "/tmp/inferlets-test/pattern/1.0.0");
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_INFERLET_HOME") };
    }
}
