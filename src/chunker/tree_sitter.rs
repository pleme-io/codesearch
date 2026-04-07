#![allow(dead_code)]

use super::{Chunk, ChunkKind, Chunker};
use crate::cache::normalize_path;
use anyhow::Result;
use std::path::Path;

/// Smart chunker using tree-sitter for semantic boundaries
pub struct TreeSitterChunker {
    max_chunk_lines: usize,
    #[allow(dead_code)]
    max_chunk_chars: usize,
    overlap_lines: usize,
}

impl TreeSitterChunker {
    pub fn new(max_chunk_lines: usize, max_chunk_chars: usize, overlap_lines: usize) -> Self {
        Self {
            max_chunk_lines,
            max_chunk_chars,
            overlap_lines,
        }
    }
}

impl Chunker for TreeSitterChunker {
    fn chunk_file(&self, path: &Path, content: &str) -> Result<Vec<Chunk>> {
        // TODO: Implement tree-sitter based chunking
        // For now, use fallback chunking
        Ok(fallback_chunk(
            path,
            content,
            self.max_chunk_lines,
            self.overlap_lines,
        ))
    }
}

/// Fallback chunking strategy (sliding window)
fn fallback_chunk(
    path: &Path,
    content: &str,
    max_chunk_lines: usize,
    overlap_lines: usize,
) -> Vec<Chunk> {
    let lines: Vec<&str> = content.lines().collect();
    let mut chunks = Vec::new();
    let stride = max_chunk_lines.saturating_sub(overlap_lines).max(1);

    let path_str = normalize_path(path);
    let context = vec![format!("File: {}", path_str)];

    let mut i = 0;
    while i < lines.len() {
        let end = (i + max_chunk_lines).min(lines.len());
        let chunk_lines = &lines[i..end];

        if !chunk_lines.is_empty() {
            let content = chunk_lines.join("\n");
            let mut chunk = Chunk::new(content, i, end, ChunkKind::Block, path_str.clone());
            chunk.context = context.clone();
            chunks.push(chunk);
        }

        i += stride;
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree_sitter_chunker_new() {
        let chunker = TreeSitterChunker::new(50, 2000, 5);
        assert_eq!(chunker.max_chunk_lines, 50);
        assert_eq!(chunker.max_chunk_chars, 2000);
        assert_eq!(chunker.overlap_lines, 5);
    }

    #[test]
    fn test_fallback_chunk_basic() {
        let path = Path::new("test.rs");
        let content = "line 1\nline 2\nline 3\nline 4\nline 5";
        let chunks = fallback_chunk(path, content, 3, 1);

        assert!(!chunks.is_empty());
        assert_eq!(chunks[0].start_line, 0);
        assert_eq!(chunks[0].end_line, 3);
        assert_eq!(chunks[0].kind, ChunkKind::Block);
    }

    #[test]
    fn test_fallback_chunk_empty_content() {
        let path = Path::new("empty.rs");
        let chunks = fallback_chunk(path, "", 10, 2);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_fallback_chunk_single_line() {
        let path = Path::new("single.rs");
        let chunks = fallback_chunk(path, "fn main() {}", 10, 2);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].content, "fn main() {}");
        assert_eq!(chunks[0].start_line, 0);
        assert_eq!(chunks[0].end_line, 1);
    }

    #[test]
    fn test_fallback_chunk_overlap() {
        let path = Path::new("overlap.rs");
        let content = (0..10).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
        let chunks = fallback_chunk(path, &content, 4, 2);

        assert!(chunks.len() > 1);
        // With max_chunk_lines=4 and overlap=2, stride=2
        // Chunk 0: lines 0-3, Chunk 1: lines 2-5, etc.
        assert_eq!(chunks[0].start_line, 0);
        assert_eq!(chunks[0].end_line, 4);
        assert_eq!(chunks[1].start_line, 2);
        assert_eq!(chunks[1].end_line, 6);
    }

    #[test]
    fn test_fallback_chunk_stride_minimum_one() {
        let path = Path::new("stride.rs");
        let content = "a\nb\nc";
        // overlap_lines >= max_chunk_lines would make stride 0, but .max(1) prevents that
        let chunks = fallback_chunk(path, content, 2, 5);
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_fallback_chunk_context_set() {
        let path = Path::new("src/main.rs");
        let chunks = fallback_chunk(path, "fn main() {}\n", 10, 2);

        assert!(!chunks.is_empty());
        assert!(!chunks[0].context.is_empty());
        assert!(chunks[0].context[0].contains("main.rs"));
    }

    #[test]
    fn test_tree_sitter_chunker_chunk_file() {
        let chunker = TreeSitterChunker::new(5, 500, 1);
        let path = Path::new("test.rs");
        let content = "fn main() {\n    println!(\"hello\");\n}\n\nfn other() {\n    // nothing\n}\n";

        let result = chunker.chunk_file(path, content);
        assert!(result.is_ok());
        let chunks = result.unwrap();
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_fallback_chunk_exact_boundary_no_overlap() {
        let path = Path::new("boundary.rs");
        // Exactly max_chunk_lines lines, 0 overlap → stride = max_chunk_lines
        let content = (0..5).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
        let chunks = fallback_chunk(path, &content, 5, 0);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].start_line, 0);
        assert_eq!(chunks[0].end_line, 5);
    }
}