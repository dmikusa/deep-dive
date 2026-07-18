#![allow(dead_code)]

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap};
use ratatui::Frame;

use crate::analysis::comparer::Comparer;
use crate::analysis::filetree::DiffType;
use crate::tui::state::{AppState, CompareMode, FocusPane};

pub struct FileTreeWidget;

impl FileTreeWidget {
    pub fn render(frame: &mut Frame, area: Rect, state: &mut AppState, comparer: &mut Comparer) {
        let mode_label = match state.compare_mode {
            CompareMode::Natural => "layer",
            CompareMode::Aggregated => "all",
        };
        let mut title = match state.focus {
            FocusPane::FileTree => format!("File Tree [{}] [*]", mode_label),
            _ => format!("File Tree [{}]", mode_label),
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

        let visible_paths =
            tree.visible_paths_filtered(&state.hidden_diff_types, state.filter_regex().as_ref());
        let total_rows = visible_paths.len();
        let height = area.height.saturating_sub(2) as usize; // account for borders
        let max_offset = total_rows.saturating_sub(height);

        // Keep the selected path visible in the viewport.
        if let Some(selected) = &state.selected_tree_path {
            if let Some(index) = visible_paths.iter().position(|p| p == selected) {
                if index < state.tree_scroll_offset {
                    state.tree_scroll_offset = index;
                } else if index >= state.tree_scroll_offset + height {
                    state.tree_scroll_offset = index.saturating_sub(height - 1);
                }
            }
        }

        state.tree_scroll_offset = state.tree_scroll_offset.min(max_offset);

        let lines = tree.render_tree_filtered(
            state.tree_scroll_offset,
            state.tree_scroll_offset + height,
            &state.hidden_diff_types,
            state.filter_regex().as_ref(),
            state.show_attributes,
        );

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

        // Render a scrollbar on the right edge of the file tree pane.
        let mut scrollbar_state =
            ScrollbarState::new(total_rows).position(state.tree_scroll_offset);
        let scrollbar = Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None);
        frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
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
            layers: vec![Layer::new(0, "ADD files", 100, tree)],
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
    fn test_file_tree_scrolls_to_keep_selection_visible() {
        let mut tree = FileTree::new();
        for i in 0..50 {
            let path = format!("file{}.txt", i);
            tree.add_path(
                &path,
                FileInfo {
                    entry_type: TarEntryType::Regular,
                    size: 10,
                    content_hash: i as u64,
                    ..Default::default()
                },
            );
        }
        tree.mark_all(DiffType::Added);

        let image = Image {
            reference: "test".into(),
            layers: vec![Layer::new(0, "ADD files", 500, tree)],
        };

        let mut state = AppState::new(image);
        state.focus = FocusPane::FileTree;
        // Select a node well below the initial viewport.
        state.selected_tree_path = Some("file40.txt".into());
        let mut comparer = Comparer::new(state.image.layers.clone());
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| FileTreeWidget::render(f, f.area(), &mut state, &mut comparer))
            .unwrap();

        assert!(
            state.tree_scroll_offset > 0,
            "scroll offset should move to keep selection visible"
        );
    }

    #[test]
    fn test_file_tree_scrolls_up_to_keep_selection_visible() {
        let mut tree = FileTree::new();
        for i in 0..50 {
            let path = format!("file{}.txt", i);
            tree.add_path(
                &path,
                FileInfo {
                    entry_type: TarEntryType::Regular,
                    size: 10,
                    content_hash: i as u64,
                    ..Default::default()
                },
            );
        }
        tree.mark_all(DiffType::Added);

        let image = Image {
            reference: "test".into(),
            layers: vec![Layer::new(0, "ADD files", 500, tree)],
        };

        let mut state = AppState::new(image);
        state.focus = FocusPane::FileTree;
        state.selected_tree_path = Some("file0.txt".into());
        // Start scrolled far down; render should snap back to show the selection.
        state.tree_scroll_offset = 40;
        let mut comparer = Comparer::new(state.image.layers.clone());
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| FileTreeWidget::render(f, f.area(), &mut state, &mut comparer))
            .unwrap();

        assert_eq!(
            state.tree_scroll_offset, 0,
            "scroll offset should snap back to top when selection is above viewport"
        );
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
            layers: vec![Layer::new(0, "ADD files", 110, tree)],
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
