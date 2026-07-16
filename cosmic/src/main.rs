// SPDX-License-Identifier: AGPL-3.0-or-later

mod aggregator;
mod app;
mod app_theme;
#[cfg(test)]
mod bdd;
mod client;
mod component;
mod config;
mod cosmic_ext;
mod document_provider;
mod fonts;
mod forms;
mod i18n;
mod iter;
mod layout;
mod logging;
mod page;
mod reading_progress;
mod render_blocks;
mod state;
mod subscription;

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use provider::r#async::Provider;
use read_flow_core::ApplicationModule as GenericApplicationModule;
use read_flow_core::settings;
use read_flow_core::settings::Settings;
use read_flow_core::settings::SettingsError;

const ICON_SIZE: u16 = 16;

pub type ApplicationModule = GenericApplicationModule<AppSettings>;

#[derive(Debug, clap::Parser)]
pub struct Cli {
    #[clap(long)]
    /// Path to the configuration file to use instead of the default
    configuration_file: Option<PathBuf>,
    #[clap(long, default_value = "false")]
    /// Enable private mode, which makes all `--private-tags` visible
    private_mode: bool,
    #[clap(long)]
    /// Private tags and tagged files are hidden from the UI by default
    private_tags: Vec<String>,
    #[clap(long, default_value = "false")]
    /// Run the HTTP server headless (no UI) and exit when it stops
    headless: bool,
    #[clap(long)]
    /// Override the server bind address (headless mode); e.g. 0.0.0.0
    address: Option<String>,
    #[clap(long)]
    /// Override the server bind port (headless mode); 0 = pick a free port
    port: Option<u16>,
    /// Files to open on startup (EPUB, PDF, MOBI, or any file handled by the external viewer)
    files: Vec<PathBuf>,
}

#[derive(Debug)]
pub struct AppSettings {
    cli_parameters: Cli,
}

impl AppSettings {
    fn config_path(&self) -> PathBuf {
        self.cli_parameters
            .configuration_file
            .clone()
            .unwrap_or_else(settings::config_path)
    }
}

impl Provider<Settings> for AppSettings {
    type Error = SettingsError;
    async fn provide(&self) -> Result<Settings, Self::Error> {
        let Cli {
            configuration_file,
            private_mode,
            private_tags,
            ..
        } = &self.cli_parameters;

        // Extract settings from the application's configuration.
        let mut settings = match configuration_file {
            Some(path) => Settings::extract_from(path).expect("settings are present"),
            None => Settings::extract().expect("settings are present"),
        };
        // Merge commandline parameters with settings.
        settings
            .ui
            .merge_in((*private_mode, private_tags.clone()).into());

        Ok(settings)
    }
}

fn main() -> anyhow::Result<()> {
    // Initialize logging: structured JSON to stderr + in-memory capture that
    // the in-app server log page renders.
    let log_bus = logging::init();

    // Get the system's preferred languages.
    let requested_languages = i18n_embed::DesktopLanguageRequester::requested_languages();

    // Enable localizations to be applied.
    i18n::init(&requested_languages);

    // Start listing installed fonts in the background; first use is slow.
    fonts::preload();

    // Parse commandline parameters.
    let cli = Cli::parse();
    let headless = cli.headless;
    let address = cli.address.clone();
    let port = cli.port;
    let initial_files = cli.files.clone();
    let settings = AppSettings {
        cli_parameters: cli,
    };

    let config_path = settings.config_path();

    // Headless mode: run just the HTTP server, no UI.
    if headless {
        return run_headless(settings, config_path, address, port);
    }

    // Use a temporary runtime for async initialization, then drop it before COSMIC starts.
    let application_module = {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        Arc::new(rt.block_on(ApplicationModule::new(settings, config_path))?)
    };

    // Settings for configuring the application window and iced runtime.
    // Theme and interface font come from the `[ui.theme]` overrides so the
    // first frame already matches. The font family is written into the
    // in-process CosmicTk global (live-updatable); the size is a renderer
    // setting and only applies at startup.
    let theme_settings = Settings::extract_from(application_module.config_path())
        .map(|s| s.ui.theme().clone())
        .unwrap_or_default();
    app_theme::apply_interface_font(&theme_settings);
    let mut settings = cosmic::app::Settings::default()
        .size_limits(
            cosmic::iced::Limits::NONE
                .min_width(360.0)
                .min_height(180.0),
        )
        .theme(app_theme::effective_theme(
            &theme_settings,
            app_theme::current_system_variant(),
        ));
    if let Some(font) = app_theme::interface_font(&theme_settings) {
        settings = settings.default_font(font);
    }
    if let Some(size) = theme_settings.interface_font_size {
        settings = settings.default_text_size(f32::from(size));
    }

    // Starts the application's event loop.
    cosmic::app::run::<app::ReadFlow>(settings, (application_module, initial_files, log_bus))?;

    Ok(())
}

/// Run the embedded HTTP server without the UI. Blocks until the server stops.
fn run_headless(
    settings: AppSettings,
    config_path: PathBuf,
    address: Option<String>,
    port: Option<u16>,
) -> anyhow::Result<()> {
    use read_flow_core::server;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    rt.block_on(async move {
        let module = Arc::new(ApplicationModule::new(settings, config_path).await?);

        // Bind address: config (with env overrides) then CLI flags on top.
        let mut addr = module.settings().await.server.bind_addr();
        if let Some(address) = address {
            match address.parse() {
                Ok(ip) => addr.set_ip(ip),
                Err(_) => anyhow::bail!("invalid --address: {address}"),
            }
        }
        if let Some(port) = port {
            addr.set_port(port);
        }

        let tls = server::load_tls(&module.settings().await.server.tls).await?;
        let listener = tokio::net::TcpListener::bind(addr).await?;
        let scheme = if tls.is_some() { "https" } else { "http" };
        println!("Server listening on {scheme}://{}", listener.local_addr()?);
        let router = server::build_router(server::AppState::new(module)).await;
        server::serve_on(listener, router, tls).await?;
        Ok::<(), anyhow::Error>(())
    })
}
