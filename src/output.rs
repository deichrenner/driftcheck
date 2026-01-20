use crate::analyzer::Issue;

/// Print issues in a non-TTY friendly format
pub fn print_issues(issues: &[Issue]) {
    eprintln!();
    eprintln!("docguard: Documentation drift detected!");
    eprintln!();
    eprintln!("{}", "━".repeat(72));
    eprintln!();

    for (i, issue) in issues.iter().enumerate() {
        eprintln!(
            "Issue {}: {}:{}",
            i + 1,
            issue.file.display(),
            issue.line
        );
        eprintln!("  {}", issue.description);

        if !issue.doc_excerpt.is_empty() {
            eprintln!();
            eprintln!("  Documentation says:");
            for line in issue.doc_excerpt.lines().take(5) {
                eprintln!("    {}", line);
            }
        }

        if let Some(ref fix) = issue.suggested_fix {
            eprintln!();
            eprintln!("  Suggested fix: {}", fix);
        }

        eprintln!();
    }

    eprintln!("{}", "━".repeat(72));
}

/// Format a single issue for display
pub fn format_issue(issue: &Issue) -> String {
    let mut output = String::new();

    output.push_str(&format!(
        "📄 {}:{}\n",
        issue.file.display(),
        issue.line
    ));
    output.push_str(&format!("{}\n", "─".repeat(60)));
    output.push_str(&issue.description);
    output.push('\n');

    if !issue.doc_excerpt.is_empty() {
        output.push_str("\nDoc excerpt:\n");
        for line in issue.doc_excerpt.lines() {
            output.push_str(&format!("  {}\n", line));
        }
    }


    if let Some(ref fix) = issue.suggested_fix {
        output.push_str(&format!("\nSuggested fix: {}\n", fix));
    }

    output
}
