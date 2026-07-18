use anyhow::Result;

use crate::image::Image;

#[allow(dead_code)]
/// A single analysis that can be run against an image.
pub trait Analyzer: Send + Sync {
    /// Human-readable name (e.g. "Efficiency", "Layer Stats").
    fn name(&self) -> &'static str;

    /// Brief description of what this analyzer does.
    fn description(&self) -> &'static str;

    /// Run the analysis against the image.
    fn analyze(&self, image: &Image) -> Result<Box<dyn AnalysisResult>>;
}

#[allow(dead_code)]
/// The result of running a single analyzer.
pub trait AnalysisResult: Send + Sync {
    /// The analyzer name this result came from.
    fn analyzer_name(&self) -> &'static str;

    /// One-line summary for display in the TUI status bar.
    fn summary(&self) -> String;

    /// Detailed results — sections of labeled key-value pairs
    /// or structured data for the TUI details pane.
    fn details(&self) -> Vec<AnalysisSection>;

    /// Allow downcasting concrete result types.
    fn as_any(&self) -> &dyn std::any::Any;
}

/// A labeled section within analysis results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisSection {
    pub title: String,
    pub items: Vec<AnalysisItem>,
}

/// A single labeled value inside an analysis section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisItem {
    pub label: String,
    pub value: String,
}

/// A report collecting results from all registered analyzers.
pub struct Report {
    pub image_ref: String,
    pub results: Vec<Box<dyn AnalysisResult>>,
}

impl std::fmt::Debug for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Report")
            .field("image_ref", &self.image_ref)
            .field("results", &self.results.len())
            .finish()
    }
}

impl Report {
    /// Run all given analyzers against the image.
    pub fn generate(image: &Image, analyzers: &[Box<dyn Analyzer>]) -> Result<Self> {
        let results = analyzers
            .iter()
            .map(|a| a.analyze(image))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            image_ref: image.reference.clone(),
            results,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockAnalyzer;

    impl Analyzer for MockAnalyzer {
        fn name(&self) -> &'static str {
            "Mock"
        }

        fn description(&self) -> &'static str {
            "A test analyzer"
        }

        fn analyze(&self, _image: &Image) -> Result<Box<dyn AnalysisResult>> {
            Ok(Box::new(MockResult))
        }
    }

    struct MockResult;

    impl AnalysisResult for MockResult {
        fn analyzer_name(&self) -> &'static str {
            "Mock"
        }

        fn summary(&self) -> String {
            "mock summary".to_string()
        }

        fn details(&self) -> Vec<AnalysisSection> {
            vec![AnalysisSection {
                title: "Section".to_string(),
                items: vec![AnalysisItem {
                    label: "Key".to_string(),
                    value: "Value".to_string(),
                }],
            }]
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    fn empty_image() -> Image {
        Image {
            reference: "test".to_string(),
            layers: Vec::new(),
        }
    }

    #[test]
    fn test_generate_with_empty_analyzers() {
        let image = empty_image();
        let report = Report::generate(&image, &[]).unwrap();
        assert!(report.results.is_empty());
        assert_eq!(report.image_ref, "test");
    }

    #[test]
    fn test_generate_with_mock_analyzer() {
        let image = empty_image();
        let analyzers: Vec<Box<dyn Analyzer>> = vec![Box::new(MockAnalyzer)];
        let report = Report::generate(&image, &analyzers).unwrap();
        assert_eq!(report.results.len(), 1);
        assert_eq!(report.results[0].analyzer_name(), "Mock");
        assert_eq!(report.results[0].summary(), "mock summary");
        let details = report.results[0].details();
        assert_eq!(details.len(), 1);
        assert_eq!(details[0].title, "Section");
        assert_eq!(details[0].items[0].label, "Key");
        assert_eq!(details[0].items[0].value, "Value");
    }
}
