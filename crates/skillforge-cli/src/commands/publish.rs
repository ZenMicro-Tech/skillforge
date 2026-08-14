use anyhow::{anyhow, bail, Context, Result};
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::oci::{self, IndexEntry, PushFile, PushRequest, PushResult};
use crate::sources;

pub fn publish(
    name: &str,
    registry_override: Option<&str>,
    path: Option<&str>,
    targets: &[String],
    create_index: bool,
) -> Result<()> {
    let dir = match path {
        Some(p) => PathBuf::from(p),
        None => sources::resolve(name)?,
    };
    let manifest = skillforge_core::Manifest::from_path(dir.join("skill.toml"))?;

    let registry = match registry_override {
        Some(r) => r.to_string(),
        None => manifest
            .publish
            .as_ref()
            .map(|p| p.registry.clone())
            .ok_or_else(|| {
                anyhow!(
                    "no `[publish]` section in skill.toml and no --registry override provided.\n\
                     Add to skill.toml:\n  [publish]\n  registry = \"ghcr.io/<owner>/skills\"",
                )
            })?,
    };

    let repo = format!("{}/{}", registry, manifest.skill.name);
    let tag = &manifest.skill.version;

    if create_index {
        if targets.len() < 2 {
            bail!("--create-index requires at least two --target flags");
        }
        return publish_index_only(&manifest, &registry, &repo, tag, targets);
    }

    if targets.is_empty() {
        return publish_single(&dir, &manifest, &registry, &repo, tag, None);
    }

    if targets.len() == 1 {
        return publish_single(&dir, &manifest, &registry, &repo, tag, Some(&targets[0]));
    }

    publish_multiarch(&dir, &manifest, &registry, &repo, tag, targets)
}

fn publish_single(
    dir: &PathBuf,
    manifest: &skillforge_core::Manifest,
    registry: &str,
    repo: &str,
    tag: &str,
    target: Option<&str>,
) -> Result<()> {
    let bin = super::build::run_for_target(Some(&dir.display().to_string()), target)?;
    if !bin.exists() {
        bail!("binary missing after build: {}", bin.display());
    }

    let platform = match target {
        Some(t) => platform_from_target(t),
        None => current_platform(),
    };

    // Explicit --target → platform-specific tag (for later --create-index stitching).
    // No target → bare version tag (single-platform publish).
    let reference = match target {
        Some(_) => format!("{repo}:{tag}-{platform}"),
        None => format!("{repo}:{tag}"),
    };

    eprintln!("publishing {}:{} to {repo}", manifest.skill.name, tag);
    eprintln!("  platform: {platform}");

    let annotations = build_annotations(manifest, tag, &platform);
    let result = push_skill(dir, &bin, &manifest.skill.name, &reference, annotations)?;

    eprintln!("✓ published {reference}");
    eprintln!("  manifest: {}", result.manifest_url);

    // Tag as :latest for non-platform-specific publishes
    if target.is_none() {
        let latest_ref = format!("{repo}:latest");
        eprintln!("  tagging {latest_ref}...");
        oci::retag(&reference, &latest_ref)
            .with_context(|| format!("tagging {latest_ref}"))?;
        eprintln!("  ✓ latest");
    }

    push_catalog_entry_for(manifest, registry, repo)?;

    Ok(())
}

fn publish_multiarch(
    dir: &PathBuf,
    manifest: &skillforge_core::Manifest,
    registry: &str,
    repo: &str,
    tag: &str,
    targets: &[String],
) -> Result<()> {
    eprintln!(
        "publishing {}:{} (multi-arch) to {repo}",
        manifest.skill.name, tag
    );

    let mut index_entries: Vec<IndexEntry> = Vec::new();

    for target in targets {
        let (os, arch) = parse_rust_target(target)?;
        let platform_tag = format!("{tag}-{os}-{arch}");
        let reference = format!("{repo}:{platform_tag}");
        let platform = format!("{os}-{arch}");

        eprintln!("\n  building for {target}...");
        let bin = super::build::run_for_target(Some(&dir.display().to_string()), Some(target))?;
        if !bin.exists() {
            bail!("binary missing after build for {target}: {}", bin.display());
        }

        let annotations = build_annotations(manifest, tag, &platform);

        eprintln!("  pushing {reference}...");
        let result = push_skill(dir, &bin, &manifest.skill.name, &reference, annotations)?;

        eprintln!("  ✓ {platform_tag} → {}", result.manifest_digest);
        index_entries.push(IndexEntry {
            digest: result.manifest_digest,
            size: result.manifest_size,
            os: leak_str(os),
            arch: leak_str(arch),
        });
    }

    let index_ref = format!("{repo}:{tag}");
    eprintln!("\npushing image index as {index_ref}...");
    let url = oci::push_index(&index_ref, &index_entries)
        .with_context(|| format!("pushing image index to {index_ref}"))?;

    eprintln!("✓ published multi-arch manifest: {index_ref}");
    eprintln!("  index: {url}");

    let latest_ref = format!("{repo}:latest");
    eprintln!("  tagging {latest_ref}...");
    oci::retag(&index_ref, &latest_ref)
        .with_context(|| format!("tagging {latest_ref}"))?;
    eprintln!("  ✓ latest");

    push_catalog_entry_for(manifest, registry, repo)?;

    Ok(())
}

fn publish_index_only(
    manifest: &skillforge_core::Manifest,
    registry: &str,
    repo: &str,
    tag: &str,
    targets: &[String],
) -> Result<()> {
    eprintln!(
        "creating multi-arch index for {}:{} on {repo}",
        manifest.skill.name, tag
    );

    let mut index_entries: Vec<oci::IndexEntry> = Vec::new();

    for target in targets {
        let (os, arch) = parse_rust_target(target)?;
        let platform_tag = format!("{tag}-{os}-{arch}");
        let reference = format!("{repo}:{platform_tag}");

        eprintln!("  resolving {reference}...");
        let info = oci::fetch_manifest_info(&reference)
            .with_context(|| format!("could not find previously-pushed manifest for {target}.\n\
                Ensure `skillforge publish {name} --target {target}` was run first.",
                name = manifest.skill.name))?;

        eprintln!("  ✓ {platform_tag} → {}", info.digest);
        index_entries.push(oci::IndexEntry {
            digest: info.digest,
            size: info.size,
            os: leak_str(os),
            arch: leak_str(arch),
        });
    }

    let index_ref = format!("{repo}:{tag}");
    eprintln!("\npushing image index as {index_ref}...");
    let url = oci::push_index(&index_ref, &index_entries)
        .with_context(|| format!("pushing image index to {index_ref}"))?;

    eprintln!("✓ published multi-arch manifest: {index_ref}");
    eprintln!("  index: {url}");

    let latest_ref = format!("{repo}:latest");
    eprintln!("  tagging {latest_ref}...");
    oci::retag(&index_ref, &latest_ref)
        .with_context(|| format!("tagging {latest_ref}"))?;
    eprintln!("  ✓ latest");

    push_catalog_entry_for(manifest, registry, repo)?;

    Ok(())
}

fn build_annotations(
    manifest: &skillforge_core::Manifest,
    tag: &str,
    platform: &str,
) -> BTreeMap<String, String> {
    let mut annotations = BTreeMap::new();
    annotations.insert(
        "skillforge.skill.name".to_string(),
        manifest.skill.name.clone(),
    );
    annotations.insert("skillforge.skill.version".to_string(), tag.to_string());
    annotations.insert(
        "skillforge.skill.platform".to_string(),
        platform.to_string(),
    );
    annotations.insert(
        "org.opencontainers.image.description".to_string(),
        manifest.skill.description.clone(),
    );
    if let Some(license) = &manifest.skill.license {
        annotations.insert(
            "org.opencontainers.image.licenses".to_string(),
            license.clone(),
        );
    }
    annotations.insert(
        "skillforge.skill.interfaces".to_string(),
        interfaces_annotation(manifest),
    );
    annotations
}

/// Comma-separated list of enabled interfaces, recorded in annotations.
fn interfaces_annotation(manifest: &skillforge_core::Manifest) -> String {
    [
        manifest.interfaces.mcp.then_some("mcp"),
        manifest.interfaces.cli.then_some("cli"),
        manifest.interfaces.http.then_some("http"),
        manifest.interfaces.lib.then_some("lib"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(",")
}

fn push_skill(
    dir: &PathBuf,
    bin: &PathBuf,
    name: &str,
    reference: &str,
    annotations: BTreeMap<String, String>,
) -> Result<PushResult> {
    let toml_path = dir.join("skill.toml");
    let prompt_path = dir.join("prompt.md");
    let schema_path = dir.join("schema.json");

    let files = [
        PushFile {
            path: bin,
            media_type: "application/vnd.skillforge.binary",
            title: name,
        },
        PushFile {
            path: &toml_path,
            media_type: "application/toml",
            title: "skill.toml",
        },
        PushFile {
            path: &prompt_path,
            media_type: "text/markdown",
            title: "prompt.md",
        },
        PushFile {
            path: &schema_path,
            media_type: "application/schema+json",
            title: "schema.json",
        },
    ];

    oci::push(PushRequest {
        reference,
        files: &files,
        annotations,
    })
    .with_context(|| format!("publishing to {reference}"))
}

fn parse_rust_target(target: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = target.split('-').collect();
    if parts.len() < 3 {
        bail!("invalid target triple: {target}");
    }
    let arch = match parts[0] {
        "aarch64" => "arm64",
        "x86_64" => "amd64",
        "arm" => "arm",
        a => a,
    };
    let os = if target.contains("linux") {
        "linux"
    } else if target.contains("darwin") || target.contains("apple") {
        "darwin"
    } else if target.contains("windows") {
        "windows"
    } else {
        parts[2]
    };
    Ok((os.to_string(), arch.to_string()))
}

fn current_platform() -> String {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        o => o,
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "amd64",
        a => a,
    };
    format!("{os}-{arch}")
}

fn platform_from_target(target: &str) -> String {
    match parse_rust_target(target) {
        Ok((os, arch)) => format!("{os}-{arch}"),
        Err(_) => current_platform(),
    }
}

/// Leaks a String to get a `&'static str`. Used for IndexEntry which requires
/// `&'static str` fields. Acceptable here since we only leak a handful of short
/// platform strings per process lifetime.
fn leak_str(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn push_catalog_entry_for(
    manifest: &skillforge_core::Manifest,
    registry: &str,
    skill_repo: &str,
) -> Result<()> {
    let catalog_repo = format!("{registry}/catalog");
    let catalog_tag = format!("{}--{}", manifest.skill.name, manifest.skill.version);
    let catalog_ref = format!("{catalog_repo}:{catalog_tag}");

    let mut annotations = BTreeMap::new();
    annotations.insert(
        "org.opencontainers.image.title".to_string(),
        manifest.skill.name.clone(),
    );
    annotations.insert(
        "org.opencontainers.image.description".to_string(),
        manifest.skill.description.clone(),
    );
    annotations.insert(
        "org.opencontainers.image.version".to_string(),
        manifest.skill.version.clone(),
    );
    if let Some(license) = &manifest.skill.license {
        annotations.insert(
            "org.opencontainers.image.licenses".to_string(),
            license.clone(),
        );
    }

    annotations.insert(
        "skillforge.skill.interfaces".to_string(),
        interfaces_annotation(manifest),
    );
    annotations.insert("skillforge.skill.repo".to_string(), skill_repo.to_string());

    eprintln!("registering in catalog: {catalog_ref}");
    match oci::push_catalog_entry(&catalog_ref, annotations) {
        Ok(_url) => {
            eprintln!("✓ catalog updated");
        }
        Err(e) => {
            eprintln!("⚠ catalog update failed (skill was published successfully): {e}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[skill]
name = "word-count"
version = "1.0.0"
description = "Count words."
license = "MIT"

[runtime]
kind = "rust"
entrypoint = "src/main.rs"
"#;

    #[test]
    fn artifact_annotations_carry_full_info_metadata() {
        let manifest = skillforge_core::Manifest::from_str(SAMPLE).expect("parse");
        let annotations = build_annotations(&manifest, "1.0.0", "darwin-arm64");

        assert_eq!(
            annotations
                .get("org.opencontainers.image.description")
                .map(String::as_str),
            Some("Count words.")
        );
        assert_eq!(
            annotations
                .get("org.opencontainers.image.licenses")
                .map(String::as_str),
            Some("MIT")
        );
        assert_eq!(
            annotations
                .get("skillforge.skill.interfaces")
                .map(String::as_str),
            Some("mcp,cli,http,lib")
        );
    }

    #[test]
    fn artifact_annotations_omit_license_when_unset() {
        let manifest =
            skillforge_core::Manifest::from_str(&SAMPLE.replace("license = \"MIT\"\n", ""))
                .expect("parse");
        let annotations = build_annotations(&manifest, "1.0.0", "darwin-arm64");

        assert!(!annotations.contains_key("org.opencontainers.image.licenses"));
    }
}

