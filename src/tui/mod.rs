mod app;
mod theme;

use crate::analyzer::Issue;
use crate::config::Config;
use crate::error::Result;

pub use app::App;
pub use theme::Theme;

/// Run the TUI application
pub async fn run(config: &Config, issues: Vec<Issue>) -> Result<()> {
    let theme = Theme::from_name(&config.tui.theme);
    let mut app = App::new(issues, config.clone(), theme);
    app.run().await
}
