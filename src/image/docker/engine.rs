use std::io::Cursor;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use bollard::query_parameters::CreateImageOptionsBuilder;
use bollard::Docker;
use futures::stream::StreamExt;

use crate::analysis::archive::parse_docker_save_tar;
use crate::image::progress::{bytes, status, ProgressSender};
use crate::image::resolver::{ImageSource, Resolver};
use crate::image::Image;

pub struct DockerEngineResolver {
    client: Docker,
}

impl DockerEngineResolver {
    pub fn new() -> Result<Self> {
        let client = Docker::connect_with_defaults().context(
            "failed to connect to Docker daemon; ensure Docker is running and accessible",
        )?;
        Ok(Self { client })
    }

    pub fn with_client(client: Docker) -> Self {
        Self { client }
    }

    fn normalize_image_ref(image_ref: &str) -> Result<&str> {
        if let Some(name) = image_ref.strip_prefix("docker://") {
            if name.is_empty() {
                bail!("empty docker image reference");
            }
            Ok(name)
        } else if image_ref.contains("://") {
            bail!("expected docker:// URI, got: {}", image_ref)
        } else {
            Ok(image_ref)
        }
    }

    async fn image_exists(
        &self,
        image_name: &str,
    ) -> Result<Option<bollard::models::ImageInspect>> {
        match self.client.inspect_image(image_name).await {
            Ok(inspect) => Ok(Some(inspect)),
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn pull_image(&self, image_name: &str, progress: Option<&ProgressSender>) -> Result<()> {
        let options = CreateImageOptionsBuilder::default()
            .from_image(image_name)
            .build();

        let mut stream = self.client.create_image(Some(options), None, None);
        while let Some(event) = stream.next().await {
            let event = event?;
            if let Some(status_text) = event.status {
                let detail = event
                    .progress_detail
                    .as_ref()
                    .map_or_else(String::new, |d| match (d.current, d.total) {
                        (Some(c), Some(t)) => format!(" ({}/{})", c, t),
                        (Some(c), None) => format!(" ({})", c),
                        _ => String::new(),
                    });
                if let Some(p) = progress {
                    status(
                        p,
                        format!("Pulling {}: {}{}", image_name, status_text, detail),
                    )
                    .await;
                }
            }
            if let (Some(p), Some(detail)) = (progress, event.progress_detail.as_ref()) {
                if let (Some(current), Some(total)) = (detail.current, detail.total) {
                    bytes(p, current.max(0) as u64, Some(total.max(0) as u64)).await;
                }
            }
        }
        Ok(())
    }

    async fn export_image(
        &self,
        image_name: &str,
        progress: Option<&ProgressSender>,
        total_size: Option<u64>,
    ) -> Result<Vec<u8>> {
        let mut stream = self.client.export_image(image_name);

        let mut output = Vec::new();
        let mut chunk_count = 0;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            output.extend_from_slice(&chunk);
            chunk_count += 1;
            if chunk_count % 10 == 0 {
                if let Some(p) = progress {
                    bytes(p, output.len() as u64, total_size).await;
                }
            }
        }
        if let Some(p) = progress {
            bytes(p, output.len() as u64, Some(output.len() as u64)).await;
        }
        Ok(output)
    }
}

impl Default for DockerEngineResolver {
    fn default() -> Self {
        Self::new().expect("Docker should be available in default context")
    }
}

#[async_trait]
impl Resolver for DockerEngineResolver {
    async fn fetch(&self, image_ref: &str) -> Result<Image> {
        let image_name = Self::normalize_image_ref(image_ref)?;

        if self.image_exists(image_name).await?.is_none() {
            self.pull_image(image_name, None).await?;
        }

        let bytes = self.export_image(image_name, None, None).await?;

        let mut image = parse_docker_save_tar(Cursor::new(bytes))?;
        image.reference = image_ref.to_string();
        Ok(image)
    }

    fn source_type(&self) -> ImageSource {
        ImageSource::Docker
    }

    async fn fetch_with_progress(
        &self,
        image_ref: &str,
        progress: ProgressSender,
    ) -> Result<Image> {
        let image_name = Self::normalize_image_ref(image_ref)?;

        status(&progress, format!("Checking {}", image_name)).await;
        if self.image_exists(image_name).await?.is_none() {
            status(&progress, format!("Pulling {}", image_name)).await;
            self.pull_image(image_name, Some(&progress)).await?;
        }

        status(&progress, format!("Exporting {}", image_name)).await;
        let inspect = self
            .image_exists(image_name)
            .await?
            .context("image not found after pull/export")?;
        let total_size = inspect.size.map(|s| s.max(0) as u64);
        let bytes = self
            .export_image(image_name, Some(&progress), total_size)
            .await?;

        status(&progress, "Parsing image...".to_string()).await;
        let mut image = parse_docker_save_tar(Cursor::new(bytes))?;
        image.reference = image_ref.to_string();
        Ok(image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn docker_available() -> bool {
        Docker::connect_with_defaults().is_ok()
    }

    #[test]
    fn test_normalize_image_ref_docker_scheme() {
        assert_eq!(
            DockerEngineResolver::normalize_image_ref("docker://ubuntu:latest").unwrap(),
            "ubuntu:latest"
        );
    }

    #[test]
    fn test_normalize_image_ref_bare() {
        assert_eq!(
            DockerEngineResolver::normalize_image_ref("ubuntu:latest").unwrap(),
            "ubuntu:latest"
        );
    }

    #[test]
    fn test_normalize_image_ref_empty() {
        assert!(DockerEngineResolver::normalize_image_ref("docker://").is_err());
    }

    #[test]
    fn test_normalize_image_ref_wrong_scheme() {
        assert!(DockerEngineResolver::normalize_image_ref("oci://ubuntu").is_err());
    }

    #[tokio::test]
    async fn test_fetch_busybox_conditional() {
        if !docker_available() {
            eprintln!("Docker not available, skipping test_fetch_busybox_conditional");
            return;
        }

        let resolver = DockerEngineResolver::new().unwrap();
        let image = resolver.fetch("docker://busybox:latest").await.unwrap();
        assert!(!image.layers.is_empty());
    }
}
