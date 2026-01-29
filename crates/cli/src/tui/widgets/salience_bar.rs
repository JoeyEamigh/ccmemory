use ratatui::{buffer::Buffer, layout::Rect, style::Style, widgets::Widget};

use crate::tui::theme::Theme;

/// A visual salience bar widget
/// Displays a horizontal bar showing the salience value (0.0 - 1.0)
/// Example: ████████░░ 0.82
pub struct SalienceBar {
  value: f32,
  width: u16,
}

impl SalienceBar {
  pub fn new(value: f32) -> Self {
    Self {
      value: value.clamp(0.0, 1.0),
      width: 10,
    }
  }

  pub fn width(mut self, width: u16) -> Self {
    self.width = width;
    self
  }
}

impl Widget for SalienceBar {
  fn render(self, area: Rect, buf: &mut Buffer) {
    if area.width == 0 || area.height == 0 {
      return;
    }

    let color = Theme::salience_color(self.value);
    let bar_width = self.width.min(area.width.saturating_sub(5)); // Leave room for " 0.XX"

    let filled = ((self.value * bar_width as f32).round() as u16).min(bar_width);
    let empty = bar_width.saturating_sub(filled);

    // Render filled portion
    let filled_str: String = "█".repeat(filled as usize);
    buf.set_string(area.x, area.y, &filled_str, Style::default().fg(color));

    // Render empty portion
    let empty_str: String = "░".repeat(empty as usize);
    buf.set_string(area.x + filled, area.y, &empty_str, Style::default().fg(Theme::MUTED));

    // Render value
    if area.width > bar_width {
      let value_str = format!(" {:.2}", self.value);
      buf.set_string(
        area.x + bar_width,
        area.y,
        &value_str,
        Style::default().fg(Theme::SUBTEXT),
      );
    }
  }
}
