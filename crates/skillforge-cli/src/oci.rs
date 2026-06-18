//! Native OCI artifact pull/push, using the `oci-client` crate.
//!
//! Replaces a previous shell-out to the `oras` CLI so the skillforge binary is
//! self-contained for distribution.

use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use oci_client::client::{Client, ClientConfig, Config, ImageLayer};
use oci_client::manifest::{ImageIndexEntry, OciImageIndex, OciImageManifest, Platform};
use oci_client::secrets::RegistryAuth;
use oci_client::Reference;
use oci_spec::image::{Arch, Os};
use std::collections::BTreeMap;
use std::path::Path;
use tokio::io::AsyncWriteExt;

const ARTIFACT_TYPE: &str = "application/vnd.skillforge.skill.v1+json";
const EMPTY_CONFIG_MEDIA_TYPE: &str = "application/vnd.oci.empty.v1+json";
const TITLE: &str = "org.opencontainers.image.title";

pub struct PushFile<'a> {
    pub path: &'a Path,
    pub media_type: &'a str,
    pub title: &'a str,
}

pub struct PushRequest<'a> {
    pub reference: &'a str,
    pub files: &'a [PushFile<'a>],
    pub annotations: BTreeMap<String, String>,
}

pub struct PushResult {
    pub manifest_url: String,
    pub manifest_digest: String,
    pub manifest_size: i64,
}

pub fn push(req: PushRequest<'_>) -> Result<PushResult> {
    let reference = parse_ref(req.reference)?;
    let auth = auth_for_push(&reference)?;
    let layers = read_layers(req.files)?;

    block_on(async move {
        let client = Client::new(ClientConfig::default());
        let config = Config::new(
            Bytes::from_static(b"{}"),
            EMPTY_CONFIG_MEDIA_TYPE.to_string(),
            None,
        );
        let mut manifest =
            OciImageManifest::build(&layers, &config, Some(req.annotations.clone()));
        manifest.artifact_type = Some(ARTIFACT_TYPE.to_string());

        let resp = client
            .push(&reference, &layers, config, &auth, Some(manifest))
            .await
            .context("oci push")?;

        // Re-fetch to get the authoritative digest and serialized size
        let (stored_manifest, stored_digest) = client
            .pull_image_manifest(&reference, &auth)
            .await
            .context("fetching stored manifest")?;
        let manifest_size = serde_json::to_vec(&stored_manifest)?.len() as i64;

        Ok(PushResult {
            manifest_url: resp.manifest_url,
            manifest_digest: stored_digest,
            manifest_size,
        })
    })
}

pub struct IndexEntry {
    pub digest: String,
    pub size: i64,
    pub os: &'static str,
    pub arch: &'static str,
}

pub fn push_index(reference: &str, entries: &[IndexEntry]) -> Result<String> {
    let parsed = parse_ref(reference)?;
    let auth = auth_for_push(&parsed)?;

    let manifests: Vec<ImageIndexEntry> = entries
        .iter()
        .map(|e| ImageIndexEntry {
            media_type: "application/vnd.oci.image.manifest.v1+json".to_string(),
            digest: e.digest.clone(),
            size: e.size,
            platform: Some(Platform {
                architecture: arch_from_str(e.arch),
                os: os_from_str(e.os),
                os_version: None,
                os_features: None,
                variant: None,
                features: None,
            }),
            annotations: None,
            artifact_type: Some(ARTIFACT_TYPE.to_string()),
        })
        .collect();

    let index = OciImageIndex {
        schema_version: 2,
        media_type: Some("application/vnd.oci.image.index.v1+json".to_string()),
        manifests,
        artifact_type: Some(ARTIFACT_TYPE.to_string()),
        annotations: None,
    };

    block_on(async move {
        let client = Client::new(ClientConfig::default());
        let url = client
            .push_manifest_list(&parsed, &auth, index)
            .await
            .context("push image index")?;
        Ok(url)
    })
}

fn arch_from_str(s: &str) -> Arch {
    match s {
        "amd64" => Arch::Amd64,
        "arm64" => Arch::ARM64,
        _ => Arch::Other(s.to_string()),
    }
}

fn os_from_str(s: &str) -> Os {
    match s {
        "linux" => Os::Linux,
        "darwin" => Os::Darwin,
        "windows" => Os::Windows,
        _ => Os::Other(s.to_string()),
    }
}


pub struct PullResult {
    pub manifest_digest: String,
}

pub fn pull(reference: &str, out_dir: &Path) -> Result<PullResult> {
    let parsed = parse_ref(reference)?;
    let auth = auth_for_pull();
    std::fs::create_dir_all(out_dir).with_context(|| format!("mkdir {}", out_dir.display()))?;

    let out_dir = out_dir.to_path_buf();
    block_on(async move {
        let client = Client::new(ClientConfig::default());
        let (manifest, manifest_digest) = client
            .pull_image_manifest(&parsed, &auth)
            .await
            .context("oci pull manifest")?;

        for layer in &manifest.layers {
            let title = layer
                .annotations
                .as_ref()
                .and_then(|a| a.get(TITLE))
                .cloned()
                .ok_or_else(|| {
                    anyhow!(
                        "layer missing {TITLE} annotation; can't determine filename for digest {}",
                        layer.digest
                    )
                })?;
            let path = out_dir.join(&title);
            let mut file = tokio::fs::File::create(&path)
                .await
                .with_context(|| format!("creating {}", path.display()))?;
            client
                .pull_blob(&parsed, layer, &mut file)
                .await
                .with_context(|| format!("downloading {title}"))?;
            file.flush().await.ok();
        }
        Ok(PullResult { manifest_digest })
    })
}

fn parse_ref(s: &str) -> Result<Reference> {
    s.parse::<Reference>()
        .map_err(|e| anyhow!("invalid OCI reference {s:?}: {e}"))
}

fn read_layers(files: &[PushFile<'_>]) -> Result<Vec<ImageLayer>> {
    let mut layers = Vec::with_capacity(files.len());
    for f in files {
        let data = std::fs::read(f.path)
            .with_context(|| format!("reading {}", f.path.display()))?;
        let mut annotations = BTreeMap::new();
        annotations.insert(TITLE.to_string(), f.title.to_string());
        layers.push(ImageLayer::new(
            data,
            f.media_type.to_string(),
            Some(annotations),
        ));
    }
    Ok(layers)
}

/// Anonymous read works for public GHCR packages. Private packages will return
/// a clear unauthorized error and the user can set `GH_TOKEN` or run `gh auth
/// login`.
fn auth_for_pull() -> RegistryAuth {
    if let Some(token) = github_token() {
        return RegistryAuth::Basic(github_username().unwrap_or_default(), token);
    }
    RegistryAuth::Anonymous
}

fn auth_for_push(reference: &Reference) -> Result<RegistryAuth> {
    if reference.registry().contains("ghcr.io") {
        let user = github_username().ok_or_else(|| {
            anyhow!(
                "publishing to GHCR requires `gh` to be authenticated. Run `gh auth login` first."
            )
        })?;
        let token = github_token().ok_or_else(|| {
            anyhow!(
                "publishing to GHCR requires a GitHub token. Run `gh auth login` (with write:packages scope) first."
            )
        })?;
        return Ok(RegistryAuth::Basic(user, token));
    }
    if let Some(token) = github_token() {
        return Ok(RegistryAuth::Basic(github_username().unwrap_or_default(), token));
    }
    Ok(RegistryAuth::Anonymous)
}

fn github_token() -> Option<String> {
    if let Ok(t) = std::env::var("GH_TOKEN") {
        if !t.is_empty() {
            return Some(t);
        }
    }
    if let Ok(t) = std::env::var("GITHUB_TOKEN") {
        if !t.is_empty() {
            return Some(t);
        }
    }
    let out = std::process::Command::new("gh")
        .args(["auth", "token"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn github_username() -> Option<String> {
    if let Ok(u) = std::env::var("GITHUB_USER") {
        if !u.is_empty() {
            return Some(u);
        }
    }
    let out = std::process::Command::new("gh")
        .args(["api", "user", "--jq", ".login"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn block_on<F: std::future::Future<Output = Result<R>>, R>(fut: F) -> Result<R> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("tokio runtime")?;
    rt.block_on(fut)
}
