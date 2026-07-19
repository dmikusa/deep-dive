use std::io;
use std::time::Duration;

use anyhow::{bail, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::backend::Backend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Terminal;

use crate::analysis::comparer::Comparer;
use crate::analysis::filetree::DiffType;
use crate::analysis::report::{Analyzer, Report};
use crate::config::Config;
use crate::image::progress::Progress;
use crate::image::Image;
use crate::tui::state::{AppState, CompareMode, FocusPane};
use crate::tui::widgets::file_tree::FileTreeWidget;
use crate::tui::widgets::filter::FilterWidget;
use crate::tui::widgets::image_details::ImageDetailsWidget;
use crate::tui::widgets::layer_details::LayerDetailsWidget;
use crate::tui::widgets::layer_list::LayerListWidget;
use crate::tui::widgets::loading::LoadingWidget;
use crate::tui::widgets::modal::ModalWidget;
use crate::tui::widgets::status_bar::StatusBarWidget;

/// Action produced by the main app loop that tells the caller what to do next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppAction {
    /// Exit the application.
    Quit,
    /// Load a different image and restart the main loop.
    OpenImage(String),
}

pub async fn run(
    image_ref: String,
    analyzers: Vec<Box<dyn Analyzer>>,
    config: Config,
) -> Result<()> {
    let mut terminal = ratatui::init();
    let result = run_loop(&mut terminal, image_ref, analyzers, config).await;
    ratatui::restore();
    result
}

async fn run_loop<B: Backend>(
    terminal: &mut Terminal<B>,
    mut image_ref: String,
    analyzers: Vec<Box<dyn Analyzer>>,
    config: Config,
) -> Result<()>
where
    <B as Backend>::Error: Send + Sync + 'static,
{
    loop {
        let (image, report) = run_loading(terminal, &image_ref, &analyzers, &config).await?;
        let action = run_app(terminal, image, report, config.clone()).await?;
        match action {
            AppAction::Quit => break,
            AppAction::OpenImage(new_ref) => {
                image_ref = new_ref;
                continue;
            }
        }
    }
    Ok(())
}

async fn run_loading<B: Backend>(
    terminal: &mut Terminal<B>,
    image_ref: &str,
    analyzers: &[Box<dyn Analyzer>],
    config: &Config,
) -> Result<(Image, Report)>
where
    <B as Backend>::Error: Send + Sync + 'static,
{
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<Progress>(32);
    let status_ref = image_ref.to_string();
    let image_ref = image_ref.to_string();
    let mut handle =
        tokio::spawn(
            async move { crate::image::resolve_with_progress(&image_ref, progress_tx).await },
        );

    let mut status = format!("Loading {}", status_ref);
    let mut progress: Option<(u64, Option<u64>)> = None;

    loop {
        terminal.draw(|f| LoadingWidget::render(f, f.area(), &status, progress))?;

        tokio::select! {
            msg = progress_rx.recv() => {
                if let Some(msg) = msg {
                    match msg {
                        Progress::Status(s) => status = s,
                        Progress::Bytes { current, total } => progress = Some((current, total)),
                    }
                }
            }
            result = &mut handle => {
                let image = result.map_err(|e| anyhow::anyhow!(e))??;
                let report = Report::generate(&image, analyzers)?;
                return Ok((image, report));
            }
            _ = tokio::time::sleep(Duration::from_millis(50)) => {
                match tokio::task::spawn_blocking(|| {
                    if event::poll(Duration::ZERO).unwrap_or(false) {
                        event::read().ok()
                    } else {
                        None
                    }
                })
                .await
                {
                    Ok(Some(Event::Key(key)))
                        if key.kind == KeyEventKind::Press
                            && (config.key_matches("quit", key)
                                || (key.modifiers.contains(KeyModifiers::CONTROL)
                                    && matches!(key.code, KeyCode::Char('c')))) =>
                    {
                        handle.abort();
                        bail!("quit");
                    }
                    _ => {}
                }
            }
        }
    }
}

async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    image: Image,
    report: Report,
    config: Config,
) -> Result<AppAction>
where
    <B as Backend>::Error: Send + Sync + 'static,
{
    let mut state = AppState::with_config(image, config);
    state.report = Some(report);
    let mut comparer = Comparer::new(state.image.layers.clone());
    comparer.build_cache();

    loop {
        terminal.draw(|f| ui(f, &mut state, &mut comparer))?;

        let event = tokio::task::spawn_blocking(event::read)
            .await
            .map_err(io::Error::other)??;

        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Press {
                if state.is_modal_active() {
                    if let Some(action) = handle_modal_key(&mut state, key, &mut comparer) {
                        return Ok(action);
                    }
                    continue;
                }

                if state.is_filter_active {
                    handle_filter_key(&mut state, key);
                    continue;
                }

                state.clear_status_message();

                if state.config.key_matches("quit", key) {
                    return Ok(AppAction::Quit);
                }
                if state.config.key_matches("open_image", key) {
                    state.open_image_modal(&state.image.reference.clone());
                    continue;
                }
                if state.config.key_matches("focus_next", key) {
                    state.cycle_focus();
                    continue;
                }
                if state.config.key_matches("focus_prev", key) {
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
                    FocusPane::LayerList => {
                        handle_layer_list_keys(&mut state, key, terminal, &mut comparer)
                    }
                    FocusPane::FileTree => {
                        handle_file_tree_keys(&mut state, key, terminal, &mut comparer)
                    }
                    FocusPane::LayerDetails => handle_layer_details_keys(&mut state, key),
                    FocusPane::ImageDetails => {
                        // Image details is view-only; Tab/arrow keys handle focus.
                    }
                }
            }
        }
    }
}

fn handle_layer_list_keys<B: Backend>(
    state: &mut AppState,
    key: event::KeyEvent,
    terminal: &Terminal<B>,
    comparer: &mut Comparer,
) {
    let page_size = page_height(terminal);
    if state.config.key_matches("next_layer", key) {
        state.select_next_layer();
    } else if state.config.key_matches("prev_layer", key) {
        state.select_prev_layer();
    } else if state.config.key_matches("page_up", key) {
        state.selected_layer = state.selected_layer.saturating_sub(page_size);
    } else if state.config.key_matches("page_down", key) {
        state.selected_layer =
            (state.selected_layer + page_size).min(state.layer_count().saturating_sub(1));
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

fn handle_layer_details_keys(state: &mut AppState, key: event::KeyEvent) {
    let field_count = LayerDetailsWidget::field_count(state);
    if state.config.key_matches("next_layer", key)
        || state.config.key_matches("next_tree_node", key)
    {
        state.select_next_detail_field(field_count);
    } else if state.config.key_matches("prev_layer", key)
        || state.config.key_matches("prev_tree_node", key)
    {
        state.select_prev_detail_field(field_count);
    } else if state.config.key_matches("collapse", key) || key.code == event::KeyCode::Enter {
        let fields = LayerDetailsWidget::fields(state);
        if let Some(field) = fields.get(state.selected_detail_field) {
            state.open_detail_field_modal(field.label);
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

fn handle_modal_key(
    state: &mut AppState,
    key: event::KeyEvent,
    comparer: &mut Comparer,
) -> Option<AppAction> {
    if matches!(
        state.modal,
        crate::tui::state::ModalState::DetailField { .. }
    ) {
        if state.config.key_matches("next_layer", key) {
            state.select_next_layer();
        } else if state.config.key_matches("prev_layer", key) {
            state.select_prev_layer();
        } else if key.code == event::KeyCode::Esc || key.code == event::KeyCode::Enter {
            state.cancel_modal();
        } else if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c'))
        {
            if let Some(value) = state.detail_field_value() {
                match arboard::Clipboard::new() {
                    Ok(mut clipboard) => {
                        if let Err(e) = clipboard.set_text(&value) {
                            state.status_message = Some(format!("Copy failed: {}", e));
                        } else {
                            state.status_message = Some("Copied to clipboard".to_string());
                        }
                    }
                    Err(e) => {
                        state.status_message = Some(format!("Clipboard unavailable: {}", e));
                    }
                }
            }
        }
        return None;
    }

    if key.code == event::KeyCode::Esc {
        state.cancel_modal();
    } else if key.code == event::KeyCode::Enter {
        return state
            .confirm_open_image_modal()
            .map(AppAction::OpenImage)
            .or_else(|| {
                let tree = current_tree(state, comparer);
                if let Err(e) = state.confirm_extract_modal(&tree) {
                    state.status_message = Some(format!("Extract failed: {}", e));
                }
                None
            });
    } else if key.code == event::KeyCode::Backspace {
        state.pop_modal_char();
    } else if let event::KeyCode::Char(c) = key.code {
        state.push_modal_char(c);
    }
    None
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
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

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

    #[test]
    fn test_layer_list_page_keys_move_selection() {
        let image = Image {
            reference: "test".into(),
            layers: (0..20)
                .map(|i| Layer::new(i, format!("layer {}", i), 100, FileTree::new()))
                .collect(),
        };
        let mut state = AppState::new(image);
        state.focus = FocusPane::LayerList;
        state.selected_layer = 10;

        let backend = TestBackend::new(40, 24);
        let terminal = Terminal::new(backend).unwrap();

        let layers = state.image.layers.clone();
        handle_layer_list_keys(
            &mut state,
            key(KeyCode::PageUp, false),
            &terminal,
            &mut Comparer::new(layers),
        );
        assert!(state.selected_layer < 10);

        let layers = state.image.layers.clone();
        state.selected_layer = 5;
        handle_layer_list_keys(
            &mut state,
            key(KeyCode::PageDown, false),
            &terminal,
            &mut Comparer::new(layers),
        );
        assert!(state.selected_layer > 5);
    }

    #[test]
    fn test_layer_details_keys_select_and_open_modal() {
        let mut state = test_state();
        state.focus = FocusPane::LayerDetails;
        state.selected_layer = 1;

        assert_eq!(state.selected_detail_field, 0);
        handle_layer_details_keys(&mut state, key(KeyCode::Down, false));
        assert_eq!(state.selected_detail_field, 1);

        handle_layer_details_keys(&mut state, key(KeyCode::Enter, false));
        assert!(state.is_modal_active());
        assert!(state.detail_field_label().is_some());
    }
}
