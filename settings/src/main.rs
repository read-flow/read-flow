use std::path::PathBuf;

use clap::Parser;

mod app;
mod pages;
mod save;
mod section;
mod widgets;

use app::App;

#[derive(Debug, Parser)]
#[command(name = "read-flow-settings", about = "Settings editor for read-flow")]
struct Cli {
    #[arg(long, short = 'c', help = "Path to read-flow.toml")]
    config: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    let config_path = cli
        .config
        .unwrap_or_else(read_flow_core::settings::config_path);

    let settings =
        read_flow_core::settings::Settings::extract_from(&config_path).unwrap_or_default();

    iced::application(
        {
            let cp = config_path.clone();
            let s = settings.clone();
            move || App::new(cp.clone(), s.clone())
        },
        App::update,
        App::view,
    )
    .title(App::title)
    .window_size(iced::Size::new(920.0, 640.0))
    .run()
    .map_err(|e| anyhow::anyhow!("{e}"))
}
