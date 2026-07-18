use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use async_trait::async_trait;
use base64::Engine;
use oci_client::client::ClientConfig;
use oci_client::manifest::{ImageIndexEntry, OciImageManifest};
use oci_client::secrets::RegistryAuth;
use oci_client::{Client, Reference};
use serde::Deserialize;

use crate::analysis::archive::{build_layers, ImageConfig, LayerSource};
use crate::image::resolver::{ImageSource, Resolver};
use crate::image::Image;

pub struct RegistryResolver {
    client: Client,
}

impl Default for RegistryResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl RegistryResolver {
    pub fn new() -> Self {
        Self::with_config(ClientConfig {
            platform_resolver: Some(Box::new(platform_resolver)),
            ..Default::default()
        })
    }

    pub fn with_config(config: ClientConfig) -> Self {
        Self {
            client: Client::new(config),
        }
    }

    pub fn with_client(client: Client) -> Self {
        Self { client }
    }

    fn normalize_image_ref(image_ref: &str) -> Result<Reference> {
        let name = image_ref
            .strip_prefix("registry://")
            .ok_or_else(|| anyhow::anyhow!("Invalid registry URI: {}", image_ref))?;
        if name.is_empty() {
            anyhow::bail!("empty registry image reference");
        }
        name.parse::<Reference>()
            .with_context(|| format!("failed to parse image reference: {}", name))
    }
}

#[async_trait]
impl Resolver for RegistryResolver {
    async fn fetch(&self, image_ref: &str) -> Result<Image> {
        let reference = Self::normalize_image_ref(image_ref)?;
        let auth = registry_auth(&reference).await;

        let (manifest, _digest, config_json) = self
            .client
            .pull_manifest_and_config(&reference, &auth)
            .await
            .with_context(|| format!("failed to pull manifest for {}", reference))?;

        let config: ImageConfig = serde_json::from_str(&config_json)
            .context("failed to parse image config from registry")?;

        let layers = self.pull_layers(&reference, &manifest, &config).await?;

        Ok(Image {
            reference: image_ref.to_string(),
            layers,
        })
    }

    fn source_type(&self) -> ImageSource {
        ImageSource::Registry
    }
}

/// Prefer the native platform, but fall back to `linux/amd64` if the image is
/// not available for the current host (common for images like `alpine` that
/// only publish Linux variants).
fn platform_resolver(entries: &[ImageIndexEntry]) -> Option<String> {
    oci_client::client::current_platform_resolver(entries).or_else(|| {
        entries
            .iter()
            .find(|entry| {
                entry.platform.as_ref().is_some_and(|platform| {
                    platform.os.to_string() == "linux"
                        && platform.architecture.to_string() == "amd64"
                })
            })
            .map(|entry| entry.digest.clone())
    })
}

impl RegistryResolver {
    async fn pull_layers(
        &self,
        reference: &Reference,
        manifest: &OciImageManifest,
        config: &ImageConfig,
    ) -> Result<Vec<crate::image::Layer>> {
        let mut layer_sources = Vec::with_capacity(manifest.layers.len());

        for (i, layer) in manifest.layers.iter().enumerate() {
            let digest = layer.digest.clone();
            let mut data = Vec::new();
            self.client
                .pull_blob(reference, layer.digest.as_str(), &mut data)
                .await
                .with_context(|| {
                    format!("failed to pull layer {} ({}) for {}", i, digest, reference)
                })?;
            layer_sources.push(LayerSource {
                data: Ok(data),
                id: Some(digest.clone()),
                digest: Some(digest),
            });
        }

        build_layers(config, layer_sources.into_iter(), &[])
    }
}

async fn registry_auth(reference: &Reference) -> RegistryAuth {
    let Some(config) = load_docker_config() else {
        return RegistryAuth::Anonymous;
    };
    registry_auth_from_config(&config, reference).await
}

async fn registry_auth_from_config(config: &DockerConfig, reference: &Reference) -> RegistryAuth {
    let registry = reference.resolve_registry();

    // Prefer credential helpers over static credentials.
    if let Some(helper) = config.cred_helpers.get(registry) {
        if let Some((username, password)) = run_credential_helper(registry, helper).await {
            return RegistryAuth::Basic(username, password);
        }
    }

    // Fall back to auths entries.
    let candidates = [
        registry.to_string(),
        format!("https://{}/v1/", registry),
        format!("http://{}/v1/", registry),
    ];

    for key in &candidates {
        if let Some(entry) = config.auths.get(key) {
            if let Some(auth) = &entry.auth {
                if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(auth) {
                    if let Ok(creds) = String::from_utf8(decoded) {
                        if let Some((user, pass)) = creds.split_once(':') {
                            return RegistryAuth::Basic(user.to_string(), pass.to_string());
                        }
                    }
                }
            }
            if let (Some(user), Some(pass)) = (&entry.username, &entry.password) {
                return RegistryAuth::Basic(user.clone(), pass.clone());
            }
        }
    }

    RegistryAuth::Anonymous
}

fn load_docker_config() -> Option<DockerConfig> {
    let path = docker_config_path()?;
    let contents = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn docker_config_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("DOCKER_CONFIG") {
        return Some(PathBuf::from(path).join("config.json"));
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".docker/config.json"))
}

#[derive(Debug, Deserialize)]
struct DockerConfig {
    #[serde(default)]
    auths: HashMap<String, DockerAuthEntry>,
    #[serde(default, rename = "credHelpers")]
    cred_helpers: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct DockerAuthEntry {
    #[serde(default)]
    auth: Option<String>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    password: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CredentialHelperResponse {
    username: String,
    secret: String,
}

async fn run_credential_helper(registry: &str, helper: &str) -> Option<(String, String)> {
    let command_name = format!("docker-credential-{}", helper);
    let mut child = tokio::process::Command::new(&command_name)
        .arg("get")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .ok()?;

    {
        let mut stdin = child.stdin.take()?;
        use tokio::io::AsyncWriteExt;
        stdin.write_all(registry.as_bytes()).await.ok()?;
        // Dropping stdin closes the pipe.
    }

    let output = child.wait_with_output().await.ok()?;
    if !output.status.success() {
        return None;
    }

    let response: CredentialHelperResponse = serde_json::from_slice(&output.stdout).ok()?;
    Some((response.username, response.secret))
}

#[cfg(test)]
mod tests {
    use super::*;
    use oci_client::secrets::RegistryAuth;

    #[test]
    fn test_normalize_image_ref_registry_scheme() {
        let reference = RegistryResolver::normalize_image_ref("registry://alpine:latest").unwrap();
        assert_eq!(reference.registry(), "docker.io");
        assert_eq!(reference.repository(), "library/alpine");
        assert_eq!(reference.tag(), Some("latest"));
    }

    #[test]
    fn test_normalize_image_ref_full_host() {
        let reference =
            RegistryResolver::normalize_image_ref("registry://ghcr.io/example/app:1.0").unwrap();
        assert_eq!(reference.registry(), "ghcr.io");
        assert_eq!(reference.repository(), "example/app");
        assert_eq!(reference.tag(), Some("1.0"));
    }

    #[test]
    fn test_normalize_image_ref_invalid_scheme() {
        assert!(RegistryResolver::normalize_image_ref("docker://alpine:latest").is_err());
    }

    #[test]
    fn test_normalize_image_ref_empty() {
        assert!(RegistryResolver::normalize_image_ref("registry://").is_err());
    }

    #[tokio::test]
    async fn test_registry_auth_from_config_auths() {
        let encoded = base64::engine::general_purpose::STANDARD.encode("user:pass");
        let mut auths = HashMap::new();
        auths.insert(
            "ghcr.io".to_string(),
            DockerAuthEntry {
                auth: Some(encoded),
                username: None,
                password: None,
            },
        );
        let config = DockerConfig {
            auths,
            cred_helpers: HashMap::new(),
        };
        let reference = "ghcr.io/example/app:1.0".parse::<Reference>().unwrap();
        match registry_auth_from_config(&config, &reference).await {
            RegistryAuth::Basic(u, p) => {
                assert_eq!(u, "user");
                assert_eq!(p, "pass");
            }
            other => panic!("expected Basic auth, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_registry_auth_from_config_username_password() {
        let mut auths = HashMap::new();
        auths.insert(
            "registry.example.com".to_string(),
            DockerAuthEntry {
                auth: None,
                username: Some("alice".to_string()),
                password: Some("secret".to_string()),
            },
        );
        let config = DockerConfig {
            auths,
            cred_helpers: HashMap::new(),
        };
        let reference = "registry.example.com/foo:bar".parse::<Reference>().unwrap();
        match registry_auth_from_config(&config, &reference).await {
            RegistryAuth::Basic(u, p) => {
                assert_eq!(u, "alice");
                assert_eq!(p, "secret");
            }
            other => panic!("expected Basic auth, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_registry_auth_from_config_no_match() {
        let config = DockerConfig {
            auths: HashMap::new(),
            cred_helpers: HashMap::new(),
        };
        let reference = "alpine:latest".parse::<Reference>().unwrap();
        assert_eq!(
            registry_auth_from_config(&config, &reference).await,
            RegistryAuth::Anonymous
        );
    }

    #[tokio::test]
    #[ignore = "requires network access to Docker Hub"]
    async fn test_fetch_alpine_latest() {
        let resolver = RegistryResolver::new();
        let image = resolver.fetch("registry://alpine:latest").await.unwrap();
        assert!(!image.layers.is_empty());
        assert_eq!(image.reference, "registry://alpine:latest");
    }
}
