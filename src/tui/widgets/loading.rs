use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Clear, Gauge, Paragraph, Wrap};
use ratatui::Frame;

use crate::utils::format_size;

pub struct LoadingWidget;

impl LoadingWidget {
    pub fn render(
        frame: &mut Frame,
        area: Rect,
        status: &str,
        progress: Option<(u64, Option<u64>)>,
    ) {
        let popup_area = Self::centered_area(area);
        frame.render_widget(Clear, popup_area);

        let inner = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([Constraint::Min(2), Constraint::Length(2)])
            .split(popup_area);

        let status = Paragraph::new(Text::from(Line::from(Span::raw(status))))
            .style(Style::default().fg(Color::White))
            .wrap(Wrap { trim: true });
        frame.render_widget(status, inner[0]);

        if let Some((current, total)) = progress {
            let ratio = total.map(|t| (current as f64 / t as f64).clamp(0.0, 1.0));
            let label = match total {
                Some(t) => format!(
                    "{:.0}% ({} / {})",
                    ratio.unwrap_or(0.0) * 100.0,
                    format_size(current),
                    format_size(t)
                ),
                None => format!("{} received", format_size(current)),
            };
            let gauge = Gauge::default()
                .block(Block::default())
                .gauge_style(Style::default().fg(Color::Cyan))
                .ratio(ratio.unwrap_or(0.0))
                .label(label);
            frame.render_widget(gauge, inner[1]);
        }

        let block = Block::bordered().title("Loading");
        frame.render_widget(block, popup_area);
    }

    fn centered_area(area: Rect) -> Rect {
        let max_width = area.width.saturating_sub(2);
        let min_width = 20u16.min(max_width);
        let width = (area.width * 80 / 100).clamp(min_width, max_width);

        let max_height = area.height.saturating_sub(2);
        let min_height = 7u16.min(max_height);
        let height = (area.height * 30 / 100).clamp(min_height, max_height);

        let h_remainder = area.width.saturating_sub(width);
        let h_left = h_remainder / 2;
        let h_right = h_remainder - h_left;
        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(h_left),
                Constraint::Length(width),
                Constraint::Length(h_right),
            ])
            .split(area);

        let v_remainder = area.height.saturating_sub(height);
        let v_top = v_remainder / 2;
        let v_bottom = v_remainder - v_top;
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(v_top),
                Constraint::Length(height),
                Constraint::Length(v_bottom),
            ])
            .split(horizontal[1]);

        vertical[1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_loading_widget_renders() {
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                LoadingWidget::render(f, f.area(), "Pulling manifest...", Some((1024, Some(2048))))
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(content.contains("Loading"));
        assert!(content.contains("Pulling manifest..."));
        assert!(content.contains("1.0 KB"));
    }

    #[test]
    fn test_loading_widget_wraps_long_status() {
        let backend = TestBackend::new(40, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        let long_status =
            "Pulling a very long image reference that should wrap across multiple lines";
        terminal
            .draw(|f| LoadingWidget::render(f, f.area(), long_status, None))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(content.contains("Loading"));
        assert!(content.contains("Pulling"));
        assert!(content.contains("reference"));
    }
}
