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
    let auth = auth_for_pull(&parsed);
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

/// List all tags in a repository. Used by the catalog "bucket of tags" pattern.
pub fn list_tags(repository: &str) -> Result<Vec<String>> {
    let reference = format!("{repository}:__unused__");
    let parsed = parse_ref(&reference)?;
    let auth = auth_for_pull(&parsed);

    block_on(async move {
        let client = Client::new(ClientConfig::default());
        let resp = client
            .list_tags(&parsed, &auth, None, None)
            .await
            .context("listing tags")?;
        Ok(resp.tags)
    })
}

/// Fetch manifest-level annotations for a given reference.
pub fn fetch_manifest_annotations(reference: &str) -> Result<BTreeMap<String, String>> {
    let parsed = parse_ref(reference)?;
    let auth = auth_for_pull(&parsed);

    block_on(async move {
        let client = Client::new(ClientConfig::default());
        let (manifest, _digest) = client
            .pull_image_manifest(&parsed, &auth)
            .await
            .context("pull manifest for annotations")?;
        Ok(manifest.annotations.unwrap_or_default())
    })
}

/// Push a metadata-only manifest (no layers) with annotations to a catalog tag.
/// Used by publish to register a skill in the catalog.
pub fn push_catalog_entry(
    reference: &str,
    annotations: BTreeMap<String, String>,
) -> Result<String> {
    let parsed = parse_ref(reference)?;
    let auth = auth_for_push(&parsed)?;

    block_on(async move {
        let client = Client::new(ClientConfig::default());
        let config = Config::new(
            Bytes::from_static(b"{}"),
            EMPTY_CONFIG_MEDIA_TYPE.to_string(),
            None,
        );
        let meta_json = serde_json::to_vec(&annotations).context("serialize catalog metadata")?;
        let layers = vec![ImageLayer::new(
            meta_json,
            "application/vnd.skillforge.catalog-metadata.v1+json".to_string(),
            None,
        )];
        let mut manifest = OciImageManifest::build(&layers, &config, Some(annotations));
        manifest.artifact_type = Some("application/vnd.skillforge.catalog-entry.v1+json".to_string());

        let resp = client
            .push(&parsed, &layers, config, &auth, Some(manifest))
            .await
            .context("push catalog entry")?;

        Ok(resp.manifest_url)
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

// ---------------------------------------------------------------------------
// Authentication
//
// Resolution order:
//   1. Docker config (~/.docker/config.json): credHelpers, credsStore, auths
//   2. GitHub-specific: GH_TOKEN / GITHUB_TOKEN env vars, `gh auth token`
//   3. Anonymous
// ---------------------------------------------------------------------------

fn auth_for_pull(reference: &Reference) -> RegistryAuth {
    if let Some(auth) = docker_auth_for(reference.registry()) {
        return auth;
    }
    if let Some(token) = github_token() {
        return RegistryAuth::Basic(github_username().unwrap_or_default(), token);
    }
    RegistryAuth::Anonymous
}

fn auth_for_push(reference: &Reference) -> Result<RegistryAuth> {
    if let Some(auth) = docker_auth_for(reference.registry()) {
        return Ok(auth);
    }
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

// ---------------------------------------------------------------------------
// Docker config credential resolution
// ---------------------------------------------------------------------------

fn docker_auth_for(registry: &str) -> Option<RegistryAuth> {
    let config = load_docker_config()?;

    // 1. Per-registry credential helper (credHelpers)
    if let Some(helpers) = config.cred_helpers.as_ref() {
        if let Some(helper) = helpers.get(registry) {
            return cred_helper_get(helper, registry);
        }
    }

    // 2. Default credential store (credsStore)
    if let Some(store) = config.creds_store.as_deref() {
        if let Some(auth) = cred_helper_get(store, registry) {
            return Some(auth);
        }
    }

    // 3. Static auths (base64-encoded user:pass or token)
    if let Some(auths) = config.auths.as_ref() {
        let entry = auths.get(registry).or_else(|| {
            // Docker sometimes stores keys as https://<registry>/v1/ or similar
            auths.iter().find_map(|(k, v)| {
                if k.contains(registry) { Some(v) } else { None }
            })
        })?;
        return auth_from_docker_entry(entry);
    }

    None
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct DockerConfig {
    auths: Option<std::collections::HashMap<String, DockerAuthEntry>>,
    creds_store: Option<String>,
    cred_helpers: Option<std::collections::HashMap<String, String>>,
}

#[derive(serde::Deserialize, Default)]
struct DockerAuthEntry {
    auth: Option<String>,
    username: Option<String>,
    password: Option<String>,
}

fn load_docker_config() -> Option<DockerConfig> {
    let dir = std::env::var("DOCKER_CONFIG")
        .map(std::path::PathBuf::from)
        .ok()
        .or_else(|| dirs::home_dir().map(|h| h.join(".docker")))?;
    let data = std::fs::read_to_string(dir.join("config.json")).ok()?;
    serde_json::from_str(&data).ok()
}

fn auth_from_docker_entry(entry: &DockerAuthEntry) -> Option<RegistryAuth> {
    // Explicit username/password fields
    if let (Some(user), Some(pass)) = (entry.username.as_deref(), entry.password.as_deref()) {
        if !user.is_empty() {
            return Some(RegistryAuth::Basic(user.to_string(), pass.to_string()));
        }
    }
    // Base64-encoded "user:pass" in the `auth` field
    if let Some(encoded) = entry.auth.as_deref() {
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD.decode(encoded).ok()?;
        let decoded = String::from_utf8(decoded).ok()?;
        let (user, pass) = decoded.split_once(':')?;
        if !user.is_empty() {
            return Some(RegistryAuth::Basic(user.to_string(), pass.to_string()));
        }
    }
    None
}

/// Invoke a Docker credential helper (docker-credential-<helper>) and parse its
/// JSON output for the given registry.
fn cred_helper_get(helper: &str, registry: &str) -> Option<RegistryAuth> {
    use std::io::Write;

    let prog = format!("docker-credential-{helper}");
    let mut child = std::process::Command::new(&prog)
        .arg("get")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(registry.as_bytes()).ok()?;
    }

    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }

    let cred: CredHelperOutput = serde_json::from_slice(&output.stdout).ok()?;
    if cred.username.is_empty() {
        return None;
    }
    Some(RegistryAuth::Basic(cred.username, cred.secret))
}

#[derive(serde::Deserialize, Default)]
struct CredHelperOutput {
    #[serde(default, alias = "Username")]
    username: String,
    #[serde(default, alias = "Secret")]
    secret: String,
}

// ---------------------------------------------------------------------------
// GitHub-specific fallback (GH_TOKEN, GITHUB_TOKEN, gh CLI)
// ---------------------------------------------------------------------------

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
