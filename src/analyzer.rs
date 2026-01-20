use crate::cache;
use crate::config::Config;
use crate::error::Result;
use crate::git::ParsedDiff;
use crate::llm::{self, RawIssue};
use crate::progress::MultiProgress;
use crate::search;
use std::path::PathBuf;
use tracing::{debug, info};

/// An issue detected by the analysis
#[derive(Debug, Clone)]
pub struct Issue {
    pub file: PathBuf,
    pub line: usize,
    pub description: String,
    pub doc_excerpt: String,
    pub suggested_fix: Option<String>,
}

impl From<RawIssue> for Issue {
    fn from(raw: RawIssue) -> Self {
        Self {
            file: PathBuf::from(&raw.file),
            line: raw.line,
            description: raw.description,
            doc_excerpt: raw.doc_excerpt,
            suggested_fix: raw.suggested_fix,
        }
    }
}

/// Run the full analysis pipeline
pub async fn analyze(config: &Config, diff: &str) -> Result<Vec<Issue>> {
    // Parse the diff
    let parsed = ParsedDiff::parse(diff);

    if parsed.files.is_empty() {
        debug!("No files changed in diff");
        return Ok(vec![]);
    }

    info!("Analyzing changes to {} files", parsed.files.len());

    // Set up progress indicator
    let mut progress = MultiProgress::new(vec![
        "Generating search queries",
        "Searching documentation",
        "Analyzing consistency",
    ]);

    // Step 1: Generate search queries
    progress.next_step();

    let queries = if config.cache.enabled {
        match cache::get_queries(diff) {
            Some(cached) => {
                debug!("Using cached search queries");
                progress.update("using cache");
                cached
            }
            None => {
                debug!("Generating new search queries");
                let queries = llm::generate_search_queries(config, diff).await?;

                // Cache the queries
                if let Err(e) = cache::store_queries(diff, &queries) {
                    debug!("Failed to cache queries: {}", e);
                }

                queries
            }
        }
    } else {
        llm::generate_search_queries(config, diff).await?
    };

    if queries.is_empty() {
        debug!("No search queries generated");
        progress.finish();
        return Ok(vec![]);
    }

    info!("Generated {} search queries", queries.len());

    // Step 2: Search documentation
    progress.next_step();
    progress.update(&format!("{} queries", queries.len()));

    let doc_chunks = search::find_relevant_docs(&config.docs, &queries).await?;

    if doc_chunks.is_empty() {
        debug!("No relevant documentation found");
        progress.finish();
        return Ok(vec![]);
    }

    info!("Found {} documentation chunks", doc_chunks.len());

    // Truncate if over token budget
    let doc_chunks = truncate_to_budget(doc_chunks, config.docs.max_context_tokens);

    // Step 3: Analyze consistency
    progress.next_step();
    progress.update(&format!("{} doc chunks", doc_chunks.len()));

    let raw_issues = llm::analyze_consistency(config, diff, &doc_chunks).await?;

    progress.finish();

    if raw_issues.is_empty() {
        return Ok(vec![]);
    }

    info!("Found {} potential issues", raw_issues.len());

    // Convert to Issue structs
    let issues: Vec<Issue> = raw_issues.into_iter().map(Issue::from).collect();

    Ok(issues)
}

/// Truncate document chunks to fit within token budget
fn truncate_to_budget(mut chunks: Vec<llm::DocChunk>, max_tokens: usize) -> Vec<llm::DocChunk> {
    // Rough estimate: 4 chars per token
    let chars_budget = max_tokens * 4;
    let mut total_chars = 0;
    let mut result = Vec::new();

    // Sort by relevance (for now, just by size - smaller chunks are more focused)
    chunks.sort_by_key(|c| c.content.len());

    for chunk in chunks {
        let chunk_chars = chunk.content.len();
        if total_chars + chunk_chars > chars_budget {
            // Truncate this chunk if it's the first one
            if result.is_empty() {
                let truncated_content = chunk.content.chars().take(chars_budget).collect();
                result.push(llm::DocChunk {
                    content: truncated_content,
                    ..chunk
                });
            }
            break;
        }
        total_chars += chunk_chars;
        result.push(chunk);
    }

    result
}

