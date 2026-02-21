//! Multi-repo HTTP server for the daemon.
//!
//! Fan-out search across all managed repos, merge results by RRF score.

use std::sync::Arc;

use axum::{
    extract::{Json, State},
    http::StatusCode,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::fts::FtsStore;
use crate::vectordb::VectorStore;

use super::DaemonState;

// ── Request / Response types ─────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub path: Option<String>,
    /// Filter to a specific repo by name
    #[serde(default)]
    pub repo: Option<String>,
}

fn default_limit() -> usize {
    25
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub query: String,
    pub took_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub repo: String,
    pub path: String,
    pub content: String,
    pub start_line: usize,
    pub end_line: usize,
    pub kind: String,
    pub score: f32,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub repos: Vec<RepoStatus>,
}

#[derive(Debug, Serialize)]
pub struct RepoStatus {
    pub name: String,
    pub files: usize,
    pub chunks: usize,
}

#[derive(Debug, Serialize)]
pub struct ReposResponse {
    pub repos: Vec<RepoInfo>,
}

#[derive(Debug, Serialize)]
pub struct RepoInfo {
    pub name: String,
    pub path: String,
    pub db_path: String,
    pub files: usize,
    pub chunks: usize,
    pub indexed: bool,
}

// ── Server ───────────────────────────────────────────────────────────

pub async fn run_server(
    state: Arc<DaemonState>,
    port: u16,
    cancel_token: CancellationToken,
) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/status", get(status_handler))
        .route("/search", post(search_handler))
        .route("/repos", get(repos_handler))
        .with_state(state);

    let addr = format!("127.0.0.1:{}", port);
    info!("Daemon HTTP server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            cancel_token.cancelled().await;
            info!("HTTP server shutting down");
        })
        .await?;

    Ok(())
}

// ── Handlers ─────────────────────────────────────────────────────────

async fn health_handler(State(state): State<Arc<DaemonState>>) -> Json<HealthResponse> {
    let mut repos = Vec::new();

    for repo in &state.repos {
        let vs: tokio::sync::RwLockReadGuard<'_, VectorStore> =
            repo.stores.vector_store.read().await;
        let stats = vs.stats().unwrap_or(crate::vectordb::StoreStats {
            total_chunks: 0,
            total_files: 0,
            indexed: false,
            dimensions: 0,
            max_chunk_id: 0,
        });

        repos.push(RepoStatus {
            name: repo.name.clone(),
            files: stats.total_files,
            chunks: stats.total_chunks,
        });
    }

    Json(HealthResponse {
        status: "ready".to_string(),
        repos,
    })
}

async fn status_handler(State(state): State<Arc<DaemonState>>) -> Json<HealthResponse> {
    // Same as health for now
    health_handler(State(state)).await
}

async fn repos_handler(State(state): State<Arc<DaemonState>>) -> Json<ReposResponse> {
    let mut repos = Vec::new();

    for repo in &state.repos {
        let vs: tokio::sync::RwLockReadGuard<'_, VectorStore> =
            repo.stores.vector_store.read().await;
        let stats = vs.stats().unwrap_or(crate::vectordb::StoreStats {
            total_chunks: 0,
            total_files: 0,
            indexed: false,
            dimensions: 0,
            max_chunk_id: 0,
        });

        repos.push(RepoInfo {
            name: repo.name.clone(),
            path: repo.project_path.display().to_string(),
            db_path: repo.db_path.display().to_string(),
            files: stats.total_files,
            chunks: stats.total_chunks,
            indexed: stats.indexed,
        });
    }

    Json(ReposResponse { repos })
}

async fn search_handler(
    State(state): State<Arc<DaemonState>>,
    Json(req): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, (StatusCode, String)> {
    let start = std::time::Instant::now();

    // Embed query once
    let query_embedding = {
        let mut es = state.embedding_service.lock().await;
        es.embed_query(&req.query)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };

    // Fan-out search across all repos (or filtered repo)
    let mut all_results: Vec<SearchResult> = Vec::new();

    for repo in &state.repos {
        // Filter by repo name if requested
        if let Some(ref filter) = req.repo {
            if &repo.name != filter {
                continue;
            }
        }

        // Vector search
        let vector_results = {
            let vs: tokio::sync::RwLockReadGuard<'_, VectorStore> =
                repo.stores.vector_store.read().await;
            vs.search(&query_embedding, req.limit)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        };

        // FTS search
        let fts_results = {
            let fts: tokio::sync::RwLockReadGuard<'_, FtsStore> =
                repo.stores.fts_store.read().await;
            fts.search(&req.query, req.limit, None)
                .unwrap_or_default()
        };

        // RRF fusion per repo
        let fused = crate::rerank::rrf_fusion(
            &vector_results,
            &fts_results,
            crate::rerank::DEFAULT_RRF_K,
        );

        // Resolve chunk metadata and build results
        let vs: tokio::sync::RwLockReadGuard<'_, VectorStore> =
            repo.stores.vector_store.read().await;
        for fused_result in &fused {
            if let Ok(Some(chunk)) = vs.get_chunk(fused_result.chunk_id) {
                // Filter by path if requested
                if let Some(ref path_filter) = req.path {
                    if !chunk.path.contains(path_filter) {
                        continue;
                    }
                }

                // Make path relative to repo root
                let rel_path = chunk
                    .path
                    .strip_prefix(repo.project_path.to_str().unwrap_or(""))
                    .unwrap_or(&chunk.path)
                    .trim_start_matches('/')
                    .to_string();

                all_results.push(SearchResult {
                    repo: repo.name.clone(),
                    path: rel_path,
                    content: truncate_content(&chunk.content, 500),
                    start_line: chunk.start_line,
                    end_line: chunk.end_line,
                    kind: chunk.kind.clone(),
                    score: fused_result.rrf_score,
                });
            }
        }
    }

    // Sort all results by score descending, then truncate to limit
    all_results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    all_results.truncate(req.limit);

    let took_ms = start.elapsed().as_millis() as u64;

    Ok(Json(SearchResponse {
        results: all_results,
        query: req.query,
        took_ms,
    }))
}

fn truncate_content(content: &str, max_len: usize) -> String {
    if content.len() <= max_len {
        content.to_string()
    } else {
        format!("{}...", &content[..max_len])
    }
}
