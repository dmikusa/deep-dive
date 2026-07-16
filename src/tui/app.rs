#![allow(dead_code)]

use std::io;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::backend::Backend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Terminal;

use crate::analysis::comparer::Comparer;
use crate::image::Image;
use crate::tui::state::{AppState, FocusPane};
use crate::tui::widgets::file_tree::FileTreeWidget;
use crate::tui::widgets::layer_list::LayerListWidget;
use crate::tui::widgets::status_bar::StatusBarWidget;

pub async fn run(image: Image) -> Result<()> {
    let mut terminal = ratatui::init();
    let mut state = AppState::new(image);
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
                if state.is_filter_active {
                    handle_filter_key(state, key);
                    continue;
                }

                state.clear_status_message();

                let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('c') if ctrl => break,
                    KeyCode::Tab => state.cycle_focus(),
                    KeyCode::Char(' ') if ctrl => {
                        let mut tree = current_tree(state, &mut comparer);
                        if state.collapsed_paths.is_empty() {
                            state.collapse_all(&mut tree);
                        } else {
                            state.expand_all(&mut tree);
                        }
                    }
                    KeyCode::Char('o') if ctrl => state.toggle_sort_mode(),
                    KeyCode::Char('b') if ctrl => state.toggle_show_attributes(),
                    KeyCode::Char('p') if ctrl => state.toggle_wrap_tree(),
                    KeyCode::Char('f') if ctrl => state.toggle_filter_active(),
                    KeyCode::Char('e') if ctrl => {
                        let tree = current_tree(state, &mut comparer);
                        if let Err(e) = state.extract_selected(&tree) {
                            state.status_message = Some(format!("Extract failed: {}", e));
                        }
                    }
                    KeyCode::Char('a') if ctrl => match state.focus {
                        FocusPane::LayerList => {
                            state.compare_mode = crate::tui::state::CompareMode::Aggregated;
                        }
                        FocusPane::FileTree => {
                            state.toggle_diff_type(crate::analysis::filetree::DiffType::Added);
                        }
                    },
                    KeyCode::Char('l') if ctrl => {
                        if state.focus == FocusPane::LayerList {
                            state.compare_mode = crate::tui::state::CompareMode::Natural;
                        }
                    }
                    KeyCode::Char('r') if ctrl => {
                        if state.focus == FocusPane::FileTree {
                            state.toggle_diff_type(crate::analysis::filetree::DiffType::Removed);
                        }
                    }
                    KeyCode::Char('m') if ctrl => {
                        if state.focus == FocusPane::FileTree {
                            state.toggle_diff_type(crate::analysis::filetree::DiffType::Modified);
                        }
                    }
                    KeyCode::Char('u') if ctrl => {
                        if state.focus == FocusPane::FileTree {
                            state.toggle_diff_type(crate::analysis::filetree::DiffType::Unmodified);
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => match state.focus {
                        FocusPane::LayerList => state.select_prev_layer(),
                        FocusPane::FileTree => {
                            let tree = current_tree(state, &mut comparer);
                            state.select_prev_tree_node(&tree);
                        }
                    },
                    KeyCode::Down | KeyCode::Char('j') => match state.focus {
                        FocusPane::LayerList => state.select_next_layer(),
                        FocusPane::FileTree => {
                            let tree = current_tree(state, &mut comparer);
                            state.select_next_tree_node(&tree);
                        }
                    },
                    KeyCode::Enter | KeyCode::Char(' ') => state.toggle_collapse_selected(),
                    KeyCode::PageUp | KeyCode::Char('u') => {
                        state.page_up(page_height(terminal));
                    }
                    KeyCode::PageDown | KeyCode::Char('d') => {
                        let tree = current_tree(state, &mut comparer);
                        state.page_down(&tree, page_height(terminal));
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

fn ui(frame: &mut ratatui::Frame, state: &mut AppState, comparer: &mut Comparer) {
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(frame.area());

    let content_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(main_layout[0]);

    LayerListWidget::render(frame, content_layout[0], state);
    FileTreeWidget::render(frame, content_layout[1], state, comparer);
    StatusBarWidget::render(frame, main_layout[1], state);
}

fn current_tree(state: &AppState, comparer: &mut Comparer) -> crate::analysis::filetree::FileTree {
    let (bs, bstop, ts, tstop) = match state.compare_mode {
        crate::tui::state::CompareMode::Natural => {
            let indexes = comparer.natural_indexes();
            indexes[state.selected_layer.min(indexes.len().saturating_sub(1))]
        }
        crate::tui::state::CompareMode::Aggregated => {
            let indexes = comparer.aggregated_indexes();
            indexes[state.selected_layer.min(indexes.len().saturating_sub(1))]
        }
    };

    let mut tree = comparer.get_tree(bs, bstop, ts, tstop).clone();
    tree.set_sort_mode(state.sort_mode);
    state.apply_collapsed_to_tree(&mut tree);
    tree
}

fn handle_filter_key(state: &mut AppState, key: event::KeyEvent) {
    match key.code {
        event::KeyCode::Esc => state.toggle_filter_active(),
        event::KeyCode::Char(c) => state.push_filter_char(c),
        event::KeyCode::Backspace => state.pop_filter_char(),
        event::KeyCode::Enter => {
            // Keep filter active; user can press Esc or Ctrl+F to close.
        }
        _ => {}
    }
}

fn page_height<B: Backend>(terminal: &Terminal<B>) -> usize {
    terminal
        .size()
        .map(|s| s.height.saturating_sub(3) as usize)
        .unwrap_or(10)
}
