use anyhow::{Context, Result};

use crate::oci;
use crate::registry;
use crate::sources;

const DEFAULT_REGISTRY: &str = "ghcr.io/zenmicro-tech/skillforge/skills";

/// Install each requested skill in argument order. Processing stops at the
/// first failure so callers receive a non-zero exit status.
pub fn add_all(name_or_refs: &[String]) -> Result<()> {
    for name_or_ref in name_or_refs {
        if name_or_refs.len() > 1 {
            eprintln!("\nadding {name_or_ref}...");
        }
        add(name_or_ref).with_context(|| format!("adding {name_or_ref}"))?;
    }
    Ok(())
}

pub fn add(name_or_ref: &str) -> Result<()> {
    let dir = if is_oci_ref(name_or_ref) {
        let reference = resolve_oci_tag(name_or_ref)?;
        super::pull::fetch_oci(&reference)?
    } else {
        let (name, tag) = split_bare_ref(name_or_ref);
        match sources::resolve(name) {
            Ok(dir) if tag.is_none() => dir,
            Ok(_) | Err(_) => {
                let reference = resolve_default_repo(name, tag);
                eprintln!("not found locally; pulling {reference}...");
                let reference = resolve_oci_tag(&reference)?;
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

/// True if the argument is an explicit OCI reference (`<host>/<path>[:tag]`)
/// rather than a bare skill name, optionally followed by `:<tag>`.
fn is_oci_ref(s: &str) -> bool {
    s.contains('/')
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

/// Split a bare skill reference into its name and optional version tag.
fn split_bare_ref(reference: &str) -> (&str, Option<&str>) {
    match reference.rsplit_once(':') {
        Some((name, tag)) if !name.is_empty() && !tag.is_empty() => (name, Some(tag)),
        _ => (reference, None),
    }
}

/// Resolve a bare skill name to its repository in the default GHCR namespace.
fn resolve_default_repo(name: &str, tag: Option<&str>) -> String {
    match tag {
        Some(tag) => format!("{DEFAULT_REGISTRY}/{name}:{tag}"),
        None => format!("{DEFAULT_REGISTRY}/{name}"),
    }
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

#[cfg(test)]
mod tests {
    use super::{is_oci_ref, resolve_default_repo, split_bare_ref};

    #[test]
    fn bare_skill_names_resolve_to_the_default_ghcr_namespace() {
        assert_eq!(
            resolve_default_repo("word-count", None),
            "ghcr.io/zenmicro-tech/skillforge/skills/word-count"
        );
    }

    #[test]
    fn tagged_bare_skill_names_resolve_to_the_default_ghcr_namespace() {
        let (name, tag) = split_bare_ref("word-count:1.2.3");

        assert_eq!(name, "word-count");
        assert_eq!(tag, Some("1.2.3"));
        assert_eq!(
            resolve_default_repo(name, tag),
            "ghcr.io/zenmicro-tech/skillforge/skills/word-count:1.2.3"
        );
    }

    #[test]
    fn distinguishes_bare_skill_names_from_explicit_oci_references() {
        assert!(!is_oci_ref("word-count"));
        assert!(!is_oci_ref("word-count:1.2.3"));
        assert!(is_oci_ref("ghcr.io/acme/skills/word-count"));
        assert!(is_oci_ref("ghcr.io/acme/skills/word-count:1.2.3"));
    }
}

/// Remove each requested skill in argument order. Processing stops at the
/// first failure so callers receive a non-zero exit status.
pub fn remove_all(names: &[String]) -> Result<()> {
    for name in names {
        if names.len() > 1 {
            eprintln!("\nremoving {name}...");
        }
        remove(name).with_context(|| format!("removing {name}"))?;
    }
    Ok(())
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
