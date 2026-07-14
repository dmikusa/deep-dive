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
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                    KeyCode::Tab => state.cycle_focus(),
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
    StatusBarWidget::render(frame, main_layout[1]);
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
