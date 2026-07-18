use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::tui::state::AppState;

pub struct FilterWidget;

impl FilterWidget {
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
        let text = if state.filter_text.is_empty() {
            Text::from(Line::from(vec![
                Span::styled("Filter: ", Style::default().fg(Color::Yellow)),
                Span::styled("<empty>", Style::default().fg(Color::DarkGray)),
            ]))
        } else {
            Text::from(Line::from(vec![
                Span::styled("Filter: ", Style::default().fg(Color::Yellow)),
                Span::raw(&state.filter_text),
            ]))
        };

        let paragraph = Paragraph::new(text)
            .block(Block::bordered().title("Filter"))
            .style(Style::default().fg(Color::White));
        frame.render_widget(paragraph, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::filetree::FileTree;
    use crate::image::{Image, Layer};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_filter_widget_renders() {
        let image = Image {
            reference: "test".into(),
            layers: vec![Layer::new(0, "ADD files", 100, FileTree::new())],
        };
        let mut state = AppState::new(image);
        state.filter_text = "bin/.*".to_string();

        let backend = TestBackend::new(40, 3);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| FilterWidget::render(f, f.area(), &state))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(content.contains("Filter"));
        assert!(content.contains("bin/.*"));
    }
}
