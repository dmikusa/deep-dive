#![allow(dead_code)]

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortMode {
    #[default]
    Name,
    Size,
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
    /// Raw file contents, populated for regular files during archive parsing.
    /// Used by the TUI extract feature.
    pub content: Vec<u8>,
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
            content: Vec::new(),
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
    sort_mode: SortMode,
}

/// A rendered line of the file tree, including its diff type for styling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedLine {
    pub text: String,
    pub diff_type: DiffType,
    pub path: String,
}

impl FileTree {
    pub fn new() -> Self {
        Self {
            root: FileNode::new(PathBuf::from("/"), FileInfo::default()),
            sort_mode: SortMode::default(),
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

    pub fn get_node_mut(&mut self, path: &str) -> Option<&mut FileNode> {
        let path = path.trim_start_matches('/');
        if path.is_empty() {
            return Some(&mut self.root);
        }

        let parts: Vec<&str> = path.split('/').collect();
        let mut current = &mut self.root;

        for part in parts {
            match current.children.get_mut(part) {
                Some(node) => current = node,
                None => return None,
            }
        }

        Some(current)
    }

    /// Merge `upper` onto `self`, returning a new tree. Whiteout files in
    /// `upper` cause removals in the lower tree.
    pub fn stack(&self, upper: &FileTree) -> FileTree {
        let mut result = self.clone();
        Self::apply_children(&mut result.root, &upper.root.children);
        result
    }

    fn apply_children(parent: &mut FileNode, upper_children: &BTreeMap<String, FileNode>) {
        // First pass: apply whiteout markers so that removals happen before
        // regular entries are merged.
        for name in upper_children.keys() {
            if name == ".wh..wh..opq" {
                parent.children.clear();
            } else if let Some(target) = name.strip_prefix(".wh.") {
                parent.children.remove(target);
            }
        }

        // Second pass: upsert regular nodes.
        for (name, upper_child) in upper_children {
            if name.starts_with(".wh.") {
                continue;
            }

            if let Some(existing) = parent.children.get_mut(name) {
                existing.info = upper_child.info.clone();
                existing.diff_type = upper_child.diff_type;
                existing.collapsed = upper_child.collapsed;

                if upper_child.children.is_empty() {
                    // Upper is a leaf: it replaces any lower directory.
                    existing.children.clear();
                } else {
                    // Upper is a directory: recursively merge children.
                    Self::apply_children(existing, &upper_child.children);
                }
            } else {
                parent.children.insert(name.clone(), upper_child.clone());
            }
        }
    }

    /// Mark nodes in `self` by comparing against `reference`. The result
    /// contains the union of both trees:
    /// - nodes only in `self` are marked `Added`
    /// - nodes only in `reference` are inserted and marked `Removed`
    /// - nodes in both with differing content or metadata are `Modified`
    /// - otherwise `Unmodified`
    pub fn compare_and_mark(&mut self, reference: &FileTree) {
        Self::compare_node(&mut self.root, &reference.root);
    }

    fn compare_node(node: &mut FileNode, reference: &FileNode) {
        // Determine the fate of children already present in self.
        for (name, child) in node.children.iter_mut() {
            match reference.children.get(name) {
                Some(ref_child) => {
                    let same = child.info.entry_type == ref_child.info.entry_type
                        && child.info.content_hash == ref_child.info.content_hash
                        && child.info.size == ref_child.info.size
                        && child.info.mode == ref_child.info.mode
                        && child.info.uid == ref_child.info.uid
                        && child.info.gid == ref_child.info.gid;

                    child.diff_type = if same {
                        DiffType::Unmodified
                    } else {
                        DiffType::Modified
                    };

                    Self::compare_node(child, ref_child);
                }
                None => {
                    child.diff_type = DiffType::Added;
                    Self::mark_subtree(child, DiffType::Added);
                }
            }
        }

        // Insert reference children that do not exist in self, marking them
        // and their descendants as Removed.
        for (name, ref_child) in reference.children.iter() {
            if !node.children.contains_key(name) {
                let mut removed = ref_child.clone();
                removed.diff_type = DiffType::Removed;
                Self::mark_subtree(&mut removed, DiffType::Removed);
                node.children.insert(name.clone(), removed);
            }
        }
    }

    fn mark_subtree(node: &mut FileNode, diff_type: DiffType) {
        node.diff_type = diff_type;
        for child in node.children.values_mut() {
            Self::mark_subtree(child, diff_type);
        }
    }

    /// Render the visible tree with diff-type metadata, returning rows in the
    /// half-open range `[start_row, stop_row)`.
    pub fn render_tree(&self, start_row: usize, stop_row: usize) -> Vec<RenderedLine> {
        self.render_tree_filtered(start_row, stop_row, &HashSet::new(), None, false)
    }

    /// Render the visible tree as plain strings.
    pub fn render_string_tree(&self, start_row: usize, stop_row: usize) -> Vec<String> {
        self.render_tree(start_row, stop_row)
            .into_iter()
            .map(|line| line.text)
            .collect()
    }

    /// Render the tree with visibility filtering and optional attributes.
    pub fn render_tree_filtered(
        &self,
        start_row: usize,
        stop_row: usize,
        hidden_diff_types: &std::collections::HashSet<DiffType>,
        filter: Option<&regex::Regex>,
        show_attributes: bool,
    ) -> Vec<RenderedLine> {
        let mut lines = Vec::new();
        Self::render_node(
            &self.root,
            "",
            true,
            &mut lines,
            self.sort_mode,
            hidden_diff_types,
            filter,
            show_attributes,
        );
        lines
            .into_iter()
            .skip(start_row)
            .take(stop_row.saturating_sub(start_row))
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_node(
        node: &FileNode,
        prefix: &str,
        is_last: bool,
        lines: &mut Vec<RenderedLine>,
        sort_mode: SortMode,
        hidden_diff_types: &std::collections::HashSet<DiffType>,
        filter: Option<&regex::Regex>,
        show_attributes: bool,
    ) {
        let is_root = node.path.as_os_str() == "/";
        if !is_root && !Self::is_visible(node, hidden_diff_types, filter) {
            // Still descend into children if not collapsed, because a child may
            // match even when this node does not.
            if node.collapsed {
                return;
            }
            let children = Self::sorted_children(&node.children, sort_mode);
            for child in children.iter() {
                Self::render_node(
                    child,
                    prefix,
                    false,
                    lines,
                    sort_mode,
                    hidden_diff_types,
                    filter,
                    show_attributes,
                );
            }
            return;
        }

        if !is_root {
            let branch = if is_last { "└── " } else { "├── " };
            let name = node
                .path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "/".to_string());
            let mut text = format!("{}{}{}", prefix, branch, name);
            if show_attributes {
                text.push_str(&Self::format_attributes(&node.info));
            }
            lines.push(RenderedLine {
                text,
                diff_type: node.diff_type,
                path: node.path.to_string_lossy().into_owned(),
            });
        }

        if node.collapsed {
            return;
        }

        let children = Self::sorted_children(&node.children, sort_mode);
        let child_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });

        for (i, child) in children.iter().enumerate() {
            let is_last_child = i == children.len() - 1;
            Self::render_node(
                child,
                &child_prefix,
                is_last_child,
                lines,
                sort_mode,
                hidden_diff_types,
                filter,
                show_attributes,
            );
        }
    }

    fn format_attributes(info: &FileInfo) -> String {
        let perms = Self::format_mode(info.mode, info.entry_type);
        format!("  {} {}:{} {}", perms, info.uid, info.gid, info.size)
    }

    fn format_mode(mode: u32, entry_type: TarEntryType) -> String {
        let type_char = match entry_type {
            TarEntryType::Directory => 'd',
            TarEntryType::Symlink => 'l',
            TarEntryType::Hardlink => 'h',
            _ => '-',
        };
        let perms = (0..9).map(|i| {
            let bit = 8 - i;
            let set = (mode >> bit) & 1 == 1;
            match i % 3 {
                0 => {
                    if set {
                        'r'
                    } else {
                        '-'
                    }
                }
                1 => {
                    if set {
                        'w'
                    } else {
                        '-'
                    }
                }
                _ => {
                    if set {
                        'x'
                    } else {
                        '-'
                    }
                }
            }
        });
        std::iter::once(type_char).chain(perms).collect()
    }

    fn sorted_children(
        children: &BTreeMap<String, FileNode>,
        sort_mode: SortMode,
    ) -> Vec<&FileNode> {
        match sort_mode {
            SortMode::Name => children.values().collect(),
            SortMode::Size => {
                let mut v: Vec<&FileNode> = children.values().collect();
                v.sort_by_key(|b| std::cmp::Reverse(b.info.size));
                v
            }
        }
    }

    pub fn collapse(&mut self, path: &str) -> bool {
        if let Some(node) = self.get_node_mut(path) {
            node.collapsed = true;
            true
        } else {
            false
        }
    }

    pub fn expand(&mut self, path: &str) -> bool {
        if let Some(node) = self.get_node_mut(path) {
            node.collapsed = false;
            true
        } else {
            false
        }
    }

    pub fn collapse_all(&mut self) {
        Self::set_collapsed_recursive(&mut self.root, true);
    }

    pub fn expand_all(&mut self) {
        Self::set_collapsed_recursive(&mut self.root, false);
    }

    /// Return the paths of all directory nodes in the tree.
    pub fn directory_paths(&self) -> Vec<String> {
        let mut paths = Vec::new();
        Self::collect_directory_paths(&self.root, &mut paths);
        paths
    }

    fn collect_directory_paths(node: &FileNode, paths: &mut Vec<String>) {
        if node.path.as_os_str() != "/" && !node.children.is_empty() {
            paths.push(node.path.to_string_lossy().into_owned());
        }
        for child in node.children.values() {
            Self::collect_directory_paths(child, paths);
        }
    }

    fn set_collapsed_recursive(node: &mut FileNode, collapsed: bool) {
        if node.path.as_os_str() != "/" {
            node.collapsed = collapsed;
        }
        for child in node.children.values_mut() {
            Self::set_collapsed_recursive(child, collapsed);
        }
    }

    pub fn set_sort_mode(&mut self, mode: SortMode) {
        self.sort_mode = mode;
    }

    pub fn sort_mode(&self) -> SortMode {
        self.sort_mode
    }

    /// Mark every node in the tree with the given diff type.
    pub fn mark_all(&mut self, diff_type: DiffType) {
        Self::mark_subtree(&mut self.root, diff_type);
    }

    /// Return the paths of all visible (non-collapsed) nodes in render order,
    /// excluding the root node.
    pub fn visible_paths(&self) -> Vec<String> {
        self.visible_paths_filtered(&HashSet::new(), None)
    }

    /// Return the paths of all visible nodes after applying diff-type and
    /// regex filters.
    pub fn visible_paths_filtered(
        &self,
        hidden_diff_types: &std::collections::HashSet<DiffType>,
        filter: Option<&regex::Regex>,
    ) -> Vec<String> {
        let mut paths = Vec::new();
        Self::collect_visible_paths(
            &self.root,
            &mut paths,
            self.sort_mode,
            hidden_diff_types,
            filter,
        );
        paths
    }

    fn collect_visible_paths(
        node: &FileNode,
        paths: &mut Vec<String>,
        sort_mode: SortMode,
        hidden_diff_types: &std::collections::HashSet<DiffType>,
        filter: Option<&regex::Regex>,
    ) {
        let is_root = node.path.as_os_str() == "/";
        if !is_root && Self::is_visible(node, hidden_diff_types, filter) {
            paths.push(node.path.to_string_lossy().into_owned());
        }
        if node.collapsed {
            return;
        }
        for child in Self::sorted_children(&node.children, sort_mode) {
            Self::collect_visible_paths(child, paths, sort_mode, hidden_diff_types, filter);
        }
    }

    /// A node is visible if it is not hidden by diff type and either matches
    /// the filter or has a descendant that matches. Hidden ancestors are kept
    /// if they have visible descendants so the tree structure is preserved.
    fn is_visible(
        node: &FileNode,
        hidden_diff_types: &std::collections::HashSet<DiffType>,
        filter: Option<&regex::Regex>,
    ) -> bool {
        let diff_visible = !hidden_diff_types.contains(&node.diff_type);
        let matches_filter = filter.map_or(true, |re| re.is_match(&node.path.to_string_lossy()));
        let has_visible_child = node
            .children
            .values()
            .any(|c| Self::is_visible(c, hidden_diff_types, filter));

        has_visible_child || (diff_visible && matches_filter)
    }
}

impl Default for FileTree {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_info(size: u64, content_hash: u64) -> FileInfo {
        FileInfo {
            size,
            content_hash,
            entry_type: TarEntryType::Regular,
            ..Default::default()
        }
    }

    fn dir_info() -> FileInfo {
        FileInfo {
            entry_type: TarEntryType::Directory,
            ..Default::default()
        }
    }

    #[test]
    fn test_add_path_creates_directories() {
        let mut tree = FileTree::new();
        tree.add_path("usr/bin/bash", file_info(100, 1));

        assert!(tree.get_node("usr").is_some());
        assert!(tree.get_node("usr/bin").is_some());
        assert!(tree.get_node("usr/bin/bash").is_some());
        assert_eq!(tree.get_node("usr/bin/bash").unwrap().info.size, 100);
    }

    #[test]
    fn test_stack_simple_addition() {
        let mut lower = FileTree::new();
        lower.add_path("bin/bash", file_info(100, 1));

        let mut upper = FileTree::new();
        upper.add_path("bin/ls", file_info(50, 2));

        let stacked = lower.stack(&upper);
        assert!(stacked.get_node("bin/bash").is_some());
        assert!(stacked.get_node("bin/ls").is_some());
    }

    #[test]
    fn test_stack_overwrite_file() {
        let mut lower = FileTree::new();
        lower.add_path("etc/config", file_info(100, 1));

        let mut upper = FileTree::new();
        upper.add_path("etc/config", file_info(120, 2));

        let stacked = lower.stack(&upper);
        assert_eq!(stacked.get_node("etc/config").unwrap().info.size, 120);
    }

    #[test]
    fn test_stack_whiteout_file() {
        let mut lower = FileTree::new();
        lower.add_path("etc/config", file_info(100, 1));
        lower.add_path("etc/other", file_info(50, 2));

        let mut upper = FileTree::new();
        upper.add_path("etc/.wh.config", file_info(0, 0));

        let stacked = lower.stack(&upper);
        assert!(stacked.get_node("etc/config").is_none());
        assert!(stacked.get_node("etc/other").is_some());
        assert!(stacked.get_node("etc/.wh.config").is_none());
    }

    #[test]
    fn test_stack_opaque_whiteout() {
        let mut lower = FileTree::new();
        lower.add_path("var/cache/a", file_info(10, 1));
        lower.add_path("var/cache/b", file_info(20, 2));
        lower.add_path("var/log/msg", file_info(30, 3));

        let mut upper = FileTree::new();
        upper.add_path("var/cache/.wh..wh..opq", file_info(0, 0));
        upper.add_path("var/cache/new", file_info(40, 4));

        let stacked = lower.stack(&upper);
        assert!(stacked.get_node("var/cache/a").is_none());
        assert!(stacked.get_node("var/cache/b").is_none());
        assert!(stacked.get_node("var/cache/new").is_some());
        assert!(stacked.get_node("var/log/msg").is_some());
        assert!(stacked.get_node("var/cache/.wh..wh..opq").is_none());
    }

    #[test]
    fn test_compare_and_mark_added() {
        let mut merged = FileTree::new();
        merged.add_path("bin/new", file_info(10, 1));

        let reference = FileTree::new();
        merged.compare_and_mark(&reference);

        assert_eq!(
            merged.get_node("bin/new").unwrap().diff_type,
            DiffType::Added
        );
    }

    #[test]
    fn test_compare_and_mark_removed() {
        let mut merged = FileTree::new();

        let mut reference = FileTree::new();
        reference.add_path("bin/old", file_info(10, 1));

        merged.compare_and_mark(&reference);

        let removed = merged.get_node("bin/old").unwrap();
        assert_eq!(removed.diff_type, DiffType::Removed);
    }

    #[test]
    fn test_compare_and_mark_modified() {
        let mut merged = FileTree::new();
        merged.add_path("etc/config", file_info(120, 2));

        let mut reference = FileTree::new();
        reference.add_path("etc/config", file_info(100, 1));

        merged.compare_and_mark(&reference);

        assert_eq!(
            merged.get_node("etc/config").unwrap().diff_type,
            DiffType::Modified
        );
    }

    #[test]
    fn test_compare_and_mark_unmodified() {
        let mut merged = FileTree::new();
        merged.add_path("etc/config", file_info(100, 1));

        let mut reference = FileTree::new();
        reference.add_path("etc/config", file_info(100, 1));

        merged.compare_and_mark(&reference);

        assert_eq!(
            merged.get_node("etc/config").unwrap().diff_type,
            DiffType::Unmodified
        );
    }

    #[test]
    fn test_render_string_tree() {
        let mut tree = FileTree::new();
        tree.add_path("bin/bash", file_info(100, 1));
        tree.add_path("bin/ls", file_info(50, 2));
        tree.add_path("etc/passwd", file_info(10, 3));

        let lines = tree.render_string_tree(0, 100);
        assert!(lines.iter().any(|l| l.contains("bin")));
        assert!(lines.iter().any(|l| l.contains("bash")));
        assert!(lines.iter().any(|l| l.contains("ls")));
        assert!(lines.iter().any(|l| l.contains("etc")));
        assert!(lines.iter().any(|l| l.contains("passwd")));
    }

    #[test]
    fn test_render_string_tree_viewport() {
        let mut tree = FileTree::new();
        tree.add_path("a", file_info(1, 1));
        tree.add_path("b", file_info(1, 2));
        tree.add_path("c", file_info(1, 3));

        let lines = tree.render_string_tree(1, 3);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_collapse_hides_children() {
        let mut tree = FileTree::new();
        tree.add_path("dir/a", file_info(1, 1));
        tree.add_path("dir/b", file_info(1, 2));

        tree.collapse("dir");
        let collapsed_lines = tree.render_string_tree(0, 100);
        assert_eq!(collapsed_lines.len(), 1);
        assert!(collapsed_lines[0].contains("dir"));

        tree.expand("dir");
        let expanded_lines = tree.render_string_tree(0, 100);
        assert_eq!(expanded_lines.len(), 3);
    }

    #[test]
    fn test_sort_mode_size() {
        let mut tree = FileTree::new();
        tree.add_path("dir/small", file_info(10, 1));
        tree.add_path("dir/large", file_info(100, 2));
        tree.add_path("dir/medium", file_info(50, 3));

        tree.set_sort_mode(SortMode::Size);
        let lines = tree.render_string_tree(0, 100);
        let large_pos = lines.iter().position(|l| l.contains("large")).unwrap();
        let medium_pos = lines.iter().position(|l| l.contains("medium")).unwrap();
        let small_pos = lines.iter().position(|l| l.contains("small")).unwrap();
        assert!(large_pos < medium_pos);
        assert!(medium_pos < small_pos);
    }

    #[test]
    fn test_collapse_all_expand_all() {
        let mut tree = FileTree::new();
        tree.add_path("dir/a", file_info(1, 1));
        tree.add_path("dir/b", file_info(1, 2));
        tree.add_path("dir/sub/c", file_info(1, 3));

        tree.collapse_all();
        assert!(tree.get_node("dir").unwrap().collapsed);
        assert!(tree.get_node("dir/sub").unwrap().collapsed);
        let lines = tree.render_string_tree(0, 100);
        assert_eq!(lines.len(), 1);

        tree.expand_all();
        assert!(!tree.get_node("dir").unwrap().collapsed);
        assert!(!tree.get_node("dir/sub").unwrap().collapsed);
        let lines = tree.render_string_tree(0, 100);
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn test_directory_paths() {
        let mut tree = FileTree::new();
        tree.add_path("dir/a", file_info(1, 1));
        tree.add_path("dir/sub/b", file_info(1, 2));
        tree.add_path("file", file_info(1, 3));

        let paths = tree.directory_paths();
        assert!(paths.contains(&"dir".to_string()));
        assert!(paths.contains(&"dir/sub".to_string()));
        assert!(!paths.contains(&"file".to_string()));
    }

    #[test]
    fn test_visible_paths_filtered_by_diff_type() {
        let mut tree = FileTree::new();
        tree.add_path("added", file_info(1, 1));
        tree.add_path("unmodified", file_info(1, 2));
        tree.mark_all(DiffType::Added);
        tree.get_node_mut("unmodified").unwrap().diff_type = DiffType::Unmodified;

        let mut hidden = HashSet::new();
        hidden.insert(DiffType::Added);
        let paths = tree.visible_paths_filtered(&hidden, None);
        assert!(paths.contains(&"unmodified".to_string()));
        assert!(!paths.contains(&"added".to_string()));
    }

    #[test]
    fn test_visible_paths_filtered_by_regex() {
        let mut tree = FileTree::new();
        tree.add_path("bin/bash", file_info(1, 1));
        tree.add_path("etc/passwd", file_info(1, 2));

        let re = regex::Regex::new("passwd").unwrap();
        let paths = tree.visible_paths_filtered(&HashSet::new(), Some(&re));
        assert!(paths.contains(&"etc".to_string()));
        assert!(paths.contains(&"etc/passwd".to_string()));
        assert!(!paths.contains(&"bin".to_string()));
        assert!(!paths.contains(&"bin/bash".to_string()));
    }

    #[test]
    fn test_render_tree_filtered_hides_hidden_diff_type() {
        let mut tree = FileTree::new();
        tree.add_path("added", file_info(1, 1));
        tree.mark_all(DiffType::Added);

        let mut hidden = HashSet::new();
        hidden.insert(DiffType::Added);
        let lines = tree.render_tree_filtered(0, 100, &hidden, None, false);
        assert!(lines.is_empty());
    }

    #[test]
    fn test_render_tree_show_attributes() {
        let mut tree = FileTree::new();
        tree.add_path("file", file_info(100, 1));

        let lines = tree.render_tree_filtered(0, 100, &HashSet::new(), None, true);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].text.contains("100"));
    }

    #[test]
    fn test_format_mode() {
        let perms = FileTree::format_mode(0o644, TarEntryType::Regular);
        assert_eq!(perms, "-rw-r--r--");
        let dir_perms = FileTree::format_mode(0o755, TarEntryType::Directory);
        assert_eq!(dir_perms, "drwxr-xr-x");
    }
}
