#![allow(dead_code)]

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Paragraph, Wrap};
use ratatui::Frame;

use crate::analysis::comparer::Comparer;
use crate::analysis::filetree::DiffType;
use crate::tui::state::{AppState, CompareMode, FocusPane};

pub struct FileTreeWidget;

impl FileTreeWidget {
    pub fn render(frame: &mut Frame, area: Rect, state: &mut AppState, comparer: &mut Comparer) {
        let mut title = match state.focus {
            FocusPane::FileTree => "File Tree [*]".to_string(),
            FocusPane::LayerList => "File Tree".to_string(),
        };
        if state.show_attributes {
            title.push_str(" [attrs]");
        }
        if state.wrap_tree {
            title.push_str(" [wrap]");
        }
        if state.is_filter_active {
            title.push_str(&format!(" [filter: {}]", state.filter_text));
        }

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

        let height = area.height.saturating_sub(2) as usize; // account for borders
        let total_rows = tree.visible_paths_filtered(
            &state.hidden_diff_types,
            state.filter_regex().as_ref(),
        ).len();
        let max_offset = total_rows.saturating_sub(height);
        state.tree_scroll_offset = state.tree_scroll_offset.min(max_offset);

        let lines = tree.render_tree_filtered(
            state.tree_scroll_offset,
            state.tree_scroll_offset + height,
            &state.hidden_diff_types,
            state.filter_regex().as_ref(),
            state.show_attributes,
        );

        // Keep the selected path visible inside the viewport.
        if let Some(selected) = &state.selected_tree_path {
            if let Some(pos) = lines.iter().position(|l| &l.path == selected) {
                if pos >= height {
                    state.tree_scroll_offset += pos - height + 1;
                }
            }
        }

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

        let paragraph = if state.wrap_tree {
            Paragraph::new(Text::from(text_lines))
                .block(Block::bordered().title(title))
                .wrap(Wrap { trim: false })
        } else {
            Paragraph::new(Text::from(text_lines)).block(Block::bordered().title(title))
        };
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

    #[test]
    fn test_file_tree_filter_works_regardless_of_focus() {
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
        tree.add_path(
            "etc/passwd",
            FileInfo {
                entry_type: TarEntryType::Regular,
                size: 10,
                content_hash: 2,
                ..Default::default()
            },
        );
        tree.mark_all(DiffType::Added);

        let image = Image {
            reference: "test".into(),
            layers: vec![Layer {
                index: 0,
                command: "ADD files".into(),
                size: 110,
                tree,
            }],
        };

        let mut state = AppState::new(image);
        state.focus = FocusPane::LayerList;
        state.is_filter_active = true;
        state.filter_text = "bash".to_string();
        let mut comparer = Comparer::new(state.image.layers.clone());
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| FileTreeWidget::render(f, f.area(), &mut state, &mut comparer))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(content.contains("bash"));
        assert!(!content.contains("passwd"));
    }
}
