use crate::config::DocsConfig;
use crate::error::{DocguardError, Result};
use crate::llm::DocChunk;
use glob::glob;
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;
use tracing::{debug, warn};

/// Check if ripgrep is installed
pub fn check_ripgrep() -> Result<()> {
    which::which("rg").map_err(|_| DocguardError::RipgrepNotFound)?;
    Ok(())
}

/// Find relevant documentation based on search queries
pub async fn find_relevant_docs(
    config: &DocsConfig,
    queries: &[String],
) -> Result<Vec<DocChunk>> {
    check_ripgrep()?;

    // Expand doc paths using glob
    let doc_files = expand_doc_paths(&config.paths, &config.ignore)?;

    if doc_files.is_empty() {
        debug!("No documentation files found");
        return Ok(vec![]);
    }

    debug!("Searching {} doc files with {} queries", doc_files.len(), queries.len());
    debug!("Doc files: {:?}", doc_files);
    debug!("Search queries: {:?}", queries);

    // Run searches in parallel
    let mut handles = Vec::new();

    for query in queries {
        let query = query.clone();
        let files = doc_files.clone();

        handles.push(tokio::spawn(async move {
            search_query(&query, &files)
        }));
    }

    // Collect results
    let mut all_chunks = Vec::new();
    let mut seen: HashSet<(String, usize)> = HashSet::new();

    for handle in handles {
        match handle.await {
            Ok(Ok(chunks)) => {
                for chunk in chunks {
                    // Deduplicate by file:line
                    let key = (chunk.file.clone(), chunk.start_line);
                    if seen.insert(key) {
                        all_chunks.push(chunk);
                    }
                }
            }
            Ok(Err(e)) => {
                warn!("Search query failed: {}", e);
            }
            Err(e) => {
                warn!("Search task panicked: {}", e);
            }
        }
    }

    // Sort by file and line
    all_chunks.sort_by(|a, b| {
        a.file.cmp(&b.file).then(a.start_line.cmp(&b.start_line))
    });

    // Merge adjacent chunks in the same file
    let merged = merge_adjacent_chunks(all_chunks);

    Ok(merged)
}

fn expand_doc_paths(paths: &[String], ignore: &[String]) -> Result<Vec<PathBuf>> {
    let mut files = HashSet::new();
    let mut ignore_patterns: HashSet<PathBuf> = HashSet::new();

    // Expand ignore patterns
    for pattern in ignore {
        if let Ok(matches) = glob(pattern) {
            for path in matches.flatten() {
                ignore_patterns.insert(path);
            }
        }
    }

    // Expand doc paths
    for pattern in paths {
        // Handle special :docstrings suffix (not supported in v1)
        let pattern = pattern.trim_end_matches(":docstrings");

        match glob(pattern) {
            Ok(matches) => {
                for path in matches.flatten() {
                    if path.is_file() && !ignore_patterns.contains(&path) {
                        files.insert(path);
                    }
                }
            }
            Err(e) => {
                warn!("Invalid glob pattern '{}': {}", pattern, e);
            }
        }
    }

    Ok(files.into_iter().collect())
}

fn search_query(query: &str, files: &[PathBuf]) -> Result<Vec<DocChunk>> {
    // Use ripgrep to search
    let file_args: Vec<String> = files.iter().map(|p| p.to_string_lossy().to_string()).collect();

    let output = Command::new("rg")
        .args([
            "--line-number",
            "--no-heading",
            "--color=never",
            "-C", "3",  // 3 lines of context
            "--",
            query,
        ])
        .args(&file_args)
        .output()
        .map_err(|e| DocguardError::SearchError(e.to_string()))?;

    // ripgrep returns exit code 1 if no matches (which is fine)
    if !output.status.success() && output.status.code() != Some(1) {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DocguardError::SearchError(stderr.to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_ripgrep_output(&stdout)
}

fn parse_ripgrep_output(output: &str) -> Result<Vec<DocChunk>> {
    let mut chunks = Vec::new();
    let mut current_file: Option<String> = None;
    let mut current_lines: Vec<(usize, String)> = Vec::new();

    for line in output.lines() {
        if line == "--" {
            // Separator between matches
            if let Some(file) = &current_file {
                if !current_lines.is_empty() {
                    chunks.push(create_chunk(file.clone(), &current_lines));
                    current_lines.clear();
                }
            }
            continue;
        }

        // Parse "file:line:content" or "file-line-content" (context lines)
        if let Some((file, line_num, content)) = parse_rg_line(line) {
            if current_file.as_ref() != Some(&file) {
                // New file
                if let Some(f) = &current_file {
                    if !current_lines.is_empty() {
                        chunks.push(create_chunk(f.clone(), &current_lines));
                        current_lines.clear();
                    }
                }
                current_file = Some(file);
            }
            current_lines.push((line_num, content));
        }
    }

    // Don't forget the last chunk
    if let Some(file) = current_file {
        if !current_lines.is_empty() {
            chunks.push(create_chunk(file, &current_lines));
        }
    }

    Ok(chunks)
}

fn parse_rg_line(line: &str) -> Option<(String, usize, String)> {
    // Format: file:linenum:content or file-linenum-content (for context lines)
    // Example: "README.md:10:Some content here"
    // Example: "README.md-8-context line"

    // Try to find pattern: path:number:content (match lines use :)
    if let Some((file, rest)) = split_at_line_number(line, ':') {
        if let Some((line_str, content)) = rest.split_once(':') {
            if let Ok(line_num) = line_str.parse::<usize>() {
                return Some((file, line_num, content.to_string()));
            }
        }
    }

    // Try pattern: path-number-content (context lines use -)
    if let Some((file, rest)) = split_at_line_number(line, '-') {
        if let Some((line_str, content)) = rest.split_once('-') {
            if let Ok(line_num) = line_str.parse::<usize>() {
                return Some((file, line_num, content.to_string()));
            }
        }
    }

    None
}

/// Split a line at the separator that precedes a line number
/// Returns (file_path, rest_of_line) where rest starts with the line number
fn split_at_line_number(line: &str, sep: char) -> Option<(String, &str)> {
    // Find separator followed by a digit
    let bytes = line.as_bytes();
    for (i, window) in bytes.windows(2).enumerate() {
        if window[0] == sep as u8 && window[1].is_ascii_digit() {
            let file = &line[..i];
            let rest = &line[i + 1..];
            return Some((file.to_string(), rest));
        }
    }
    None
}

fn create_chunk(file: String, lines: &[(usize, String)]) -> DocChunk {
    let start_line = lines.first().map(|(n, _)| *n).unwrap_or(1);
    let end_line = lines.last().map(|(n, _)| *n).unwrap_or(1);
    let content = lines
        .iter()
        .map(|(_, c)| c.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    DocChunk {
        file,
        start_line,
        end_line,
        content,
    }
}

fn merge_adjacent_chunks(chunks: Vec<DocChunk>) -> Vec<DocChunk> {
    if chunks.is_empty() {
        return chunks;
    }

    let mut merged: Vec<DocChunk> = Vec::new();

    for chunk in chunks {
        if let Some(last) = merged.last_mut() {
            // Merge if same file and lines are close (within 5 lines)
            if last.file == chunk.file && chunk.start_line <= last.end_line + 5 {
                last.end_line = chunk.end_line;
                last.content.push_str("\n...\n");
                last.content.push_str(&chunk.content);
                continue;
            }
        }
        merged.push(chunk);
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rg_line_match() {
        let result = parse_rg_line("README.md:10:Some content here");
        assert!(result.is_some());
        let (file, line, content) = result.unwrap();
        assert_eq!(file, "README.md");
        assert_eq!(line, 10);
        assert_eq!(content, "Some content here");
    }

    #[test]
    fn test_parse_rg_line_context() {
        let result = parse_rg_line("README.md-8-context line here");
        assert!(result.is_some());
        let (file, line, content) = result.unwrap();
        assert_eq!(file, "README.md");
        assert_eq!(line, 8);
        assert_eq!(content, "context line here");
    }

    #[test]
    fn test_parse_rg_line_nested_path() {
        let result = parse_rg_line("docs/api/reference.md:42:API documentation");
        assert!(result.is_some());
        let (file, line, content) = result.unwrap();
        assert_eq!(file, "docs/api/reference.md");
        assert_eq!(line, 42);
        assert_eq!(content, "API documentation");
    }

    #[test]
    fn test_parse_rg_line_content_with_colons() {
        let result = parse_rg_line("README.md:5:time: 12:30:00");
        assert!(result.is_some());
        let (file, line, content) = result.unwrap();
        assert_eq!(file, "README.md");
        assert_eq!(line, 5);
        assert_eq!(content, "time: 12:30:00");
    }
}
