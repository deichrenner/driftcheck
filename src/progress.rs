use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

/// Progress indicator for TTY environments
pub struct Progress {
    bar: Option<ProgressBar>,
}

impl Progress {
    /// Create a new progress indicator (only shows if stdout is a TTY)
    pub fn new() -> Self {
        let bar = if atty::is(atty::Stream::Stdout) {
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::default_spinner()
                    .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
                    .template("{spinner:.cyan} {msg}")
                    .unwrap(),
            );
            pb.enable_steady_tick(Duration::from_millis(80));
            Some(pb)
        } else {
            None
        };

        Self { bar }
    }

    /// Update the progress message
    pub fn set_message(&self, msg: impl Into<String>) {
        if let Some(ref bar) = self.bar {
            bar.set_message(msg.into());
        }
    }

    /// Mark progress as complete (clear the line)
    pub fn finish_and_clear(&self) {
        if let Some(ref bar) = self.bar {
            bar.finish_and_clear();
        }
    }
}

impl Default for Progress {
    fn default() -> Self {
        Self::new()
    }
}

/// Multi-step progress tracker
pub struct MultiProgress {
    steps: Vec<&'static str>,
    current: usize,
    progress: Progress,
}

impl MultiProgress {
    pub fn new(steps: Vec<&'static str>) -> Self {
        Self {
            steps,
            current: 0,
            progress: Progress::new(),
        }
    }

    /// Start the next step
    pub fn next_step(&mut self) {
        if self.current < self.steps.len() {
            let step = self.steps[self.current];
            let msg = format!("[{}/{}] {}", self.current + 1, self.steps.len(), step);
            self.progress.set_message(msg);
            self.current += 1;
        }
    }

    /// Update message for current step
    pub fn update(&self, detail: &str) {
        if self.current > 0 && self.current <= self.steps.len() {
            let step = self.steps[self.current - 1];
            let msg = format!("[{}/{}] {} - {}", self.current, self.steps.len(), step, detail);
            self.progress.set_message(msg);
        }
    }

    /// Finish all progress
    pub fn finish(&self) {
        self.progress.finish_and_clear();
    }
}
