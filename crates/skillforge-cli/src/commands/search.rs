//! `skillforge search` — discover skills from an OCI catalog registry using
//! the "bucket of tags" pattern.
//!
//! Each published skill writes a metadata-only manifest to a well-known catalog
//! repository (e.g. `ghcr.io/ZenMicro-Tech/skillforge/skills/catalog`) using a tag
//! convention: `{name}--{version}`. Discovery is a tag list + per-tag manifest
//! annotation read.

use anyhow::Result;
use std::collections::BTreeMap;

use crate::oci;

const DEFAULT_CATALOG: &str = "ghcr.io/zenmicro-tech/skillforge/skills/catalog";

pub fn search(query: Option<&str>, registry: Option<&str>) -> Result<()> {
    let catalog = registry.unwrap_or(DEFAULT_CATALOG);

    eprintln!("fetching skill catalog from {catalog}...");

    let tags = oci::list_tags(catalog)?;

    let entries = parse_catalog_tags(&tags);

    let filtered: Vec<&CatalogEntry> = match query {
        Some(q) => {
            let q = q.to_lowercase();
            entries
                .iter()
                .filter(|e| e.name.contains(&q) || e.version.contains(&q))
                .collect()
        }
        None => entries.iter().collect(),
    };

    if filtered.is_empty() {
        if let Some(q) = query {
            eprintln!("no skills matching \"{q}\" found in {catalog}");
        } else {
            eprintln!("no skills found in {catalog}");
        }
        return Ok(());
    }

    // Group by name, show latest version
    let mut by_name: BTreeMap<&str, &CatalogEntry> = BTreeMap::new();
    for entry in &filtered {
        by_name
            .entry(&entry.name)
            .and_modify(|existing| {
                if semver_gt(&entry.version, &existing.version) {
                    *existing = entry;
                }
            })
            .or_insert(entry);
    }

    // Fetch annotations for each skill to get descriptions and repo refs
    let mut details: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for (name, entry) in &by_name {
        let reference = format!("{catalog}:{}", entry.tag);
        if let Ok(annotations) = oci::fetch_manifest_annotations(&reference) {
            details.insert(name.to_string(), annotations);
        }
    }

    println!("{:<20} {:<10} {}", "NAME", "VERSION", "DESCRIPTION");
    println!("{}", "-".repeat(80));
    for (name, entry) in &by_name {
        let desc = details
            .get(*name)
            .and_then(|a| a.get("org.opencontainers.image.description"))
            .map(|s| s.as_str())
            .unwrap_or("");
        println!("{:<20} {:<10} {}", name, entry.version, desc);
    }

    println!();
    println!(
        "{} skill(s) found. Run `skillforge search --info <name>` to see available versions and install instructions.",
        by_name.len()
    );

    Ok(())
}

pub fn search_detail(name: &str, registry: Option<&str>) -> Result<()> {
    // A full OCI reference (contains '/') is answered straight from the repo:
    // versions come from its tag list, metadata from annotations stamped on
    // the artifact manifest at publish time. No catalog required.
    if name.contains('/') {
        return repo_detail(name);
    }

    let catalog = registry.unwrap_or(DEFAULT_CATALOG);

    eprintln!("fetching metadata for {name} from {catalog}...");

    let tags = oci::list_tags(catalog)?;
    let entries = parse_catalog_tags(&tags);

    let mut matching: Vec<&CatalogEntry> = entries.iter().filter(|e| e.name == name).collect();

    if matching.is_empty() {
        eprintln!("skill \"{name}\" not found in {catalog}");
        return Ok(());
    }

    sort_entries_by_version_desc(&mut matching);

    // Fetch annotations from the latest version's manifest.
    let latest = matching.first().unwrap();

    let reference = format!("{catalog}:{}", latest.tag);
    let annotations = oci::fetch_manifest_annotations(&reference)?;

    println!("{}  v{}", name, latest.version);
    println!();

    if let Some(desc) = annotations.get("org.opencontainers.image.description") {
        println!("  {desc}");
        println!();
    }
    if let Some(interfaces) = annotations.get("skillforge.skill.interfaces") {
        println!("  interfaces: {interfaces}");
    }
    if let Some(license) = annotations.get("org.opencontainers.image.licenses") {
        println!("  license:    {license}");
    }
    if let Some(repo) = annotations.get("skillforge.skill.repo") {
        println!("  source:     {repo}");
    }

    println!();
    println!("  available versions:");
    for entry in &matching {
        println!("    {}", entry.version);
    }

    println!();
    println!("  install latest:");
    if let Some(repo) = annotations.get("skillforge.skill.repo") {
        println!("    skillforge add {repo}:{}", latest.version);
        println!("  install a specific version:");
        println!("    skillforge add {repo}:<version>");
    } else {
        println!("    skillforge add {catalog}:{}", latest.tag);
    }

    Ok(())
}

/// Show details for a skill addressed by its full OCI repo reference
/// (e.g. `ghcr.io/acme/skills/word-count`), without consulting a catalog.
/// Versions come from the repo's tag list; metadata comes from the
/// annotations `publish` stamps on the artifact manifest.
fn repo_detail(reference: &str) -> Result<()> {
    let repo = oci::strip_tag(reference);
    eprintln!("fetching metadata from {repo}...");

    let tags = oci::list_tags(repo)?;
    let versions = repo_versions(&tags);

    if versions.is_empty() {
        eprintln!("no published versions found in {repo}");
        return Ok(());
    }

    let latest = versions[0].version.clone();

    // Read annotations from this host's platform build when available —
    // multiarch index manifests carry no annotations — else the bare tag.
    let platform_tag = format!("{latest}-{}", super::pull::current_platform());
    let annotation_tag = if tags.iter().any(|t| *t == platform_tag) {
        platform_tag
    } else {
        latest.clone()
    };
    let annotations = match oci::fetch_manifest_annotations(&format!("{repo}:{annotation_tag}")) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("  (could not fetch manifest annotations: {e})");
            BTreeMap::new()
        }
    };

    let name = annotations
        .get("skillforge.skill.name")
        .cloned()
        .unwrap_or_else(|| repo.rsplit('/').next().unwrap_or(repo).to_string());

    println!("{name}  v{latest}");
    println!();

    if let Some(desc) = annotations.get("org.opencontainers.image.description") {
        println!("  {desc}");
        println!();
    }
    if let Some(interfaces) = annotations.get("skillforge.skill.interfaces") {
        println!("  interfaces: {interfaces}");
    }
    if let Some(license) = annotations.get("org.opencontainers.image.licenses") {
        println!("  license:    {license}");
    }
    println!("  source:     {repo}");

    println!();
    println!("  available versions:");
    for v in &versions {
        if v.platforms.is_empty() {
            println!("    {}", v.version);
        } else {
            println!("    {}  ({})", v.version, v.platforms.join(", "));
        }
    }

    println!();
    println!("  install latest:");
    println!("    skillforge add {repo}:{latest}");
    println!("  install a specific version:");
    println!("    skillforge add {repo}:<version>");

    Ok(())
}

/// A published version and the platforms it has dedicated builds for.
struct RepoVersion {
    version: String,
    platforms: Vec<String>,
}

/// Normalize raw repo tags into a newest-first list of published versions.
/// Drops `latest`, maps platform-specific tags (`0.2.0-darwin-arm64`) onto
/// their base version, and records which platforms each version covers.
fn repo_versions(tags: &[String]) -> Vec<RepoVersion> {
    let mut by_version: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for tag in tags {
        if tag == "latest" {
            continue;
        }
        let (base, platform) = match tag.split_once('-') {
            Some((b, p)) => (b, Some(p)),
            None => (tag.as_str(), None),
        };
        if !oci::is_bare_semver(base) {
            continue;
        }
        let platforms = by_version.entry(base.to_string()).or_default();
        if let Some(p) = platform {
            platforms.push(p.to_string());
        }
    }
    let mut versions: Vec<RepoVersion> = by_version
        .into_iter()
        .map(|(version, platforms)| RepoVersion { version, platforms })
        .collect();
    versions.sort_by(|a, b| cmp_semver(&b.version, &a.version));
    versions
}

struct CatalogEntry {
    name: String,
    version: String,
    tag: String,
}

fn parse_catalog_tags(tags: &[String]) -> Vec<CatalogEntry> {
    tags.iter()
        .filter_map(|tag| {
            let (name, version) = tag.rsplit_once("--")?;
            if name.is_empty() || version.is_empty() {
                return None;
            }
            Some(CatalogEntry {
                name: name.to_string(),
                version: version.to_string(),
                tag: tag.clone(),
            })
        })
        .collect()
}

fn semver_gt(a: &str, b: &str) -> bool {
    cmp_semver(a, b) == std::cmp::Ordering::Greater
}

fn sort_entries_by_version_desc(entries: &mut [&CatalogEntry]) {
    entries.sort_by(|a, b| cmp_semver(&b.version, &a.version));
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
    use super::{parse_catalog_tags, repo_versions, sort_entries_by_version_desc};

    #[test]
    fn repo_versions_normalize_platform_tags_and_drop_latest() {
        let versions = repo_versions(&[
            "0.1.0".to_string(),
            "0.2.0".to_string(),
            "0.2.0-darwin-arm64".to_string(),
            "0.2.0-linux-amd64".to_string(),
            "latest".to_string(),
        ]);

        let names: Vec<&str> = versions.iter().map(|v| v.version.as_str()).collect();
        assert_eq!(names, ["0.2.0", "0.1.0"]);
        assert_eq!(versions[0].platforms, ["darwin-arm64", "linux-amd64"]);
    }

    #[test]
    fn repo_versions_skip_non_semver_tags() {
        let versions = repo_versions(&["nightly".to_string(), "latest".to_string()]);
        assert!(versions.is_empty());
    }

    #[test]
    fn detail_versions_are_sorted_from_latest_to_oldest() {
        let entries = parse_catalog_tags(&[
            "word-count--0.1.0".to_string(),
            "word-count--1.0.0".to_string(),
            "word-count--0.2.0".to_string(),
        ]);
        let mut versions: Vec<_> = entries.iter().collect();

        sort_entries_by_version_desc(&mut versions);

        assert_eq!(
            versions
                .iter()
                .map(|entry| entry.version.as_str())
                .collect::<Vec<_>>(),
            ["1.0.0", "0.2.0", "0.1.0"]
        );
    }
}
