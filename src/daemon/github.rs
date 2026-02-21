//! GitHub auto-discovery: list repos from orgs/users, clone missing ones.
//!
//! All errors are non-fatal — GitHub failure never blocks the daemon.
//! Missing tokens, API errors, and clone failures are logged and skipped.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;
use tracing::{error, info, warn};

use super::{GitHubConfig, GitHubSource, OwnerKind};

/// Minimal GitHub repo response (only fields we need).
#[derive(Debug, Deserialize)]
struct GitHubRepo {
    name: String,
    clone_url: String,
    archived: bool,
    fork: bool,
}

/// GitHub API client with bearer token auth.
struct GitHubClient {
    client: reqwest::Client,
    token: String,
}

impl GitHubClient {
    fn new(token: String) -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent("codesearch-daemon")
            .build()
            .context("Failed to build HTTP client")?;
        Ok(Self { client, token })
    }

    /// List all repos for a source (paginated, 100 per page).
    async fn list_repos(&self, source: &GitHubSource) -> Result<Vec<GitHubRepo>> {
        let base_url = match source.kind {
            OwnerKind::Org => format!("https://api.github.com/orgs/{}/repos", source.owner),
            OwnerKind::User => format!("https://api.github.com/users/{}/repos", source.owner),
        };

        let mut all_repos = Vec::new();
        let mut page = 1u32;

        loop {
            let resp = self
                .client
                .get(&base_url)
                .query(&[
                    ("per_page", "100"),
                    ("page", &page.to_string()),
                ])
                .header("Authorization", format!("Bearer {}", self.token))
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("Accept", "application/vnd.github+json")
                .send()
                .await
                .with_context(|| format!("GitHub API request failed (page {})", page))?;

            // Check rate limit before processing
            if let Some(remaining) = resp
                .headers()
                .get("x-ratelimit-remaining")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u32>().ok())
            {
                if remaining == 0 {
                    warn!("GitHub API rate limit exhausted, stopping pagination");
                    break;
                }
            }

            let status = resp.status();
            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                return Err(anyhow::anyhow!(
                    "GitHub API returned {}: {}",
                    status,
                    body
                ));
            }

            let repos: Vec<GitHubRepo> = resp
                .json()
                .await
                .context("Failed to parse GitHub repo list")?;

            let count = repos.len();
            all_repos.extend(repos);

            // Last page when we get fewer than 100
            if count < 100 {
                break;
            }
            page += 1;
        }

        Ok(all_repos)
    }
}

/// Resolve token from token_file (with ~ expansion) or GITHUB_TOKEN env var.
fn resolve_token(config: &GitHubConfig) -> Option<String> {
    // Try token_file first
    if let Some(ref path) = config.token_file {
        let expanded = shellexpand::tilde(path);
        match std::fs::read_to_string(expanded.as_ref()) {
            Ok(token) => {
                let token = token.trim().to_string();
                if !token.is_empty() {
                    return Some(token);
                }
                warn!("Token file {} is empty", path);
            }
            Err(e) => {
                warn!("Failed to read token file {}: {}", path, e);
            }
        }
    }

    // Fall back to env var
    match std::env::var("GITHUB_TOKEN") {
        Ok(token) if !token.is_empty() => Some(token),
        _ => None,
    }
}

/// Simple wildcard pattern matcher (supports `*` only).
///
/// Patterns like `"*.wiki"`, `"legacy-*"`, `"test-*-old"` are matched
/// against repo names. `*` matches any sequence of characters (including empty).
fn matches_pattern(name: &str, pattern: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();

    if parts.len() == 1 {
        // No wildcard — exact match
        return name == pattern;
    }

    let mut pos = 0;

    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }

        if i == 0 {
            // First segment must match at start
            if !name.starts_with(part) {
                return false;
            }
            pos = part.len();
        } else if i == parts.len() - 1 {
            // Last segment must match at end
            if !name[pos..].ends_with(part) {
                return false;
            }
            pos = name.len();
        } else {
            // Middle segment — find anywhere after current pos
            match name[pos..].find(part) {
                Some(found) => pos += found + part.len(),
                None => return false,
            }
        }
    }

    true
}

/// Check if a repo name matches any exclude pattern.
fn is_excluded(name: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|p| matches_pattern(name, p))
}

/// Filter repos based on source configuration.
fn filter_repos(repos: Vec<GitHubRepo>, source: &GitHubSource) -> Vec<GitHubRepo> {
    repos
        .into_iter()
        .filter(|r| {
            if source.skip_archived && r.archived {
                return false;
            }
            if source.skip_forks && r.fork {
                return false;
            }
            if is_excluded(&r.name, &source.exclude) {
                return false;
            }
            true
        })
        .collect()
}

/// Clone a repo using gix (blocking, runs in spawn_blocking).
async fn clone_repo(clone_url: &str, dest: &Path, token: &str) -> Result<()> {
    let url_with_auth = clone_url.replacen("https://", &format!("https://x-access-token:{}@", token), 1);
    let dest = dest.to_path_buf();
    let url = url_with_auth.clone();

    tokio::task::spawn_blocking(move || -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {}", parent.display()))?;
        }

        let mut prepare = gix::prepare_clone(gix::url::parse(url.as_str().into())?, &dest)
            .with_context(|| format!("Failed to prepare clone to {}", dest.display()))?;

        let (mut checkout, _outcome) = prepare
            .fetch_then_checkout(gix::progress::Discard, &gix::interrupt::IS_INTERRUPTED)
            .with_context(|| format!("Failed to fetch {}", dest.display()))?;

        let (_repo, _outcome) = checkout
            .main_worktree(gix::progress::Discard, &gix::interrupt::IS_INTERRUPTED)
            .with_context(|| format!("Failed to checkout {}", dest.display()))?;

        Ok(())
    })
    .await
    .context("Clone task panicked")?
}

/// Resolve all repos from GitHub sources + explicit list.
///
/// Returns a deduplicated list of repo paths. All GitHub errors are non-fatal.
pub async fn resolve_all_repos(
    explicit: Vec<PathBuf>,
    github_config: Option<&GitHubConfig>,
) -> Vec<PathBuf> {
    let mut all_paths: Vec<PathBuf> = explicit;
    let mut seen = HashSet::new();

    let config = match github_config {
        Some(c) if !c.sources.is_empty() => c,
        _ => {
            // No GitHub config or no sources — just return explicit repos
            return all_paths;
        }
    };

    let token = match resolve_token(config) {
        Some(t) => t,
        None => {
            warn!("No GitHub token available — skipping repo discovery (set token_file or GITHUB_TOKEN)");
            return all_paths;
        }
    };

    let client = match GitHubClient::new(token.clone()) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to create GitHub client: {}", e);
            return all_paths;
        }
    };

    for source in &config.sources {
        info!(
            "Discovering repos from {} {} (clone_base: {})",
            match source.kind {
                OwnerKind::Org => "org",
                OwnerKind::User => "user",
            },
            source.owner,
            source.clone_base.display()
        );

        let repos = match client.list_repos(source).await {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to list repos for {}: {}", source.owner, e);
                continue;
            }
        };

        let total = repos.len();
        let filtered = filter_repos(repos, source);
        info!(
            "Found {} repos for {} ({} after filtering)",
            total,
            source.owner,
            filtered.len()
        );

        // Expand ~ in clone_base
        let clone_base_str = source.clone_base.to_string_lossy();
        let expanded = shellexpand::tilde(&clone_base_str);
        let clone_base = PathBuf::from(expanded.as_ref());

        for repo in &filtered {
            let local_path = clone_base.join(&repo.name);

            if local_path.exists() {
                info!("Found local clone: {}", local_path.display());
                all_paths.push(local_path);
            } else if source.auto_clone {
                info!("Cloning {} → {}", repo.name, local_path.display());
                match clone_repo(&repo.clone_url, &local_path, &token).await {
                    Ok(()) => {
                        info!("Cloned {}", repo.name);
                        all_paths.push(local_path);
                    }
                    Err(e) => {
                        error!("Failed to clone {}: {}", repo.name, e);
                    }
                }
            } else {
                info!("Skipping {} (not cloned, auto_clone=false)", repo.name);
            }
        }
    }

    // Deduplicate by canonical path
    all_paths.retain(|p| {
        let key = p.canonicalize().unwrap_or_else(|_| p.clone());
        seen.insert(key)
    });

    all_paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_pattern_exact() {
        assert!(matches_pattern("foo", "foo"));
        assert!(!matches_pattern("foo", "bar"));
    }

    #[test]
    fn test_matches_pattern_suffix_wildcard() {
        assert!(matches_pattern("legacy-api", "legacy-*"));
        assert!(matches_pattern("legacy-", "legacy-*"));
        assert!(!matches_pattern("new-api", "legacy-*"));
    }

    #[test]
    fn test_matches_pattern_prefix_wildcard() {
        assert!(matches_pattern("repo.wiki", "*.wiki"));
        assert!(matches_pattern(".wiki", "*.wiki"));
        assert!(!matches_pattern("repo.git", "*.wiki"));
    }

    #[test]
    fn test_matches_pattern_middle_wildcard() {
        assert!(matches_pattern("test-foo-old", "test-*-old"));
        assert!(matches_pattern("test--old", "test-*-old"));
        assert!(!matches_pattern("test-foo-new", "test-*-old"));
    }

    #[test]
    fn test_matches_pattern_star_only() {
        assert!(matches_pattern("anything", "*"));
        assert!(matches_pattern("", "*"));
    }

    #[test]
    fn test_is_excluded() {
        let patterns = vec!["*.wiki".to_string(), "legacy-*".to_string()];
        assert!(is_excluded("repo.wiki", &patterns));
        assert!(is_excluded("legacy-api", &patterns));
        assert!(!is_excluded("codesearch", &patterns));
    }
}
