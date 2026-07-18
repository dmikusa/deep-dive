use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::state::AppState;

pub struct ModalWidget;

impl ModalWidget {
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
        let Some(destination) = state.modal_destination() else {
            return;
        };

        let popup_area = Self::modal_area(area);
        frame.render_widget(Clear, popup_area);

        let text = Text::from(Line::from(vec![
            Span::raw(destination),
            Span::styled("▌", Style::default().fg(Color::White)),
        ]));
        let paragraph = Paragraph::new(text)
            .block(Block::bordered().title("Extract to"))
            .style(Style::default().fg(Color::White))
            .wrap(ratatui::widgets::Wrap { trim: false });
        frame.render_widget(paragraph, popup_area);
    }

    fn modal_area(area: Rect) -> Rect {
        let width = area.width.saturating_sub(4).clamp(20, 60);
        let height = 3u16;
        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length((area.width.saturating_sub(width)) / 2),
                Constraint::Length(width),
                Constraint::Length((area.width.saturating_sub(width)) / 2),
            ])
            .split(area);
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length((area.height.saturating_sub(height)) / 2),
                Constraint::Length(height),
                Constraint::Length((area.height.saturating_sub(height)) / 2),
            ])
            .split(horizontal[1]);
        vertical[1]
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
    fn test_modal_renders() {
        let image = Image {
            reference: "test".into(),
            layers: vec![Layer::new(0, "ADD", 0, FileTree::new())],
        };
        let mut state = AppState::new(image);
        state.modal = crate::tui::state::ModalState::ExtractTo {
            destination: "/tmp/output".to_string(),
            original_path: "file.txt".to_string(),
        };

        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| ModalWidget::render(f, f.area(), &state))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(content.contains("Extract to"));
        assert!(content.contains("/tmp/output"));
    }
}
