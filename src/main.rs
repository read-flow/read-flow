use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{filter::EnvFilter, fmt, prelude::*};

use archive_organizer::{
    file_system_visitor::FileSystemVisitor,
    get_connection_pool,
    gui::gui,
    modules::{file_extension_finder::FileExtensionFinder, scm_project_finder::ScmProjectFinder},
};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Scan { path: PathBuf },
    Gui,
}

fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Scan { path } => scan(path)?,
        Commands::Gui => gui()?,
    };

    Ok(())
}

fn scan(path: PathBuf) -> Result<()> {
    let connection_pool = get_connection_pool();

    let visitor = FileSystemVisitor::new(
        vec![
            Box::new(ScmProjectFinder::new(
                ".git".into(),
                connection_pool.clone(),
            )),
            Box::new(ScmProjectFinder::new(".hg".into(), connection_pool.clone())),
        ],
        vec![
            Box::new(FileExtensionFinder::new(
                "pdf".into(),
                connection_pool.clone(),
            )),
            Box::new(FileExtensionFinder::new(
                "epub".into(),
                connection_pool.clone(),
            )),
            Box::new(FileExtensionFinder::new("mobi".into(), connection_pool)),
        ],
    );

    let path = path.canonicalize()?;
    visitor.visit(&path)?;

    visitor.finalize();

    Ok(())
}
