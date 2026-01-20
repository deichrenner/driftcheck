use crate::analyzer::Issue;
use crate::config::Config;
use crate::error::{DocguardError, Result};
use crate::llm;
use std::env;
use std::fs;
use std::process::Command;

/// Apply a fix to an issue
pub async fn apply_fix(config: &Config, issue: &Issue) -> Result<()> {
    // Read the current file content
    let content = fs::read_to_string(&issue.file).map_err(|e| DocguardError::FixApplicationError {
        path: issue.file.clone(),
        reason: e.to_string(),
    })?;

    // Generate a patch using LLM
    let raw_issue = llm::RawIssue {
        file: issue.file.to_string_lossy().to_string(),
        line: issue.line,
        description: issue.description.clone(),
        doc_excerpt: issue.doc_excerpt.clone(),
        suggested_fix: issue.suggested_fix.clone(),
    };

    let patch = llm::generate_fix(config, &raw_issue, &content).await?;

    // Try to apply the patch
    apply_patch(&issue.file.to_string_lossy(), &patch)?;

    Ok(())
}

/// Apply a unified diff patch
fn apply_patch(file: &str, patch: &str) -> Result<()> {
    // Write patch to temp file
    let temp_dir = env::temp_dir();
    let patch_file = temp_dir.join("docguard_patch.diff");

    fs::write(&patch_file, patch).map_err(|e| DocguardError::FixApplicationError {
        path: file.into(),
        reason: format!("Failed to write patch file: {}", e),
    })?;

    // Try to apply with patch command
    let output = Command::new("patch")
        .args(["-p1", "--forward", "--input"])
        .arg(&patch_file)
        .output()
        .map_err(|e| DocguardError::FixApplicationError {
            path: file.into(),
            reason: format!("Failed to run patch command: {}", e),
        })?;

    // Clean up temp file
    let _ = fs::remove_file(&patch_file);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DocguardError::FixApplicationError {
            path: file.into(),
            reason: format!("Patch failed: {}", stderr),
        });
    }

    Ok(())
}

/// Open a file in the user's editor at a specific line
pub fn open_in_editor(file: &str, line: usize) -> Result<()> {
    let editor = env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());

    // Most editors support +line syntax
    let line_arg = format!("+{}", line);

    let status = Command::new(&editor)
        .arg(&line_arg)
        .arg(file)
        .status()
        .map_err(|e| DocguardError::FixApplicationError {
            path: file.into(),
            reason: format!("Failed to open editor: {}", e),
        })?;

    if !status.success() {
        return Err(DocguardError::FixApplicationError {
            path: file.into(),
            reason: "Editor exited with error".to_string(),
        });
    }

    Ok(())
}

/// Parse a unified diff to extract changes
#[derive(Debug)]
pub struct DiffHunk {
    pub original_start: usize,
    pub original_count: usize,
    pub new_start: usize,
    pub new_count: usize,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug)]
pub enum DiffLine {
    Context(String),
    Add(String),
    Remove(String),
}

pub fn parse_unified_diff(diff: &str) -> Vec<DiffHunk> {
    let mut hunks = Vec::new();
    let mut current_hunk: Option<DiffHunk> = None;

    for line in diff.lines() {
        if line.starts_with("@@") {
            // Save previous hunk
            if let Some(hunk) = current_hunk.take() {
                hunks.push(hunk);
            }

            // Parse hunk header
            if let Some(hunk) = parse_hunk_header(line) {
                current_hunk = Some(hunk);
            }
        } else if let Some(ref mut hunk) = current_hunk {
            if line.starts_with('+') && !line.starts_with("+++") {
                hunk.lines.push(DiffLine::Add(line[1..].to_string()));
            } else if line.starts_with('-') && !line.starts_with("---") {
                hunk.lines.push(DiffLine::Remove(line[1..].to_string()));
            } else if line.starts_with(' ') || line.is_empty() {
                let content = if line.is_empty() {
                    String::new()
                } else {
                    line[1..].to_string()
                };
                hunk.lines.push(DiffLine::Context(content));
            }
        }
    }

    // Save last hunk
    if let Some(hunk) = current_hunk {
        hunks.push(hunk);
    }

    hunks
}

fn parse_hunk_header(line: &str) -> Option<DiffHunk> {
    // @@ -start,count +start,count @@
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 3 {
        return None;
    }

    let old_range = parts[1].trim_start_matches('-');
    let new_range = parts[2].trim_start_matches('+');

    let (original_start, original_count) = parse_range(old_range);
    let (new_start, new_count) = parse_range(new_range);

    Some(DiffHunk {
        original_start,
        original_count,
        new_start,
        new_count,
        lines: Vec::new(),
    })
}

fn parse_range(range: &str) -> (usize, usize) {
    let parts: Vec<&str> = range.split(',').collect();
    let start = parts[0].parse().unwrap_or(1);
    let count = if parts.len() > 1 {
        parts[1].parse().unwrap_or(1)
    } else {
        1
    };
    (start, count)
}
