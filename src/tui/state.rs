#![allow(dead_code)]

use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result};
use regex::Regex;

use crate::analysis::filetree::{DiffType, FileTree, SortMode};
use crate::image::Image;

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompareMode {
    #[default]
    Natural,
    Aggregated,
}

/// Mutable application state for the TUI.
#[derive(Debug, Clone)]
pub struct AppState {
    pub image: Image,
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
}

impl AppState {
    pub fn new(image: Image) -> Self {
        let selected_layer = if image.layers.is_empty() {
            0
        } else {
            image.layers.len() - 1
        };
        Self {
            image,
            selected_layer,
            collapsed_paths: HashSet::new(),
            hidden_diff_types: HashSet::new(),
            sort_mode: SortMode::default(),
            compare_mode: CompareMode::default(),
            focus: FocusPane::default(),
            show_attributes: false,
            filter_text: String::new(),
            is_filter_active: false,
            selected_tree_path: None,
            tree_scroll_offset: 0,
            wrap_tree: false,
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
        self.focus = match self.focus {
            FocusPane::LayerList => FocusPane::FileTree,
            FocusPane::FileTree => FocusPane::LayerList,
        };
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

    pub fn extract_selected(&self, tree: &FileTree) -> Result<()> {
        let path = self
            .selected_tree_path
            .as_ref()
            .context("no file selected")?;
        let node = tree.get_node(path).context("selected node not found")?;
        anyhow::ensure!(
            node.info.entry_type == crate::analysis::filetree::TarEntryType::Regular,
            "selected node is not a regular file"
        );
        let filename = Path::new(path)
            .file_name()
            .context("invalid path")?
            .to_string_lossy()
            .into_owned();
        std::fs::write(&filename, &node.info.content)
            .with_context(|| format!("failed to write {}", filename))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::filetree::{FileInfo, TarEntryType};

    fn empty_image() -> Image {
        Image {
            reference: String::new(),
            layers: Vec::new(),
        }
    }

    fn image_with_layers(count: usize) -> Image {
        let layers = (0..count)
            .map(|i| crate::image::Layer {
                index: i,
                command: format!("layer {}", i),
                size: i as u64 * 100,
                tree: FileTree::new(),
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
        assert_eq!(state.focus, FocusPane::LayerList);
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
    fn test_extract_selected_writes_file() {
        let mut tree = FileTree::new();
        let mut info = file_info(0, 1);
        info.content = b"hello world".to_vec();
        tree.add_path("dir/file.txt", info);

        let temp_dir = tempfile::tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let mut state = AppState::new(empty_image());
        state.selected_tree_path = Some("dir/file.txt".to_string());
        let result = state.extract_selected(&tree);

        std::env::set_current_dir(&original_dir).unwrap();
        result.unwrap();

        let written = std::fs::read(temp_dir.path().join("file.txt")).unwrap();
        assert_eq!(written, b"hello world");
    }

    #[test]
    fn test_extract_selected_requires_regular_file() {
        let mut tree = FileTree::new();
        tree.add_path("dir", dir_info());

        let mut state = AppState::new(empty_image());
        state.selected_tree_path = Some("dir".to_string());
        assert!(state.extract_selected(&tree).is_err());
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
