use std::collections::HashSet;

use anyhow::Result;

use crate::analysis::filetree::FileTree;
use crate::analysis::report::{AnalysisItem, AnalysisResult, AnalysisSection, Analyzer};
use crate::image::Image;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct LayerStats {
    pub index: usize,
    pub command: String,
    pub raw_size: u64,
    pub unique_size: u64,
    pub wasted_size: u64,
    pub compressed_size: Option<u64>,
    pub compression_ratio: Option<f64>,
    pub file_count: usize,
    pub dir_count: usize,
    pub symlink_count: usize,
    pub percent_of_image: f64,
    pub cumulative_percent: f64,
    pub whiteout_count: usize,
    pub whiteout_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct LayerStatsResult {
    pub layers: Vec<LayerStats>,
}

impl AnalysisResult for LayerStatsResult {
    fn analyzer_name(&self) -> &'static str {
        "Layer Stats"
    }

    fn summary(&self) -> String {
        let total_unique: u64 = self.layers.iter().map(|l| l.unique_size).sum();
        format!(
            "Layer Stats: {} layers, {} unique",
            self.layers.len(),
            crate::utils::format_size(total_unique)
        )
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn details(&self) -> Vec<AnalysisSection> {
        let mut items = Vec::new();
        for stat in &self.layers {
            items.push(AnalysisItem {
                label: format!("#{}: {}", stat.index, stat.command),
                value: format!(
                    "raw={} unique={} wasted={} files={} dirs={} wh={} {:.1}% of image ({:.1}% cumulative)",
                    crate::utils::format_size(stat.raw_size),
                    crate::utils::format_size(stat.unique_size),
                    crate::utils::format_size(stat.wasted_size),
                    stat.file_count,
                    stat.dir_count,
                    stat.whiteout_count,
                    stat.percent_of_image * 100.0,
                    stat.cumulative_percent * 100.0,
                ),
            });
        }
        vec![AnalysisSection {
            title: "Per-layer metrics".to_string(),
            items,
        }]
    }
}

pub struct LayerStatsAnalyzer;

impl Analyzer for LayerStatsAnalyzer {
    fn name(&self) -> &'static str {
        "Layer Stats"
    }

    fn description(&self) -> &'static str {
        "Per-layer metrics: size, compression, file counts"
    }

    fn analyze(&self, image: &Image) -> Result<Box<dyn AnalysisResult>> {
        let trees: Vec<&FileTree> = image.layers.iter().map(|l| &l.tree).collect();
        let mut seen_paths: HashSet<String> = HashSet::new();
        let mut cumulative_unique: u64 = 0;
        let total_bytes: u64 = image.layers.iter().map(|l| l.size).sum();
        let mut stats = Vec::new();

        for (layer_idx, layer) in image.layers.iter().enumerate() {
            let tree = &layer.tree;
            let mut file_count = 0usize;
            let mut dir_count = 0usize;
            let mut symlink_count = 0usize;
            let mut whiteout_count = 0usize;
            let mut whiteout_bytes: u64 = 0;
            let mut unique_size: u64 = 0;
            let mut wasted_size: u64 = 0;

            for path in tree.leaf_paths() {
                let node = match tree.get_node(&path) {
                    Some(n) => n,
                    None => continue,
                };

                match node.info.entry_type {
                    crate::analysis::filetree::TarEntryType::Directory => {
                        dir_count += 1;
                    }
                    crate::analysis::filetree::TarEntryType::Symlink
                    | crate::analysis::filetree::TarEntryType::Hardlink => {
                        symlink_count += 1;
                    }
                    _ => {}
                }

                if node.info.entry_type == crate::analysis::filetree::TarEntryType::Directory {
                    continue;
                }

                if node.info.entry_type == crate::analysis::filetree::TarEntryType::Regular {
                    file_count += 1;
                }

                if FileTree::is_whiteout_path(&path) {
                    whiteout_count += 1;
                    let target = FileTree::whiteout_target_path(&path).unwrap_or_default();
                    if !target.is_empty() {
                        let previous = stack_trees(&trees[..layer_idx]);
                        if let Some(n) = previous.get_node(&target) {
                            if n.info.entry_type
                                == crate::analysis::filetree::TarEntryType::Directory
                            {
                                whiteout_bytes += previous.subtree_size(&target);
                            } else {
                                whiteout_bytes += n.info.size;
                            }
                        }
                    }
                    continue;
                }

                let key = FileTree::whiteout_target_path(&path).unwrap_or_else(|| path.clone());
                if key.is_empty() {
                    continue;
                }

                let size = tree.subtree_size(&path);

                if seen_paths.contains(&key) {
                    wasted_size += size;
                } else {
                    unique_size += size;
                    seen_paths.insert(key);
                }
            }

            cumulative_unique += unique_size;
            let percent_of_image = if total_bytes > 0 {
                unique_size as f64 / total_bytes as f64
            } else {
                0.0
            };
            let cumulative_pct = if total_bytes > 0 {
                cumulative_unique as f64 / total_bytes as f64
            } else {
                0.0
            };

            stats.push(LayerStats {
                index: layer.index,
                command: layer.command.clone(),
                raw_size: layer.size,
                unique_size,
                wasted_size,
                compressed_size: None,
                compression_ratio: None,
                file_count,
                dir_count,
                symlink_count,
                percent_of_image,
                cumulative_percent: cumulative_pct,
                whiteout_count,
                whiteout_bytes,
            });
        }

        Ok(Box::new(LayerStatsResult { layers: stats }))
    }
}

fn stack_trees(trees: &[&FileTree]) -> FileTree {
    if trees.is_empty() {
        return FileTree::new();
    }
    let mut result = trees[0].clone();
    for tree in &trees[1..] {
        result = result.stack(tree);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::filetree::{FileInfo, TarEntryType};
    use crate::image::{Image, Layer};

    fn file_info(size: u64) -> FileInfo {
        FileInfo {
            size,
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

    fn symlink_info() -> FileInfo {
        FileInfo {
            entry_type: TarEntryType::Symlink,
            ..Default::default()
        }
    }

    fn whiteout_info() -> FileInfo {
        FileInfo {
            entry_type: TarEntryType::Regular,
            ..Default::default()
        }
    }

    #[test]
    fn test_single_layer() {
        let mut tree = FileTree::new();
        tree.add_path("a", file_info(100));
        tree.add_path("b", dir_info());

        let image = Image {
            reference: "test".into(),
            layers: vec![Layer::new(0, "add files", 100, tree)],
        };

        let analyzer = LayerStatsAnalyzer;
        let result = analyzer.analyze(&image).unwrap();
        let stats = result.as_any().downcast_ref::<LayerStatsResult>().unwrap();
        assert_eq!(stats.layers.len(), 1);
        assert_eq!(stats.layers[0].file_count, 1);
        assert_eq!(stats.layers[0].dir_count, 1);
        assert_eq!(stats.layers[0].whiteout_count, 0);
        assert_eq!(stats.layers[0].unique_size, 100);
        assert_eq!(stats.layers[0].wasted_size, 0);
    }

    #[test]
    fn test_duplicate_across_layers() {
        let mut layer0 = FileTree::new();
        layer0.add_path("a", file_info(100));
        let mut layer1 = FileTree::new();
        layer1.add_path("a", file_info(100));

        let image = Image {
            reference: "test".into(),
            layers: vec![
                Layer::new(0, "add a", 100, layer0),
                Layer::new(1, "add a again", 100, layer1),
            ],
        };

        let analyzer = LayerStatsAnalyzer;
        let result = analyzer.analyze(&image).unwrap();
        let stats = result.as_any().downcast_ref::<LayerStatsResult>().unwrap();
        assert_eq!(stats.layers.len(), 2);
        assert_eq!(stats.layers[0].unique_size, 100);
        assert_eq!(stats.layers[0].wasted_size, 0);
        assert_eq!(stats.layers[1].unique_size, 0);
        assert_eq!(stats.layers[1].wasted_size, 100);
    }

    #[test]
    fn test_whiteout_count_and_bytes() {
        let mut layer0 = FileTree::new();
        layer0.add_path("a", file_info(100));
        let mut layer1 = FileTree::new();
        layer1.add_path(".wh.a", whiteout_info());

        let image = Image {
            reference: "test".into(),
            layers: vec![
                Layer::new(0, "add a", 100, layer0),
                Layer::new(1, "remove a", 0, layer1),
            ],
        };

        let analyzer = LayerStatsAnalyzer;
        let result = analyzer.analyze(&image).unwrap();
        let stats = result.as_any().downcast_ref::<LayerStatsResult>().unwrap();
        assert_eq!(stats.layers[1].whiteout_count, 1);
        assert_eq!(stats.layers[1].whiteout_bytes, 100);
    }

    #[test]
    fn test_fixture_layer_stats() {
        use std::fs::File;
        let file = File::open("tests/fixtures/test-docker-image.tar").unwrap();
        let image = crate::analysis::archive::parse_docker_save_tar(file).unwrap();

        let analyzer = LayerStatsAnalyzer;
        let result = analyzer.analyze(&image).unwrap();
        let stats = result.as_any().downcast_ref::<LayerStatsResult>().unwrap();

        assert!(!stats.layers.is_empty());
        assert!(stats.layers.len() > 1);
    }
}
