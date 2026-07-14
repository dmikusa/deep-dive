#![allow(dead_code)]

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::analysis::comparer::Comparer;
use crate::analysis::filetree::DiffType;
use crate::tui::state::{AppState, CompareMode, FocusPane};

pub struct FileTreeWidget;

impl FileTreeWidget {
    pub fn render(frame: &mut Frame, area: Rect, state: &mut AppState, comparer: &mut Comparer) {
        let title = match state.focus {
            FocusPane::FileTree => "File Tree [*]",
            FocusPane::LayerList => "File Tree",
        };

        let (bs, bstop, ts, tstop) = match state.compare_mode {
            CompareMode::Natural => {
                let indexes = comparer.natural_indexes();
                indexes[state.selected_layer.min(indexes.len().saturating_sub(1))]
            }
            CompareMode::Aggregated => {
                let indexes = comparer.aggregated_indexes();
                indexes[state.selected_layer.min(indexes.len().saturating_sub(1))]
            }
        };

        let base_tree = comparer.get_tree(bs, bstop, ts, tstop).clone();
        let mut tree = base_tree;
        tree.set_sort_mode(state.sort_mode);
        state.apply_collapsed_to_tree(&mut tree);

        let height = area.height as usize;
        let lines = tree.render_tree(0, height);

        let text_lines: Vec<Line> = lines
            .into_iter()
            .map(|rendered| {
                let style = Self::style_for_diff_type(rendered.diff_type);
                let is_selected = state
                    .selected_tree_path
                    .as_ref()
                    .map(|p| *p == rendered.path)
                    .unwrap_or(false);

                let style = if is_selected {
                    style
                        .bg(Color::DarkGray)
                        .add_modifier(ratatui::style::Modifier::BOLD)
                } else {
                    style
                };

                Line::from(Span::styled(rendered.text, style))
            })
            .collect();

        let paragraph =
            Paragraph::new(Text::from(text_lines)).block(Block::bordered().title(title));
        frame.render_widget(paragraph, area);
    }

    fn style_for_diff_type(diff_type: DiffType) -> Style {
        match diff_type {
            DiffType::Added => Style::default().fg(Color::Green),
            DiffType::Removed => Style::default().fg(Color::Red),
            DiffType::Modified => Style::default().fg(Color::Yellow),
            DiffType::Unmodified => Style::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::filetree::{FileInfo, FileTree, TarEntryType};
    use crate::image::{Image, Layer};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn test_image() -> Image {
        let mut tree = FileTree::new();
        tree.add_path(
            "bin/bash",
            FileInfo {
                entry_type: TarEntryType::Regular,
                size: 100,
                content_hash: 1,
                ..Default::default()
            },
        );
        tree.mark_all(DiffType::Added);

        Image {
            reference: "test".into(),
            layers: vec![Layer {
                index: 0,
                command: "ADD files".into(),
                size: 100,
                tree,
            }],
        }
    }

    #[test]
    fn test_file_tree_renders() {
        let mut state = AppState::new(test_image());
        let mut comparer = Comparer::new(state.image.layers.clone());
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| FileTreeWidget::render(f, f.area(), &mut state, &mut comparer))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(content.contains("bin"));
        assert!(content.contains("bash"));
    }
}
