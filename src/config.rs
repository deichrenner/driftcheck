use crate::error::{DriftcheckError, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_ANALYSIS_PROMPT: &str = r#"You are a strict documentation consistency reviewer. Your job is to find ONLY clear, obvious documentation errors caused by code changes.

ONLY report an issue if:
1. Documentation explicitly states something that is NOW FACTUALLY WRONG due to the code change
2. A code example in the docs would NOW FAIL or produce different results
3. A function signature, parameter, or return type documented is NOW DIFFERENT in the code

DO NOT report:
- Stylistic improvements or suggestions
- Documentation that is vague but not technically wrong
- Potential improvements or clarifications
- Anything where the docs are still technically accurate
- Issues that appear to have been ALREADY FIXED in recent commits (check the git log provided)

IMPORTANT: Review the recent commits section. If a documentation file was recently modified, assume the developer has already addressed an issues in that file. Only flag issues for files that were updated in recent commits unless you can see the docs are STILL wrong.

Be conservative. When in doubt, think twice. False positives waste developer time.

If there are no clear issues, return an empty array: []

Output as JSON array with objects containing:
- "file": the documentation file path
- "line": approximate line number (0 if unknown)
- "description": what is FACTUALLY WRONG (be specific)
- "doc_excerpt": the exact doc text that is wrong
- "suggested_fix": minimal fix (optional)"#;

const DEFAULT_SEARCH_QUERIES_PROMPT: &str = r#"Given this code diff, output a JSON array of search patterns to find related documentation.
Focus on: function names, class names, API endpoints, CLI flags, config keys, error messages.
Output ONLY valid JSON, no explanation. Example: ["process_data", "API endpoint", "--verbose"]"#;

const DEFAULT_SUGGESTIONS_PROMPT: &str = r#"Given the documentation issue identified, suggest a minimal fix.
Output as a unified diff patch that can be applied with `patch -p1`."#;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub docs: DocsConfig,
    #[serde(default)]
    pub llm: LlmConfig,
    #[serde(default)]
    pub prompts: PromptsConfig,
    #[serde(default)]
    pub tui: TuiConfig,
    #[serde(default)]
    pub cache: CacheConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub allow_push_on_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocsConfig {
    #[serde(default = "default_doc_paths")]
    pub paths: Vec<String>,
    #[serde(default)]
    pub ignore: Vec<String>,
    #[serde(default = "default_max_context_tokens")]
    pub max_context_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptsConfig {
    #[serde(default = "default_analysis_prompt")]
    pub analysis: String,
    #[serde(default = "default_search_queries_prompt")]
    pub search_queries: String,
    #[serde(default = "default_suggestions_prompt")]
    pub suggestions: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiConfig {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_true")]
    pub show_diff_preview: bool,
    #[serde(default)]
    pub auto_apply: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_cache_dir")]
    pub dir: String,
    #[serde(default = "default_ttl")]
    pub ttl: u64,
}

// Default value functions
fn default_true() -> bool {
    true
}

fn default_doc_paths() -> Vec<String> {
    vec!["README.md".to_string(), "docs/**/*.md".to_string()]
}

fn default_max_context_tokens() -> usize {
    8000
}

fn default_base_url() -> String {
    "https://api.openai.com/v1".to_string()
}

fn default_model() -> String {
    "gpt-4o".to_string()
}

fn default_timeout() -> u64 {
    30
}

fn default_max_retries() -> u32 {
    2
}

fn default_analysis_prompt() -> String {
    DEFAULT_ANALYSIS_PROMPT.to_string()
}

fn default_search_queries_prompt() -> String {
    DEFAULT_SEARCH_QUERIES_PROMPT.to_string()
}

fn default_suggestions_prompt() -> String {
    DEFAULT_SUGGESTIONS_PROMPT.to_string()
}

fn default_theme() -> String {
    "default".to_string()
}

fn default_cache_dir() -> String {
    ".git/driftcheck_cache".to_string()
}

fn default_ttl() -> u64 {
    3600
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            allow_push_on_error: false,
        }
    }
}

impl Default for DocsConfig {
    fn default() -> Self {
        Self {
            paths: default_doc_paths(),
            ignore: vec![],
            max_context_tokens: default_max_context_tokens(),
        }
    }
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            base_url: default_base_url(),
            model: default_model(),
            timeout: default_timeout(),
            max_retries: default_max_retries(),
        }
    }
}

impl Default for PromptsConfig {
    fn default() -> Self {
        Self {
            analysis: default_analysis_prompt(),
            search_queries: default_search_queries_prompt(),
            suggestions: default_suggestions_prompt(),
        }
    }
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            show_diff_preview: true,
            auto_apply: false,
        }
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            dir: default_cache_dir(),
            ttl: default_ttl(),
        }
    }
}

impl Config {
    /// Find and load the configuration file.
    /// Searches in order: DRIFTCHECK_CONFIG env var, .driftcheck.toml, driftcheck.toml
    pub fn load() -> Result<Self> {
        let path = Self::find_config_path()?;
        Self::load_from_path(&path)
    }

    /// Load configuration from a specific path
    pub fn load_from_path(path: &Path) -> Result<Self> {
        let contents = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&contents)?;
        Ok(config)
    }

    /// Find the configuration file path
    pub fn find_config_path() -> Result<PathBuf> {
        // Check environment variable first
        if let Ok(path) = env::var("DRIFTCHECK_CONFIG") {
            let path = PathBuf::from(path);
            if path.exists() {
                return Ok(path);
            }
        }

        // Find git root
        let git_root = Self::find_git_root()?;

        // Check .driftcheck.toml
        let dotfile = git_root.join(".driftcheck.toml");
        if dotfile.exists() {
            return Ok(dotfile);
        }

        // Check driftcheck.toml
        let regular = git_root.join("driftcheck.toml");
        if regular.exists() {
            return Ok(regular);
        }

        Err(DriftcheckError::ConfigNotFound)
    }

    /// Find the git repository root
    pub fn find_git_root() -> Result<PathBuf> {
        let current = env::current_dir()?;
        let mut path = current.as_path();

        loop {
            if path.join(".git").exists() {
                return Ok(path.to_path_buf());
            }

            match path.parent() {
                Some(parent) => path = parent,
                None => return Err(DriftcheckError::NotGitRepo),
            }
        }
    }

    /// Check if driftcheck is enabled (config + env var)
    pub fn is_enabled(&self) -> bool {
        if env::var("DRIFTCHECK_DISABLED").map(|v| v == "1").unwrap_or(false) {
            return false;
        }
        self.general.enabled
    }

    /// Get the API key from environment or file
    /// Checks in order:
    /// 1. DRIFTCHECK_API_KEY env var
    /// 2. DRIFTCHECK_API_KEY_FILE env var (reads key from file path)
    pub fn get_api_key() -> Result<String> {
        // First try direct env var
        if let Ok(key) = env::var("DRIFTCHECK_API_KEY") {
            return Ok(key);
        }

        // Then try reading from file
        if let Ok(path) = env::var("DRIFTCHECK_API_KEY_FILE") {
            return fs::read_to_string(&path)
                .map(|s| s.trim().to_string())
                .map_err(|_| DriftcheckError::ApiKeyNotFound);
        }

        Err(DriftcheckError::ApiKeyNotFound)
    }

    /// Check if debug mode is enabled
    pub fn is_debug() -> bool {
        env::var("DRIFTCHECK_DEBUG").map(|v| v == "1").unwrap_or(false)
    }

    /// Save the configuration to the default path
    pub fn save(&self) -> Result<()> {
        let git_root = Self::find_git_root()?;
        let path = git_root.join(".driftcheck.toml");
        self.save_to_path(&path)
    }

    /// Save the configuration to a specific path
    pub fn save_to_path(&self, path: &Path) -> Result<()> {
        let contents = toml::to_string_pretty(self)
            .map_err(|e| DriftcheckError::ConfigInvalid(e.to_string()))?;
        fs::write(path, contents)?;
        Ok(())
    }
}
