#![allow(dead_code)]

pub mod docker;
pub mod oci;
pub mod podman;
pub mod progress;
pub mod registry;
pub mod resolver;

use crate::analysis::filetree::FileTree;

#[derive(Debug, Clone)]
pub struct Image {
    pub reference: String,
    pub layers: Vec<Layer>,
}

#[derive(Debug, Clone, Default)]
pub struct Layer {
    pub index: usize,
    pub command: String,
    pub size: u64,
    pub tree: FileTree,
    /// Layer identifier (e.g., directory hash from a Docker save archive).
    pub id: Option<String>,
    /// Content digest when available (e.g., OCI layer descriptor digest).
    pub digest: Option<String>,
    /// Tags associated with this layer in the archive (typically image tags).
    pub tags: Vec<String>,
}

impl Layer {
    pub fn new(index: usize, command: impl Into<String>, size: u64, tree: FileTree) -> Self {
        Self {
            index,
            command: command.into(),
            size,
            tree,
            ..Default::default()
        }
    }
}

/// Resolve an image URI while reporting progress to the given sender.
///
/// The URI scheme selects the resolver:
/// - `docker://` or bare reference → Docker daemon
/// - `docker-archive://` → Docker save tar file
/// - `oci://` → OCI layout directory or archive
/// - `registry://` → OCI registry
/// - `podman://` → Podman
pub async fn resolve_with_progress(
    uri: &str,
    progress: progress::ProgressSender,
) -> anyhow::Result<Image> {
    use resolver::{is_docker_uri, Resolver};

    if is_docker_uri(uri) {
        docker::engine::DockerEngineResolver::new()?
            .fetch_with_progress(uri, progress)
            .await
    } else if uri.starts_with("docker-archive://") {
        docker::archive::DockerArchiveResolver::new()
            .fetch_with_progress(uri, progress)
            .await
    } else if uri.starts_with("oci://") {
        oci::layout::OciLayoutResolver::new()
            .fetch_with_progress(uri, progress)
            .await
    } else if uri.starts_with("registry://") {
        registry::RegistryResolver::new()
            .fetch_with_progress(uri, progress)
            .await
    } else if uri.starts_with("podman://") {
        podman::resolver::PodmanResolver::new()
            .fetch_with_progress(uri, progress)
            .await
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

    fn cwd() -> String {
        std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .to_string()
    }

    #[tokio::test]
    async fn test_resolve_docker_archive_fixture() {
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let uri = format!(
            "docker-archive://{}/tests/fixtures/test-docker-image.tar",
            cwd()
        );
        let image = resolve_with_progress(&uri, tx).await.unwrap();
        assert_eq!(image.reference, uri);
        assert!(!image.layers.is_empty());
    }

    #[tokio::test]
    async fn test_resolve_oci_layout_fixture() {
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let uri = format!("oci://{}/tests/fixtures/test-oci-gzip-image.tar", cwd());
        let image = resolve_with_progress(&uri, tx).await.unwrap();
        assert_eq!(image.reference, uri);
    }

    #[tokio::test]
    async fn test_resolve_unsupported_scheme() {
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let err = resolve_with_progress("ftp://example.com", tx)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Unsupported image URI"));
    }
}
