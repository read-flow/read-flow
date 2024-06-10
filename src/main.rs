use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use archive_organizer::{
    file_system_visitor::FileSystemVisitor,
    modules::{file_extension_finder::FileExtensionFinder, git::GitProjects},
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
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let visitor = FileSystemVisitor::new(
        vec![Box::<GitProjects>::default()],
        vec![
            Box::new(FileExtensionFinder::new("pdf".into())),
            Box::new(FileExtensionFinder::new("epub".into())),
            Box::new(FileExtensionFinder::new("mobi".into())),
        ],
    );

    match cli.command {
        Commands::Scan { path } => {
            let path = path.canonicalize()?;
            visitor.visit(&path)?
        }
    }

    visitor.finalize();

    Ok(())
}
