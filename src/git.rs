use crate::error::{DriftcheckError, Result};
use std::fs;
use std::path::Path;
use std::process::Command;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const HOOK_SCRIPT: &str = r#"#!/bin/sh
# driftcheck pre-push hook
# This hook is called with the following parameters:
#   $1 -- Name of the remote to which the push is being done
#   $2 -- URL to which the push is being done

exec driftcheck hook
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
        .map_err(|e| DriftcheckError::GitError(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DriftcheckError::GitError(stderr.to_string()));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Get the upstream tracking branch
fn get_upstream() -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
        .output()
        .map_err(|e| DriftcheckError::GitError(e.to_string()))?;

    if !output.status.success() {
        return Err(DriftcheckError::NoUpstream);
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
            .map_err(|e| DriftcheckError::HookInstallError(e.to_string()))?;
    }

    // Check if hook already exists
    if hook_path.exists() && !force {
        // Read existing hook to check if it's ours
        let content = fs::read_to_string(&hook_path)
            .map_err(|e| DriftcheckError::HookInstallError(e.to_string()))?;

        if !content.contains("driftcheck") {
            return Err(DriftcheckError::HookInstallError(
                "A pre-push hook already exists. Use --force to overwrite, \
                 or manually add 'driftcheck hook' to your existing hook."
                    .to_string(),
            ));
        }
    }

    // Write the hook
    fs::write(&hook_path, HOOK_SCRIPT)
        .map_err(|e| DriftcheckError::HookInstallError(e.to_string()))?;

    // Make it executable (Unix only - Windows doesn't need this)
    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&hook_path)
            .map_err(|e| DriftcheckError::HookInstallError(e.to_string()))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&hook_path, perms)
            .map_err(|e| DriftcheckError::HookInstallError(e.to_string()))?;
    }

    Ok(())
}

/// Parsed diff - extracts file names from a git diff
#[derive(Debug, Clone)]
pub struct ParsedDiff {
    pub files: Vec<String>,
}

impl ParsedDiff {
    pub fn parse(diff: &str) -> Self {
        let mut files = Vec::new();

        for line in diff.lines() {
            if line.starts_with("diff --git") {
                // Extract filename from "diff --git a/path b/path"
                if let Some(b_path) = line.split(" b/").nth(1) {
                    files.push(b_path.to_string());
                }
            }
        }

        Self { files }
    }
}
