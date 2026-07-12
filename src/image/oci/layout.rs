#![allow(dead_code)]

use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;

use crate::analysis::archive::parse_oci_layout;
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

        let mut image = parse_oci_layout(&path)?;
        image.reference = image_ref.to_string();

        Ok(image)
    }

    fn source_type(&self) -> ImageSource {
        ImageSource::Oci
    }
}
