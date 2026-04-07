//! Centralized error types for codesearch
//!
//! This module provides a unified error handling approach using thiserror,
//! replacing the ad-hoc anyhow::Error usage throughout the codebase.

use std::path::PathBuf;
use thiserror::Error;

/// Main error type for codesearch operations
#[derive(Error, Debug)]
pub enum CodeSearchError {
    /// Database-related errors
    #[error("Database error: {message}")]
    Database {
        message: String,
        source: Option<anyhow::Error>,
    },

    /// I/O operation errors
    #[error("I/O error: {path} - {message}")]
    Io {
        path: PathBuf,
        message: String,
        source: Option<anyhow::Error>,
    },

    /// Embedding model errors
    #[error("Embedding error: {message}")]
    Embedding { message: String },

    /// Search operation errors
    #[error("Search error: {message}")]
    Search { message: String },

    /// Index operation errors
    #[error("Index error: {message}")]
    Index { message: String },

    /// Configuration errors
    #[error("Configuration error: {message}")]
    Config { message: String },

    /// MCP server errors
    #[error("MCP error: {message}")]
    Mcp { message: String },

    /// File parsing errors
    #[error("Parse error: {path} - {message}")]
    Parse {
        path: PathBuf,
        message: String,
        source: Option<anyhow::Error>,
    },

    /// Validation errors
    #[error("Validation error: {message}")]
    Validation { message: String },
}

impl CodeSearchError {
    /// Create a database error
    pub fn database(message: impl Into<String>) -> Self {
        Self::Database {
            message: message.into(),
            source: None,
        }
    }

    /// Create an I/O error
    pub fn io(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self::Io {
            path: path.into(),
            message: message.into(),
            source: None,
        }
    }

    /// Create an embedding error
    pub fn embedding(message: impl Into<String>) -> Self {
        Self::Embedding {
            message: message.into(),
        }
    }

    /// Create a search error
    pub fn search(message: impl Into<String>) -> Self {
        Self::Search {
            message: message.into(),
        }
    }

    /// Create an index error
    pub fn index(message: impl Into<String>) -> Self {
        Self::Index {
            message: message.into(),
        }
    }

    /// Create a configuration error
    pub fn config(message: impl Into<String>) -> Self {
        Self::Config {
            message: message.into(),
        }
    }

    /// Create an MCP error
    pub fn mcp(message: impl Into<String>) -> Self {
        Self::Mcp {
            message: message.into(),
        }
    }

    /// Create a parse error
    pub fn parse(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self::Parse {
            path: path.into(),
            message: message.into(),
            source: None,
        }
    }

    /// Create a validation error
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation {
            message: message.into(),
        }
    }
}

/// Result type alias for codesearch operations
pub type Result<T> = std::result::Result<T, CodeSearchError>;

// Conversion from std::io::Error
impl From<std::io::Error> for CodeSearchError {
    fn from(err: std::io::Error) -> Self {
        Self::Io {
            path: PathBuf::new(),
            message: err.to_string(),
            source: None,
        }
    }
}

// Conversion from anyhow::Error (for gradual migration)
impl From<anyhow::Error> for CodeSearchError {
    fn from(err: anyhow::Error) -> Self {
        Self::Database {
            message: err.to_string(),
            source: Some(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = CodeSearchError::database("Test error");
        assert!(err.to_string().contains("Database error"));

        let err = CodeSearchError::validation("Invalid input");
        assert!(err.to_string().contains("Validation error"));
    }

    #[test]
    fn test_io_error() {
        let path = PathBuf::from("/test/path");
        let err = CodeSearchError::io(&path, "File not found");
        assert!(err.to_string().contains("I/O error"));
        assert!(err.to_string().contains("/test/path"));
    }

    #[test]
    fn test_embedding_error() {
        let err = CodeSearchError::embedding("Model not loaded");
        assert!(err.to_string().contains("Embedding error"));
        assert!(err.to_string().contains("Model not loaded"));
    }

    #[test]
    fn test_search_error() {
        let err = CodeSearchError::search("No index found");
        assert!(err.to_string().contains("Search error"));
    }

    #[test]
    fn test_index_error() {
        let err = CodeSearchError::index("Corrupted index");
        assert!(err.to_string().contains("Index error"));
    }

    #[test]
    fn test_config_error() {
        let err = CodeSearchError::config("Invalid config key");
        assert!(err.to_string().contains("Configuration error"));
    }

    #[test]
    fn test_mcp_error() {
        let err = CodeSearchError::mcp("Connection refused");
        assert!(err.to_string().contains("MCP error"));
    }

    #[test]
    fn test_parse_error() {
        let err = CodeSearchError::parse("/src/main.rs", "Syntax error at line 5");
        assert!(err.to_string().contains("Parse error"));
        assert!(err.to_string().contains("/src/main.rs"));
        assert!(err.to_string().contains("Syntax error"));
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let cs_err: CodeSearchError = io_err.into();
        assert!(cs_err.to_string().contains("I/O error"));
        assert!(cs_err.to_string().contains("file not found"));
    }

    #[test]
    fn test_from_anyhow_error() {
        let anyhow_err = anyhow::anyhow!("something went wrong");
        let cs_err: CodeSearchError = anyhow_err.into();
        assert!(cs_err.to_string().contains("Database error"));
        assert!(cs_err.to_string().contains("something went wrong"));
    }

    #[test]
    fn test_error_is_debug() {
        let err = CodeSearchError::database("test");
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("Database"));
    }

    #[test]
    fn test_result_type_alias() {
        fn returns_ok() -> Result<i32> {
            Ok(42)
        }
        fn returns_err() -> Result<i32> {
            Err(CodeSearchError::validation("bad"))
        }
        assert!(returns_ok().is_ok());
        assert!(returns_err().is_err());
    }
}
