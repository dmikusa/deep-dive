#![allow(dead_code)]

use std::collections::HashMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::analysis::filetree::{FileInfo, FileTree, TarEntryType};
use crate::image::{Image, Layer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionType {
    Gzip,
    Zstd,
    Uncompressed,
}

#[derive(Debug, Clone)]
pub struct TarEntry {
    pub path: String,
    pub size: u64,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub entry_type: TarEntryType,
    pub linkname: String,
    pub content_hash: u64,
}

#[derive(Deserialize)]
struct DockerManifest {
    #[serde(rename = "Config")]
    config: String,
    #[serde(rename = "RepoTags")]
    repo_tags: Vec<String>,
    #[serde(rename = "Layers")]
    layers: Vec<String>,
}

#[derive(Deserialize)]
struct HistoryEntry {
    created_by: String,
    #[serde(default)]
    empty_layer: Option<bool>,
}

#[derive(Deserialize)]
struct ImageConfig {
    history: Vec<HistoryEntry>,
}

#[derive(Deserialize)]
struct OciLayout {
    #[serde(rename = "imageLayoutVersion")]
    image_layout_version: String,
}

#[derive(Deserialize)]
struct OciIndex {
    manifests: Vec<OciManifestDescriptor>,
}

#[derive(Deserialize)]
struct OciManifestDescriptor {
    digest: String,
}

#[derive(Deserialize)]
struct OciManifest {
    config: OciDescriptor,
    layers: Vec<OciDescriptor>,
}

#[derive(Deserialize)]
struct OciDescriptor {
    digest: String,
}

pub fn parse_docker_save_tar(reader: impl Read) -> Result<Image> {
    let mut archive = tar::Archive::new(reader);
    let mut entries: HashMap<String, Vec<u8>> = HashMap::new();

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_string_lossy().into_owned();
        let mut data = Vec::new();
        entry.read_to_end(&mut data)?;
        entries.insert(path, data);
    }

    let manifest_data = entries
        .get("manifest.json")
        .context("manifest.json not found in archive")?;
    let manifests: Vec<DockerManifest> =
        serde_json::from_slice(manifest_data).context("failed to parse manifest.json")?;
    let manifest = manifests
        .first()
        .context("no manifest found in manifest.json")?;

    let config_data = entries
        .get(&manifest.config)
        .with_context(|| format!("config file {} not found", manifest.config))?;
    let config: ImageConfig =
        serde_json::from_slice(config_data).context("failed to parse image config")?;

    let non_empty_history: Vec<&HistoryEntry> = config
        .history
        .iter()
        .filter(|h| !h.empty_layer.unwrap_or(false))
        .collect();

    let mut layers = Vec::new();
    for (i, layer_path) in manifest.layers.iter().enumerate() {
        let layer_data = entries
            .get(layer_path)
            .with_context(|| format!("layer {} not found", layer_path))?;

        let decompressed = decompress_to_vec(layer_data)?;
        let tar_entries = parse_tar_entries(decompressed.as_slice())?;

        let mut tree = FileTree::new();
        let mut size = 0u64;
        for te in &tar_entries {
            let info = FileInfo {
                size: te.size,
                mode: te.mode,
                uid: te.uid,
                gid: te.gid,
                entry_type: te.entry_type,
                linkname: te.linkname.clone(),
                content_hash: te.content_hash,
            };
            tree.add_path(&te.path, info);
            if te.entry_type == TarEntryType::Regular {
                size += te.size;
            }
        }

        let command = non_empty_history
            .get(i)
            .map(|h| strip_command_prefix(&h.created_by))
            .unwrap_or_default();

        layers.push(Layer {
            index: i,
            command,
            size,
            tree,
        });
    }

    let reference = manifest.repo_tags.first().cloned().unwrap_or_default();

    Ok(Image { reference, layers })
}

pub fn parse_oci_layout(path: &Path) -> Result<Image> {
    let oci_layout_data =
        fs::read_to_string(path.join("oci-layout")).context("failed to read oci-layout")?;
    let oci_layout: OciLayout =
        serde_json::from_str(&oci_layout_data).context("failed to parse oci-layout")?;
    if oci_layout.image_layout_version != "1.0.0" {
        bail!(
            "unsupported OCI layout version: {}",
            oci_layout.image_layout_version
        );
    }

    let index_data =
        fs::read_to_string(path.join("index.json")).context("failed to read index.json")?;
    let index: OciIndex =
        serde_json::from_str(&index_data).context("failed to parse index.json")?;
    let manifest_desc = index
        .manifests
        .first()
        .context("no manifests found in index.json")?;

    let manifest_hash = digest_to_hex(&manifest_desc.digest)?;
    let manifest_data = fs::read(path.join("blobs/sha256").join(&manifest_hash))
        .context("failed to read manifest blob")?;
    let manifest: OciManifest =
        serde_json::from_slice(&manifest_data).context("failed to parse manifest blob")?;

    let config_hash = digest_to_hex(&manifest.config.digest)?;
    let config_data = fs::read(path.join("blobs/sha256").join(&config_hash))
        .context("failed to read config blob")?;
    let config: ImageConfig =
        serde_json::from_slice(&config_data).context("failed to parse config blob")?;

    let non_empty_history: Vec<&HistoryEntry> = config
        .history
        .iter()
        .filter(|h| !h.empty_layer.unwrap_or(false))
        .collect();

    let mut layers = Vec::new();
    for (i, layer_desc) in manifest.layers.iter().enumerate() {
        let layer_hash = digest_to_hex(&layer_desc.digest)?;
        let layer_data = fs::read(path.join("blobs/sha256").join(&layer_hash))
            .with_context(|| format!("failed to read layer blob {}", layer_desc.digest))?;

        let decompressed = decompress_to_vec(&layer_data)?;
        let tar_entries = parse_tar_entries(decompressed.as_slice())?;

        let mut tree = FileTree::new();
        let mut size = 0u64;
        for te in &tar_entries {
            let info = FileInfo {
                size: te.size,
                mode: te.mode,
                uid: te.uid,
                gid: te.gid,
                entry_type: te.entry_type,
                linkname: te.linkname.clone(),
                content_hash: te.content_hash,
            };
            tree.add_path(&te.path, info);
            if te.entry_type == TarEntryType::Regular {
                size += te.size;
            }
        }

        let command = non_empty_history
            .get(i)
            .map(|h| strip_command_prefix(&h.created_by))
            .unwrap_or_default();

        layers.push(Layer {
            index: i,
            command,
            size,
            tree,
        });
    }

    Ok(Image {
        reference: String::new(),
        layers,
    })
}

pub fn detect_compression(data: &[u8]) -> CompressionType {
    if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
        CompressionType::Gzip
    } else if data.len() >= 4
        && data[0] == 0x28
        && data[1] == 0xb5
        && data[2] == 0x2f
        && data[3] == 0xfd
    {
        CompressionType::Zstd
    } else {
        CompressionType::Uncompressed
    }
}

pub fn decompress_layer(mut reader: impl Read + 'static) -> Result<Box<dyn Read>> {
    let mut header = vec![0u8; 4];
    let n = reader.read(&mut header)?;
    header.truncate(n);
    let compression = detect_compression(&header);
    let chained: Box<dyn Read> = Box::new(Cursor::new(header).chain(reader));

    match compression {
        CompressionType::Gzip => Ok(Box::new(flate2::read::GzDecoder::new(chained))),
        CompressionType::Zstd => Ok(Box::new(zstd::stream::read::Decoder::new(chained)?)),
        CompressionType::Uncompressed => Ok(chained),
    }
}

pub fn parse_tar_entries(reader: impl Read) -> Result<Vec<TarEntry>> {
    let mut archive = tar::Archive::new(reader);
    let mut entries = Vec::new();

    for entry in archive.entries()? {
        let mut entry = entry?;
        let raw_path = entry.path()?.to_string_lossy().into_owned();
        let path = normalize_path(&raw_path);
        if path.is_empty() {
            continue;
        }

        let header = entry.header();
        let entry_type = match header.entry_type() {
            tar::EntryType::Regular | tar::EntryType::Continuous => TarEntryType::Regular,
            tar::EntryType::Directory => TarEntryType::Directory,
            tar::EntryType::Symlink => TarEntryType::Symlink,
            tar::EntryType::Link => TarEntryType::Hardlink,
            _ => TarEntryType::Other,
        };

        let size = header.size()?;
        let mode = header.mode()?;
        let uid = header.uid()? as u32;
        let gid = header.gid()? as u32;

        let linkname = header
            .link_name()?
            .map(|l| l.to_string_lossy().into_owned())
            .unwrap_or_default();

        let mut content_hash = 0u64;
        if entry_type == TarEntryType::Regular {
            let mut data = Vec::new();
            entry.read_to_end(&mut data)?;
            content_hash = compute_content_hash(&data);
        }

        entries.push(TarEntry {
            path,
            size,
            mode,
            uid,
            gid,
            entry_type,
            linkname,
            content_hash,
        });
    }

    Ok(entries)
}

pub fn strip_command_prefix(cmd: &str) -> String {
    if let Some(stripped) = cmd.strip_prefix("/bin/sh -c #(nop) ") {
        stripped.trim().to_string()
    } else if let Some(stripped) = cmd.strip_prefix("/bin/sh -c ") {
        stripped.trim().to_string()
    } else {
        cmd.to_string()
    }
}

pub fn compute_content_hash(data: &[u8]) -> u64 {
    xxhash_rust::xxh64::xxh64(data, 0)
}

fn decompress_to_vec(data: &[u8]) -> Result<Vec<u8>> {
    match detect_compression(data) {
        CompressionType::Gzip => {
            let mut decoder = flate2::read::GzDecoder::new(data);
            let mut out = Vec::new();
            decoder.read_to_end(&mut out)?;
            Ok(out)
        }
        CompressionType::Zstd => {
            let mut decoder = zstd::stream::read::Decoder::new(data)?;
            let mut out = Vec::new();
            decoder.read_to_end(&mut out)?;
            Ok(out)
        }
        CompressionType::Uncompressed => Ok(data.to_vec()),
    }
}

fn normalize_path(path: &str) -> String {
    let p = path.trim_start_matches("./");
    p.trim_start_matches('/').to_string()
}

fn digest_to_hex(digest: &str) -> Result<String> {
    digest
        .strip_prefix("sha256:")
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("invalid digest format: {}", digest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_detect_compression_gzip() {
        assert_eq!(
            detect_compression(&[0x1f, 0x8b, 0x08, 0x00]),
            CompressionType::Gzip
        );
    }

    #[test]
    fn test_detect_compression_zstd() {
        assert_eq!(
            detect_compression(&[0x28, 0xb5, 0x2f, 0xfd]),
            CompressionType::Zstd
        );
    }

    #[test]
    fn test_detect_compression_uncompressed() {
        assert_eq!(
            detect_compression(&[0x62, 0x69, 0x6e, 0x2f]),
            CompressionType::Uncompressed
        );
    }

    #[test]
    fn test_detect_compression_empty() {
        assert_eq!(detect_compression(&[]), CompressionType::Uncompressed);
    }

    #[test]
    fn test_strip_command_prefix_nop() {
        assert_eq!(
            strip_command_prefix("/bin/sh -c #(nop) ADD file:abc123 in /"),
            "ADD file:abc123 in /"
        );
    }

    #[test]
    fn test_strip_command_prefix_shell() {
        assert_eq!(
            strip_command_prefix("/bin/sh -c mkdir -p /root/example"),
            "mkdir -p /root/example"
        );
    }

    #[test]
    fn test_strip_command_prefix_no_prefix() {
        assert_eq!(
            strip_command_prefix("COPY README.md /README.md"),
            "COPY README.md /README.md"
        );
    }

    #[test]
    fn test_compute_content_hash() {
        let hash1 = compute_content_hash(b"hello world");
        let hash2 = compute_content_hash(b"hello world");
        let hash3 = compute_content_hash(b"different content");
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_normalize_path() {
        assert_eq!(normalize_path("./bin/bash"), "bin/bash");
        assert_eq!(normalize_path("/bin/bash"), "bin/bash");
        assert_eq!(normalize_path("./usr/local/bin"), "usr/local/bin");
        assert_eq!(normalize_path("bin/bash"), "bin/bash");
    }

    #[test]
    fn test_digest_to_hex() {
        assert_eq!(digest_to_hex("sha256:abc123").unwrap(), "abc123");
        assert!(digest_to_hex("invalid").is_err());
    }

    #[test]
    fn test_parse_docker_save_tar() {
        let file = File::open("tests/fixtures/test-docker-image.tar").unwrap();
        let image = parse_docker_save_tar(file).unwrap();

        assert_eq!(image.reference, "dive-test:latest");
        assert_eq!(image.layers.len(), 14);

        assert!(image.layers[0].command.contains("ADD file:"));
        assert_eq!(image.layers[0].index, 0);

        for layer in &image.layers {
            assert!(layer.tree.root.children.len() > 0 || layer.size == 0);
        }
    }

    #[test]
    fn test_parse_docker_save_tar_layer_contents() {
        let file = File::open("tests/fixtures/test-docker-image.tar").unwrap();
        let image = parse_docker_save_tar(file).unwrap();

        let first_layer = &image.layers[0];
        assert!(first_layer.tree.get_node("bin").is_some());
    }

    #[test]
    fn test_parse_oci_layout_gzip() {
        let dir = extract_oci_fixture("test-oci-gzip-image.tar");
        let image = parse_oci_layout(dir.path()).unwrap();

        assert_eq!(image.layers.len(), 1);
        assert!(!image.layers[0].command.is_empty());
        assert!(image.layers[0].tree.root.children.len() > 0);
    }

    #[test]
    fn test_parse_oci_layout_zstd() {
        let dir = extract_oci_fixture("test-oci-zstd-image.tar");
        let image = parse_oci_layout(dir.path()).unwrap();

        assert_eq!(image.layers.len(), 1);
        assert!(!image.layers[0].command.is_empty());
        assert!(image.layers[0].tree.root.children.len() > 0);
    }

    #[test]
    fn test_decompress_layer_gzip() {
        let original = b"hello world this is a test";
        let mut compressed = Vec::new();
        {
            let mut encoder =
                flate2::write::GzEncoder::new(&mut compressed, flate2::Compression::default());
            encoder.write_all(original).unwrap();
        }

        let mut decompressed = Vec::new();
        let mut reader = decompress_layer(Cursor::new(compressed)).unwrap();
        reader.read_to_end(&mut decompressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_decompress_layer_uncompressed() {
        let data = b"hello world uncompressed".to_vec();
        let mut output = Vec::new();
        let mut reader = decompress_layer(Cursor::new(data)).unwrap();
        reader.read_to_end(&mut output).unwrap();
        assert_eq!(output, b"hello world uncompressed");
    }

    fn extract_oci_fixture(tar_name: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let file = File::open(format!("tests/fixtures/{}", tar_name)).unwrap();
        let mut archive = tar::Archive::new(file);
        archive.unpack(dir.path()).unwrap();
        dir
    }
}
