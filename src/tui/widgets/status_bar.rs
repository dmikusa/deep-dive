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
        let text = if let Some(msg) = &state.status_message {
            msg.clone()
        } else if state.is_filter_active {
            "Filter: type pattern, Esc close".to_string()
        } else {
            match state.focus {
                FocusPane::LayerList => {
                    "↑↓/kj layer  Tab focus  Ctrl+A aggregated  Ctrl+L layer  Space collapse  Ctrl+O open  q quit".to_string()
                }
                FocusPane::FileTree => {
                    "↑↓/kj nav  PgUp/dn/u/d page  Space collapse  Ctrl+Space all  Ctrl+O open  Ctrl+Shift+O sort  Ctrl+B attrs  Ctrl+F filter  Ctrl+E extract  Ctrl+A/R/M/U toggle  q quit".to_string()
                }
                FocusPane::LayerDetails | FocusPane::ImageDetails => {
                    "Tab focus  ←→ pane  Ctrl+O open  q quit".to_string()
                }
            }
        };
        let paragraph = Paragraph::new(Text::from(text)).style(Style::default().fg(Color::White));
        frame.render_widget(paragraph, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::image::Image;
    use crate::tui::state::AppState;

    #[test]
    fn test_status_bar_renders() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = AppState::new(Image {
            reference: "test".into(),
            layers: Vec::new(),
        });
        terminal
            .draw(|f| StatusBarWidget::render(f, f.area(), &state))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(content.contains("layer") || content.contains("q quit"));
    }

    #[test]
    fn test_status_bar_shows_message() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut state = AppState::new(Image {
            reference: "test".into(),
            layers: Vec::new(),
        });
        state.status_message = Some("saved".into());
        terminal
            .draw(|f| StatusBarWidget::render(f, f.area(), &state))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(content.contains("saved"));
    }
}
