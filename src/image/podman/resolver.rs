use std::io::Cursor;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use tokio::process::Command;

use crate::analysis::archive::parse_docker_save_tar;
use crate::image::progress::{status, ProgressSender};
use crate::image::resolver::{ImageSource, Resolver};
use crate::image::Image;

pub struct PodmanResolver;

impl Default for PodmanResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl PodmanResolver {
    pub fn new() -> Self {
        Self
    }

    fn normalize_image_ref(image_ref: &str) -> Result<&str> {
        let name = image_ref
            .strip_prefix("podman://")
            .ok_or_else(|| anyhow::anyhow!("Invalid podman URI: {}", image_ref))?;
        if name.is_empty() {
            anyhow::bail!("empty podman image reference");
        }
        Ok(name)
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[async_trait]
impl Resolver for PodmanResolver {
    async fn fetch(&self, image_ref: &str) -> Result<Image> {
        let image_name = Self::normalize_image_ref(image_ref)?;

        if !Self::image_exists(image_name).await {
            Self::pull_image(image_name).await?;
        }

        let output = Self::save_image(image_name).await?;

        let mut image = parse_docker_save_tar(Cursor::new(output))?;
        image.reference = image_ref.to_string();
        Ok(image)
    }

    fn source_type(&self) -> ImageSource {
        ImageSource::Podman
    }

    async fn fetch_with_progress(
        &self,
        image_ref: &str,
        progress: ProgressSender,
    ) -> Result<Image> {
        let image_name = Self::normalize_image_ref(image_ref)?;

        status(&progress, format!("Checking podman image {}", image_name)).await;
        if !Self::image_exists(image_name).await {
            status(&progress, format!("Pulling {} with podman", image_name)).await;
            Self::pull_image(image_name).await?;
        }

        status(&progress, format!("Exporting {} with podman", image_name)).await;
        let output = Self::save_image(image_name).await?;

        status(&progress, "Parsing image...".to_string()).await;
        let mut image = parse_docker_save_tar(Cursor::new(output))?;
        image.reference = image_ref.to_string();
        Ok(image)
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
impl PodmanResolver {
    async fn image_exists(name: &str) -> bool {
        Command::new("podman")
            .arg("image")
            .arg("exists")
            .arg(name)
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }

    async fn pull_image(name: &str) -> Result<()> {
        let status = Command::new("podman")
            .arg("pull")
            .arg(name)
            .status()
            .await
            .with_context(|| format!("failed to run podman pull {}", name))?;
        if !status.success() {
            bail!("podman pull failed for {}", name);
        }
        Ok(())
    }

    async fn save_image(name: &str) -> Result<Vec<u8>> {
        let output = Command::new("podman")
            .arg("image")
            .arg("save")
            .arg(name)
            .output()
            .await
            .with_context(|| format!("failed to run podman image save {}", name))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("podman image save failed: {}", stderr);
        }
        Ok(output.stdout)
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
#[async_trait]
impl Resolver for PodmanResolver {
    async fn fetch(&self, image_ref: &str) -> Result<Image> {
        let _ = Self::normalize_image_ref(image_ref)?;
        bail!("Podman resolver is not supported on this platform")
    }

    fn source_type(&self) -> ImageSource {
        ImageSource::Podman
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn podman_available() -> bool {
        std::process::Command::new("podman")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    #[test]
    fn test_normalize_image_ref() {
        assert_eq!(
            PodmanResolver::normalize_image_ref("podman://alpine:latest").unwrap(),
            "alpine:latest"
        );
    }

    #[test]
    fn test_normalize_image_ref_invalid_scheme() {
        assert!(PodmanResolver::normalize_image_ref("docker://alpine:latest").is_err());
    }

    #[test]
    fn test_normalize_image_ref_empty() {
        assert!(PodmanResolver::normalize_image_ref("podman://").is_err());
    }

    #[tokio::test]
    async fn test_fetch_alpine_conditional() {
        if !podman_available() {
            eprintln!("Podman not available, skipping test_fetch_alpine_conditional");
            return;
        }

        let resolver = PodmanResolver::new();
        let image = resolver.fetch("podman://alpine:latest").await.unwrap();
        assert!(!image.layers.is_empty());
        assert_eq!(image.reference, "podman://alpine:latest");
    }
}
