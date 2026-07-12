#![allow(dead_code)]

use anyhow::Result;
use async_trait::async_trait;

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
}
