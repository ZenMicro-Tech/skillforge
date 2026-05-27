use anyhow::{anyhow, bail, Context, Result};
use std::collections::BTreeMap;

use crate::oci::{self, PushFile, PushRequest};
use crate::sources;

pub fn publish(name: &str, repo_override: Option<&str>) -> Result<()> {
    let dir = sources::resolve(name)?;
    let manifest = skillforge_core::Manifest::from_path(dir.join("skill.toml"))?;

    let repo = match repo_override {
        Some(r) => r.to_string(),
        None => manifest
            .publish
            .as_ref()
            .map(|p| p.repo.clone())
            .ok_or_else(|| {
                anyhow!(
                    "no `[publish]` section in skill.toml and no --repo override provided.\n\
                     Add to skill.toml:\n  [publish]\n  repo = \"ghcr.io/<owner>/skills/{}\"",
                    manifest.skill.name
                )
            })?,
    };

    // Always rebuild before publish: skill.toml / prompt.md / schema.json are embedded
    // at build time via build.rs, and Cargo's change detection on those files via
    // `cargo:rerun-if-changed` works only if the source files mtime forward. Forcing a
    // build avoids shipping a stale embedded version.
    super::build::run(Some(&dir.display().to_string()))?;
    let bin = dir.join("target/release").join(&manifest.skill.name);
    if !bin.exists() {
        bail!("binary missing after build: {}", bin.display());
    }

    let toml_path = dir.join("skill.toml");
    let prompt_path = dir.join("prompt.md");
    let schema_path = dir.join("schema.json");

    let tag = &manifest.skill.version;
    let platform = current_platform();
    let reference = format!("{repo}:{tag}");

    eprintln!("publishing {}:{} to {repo}", manifest.skill.name, tag);
    eprintln!("  platform: {platform}");

    let mut annotations = BTreeMap::new();
    annotations.insert(
        "skillforge.skill.name".to_string(),
        manifest.skill.name.clone(),
    );
    annotations.insert("skillforge.skill.version".to_string(), tag.clone());
    annotations.insert("skillforge.skill.platform".to_string(), platform);
    annotations.insert(
        "org.opencontainers.image.description".to_string(),
        manifest.skill.description.clone(),
    );

    let files = [
        PushFile {
            path: &bin,
            media_type: "application/vnd.skillforge.binary",
            title: &manifest.skill.name,
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

    let manifest_url = oci::push(PushRequest {
        reference: &reference,
        files: &files,
        annotations,
    })
    .with_context(|| format!("publishing to {reference}"))?;

    eprintln!("✓ published {reference}");
    eprintln!("  manifest: {manifest_url}");
    Ok(())
}

fn current_platform() -> String {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let arch = match arch {
        "aarch64" => "arm64",
        "x86_64" => "amd64",
        a => a,
    };
    format!("{os}-{arch}")
}
