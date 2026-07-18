use std::collections::HashMap;

use anyhow::Result;

use crate::analysis::filetree::FileTree;
use crate::analysis::report::{AnalysisItem, AnalysisResult, AnalysisSection, Analyzer};
use crate::image::Image;

/// Data collected for a single path across all layers.
#[derive(Debug, Clone)]
struct EfficiencyData {
    path: String,
    cumulative_size: u64,
    min_size: u64,
    occurrences: usize,
}

/// A path that contributes to wasted space.
#[derive(Debug, Clone)]
pub struct Inefficiency {
    pub path: String,
    pub cumulative_size: u64,
    pub wasted_bytes: u64,
}

/// Result of the efficiency analyzer.
#[derive(Debug, Clone)]
pub struct EfficiencyResult {
    pub score: f64,
    pub total_bytes: u64,
    pub user_bytes: u64,
    pub wasted_bytes: u64,
    pub wasted_user_percent: f64,
    pub inefficiencies: Vec<Inefficiency>,
}

impl EfficiencyResult {
    pub fn top_inefficiencies(&self, n: usize) -> Vec<&Inefficiency> {
        self.inefficiencies.iter().take(n).collect()
    }
}

impl AnalysisResult for EfficiencyResult {
    fn analyzer_name(&self) -> &'static str {
        "Efficiency"
    }

    fn summary(&self) -> String {
        format!(
            "Efficiency: {:.2}%  Wasted: {}",
            self.score * 100.0,
            crate::utils::format_size(self.wasted_bytes)
        )
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn details(&self) -> Vec<AnalysisSection> {
        let mut sections = vec![AnalysisSection {
            title: "Summary".to_string(),
            items: vec![
                AnalysisItem {
                    label: "Efficiency score".to_string(),
                    value: format!("{:.2}%", self.score * 100.0),
                },
                AnalysisItem {
                    label: "Total bytes".to_string(),
                    value: crate::utils::format_size(self.total_bytes),
                },
                AnalysisItem {
                    label: "User bytes".to_string(),
                    value: crate::utils::format_size(self.user_bytes),
                },
                AnalysisItem {
                    label: "Wasted bytes".to_string(),
                    value: crate::utils::format_size(self.wasted_bytes),
                },
                AnalysisItem {
                    label: "Wasted user percent".to_string(),
                    value: format!("{:.2}%", self.wasted_user_percent * 100.0),
                },
            ],
        }];

        if !self.inefficiencies.is_empty() {
            let items = self
                .top_inefficiencies(10)
                .iter()
                .map(|i| AnalysisItem {
                    label: i.path.clone(),
                    value: format!(
                        "{} wasted ({} cumulative)",
                        crate::utils::format_size(i.wasted_bytes),
                        crate::utils::format_size(i.cumulative_size)
                    ),
                })
                .collect();
            sections.push(AnalysisSection {
                title: "Top inefficient files".to_string(),
                items,
            });
        }

        sections
    }
}

/// Measures space wasted by file duplication and deletions across layers.
pub struct EfficiencyAnalyzer;

impl Analyzer for EfficiencyAnalyzer {
    fn name(&self) -> &'static str {
        "Efficiency"
    }

    fn description(&self) -> &'static str {
        "Measures space wasted by file duplication across layers"
    }

    fn analyze(&self, image: &Image) -> Result<Box<dyn AnalysisResult>> {
        let trees: Vec<&FileTree> = image.layers.iter().map(|l| &l.tree).collect();
        let mut data_map: HashMap<String, EfficiencyData> = HashMap::new();

        for (layer_idx, tree) in trees.iter().enumerate() {
            for path in tree.leaf_paths() {
                // Skip opaque whiteout markers. They are needed for stacking
                // but, like dive, are not counted toward efficiency waste.
                if path.ends_with(".wh..wh..opq") {
                    continue;
                }

                // Skip directory nodes; only files and whiteouts contribute
                // to efficiency waste.
                if let Some(node) = tree.get_node(&path) {
                    if node.info.entry_type == crate::analysis::filetree::TarEntryType::Directory {
                        continue;
                    }
                }

                let size = if FileTree::is_whiteout_path(&path) {
                    let target = FileTree::whiteout_target_path(&path).unwrap_or_default();
                    let previous = Self::stack_trees(&trees[..layer_idx]);
                    if let Some(node) = previous.get_node(&target) {
                        if node.info.entry_type
                            == crate::analysis::filetree::TarEntryType::Directory
                        {
                            previous.subtree_size(&target)
                        } else {
                            // Match dive: file whiteouts contribute 0 to
                            // cumulative size; the original file is already
                            // counted by its leaf occurrence.
                            0
                        }
                    } else {
                        0
                    }
                } else {
                    tree.subtree_size(&path)
                };

                let key = FileTree::whiteout_target_path(&path).unwrap_or_else(|| path.clone());
                if key.is_empty() {
                    continue;
                }

                let entry = data_map.entry(key).or_insert_with(|| EfficiencyData {
                    path: path.clone(),
                    cumulative_size: 0,
                    min_size: u64::MAX,
                    occurrences: 0,
                });
                entry.cumulative_size += size;
                entry.min_size = entry.min_size.min(size);
                entry.occurrences += 1;
            }
        }

        let mut total_bytes = 0u64;
        let mut min_total = 0u64;
        let mut inefficiencies = Vec::new();

        for data in data_map.values() {
            total_bytes += data.cumulative_size;
            min_total += data.min_size;
            if data.occurrences >= 2 {
                inefficiencies.push(Inefficiency {
                    path: data.path.clone(),
                    cumulative_size: data.cumulative_size,
                    wasted_bytes: data.cumulative_size,
                });
            }
        }

        inefficiencies.sort_by_key(|b| std::cmp::Reverse(b.wasted_bytes));

        let score = if total_bytes == 0 {
            1.0
        } else {
            min_total as f64 / total_bytes as f64
        };

        let user_bytes = Self::user_bytes(image);
        let wasted_bytes = inefficiencies.iter().map(|i| i.cumulative_size).sum();
        let wasted_user_percent = if user_bytes == 0 {
            0.0
        } else {
            wasted_bytes as f64 / user_bytes as f64
        };

        Ok(Box::new(EfficiencyResult {
            score,
            total_bytes,
            user_bytes,
            wasted_bytes,
            wasted_user_percent,
            inefficiencies,
        }))
    }
}

impl EfficiencyAnalyzer {
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

    fn user_bytes(image: &Image) -> u64 {
        // User bytes = sum of layer sizes excluding the base layer, matching
        // dive's convention.
        if image.layers.len() <= 1 {
            return 0;
        }
        image.layers[1..].iter().map(|l| l.size).sum()
    }
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

    fn whiteout_info() -> FileInfo {
        FileInfo {
            entry_type: TarEntryType::Regular,
            ..Default::default()
        }
    }

    #[test]
    fn test_efficiency_perfect_score() {
        let mut tree = FileTree::new();
        tree.add_path("a", file_info(100));

        let image = Image {
            reference: "test".into(),
            layers: vec![Layer {
                index: 0,
                command: "add a".into(),
                size: 100,
                tree,
            }],
        };

        let analyzer = EfficiencyAnalyzer;
        let result = analyzer.analyze(&image).unwrap();
        let eff = result.as_any().downcast_ref::<EfficiencyResult>().unwrap();
        assert_eq!(eff.score, 1.0);
        assert_eq!(eff.wasted_bytes, 0);
    }

    #[test]
    fn test_efficiency_duplicate_file() {
        let mut layer0 = FileTree::new();
        layer0.add_path("a", file_info(100));

        let mut layer1 = FileTree::new();
        layer1.add_path("a", file_info(100));

        let image = Image {
            reference: "test".into(),
            layers: vec![
                Layer {
                    index: 0,
                    command: "add a".into(),
                    size: 100,
                    tree: layer0,
                },
                Layer {
                    index: 1,
                    command: "add a".into(),
                    size: 100,
                    tree: layer1,
                },
            ],
        };

        let analyzer = EfficiencyAnalyzer;
        let result = analyzer.analyze(&image).unwrap();
        let eff = result.as_any().downcast_ref::<EfficiencyResult>().unwrap();
        assert!(eff.score < 1.0);
        assert_eq!(eff.wasted_bytes, 200);
        assert_eq!(eff.inefficiencies.len(), 1);
    }

    #[test]
    fn test_efficiency_whiteout() {
        let mut layer0 = FileTree::new();
        layer0.add_path("a", file_info(100));

        let mut layer1 = FileTree::new();
        layer1.add_path(".wh.a", whiteout_info());

        let image = Image {
            reference: "test".into(),
            layers: vec![
                Layer {
                    index: 0,
                    command: "add a".into(),
                    size: 100,
                    tree: layer0,
                },
                Layer {
                    index: 1,
                    command: "remove a".into(),
                    size: 0,
                    tree: layer1,
                },
            ],
        };

        let analyzer = EfficiencyAnalyzer;
        let result = analyzer.analyze(&image).unwrap();
        let eff = result.as_any().downcast_ref::<EfficiencyResult>().unwrap();
        assert!(eff.score < 1.0);
        assert_eq!(eff.wasted_bytes, 100);
    }

    #[test]
    fn test_fixture_efficiency() {
        use std::fs::File;
        let file = File::open("tests/fixtures/test-docker-image.tar").unwrap();
        let image = crate::analysis::archive::parse_docker_save_tar(file).unwrap();

        let analyzer = EfficiencyAnalyzer;
        let result = analyzer.analyze(&image).unwrap();
        let eff = result.as_any().downcast_ref::<EfficiencyResult>().unwrap();

        // Known values from dive's test fixtures.
        assert!((eff.score - 0.9844).abs() < 0.01);
        assert!(eff.wasted_bytes >= 30_000 && eff.wasted_bytes <= 35_000);
    }
}
