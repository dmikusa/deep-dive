use std::collections::HashMap;

use anyhow::Result;

use crate::analysis::filetree::FileTree;
use crate::analysis::report::{AnalysisItem, AnalysisResult, AnalysisSection, Analyzer};
use crate::image::Image;

#[derive(Debug, Clone)]
pub struct ShadedOccurrence {
    pub layer_index: usize,
    pub size: u64,
    #[allow(dead_code)]
    pub content_hash: u64,
    pub is_shaded: bool,
}

#[derive(Debug, Clone)]
pub struct ShadedFile {
    pub path: String,
    pub occurrences: Vec<ShadedOccurrence>,
    pub total_wasted: u64,
    pub content_identical: bool,
    pub deleted_by_whiteout: bool,
}

#[derive(Debug, Clone)]
pub struct ShadedFileResult {
    pub shaded_files: Vec<ShadedFile>,
    pub total_shaded_bytes: u64,
    pub shaded_file_count: usize,
}

impl AnalysisResult for ShadedFileResult {
    fn analyzer_name(&self) -> &'static str {
        "Shaded Files"
    }

    fn summary(&self) -> String {
        format!(
            "Shaded Files: {} files, {} wasted",
            self.shaded_file_count,
            crate::utils::format_size(self.total_shaded_bytes)
        )
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn details(&self) -> Vec<AnalysisSection> {
        if self.shaded_files.is_empty() {
            return vec![AnalysisSection {
                title: "Shaded Files".to_string(),
                items: vec![AnalysisItem {
                    label: "Status".to_string(),
                    value: "No shaded files found".to_string(),
                }],
            }];
        }

        let mut sections = vec![AnalysisSection {
            title: "Summary".to_string(),
            items: vec![
                AnalysisItem {
                    label: "Total shaded files".to_string(),
                    value: self.shaded_file_count.to_string(),
                },
                AnalysisItem {
                    label: "Total wasted bytes".to_string(),
                    value: crate::utils::format_size(self.total_shaded_bytes),
                },
            ],
        }];

        let items: Vec<AnalysisItem> = self
            .shaded_files
            .iter()
            .map(|sf| {
                let mut lines = Vec::new();
                for occ in &sf.occurrences {
                    if occ.is_shaded {
                        lines.push(format!(
                            "  Layer {}: {} (shaded)",
                            occ.layer_index,
                            crate::utils::format_size(occ.size)
                        ));
                    } else {
                        lines.push(format!(
                            "  Layer {}: {} ← visible",
                            occ.layer_index,
                            crate::utils::format_size(occ.size)
                        ));
                    }
                }
                if sf.content_identical {
                    lines.push("  (content identical across layers)".to_string());
                }
                if sf.deleted_by_whiteout {
                    lines.push("  (deleted by whiteout in final layer)".to_string());
                }
                AnalysisItem {
                    label: sf.path.clone(),
                    value: lines.join("\n"),
                }
            })
            .collect();

        sections.push(AnalysisSection {
            title: "Shaded files by waste (descending)".to_string(),
            items,
        });

        sections
    }
}

pub struct ShadedFileAnalyzer;

impl Analyzer for ShadedFileAnalyzer {
    fn name(&self) -> &'static str {
        "Shaded Files"
    }

    fn description(&self) -> &'static str {
        "Files whose content is hidden by a newer version in a higher layer"
    }

    fn analyze(&self, image: &Image) -> Result<Box<dyn AnalysisResult>> {
        #[derive(Debug, Clone)]
        struct FileOccurrence {
            layer_index: usize,
            size: u64,
            content_hash: u64,
        }

        #[derive(Debug, Clone)]
        struct WhiteoutOccurrence {
            layer_index: usize,
        }

        let mut file_occurrences: HashMap<String, Vec<FileOccurrence>> = HashMap::new();
        let mut whiteout_layers: HashMap<String, Vec<WhiteoutOccurrence>> = HashMap::new();

        for layer in &image.layers {
            for path in layer.tree.leaf_paths() {
                if FileTree::is_whiteout_path(&path) {
                    let target = FileTree::whiteout_target_path(&path).unwrap_or_default();
                    if target.is_empty() {
                        continue;
                    }
                    whiteout_layers
                        .entry(target)
                        .or_default()
                        .push(WhiteoutOccurrence {
                            layer_index: layer.index,
                        });
                    continue;
                }

                let node = match layer.tree.get_node(&path) {
                    Some(n) => n,
                    None => continue,
                };

                if node.info.entry_type == crate::analysis::filetree::TarEntryType::Directory {
                    continue;
                }

                let size = layer.tree.subtree_size(&path);
                file_occurrences.entry(path.clone()).or_default().push(
                    FileOccurrence {
                        layer_index: layer.index,
                        size,
                        content_hash: node.info.content_hash,
                    },
                );
            }
        }

        let mut shaded_files: Vec<ShadedFile> = Vec::new();

        for (path, mut files) in file_occurrences {
            if files.len() < 2 {
                continue;
            }

            files.sort_by_key(|o| o.layer_index);

            let has_whiteout_above = whiteout_layers
                .get(&path)
                .map(|wos| wos.iter().any(|wo| wo.layer_index > files.last().unwrap().layer_index))
                .unwrap_or(false);

            let deleted_by_whiteout = has_whiteout_above;

            let total_wasted: u64 = files[..files.len() - 1]
                .iter()
                .map(|o| o.size)
                .sum();

            let visible = files.last().unwrap();
            let content_identical = files.iter().all(|o| o.content_hash == visible.content_hash);

            let occurrences: Vec<ShadedOccurrence> = files
                .iter()
                .enumerate()
                .map(|(i, o)| ShadedOccurrence {
                    layer_index: o.layer_index,
                    size: o.size,
                    content_hash: o.content_hash,
                    is_shaded: i < files.len() - 1,
                })
                .collect();

            shaded_files.push(ShadedFile {
                path,
                occurrences,
                total_wasted,
                content_identical,
                deleted_by_whiteout,
            });
        }

        shaded_files.sort_by_key(|sf| std::cmp::Reverse(sf.total_wasted));

        let total_shaded_bytes: u64 = shaded_files.iter().map(|sf| sf.total_wasted).sum();
        let shaded_file_count = shaded_files.len();

        Ok(Box::new(ShadedFileResult {
            shaded_files,
            total_shaded_bytes,
            shaded_file_count,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::filetree::{FileInfo, TarEntryType};
    use crate::image::{Image, Layer};

    fn file_info(size: u64, content_hash: u64) -> FileInfo {
        FileInfo {
            size,
            content_hash,
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
    fn test_no_shaded_files_single_layer() {
        let mut tree = FileTree::new();
        tree.add_path("a", file_info(100, 1));

        let image = Image {
            reference: "test".into(),
            layers: vec![Layer::new(0, "add a", 100, tree)],
        };

        let analyzer = ShadedFileAnalyzer;
        let result = analyzer.analyze(&image).unwrap();
        let shaded = result
            .as_any()
            .downcast_ref::<ShadedFileResult>()
            .unwrap();
        assert_eq!(shaded.shaded_file_count, 0);
        assert_eq!(shaded.total_shaded_bytes, 0);
    }

    #[test]
    fn test_simple_shading() {
        let mut layer0 = FileTree::new();
        layer0.add_path("a", file_info(100, 1));
        let mut layer1 = FileTree::new();
        layer1.add_path("a", file_info(200, 2));

        let image = Image {
            reference: "test".into(),
            layers: vec![
                Layer::new(0, "add a", 100, layer0),
                Layer::new(1, "overwrite a", 200, layer1),
            ],
        };

        let analyzer = ShadedFileAnalyzer;
        let result = analyzer.analyze(&image).unwrap();
        let shaded = result
            .as_any()
            .downcast_ref::<ShadedFileResult>()
            .unwrap();
        assert_eq!(shaded.shaded_file_count, 1);
        assert_eq!(shaded.total_shaded_bytes, 100);

        let sf = &shaded.shaded_files[0];
        assert_eq!(sf.path, "a");
        assert_eq!(sf.occurrences.len(), 2);
        assert!(sf.occurrences[0].is_shaded);
        assert!(!sf.occurrences[1].is_shaded);
        assert!(!sf.content_identical);
    }

    #[test]
    fn test_identical_content_across_layers() {
        let mut layer0 = FileTree::new();
        layer0.add_path("a", file_info(100, 1));
        let mut layer1 = FileTree::new();
        layer1.add_path("a", file_info(100, 1));

        let image = Image {
            reference: "test".into(),
            layers: vec![
                Layer::new(0, "add a", 100, layer0),
                Layer::new(1, "copy a", 100, layer1),
            ],
        };

        let analyzer = ShadedFileAnalyzer;
        let result = analyzer.analyze(&image).unwrap();
        let shaded = result
            .as_any()
            .downcast_ref::<ShadedFileResult>()
            .unwrap();
        assert_eq!(shaded.shaded_file_count, 1);
        assert_eq!(shaded.total_shaded_bytes, 100);
        assert!(shaded.shaded_files[0].content_identical);
    }

    #[test]
    fn test_three_layers_with_shading() {
        let mut layer0 = FileTree::new();
        layer0.add_path("a", file_info(100, 1));
        let mut layer1 = FileTree::new();
        layer1.add_path("a", file_info(150, 2));
        let mut layer2 = FileTree::new();
        layer2.add_path("a", file_info(200, 3));

        let image = Image {
            reference: "test".into(),
            layers: vec![
                Layer::new(0, "add a", 100, layer0),
                Layer::new(1, "update a", 150, layer1),
                Layer::new(2, "final a", 200, layer2),
            ],
        };

        let analyzer = ShadedFileAnalyzer;
        let result = analyzer.analyze(&image).unwrap();
        let shaded = result
            .as_any()
            .downcast_ref::<ShadedFileResult>()
            .unwrap();
        assert_eq!(shaded.shaded_file_count, 1);
        assert_eq!(shaded.total_shaded_bytes, 250);

        let sf = &shaded.shaded_files[0];
        assert_eq!(sf.occurrences.len(), 3);
        assert!(sf.occurrences[0].is_shaded);
        assert!(sf.occurrences[1].is_shaded);
        assert!(!sf.occurrences[2].is_shaded);
    }

    #[test]
    fn test_shaded_then_whited_out() {
        let mut layer0 = FileTree::new();
        layer0.add_path("a", file_info(100, 1));
        let mut layer1 = FileTree::new();
        layer1.add_path("a", file_info(200, 2));
        let mut layer2 = FileTree::new();
        layer2.add_path(".wh.a", whiteout_info());

        let image = Image {
            reference: "test".into(),
            layers: vec![
                Layer::new(0, "add a", 100, layer0),
                Layer::new(1, "update a", 200, layer1),
                Layer::new(2, "remove a", 0, layer2),
            ],
        };

        let analyzer = ShadedFileAnalyzer;
        let result = analyzer.analyze(&image).unwrap();
        let shaded = result
            .as_any()
            .downcast_ref::<ShadedFileResult>()
            .unwrap();
        assert_eq!(shaded.shaded_file_count, 1);
        assert_eq!(shaded.total_shaded_bytes, 100);

        let sf = &shaded.shaded_files[0];
        assert_eq!(sf.path, "a");
        assert!(sf.deleted_by_whiteout);
        assert!(sf.occurrences[0].is_shaded);
        assert!(!sf.occurrences[1].is_shaded);
    }

    #[test]
    fn test_multiple_files_with_different_shading() {
        let mut layer0 = FileTree::new();
        layer0.add_path("shared", file_info(100, 1));
        layer0.add_path("unique", file_info(50, 2));

        let mut layer1 = FileTree::new();
        layer1.add_path("shared", file_info(200, 3));

        let image = Image {
            reference: "test".into(),
            layers: vec![
                Layer::new(0, "layer 0", 150, layer0),
                Layer::new(1, "layer 1", 200, layer1),
            ],
        };

        let analyzer = ShadedFileAnalyzer;
        let result = analyzer.analyze(&image).unwrap();
        let shaded = result
            .as_any()
            .downcast_ref::<ShadedFileResult>()
            .unwrap();
        assert_eq!(shaded.shaded_file_count, 1);
        assert_eq!(shaded.shaded_files[0].path, "shared");
    }

    #[test]
    fn test_fixture_shaded_files() {
        use std::fs::File;
        let file = File::open("tests/fixtures/test-docker-image.tar").unwrap();
        let image = crate::analysis::archive::parse_docker_save_tar(file).unwrap();

        let analyzer = ShadedFileAnalyzer;
        let result = analyzer.analyze(&image).unwrap();
        let shaded = result
            .as_any()
            .downcast_ref::<ShadedFileResult>()
            .unwrap();

        assert!(!shaded.shaded_files.is_empty());
        assert!(shaded.total_shaded_bytes > 0);

        assert!(
            shaded.shaded_file_count > 0,
            "expected at least one shaded file, got: {:?}",
            shaded
                .shaded_files
                .iter()
                .map(|sf| sf.path.as_str())
                .collect::<Vec<_>>()
        );
    }
}
