use ccengram::ipc::docs::DocSearchItem;
use ratatui::{
  buffer::Buffer,
  layout::{Constraint, Direction, Layout, Rect},
  style::Style,
  widgets::{Block, Borders, Widget},
};

use crate::tui::theme::Theme;

/// Document browser view state
#[derive(Debug, Default)]
pub struct DocumentState {
  pub documents: Vec<DocSearchItem>,
  pub selected: usize,
  pub search_query: String,
  pub detail_scroll: u16,
  pub loading: bool,
  pub error: Option<String>,
}

impl DocumentState {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn set_documents(&mut self, documents: Vec<DocSearchItem>) {
    self.documents = documents;
    if self.selected >= self.documents.len() && !self.documents.is_empty() {
      self.selected = self.documents.len() - 1;
    }
  }

  pub fn selected_document(&self) -> Option<&DocSearchItem> {
    self.documents.get(self.selected)
  }

  pub fn select_next(&mut self) {
    if self.documents.is_empty() {
      return;
    }
    self.selected = (self.selected + 1).min(self.documents.len() - 1);
    self.detail_scroll = 0;
  }

  pub fn select_prev(&mut self) {
    if self.documents.is_empty() {
      return;
    }
    self.selected = self.selected.saturating_sub(1);
    self.detail_scroll = 0;
  }

  pub fn scroll_detail_down(&mut self) {
    self.detail_scroll = self.detail_scroll.saturating_add(1);
  }

  pub fn scroll_detail_up(&mut self) {
    self.detail_scroll = self.detail_scroll.saturating_sub(1);
  }
}

/// Document browser view widget
pub struct DocumentView<'a> {
  state: &'a DocumentState,
}

impl<'a> DocumentView<'a> {
  pub fn new(state: &'a DocumentState) -> Self {
    Self { state }
  }
}

impl Widget for DocumentView<'_> {
  fn render(self, area: Rect, buf: &mut Buffer) {
    // Split into list and detail panels
    let chunks = Layout::default()
      .direction(Direction::Horizontal)
      .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
      .split(area);

    // Render list panel
    self.render_list(chunks[0], buf);

    // Render detail panel
    self.render_detail(chunks[1], buf);
  }
}

impl DocumentView<'_> {
  fn render_list(&self, area: Rect, buf: &mut Buffer) {
    let title = if !self.state.search_query.is_empty() {
      format!(
        "DOCUMENTS ({}) - Search: {}",
        self.state.documents.len(),
        self.state.search_query
      )
    } else {
      format!("DOCUMENTS ({})", self.state.documents.len())
    };

    let border_color = Theme::ACCENT;

    let block = Block::default()
      .title(title)
      .title_style(Style::default().fg(Theme::REFLECTIVE).bold())
      .borders(Borders::ALL)
      .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    block.render(area, buf);

    if self.state.documents.is_empty() {
      let msg = if self.state.loading {
        "Loading..."
      } else if let Some(ref err) = self.state.error {
        err
      } else {
        "No documents found"
      };
      buf.set_string(inner.x, inner.y, msg, Style::default().fg(Theme::MUTED));
      return;
    }

    // Render document list items
    let visible_height = inner.height as usize;
    let start = if self.state.selected >= visible_height {
      self.state.selected - visible_height + 1
    } else {
      0
    };

    for (i, doc) in self.state.documents.iter().enumerate().skip(start).take(visible_height) {
      let y = inner.y + (i - start) as u16;
      if y >= inner.y + inner.height {
        break;
      }

      let is_selected = i == self.state.selected;
      self.render_document_item(doc, inner.x, y, inner.width, is_selected, buf);
    }
  }

  fn render_document_item(&self, doc: &DocSearchItem, x: u16, y: u16, width: u16, selected: bool, buf: &mut Buffer) {
    let title = &doc.title;
    // DocSearchItem doesn't have source_type, derive from source
    let source_type = if doc.source.starts_with("http") { "url" } else { "file" };
    let chunk_count = doc.total_chunks;

    let bg = if selected { Theme::SURFACE } else { Theme::BG };
    let fg = if selected { Theme::TEXT } else { Theme::SUBTEXT };

    // Clear line with background
    for i in 0..width {
      buf[(x + i, y)].set_bg(bg);
    }

    // Selection indicator
    let indicator = if selected { "▶ " } else { "  " };
    buf.set_string(x, y, indicator, Style::default().fg(Theme::ACCENT));

    // Source type icon
    let icon = match source_type {
      "url" => "🌐 ",
      "file" => "📄 ",
      _ => "📝 ",
    };
    buf.set_string(x + 2, y, icon, Style::default());

    // Title
    let title_start = x + 5;
    let title_width = width.saturating_sub(title_start - x + 8) as usize;
    let display_title = if title.len() > title_width {
      format!("{}...", &title[..title_width.saturating_sub(3)])
    } else {
      title.to_string()
    };
    buf.set_string(title_start, y, &display_title, Style::default().fg(fg));

    // Chunk count
    let count = format!(" ({} chunks)", chunk_count);
    let count_x = x + width.saturating_sub(count.len() as u16 + 1);
    buf.set_string(count_x, y, &count, Style::default().fg(Theme::MUTED));
  }

  fn render_detail(&self, area: Rect, buf: &mut Buffer) {
    let block = Block::default()
      .title("DOCUMENT DETAIL")
      .title_style(Style::default().fg(Theme::ACCENT).bold())
      .borders(Borders::ALL)
      .border_style(Style::default().fg(Theme::OVERLAY));

    let inner = block.inner(area);
    block.render(area, buf);

    let Some(doc) = self.state.selected_document() else {
      buf.set_string(
        inner.x,
        inner.y,
        "Select a document to view details",
        Style::default().fg(Theme::MUTED),
      );
      return;
    };

    let mut y = inner.y;

    // Title
    let title = &doc.title;
    buf.set_string(inner.x, y, "Title: ", Style::default().fg(Theme::SUBTEXT));
    buf.set_string(inner.x + 7, y, title, Style::default().fg(Theme::TEXT).bold());
    y += 1;

    // ID
    let id = &doc.id;
    let short_id = if id.len() > 8 { &id[..8] } else { id };
    buf.set_string(inner.x, y, "ID: ", Style::default().fg(Theme::SUBTEXT));
    buf.set_string(inner.x + 4, y, short_id, Style::default().fg(Theme::TEXT));
    buf.set_string(
      inner.x + 4 + short_id.len() as u16,
      y,
      "...",
      Style::default().fg(Theme::MUTED),
    );
    y += 1;

    // Source
    let source = &doc.source;
    buf.set_string(inner.x, y, "Source: ", Style::default().fg(Theme::SUBTEXT));
    let max_len = inner.width as usize - 8;
    let display_source = if source.len() > max_len {
      format!("{}...", &source[..max_len - 3])
    } else {
      source.to_string()
    };
    buf.set_string(inner.x + 8, y, &display_source, Style::default().fg(Theme::INFO));
    y += 1;

    // Source type - derive from source
    let source_type = if doc.source.starts_with("http") { "url" } else { "file" };
    buf.set_string(inner.x, y, "Type: ", Style::default().fg(Theme::SUBTEXT));
    buf.set_string(
      inner.x + 6,
      y,
      capitalize(source_type),
      Style::default().fg(Theme::TEXT),
    );
    y += 1;

    // Chunks info
    let chunk_count = doc.total_chunks;
    buf.set_string(inner.x, y, "Chunks: ", Style::default().fg(Theme::SUBTEXT));
    buf.set_string(
      inner.x + 8,
      y,
      format!("{}/{}", doc.chunk_index + 1, chunk_count),
      Style::default().fg(Theme::TEXT),
    );
    y += 1;

    y += 1;

    // Content preview
    let content = &doc.content;
    buf.set_string(inner.x, y, "CONTENT PREVIEW", Style::default().fg(Theme::ACCENT).bold());
    y += 1;

    let lines: Vec<&str> = content.lines().collect();
    let scroll = self.state.detail_scroll as usize;

    for line in lines.iter().skip(scroll) {
      if y >= inner.y + inner.height {
        break;
      }

      let display_line = if line.len() > inner.width as usize {
        format!("{}...", &line[..inner.width as usize - 3])
      } else {
        line.to_string()
      };
      buf.set_string(inner.x, y, &display_line, Style::default().fg(Theme::TEXT));
      y += 1;
    }
  }
}

fn capitalize(s: &str) -> String {
  let mut chars = s.chars();
  match chars.next() {
    None => String::new(),
    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
  }
}
