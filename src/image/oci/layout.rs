#![allow(dead_code)]

use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use anyhow::{Context, Result};
use async_trait::async_trait;

use crate::analysis::archive::{parse_oci_archive_tar, parse_oci_layout};
use crate::image::resolver::{ImageSource, Resolver};
use crate::image::Image;

pub struct OciLayoutResolver;

impl Default for OciLayoutResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl OciLayoutResolver {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Resolver for OciLayoutResolver {
    async fn fetch(&self, image_ref: &str) -> Result<Image> {
        let path_str = image_ref
            .strip_prefix("oci://")
            .ok_or_else(|| anyhow::anyhow!("Invalid oci URI: {}", image_ref))?;
        let path = PathBuf::from(path_str);

        let mut image = if path.is_dir() {
            parse_oci_layout(&path)?
        } else if path.is_file() {
            let file = File::open(&path)
                .with_context(|| format!("failed to open OCI archive {}", path_str))?;
            parse_oci_archive_tar(BufReader::new(file))?
        } else {
            anyhow::bail!("OCI path does not exist: {}", path_str);
        };

        image.reference = image_ref.to_string();

        Ok(image)
    }

    fn source_type(&self) -> ImageSource {
        ImageSource::Oci
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_oci_layout_resolver_directory() {
        let dir = extract_oci_fixture("test-oci-gzip-image.tar");
        let resolver = OciLayoutResolver::new();
        let uri = format!("oci://{}", dir.path().display());
        let image = resolver.fetch(&uri).await.unwrap();

        assert_eq!(image.reference, uri);
        assert_eq!(image.layers.len(), 1);
        assert!(!image.layers[0].command.is_empty());
    }

    #[tokio::test]
    async fn test_oci_layout_resolver_archive() {
        let resolver = OciLayoutResolver::new();
        let image = resolver
            .fetch("oci://tests/fixtures/test-oci-zstd-image.tar")
            .await
            .unwrap();

        assert_eq!(
            image.reference,
            "oci://tests/fixtures/test-oci-zstd-image.tar"
        );
        assert_eq!(image.layers.len(), 1);
        assert!(!image.layers[0].command.is_empty());
    }

    fn extract_oci_fixture(tar_name: &str) -> tempfile::TempDir {
        use std::fs::File;
        let dir = tempfile::tempdir().unwrap();
        let file = File::open(format!("tests/fixtures/{}", tar_name)).unwrap();
        let mut archive = tar::Archive::new(file);
        archive.unpack(dir.path()).unwrap();
        dir
    }
}
