use std::path::Path;

/// Supported programming languages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Java,
    C,
    Cpp,
    CSharp,
    Ruby,
    Php,
    Swift,
    Kotlin,
    Shell,
    Markdown,
    Json,
    Yaml,
    Toml,
    Sql,
    Html,
    Css,
    Unknown,
}

impl Language {
    /// Detect language from file extension
    pub fn from_path(path: &Path) -> Self {
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        Self::from_extension(extension)
    }

    /// Detect language from extension string
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "rs" => Self::Rust,
            "py" | "pyw" | "pyi" => Self::Python,
            "js" | "mjs" | "cjs" => Self::JavaScript,
            "ts" | "mts" | "cts" => Self::TypeScript,
            "tsx" | "jsx" => Self::TypeScript, // Treat JSX/TSX as TypeScript
            "go" => Self::Go,
            "java" => Self::Java,
            "c" | "h" => Self::C,
            "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Self::Cpp,
            "cs" => Self::CSharp,
            "rb" | "rake" => Self::Ruby,
            "php" => Self::Php,
            "swift" => Self::Swift,
            "kt" | "kts" => Self::Kotlin,
            "sh" | "bash" | "zsh" => Self::Shell,
            "md" | "markdown" | "txt" => Self::Markdown, // Treat txt as markdown-like
            "json" => Self::Json,
            "yaml" | "yml" => Self::Yaml,
            "toml" => Self::Toml,
            "sql" => Self::Sql,
            "html" | "htm" => Self::Html,
            "css" | "scss" | "sass" | "less" => Self::Css,
            _ => Self::Unknown,
        }
    }

    /// Check if this language is supported for semantic chunking
    #[allow(dead_code)] // Reserved for tree-sitter chunking feature
    pub fn supports_tree_sitter(&self) -> bool {
        matches!(
            self,
            Self::Rust
                | Self::Python
                | Self::JavaScript
                | Self::TypeScript
                | Self::C
                | Self::Cpp
                | Self::CSharp
                | Self::Go
                | Self::Java
        )
    }

    /// Check if this is a text-based language (should be indexed)
    pub fn is_indexable(&self) -> bool {
        !matches!(self, Self::Unknown)
    }

    /// Get the language name as a string
    pub fn name(&self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Python => "Python",
            Self::JavaScript => "JavaScript",
            Self::TypeScript => "TypeScript",
            Self::Go => "Go",
            Self::Java => "Java",
            Self::C => "C",
            Self::Cpp => "C++",
            Self::CSharp => "C#",
            Self::Ruby => "Ruby",
            Self::Php => "PHP",
            Self::Swift => "Swift",
            Self::Kotlin => "Kotlin",
            Self::Shell => "Shell",
            Self::Markdown => "Markdown",
            Self::Json => "JSON",
            Self::Yaml => "YAML",
            Self::Toml => "TOML",
            Self::Sql => "SQL",
            Self::Html => "HTML",
            Self::Css => "CSS",
            Self::Unknown => "Unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_rust_detection() {
        assert_eq!(Language::from_extension("rs"), Language::Rust);
        assert_eq!(
            Language::from_path(&PathBuf::from("main.rs")),
            Language::Rust
        );
    }

    #[test]
    fn test_python_detection() {
        assert_eq!(Language::from_extension("py"), Language::Python);
        assert_eq!(Language::from_extension("pyi"), Language::Python);
    }

    #[test]
    fn test_typescript_detection() {
        assert_eq!(Language::from_extension("ts"), Language::TypeScript);
        assert_eq!(Language::from_extension("tsx"), Language::TypeScript);
        assert_eq!(Language::from_extension("jsx"), Language::TypeScript);
    }

    #[test]
    fn test_tree_sitter_support() {
        assert!(Language::Rust.supports_tree_sitter());
        assert!(Language::Python.supports_tree_sitter());
        assert!(Language::TypeScript.supports_tree_sitter());
        assert!(!Language::Markdown.supports_tree_sitter());
        assert!(!Language::Json.supports_tree_sitter());
    }

    #[test]
    fn test_indexable() {
        assert!(Language::Rust.is_indexable());
        assert!(Language::Markdown.is_indexable());
        assert!(!Language::Unknown.is_indexable());
    }

    #[test]
    fn test_case_insensitive_extension() {
        assert_eq!(Language::from_extension("RS"), Language::Rust);
        assert_eq!(Language::from_extension("PY"), Language::Python);
        assert_eq!(Language::from_extension("Js"), Language::JavaScript);
    }

    #[test]
    fn test_unknown_extension() {
        assert_eq!(Language::from_extension("xyz"), Language::Unknown);
        assert_eq!(Language::from_extension(""), Language::Unknown);
    }

    #[test]
    fn test_all_language_names() {
        assert_eq!(Language::Rust.name(), "Rust");
        assert_eq!(Language::Python.name(), "Python");
        assert_eq!(Language::JavaScript.name(), "JavaScript");
        assert_eq!(Language::TypeScript.name(), "TypeScript");
        assert_eq!(Language::Go.name(), "Go");
        assert_eq!(Language::Java.name(), "Java");
        assert_eq!(Language::C.name(), "C");
        assert_eq!(Language::Cpp.name(), "C++");
        assert_eq!(Language::CSharp.name(), "C#");
        assert_eq!(Language::Ruby.name(), "Ruby");
        assert_eq!(Language::Php.name(), "PHP");
        assert_eq!(Language::Swift.name(), "Swift");
        assert_eq!(Language::Kotlin.name(), "Kotlin");
        assert_eq!(Language::Shell.name(), "Shell");
        assert_eq!(Language::Markdown.name(), "Markdown");
        assert_eq!(Language::Json.name(), "JSON");
        assert_eq!(Language::Yaml.name(), "YAML");
        assert_eq!(Language::Toml.name(), "TOML");
        assert_eq!(Language::Sql.name(), "SQL");
        assert_eq!(Language::Html.name(), "HTML");
        assert_eq!(Language::Css.name(), "CSS");
        assert_eq!(Language::Unknown.name(), "Unknown");
    }

    #[test]
    fn test_from_path_no_extension() {
        assert_eq!(Language::from_path(&PathBuf::from("Makefile")), Language::Unknown);
    }

    #[test]
    fn test_cpp_variants() {
        assert_eq!(Language::from_extension("cpp"), Language::Cpp);
        assert_eq!(Language::from_extension("cc"), Language::Cpp);
        assert_eq!(Language::from_extension("cxx"), Language::Cpp);
        assert_eq!(Language::from_extension("hpp"), Language::Cpp);
        assert_eq!(Language::from_extension("hxx"), Language::Cpp);
    }

    #[test]
    fn test_css_variants() {
        assert_eq!(Language::from_extension("css"), Language::Css);
        assert_eq!(Language::from_extension("scss"), Language::Css);
        assert_eq!(Language::from_extension("sass"), Language::Css);
        assert_eq!(Language::from_extension("less"), Language::Css);
    }

    #[test]
    fn test_shell_variants() {
        assert_eq!(Language::from_extension("sh"), Language::Shell);
        assert_eq!(Language::from_extension("bash"), Language::Shell);
        assert_eq!(Language::from_extension("zsh"), Language::Shell);
    }

    #[test]
    fn test_yaml_variants() {
        assert_eq!(Language::from_extension("yaml"), Language::Yaml);
        assert_eq!(Language::from_extension("yml"), Language::Yaml);
    }

    #[test]
    fn test_all_tree_sitter_languages() {
        let supported = [
            Language::Rust, Language::Python, Language::JavaScript,
            Language::TypeScript, Language::C, Language::Cpp,
            Language::CSharp, Language::Go, Language::Java,
        ];
        for lang in &supported {
            assert!(lang.supports_tree_sitter(), "{:?} should support tree-sitter", lang);
        }

        let unsupported = [
            Language::Ruby, Language::Php, Language::Swift, Language::Kotlin,
            Language::Shell, Language::Markdown, Language::Json, Language::Yaml,
            Language::Toml, Language::Sql, Language::Html, Language::Css,
            Language::Unknown,
        ];
        for lang in &unsupported {
            assert!(!lang.supports_tree_sitter(), "{:?} should not support tree-sitter", lang);
        }
    }

    #[test]
    fn test_language_equality() {
        assert_eq!(Language::Rust, Language::Rust);
        assert_ne!(Language::Rust, Language::Python);
    }

    #[test]
    fn test_language_clone() {
        let lang = Language::TypeScript;
        let cloned = lang;
        assert_eq!(lang, cloned);
    }
}
