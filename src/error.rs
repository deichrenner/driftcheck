use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DocguardError {
    #[error("Configuration file not found. Run 'docguard init' to create one.")]
    ConfigNotFound,

    #[error("Invalid configuration: {0}")]
    ConfigInvalid(String),

    #[error("Failed to read configuration file: {0}")]
    ConfigRead(#[from] std::io::Error),

    #[error("Failed to parse configuration: {0}")]
    ConfigParse(#[from] toml::de::Error),

    #[error("Not a git repository (or any parent up to mount point)")]
    NotGitRepo,

    #[error("Git command failed: {0}")]
    GitError(String),

    #[error("No upstream branch configured. Run 'git push -u origin <branch>' first.")]
    NoUpstream,

    #[error("ripgrep (rg) not found. Please install it: https://github.com/BurntSushi/ripgrep#installation")]
    RipgrepNotFound,

    #[error("Search failed: {0}")]
    SearchError(String),

    #[error("LLM API error: {0}")]
    LlmError(String),

    #[error("LLM request timed out after {0} seconds")]
    LlmTimeout(u64),

    #[error("API key not found. Set DOCGUARD_API_KEY environment variable.")]
    ApiKeyNotFound,

    #[error("Failed to parse LLM response: {0}")]
    LlmResponseParse(String),

    #[error("Cache error: {0}")]
    CacheError(String),

    #[error("Hook installation failed: {0}")]
    HookInstallError(String),

    #[error("Failed to apply fix to {path}: {reason}")]
    FixApplicationError { path: PathBuf, reason: String },

    #[error("TUI error: {0}")]
    TuiError(String),

    #[error("docguard is disabled. Run 'docguard enable' to re-enable.")]
    Disabled,
}

pub type Result<T> = std::result::Result<T, DocguardError>;
