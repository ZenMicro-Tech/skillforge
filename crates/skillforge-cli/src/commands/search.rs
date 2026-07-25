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
    use super::{parse_catalog_tags, sort_entries_by_version_desc};

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
