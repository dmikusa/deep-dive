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
