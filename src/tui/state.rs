#![allow(dead_code)]

use std::collections::HashSet;

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
        }
    }

    pub fn select_prev_layer(&mut self) {
        if self.selected_layer > 0 {
            self.selected_layer -= 1;
            self.selected_tree_path = None;
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
        let paths = Self::visible_paths(tree);
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
        let paths = Self::visible_paths(tree);
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
}
