#![allow(dead_code)]

use std::io::BufReader;
use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;

use crate::analysis::archive::parse_docker_save_tar;
use crate::image::resolver::{ImageSource, Resolver};
use crate::image::Image;

pub struct DockerArchiveResolver;

impl Default for DockerArchiveResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl DockerArchiveResolver {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Resolver for DockerArchiveResolver {
    async fn fetch(&self, image_ref: &str) -> Result<Image> {
        let path_str = image_ref
            .strip_prefix("docker-archive://")
            .ok_or_else(|| anyhow::anyhow!("Invalid docker-archive URI: {}", image_ref))?;
        let path = PathBuf::from(path_str);

        let file = std::fs::File::open(&path)?;
        let reader = BufReader::new(file);

        let mut image = parse_docker_save_tar(reader)?;
        image.reference = image_ref.to_string();

        Ok(image)
    }

    fn source_type(&self) -> ImageSource {
        ImageSource::DockerArchive
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::resolver::Resolver;

    #[tokio::test]
    async fn test_fetch_fixture_tar() {
        let path = format!(
            "docker-archive://{}/tests/fixtures/test-docker-image.tar",
            std::env::current_dir().unwrap().display()
        );
        let resolver = DockerArchiveResolver::new();
        let image = resolver.fetch(&path).await.unwrap();
        assert_eq!(image.reference, path);
        assert!(!image.layers.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_invalid_scheme() {
        let resolver = DockerArchiveResolver::new();
        let err = resolver.fetch("oci://layout").await.unwrap_err();
        assert!(err.to_string().contains("Invalid docker-archive URI"));
    }

    #[tokio::test]
    async fn test_fetch_missing_file() {
        let resolver = DockerArchiveResolver::new();
        let err = resolver
            .fetch("docker-archive:///does/not/exist.tar")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("No such file") || err.to_string().contains("os error"));
    }

    #[test]
    fn test_source_type() {
        assert_eq!(
            DockerArchiveResolver::new().source_type(),
            ImageSource::DockerArchive
        );
    }
}
