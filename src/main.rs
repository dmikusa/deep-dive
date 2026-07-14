use clap::Parser;
use tracing_subscriber::EnvFilter;

mod analysis;
mod cli;
mod config;
mod image;
mod tui;
mod utils;

use cli::Cli;
use image::docker::archive::DockerArchiveResolver;
use image::oci::layout::OciLayoutResolver;
use image::resolver::Resolver;
use image::Image;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Cli::parse();
    init_tracing(&args);

    tracing::info!("deep-dive starting for image: {}", args.image);

    let mut loader = tui::loader::Loader::new(format!("Loading {}", args.image));
    let result = resolve_image(&args.image).await;
    loader.stop();

    let image = result?;
    tui::app::run(image).await
}

fn init_tracing(args: &Cli) {
    let filter = match (args.verbose, args.quiet) {
        (0, true) => "error",
        (0, false) => "warn",
        (1, _) => "info",
        (2, _) => "debug",
        _ => "trace",
    };

    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter)),
        )
        .try_init();
}

async fn resolve_image(uri: &str) -> anyhow::Result<Image> {
    if uri.starts_with("docker-archive://") {
        DockerArchiveResolver::new().fetch(uri).await
    } else if uri.starts_with("oci://") {
        OciLayoutResolver::new().fetch(uri).await
    } else if uri.starts_with("docker://") {
        anyhow::bail!(
            "Docker daemon resolver is not yet implemented (coming in Phase 5). \
             Use docker-archive://path/to/image.tar instead."
        )
    } else if uri.starts_with("registry://") {
        anyhow::bail!(
            "Registry resolver is not yet implemented (coming in Phase 10). \
             Use docker-archive://path/to/image.tar or oci://path/to/layout instead."
        )
    } else if uri.starts_with("podman://") {
        anyhow::bail!(
            "Podman resolver is not yet implemented (coming in Phase 11). \
             Use docker-archive://path/to/image.tar instead."
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

    #[tokio::test]
    async fn test_resolve_image_docker_scheme_not_implemented() {
        let err = resolve_image("docker://ubuntu:latest").await.unwrap_err();
        assert!(err.to_string().contains("Phase 5"));
    }
}
