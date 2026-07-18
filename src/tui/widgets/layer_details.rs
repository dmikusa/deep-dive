use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::tui::state::{AppState, FocusPane};
use crate::utils::{format_size, sanitize_and_truncate};

pub struct LayerDetailsWidget;

/// A single detail field with its display label and full value.
#[derive(Debug, Clone)]
pub struct DetailField {
    pub label: &'static str,
    pub value: String,
}

impl LayerDetailsWidget {
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
        let title = match state.focus {
            FocusPane::LayerDetails => "Layer Details [*]".to_string(),
            _ => "Layer Details".to_string(),
        };

        let inner_width = area.width.saturating_sub(2) as usize;
        let fields = Self::fields(state);
        let selected = if state.focus == FocusPane::LayerDetails {
            Some(
                state
                    .selected_detail_field
                    .min(fields.len().saturating_sub(1)),
            )
        } else {
            None
        };

        let lines: Vec<Line> = fields
            .iter()
            .enumerate()
            .map(|(i, field)| {
                let is_selected = selected == Some(i);
                Self::detail_line(field, inner_width, is_selected)
            })
            .collect();

        let paragraph = Paragraph::new(Text::from(lines))
            .block(Block::bordered().title(title))
            .wrap(ratatui::widgets::Wrap { trim: false });
        frame.render_widget(paragraph, area);
    }

    /// Build the ordered list of detail fields for the currently selected layer.
    pub fn fields(state: &AppState) -> Vec<DetailField> {
        let mut fields = Vec::new();
        if let Some(layer) = state.image.layers.get(state.selected_layer) {
            fields.push(DetailField {
                label: "Index",
                value: layer.index.to_string(),
            });
            fields.push(DetailField {
                label: "ID",
                value: layer.id.clone().unwrap_or_else(|| "n/a".to_string()),
            });
            fields.push(DetailField {
                label: "Size",
                value: format_size(layer.size),
            });
            fields.push(DetailField {
                label: "Command",
                value: layer.command.clone(),
            });
            fields.push(DetailField {
                label: "Digest",
                value: layer.digest.clone().unwrap_or_else(|| "n/a".to_string()),
            });
            let tags = if layer.tags.is_empty() {
                "n/a".to_string()
            } else {
                layer.tags.join(", ")
            };
            fields.push(DetailField {
                label: "Tags",
                value: tags,
            });
        }
        fields
    }

    pub fn field_count(state: &AppState) -> usize {
        Self::fields(state).len()
    }

    fn detail_line(field: &DetailField, inner_width: usize, is_selected: bool) -> Line<'static> {
        let label_prefix = format!("{}: ", field.label);
        let available = inner_width.saturating_sub(label_prefix.len());
        let display_value = sanitize_and_truncate(&field.value, available);

        let mut spans = vec![
            Span::styled(label_prefix, Style::default().fg(Color::Yellow)),
            Span::raw(display_value),
        ];

        if is_selected {
            // Highlight by applying a background to the entire visible line.
            let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
            spans = vec![Span::styled(
                text,
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            )];
        }

        Line::from(spans)
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

    #[test]
    fn test_layer_details_truncates_long_value() {
        let mut image = test_image();
        image.layers[0].command = "RUN ".to_string() + &"x".repeat(200);
        let state = AppState::new(image);
        let backend = TestBackend::new(20, 12);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| LayerDetailsWidget::render(f, f.area(), &state))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(content.contains("..."));
    }
}
