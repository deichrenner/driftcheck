use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone)]
pub struct Theme {
    pub foreground: Color,
    pub highlight: Color,
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
            foreground: Color::Reset,
            highlight: Color::Cyan,
            warning: Color::Yellow,
            success: Color::Green,
            muted: Color::DarkGray,
            border: Color::Gray,
            selection: Color::Blue,
        }
    }

    pub fn minimal() -> Self {
        Self {
            foreground: Color::Reset,
            highlight: Color::White,
            warning: Color::Yellow,
            success: Color::Green,
            muted: Color::DarkGray,
            border: Color::DarkGray,
            selection: Color::White,
        }
    }

    pub fn colorful() -> Self {
        Self {
            foreground: Color::Reset,
            highlight: Color::Magenta,
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
}
