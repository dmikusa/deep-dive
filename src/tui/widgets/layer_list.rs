#![allow(dead_code)]

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Text;
use ratatui::widgets::{Block, List, ListItem, ListState};
use ratatui::Frame;

use crate::tui::state::{AppState, CompareMode, FocusPane};
use crate::utils::format_size;

pub struct LayerListWidget;

impl LayerListWidget {
    pub fn render(frame: &mut Frame, area: Rect, state: &mut AppState) {
        let mode_label = match state.compare_mode {
            CompareMode::Natural => "layer",
            CompareMode::Aggregated => "all",
        };
        let title = match state.focus {
            FocusPane::LayerList => format!("Layers [{}] [*]", mode_label),
            _ => format!("Layers [{}]", mode_label),
        };

        let items: Vec<ListItem> = state
            .layers()
            .iter()
            .map(|layer| {
                let text = format!(
                    "{}: [{}] {}",
                    layer.index,
                    format_size(layer.size),
                    layer.command
                );
                ListItem::new(Text::from(text))
            })
            .collect();

        let list = List::new(items)
            .block(Block::bordered().title(title))
            .highlight_style(
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            );

        let mut list_state = ListState::default().with_selected(Some(state.selected_layer));
        frame.render_stateful_widget(list, area, &mut list_state);
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
            reference: "test".into(),
            layers: vec![
                Layer::new(0, "FROM scratch", 0, FileTree::new()),
                Layer::new(1, "ADD file", 1024, FileTree::new()),
            ],
        }
    }

    #[test]
    fn test_layer_list_renders() {
        let mut state = AppState::new(test_image());
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| LayerListWidget::render(f, f.area(), &mut state))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(content.contains("FROM scratch"));
        assert!(content.contains("ADD file"));
    }
}
