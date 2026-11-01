//! Include resolution — local files and remote URLs with auth.
//!
//! ## Supported types
//!
//! | Type | Syntax | Example |
//! |------|--------|---------|
//! | Local file | `- file:` | `includes: [{file: "./shared-tasks.yml"}]` |
//! | Remote URL | `- url:` | `includes: [{url: "https://example.com/tasks.yml"}]` |
//!
//! ## Path resolution (`path_resolve`)
//!
//! - `relative` (default): resolves relative to the including file's directory
//! - `root`: resolves relative to workspace root
//! - `absolute`: path is treated as absolute
//!
//! ## Circular detection
//!
//! Tracks visited URLs/paths by content hash to detect circular includes.
//! Returns [`TasksfileError::CircularInclude`] if a cycle is detected.

use crate::tasks::error::{Result, TasksfileError};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Resolved include content with metadata.
#[derive(Debug)]
pub struct ResolvedInclude {
    /// Raw text content fetched from the include source.
    pub content: String,
    /// Where the content was resolved from.
    pub source: IncludeSource,
}

/// Origin of a resolved include — a local file or a fetched URL.
#[derive(Debug)]
pub enum IncludeSource {
    /// A local file resolved at the given path.
    Local(PathBuf),
    /// A remote URL fetched over HTTP.
    Remote {
        /// The URL that was fetched.
        url: String,
        /// HTTP status code returned by the fetch.
        status_code: u16,
    },
}

/// Resolve includes from a Tasksfile root directory and set of already-visited IDs.
/// Returns the merged content of all includes, or an error on circular include / fetch failure.
#[cfg(feature = "http-client")]
pub fn resolve_includes(
    includes: &[crate::tasks::schema::IncludeSpec],
    base_dir: &Path,
    visited: &mut HashSet<String>,
) -> Result<Vec<ResolvedInclude>> {
    // Embed a minimal single-thread runtime to drive the async HTTP fetches.
    // This keeps the public API synchronous while supporting URL includes.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| TasksfileError::IncludeFailed(format!("tokio runtime: {}", e)))?;
    rt.block_on(async_resolve_includes(includes, base_dir, visited))
}

/// Async helper — called within the embedded runtime above.
#[cfg(feature = "http-client")]
async fn async_resolve_includes(
    includes: &[crate::tasks::schema::IncludeSpec],
    base_dir: &Path,
    visited: &mut HashSet<String>,
) -> Result<Vec<ResolvedInclude>> {
    let mut resolved = Vec::new();

    for spec in includes {
        if let Some(file_path) = &spec.file {
            let resolved_path = resolve_path(file_path, spec.path_resolve.as_str(), base_dir);
            let content = read_local_file(&resolved_path, visited)?;
            resolved.push(ResolvedInclude {
                content,
                source: IncludeSource::Local(resolved_path),
            });
        } else if let Some(url) = &spec.url {
            let content = fetch_url(url, spec.auth.as_ref(), visited).await?;
            let status = 200;
            resolved.push(ResolvedInclude {
                content,
                source: IncludeSource::Remote {
                    url: url.clone(),
                    status_code: status,
                },
            });
        }
    }

    Ok(resolved)
}

/// Non-async version — resolves local files only (no HTTP feature).
#[cfg(not(feature = "http-client"))]
pub fn resolve_includes(
    includes: &[crate::tasks::schema::IncludeSpec],
    base_dir: &Path,
    visited: &mut HashSet<String>,
) -> Result<Vec<ResolvedInclude>> {
    let mut resolved = Vec::new();

    for spec in includes {
        if let Some(file_path) = &spec.file {
            let resolved_path = resolve_path(file_path, spec.path_resolve.as_str(), base_dir);
            let content = read_local_file(&resolved_path, visited)?;
            resolved.push(ResolvedInclude {
                content,
                source: IncludeSource::Local(resolved_path),
            });
        } else if spec.url.is_some() {
            return Err(TasksfileError::IncludeFailed(
                "URL includes require http-client feature".to_string(),
            ));
        }
    }

    Ok(resolved)
}

/// Resolve a path according to `path_resolve` strategy.
fn resolve_path(file: &str, path_resolve: &str, base_dir: &Path) -> PathBuf {
    match path_resolve {
        "absolute" => PathBuf::from(file),
        "root" => PathBuf::from(file), // relative to workspace root
        _ => base_dir.join(file),      // "relative" — default
    }
}

/// Read a local file and check for circular includes via content hash.
fn read_local_file(path: &Path, visited: &mut HashSet<String>) -> Result<String> {
    let content = fs::read_to_string(path)?;
    let id = content_id(&content);

    if visited.contains(&id) {
        return Err(TasksfileError::CircularInclude(format!(
            "Circular include detected: {}",
            path.display()
        )));
    }
    visited.insert(id);
    Ok(content)
}

/// HTTP fetch with optional .netrc authentication.
#[cfg(feature = "http-client")]
async fn fetch_url(
    url: &str,
    auth: Option<&crate::tasks::schema::NetrcAuth>,
    visited: &mut HashSet<String>,
) -> Result<String> {
    use std::time::Duration;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| TasksfileError::IncludeFailed(e.to_string()))?;

    let mut request = client.get(url);

    // Add basic auth if .netrc credentials are provided
    if let Some(auth_info) = auth
        && let (Some(u), Some(p)) = (&auth_info.username, &auth_info.password)
    {
        request = request.basic_auth(u, Some(p));
    }

    let response = request
        .send()
        .await
        .map_err(|e| TasksfileError::IncludeFailed(format!("HTTP fetch failed: {}", e)))?;

    let status = response.status().as_u16();
    if status != 200 {
        return Err(TasksfileError::IncludeFailed(format!(
            "HTTP {} for URL: {}",
            status, url
        )));
    }

    let content = response
        .text()
        .await
        .map_err(|e| TasksfileError::IncludeFailed(format!("Failed to read response: {}", e)))?;

    let id = content_id(&content);
    if visited.contains(&id) {
        return Err(TasksfileError::CircularInclude(format!(
            "Circular include: {}",
            url
        )));
    }
    visited.insert(id);
    Ok(content)
}

/// Compute a content hash for circular include detection.
fn content_id(content: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    content.hash(&mut h);
    format!("{:x}", h.finish())
}

/// Parse `.netrc` file for machine authentication.
///
/// Returns a map of machine → (login, password); an entry without a password is not stored.
///
/// `.netrc` is a stream of tokens, not of lines: `machine h login u password p` on ONE line is
/// the common form. This parser used to read one keyword per line, so that form yielded
/// nothing, and a following `machine` stored its own name as the previous entry's password.
/// `default` closes the current entry and is not stored; `macdef` bodies (to the next blank
/// line) and `account` values are skipped.
#[cfg(feature = "http-client")]
pub fn parse_netrc(netrc_path: &Path) -> std::collections::HashMap<String, (String, String)> {
    let mut machines = std::collections::HashMap::new();
    let Ok(content) = fs::read_to_string(netrc_path) else {
        return machines;
    };

    let store = |out: &mut std::collections::HashMap<String, (String, String)>,
                 machine: Option<String>,
                 login: Option<String>,
                 password: Option<String>| {
        if let (Some(m), Some(l), Some(p)) = (machine, login, password) {
            out.insert(m, (l, p));
        }
    };
    let (mut machine, mut login, mut password) = (None, None, None);
    let mut tokens = netrc_tokens(&content).into_iter();
    while let Some(token) = tokens.next() {
        match token {
            "machine" | "default" => {
                store(&mut machines, machine.take(), login.take(), password.take());
                machine = (token == "machine")
                    .then(|| tokens.next().map(str::to_string))
                    .flatten();
            }
            "login" => login = tokens.next().map(str::to_string),
            "password" => password = tokens.next().map(str::to_string),
            "account" => {
                tokens.next();
            }
            _ => {}
        }
    }
    store(&mut machines, machine, login, password);
    machines
}

/// The `.netrc` tokens across lines, without `#` comment lines and `macdef` bodies
/// (a macro body runs to the next blank line).
#[cfg(feature = "http-client")]
fn netrc_tokens(content: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut in_macro = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if in_macro {
            in_macro = !trimmed.is_empty();
            continue;
        }
        if trimmed.starts_with('#') {
            continue;
        }
        for word in trimmed.split_whitespace() {
            if word == "macdef" {
                in_macro = true;
                break;
            }
            tokens.push(word);
        }
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_temp_dir() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn test_resolve_local_file() {
        let dir = make_temp_dir();
        let file_path = dir.path().join("included.yml");
        fs::write(&file_path, "tasks:\n  build: {}").unwrap();

        let spec = crate::tasks::schema::IncludeSpec {
            file: Some("included.yml".to_string()),
            url: None,
            path_resolve: "relative".to_string(),
            auth: None,
        };

        let mut visited = HashSet::new();
        let includes = resolve_includes(&[spec], dir.path(), &mut visited).unwrap();
        assert_eq!(includes.len(), 1);
        assert!(includes[0].content.contains("tasks:"));
    }

    #[test]
    fn test_circular_include_detection() {
        let dir = make_temp_dir();
        let file_path = dir.path().join("self.yml");
        fs::write(&file_path, "tasks:\n  build: {}").unwrap();

        let spec = crate::tasks::schema::IncludeSpec {
            file: Some("self.yml".to_string()),
            url: None,
            path_resolve: "relative".to_string(),
            auth: None,
        };

        let mut visited = HashSet::new();
        // First call — OK
        let _ = resolve_includes(std::slice::from_ref(&spec), dir.path(), &mut visited);
        // Second call with same content — should detect circular
        let result = resolve_includes(&[spec], dir.path(), &mut visited);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TasksfileError::CircularInclude(_)
        ));
    }

    #[cfg(feature = "http-client")]
    #[test]
    fn test_netrc_parser() {
        let dir = make_temp_dir();
        let netrc_path = dir.path().join(".netrc");
        fs::write(
            &netrc_path,
            "machine example.com login user password pass123\nmachine other.net login admin",
        )
        .unwrap();

        let machines = parse_netrc(&netrc_path);
        assert_eq!(
            machines.get("example.com"),
            Some(&("user".to_string(), "pass123".to_string()))
        );
        // other.net has no password — not stored
        assert!(!machines.contains_key("other.net"));
    }

    #[cfg(feature = "http-client")]
    #[test]
    fn test_netrc_tokens_span_lines_and_skip_default_and_macros() {
        let dir = make_temp_dir();
        let netrc_path = dir.path().join(".netrc");
        fs::write(
            &netrc_path,
            "# comment\nmachine a.com\n  login ua\n  password pa\n\
             machine b.com login ub\nmacdef init\ncd /tmp\nput x\n\n\
             default login anon password guest\n\
             machine c.com login uc account acct password pc\n",
        )
        .unwrap();

        let machines = parse_netrc(&netrc_path);
        assert_eq!(
            machines.get("a.com"),
            Some(&("ua".to_string(), "pa".to_string()))
        );
        // b.com has no password: not stored, and the next `machine` name is never its password
        assert!(!machines.contains_key("b.com"), "{machines:?}");
        assert_eq!(
            machines.get("c.com"),
            Some(&("uc".to_string(), "pc".to_string()))
        );
        assert_eq!(machines.len(), 2, "default is not a machine: {machines:?}");
    }
}
