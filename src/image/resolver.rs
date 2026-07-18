#![allow(dead_code)]

use anyhow::Result;
use async_trait::async_trait;

use crate::image::progress::{status, ProgressSender};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageSource {
    Docker,
    DockerArchive,
    Oci,
    Registry,
    Podman,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    DockerSave,
    OciLayout,
}

#[async_trait]
pub trait Resolver {
    async fn fetch(&self, image_ref: &str) -> Result<crate::image::Image>;
    fn source_type(&self) -> ImageSource;

    /// Fetch an image while reporting progress to the given sender.
    ///
    /// The default implementation sends a single status message and delegates to
    /// [`Resolver::fetch`]. Resolvers that can provide finer-grained updates
    /// should override this method.
    async fn fetch_with_progress(
        &self,
        image_ref: &str,
        progress: ProgressSender,
    ) -> Result<crate::image::Image> {
        status(&progress, format!("Loading {}", image_ref)).await;
        self.fetch(image_ref).await
    }
}

/// Returns true if the URI should be handled by the Docker daemon resolver.
/// This includes explicit `docker://` URIs and bare references like `ubuntu:latest`.
pub fn is_docker_uri(uri: &str) -> bool {
    uri.starts_with("docker://") || !uri.contains("://")
}
