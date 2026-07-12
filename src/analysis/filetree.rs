#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffType {
    #[default]
    Unmodified,
    Added,
    Removed,
    Modified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TarEntryType {
    Regular,
    Directory,
    Symlink,
    Hardlink,
    Other,
}

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub size: u64,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub entry_type: TarEntryType,
    pub linkname: String,
    pub content_hash: u64,
}

impl Default for FileInfo {
    fn default() -> Self {
        Self {
            size: 0,
            mode: 0,
            uid: 0,
            gid: 0,
            entry_type: TarEntryType::Other,
            linkname: String::new(),
            content_hash: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileNode {
    pub path: PathBuf,
    pub info: FileInfo,
    pub children: BTreeMap<String, FileNode>,
    pub diff_type: DiffType,
    pub collapsed: bool,
}

impl FileNode {
    pub fn new(path: PathBuf, info: FileInfo) -> Self {
        Self {
            path,
            info,
            children: BTreeMap::new(),
            diff_type: DiffType::Unmodified,
            collapsed: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileTree {
    pub root: FileNode,
}

impl FileTree {
    pub fn new() -> Self {
        Self {
            root: FileNode::new(PathBuf::from("/"), FileInfo::default()),
        }
    }

    pub fn add_path(&mut self, path: &str, info: FileInfo) {
        let path = path.trim_start_matches('/');
        if path.is_empty() {
            return;
        }

        let parts: Vec<&str> = path.split('/').collect();
        let mut current = &mut self.root;

        for (i, part) in parts.iter().enumerate() {
            let is_last = i == parts.len() - 1;

            if is_last {
                let node = FileNode::new(PathBuf::from(path), info.clone());
                current.children.insert(part.to_string(), node);
            } else {
                if !current.children.contains_key(*part) {
                    let dir_path = parts[..=i].join("/");
                    let dir_info = FileInfo {
                        entry_type: TarEntryType::Directory,
                        ..Default::default()
                    };
                    let dir_node = FileNode::new(PathBuf::from(dir_path), dir_info);
                    current.children.insert(part.to_string(), dir_node);
                }
                current = current.children.get_mut(*part).unwrap();
            }
        }
    }

    pub fn get_node(&self, path: &str) -> Option<&FileNode> {
        let path = path.trim_start_matches('/');
        if path.is_empty() {
            return Some(&self.root);
        }

        let parts: Vec<&str> = path.split('/').collect();
        let mut current = &self.root;

        for part in parts {
            match current.children.get(part) {
                Some(node) => current = node,
                None => return None,
            }
        }

        Some(current)
    }
}

impl Default for FileTree {
    fn default() -> Self {
        Self::new()
    }
}
