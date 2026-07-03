use clap::Parser;
use tracing_subscriber::EnvFilter;

mod analysis;
mod cli;
mod config;
mod image;
mod tui;
mod utils;

use cli::Cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Cli::parse();

    let filter = match (args.verbose, args.quiet) {
        (0, true) => "error",
        (0, false) => "warn",
        (1, _) => "info",
        (2, _) => "debug",
        _ => "trace",
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter)),
        )
        .init();

    tracing::info!("deep-dive starting for image: {}", args.image);

    eprintln!("deep-dive is not yet implemented. Image: {}", args.image);

    Ok(())
}
