#![allow(dead_code)]

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Text;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct StatusBarWidget;

impl StatusBarWidget {
    pub fn render(frame: &mut Frame, area: Rect) {
        let text = "↑↓/kj layer  Tab focus  Enter/Space collapse  q quit";
        let paragraph = Paragraph::new(Text::from(text)).style(Style::default().fg(Color::White));
        frame.render_widget(paragraph, area);
    }
}
