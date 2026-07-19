use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::tui::state::{AppState, ModalState};

pub struct ModalWidget;

impl ModalWidget {
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
        match &state.modal {
            ModalState::DetailField { label } => {
                Self::render_detail_field(frame, area, state, label);
            }
            ModalState::ExtractTo { destination, .. } => {
                Self::render_input_modal(frame, area, "Extract to", destination);
            }
            ModalState::OpenImage { url } => {
                Self::render_input_modal(frame, area, "Open image", url);
            }
            ModalState::None => {}
        }
    }

    fn render_input_modal(frame: &mut Frame, area: Rect, title: &str, input: &str) {
        let popup_area = Self::small_modal_area(area);
        frame.render_widget(Clear, popup_area);

        let text = Text::from(Line::from(vec![
            Span::raw(input),
            Span::styled("▌", Style::default().fg(Color::White)),
        ]));
        let paragraph = Paragraph::new(text)
            .block(Block::bordered().title(title))
            .style(Style::default().fg(Color::White))
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, popup_area);
    }

    fn render_detail_field(frame: &mut Frame, area: Rect, state: &AppState, label: &str) {
        let popup_area = Self::large_modal_area(area);
        frame.render_widget(Clear, popup_area);

        let value = Self::detail_field_value(state, label);
        let sanitized = Self::sanitize_value(&value);
        let text = Text::from(vec![
            Line::from(vec![Span::styled(
                format!("{}: ", label),
                Style::default().fg(Color::Yellow),
            )]),
            Line::from(Span::raw(sanitized)),
            Line::from(""),
            Line::from(Span::styled(
                "↑/↓ change layer  •  Ctrl+C copy  •  Esc/Enter close",
                Style::default().fg(Color::DarkGray),
            )),
        ]);
        let paragraph = Paragraph::new(text)
            .block(Block::bordered().title(format!("{} - Full value", label)))
            .style(Style::default().fg(Color::White))
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, popup_area);
    }

    fn detail_field_value(state: &AppState, label: &str) -> String {
        use crate::tui::widgets::layer_details::LayerDetailsWidget;
        let fields = LayerDetailsWidget::fields(state);
        fields
            .iter()
            .find(|f| f.label == label)
            .map(|f| f.value.clone())
            .unwrap_or_default()
    }

    fn sanitize_value(value: &str) -> String {
        let ansi_stripped = regex::Regex::new(r"\x1B\[[0-9;]*[A-Za-z]")
            .unwrap()
            .replace_all(value, "");
        let tab_replaced = ansi_stripped.replace('\t', " ");
        tab_replaced
            .chars()
            .filter(|ch| !ch.is_control() || *ch == '\n')
            .collect()
    }

    fn small_modal_area(area: Rect) -> Rect {
        let width = area.width.saturating_sub(4).clamp(20, 60);
        let height = 3u16;
        Self::centered_area(area, width, height)
    }

    fn large_modal_area(area: Rect) -> Rect {
        let width = (area.width * 75) / 100;
        let height = (area.height * 75) / 100;
        Self::centered_area(area, width, height)
    }

    fn centered_area(area: Rect, width: u16, height: u16) -> Rect {
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

    #[test]
    fn test_detail_field_modal_renders() {
        let image = Image {
            reference: "test".into(),
            layers: vec![Layer::new(0, "RUN echo hello", 0, FileTree::new())],
        };
        let mut state = AppState::new(image);
        state.open_detail_field_modal("Command");

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| ModalWidget::render(f, f.area(), &state))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(content.contains("Command"));
        assert!(content.contains("RUN echo hello"));
        assert!(content.contains("copy"));
    }
}
