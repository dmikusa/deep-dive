use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::tui::state::AppState;

pub struct ImageDetailsWidget;

impl ImageDetailsWidget {
    pub fn render(frame: &mut Frame, area: Rect, state: &mut AppState) {
        let title = match state.focus {
            crate::tui::state::FocusPane::ImageDetails => "Image Details [*]",
            _ => "Image Details",
        };
        let text = if let Some(report) = &state.report {
            let mut lines: Vec<Line> = Vec::new();
            lines.push(Line::from(Span::styled(
                format!("Image: {}", report.image_ref),
                Style::default().fg(Color::White),
            )));
            lines.push(Line::from(""));

            for result in &report.results {
                lines.push(Line::from(Span::styled(
                    format!("[{}] {}", result.analyzer_name(), result.summary()),
                    Style::default().fg(Color::Cyan),
                )));
                for section in result.details() {
                    lines.push(Line::from(Span::styled(
                        format!("{}:", section.title),
                        Style::default().fg(Color::Yellow),
                    )));
                    for item in section.items {
                        lines.push(Line::from(vec![
                            Span::raw(format!("  {}: ", item.label)),
                            Span::styled(item.value, Style::default().fg(Color::White)),
                        ]));
                    }
                    lines.push(Line::from(""));
                }
            }
            Text::from(lines)
        } else {
            Text::from("No analysis report available.")
        };

        // Clamp scroll so it doesn't go past the content.
        let inner_width = area.width.saturating_sub(2) as usize;
        let content_height = area.height.saturating_sub(2) as usize;
        let total_estimated = estimate_wrapped_lines(&text, inner_width).max(1);
        if total_estimated > content_height {
            let max_scroll = total_estimated - content_height;
            if state.image_details_scroll > max_scroll {
                state.image_details_scroll = max_scroll;
            }
        } else {
            state.image_details_scroll = 0;
        }

        let paragraph = Paragraph::new(text)
            .block(Block::bordered().title(title))
            .wrap(ratatui::widgets::Wrap { trim: false })
            .scroll((state.image_details_scroll as u16, 0));
        frame.render_widget(paragraph, area);
    }
}

/// Estimate the number of lines the text will produce when wrapped at max_width.
fn estimate_wrapped_lines(text: &Text, max_width: usize) -> usize {
    if max_width == 0 {
        return text.lines.len();
    }
    let mut total = 0usize;
    for line in &text.lines {
        let width: usize = line.spans.iter().map(|s| s.content.len()).sum();
        if width == 0 {
            total += 1;
        } else {
            total += width.div_ceil(max_width);
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::report::{AnalysisItem, AnalysisResult, AnalysisSection, Analyzer};
    use crate::image::Image;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    struct MockAnalyzer;

    impl Analyzer for MockAnalyzer {
        fn name(&self) -> &'static str {
            "Mock"
        }

        fn description(&self) -> &'static str {
            "Mock analyzer"
        }

        fn analyze(&self, _image: &Image) -> anyhow::Result<Box<dyn AnalysisResult>> {
            Ok(Box::new(MockResult))
        }
    }

    struct MockResult;

    impl AnalysisResult for MockResult {
        fn analyzer_name(&self) -> &'static str {
            "Mock"
        }

        fn summary(&self) -> String {
            "Summary line".to_string()
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
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
    }

    fn test_state() -> AppState {
        let image = Image {
            reference: "test-image".into(),
            layers: Vec::new(),
        };
        let mut state = AppState::new(image);
        let analyzers: Vec<Box<dyn Analyzer>> = vec![Box::new(MockAnalyzer)];
        state.report =
            Some(crate::analysis::report::Report::generate(&state.image, &analyzers).unwrap());
        state
    }

    #[test]
    fn test_image_details_renders() {
        let mut state = test_state();
        let backend = TestBackend::new(40, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| ImageDetailsWidget::render(f, f.area(), &mut state))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(content.contains("test-image"));
        assert!(content.contains("Summary line"));
        assert!(content.contains("Section"));
        assert!(content.contains("Key"));
        assert!(content.contains("Value"));
    }
}
