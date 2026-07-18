use std::io;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::backend::Backend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Terminal;

use crate::analysis::comparer::Comparer;
use crate::analysis::filetree::DiffType;
use crate::analysis::report::Report;
use crate::config::Config;
use crate::image::Image;
use crate::tui::state::{AppState, CompareMode, FocusPane};
use crate::tui::widgets::file_tree::FileTreeWidget;
use crate::tui::widgets::filter::FilterWidget;
use crate::tui::widgets::image_details::ImageDetailsWidget;
use crate::tui::widgets::layer_details::LayerDetailsWidget;
use crate::tui::widgets::layer_list::LayerListWidget;
use crate::tui::widgets::modal::ModalWidget;
use crate::tui::widgets::status_bar::StatusBarWidget;

pub async fn run(image: Image, report: Report, config: Config) -> Result<()> {
    let mut terminal = ratatui::init();
    let mut state = AppState::with_config(image, config);
    state.report = Some(report);
    let result = run_app(&mut terminal, &mut state).await;
    ratatui::restore();
    result
}

async fn run_app<B: Backend>(terminal: &mut Terminal<B>, state: &mut AppState) -> Result<()> {
    let mut comparer = Comparer::new(state.image.layers.clone());
    comparer.build_cache();

    loop {
        terminal.draw(|f| ui(f, state, &mut comparer))?;

        let event = tokio::task::spawn_blocking(event::read)
            .await
            .map_err(io::Error::other)??;

        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Press {
                if state.is_modal_active() {
                    handle_modal_key(state, key, &mut comparer);
                    continue;
                }

                if state.is_filter_active {
                    handle_filter_key(state, key);
                    continue;
                }

                state.clear_status_message();

                if state.config.key_matches("quit", key) {
                    break;
                }
                if state.config.key_matches("focus_next", key)
                    || (state.focus != FocusPane::FileTree
                        && matches!(key.code, KeyCode::Right | KeyCode::Char('l')))
                {
                    state.cycle_focus();
                    continue;
                }
                if state.config.key_matches("focus_prev", key)
                    || (state.focus != FocusPane::FileTree
                        && matches!(key.code, KeyCode::Left | KeyCode::Char('h')))
                {
                    state.cycle_focus_reverse();
                    continue;
                }
                if state.config.key_matches("filter", key) {
                    state.toggle_filter_active();
                    continue;
                }

                // Global toggles that are available from any pane.
                if state.config.key_matches("toggle_attributes", key) {
                    state.toggle_show_attributes();
                    continue;
                }
                if state.config.key_matches("toggle_wrap", key) {
                    state.toggle_wrap_tree();
                    continue;
                }
                if state.config.key_matches("toggle_sort", key) {
                    state.toggle_sort_mode();
                    continue;
                }

                match state.focus {
                    FocusPane::LayerList => handle_layer_list_keys(state, key, &mut comparer),
                    FocusPane::FileTree => {
                        handle_file_tree_keys(state, key, terminal, &mut comparer)
                    }
                    FocusPane::LayerDetails | FocusPane::ImageDetails => {
                        // Detail panes are view-only; Tab/arrow keys handle focus.
                    }
                }
            }
        }
    }

    Ok(())
}

fn handle_layer_list_keys(state: &mut AppState, key: event::KeyEvent, comparer: &mut Comparer) {
    if state.config.key_matches("next_layer", key) {
        state.select_next_layer();
    } else if state.config.key_matches("prev_layer", key) {
        state.select_prev_layer();
    } else if state.config.key_matches("collapse", key) {
        state.toggle_collapse_selected();
    } else if state.config.key_matches("compare_aggregated", key) {
        state.compare_mode = CompareMode::Aggregated;
    } else if state.config.key_matches("compare_natural", key) {
        state.compare_mode = CompareMode::Natural;
    } else if state.config.key_matches("collapse_all", key) {
        let mut tree = current_tree(state, comparer);
        if state.collapsed_paths.is_empty() {
            state.collapse_all(&mut tree);
        } else {
            state.expand_all(&mut tree);
        }
    }
}

fn handle_file_tree_keys<B: Backend>(
    state: &mut AppState,
    key: event::KeyEvent,
    terminal: &Terminal<B>,
    comparer: &mut Comparer,
) {
    if state.config.key_matches("next_tree_node", key) {
        let tree = current_tree(state, comparer);
        state.select_next_tree_node(&tree);
    } else if state.config.key_matches("prev_tree_node", key) {
        let tree = current_tree(state, comparer);
        state.select_prev_tree_node(&tree);
    } else if state.config.key_matches("collapse", key) {
        state.toggle_collapse_selected();
    } else if state.config.key_matches("collapse_all", key) {
        let mut tree = current_tree(state, comparer);
        if state.collapsed_paths.is_empty() {
            state.collapse_all(&mut tree);
        } else {
            state.expand_all(&mut tree);
        }
    } else if state.config.key_matches("page_up", key) {
        state.page_up(page_height(terminal));
    } else if state.config.key_matches("page_down", key) {
        let tree = current_tree(state, comparer);
        state.page_down(&tree, page_height(terminal));
    } else if state.config.key_matches("extract", key) {
        let tree = current_tree(state, comparer);
        if let Err(e) = state.open_extract_modal(&tree) {
            state.status_message = Some(format!("Extract failed: {}", e));
        }
    } else if state.config.key_matches("toggle_diff_added", key) {
        state.toggle_diff_type(DiffType::Added);
    } else if state.config.key_matches("toggle_diff_removed", key) {
        state.toggle_diff_type(DiffType::Removed);
    } else if state.config.key_matches("toggle_diff_modified", key) {
        state.toggle_diff_type(DiffType::Modified);
    } else if state.config.key_matches("toggle_diff_unmodified", key) {
        state.toggle_diff_type(DiffType::Unmodified);
    } else if key.code == KeyCode::Left || key.code == KeyCode::Char('h') {
        // Collapse the selected directory, if any.
        if let Some(path) = state.selected_tree_path.clone() {
            if !state.is_collapsed(&path) {
                state.collapsed_paths.insert(path);
            }
        }
    } else if key.code == KeyCode::Right || key.code == KeyCode::Char('l') {
        // Expand the selected directory, if any.
        if let Some(path) = state.selected_tree_path.clone() {
            state.collapsed_paths.remove(&path);
        }
    }
}

fn handle_filter_key(state: &mut AppState, key: event::KeyEvent) {
    if state.config.key_matches("filter", key) || key.code == event::KeyCode::Esc {
        state.toggle_filter_active();
    } else if key.code == event::KeyCode::Backspace {
        state.pop_filter_char();
    } else if let event::KeyCode::Char(c) = key.code {
        state.push_filter_char(c);
    }
}

fn handle_modal_key(state: &mut AppState, key: event::KeyEvent, comparer: &mut Comparer) {
    if key.code == event::KeyCode::Esc {
        state.cancel_modal();
    } else if key.code == event::KeyCode::Enter {
        let tree = current_tree(state, comparer);
        if let Err(e) = state.confirm_extract_modal(&tree) {
            state.status_message = Some(format!("Extract failed: {}", e));
        }
    } else if key.code == event::KeyCode::Backspace {
        state.pop_modal_char();
    } else if let event::KeyCode::Char(c) = key.code {
        state.push_modal_char(c);
    }
}

fn ui(frame: &mut ratatui::Frame, state: &mut AppState, comparer: &mut Comparer) {
    let filter_constraint = if state.is_filter_active {
        Constraint::Length(1)
    } else {
        Constraint::Length(0)
    };

    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), filter_constraint, Constraint::Length(1)])
        .split(frame.area());

    let content_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(main_layout[0]);

    let left_column = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(content_layout[0]);

    let right_column = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(content_layout[1]);

    LayerListWidget::render(frame, left_column[0], state);
    LayerDetailsWidget::render(frame, left_column[1], state);
    FileTreeWidget::render(frame, right_column[0], state, comparer);
    ImageDetailsWidget::render(frame, right_column[1], state);

    if state.is_filter_active {
        FilterWidget::render(frame, main_layout[1], state);
    }
    StatusBarWidget::render(frame, main_layout[2], state);

    if state.is_modal_active() {
        ModalWidget::render(frame, frame.area(), state);
    }
}

fn current_tree(state: &AppState, comparer: &mut Comparer) -> crate::analysis::filetree::FileTree {
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

    let mut tree = comparer.get_tree(bs, bstop, ts, tstop).clone();
    tree.set_sort_mode(state.sort_mode);
    state.apply_collapsed_to_tree(&mut tree);
    tree
}

fn page_height<B: Backend>(terminal: &Terminal<B>) -> usize {
    terminal
        .size()
        .map(|s| s.height.saturating_sub(3) as usize)
        .unwrap_or(10)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::filetree::FileTree;
    use crate::image::{Image, Layer};
    use crossterm::event::KeyModifiers;

    fn test_state() -> AppState {
        let image = Image {
            reference: "test".into(),
            layers: vec![
                Layer::new(0, "FROM scratch", 0, FileTree::new()),
                Layer::new(1, "ADD file", 100, FileTree::new()),
            ],
        };
        AppState::new(image)
    }

    fn key(code: KeyCode, ctrl: bool) -> event::KeyEvent {
        let modifiers = if ctrl {
            KeyModifiers::CONTROL
        } else {
            KeyModifiers::empty()
        };
        event::KeyEvent::new(code, modifiers)
    }

    #[test]
    fn test_quit_binding() {
        let state = test_state();
        assert!(state
            .config
            .key_matches("quit", key(KeyCode::Char('q'), false)));
        assert!(state
            .config
            .key_matches("quit", key(KeyCode::Char('c'), true)));
    }

    #[test]
    fn test_focus_next_binding() {
        let mut state = test_state();
        assert_eq!(state.focus, FocusPane::LayerList);
        state.cycle_focus();
        assert_eq!(state.focus, FocusPane::FileTree);
        state.cycle_focus();
        assert_eq!(state.focus, FocusPane::LayerDetails);
    }
}
