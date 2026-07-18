use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::tui::state::{AppState, FocusPane};
use crate::utils::format_size;

pub struct LayerDetailsWidget;

impl LayerDetailsWidget {
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
        let title = match state.focus {
            FocusPane::LayerDetails => "Layer Details [*]".to_string(),
            _ => "Layer Details".to_string(),
        };

        let mut lines: Vec<Line> = Vec::new();

        if let Some(layer) = state.image.layers.get(state.selected_layer) {
            lines.push(Self::detail_line("Index", &layer.index.to_string()));
            lines.push(Self::detail_line(
                "ID",
                layer.id.as_deref().unwrap_or("n/a"),
            ));
            lines.push(Self::detail_line("Size", &format_size(layer.size)));
            lines.push(Self::detail_line("Command", &layer.command));
            lines.push(Self::detail_line(
                "Digest",
                layer.digest.as_deref().unwrap_or("n/a"),
            ));
            let tags = if layer.tags.is_empty() {
                "n/a".to_string()
            } else {
                layer.tags.join(", ")
            };
            lines.push(Self::detail_line("Tags", &tags));
        } else {
            lines.push(Line::from("No layer selected."));
        }

        let paragraph = Paragraph::new(Text::from(lines))
            .block(Block::bordered().title(title))
            .wrap(ratatui::widgets::Wrap { trim: false });
        frame.render_widget(paragraph, area);
    }

    fn detail_line(label: &str, value: &str) -> Line<'static> {
        Line::from(vec![
            Span::styled(format!("{}: ", label), Style::default().fg(Color::Yellow)),
            Span::raw(value.to_string()),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::filetree::FileTree;
    use crate::image::{Image, Layer};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn test_image() -> Image {
        Image {
            reference: "test-image:latest".into(),
            layers: vec![Layer {
                index: 0,
                command: "ADD file".into(),
                size: 1024,
                tree: FileTree::new(),
                id: Some("abc123".into()),
                digest: Some("sha256:abc123".into()),
                tags: vec!["test-image:latest".into()],
            }],
        }
    }

    #[test]
    fn test_layer_details_renders() {
        let state = AppState::new(test_image());
        let backend = TestBackend::new(40, 12);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| LayerDetailsWidget::render(f, f.area(), &state))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(content.contains("Layer Details"));
        assert!(content.contains("abc123"));
        assert!(content.contains("ADD file"));
        assert!(content.contains("1.0 KB"));
        assert!(content.contains("test-image:latest"));
    }
}
