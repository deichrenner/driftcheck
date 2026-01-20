use crate::analyzer::Issue;
use crate::config::Config;
use crate::error::{DriftcheckError, Result};
use crate::tui::Theme;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use std::fs;
use std::io::{self, Stdout};
use tokio::task::JoinHandle;

pub struct App {
    issues: Vec<Issue>,
    config: Config,
    theme: Theme,
    current_issue: usize,
    list_state: ListState,
    show_help: bool,
    actions: Vec<IssueAction>,
    should_quit: bool,
    should_abort: bool,
    status_message: Option<String>,
    // Background task tracking
    active_task: Option<ActiveTask>,
    spinner_frame: usize,
}

struct ActiveTask {
    issue_idx: usize,
    handle: JoinHandle<Result<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum IssueAction {
    Pending,
    Applying,
    Skip,
    Applied,
    Error,
}

impl App {
    pub fn new(issues: Vec<Issue>, config: Config, theme: Theme) -> Self {
        let count = issues.len();
        let mut list_state = ListState::default();
        if count > 0 {
            list_state.select(Some(0));
        }

        Self {
            issues,
            config,
            theme,
            current_issue: 0,
            list_state,
            show_help: false,
            actions: vec![IssueAction::Pending; count],
            should_quit: false,
            should_abort: false,
            status_message: None,
            active_task: None,
            spinner_frame: 0,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        // Setup terminal
        enable_raw_mode().map_err(|e| DriftcheckError::TuiError(e.to_string()))?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
            .map_err(|e| DriftcheckError::TuiError(e.to_string()))?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal =
            Terminal::new(backend).map_err(|e| DriftcheckError::TuiError(e.to_string()))?;

        // Run the app
        let result = self.run_loop(&mut terminal).await;

        // Restore terminal
        disable_raw_mode().map_err(|e| DriftcheckError::TuiError(e.to_string()))?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )
        .map_err(|e| DriftcheckError::TuiError(e.to_string()))?;
        terminal
            .show_cursor()
            .map_err(|e| DriftcheckError::TuiError(e.to_string()))?;

        result
    }

    async fn run_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
        loop {
            // Check if background task completed
            self.check_task_completion().await;

            // Update spinner
            self.spinner_frame = (self.spinner_frame + 1) % 10;

            terminal
                .draw(|f| self.draw(f))
                .map_err(|e| DriftcheckError::TuiError(e.to_string()))?;

            // Use shorter poll time when task is active (for spinner animation)
            let poll_duration = if self.active_task.is_some() {
                std::time::Duration::from_millis(80)
            } else {
                std::time::Duration::from_millis(100)
            };

            if event::poll(poll_duration).map_err(|e| DriftcheckError::TuiError(e.to_string()))? {
                if let Event::Key(key) =
                    event::read().map_err(|e| DriftcheckError::TuiError(e.to_string()))?
                {
                    self.handle_key(key.code, key.modifiers);
                }
            }

            if self.should_quit {
                break;
            }

            if self.should_abort {
                return Err(DriftcheckError::TuiError("Push aborted by user".to_string()));
            }
        }

        Ok(())
    }

    async fn check_task_completion(&mut self) {
        if let Some(task) = &mut self.active_task {
            // Check if task is finished (non-blocking)
            if task.handle.is_finished() {
                let task = self.active_task.take().unwrap();
                match task.handle.await {
                    Ok(Ok(msg)) => {
                        self.actions[task.issue_idx] = IssueAction::Applied;
                        self.status_message = Some(msg);
                        // Move to next pending issue
                        self.move_to_next_pending();
                    }
                    Ok(Err(e)) => {
                        self.actions[task.issue_idx] = IssueAction::Error;
                        self.status_message = Some(format!("Error: {}", e));
                    }
                    Err(e) => {
                        self.actions[task.issue_idx] = IssueAction::Error;
                        self.status_message = Some(format!("Task failed: {}", e));
                    }
                }
            }
        }
    }

    fn move_to_next_pending(&mut self) {
        // Find next pending issue
        for i in 0..self.issues.len() {
            let idx = (self.current_issue + 1 + i) % self.issues.len();
            if self.actions[idx] == IssueAction::Pending {
                self.current_issue = idx;
                self.list_state.select(Some(idx));
                return;
            }
        }
    }

    fn handle_key(&mut self, key: KeyCode, _modifiers: KeyModifiers) {
        // Clear status message on any key (except when task is running)
        if self.active_task.is_none() {
            self.status_message = None;
        }

        if self.show_help {
            self.show_help = false;
            return;
        }

        // Ignore most keys while task is running
        if self.active_task.is_some() {
            match key {
                KeyCode::Char('q') | KeyCode::Esc => {
                    self.should_abort = true;
                }
                _ => {}
            }
            return;
        }

        match key {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.should_abort = true;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.next_issue();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.prev_issue();
            }
            KeyCode::Char('a') => {
                self.apply_current();
            }
            KeyCode::Char('s') => {
                self.skip_current();
            }
            KeyCode::Enter => {
                self.confirm_and_continue();
            }
            KeyCode::Char('?') => {
                self.show_help = true;
            }
            _ => {}
        }
    }

    fn next_issue(&mut self) {
        if self.issues.is_empty() {
            return;
        }
        self.current_issue = (self.current_issue + 1) % self.issues.len();
        self.list_state.select(Some(self.current_issue));
    }

    fn prev_issue(&mut self) {
        if self.issues.is_empty() {
            return;
        }
        if self.current_issue == 0 {
            self.current_issue = self.issues.len() - 1;
        } else {
            self.current_issue -= 1;
        }
        self.list_state.select(Some(self.current_issue));
    }

    fn apply_current(&mut self) {
        if self.current_issue >= self.issues.len() {
            return;
        }

        // Don't start if already applying something
        if self.active_task.is_some() {
            return;
        }

        let issue = &self.issues[self.current_issue];
        if !issue.file.exists() {
            self.status_message = Some(format!("File not found: {}", issue.file.display()));
            return;
        }

        // Mark as applying
        self.actions[self.current_issue] = IssueAction::Applying;

        // Clone data needed for the async task
        let config = self.config.clone();
        let issue = self.issues[self.current_issue].clone();
        let issue_idx = self.current_issue;
        let file_display = issue.file.display().to_string();

        // Spawn background task
        let handle = tokio::spawn(async move {
            apply_fix_task(config, issue).await
        });

        self.active_task = Some(ActiveTask {
            issue_idx,
            handle,
        });

        self.status_message = Some(format!("Generating fix for {}...", file_display));
    }

    fn skip_current(&mut self) {
        if self.current_issue < self.actions.len() {
            self.actions[self.current_issue] = IssueAction::Skip;
            self.next_issue();
        }
    }

    fn confirm_and_continue(&mut self) {
        // Don't allow confirm while task is running
        if self.active_task.is_some() {
            return;
        }

        let pending = self
            .actions
            .iter()
            .filter(|a| **a == IssueAction::Pending)
            .count();
        if pending == 0 {
            self.should_quit = true;
        } else {
            // Jump to next pending issue
            for (i, action) in self.actions.iter().enumerate() {
                if *action == IssueAction::Pending {
                    self.current_issue = i;
                    self.list_state.select(Some(i));
                    break;
                }
            }
        }
    }

    fn get_spinner_char(&self) -> &'static str {
        const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        SPINNER[self.spinner_frame]
    }

    fn draw(&mut self, f: &mut Frame) {
        let size = f.area();

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(10),   // Content
                Constraint::Length(3), // Footer
            ])
            .split(size);

        self.draw_header(f, chunks[0]);
        self.draw_content(f, chunks[1]);
        self.draw_footer(f, chunks[2]);

        if self.show_help {
            self.draw_help_popup(f, size);
        }
    }

    fn draw_header(&self, f: &mut Frame, area: Rect) {
        let pending = self
            .actions
            .iter()
            .filter(|a| **a == IssueAction::Pending)
            .count();
        let applied = self
            .actions
            .iter()
            .filter(|a| **a == IssueAction::Applied)
            .count();
        let skipped = self
            .actions
            .iter()
            .filter(|a| **a == IssueAction::Skip)
            .count();
        let applying = self
            .actions
            .iter()
            .filter(|a| **a == IssueAction::Applying)
            .count();

        let title = format!(
            " driftcheck - {} issues ({} pending, {} applied, {} skipped) ",
            self.issues.len(),
            pending,
            applied,
            skipped
        );

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.border_style())
            .title(Span::styled(title, self.theme.title_style()));

        let status_text = if applying > 0 {
            Span::styled(
                format!("{} {}", self.get_spinner_char(), self.status_message.as_deref().unwrap_or("Applying fix...")),
                self.theme.highlight_style(),
            )
        } else if let Some(ref msg) = self.status_message {
            Span::styled(msg.as_str(), self.theme.highlight_style())
        } else if pending > 0 {
            Span::styled(
                "Documentation issues detected",
                self.theme.warning_style(),
            )
        } else {
            Span::styled("All issues addressed", self.theme.success_style())
        };

        let paragraph = Paragraph::new(Line::from(status_text)).block(block);

        f.render_widget(paragraph, area);
    }

    fn draw_content(&mut self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(area);

        self.draw_issue_list(f, chunks[0]);
        self.draw_issue_detail(f, chunks[1]);
    }

    fn draw_issue_list(&mut self, f: &mut Frame, area: Rect) {
        let spinner = self.get_spinner_char();

        let items: Vec<ListItem> = self
            .issues
            .iter()
            .enumerate()
            .map(|(i, issue)| {
                let action = &self.actions[i];
                let prefix = match action {
                    IssueAction::Pending => "○",
                    IssueAction::Applying => spinner,
                    IssueAction::Skip => "⊘",
                    IssueAction::Applied => "✓",
                    IssueAction::Error => "✗",
                };

                let style = match action {
                    IssueAction::Pending => self.theme.normal_style(),
                    IssueAction::Applying => self.theme.highlight_style(),
                    IssueAction::Skip => self.theme.muted_style(),
                    IssueAction::Applied => self.theme.success_style(),
                    IssueAction::Error => self.theme.warning_style(),
                };

                let text = format!(
                    "{} {}:{}",
                    prefix,
                    issue.file.file_name().unwrap_or_default().to_string_lossy(),
                    issue.line
                );

                ListItem::new(text).style(style)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(self.theme.border_style())
                    .title(" Issues "),
            )
            .highlight_style(self.theme.selected_style())
            .highlight_symbol("> ");

        f.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn draw_issue_detail(&self, f: &mut Frame, area: Rect) {
        if self.issues.is_empty() {
            let paragraph = Paragraph::new("No issues").block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(self.theme.border_style())
                    .title(" Details "),
            );
            f.render_widget(paragraph, area);
            return;
        }

        let issue = &self.issues[self.current_issue];
        let is_applying = self.actions[self.current_issue] == IssueAction::Applying;

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);

        // Issue description
        let mut lines = vec![
            Line::from(Span::styled(
                format!("{}", issue.file.display()),
                self.theme.highlight_style(),
            )),
            Line::from(""),
            Line::from(issue.description.as_str()),
        ];

        if !issue.doc_excerpt.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Documentation excerpt:",
                self.theme.muted_style(),
            )));
            for line in issue.doc_excerpt.lines().take(5) {
                lines.push(Line::from(format!("  {}", line)));
            }
        }

        let title = if is_applying {
            format!(
                " Issue {}/{} {} Generating fix... ",
                self.current_issue + 1,
                self.issues.len(),
                self.get_spinner_char()
            )
        } else {
            format!(
                " Issue {}/{} ",
                self.current_issue + 1,
                self.issues.len()
            )
        };

        let desc_para = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(if is_applying {
                        self.theme.highlight_style()
                    } else {
                        self.theme.border_style()
                    })
                    .title(title),
            )
            .wrap(Wrap { trim: false });

        f.render_widget(desc_para, chunks[0]);

        // Suggested fix
        let fix_text = issue
            .suggested_fix
            .as_deref()
            .unwrap_or("No fix suggestion available");

        let fix_para = Paragraph::new(fix_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(self.theme.border_style())
                    .title(" Suggested Fix "),
            )
            .wrap(Wrap { trim: false });

        f.render_widget(fix_para, chunks[1]);
    }

    fn draw_footer(&self, f: &mut Frame, area: Rect) {
        let keybindings = if self.active_task.is_some() {
            vec![("q", "Abort")]
        } else {
            vec![
                ("a", "Apply"),
                ("s", "Skip"),
                ("j/k", "Nav"),
                ("Enter", "Done"),
                ("q", "Abort"),
                ("?", "Help"),
            ]
        };

        let spans: Vec<Span> = keybindings
            .into_iter()
            .flat_map(|(key, action)| {
                vec![
                    Span::styled(format!(" {} ", key), self.theme.highlight_style()),
                    Span::styled(format!("{} ", action), self.theme.muted_style()),
                ]
            })
            .collect();

        let paragraph = Paragraph::new(Line::from(spans)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(self.theme.border_style()),
        );

        f.render_widget(paragraph, area);
    }

    fn draw_help_popup(&self, f: &mut Frame, area: Rect) {
        let popup_area = centered_rect(60, 70, area);

        let help_text = vec![
            Line::from(Span::styled("Keybindings", self.theme.title_style())),
            Line::from(""),
            Line::from("  a        Apply fix (uses LLM to generate fix)"),
            Line::from("  s        Skip this issue"),
            Line::from("  j / Down Next issue"),
            Line::from("  k / Up   Previous issue"),
            Line::from("  Enter    Confirm all and continue push"),
            Line::from("  q / Esc  Abort push"),
            Line::from("  ?        Show this help"),
            Line::from(""),
            Line::from(Span::styled(
                "Review changes with 'git diff' after exiting",
                self.theme.muted_style(),
            )),
            Line::from(Span::styled(
                "Press any key to close",
                self.theme.muted_style(),
            )),
        ];

        let help = Paragraph::new(help_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(self.theme.highlight_style())
                    .title(" Help "),
            )
            .wrap(Wrap { trim: false });

        f.render_widget(Clear, popup_area);
        f.render_widget(help, popup_area);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Background task to apply a fix
async fn apply_fix_task(config: Config, issue: Issue) -> Result<String> {
    let file_path = &issue.file;

    // Read the current file content
    let original_content = fs::read_to_string(file_path).map_err(|e| {
        DriftcheckError::TuiError(format!("Failed to read {}: {}", file_path.display(), e))
    })?;

    // Generate the fix using LLM
    let fixed_content = generate_doc_fix(&config, &issue, &original_content).await?;

    // Write the fixed content
    fs::write(file_path, &fixed_content).map_err(|e| {
        DriftcheckError::TuiError(format!("Failed to write {}: {}", file_path.display(), e))
    })?;

    Ok(format!("Applied fix to {}", file_path.display()))
}

/// Generate a fixed version of the documentation using LLM
async fn generate_doc_fix(config: &Config, issue: &Issue, original_content: &str) -> Result<String> {
    use crate::llm::LlmClient;

    let client = LlmClient::new(&config.llm)?;

    let system_prompt = r#"You are a documentation editor. Given an issue description and the current documentation content, output the COMPLETE fixed documentation file.

Rules:
1. Output ONLY the fixed file content, no explanations
2. Make minimal changes - only fix what's necessary
3. Preserve all formatting, whitespace, and structure
4. If the issue mentions missing documentation, add it in the appropriate place"#;

    let user_prompt = format!(
        r#"## Issue
File: {}
Line: {}
Problem: {}

## Suggested Fix
{}

## Current File Content
```
{}
```

Output the complete fixed file content:"#,
        issue.file.display(),
        issue.line,
        issue.description,
        issue.suggested_fix.as_deref().unwrap_or("(none)"),
        original_content
    );

    client.chat(system_prompt, &user_prompt).await
}
