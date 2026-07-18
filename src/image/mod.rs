#![allow(dead_code)]

pub mod docker;
pub mod oci;
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
