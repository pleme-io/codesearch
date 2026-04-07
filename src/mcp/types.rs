//! MCP types and request/response structures

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Request for semantic search
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SemanticSearchRequest {
    /// The search query (natural language or code snippet)
    pub query: String,

    /// Maximum number of results to return (default: 10)
    pub limit: Option<usize>,

    /// Return compact results (metadata only) to save tokens (default: true).
    /// When true: returns only path, start_line, end_line, kind, signature, score.
    /// When false: also includes full code content and surrounding context.
    /// Use compact=true (default) and then read specific files with line offsets for the code you need.
    pub compact: Option<bool>,

    /// Only return results from files under this path prefix (e.g., "src/api/")
    pub filter_path: Option<String>,
}

/// Request to get file chunks
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetFileChunksRequest {
    /// Path to the file (relative to project root)
    pub path: String,

    /// Return compact results (metadata only) to save tokens (default: true).
    /// When true: returns only path, start_line, end_line, kind, signature.
    /// When false: also includes full code content.
    pub compact: Option<bool>,
}

/// Request to find references/call sites of a symbol.
/// Use this AFTER semantic_search to find where a function/class/variable is used.
/// Use this INSTEAD OF grep for finding symbol usages in the codebase.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindReferencesRequest {
    /// The symbol name to find references for (e.g., "authenticate", "User", "Config")
    pub symbol: String,

    /// Maximum number of references to return (default: 20)
    pub limit: Option<usize>,
}

/// Search result item - returned by semantic_search and get_file_chunks
#[derive(Debug, Serialize)]
pub struct SearchResultItem {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub kind: String,
    pub score: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_prev: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_next: Option<String>,
}

/// Reference/call site item - returned by find_references
#[derive(Debug, Serialize)]
pub struct ReferenceItem {
    /// File path containing the reference
    pub path: String,
    /// Line number of the reference
    pub line: usize,
    /// The kind of chunk containing the reference (e.g., "Function", "Method")
    pub kind: String,
    /// Signature of the containing function/method (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// FTS relevance score
    pub score: f32,
}

/// Index status response
#[derive(Debug, Serialize)]
pub struct IndexStatusResponse {
    pub indexed: bool,
    pub total_chunks: usize,
    pub total_files: usize,
    pub model: String,
    pub dimensions: usize,
    pub max_chunk_id: u32,
    pub db_path: String,
    pub project_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

/// Database info response
#[derive(Debug, Serialize)]
pub struct DatabaseInfoResponse {
    pub database_path: String,
    pub project_path: String,
    pub is_current_directory: bool,
    pub depth_from_current: usize,
    pub total_chunks: usize,
    pub total_files: usize,
    pub model: String,
}

/// Find databases response
#[derive(Debug, Serialize)]
pub struct FindDatabasesResponse {
    pub databases: Vec<DatabaseInfoResponse>,
    pub message: String,
    pub current_directory: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_result_item_serialization_compact() {
        let item = SearchResultItem {
            path: "src/main.rs".to_string(),
            start_line: 1,
            end_line: 10,
            kind: "Function".to_string(),
            score: 0.95,
            signature: Some("fn main()".to_string()),
            content: None,
            context_prev: None,
            context_next: None,
        };

        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("src/main.rs"));
        assert!(json.contains("Function"));
        assert!(json.contains("fn main()"));
        // None fields should be omitted
        assert!(!json.contains("content"));
        assert!(!json.contains("context_prev"));
        assert!(!json.contains("context_next"));
    }

    #[test]
    fn test_search_result_item_serialization_full() {
        let item = SearchResultItem {
            path: "src/lib.rs".to_string(),
            start_line: 5,
            end_line: 20,
            kind: "Struct".to_string(),
            score: 0.85,
            signature: None,
            content: Some("pub struct Foo {}".to_string()),
            context_prev: Some("// before".to_string()),
            context_next: Some("// after".to_string()),
        };

        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("content"));
        assert!(json.contains("context_prev"));
        assert!(json.contains("context_next"));
        assert!(!json.contains("signature"));
    }

    #[test]
    fn test_reference_item_serialization() {
        let item = ReferenceItem {
            path: "src/search/mod.rs".to_string(),
            line: 42,
            kind: "Method".to_string(),
            signature: Some("pub fn search()".to_string()),
            score: 0.75,
        };

        let json = serde_json::to_string(&item).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["path"], "src/search/mod.rs");
        assert_eq!(parsed["line"], 42);
        assert_eq!(parsed["kind"], "Method");
        assert_eq!(parsed["score"], 0.75);
    }

    #[test]
    fn test_reference_item_no_signature() {
        let item = ReferenceItem {
            path: "test.rs".to_string(),
            line: 1,
            kind: "Function".to_string(),
            signature: None,
            score: 0.5,
        };

        let json = serde_json::to_string(&item).unwrap();
        assert!(!json.contains("signature"));
    }

    #[test]
    fn test_index_status_response_serialization() {
        let resp = IndexStatusResponse {
            indexed: true,
            total_chunks: 1000,
            total_files: 50,
            model: "bge-small".to_string(),
            dimensions: 384,
            max_chunk_id: 999,
            db_path: "/tmp/db".to_string(),
            project_path: "/tmp/project".to_string(),
            error_message: None,
        };

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"indexed\":true"));
        assert!(json.contains("\"total_chunks\":1000"));
        assert!(!json.contains("error_message"));
    }

    #[test]
    fn test_index_status_with_error() {
        let resp = IndexStatusResponse {
            indexed: false,
            total_chunks: 0,
            total_files: 0,
            model: "unknown".to_string(),
            dimensions: 0,
            max_chunk_id: 0,
            db_path: "".to_string(),
            project_path: "".to_string(),
            error_message: Some("Database not found".to_string()),
        };

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("error_message"));
        assert!(json.contains("Database not found"));
    }

    #[test]
    fn test_database_info_response_serialization() {
        let resp = DatabaseInfoResponse {
            database_path: "/home/user/.codesearch.db".to_string(),
            project_path: "/home/user/project".to_string(),
            is_current_directory: true,
            depth_from_current: 0,
            total_chunks: 500,
            total_files: 25,
            model: "bge-small".to_string(),
        };

        let json = serde_json::to_string(&resp).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["is_current_directory"], true);
        assert_eq!(parsed["depth_from_current"], 0);
        assert_eq!(parsed["total_chunks"], 500);
    }

    #[test]
    fn test_find_databases_response() {
        let resp = FindDatabasesResponse {
            databases: vec![],
            message: "No databases found".to_string(),
            current_directory: "/home/user".to_string(),
        };

        let json = serde_json::to_string(&resp).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["databases"].as_array().unwrap().is_empty());
        assert_eq!(parsed["message"], "No databases found");
    }

    #[test]
    fn test_semantic_search_request_deserialization() {
        let json = r#"{"query": "find authentication", "limit": 5, "compact": true}"#;
        let req: SemanticSearchRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.query, "find authentication");
        assert_eq!(req.limit, Some(5));
        assert_eq!(req.compact, Some(true));
        assert_eq!(req.filter_path, None);
    }

    #[test]
    fn test_semantic_search_request_minimal() {
        let json = r#"{"query": "test"}"#;
        let req: SemanticSearchRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.query, "test");
        assert_eq!(req.limit, None);
        assert_eq!(req.compact, None);
    }

    #[test]
    fn test_get_file_chunks_request_deserialization() {
        let json = r#"{"path": "src/main.rs", "compact": false}"#;
        let req: GetFileChunksRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.path, "src/main.rs");
        assert_eq!(req.compact, Some(false));
    }

    #[test]
    fn test_find_references_request_deserialization() {
        let json = r#"{"symbol": "UserService", "limit": 10}"#;
        let req: FindReferencesRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.symbol, "UserService");
        assert_eq!(req.limit, Some(10));
    }

    #[test]
    fn test_find_references_request_minimal() {
        let json = r#"{"symbol": "foo"}"#;
        let req: FindReferencesRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.symbol, "foo");
        assert_eq!(req.limit, None);
    }
}
