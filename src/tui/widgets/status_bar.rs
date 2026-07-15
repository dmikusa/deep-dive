#![allow(dead_code)]

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Text;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::tui::state::{AppState, FocusPane};

pub struct StatusBarWidget;

impl StatusBarWidget {
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
        let text = if state.is_filter_active {
            "Filter: type pattern, Esc close".to_string()
        } else {
            match state.focus {
                FocusPane::LayerList => {
                    "↑↓/kj layer  Tab focus  Ctrl+A aggregated  Ctrl+L layer  Space collapse  q quit".to_string()
                }
                FocusPane::FileTree => {
                    "↑↓/kj nav  PgUp/dn/u/d page  Space collapse  Ctrl+Space all  Ctrl+O sort  Ctrl+B attrs  Ctrl+F filter  Ctrl+E extract  Ctrl+A/R/M/U toggle  q quit".to_string()
                }
            }
        };
        let paragraph = Paragraph::new(Text::from(text)).style(Style::default().fg(Color::White));
        frame.render_widget(paragraph, area);
    }
}
