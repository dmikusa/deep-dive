#![allow(dead_code)]

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use regex::Regex;

use crate::analysis::filetree::{DiffType, FileTree, SortMode, TarEntryType};
use crate::analysis::report::Report;
use crate::config::Config;
use crate::image::Image;
use crate::utils::expand_tilde;

/// Display data for a single layer in the layer list.
#[derive(Debug, Clone)]
pub struct LayerData {
    pub index: usize,
    pub command: String,
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusPane {
    #[default]
    LayerList,
    FileTree,
    LayerDetails,
    ImageDetails,
}

impl FocusPane {
    /// All focusable panes in their natural order.
    const ORDER: [Self; 4] = [
        Self::LayerList,
        Self::FileTree,
        Self::LayerDetails,
        Self::ImageDetails,
    ];

    pub fn next(self) -> Self {
        let mut iter = Self::ORDER.iter().cycle();
        while let Some(pane) = iter.next() {
            if *pane == self {
                return *iter.next().unwrap_or(&Self::LayerList);
            }
        }
        Self::LayerList
    }

    pub fn prev(self) -> Self {
        let mut iter = Self::ORDER.iter().rev().cycle();
        while let Some(pane) = iter.next() {
            if *pane == self {
                return *iter.next().unwrap_or(&Self::LayerList);
            }
        }
        Self::LayerList
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompareMode {
    #[default]
    Natural,
    Aggregated,
}

/// Modal overlay state for the TUI.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ModalState {
    #[default]
    None,
    ExtractTo {
        destination: String,
        original_path: String,
    },
    OpenImage {
        url: String,
    },
}

/// Mutable application state for the TUI.
#[derive(Debug)]
pub struct AppState {
    pub image: Image,
    pub config: Config,
    pub selected_layer: usize,
    pub collapsed_paths: HashSet<String>,
    pub hidden_diff_types: HashSet<DiffType>,
    pub sort_mode: SortMode,
    pub compare_mode: CompareMode,
    pub focus: FocusPane,
    pub show_attributes: bool,
    pub filter_text: String,
    pub is_filter_active: bool,
    pub selected_tree_path: Option<String>,
    pub tree_scroll_offset: usize,
    pub wrap_tree: bool,
    pub status_message: Option<String>,
    pub report: Option<Report>,
    pub modal: ModalState,
}

impl AppState {
    pub fn new(image: Image) -> Self {
        Self::with_config(image, Config::default())
    }

    pub fn with_config(image: Image, config: Config) -> Self {
        let selected_layer = if image.layers.is_empty() {
            0
        } else {
            image.layers.len() - 1
        };

        let compare_mode = config
            .compare_mode
            .as_deref()
            .and_then(|s| match s.to_ascii_lowercase().as_str() {
                "aggregated" => Some(CompareMode::Aggregated),
                "natural" => Some(CompareMode::Natural),
                _ => None,
            })
            .unwrap_or_default();

        let sort_mode = config
            .sort_mode
            .as_deref()
            .and_then(|s| match s.to_ascii_lowercase().as_str() {
                "size" => Some(SortMode::Size),
                "name" => Some(SortMode::Name),
                _ => None,
            })
            .unwrap_or_default();

        Self {
            image,
            config: config.clone(),
            selected_layer,
            collapsed_paths: HashSet::new(),
            hidden_diff_types: HashSet::new(),
            sort_mode,
            compare_mode,
            focus: FocusPane::default(),
            show_attributes: config.show_attributes.unwrap_or(false),
            filter_text: String::new(),
            is_filter_active: false,
            selected_tree_path: None,
            tree_scroll_offset: 0,
            wrap_tree: config.wrap_tree.unwrap_or(false),
            status_message: None,
            report: None,
            modal: ModalState::default(),
        }
    }

    pub fn layers(&self) -> Vec<LayerData> {
        self.image
            .layers
            .iter()
            .map(|l| LayerData {
                index: l.index,
                command: l.command.clone(),
                size: l.size,
            })
            .collect()
    }

    pub fn layer_count(&self) -> usize {
        self.image.layers.len()
    }

    pub fn select_next_layer(&mut self) {
        if self.selected_layer + 1 < self.image.layers.len() {
            self.selected_layer += 1;
            self.selected_tree_path = None;
            self.tree_scroll_offset = 0;
        }
    }

    pub fn select_prev_layer(&mut self) {
        if self.selected_layer > 0 {
            self.selected_layer -= 1;
            self.selected_tree_path = None;
            self.tree_scroll_offset = 0;
        }
    }

    pub fn cycle_focus(&mut self) {
        self.focus = self.focus.next();
    }

    pub fn cycle_focus_reverse(&mut self) {
        self.focus = self.focus.prev();
    }

    /// Move the file-tree selection to the next visible node.
    pub fn select_next_tree_node(&mut self, tree: &FileTree) {
        let paths = self.filtered_visible_paths(tree);
        if paths.is_empty() {
            self.selected_tree_path = None;
            return;
        }

        let next_index = match &self.selected_tree_path {
            Some(path) => paths
                .iter()
                .position(|p| p == path)
                .map(|i| (i + 1).min(paths.len() - 1))
                .unwrap_or(0),
            None => 0,
        };
        self.selected_tree_path = Some(paths[next_index].clone());
    }

    /// Move the file-tree selection to the previous visible node.
    pub fn select_prev_tree_node(&mut self, tree: &FileTree) {
        let paths = self.filtered_visible_paths(tree);
        if paths.is_empty() {
            self.selected_tree_path = None;
            return;
        }

        let prev_index = match &self.selected_tree_path {
            Some(path) => paths
                .iter()
                .position(|p| p == path)
                .map(|i| i.saturating_sub(1))
                .unwrap_or(0),
            None => 0,
        };
        self.selected_tree_path = Some(paths[prev_index].clone());
    }

    /// Toggle collapse on the currently selected file-tree node.
    pub fn toggle_collapse_selected(&mut self) {
        if let Some(path) = &self.selected_tree_path {
            if self.collapsed_paths.contains(path) {
                self.collapsed_paths.remove(path);
            } else {
                self.collapsed_paths.insert(path.clone());
            }
        }
    }

    /// Collapse/expand helpers used by widgets.
    pub fn is_collapsed(&self, path: &str) -> bool {
        self.collapsed_paths.contains(path)
    }

    pub fn apply_collapsed_to_tree(&self, tree: &mut FileTree) {
        for path in &self.collapsed_paths {
            tree.collapse(path);
        }
    }

    fn visible_paths(tree: &FileTree) -> Vec<String> {
        tree.visible_paths()
    }

    pub fn filtered_visible_paths(&self, tree: &FileTree) -> Vec<String> {
        tree.visible_paths_filtered(&self.hidden_diff_types, self.filter_regex().as_ref())
    }

    pub fn filter_regex(&self) -> Option<Regex> {
        if self.is_filter_active && !self.filter_text.is_empty() {
            Regex::new(&self.filter_text).ok()
        } else {
            None
        }
    }

    pub fn page_down(&mut self, tree: &FileTree, page_height: usize) {
        let visible = self.filtered_visible_paths(tree);
        let max_offset = visible.len().saturating_sub(page_height);
        self.tree_scroll_offset = (self.tree_scroll_offset + page_height).min(max_offset);
    }

    pub fn page_up(&mut self, page_height: usize) {
        self.tree_scroll_offset = self.tree_scroll_offset.saturating_sub(page_height);
    }

    pub fn toggle_sort_mode(&mut self) {
        self.sort_mode = match self.sort_mode {
            SortMode::Name => SortMode::Size,
            SortMode::Size => SortMode::Name,
        };
    }

    pub fn toggle_show_attributes(&mut self) {
        self.show_attributes = !self.show_attributes;
    }

    pub fn toggle_wrap_tree(&mut self) {
        self.wrap_tree = !self.wrap_tree;
    }

    pub fn toggle_diff_type(&mut self, diff_type: DiffType) {
        if self.hidden_diff_types.contains(&diff_type) {
            self.hidden_diff_types.remove(&diff_type);
        } else {
            self.hidden_diff_types.insert(diff_type);
        }
    }

    pub fn toggle_filter_active(&mut self) {
        self.is_filter_active = !self.is_filter_active;
        if !self.is_filter_active {
            self.filter_text.clear();
        }
        self.tree_scroll_offset = 0;
        self.selected_tree_path = None;
    }

    pub fn push_filter_char(&mut self, c: char) {
        self.filter_text.push(c);
        self.tree_scroll_offset = 0;
    }

    pub fn pop_filter_char(&mut self) {
        self.filter_text.pop();
        self.tree_scroll_offset = 0;
    }

    pub fn collapse_all(&mut self, tree: &mut FileTree) {
        tree.collapse_all();
        self.collapsed_paths = tree.directory_paths().into_iter().collect();
    }

    pub fn expand_all(&mut self, tree: &mut FileTree) {
        tree.expand_all();
        self.collapsed_paths.clear();
    }

    pub fn extract_selected_to(&mut self, tree: &FileTree, dest: impl AsRef<Path>) -> Result<()> {
        let path = self
            .selected_tree_path
            .as_ref()
            .context("no file selected")?;
        let node = tree.get_node(path).context("selected node not found")?;
        let target = resolve_extract_destination(dest.as_ref(), path)?;

        match node.info.entry_type {
            TarEntryType::Regular => {
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("failed to create directory {:?}", parent))?;
                }
                std::fs::write(&target, &node.info.content)
                    .with_context(|| format!("failed to write {}", target.display()))?;
                self.status_message = Some(format!("Extracted {}", target.display()));
            }
            TarEntryType::Symlink => {
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("failed to create directory {:?}", parent))?;
                }
                #[cfg(unix)]
                {
                    std::os::unix::fs::symlink(&node.info.linkname, &target).with_context(
                        || format!("failed to create symlink {}", target.display()),
                    )?;
                    self.status_message = Some(format!("Extracted symlink {}", target.display()));
                }
                #[cfg(not(unix))]
                {
                    anyhow::bail!("symlink extraction is not supported on this platform");
                }
            }
            TarEntryType::Hardlink => {
                anyhow::bail!("cannot extract {}: hardlinks are not supported yet", path);
            }
            other => {
                anyhow::bail!(
                    "cannot extract {}: selected node is a {} (not a file or symlink)",
                    path,
                    format!("{:?}", other).to_lowercase()
                );
            }
        }

        Ok(())
    }

    pub fn open_extract_modal(&mut self, tree: &FileTree) -> Result<()> {
        let path = self
            .selected_tree_path
            .as_ref()
            .context("no file selected")?;
        let node = tree.get_node(path).context("selected node not found")?;
        if !matches!(
            node.info.entry_type,
            TarEntryType::Regular | TarEntryType::Symlink
        ) {
            anyhow::bail!(
                "cannot extract {}: selected node is not a file or symlink",
                path
            );
        }

        let default = self
            .config
            .extract_default_directory()
            .or_else(|| std::env::current_dir().ok())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());

        self.modal = ModalState::ExtractTo {
            destination: default,
            original_path: path.clone(),
        };
        Ok(())
    }
    pub fn confirm_extract_modal(&mut self, tree: &FileTree) -> Result<()> {
        if let ModalState::ExtractTo {
            ref destination, ..
        } = self.modal
        {
            let dest = expand_tilde(destination).context("invalid destination path")?;
            self.extract_selected_to(tree, &dest)?;
        }
        self.modal = ModalState::None;
        Ok(())
    }

    /// Confirm the "Open image" modal and return the entered URL.
    pub fn confirm_open_image_modal(&mut self) -> Option<String> {
        let url = match &self.modal {
            ModalState::OpenImage { url } => Some(url.clone()),
            _ => None,
        };
        self.modal = ModalState::None;
        url
    }

    pub fn cancel_modal(&mut self) {
        self.modal = ModalState::None;
    }

    pub fn is_modal_active(&self) -> bool {
        !matches!(self.modal, ModalState::None)
    }

    pub fn modal_input(&self) -> Option<&str> {
        match &self.modal {
            ModalState::ExtractTo { destination, .. } => Some(destination),
            ModalState::OpenImage { url } => Some(url),
            ModalState::None => None,
        }
    }

    pub fn modal_destination(&self) -> Option<&str> {
        match &self.modal {
            ModalState::ExtractTo { destination, .. } => Some(destination),
            _ => None,
        }
    }

    pub fn push_modal_char(&mut self, c: char) {
        match &mut self.modal {
            ModalState::ExtractTo { destination, .. } => destination.push(c),
            ModalState::OpenImage { url } => url.push(c),
            ModalState::None => {}
        }
    }

    pub fn pop_modal_char(&mut self) {
        match &mut self.modal {
            ModalState::ExtractTo { destination, .. } => {
                destination.pop();
            }
            ModalState::OpenImage { url } => {
                url.pop();
            }
            ModalState::None => {}
        }
    }

    pub fn open_image_modal(&mut self, current_url: &str) {
        self.modal = ModalState::OpenImage {
            url: current_url.to_string(),
        };
    }

    pub fn clear_status_message(&mut self) {
        self.status_message = None;
    }
}

/// Resolve the destination path for an extraction.
///
/// If `dest` ends with a path separator or exists as a directory, the image's
/// relative path is preserved inside `dest`. Otherwise `dest` is treated as the
/// exact output path.
fn resolve_extract_destination(dest: &Path, image_path: &str) -> Result<PathBuf> {
    let dest_str = dest.to_string_lossy();
    let is_dir_target = dest_str.ends_with(std::path::MAIN_SEPARATOR)
        || dest_str.ends_with('/')
        || dest_str.ends_with('\\')
        || dest.is_dir();

    if is_dir_target {
        Ok(dest.join(image_path))
    } else {
        Ok(dest.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::filetree::FileInfo;

    fn empty_image() -> Image {
        Image {
            reference: String::new(),
            layers: Vec::new(),
        }
    }

    fn image_with_layers(count: usize) -> Image {
        let layers = (0..count)
            .map(|i| {
                crate::image::Layer::new(i, format!("layer {}", i), i as u64 * 100, FileTree::new())
            })
            .collect();
        Image {
            reference: "test".into(),
            layers,
        }
    }

    fn tree_with_nodes() -> FileTree {
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
            "bin/ls",
            FileInfo {
                entry_type: TarEntryType::Regular,
                size: 50,
                content_hash: 2,
                ..Default::default()
            },
        );
        tree.add_path(
            "etc/passwd",
            FileInfo {
                entry_type: TarEntryType::Regular,
                size: 10,
                content_hash: 3,
                ..Default::default()
            },
        );
        tree
    }

    #[test]
    fn test_new_selects_last_layer() {
        let image = image_with_layers(3);
        let state = AppState::new(image);
        assert_eq!(state.selected_layer, 2);
    }

    #[test]
    fn test_select_next_layer() {
        let image = image_with_layers(3);
        let mut state = AppState::new(image);
        state.selected_layer = 0;
        state.select_next_layer();
        assert_eq!(state.selected_layer, 1);
        state.select_next_layer();
        assert_eq!(state.selected_layer, 2);
        state.select_next_layer();
        assert_eq!(state.selected_layer, 2);
    }

    #[test]
    fn test_select_prev_layer() {
        let image = image_with_layers(3);
        let mut state = AppState::new(image);
        state.select_prev_layer();
        assert_eq!(state.selected_layer, 1);
        state.select_prev_layer();
        assert_eq!(state.selected_layer, 0);
        state.select_prev_layer();
        assert_eq!(state.selected_layer, 0);
    }

    #[test]
    fn test_cycle_focus() {
        let mut state = AppState::new(empty_image());
        assert_eq!(state.focus, FocusPane::LayerList);
        state.cycle_focus();
        assert_eq!(state.focus, FocusPane::FileTree);
        state.cycle_focus();
        assert_eq!(state.focus, FocusPane::LayerDetails);
        state.cycle_focus();
        assert_eq!(state.focus, FocusPane::ImageDetails);
        state.cycle_focus();
        assert_eq!(state.focus, FocusPane::LayerList);
    }

    #[test]
    fn test_cycle_focus_reverse() {
        let mut state = AppState::new(empty_image());
        state.cycle_focus_reverse();
        assert_eq!(state.focus, FocusPane::ImageDetails);
        state.cycle_focus_reverse();
        assert_eq!(state.focus, FocusPane::LayerDetails);
    }

    #[test]
    fn test_tree_navigation() {
        let mut state = AppState::new(empty_image());
        let tree = tree_with_nodes();

        state.select_next_tree_node(&tree);
        assert!(state.selected_tree_path.is_some());

        let first = state.selected_tree_path.clone().unwrap();
        state.select_next_tree_node(&tree);
        let second = state.selected_tree_path.clone().unwrap();
        assert_ne!(first, second);

        state.select_prev_tree_node(&tree);
        assert_eq!(state.selected_tree_path, Some(first));
    }

    #[test]
    fn test_toggle_collapse() {
        let mut state = AppState::new(empty_image());
        state.selected_tree_path = Some("bin".into());

        assert!(!state.is_collapsed("bin"));
        state.toggle_collapse_selected();
        assert!(state.is_collapsed("bin"));
        state.toggle_collapse_selected();
        assert!(!state.is_collapsed("bin"));
    }

    #[test]
    fn test_apply_collapsed_to_tree() {
        let mut state = AppState::new(empty_image());
        state.selected_tree_path = Some("bin".into());
        state.toggle_collapse_selected();

        let mut tree = tree_with_nodes();
        state.apply_collapsed_to_tree(&mut tree);
        assert!(tree.get_node("bin").unwrap().collapsed);
    }

    #[test]
    fn test_toggle_sort_mode() {
        let mut state = AppState::new(empty_image());
        assert_eq!(state.sort_mode, SortMode::Name);
        state.toggle_sort_mode();
        assert_eq!(state.sort_mode, SortMode::Size);
        state.toggle_sort_mode();
        assert_eq!(state.sort_mode, SortMode::Name);
    }

    #[test]
    fn test_toggle_show_attributes() {
        let mut state = AppState::new(empty_image());
        assert!(!state.show_attributes);
        state.toggle_show_attributes();
        assert!(state.show_attributes);
    }

    #[test]
    fn test_toggle_wrap_tree() {
        let mut state = AppState::new(empty_image());
        assert!(!state.wrap_tree);
        state.toggle_wrap_tree();
        assert!(state.wrap_tree);
    }

    #[test]
    fn test_toggle_diff_type() {
        let mut state = AppState::new(empty_image());
        assert!(!state.hidden_diff_types.contains(&DiffType::Added));
        state.toggle_diff_type(DiffType::Added);
        assert!(state.hidden_diff_types.contains(&DiffType::Added));
        state.toggle_diff_type(DiffType::Added);
        assert!(!state.hidden_diff_types.contains(&DiffType::Added));
    }

    #[test]
    fn test_filter_regex() {
        let mut state = AppState::new(empty_image());
        assert!(state.filter_regex().is_none());
        state.is_filter_active = true;
        state.filter_text = "bin.*".to_string();
        assert!(state.filter_regex().is_some());
        state.filter_text = "[".to_string();
        assert!(state.filter_regex().is_none());
    }

    #[test]
    fn test_filter_text_typing() {
        let mut state = AppState::new(empty_image());
        state.toggle_filter_active();
        state.push_filter_char('a');
        state.push_filter_char('b');
        assert_eq!(state.filter_text, "ab");
        state.pop_filter_char();
        assert_eq!(state.filter_text, "a");
    }

    #[test]
    fn test_collapse_expand_all() {
        let mut state = AppState::new(empty_image());
        let mut tree = tree_with_nodes();
        state.collapse_all(&mut tree);
        assert!(state.collapsed_paths.contains("bin"));
        assert!(state.collapsed_paths.contains("etc"));
        state.expand_all(&mut tree);
        assert!(state.collapsed_paths.is_empty());
    }

    #[test]
    fn test_extract_selected_to_directory() {
        let mut tree = FileTree::new();
        let mut info = file_info(0, 1);
        info.content = b"hello world".to_vec();
        tree.add_path("dir/file.txt", info);

        let temp_dir = tempfile::tempdir().unwrap();
        let mut state = AppState::new(empty_image());
        state.selected_tree_path = Some("dir/file.txt".to_string());
        state.extract_selected_to(&tree, temp_dir.path()).unwrap();

        let written = std::fs::read(temp_dir.path().join("dir/file.txt")).unwrap();
        assert_eq!(written, b"hello world");
    }

    #[test]
    fn test_extract_selected_to_file_path() {
        let mut tree = FileTree::new();
        let mut info = file_info(0, 1);
        info.content = b"hello world".to_vec();
        tree.add_path("dir/file.txt", info);

        let temp_dir = tempfile::tempdir().unwrap();
        let output = temp_dir.path().join("renamed.txt");
        let mut state = AppState::new(empty_image());
        state.selected_tree_path = Some("dir/file.txt".to_string());
        state.extract_selected_to(&tree, &output).unwrap();

        let written = std::fs::read(&output).unwrap();
        assert_eq!(written, b"hello world");
    }

    #[test]
    fn test_extract_selected_requires_regular_file() {
        let mut tree = FileTree::new();
        tree.add_path("dir", dir_info());

        let temp_dir = tempfile::tempdir().unwrap();
        let mut state = AppState::new(empty_image());
        state.selected_tree_path = Some("dir".to_string());
        assert!(state.extract_selected_to(&tree, temp_dir.path()).is_err());
    }

    #[test]
    fn test_resolve_extract_destination_directory() {
        let dir = tempfile::tempdir().unwrap();
        let dest = resolve_extract_destination(dir.path(), "a/b.txt").unwrap();
        assert_eq!(dest, dir.path().join("a/b.txt"));
    }

    #[test]
    fn test_resolve_extract_destination_trailing_separator() {
        let dir = tempfile::tempdir().unwrap();
        let dest =
            resolve_extract_destination(dir.path().join("out").as_path(), "a/b.txt").unwrap();
        // Without trailing separator and path doesn't exist, treat as file.
        assert_eq!(dest, dir.path().join("out"));
    }

    #[test]
    fn test_resolve_extract_destination_exact_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("renamed.txt");
        let dest = resolve_extract_destination(&file, "a/b.txt").unwrap();
        assert_eq!(dest, file);
    }

    #[test]
    fn test_modal_open_and_cancel() {
        let mut tree = FileTree::new();
        let mut info = file_info(0, 1);
        info.content = b"hello".to_vec();
        tree.add_path("file.txt", info);

        let mut state = AppState::new(empty_image());
        state.selected_tree_path = Some("file.txt".to_string());
        state.open_extract_modal(&tree).unwrap();
        assert!(state.is_modal_active());
        assert!(state.modal_destination().is_some());

        state.cancel_modal();
        assert!(!state.is_modal_active());
    }

    #[test]
    fn test_modal_typing() {
        let mut tree = FileTree::new();
        let mut info = file_info(0, 1);
        info.content = b"hello".to_vec();
        tree.add_path("file.txt", info);

        let mut state = AppState::new(empty_image());
        state.selected_tree_path = Some("file.txt".to_string());
        state.open_extract_modal(&tree).unwrap();
        state.push_modal_char('/');
        state.push_modal_char('t');
        state.push_modal_char('o');
        state.pop_modal_char();
        assert!(state.modal_destination().unwrap().contains("/t"));
    }

    #[test]
    fn test_modal_confirm_extract() {
        let mut tree = FileTree::new();
        let mut info = file_info(0, 1);
        info.content = b"hello world".to_vec();
        tree.add_path("dir/file.txt", info);

        let temp_dir = tempfile::tempdir().unwrap();
        let mut state = AppState::new(empty_image());
        state.selected_tree_path = Some("dir/file.txt".to_string());
        state.open_extract_modal(&tree).unwrap();
        state.modal = ModalState::ExtractTo {
            destination: temp_dir.path().to_string_lossy().to_string(),
            original_path: "dir/file.txt".to_string(),
        };
        state.confirm_extract_modal(&tree).unwrap();

        assert!(!state.is_modal_active());
        let written = std::fs::read(temp_dir.path().join("dir/file.txt")).unwrap();
        assert_eq!(written, b"hello world");
        assert!(state.status_message.as_ref().unwrap().contains("Extracted"));
    }

    #[test]
    fn test_extract_modal_defaults_to_config_directory() {
        let mut config = Config::default();
        let dir = tempfile::tempdir().unwrap();
        config.extract.default_directory = Some(dir.path().to_string_lossy().to_string());

        let mut tree = FileTree::new();
        let mut info = file_info(0, 1);
        info.content = b"x".to_vec();
        tree.add_path("file.txt", info);

        let mut state = AppState::with_config(empty_image(), config);
        state.selected_tree_path = Some("file.txt".to_string());
        state.open_extract_modal(&tree).unwrap();

        assert_eq!(
            PathBuf::from(state.modal_destination().unwrap()),
            dir.path()
        );
    }

    fn file_info(size: u64, content_hash: u64) -> FileInfo {
        FileInfo {
            size,
            content_hash,
            entry_type: TarEntryType::Regular,
            ..Default::default()
        }
    }

    fn dir_info() -> FileInfo {
        FileInfo {
            entry_type: TarEntryType::Directory,
            ..Default::default()
        }
    }
}
