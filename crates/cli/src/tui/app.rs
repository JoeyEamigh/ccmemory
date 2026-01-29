use std::{io, path::PathBuf, time::Duration};

use anyhow::Result;
use ccengram::ipc::{
  Client,
  code::{CodeListParams, CodeSearchParams, CodeStatsParams},
  docs::DocsSearchParams,
  memory::{MemoryDeemphasizeParams, MemoryListParams, MemoryReinforceParams, MemorySearchParams},
  system::{HealthCheckParams, MetricsParams, ProjectStatsParams, ShutdownParams},
  watch::WatchStatusParams,
};
use crossterm::{
  event::{self, Event as CrosstermEvent, KeyEventKind},
  execute,
  terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
  Terminal,
  backend::CrosstermBackend,
  buffer::Buffer,
  layout::{Constraint, Direction, Layout, Rect},
  style::Style,
  widgets::{Block, Borders, Clear, Widget},
};
use tokio::time::interval;
use tracing::{info, warn};

use crate::tui::{
  event::{Action, key_to_action},
  theme::Theme,
  views::{
    CodeView, DashboardView, DocumentView, MemoryView, SearchView, SessionView,
    code::CodeState,
    dashboard::DashboardState,
    document::DocumentState,
    memory::MemoryState,
    search::{SearchResult, SearchResultType, SearchState},
    session::SessionState,
  },
};

/// The current view being displayed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
  #[default]
  Dashboard,
  Memory,
  Code,
  Document,
  Session,
  Search,
}

impl View {
  pub fn name(&self) -> &'static str {
    match self {
      View::Dashboard => "Dashboard",
      View::Memory => "Memories",
      View::Code => "Code",
      View::Document => "Docs",
      View::Session => "Sessions",
      View::Search => "Search",
    }
  }

  pub fn index(&self) -> usize {
    match self {
      View::Dashboard => 0,
      View::Memory => 1,
      View::Code => 2,
      View::Document => 3,
      View::Session => 4,
      View::Search => 5,
    }
  }

  pub fn from_index(index: usize) -> Self {
    match index {
      0 => View::Dashboard,
      1 => View::Memory,
      2 => View::Code,
      3 => View::Document,
      4 => View::Session,
      5 => View::Search,
      _ => View::Dashboard,
    }
  }
}

/// Input mode for the application
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
  #[default]
  Normal,
  Search,
  Filter,
}

/// Main application state
pub struct App {
  pub current_view: View,
  pub client: Client,
  pub daemon_started_by_tui: bool,
  pub input_mode: InputMode,
  pub should_quit: bool,
  pub show_help: bool,
  pub project_path: PathBuf,

  // View states
  pub dashboard: DashboardState,
  pub memory: MemoryState,
  pub code: CodeState,
  pub document: DocumentState,
  pub session: SessionState,
  pub search: SearchState,
}

impl App {
  pub async fn new(project_path: PathBuf) -> Result<Self> {
    let (client, daemon_started_by_tui) = if let Ok(client) = Client::connect(project_path.clone()).await {
      info!("Connected to existing daemon");
      (client, false)
    } else {
      info!("Starting daemon and connecting");
      (ccengram::Daemon::connect_or_start(project_path.clone()).await?, true)
    };

    Ok(Self {
      current_view: View::Dashboard,
      client,
      daemon_started_by_tui,
      input_mode: InputMode::Normal,
      should_quit: false,
      show_help: false,
      project_path,
      dashboard: DashboardState::new(),
      memory: MemoryState::new(),
      code: CodeState::new(),
      document: DocumentState::new(),
      session: SessionState::new(),
      search: SearchState::new(),
    })
  }

  pub async fn refresh_current_view(&mut self) {
    match self.current_view {
      View::Dashboard => {
        self.dashboard.loading = true;
        if let Ok(stats) = self.client.call(ProjectStatsParams).await {
          self.dashboard.set_stats(stats);
        }
        if let Ok(health) = self.client.call(HealthCheckParams).await {
          self.dashboard.set_health(health);
        }
        if let Ok(watch) = self.client.call(WatchStatusParams).await {
          self.dashboard.set_watch_status(watch);
        }
        if let Ok(stats) = self.client.call(CodeStatsParams).await {
          self.dashboard.set_code_stats(stats);
        }
        if let Ok(metrics) = self.client.call(MetricsParams).await {
          self.dashboard.set_daemon_metrics(metrics);
        }
        self.dashboard.loading = false;
      }
      View::Memory => {
        self.memory.loading = true;
        match self
          .client
          .call(MemoryListParams {
            offset: Some(0),
            limit: Some(100),
            ..Default::default()
          })
          .await
        {
          Ok(memories) => {
            self.memory.set_memories(memories);
            self.memory.error = None;
          }
          Err(e) => {
            self.memory.error = Some(format!("{}", e));
          }
        }
        self.memory.loading = false;
      }
      View::Code => {
        self.code.loading = true;
        match self.client.call(CodeListParams { limit: Some(100) }).await {
          Ok(chunks) => {
            self.code.set_chunks(chunks);
            self.code.error = None;
          }
          Err(e) => {
            self.code.error = Some(format!("{}", e));
          }
        }
        if let Ok(stats) = self.client.call(CodeStatsParams).await {
          self.code.set_stats(stats);
        }
        self.code.loading = false;
      }
      View::Document => {
        self.document.loading = true;
        // Documents are fetched via docs_search with empty query to list all
        match self
          .client
          .call(DocsSearchParams {
            limit: Some(100),
            ..Default::default()
          })
          .await
        {
          Ok(docs) => {
            self.document.set_documents(docs);
            self.document.error = None;
          }
          Err(e) => {
            self.document.error = Some(format!("{}", e));
          }
        }
        self.document.loading = false;
      }
      View::Session => {
        self.session.loading = true;
        // TODO: Session view needs a proper session listing API
        // For now, just show that there's no session data available
        self.session.error = Some("Session listing not yet implemented".to_string());
        self.session.loading = false;
      }
      View::Search => {
        // Search is triggered explicitly, not on refresh
      }
    }
  }

  pub async fn handle_action(&mut self, action: Action) {
    match action {
      Action::Quit => self.should_quit = true,
      Action::SwitchView(index) => {
        self.current_view = View::from_index(index);
        self.input_mode = InputMode::Normal;
        self.refresh_current_view().await;
      }
      Action::NavigateUp => self.navigate_up(),
      Action::NavigateDown => self.navigate_down(),
      Action::NavigateLeft => self.navigate_left(),
      Action::NavigateRight => self.navigate_right(),
      Action::Select => self.select().await,
      Action::Back => self.back(),
      Action::OpenSearch => self.open_search(),
      Action::OpenFilter => self.open_filter(),
      Action::ToggleHelp => self.show_help = !self.show_help,
      Action::Reinforce => self.reinforce().await,
      Action::Deemphasize => {
        // In Search view, 'd' toggles documents scope instead of deemphasize
        if self.current_view == View::Search && !self.search.input_active {
          self.toggle_search_documents().await;
        } else {
          self.deemphasize().await;
        }
      }
      Action::Submit => self.submit().await,
      Action::Input(c) => self.input_char(c),
      Action::DeleteChar => self.delete_char(),
      Action::PageUp => self.page_up(),
      Action::PageDown => self.page_down(),
      Action::GoToTop => self.go_to_top(),
      Action::GoToBottom => self.go_to_bottom(),
      Action::NextPanel => self.next_panel(),
      Action::Refresh => self.refresh_current_view().await,
      Action::CycleSort => self.cycle_sort(),
      Action::ToggleSearchMemories => self.toggle_search_memories().await,
      Action::ToggleSearchCode => self.toggle_search_code().await,
      Action::None => {}
    }
  }

  fn navigate_up(&mut self) {
    match self.current_view {
      View::Memory => self.memory.select_prev(),
      View::Code => self.code.select_prev(),
      View::Document => self.document.select_prev(),
      View::Session => self.session.select_prev(),
      View::Search => self.search.select_prev(),
      _ => {}
    }
  }

  fn navigate_down(&mut self) {
    match self.current_view {
      View::Memory => self.memory.select_next(),
      View::Code => self.code.select_next(),
      View::Document => self.document.select_next(),
      View::Session => self.session.select_next(),
      View::Search => self.search.select_next(),
      _ => {}
    }
  }

  fn navigate_left(&mut self) {
    match self.current_view {
      View::Memory => self.memory.scroll_detail_up(),
      View::Document => self.document.scroll_detail_up(),
      _ => {}
    }
  }

  fn navigate_right(&mut self) {
    match self.current_view {
      View::Memory => self.memory.scroll_detail_down(),
      View::Document => self.document.scroll_detail_down(),
      _ => {}
    }
  }

  async fn select(&mut self) {
    match self.current_view {
      View::Session => self.session.toggle_expand(),
      View::Search => {
        if self.search.input_active {
          self.execute_search().await;
        }
      }
      _ => {}
    }
  }

  fn back(&mut self) {
    match self.input_mode {
      InputMode::Search => {
        self.input_mode = InputMode::Normal;
        if self.current_view == View::Search {
          self.search.input_active = false;
        }
      }
      InputMode::Filter => {
        // Cancel filter input, clear filter text (don't apply)
        self.input_mode = InputMode::Normal;
        self.search.filter_input_active = false;
        self.search.filter_text.clear();
        // Keep any previously applied filter (don't call clear_filter)
      }
      InputMode::Normal => {
        if self.show_help {
          self.show_help = false;
        } else if self.current_view == View::Search && self.search.filter_active {
          // Clear active filter first
          self.search.clear_filter();
        } else {
          self.current_view = View::Dashboard;
        }
      }
    }
  }

  fn open_search(&mut self) {
    self.input_mode = InputMode::Search;
    if self.current_view == View::Search {
      self.search.input_active = true;
    } else {
      // Switch to search view
      self.current_view = View::Search;
      self.search.input_active = true;
    }
  }

  fn open_filter(&mut self) {
    // Filter only works in Search view
    if self.current_view == View::Search {
      self.input_mode = InputMode::Filter;
      self.search.filter_input_active = true;
      // Clear any previous partial filter text when opening filter mode
      self.search.filter_text.clear();
    }
  }

  async fn reinforce(&mut self) {
    if self.current_view != View::Memory {
      return;
    }
    let Some(id) = self.memory.selected_id() else {
      return;
    };

    if let Err(e) = self
      .client
      .call(MemoryReinforceParams {
        memory_id: id,
        ..Default::default()
      })
      .await
    {
      self.memory.error = Some(format!("Reinforce failed: {}", e));
    } else {
      // Refresh to show updated salience
      self.refresh_current_view().await;
    }
  }

  async fn deemphasize(&mut self) {
    if self.current_view != View::Memory {
      return;
    }
    let Some(id) = self.memory.selected_id() else {
      return;
    };

    if let Err(e) = self
      .client
      .call(MemoryDeemphasizeParams {
        memory_id: id,
        ..Default::default()
      })
      .await
    {
      self.memory.error = Some(format!("Deemphasize failed: {}", e));
    } else {
      self.refresh_current_view().await;
    }
  }

  async fn submit(&mut self) {
    match self.input_mode {
      InputMode::Search => {
        self.execute_search().await;
        self.input_mode = InputMode::Normal;
        self.search.input_active = false;
      }
      InputMode::Filter => {
        // Apply filter and exit filter mode
        self.search.apply_filter();
        self.input_mode = InputMode::Normal;
        self.search.filter_input_active = false;
      }
      InputMode::Normal => {}
    }
  }

  fn input_char(&mut self, c: char) {
    match self.input_mode {
      InputMode::Search => {
        if self.current_view == View::Search {
          self.search.query.push(c);
        } else {
          self.memory.search_query.push(c);
        }
      }
      InputMode::Filter => {
        // Add to filter string and apply live filtering
        self.search.filter_text.push(c);
        self.search.apply_filter();
      }
      InputMode::Normal => {}
    }
  }

  fn delete_char(&mut self) {
    match self.input_mode {
      InputMode::Search => {
        if self.current_view == View::Search {
          self.search.query.pop();
        } else {
          self.memory.search_query.pop();
        }
      }
      InputMode::Filter => {
        // Remove from filter string and apply live filtering
        self.search.filter_text.pop();
        self.search.apply_filter();
      }
      InputMode::Normal => {}
    }
  }

  fn page_up(&mut self) {
    for _ in 0..10 {
      self.navigate_up();
    }
  }

  fn page_down(&mut self) {
    for _ in 0..10 {
      self.navigate_down();
    }
  }

  fn go_to_top(&mut self) {
    match self.current_view {
      View::Memory => self.memory.selected = 0,
      View::Code => self.code.selected = 0,
      View::Document => self.document.selected = 0,
      View::Session => self.session.selected = 0,
      View::Search => self.search.selected = 0,
      _ => {}
    }
  }

  fn go_to_bottom(&mut self) {
    match self.current_view {
      View::Memory => {
        if !self.memory.memories.is_empty() {
          self.memory.selected = self.memory.memories.len() - 1;
        }
      }
      View::Code => {
        if !self.code.chunks.is_empty() {
          self.code.selected = self.code.chunks.len() - 1;
        }
      }
      View::Document => {
        if !self.document.documents.is_empty() {
          self.document.selected = self.document.documents.len() - 1;
        }
      }
      View::Session => {
        if !self.session.sessions.is_empty() {
          self.session.selected = self.session.sessions.len() - 1;
        }
      }
      View::Search => {
        let display_len = self.search.display_results().len();
        if display_len > 0 {
          self.search.selected = display_len - 1;
        }
      }
      _ => {}
    }
  }

  fn next_panel(&mut self) {
    // Cycle through views
    let next = (self.current_view.index() + 1) % 7;
    self.current_view = View::from_index(next);
  }

  fn cycle_sort(&mut self) {
    // Only memory view supports sorting currently
    if self.current_view == View::Memory {
      self.memory.cycle_sort();
    }
  }

  async fn toggle_search_memories(&mut self) {
    // Only works in Search view, normal mode
    if self.current_view != View::Search || self.search.input_active {
      return;
    }
    if self.search.toggle_memories() && !self.search.query.is_empty() {
      self.execute_search().await;
    }
  }

  async fn toggle_search_code(&mut self) {
    // Only works in Search view, normal mode
    if self.current_view != View::Search || self.search.input_active {
      return;
    }
    if self.search.toggle_code() && !self.search.query.is_empty() {
      self.execute_search().await;
    }
  }

  async fn toggle_search_documents(&mut self) {
    // Only works in Search view, normal mode
    if self.current_view != View::Search || self.search.input_active {
      return;
    }
    if self.search.toggle_documents() && !self.search.query.is_empty() {
      self.execute_search().await;
    }
  }

  async fn execute_search(&mut self) {
    if self.search.query.is_empty() {
      return;
    }

    self.search.loading = true;
    let mut results = Vec::new();

    // Search memories
    if self.search.search_memories
      && let Ok(result) = self
        .client
        .call(MemorySearchParams {
          query: self.search.query.clone(),
          limit: Some(20),
          ..Default::default()
        })
        .await
    {
      for memory in result.items {
        let similarity = memory.similarity.unwrap_or(0.0);
        // Convert typed struct back to Value for rendering
        if let Ok(data) = serde_json::to_value(&memory) {
          results.push(SearchResult {
            result_type: SearchResultType::Memory,
            data,
            similarity,
          });
        }
      }
    }

    // Search code
    if self.search.search_code
      && let Ok(code_result) = self
        .client
        .call(CodeSearchParams {
          query: self.search.query.clone(),
          limit: Some(20),
          ..Default::default()
        })
        .await
    {
      // CodeSearchResult has a .chunks field containing the actual results
      for chunk in code_result.chunks {
        let similarity = chunk.similarity.unwrap_or(0.0);
        // Convert typed struct back to Value for rendering
        if let Ok(data) = serde_json::to_value(&chunk) {
          results.push(SearchResult {
            result_type: SearchResultType::Code,
            data,
            similarity,
          });
        }
      }
    }

    // Search documents
    if self.search.search_documents
      && let Ok(docs) = self
        .client
        .call(DocsSearchParams {
          query: self.search.query.clone(),
          limit: Some(20),
        })
        .await
    {
      for doc in docs {
        let similarity = doc.similarity.unwrap_or(0.0);
        // Convert typed struct back to Value for rendering
        if let Ok(data) = serde_json::to_value(&doc) {
          results.push(SearchResult {
            result_type: SearchResultType::Document,
            data,
            similarity,
          });
        }
      }
    }

    // Sort by similarity
    results.sort_by(|a, b| {
      b.similarity
        .partial_cmp(&a.similarity)
        .unwrap_or(std::cmp::Ordering::Equal)
    });

    self.search.set_results(results);
    self.search.loading = false;
  }

  pub async fn cleanup(&mut self) -> Result<()> {
    if self.daemon_started_by_tui {
      info!("TUI started daemon, sending shutdown request");
      if let Err(e) = self.client.call(ShutdownParams).await {
        warn!("Failed to shutdown daemon: {}", e);
      }
    }
    Ok(())
  }
}

/// Run the TUI application
pub async fn run(project_path: PathBuf) -> Result<()> {
  // Setup terminal
  enable_raw_mode()?;
  let mut stdout = io::stdout();
  execute!(stdout, EnterAlternateScreen)?;
  let backend = CrosstermBackend::new(stdout);
  let mut terminal = Terminal::new(backend)?;

  // Create app
  let mut app = App::new(project_path).await?;

  // Initial data load
  app.refresh_current_view().await;

  // Event loop with adaptive refresh
  let mut current_refresh_interval = Duration::from_secs(30);
  let mut refresh_interval = interval(current_refresh_interval);

  loop {
    // Draw
    terminal.draw(|f| {
      render_app(&app, f.area(), f.buffer_mut());
    })?;

    // Handle events
    tokio::select! {
        _ = refresh_interval.tick() => {
            app.refresh_current_view().await;

            // Check if refresh interval should change (only on Dashboard view)
            if app.current_view == View::Dashboard {
                let suggested = app.dashboard.suggested_refresh_interval();
                if suggested != current_refresh_interval {
                    current_refresh_interval = suggested;
                    refresh_interval = interval(current_refresh_interval);
                }
            }
        }
        result = tokio::task::spawn_blocking(|| {
            if event::poll(Duration::from_millis(100)).ok()? {
                event::read().ok()
            } else {
                None
            }
        }) => {
            if let Ok(Some(event)) = result {
                match event {
                    CrosstermEvent::Key(key) if key.kind == KeyEventKind::Press => {
                        let action = key_to_action(key, app.input_mode != InputMode::Normal);
                        app.handle_action(action).await;
                    }
                    CrosstermEvent::Resize(_, _) => {
                        // Terminal will redraw on next loop
                    }
                    _ => {}
                }
            }
        }
    }

    if app.should_quit {
      break;
    }
  }

  // Cleanup
  app.cleanup().await?;
  disable_raw_mode()?;
  execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

  Ok(())
}

/// Render the application
fn render_app(app: &App, area: Rect, buf: &mut Buffer) {
  // Clear with background
  Clear.render(area, buf);
  for y in area.y..area.y + area.height {
    for x in area.x..area.x + area.width {
      buf[(x, y)].set_bg(Theme::BG);
    }
  }

  // Layout: header + content + footer
  let chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints([
      Constraint::Length(2), // Header with tabs
      Constraint::Min(10),   // Content
      Constraint::Length(1), // Footer with keybindings
    ])
    .split(area);

  // Render header with tabs
  render_header(app, chunks[0], buf);

  // Render current view
  match app.current_view {
    View::Dashboard => DashboardView::new(&app.dashboard).render(chunks[1], buf),
    View::Memory => MemoryView::new(&app.memory).render(chunks[1], buf),
    View::Code => CodeView::new(&app.code).render(chunks[1], buf),
    View::Document => DocumentView::new(&app.document).render(chunks[1], buf),
    View::Session => SessionView::new(&app.session).render(chunks[1], buf),
    View::Search => SearchView::new(&app.search).render(chunks[1], buf),
  }

  // Render footer
  render_footer(app, chunks[2], buf);

  // Render help overlay if active
  if app.show_help {
    render_help_overlay(area, buf);
  }
}

fn render_header(app: &App, area: Rect, buf: &mut Buffer) {
  // Title
  let title = "CCEngram TUI";
  buf.set_string(area.x + 1, area.y, title, Style::default().fg(Theme::ACCENT).bold());

  // Tabs
  let tabs_x = area.x + title.len() as u16 + 3;
  let views = [
    View::Dashboard,
    View::Memory,
    View::Code,
    View::Document,
    View::Session,
    View::Search,
  ];

  let mut x = tabs_x;
  for (i, view) in views.iter().enumerate() {
    let is_selected = *view == app.current_view;
    let label = format!("[{}]{} ", i + 1, view.name());

    let style = if is_selected {
      Style::default().fg(Theme::ACCENT).bold()
    } else {
      Style::default().fg(Theme::SUBTEXT)
    };

    buf.set_string(x, area.y, &label, style);
    x += label.len() as u16;
  }

  // Connection status
  // let conn_status = if app.client.() {
  //   "● Connected"
  // } else {
  //   "○ Disconnected"
  // };
  let conn_status = "● Connected";
  // let conn_color = if app.client.is_some() {
  //   Theme::SUCCESS
  // } else {
  //   Theme::ERROR
  // };
  let conn_color = Theme::SUCCESS;
  let conn_x = area.x + area.width.saturating_sub(conn_status.len() as u16 + 2);
  buf.set_string(conn_x, area.y, conn_status, Style::default().fg(conn_color));

  // Separator
  for x in area.x..area.x + area.width {
    buf[(x, area.y + 1)].set_char('─').set_fg(Theme::OVERLAY);
  }
}

fn render_footer(app: &App, area: Rect, buf: &mut Buffer) {
  let keybindings = match app.input_mode {
    InputMode::Normal => match app.current_view {
      View::Memory => "q:Quit  1-7:Views  j/k:Nav  /:Search  s:Sort  ?:Help  r/d:Salience",
      View::Search => "q:Quit  /:Search  f:Filter  m/c/d:Scopes  j/k:Nav  Esc:Clear  ?:Help",
      _ => "q:Quit  1-7:Views  j/k:Nav  /:Search  ?:Help  R:Refresh",
    },
    InputMode::Search => "Enter:Search  Esc:Cancel  Type to search...",
    InputMode::Filter => {
      let filter_hint = format!("Enter:Apply  Esc:Cancel  Filter: {}_", app.search.filter_text);
      // We'll set this directly below since it's dynamic
      return render_footer_with_filter(app, area, buf, &filter_hint);
    }
  };

  buf.set_string(area.x + 1, area.y, keybindings, Style::default().fg(Theme::MUTED));

  // Project path on right
  let path_display = app.project_path.file_name().and_then(|n| n.to_str()).unwrap_or(".");
  let path_x = area.x + area.width.saturating_sub(path_display.len() as u16 + 2);
  buf.set_string(path_x, area.y, path_display, Style::default().fg(Theme::SUBTEXT));
}

fn render_footer_with_filter(app: &App, area: Rect, buf: &mut Buffer, text: &str) {
  // Show filter input prominently
  buf.set_string(area.x + 1, area.y, text, Style::default().fg(Theme::ACCENT));

  // Project path on right
  let path_display = app.project_path.file_name().and_then(|n| n.to_str()).unwrap_or(".");
  let path_x = area.x + area.width.saturating_sub(path_display.len() as u16 + 2);
  buf.set_string(path_x, area.y, path_display, Style::default().fg(Theme::SUBTEXT));
}

fn render_help_overlay(area: Rect, buf: &mut Buffer) {
  // Center the help box
  let help_width = 55;
  let help_height = 24;
  let x = area.x + (area.width.saturating_sub(help_width)) / 2;
  let y = area.y + (area.height.saturating_sub(help_height)) / 2;

  let help_area = Rect::new(x, y, help_width, help_height);

  // Clear and draw border
  for hy in help_area.y..help_area.y + help_area.height {
    for hx in help_area.x..help_area.x + help_area.width {
      buf[(hx, hy)].set_bg(Theme::SURFACE).set_char(' ');
    }
  }

  let block = Block::default()
    .title("Help")
    .title_style(Style::default().fg(Theme::ACCENT).bold())
    .borders(Borders::ALL)
    .border_style(Style::default().fg(Theme::ACCENT));
  let inner = block.inner(help_area);
  block.render(help_area, buf);

  let help_text = [
    "NAVIGATION",
    "  1-7      Switch views",
    "  Tab      Cycle views",
    "  j/k      Navigate up/down",
    "  h/l      Scroll detail left/right",
    "  Enter    Select/expand",
    "  Esc      Back/cancel/clear filter",
    "",
    "ACTIONS",
    "  /        Open search",
    "  f        Open filter (Search view)",
    "  s        Cycle sort (Memory view)",
    "  r        Reinforce memory",
    "  d        Deemphasize memory",
    "  R        Refresh view",
    "  q        Quit",
    "  ?        Toggle help",
    "",
    "SEARCH VIEW",
    "  m        Toggle memories scope",
    "  c        Toggle code scope",
    "  d        Toggle documents scope",
  ];

  for (i, line) in help_text.iter().enumerate() {
    if i as u16 >= inner.height {
      break;
    }
    let style = if line.starts_with(|c: char| c.is_uppercase()) {
      Style::default().fg(Theme::ACCENT).bold()
    } else {
      Style::default().fg(Theme::TEXT)
    };
    buf.set_string(inner.x, inner.y + i as u16, line, style);
  }
}
