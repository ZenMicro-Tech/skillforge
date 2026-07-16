use anyhow::{Context, Result};

use crate::oci;
use crate::registry;
use crate::sources;

const DEFAULT_CATALOG: &str = "ghcr.io/zenmicro-tech/skillforge/skills/catalog";

pub fn add(name_or_ref: &str) -> Result<()> {
    let dir = if is_oci_ref(name_or_ref) {
        let reference = resolve_oci_tag(name_or_ref)?;
        super::pull::fetch_oci(&reference)?
    } else {
        match sources::resolve(name_or_ref) {
            Ok(dir) => dir,
            Err(_) => {
                let reference = resolve_from_catalog(name_or_ref)?;
                super::pull::fetch_oci(&reference)?
            }
        }
    };
    let dir_str = dir.display().to_string();
    eprintln!("found at {dir_str}");
    super::build::run(Some(&dir_str))?;

    let manifest = skillforge_core::Manifest::from_path(dir.join("skill.toml"))?;
    let bin = std::fs::canonicalize(dir.join("target/release").join(&manifest.skill.name))?;
    let (skill_name, entry) = registry::entry_from_dir(&dir, bin)?;
    registry::upsert_skill(&skill_name, entry)?;

    if registry::is_mux_enabled() {
        eprintln!("mux active — {skill_name} available via skillforge MCP server");
    } else {
        super::link::link(Some(&dir_str), None)?;
    }
    Ok(())
}

/// True if the argument looks like an OCI reference (`<host>/<path>[:tag]`)
/// rather than a bare skill name.
fn is_oci_ref(s: &str) -> bool {
    s.contains('/') || s.contains(':')
}

/// When an OCI reference has no explicit tag, resolve the latest version by
/// listing tags on the target repo and picking the highest semver.
/// Prefers platform-specific tags (e.g. `0.1.0-darwin-arm64`) that match this host.
fn resolve_oci_tag(reference: &str) -> Result<String> {
    if reference.contains(':') {
        return Ok(reference.to_string());
    }
    eprintln!("resolving latest version for {reference}...");
    let tags = oci::list_tags(reference)
        .with_context(|| format!("listing tags for {reference}"))?;

    let platform_suffix = format!("-{}", current_platform());

    // Prefer a platform-specific tag matching this host (e.g. "0.1.0-darwin-arm64")
    let platform_match = tags
        .iter()
        .filter(|t| t.ends_with(&platform_suffix))
        .max_by(|a, b| cmp_semver(a, b));

    if let Some(tag) = platform_match {
        eprintln!("  resolved to {reference}:{tag}");
        return Ok(format!("{reference}:{tag}"));
    }

    // Fall back to bare semver tags (image index or single-platform)
    let best = tags
        .iter()
        .filter(|t| !t.contains('-'))
        .max_by(|a, b| cmp_semver(a, b))
        .or_else(|| tags.iter().max_by(|a, b| cmp_semver(a, b)));

    match best {
        Some(tag) => {
            eprintln!("  resolved to {reference}:{tag}");
            Ok(format!("{reference}:{tag}"))
        }
        None => anyhow::bail!("no tags found for {reference}"),
    }
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

/// Resolve a bare skill name by looking it up in the catalog.
fn resolve_from_catalog(name: &str) -> Result<String> {
    eprintln!("searching registry for \"{name}\"...");
    let tags = oci::list_tags(DEFAULT_CATALOG)
        .context("fetching skill catalog")?;

    let matching: Vec<(&str, &str)> = tags
        .iter()
        .filter_map(|tag| {
            let (n, v) = tag.rsplit_once("--")?;
            if n == name { Some((n, v)) } else { None }
        })
        .collect();

    if matching.is_empty() {
        anyhow::bail!(
            "skill \"{name}\" not found in registry.\n\
             Run `skillforge search` to see available skills."
        );
    }

    let latest_version = matching
        .iter()
        .map(|(_, v)| *v)
        .max_by(|a, b| cmp_semver(a, b))
        .unwrap();

    let catalog_tag = format!("{name}--{latest_version}");
    let catalog_ref = format!("{DEFAULT_CATALOG}:{catalog_tag}");
    let annotations = oci::fetch_manifest_annotations(&catalog_ref)
        .with_context(|| format!("fetching catalog metadata for {name}"))?;

    let repo = annotations
        .get("skillforge.skill.repo")
        .ok_or_else(|| anyhow::anyhow!(
            "catalog entry for \"{name}\" is missing repo reference"
        ))?;

    let reference = format!("{repo}:{latest_version}");
    eprintln!("  found {name} v{latest_version} at {repo}");
    Ok(reference)
}

fn cmp_semver(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| -> (u64, u64, u64) {
        let core = s.split(['-', '+']).next().unwrap_or("");
        let parts: Vec<u64> = core.split('.').filter_map(|p| p.parse().ok()).collect();
        (
            parts.first().copied().unwrap_or(0),
            parts.get(1).copied().unwrap_or(0),
            parts.get(2).copied().unwrap_or(0),
        )
    };
    parse(a).cmp(&parse(b))
}

pub fn remove(name: &str) -> Result<()> {
    let reg = registry::load()?;
    let entry = reg.skills.get(name).cloned();

    if !reg.mux.enabled {
        let dir_hint = entry
            .as_ref()
            .map(|e| e.source_dir.display().to_string())
            .or_else(|| sources::resolve(name).ok().map(|p| p.display().to_string()));
        if let Some(dir) = dir_hint {
            let _ = super::link::unlink(Some(&dir), None);
        }
    }

    if registry::remove_skill(name)? {
        eprintln!("removed {name} from registry");
    } else {
        eprintln!("{name} was not in the registry");
    }

    if let Some(entry) = entry {
        let home = registry::home();
        if entry.source_dir.starts_with(&home) && entry.source_dir.is_dir() {
            std::fs::remove_dir_all(&entry.source_dir)?;
            eprintln!("deleted {}", entry.source_dir.display());
        }
    }

    Ok(())
}
