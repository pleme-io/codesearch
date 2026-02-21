//! Multi-repo daemon with periodic indexing.
//!
//! Replaces N `codesearch serve` processes (one per repo) with a single daemon
//! that manages all repos from one process. Each repo keeps its own `.codesearch.db`
//! (LMDB + tantivy + metadata) — the daemon opens all of them and searches across
//! all stores, merging results by score.
//!
//! # Configuration
//!
//! The daemon reads a YAML config file (following the hanabi/kenshi/shinka pattern):
//!
//! ```yaml
//! port: 4444
//! index_interval: 300
//! lmdb_map_size_mb: 2048
//! repos:
//!   - /path/to/repo1
//!   - /path/to/repo2
//! ```
//!
//! Load order: defaults → YAML file → env vars

pub mod github;
pub mod server;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::constants::DB_DIR_NAME;
use crate::db_discovery::find_best_database;
use crate::embed::{EmbeddingService, ModelType};
use crate::index::{IndexManager, SharedStores};
use crate::vectordb::VectorStore;

/// Daemon configuration loaded from YAML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// HTTP listen port
    #[serde(default = "default_port")]
    pub port: u16,

    /// Repository paths to manage
    #[serde(default)]
    pub repos: Vec<PathBuf>,

    /// Re-index interval in seconds
    #[serde(default = "default_index_interval")]
    pub index_interval: u64,

    /// LMDB map size in MB (overrides CODESEARCH_LMDB_MAP_SIZE_MB)
    #[serde(default)]
    pub lmdb_map_size_mb: Option<usize>,

    /// Embedding model name (null = default mxbai-embed-xsmall-v1)
    #[serde(default)]
    pub model: Option<String>,

    /// GitHub auto-discovery configuration
    #[serde(default)]
    pub github: Option<GitHubConfig>,
}

/// GitHub auto-discovery: resolve repos from GitHub orgs/users.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GitHubConfig {
    /// Path to file containing GitHub token (supports ~ expansion)
    pub token_file: Option<String>,
    /// Sources to discover repos from
    #[serde(default)]
    pub sources: Vec<GitHubSource>,
}

/// A single GitHub owner (org or user) to discover repos from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubSource {
    /// GitHub owner name (org or username)
    pub owner: String,
    /// Whether this is an org or user account
    #[serde(default)]
    pub kind: OwnerKind,
    /// Local directory where repos are/should be cloned
    pub clone_base: PathBuf,
    /// Clone repos that don't exist locally
    #[serde(default)]
    pub auto_clone: bool,
    /// Skip archived repositories
    #[serde(default = "default_true")]
    pub skip_archived: bool,
    /// Skip forked repositories
    #[serde(default)]
    pub skip_forks: bool,
    /// Glob patterns to exclude repo names (e.g. "*.wiki", "legacy-*")
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// Whether a GitHub source is an organization or user.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OwnerKind {
    #[default]
    Org,
    User,
}

fn default_port() -> u16 {
    4444
}

fn default_index_interval() -> u64 {
    300
}

fn default_true() -> bool {
    true
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            port: default_port(),
            repos: Vec::new(),
            index_interval: default_index_interval(),
            lmdb_map_size_mb: None,
            model: None,
            github: None,
        }
    }
}

impl DaemonConfig {
    /// Load config from a YAML file path.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read config file {}: {}", path.display(), e))?;
        let mut config: Self = serde_yaml::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse config {}: {}", path.display(), e))?;

        // Env var overrides
        if let Ok(port) = std::env::var("CODESEARCH_DAEMON_PORT") {
            if let Ok(p) = port.parse() {
                config.port = p;
            }
        }
        if let Ok(interval) = std::env::var("CODESEARCH_INDEX_INTERVAL") {
            if let Ok(i) = interval.parse() {
                config.index_interval = i;
            }
        }
        if let Ok(size) = std::env::var("CODESEARCH_LMDB_MAP_SIZE_MB") {
            if let Ok(s) = size.parse() {
                config.lmdb_map_size_mb = Some(s);
            }
        }

        Ok(config)
    }
}

/// Per-repo handle holding its stores and metadata.
pub struct RepoHandle {
    pub name: String,
    pub project_path: PathBuf,
    pub db_path: PathBuf,
    pub stores: Arc<SharedStores>,
}

/// Shared daemon state accessible from HTTP handlers and the reindex task.
pub struct DaemonState {
    pub repos: Vec<RepoHandle>,
    pub embedding_service: tokio::sync::Mutex<EmbeddingService>,
}

/// Main daemon entry point.
pub async fn run_daemon(config: DaemonConfig, cancel_token: CancellationToken) -> Result<()> {
    info!("Starting codesearch daemon on port {}", config.port);

    // Set LMDB map size env var if configured (used by VectorStore::new)
    if let Some(size) = config.lmdb_map_size_mb {
        std::env::set_var("CODESEARCH_LMDB_MAP_SIZE_MB", size.to_string());
    }

    // Resolve repos: merge explicit list with GitHub-discovered repos
    let all_repos = github::resolve_all_repos(config.repos.clone(), config.github.as_ref()).await;

    info!(
        "Managing {} repos, re-index every {}s",
        all_repos.len(),
        config.index_interval
    );

    // Load embedding model once (respects config or uses default)
    let cache_dir = crate::constants::get_global_models_cache_dir()?;
    let model_type = config
        .model
        .as_ref()
        .and_then(|m| ModelType::parse(m))
        .unwrap_or_default();
    info!("Loading embedding model: {:?}", model_type);
    let embedding_service = EmbeddingService::with_cache_dir(model_type, Some(&cache_dir))?;
    let dimensions = embedding_service.dimensions();

    // Initialize repos
    let mut repo_handles = Vec::new();

    for repo_path in &all_repos {
        match init_repo(repo_path, dimensions, &cancel_token).await {
            Ok(handle) => {
                info!("Initialized repo: {} ({})", handle.name, handle.db_path.display());
                repo_handles.push(handle);
            }
            Err(e) => {
                error!("Failed to initialize repo {}: {}", repo_path.display(), e);
                // Continue with other repos — don't fail the whole daemon
            }
        }
    }

    if repo_handles.is_empty() {
        return Err(anyhow::anyhow!(
            "No repos initialized successfully. Check paths and ensure indexes exist \
             (run `codesearch index --add -g` per repo first)."
        ));
    }

    info!("{}/{} repos initialized", repo_handles.len(), all_repos.len());

    let state = Arc::new(DaemonState {
        repos: repo_handles,
        embedding_service: tokio::sync::Mutex::new(embedding_service),
    });

    // Start periodic re-index task
    let reindex_state = state.clone();
    let reindex_cancel = cancel_token.clone();
    let interval = Duration::from_secs(config.index_interval);
    tokio::spawn(async move {
        periodic_reindex(reindex_state, interval, reindex_cancel).await;
    });

    // Start HTTP server (blocks until shutdown)
    server::run_server(state, config.port, cancel_token).await
}

/// Initialize a single repo: find/create DB, open stores, clear stale readers, refresh index.
async fn init_repo(
    repo_path: &Path,
    dimensions: usize,
    cancel_token: &CancellationToken,
) -> Result<RepoHandle> {
    let canonical = repo_path.canonicalize().map_err(|e| {
        anyhow::anyhow!("Cannot canonicalize {}: {}", repo_path.display(), e)
    })?;

    let name = canonical
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| canonical.display().to_string());

    // Find existing database
    let db_info = find_best_database(Some(&canonical))?;

    let (project_path, db_path) = if let Some(info) = db_info {
        (info.project_path, info.db_path)
    } else {
        // No DB found — create a global index
        info!("No index found for {}, creating global index...", name);
        crate::index::add_to_index(Some(canonical.clone()), true, cancel_token.clone()).await?;

        // Symlink workaround for DB discovery
        let global_db = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("No home directory"))?
            .join(".codesearch.dbs")
            .join(&name)
            .join(DB_DIR_NAME);

        let local_link = canonical.join(DB_DIR_NAME);
        if global_db.exists() && !local_link.exists() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(&global_db, &local_link).ok();
        }

        let info = find_best_database(Some(&canonical))?
            .ok_or_else(|| anyhow::anyhow!("Index creation succeeded but DB not found"))?;
        (info.project_path, info.db_path)
    };

    // Open shared stores (read-write, acquires writer lock)
    let stores = SharedStores::new(&db_path, dimensions)?;
    let stores = Arc::new(stores);

    // Clear stale LMDB readers from crashed processes
    {
        let vs: tokio::sync::RwLockReadGuard<'_, VectorStore> = stores.vector_store.read().await;
        match vs.clear_stale_readers() {
            Ok(cleared) if cleared > 0 => {
                info!("Cleared {} stale LMDB readers for {}", cleared, name);
            }
            Err(e) => warn!("Failed to clear stale readers for {}: {}", name, e),
            _ => {}
        }
    }

    // Perform incremental refresh to bring index up to date
    info!("Refreshing index for {}...", name);
    IndexManager::perform_incremental_refresh_with_stores(&project_path, &db_path, &stores).await?;

    Ok(RepoHandle {
        name,
        project_path,
        db_path,
        stores,
    })
}

/// Periodically re-index all repos on a timer.
async fn periodic_reindex(
    state: Arc<DaemonState>,
    interval: Duration,
    cancel_token: CancellationToken,
) {
    let mut timer = tokio::time::interval(interval);
    // First tick fires immediately — skip it since we just indexed
    timer.tick().await;

    loop {
        tokio::select! {
            _ = timer.tick() => {
                info!("Periodic re-index starting...");
                for repo in &state.repos {
                    if cancel_token.is_cancelled() {
                        return;
                    }

                    // Clear stale readers as safety measure
                    {
                        let vs: tokio::sync::RwLockReadGuard<'_, VectorStore> = repo.stores.vector_store.read().await;
                        let _ = vs.clear_stale_readers();
                    }

                    match IndexManager::perform_incremental_refresh_with_stores(
                        &repo.project_path,
                        &repo.db_path,
                        &repo.stores,
                    ).await {
                        Ok(()) => info!("Re-indexed {}", repo.name),
                        Err(e) => error!("Re-index failed for {}: {}", repo.name, e),
                    }
                }
                info!("Periodic re-index complete");
            }
            _ = cancel_token.cancelled() => {
                info!("Periodic re-index task shutting down");
                return;
            }
        }
    }
}
