use crate::error::{DocguardError, Result};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

const HOOK_SCRIPT: &str = r#"#!/bin/sh
# docguard pre-push hook
# This hook is called with the following parameters:
#   $1 -- Name of the remote to which the push is being done
#   $2 -- URL to which the push is being done

exec docguard hook
"#;

/// Get the diff between upstream and HEAD (or custom range)
pub fn get_diff(range: &Option<String>) -> Result<String> {
    let range = match range {
        Some(r) => r.clone(),
        None => {
            // Get the upstream tracking branch
            let upstream = get_upstream()?;
            format!("{}..HEAD", upstream)
        }
    };

    let output = Command::new("git")
        .args(["diff", &range])
        .output()
        .map_err(|e| DocguardError::GitError(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DocguardError::GitError(stderr.to_string()));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Get the upstream tracking branch
fn get_upstream() -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
        .output()
        .map_err(|e| DocguardError::GitError(e.to_string()))?;

    if !output.status.success() {
        return Err(DocguardError::NoUpstream);
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Install the pre-push hook
pub fn install_hook(git_root: &Path, force: bool) -> Result<()> {
    let hooks_dir = git_root.join(".git/hooks");
    let hook_path = hooks_dir.join("pre-push");

    // Create hooks directory if it doesn't exist
    if !hooks_dir.exists() {
        fs::create_dir_all(&hooks_dir)
            .map_err(|e| DocguardError::HookInstallError(e.to_string()))?;
    }

    // Check if hook already exists
    if hook_path.exists() && !force {
        // Read existing hook to check if it's ours
        let content = fs::read_to_string(&hook_path)
            .map_err(|e| DocguardError::HookInstallError(e.to_string()))?;

        if !content.contains("docguard") {
            return Err(DocguardError::HookInstallError(
                "A pre-push hook already exists. Use --force to overwrite, \
                 or manually add 'docguard hook' to your existing hook."
                    .to_string(),
            ));
        }
    }

    // Write the hook
    fs::write(&hook_path, HOOK_SCRIPT)
        .map_err(|e| DocguardError::HookInstallError(e.to_string()))?;

    // Make it executable
    let mut perms = fs::metadata(&hook_path)
        .map_err(|e| DocguardError::HookInstallError(e.to_string()))?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&hook_path, perms)
        .map_err(|e| DocguardError::HookInstallError(e.to_string()))?;

    Ok(())
}

/// Parse a diff into structured hunks
#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub file: String,
    pub old_start: usize,
    pub old_count: usize,
    pub new_start: usize,
    pub new_count: usize,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct ParsedDiff {
    pub files: Vec<String>,
    pub hunks: Vec<DiffHunk>,
    pub raw: String,
}

impl ParsedDiff {
    pub fn parse(diff: &str) -> Self {
        let mut files = Vec::new();
        let mut hunks = Vec::new();
        let mut current_file: Option<String> = None;
        let mut current_hunk: Option<DiffHunk> = None;

        for line in diff.lines() {
            if line.starts_with("diff --git") {
                // Save previous hunk
                if let Some(hunk) = current_hunk.take() {
                    hunks.push(hunk);
                }

                // Extract filename from "diff --git a/path b/path"
                if let Some(b_path) = line.split(" b/").nth(1) {
                    current_file = Some(b_path.to_string());
                    files.push(b_path.to_string());
                }
            } else if line.starts_with("@@") {
                // Save previous hunk
                if let Some(hunk) = current_hunk.take() {
                    hunks.push(hunk);
                }

                // Parse hunk header: @@ -old_start,old_count +new_start,new_count @@
                if let Some(file) = &current_file {
                    let (old_start, old_count, new_start, new_count) = parse_hunk_header(line);
                    current_hunk = Some(DiffHunk {
                        file: file.clone(),
                        old_start,
                        old_count,
                        new_start,
                        new_count,
                        content: String::new(),
                    });
                }
            } else if let Some(ref mut hunk) = current_hunk {
                // Add line to current hunk content
                hunk.content.push_str(line);
                hunk.content.push('\n');
            }
        }

        // Save last hunk
        if let Some(hunk) = current_hunk {
            hunks.push(hunk);
        }

        Self {
            files,
            hunks,
            raw: diff.to_string(),
        }
    }
}

fn parse_hunk_header(line: &str) -> (usize, usize, usize, usize) {
    // @@ -7,6 +7,7 @@ optional context
    let parts: Vec<&str> = line.split_whitespace().collect();
    let mut old_start = 0;
    let mut old_count = 1;
    let mut new_start = 0;
    let mut new_count = 1;

    for part in parts {
        if part.starts_with('-') && !part.starts_with("---") {
            let nums: Vec<&str> = part[1..].split(',').collect();
            if !nums.is_empty() {
                old_start = nums[0].parse().unwrap_or(0);
            }
            if nums.len() > 1 {
                old_count = nums[1].parse().unwrap_or(1);
            }
        } else if part.starts_with('+') && !part.starts_with("+++") {
            let nums: Vec<&str> = part[1..].split(',').collect();
            if !nums.is_empty() {
                new_start = nums[0].parse().unwrap_or(0);
            }
            if nums.len() > 1 {
                new_count = nums[1].parse().unwrap_or(1);
            }
        }
    }

    (old_start, old_count, new_start, new_count)
}

/// Check if the diff only contains non-code files (docs, configs, etc.)
pub fn is_docs_only_diff(diff: &ParsedDiff) -> bool {
    let doc_extensions = [".md", ".txt", ".rst", ".toml", ".yaml", ".yml", ".json"];

    diff.files.iter().all(|f| {
        doc_extensions.iter().any(|ext| f.ends_with(ext))
    })
}

/// Get recent commit log to provide context about what's already been done
pub fn get_recent_commits(count: usize) -> Result<String> {
    let output = Command::new("git")
        .args([
            "log",
            &format!("-{}", count),
            "--pretty=format:%h %s",
            "--name-only",
        ])
        .output()
        .map_err(|e| DocguardError::GitError(e.to_string()))?;

    if !output.status.success() {
        // Not fatal - just return empty
        return Ok(String::new());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Get the files changed in recent commits (to know what docs were recently updated)
pub fn get_recently_changed_docs(count: usize) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args([
            "log",
            &format!("-{}", count),
            "--pretty=format:",
            "--name-only",
            "--diff-filter=AM", // Added or Modified
        ])
        .output()
        .map_err(|e| DocguardError::GitError(e.to_string()))?;

    if !output.status.success() {
        return Ok(vec![]);
    }

    let doc_extensions = [".md", ".txt", ".rst"];
    let files: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .filter(|l| doc_extensions.iter().any(|ext| l.ends_with(ext)))
        .map(|s| s.to_string())
        .collect();

    Ok(files)
}
