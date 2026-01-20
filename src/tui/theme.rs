use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub background: Color,
    pub foreground: Color,
    pub highlight: Color,
    pub error: Color,
    pub warning: Color,
    pub success: Color,
    pub muted: Color,
    pub border: Color,
    pub selection: Color,
}

impl Theme {
    pub fn from_name(name: &str) -> Self {
        match name {
            "minimal" => Self::minimal(),
            "colorful" => Self::colorful(),
            _ => Self::default(),
        }
    }

    pub fn default() -> Self {
        Self {
            name: "default".to_string(),
            background: Color::Reset,
            foreground: Color::Reset,
            highlight: Color::Cyan,
            error: Color::Red,
            warning: Color::Yellow,
            success: Color::Green,
            muted: Color::DarkGray,
            border: Color::Gray,
            selection: Color::Blue,
        }
    }

    pub fn minimal() -> Self {
        Self {
            name: "minimal".to_string(),
            background: Color::Reset,
            foreground: Color::Reset,
            highlight: Color::White,
            error: Color::Red,
            warning: Color::Yellow,
            success: Color::Green,
            muted: Color::DarkGray,
            border: Color::DarkGray,
            selection: Color::White,
        }
    }

    pub fn colorful() -> Self {
        Self {
            name: "colorful".to_string(),
            background: Color::Reset,
            foreground: Color::Reset,
            highlight: Color::Magenta,
            error: Color::LightRed,
            warning: Color::LightYellow,
            success: Color::LightGreen,
            muted: Color::Gray,
            border: Color::Cyan,
            selection: Color::LightBlue,
        }
    }

    pub fn title_style(&self) -> Style {
        Style::default()
            .fg(self.highlight)
            .add_modifier(Modifier::BOLD)
    }

    pub fn normal_style(&self) -> Style {
        Style::default().fg(self.foreground)
    }

    pub fn muted_style(&self) -> Style {
        Style::default().fg(self.muted)
    }

    pub fn error_style(&self) -> Style {
        Style::default().fg(self.error)
    }

    pub fn warning_style(&self) -> Style {
        Style::default().fg(self.warning)
    }

    pub fn success_style(&self) -> Style {
        Style::default().fg(self.success)
    }

    pub fn selected_style(&self) -> Style {
        Style::default()
            .bg(self.selection)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    }

    pub fn border_style(&self) -> Style {
        Style::default().fg(self.border)
    }

    pub fn highlight_style(&self) -> Style {
        Style::default()
            .fg(self.highlight)
            .add_modifier(Modifier::BOLD)
    }

    pub fn diff_add_style(&self) -> Style {
        Style::default().fg(Color::Green)
    }

    pub fn diff_remove_style(&self) -> Style {
        Style::default().fg(Color::Red)
    }
}
