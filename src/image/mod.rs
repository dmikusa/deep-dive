#![allow(dead_code)]

pub mod docker;
pub mod oci;
pub mod resolver;

use crate::analysis::filetree::FileTree;

pub struct Image {
    pub reference: String,
    pub layers: Vec<Layer>,
}

pub struct Layer {
    pub index: usize,
    pub command: String,
    pub size: u64,
    pub tree: FileTree,
}
