use std::fs::OpenOptions;

use clap::Parser;
use tracing_subscriber::EnvFilter;

mod analysis;
mod cli;
mod config;
mod image;
mod tui;
mod utils;

use analysis::analyzers::efficiency::EfficiencyAnalyzer;
use analysis::report::Analyzer;
use cli::Cli;
use config::Config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Cli::parse();
    init_tracing(&args);

    tracing::info!("deep-dive starting for image: {}", args.image);

    let config = Config::load(args.config.as_deref())?;
    let analyzers: Vec<Box<dyn Analyzer>> = vec![Box::new(EfficiencyAnalyzer)];

    tui::app::run(args.image, analyzers, config).await
}

fn init_tracing(args: &Cli) {
    let filter = match (args.verbose, args.quiet) {
        (0, true) => "error",
        (0, false) => "warn",
        (1, _) => "info",
        (2, _) => "debug",
        _ => "trace",
    };

    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("deep-dive.log");

    if let Ok(file) = log_file {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter)),
            )
            .with_writer(move || {
                file.try_clone()
                    .map(|f| Box::new(f) as Box<dyn std::io::Write + Send>)
                    .unwrap_or_else(|_| Box::new(std::io::sink()))
            })
            .try_init();
    } else {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter)),
            )
            .try_init();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::resolver::is_docker_uri;

    #[tokio::test]
    async fn test_resolve_image_unsupported_scheme() {
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let err = image::resolve_with_progress("ftp://example.com/image.tar", tx)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Unsupported image URI"));
    }

    #[test]
    fn test_is_docker_uri() {
        assert!(is_docker_uri("docker://ubuntu:latest"));
        assert!(is_docker_uri("ubuntu:latest"));
        assert!(!is_docker_uri("docker-archive://image.tar"));
        assert!(!is_docker_uri("oci://layout"));
    }
}
