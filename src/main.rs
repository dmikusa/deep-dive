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
use analysis::report::{Analyzer, Report};
use cli::Cli;
use config::Config;
use image::docker::archive::DockerArchiveResolver;
use image::docker::engine::DockerEngineResolver;
use image::oci::layout::OciLayoutResolver;
use image::resolver::Resolver;
use image::Image;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Cli::parse();
    init_tracing(&args);

    tracing::info!("deep-dive starting for image: {}", args.image);

    let config = Config::load(args.config.as_deref())?;

    let image = if is_docker_uri(&args.image) {
        DockerEngineResolver::new()?.fetch(&args.image).await
    } else {
        let mut loader = tui::loader::Loader::new(format!("Loading {}", args.image));
        let result = resolve_image(&args.image).await;
        loader.stop();
        result
    }?;

    let analyzers: Vec<Box<dyn Analyzer>> = vec![Box::new(EfficiencyAnalyzer)];
    let report = Report::generate(&image, &analyzers)?;

    tui::app::run(image, report, config).await
}

fn is_docker_uri(uri: &str) -> bool {
    uri.starts_with("docker://") || !uri.contains("://")
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

async fn resolve_image(uri: &str) -> anyhow::Result<Image> {
    if uri.starts_with("docker-archive://") {
        DockerArchiveResolver::new().fetch(uri).await
    } else if uri.starts_with("oci://") {
        OciLayoutResolver::new().fetch(uri).await
    } else if uri.starts_with("registry://") {
        anyhow::bail!(
            "Registry resolver is not yet implemented (coming in Phase 10). \
             Use docker://, docker-archive://path/to/image.tar, or oci://path/to/layout instead."
        )
    } else if uri.starts_with("podman://") {
        anyhow::bail!(
            "Podman resolver is not yet implemented (coming in Phase 11). \
             Use docker://, docker-archive://path/to/image.tar, or oci://path/to/layout instead."
        )
    } else {
        anyhow::bail!(
            "Unsupported image URI: {}. Expected one of: \
             docker://..., docker-archive://..., oci://..., registry://..., podman://...",
            uri
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_resolve_image_unsupported_scheme() {
        let err = resolve_image("ftp://example.com/image.tar")
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
