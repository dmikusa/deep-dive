#![allow(dead_code)]

use std::collections::HashMap;

use crate::image::Layer;

use super::filetree::FileTree;

/// Pre-computes and caches diff-marked file trees for layer ranges.
///
/// A cache key `(bottom_start, bottom_stop, top_start, top_stop)` represents
/// the comparison of the lower layer range against the upper layer range. The
/// merged tree is built by stacking the lower range and then the upper range
/// on top; it is then marked with `DiffType`s relative to the lower range.
pub struct Comparer {
    layers: Vec<Layer>,
    cache: HashMap<(usize, usize, usize, usize), FileTree>,
}

impl Comparer {
    pub fn new(layers: Vec<Layer>) -> Self {
        Self {
            layers,
            cache: HashMap::new(),
        }
    }

    /// Number of layers available to the comparer.
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// Natural indexes: each layer vs all previous layers.
    ///
    /// Layer 0 compares against an empty reference so all files are marked
    /// `Added`. Layer *i* compares layers `0..i` against layers `0..i-1`.
    pub fn natural_indexes(&self) -> Vec<(usize, usize, usize, usize)> {
        let mut indexes = Vec::new();
        for i in 0..self.layers.len() {
            if i == 0 {
                indexes.push((0, 0, 0, 0));
            } else {
                indexes.push((0, i - 1, i, i));
            }
        }
        indexes
    }

    /// Aggregated indexes: cumulative tree up to each layer vs the base layer.
    ///
    /// Layer *i* compares layers `0..i` against layer 0.
    pub fn aggregated_indexes(&self) -> Vec<(usize, usize, usize, usize)> {
        let mut indexes = Vec::new();
        for i in 0..self.layers.len() {
            indexes.push((0, 0, 0, i));
        }
        indexes
    }

    /// Return the diff-marked tree for the given layer range, computing and
    /// caching it if necessary.
    pub fn get_tree(
        &mut self,
        bottom_start: usize,
        bottom_stop: usize,
        top_start: usize,
        top_stop: usize,
    ) -> &FileTree {
        let key = (bottom_start, bottom_stop, top_start, top_stop);
        if !self.cache.contains_key(&key) {
            let merged = self.build_merged_tree(bottom_start, bottom_stop, top_start, top_stop);
            self.cache.insert(key, merged);
        }
        self.cache.get(&key).unwrap()
    }

    /// Pre-compute and cache all natural-index trees.
    pub fn build_cache(&mut self) {
        let indexes = self.natural_indexes();
        for (bs, bstop, ts, tstop) in indexes {
            self.get_tree(bs, bstop, ts, tstop);
        }
    }

    fn build_merged_tree(
        &self,
        bottom_start: usize,
        bottom_stop: usize,
        top_start: usize,
        top_stop: usize,
    ) -> FileTree {
        let lower = self.stack_range(bottom_start, bottom_stop);
        let upper = self.stack_range(top_start, top_stop);
        let mut merged = lower.stack(&upper);

        if bottom_start == top_start && bottom_stop == top_stop {
            // Single range: show the range's contents as all Added.
            merged.compare_and_mark(&FileTree::new());
        } else {
            merged.compare_and_mark(&lower);
        }

        merged
    }

    fn stack_range(&self, start: usize, stop: usize) -> FileTree {
        if start > stop {
            return FileTree::new();
        }

        let mut result = self.layers[start].tree.clone();
        for i in (start + 1)..=stop {
            result = result.stack(&self.layers[i].tree);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::super::filetree::{FileInfo, TarEntryType};
    use super::*;
    use crate::image::Layer;

    fn layer_with_file(index: usize, path: &str, size: u64, hash: u64) -> Layer {
        let mut tree = FileTree::new();
        tree.add_path(
            path,
            FileInfo {
                size,
                content_hash: hash,
                entry_type: TarEntryType::Regular,
                ..Default::default()
            },
        );
        Layer::new(index, format!("layer {}", index), size, tree)
    }

    fn empty_layer(index: usize) -> Layer {
        Layer::new(index, format!("empty {}", index), 0, FileTree::new())
    }

    #[test]
    fn test_natural_indexes() {
        let layers = vec![empty_layer(0), empty_layer(1), empty_layer(2)];
        let comparer = Comparer::new(layers);

        assert_eq!(
            comparer.natural_indexes(),
            vec![(0, 0, 0, 0), (0, 0, 1, 1), (0, 1, 2, 2)]
        );
    }

    #[test]
    fn test_aggregated_indexes() {
        let layers = vec![empty_layer(0), empty_layer(1), empty_layer(2)];
        let comparer = Comparer::new(layers);

        assert_eq!(
            comparer.aggregated_indexes(),
            vec![(0, 0, 0, 0), (0, 0, 0, 1), (0, 0, 0, 2)]
        );
    }

    #[test]
    fn test_get_tree_layer_zero_marked_added() {
        let layers = vec![layer_with_file(0, "bin/bash", 100, 1)];
        let mut comparer = Comparer::new(layers);

        let tree = comparer.get_tree(0, 0, 0, 0);
        assert_eq!(
            tree.get_node("bin/bash").unwrap().diff_type,
            super::super::filetree::DiffType::Added
        );
    }

    #[test]
    fn test_get_tree_natural_addition() {
        let layers = vec![
            layer_with_file(0, "bin/bash", 100, 1),
            layer_with_file(1, "bin/ls", 50, 2),
        ];
        let mut comparer = Comparer::new(layers);

        let tree = comparer.get_tree(0, 0, 1, 1);
        assert_eq!(
            tree.get_node("bin/ls").unwrap().diff_type,
            super::super::filetree::DiffType::Added
        );
        assert_eq!(
            tree.get_node("bin/bash").unwrap().diff_type,
            super::super::filetree::DiffType::Unmodified
        );
    }

    #[test]
    fn test_get_tree_natural_whiteout_removed() {
        use super::super::filetree::{DiffType, FileInfo, TarEntryType};

        let mut layer0 = FileTree::new();
        layer0.add_path(
            "etc/config",
            FileInfo {
                size: 100,
                content_hash: 1,
                entry_type: TarEntryType::Regular,
                ..Default::default()
            },
        );

        let mut layer1 = FileTree::new();
        layer1.add_path(
            "etc/.wh.config",
            FileInfo {
                size: 0,
                content_hash: 0,
                entry_type: TarEntryType::Regular,
                ..Default::default()
            },
        );

        let layers = vec![
            Layer::new(0, "add config", 100, layer0),
            Layer::new(1, "remove config", 0, layer1),
        ];
        let mut comparer = Comparer::new(layers);

        let tree = comparer.get_tree(0, 0, 1, 1);
        assert_eq!(
            tree.get_node("etc/config").unwrap().diff_type,
            DiffType::Removed
        );
    }

    #[test]
    fn test_caching_returns_same_tree() {
        let layers = vec![
            layer_with_file(0, "bin/bash", 100, 1),
            layer_with_file(1, "bin/ls", 50, 2),
        ];
        let mut comparer = Comparer::new(layers);

        let tree1 = comparer.get_tree(0, 0, 1, 1);
        let ptr1 = tree1 as *const FileTree;
        let tree2 = comparer.get_tree(0, 0, 1, 1);
        let ptr2 = tree2 as *const FileTree;

        assert_eq!(ptr1, ptr2);
    }

    #[test]
    fn test_build_cache_populates_all_natural_trees() {
        let layers = vec![
            layer_with_file(0, "bin/bash", 100, 1),
            layer_with_file(1, "bin/ls", 50, 2),
            layer_with_file(2, "bin/cat", 30, 3),
        ];
        let mut comparer = Comparer::new(layers);
        comparer.build_cache();

        let indexes = comparer.natural_indexes();
        for (bs, bstop, ts, tstop) in indexes {
            assert!(comparer.cache.contains_key(&(bs, bstop, ts, tstop)));
        }
    }

    #[test]
    fn test_fixture_natural_tree_renders() {
        use std::fs::File;
        let file = File::open("tests/fixtures/test-docker-image.tar").unwrap();
        let image = crate::analysis::archive::parse_docker_save_tar(file).unwrap();

        let mut comparer = Comparer::new(image.layers);
        comparer.build_cache();

        let indexes = comparer.natural_indexes();
        assert!(!indexes.is_empty());

        let (bs, bstop, ts, tstop) = indexes[0];
        let tree = comparer.get_tree(bs, bstop, ts, tstop);
        let lines = tree.render_string_tree(0, 10);
        assert!(!lines.is_empty());
    }
}
