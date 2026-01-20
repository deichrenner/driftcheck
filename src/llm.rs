use crate::config::{Config, LlmConfig};
use crate::error::{DriftcheckError, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, warn};

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    temperature: f32,
}

#[derive(Debug, Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

pub struct LlmClient {
    client: reqwest::Client,
    config: LlmConfig,
    api_key: String,
}

impl LlmClient {
    pub fn new(config: &LlmConfig) -> Result<Self> {
        let api_key = Config::get_api_key()?;

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout))
            .build()
            .map_err(|e| DriftcheckError::LlmError(e.to_string()))?;

        Ok(Self {
            client,
            config: config.clone(),
            api_key,
        })
    }

    pub async fn chat(&self, system_prompt: &str, user_message: &str) -> Result<String> {
        let url = format!("{}/chat/completions", self.config.base_url.trim_end_matches('/'));

        debug!("LLM request to: {}", url);
        debug!("LLM model: {}", self.config.model);
        debug!("System prompt: {}", &system_prompt);
        debug!("User message: {}", &user_message);
        debug!("User message length: {} chars", user_message.len());

        let request = ChatRequest {
            model: self.config.model.clone(),
            messages: vec![
                Message {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                Message {
                    role: "user".to_string(),
                    content: user_message.to_string(),
                },
            ],
            temperature: 0.1,
        };

        let mut last_error = None;

        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                let delay = Duration::from_millis(500 * 2u64.pow(attempt - 1));
                debug!("Retrying LLM request after {:?}", delay);
                tokio::time::sleep(delay).await;
            }

            match self.make_request(&url, &request).await {
                Ok(response) => {
                    debug!("LLM response: {}", &response[..response.len().min(500)]);
                    return Ok(response);
                }
                Err(e) => {
                    warn!("LLM request attempt {} failed: {}", attempt + 1, e);
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| DriftcheckError::LlmError("Unknown error".to_string())))
    }

    async fn make_request(&self, url: &str, request: &ChatRequest) -> Result<String> {
        let response = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    DriftcheckError::LlmTimeout(self.config.timeout)
                } else {
                    DriftcheckError::LlmError(e.to_string())
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(DriftcheckError::LlmError(format!(
                "HTTP {}: {}",
                status, body
            )));
        }

        let chat_response: ChatResponse = response
            .json()
            .await
            .map_err(|e| DriftcheckError::LlmResponseParse(e.to_string()))?;

        chat_response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .ok_or_else(|| DriftcheckError::LlmResponseParse("No response choices".to_string()))
    }
}

/// Generate search queries from a diff
pub async fn generate_search_queries(config: &Config, diff: &str) -> Result<Vec<String>> {
    let client = LlmClient::new(&config.llm)?;

    let response = client
        .chat(&config.prompts.search_queries, diff)
        .await?;

    // Parse JSON array of queries
    parse_search_queries(&response)
}

fn parse_search_queries(response: &str) -> Result<Vec<String>> {
    // Try to find JSON array in the response
    let response = response.trim();

    // Find the start of the JSON array
    let start = response.find('[').ok_or_else(|| {
        DriftcheckError::LlmResponseParse("No JSON array found in response".to_string())
    })?;

    // Find the matching end bracket
    let end = response.rfind(']').ok_or_else(|| {
        DriftcheckError::LlmResponseParse("No closing bracket found".to_string())
    })?;

    let json_str = &response[start..=end];

    let queries: Vec<String> = serde_json::from_str(json_str)
        .map_err(|e| DriftcheckError::LlmResponseParse(e.to_string()))?;

    Ok(queries)
}

/// Analyze consistency between diff and documentation
pub async fn analyze_consistency(
    config: &Config,
    diff: &str,
    doc_chunks: &[DocChunk],
) -> Result<Vec<RawIssue>> {
    if doc_chunks.is_empty() {
        return Ok(vec![]);
    }

    let client = LlmClient::new(&config.llm)?;

    // Format doc chunks for the prompt
    let docs_context = doc_chunks
        .iter()
        .map(|c| format!("--- {} (lines {}-{}) ---\n{}", c.file, c.start_line, c.end_line, c.content))
        .collect::<Vec<_>>()
        .join("\n\n");

    let user_message = format!(
        "## Code Diff (changes being pushed)\n```diff\n{}\n```\n\n## Documentation Excerpts\n{}",
        diff, docs_context
    );

    let response = client.chat(&config.prompts.analysis, &user_message).await?;

    parse_issues(&response)
}

fn parse_issues(response: &str) -> Result<Vec<RawIssue>> {
    let response = response.trim();

    // Try to find JSON array in the response
    let start = match response.find('[') {
        Some(s) => s,
        None => {
            // No JSON array means no issues found
            if response.to_lowercase().contains("no issues")
                || response.to_lowercase().contains("no documentation")
            {
                return Ok(vec![]);
            }
            return Err(DriftcheckError::LlmResponseParse(
                "Could not parse issues from response".to_string(),
            ));
        }
    };

    let end = response.rfind(']').ok_or_else(|| {
        DriftcheckError::LlmResponseParse("No closing bracket found".to_string())
    })?;

    let json_str = &response[start..=end];

    // Handle empty array
    if json_str.trim() == "[]" {
        return Ok(vec![]);
    }

    let issues: Vec<RawIssue> = serde_json::from_str(json_str)
        .map_err(|e| DriftcheckError::LlmResponseParse(format!("Failed to parse issues: {}", e)))?;

    Ok(issues)
}

#[derive(Debug, Clone)]
pub struct DocChunk {
    pub file: String,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawIssue {
    pub file: String,
    #[serde(default)]
    pub line: usize,
    pub description: String,
    #[serde(default)]
    pub doc_excerpt: String,
    pub suggested_fix: Option<String>,
}
