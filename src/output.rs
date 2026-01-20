use crate::analyzer::Issue;

/// Print issues in a non-TTY friendly format
pub fn print_issues(issues: &[Issue]) {
    eprintln!();
    eprintln!("driftcheck: Documentation drift detected!");
    eprintln!();
    eprintln!("{}", "━".repeat(72));
    eprintln!();

    for (i, issue) in issues.iter().enumerate() {
        eprintln!("Issue {}: {}:{}", i + 1, issue.file.display(), issue.line);
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
