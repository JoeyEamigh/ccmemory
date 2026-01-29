mod app;
mod event;
mod theme;
mod views;
mod widgets;

pub async fn run(project_path: std::path::PathBuf) -> anyhow::Result<()> {
  app::run(project_path).await
}
