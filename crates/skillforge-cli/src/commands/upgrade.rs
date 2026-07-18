//! `skillforge upgrade` — check the OCI catalog for newer versions of
//! installed skills and upgrade them.

use anyhow::{Context, Result};

use crate::oci;
use crate::registry;

const DEFAULT_CATALOG: &str = "ghcr.io/zenmicro-tech/skillforge/skills/catalog";

pub fn upgrade(name: Option<&str>, check_only: bool) -> Result<()> {
    let reg = registry::load()?;

    if reg.skills.is_empty() {
        eprintln!("No skills installed.");
        eprintln!("  Use `skillforge add <name>` to install a skill.");
        return Ok(());
    }

    // Build the set of skills to check
    let targets: Vec<(String, registry::SkillEntry)> = if let Some(n) = name {
        match reg.skills.get(n) {
            Some(entry) => vec![(n.to_string(), entry.clone())],
            None => {
                eprintln!("skill \"{n}\" is not installed");
                return Ok(());
            }
        }
    } else {
        reg.skills
            .iter()
            .map(|(n, e)| (n.clone(), e.clone()))
            .collect()
    };

    // Fetch the catalog tags once
    eprintln!("fetching catalog from {DEFAULT_CATALOG}...");
    let tags = oci::list_tags(DEFAULT_CATALOG)
        .context("fetching skill catalog")?;

    let catalog_entries = parse_catalog_tags(&tags);

    let mut upgraded = 0u32;
    let mut up_to_date = 0u32;

    for (skill_name, entry) in &targets {
        let current_version = &entry.version;

        // Find the latest version for this skill in the catalog
        let latest = catalog_entries
            .iter()
            .filter(|e| e.name == *skill_name)
            .max_by(|a, b| cmp_semver(&a.version, &b.version));

        let Some(latest) = latest else {
            eprintln!(
                "  · {skill_name} v{current_version} — not found in catalog (skipping)"
            );
            continue;
        };

        if cmp_semver(&latest.version, current_version) != std::cmp::Ordering::Greater {
            up_to_date += 1;
            if name.is_some() {
                eprintln!(
                    "  ✓ {skill_name} v{current_version} is already up to date"
                );
            }
            continue;
        }

        eprintln!(
            "  ↑ {skill_name} v{current_version} → v{}",
            latest.version
        );

        if check_only {
            upgraded += 1;
            continue;
        }

        // Resolve the OCI repo reference from the catalog entry annotations
        let catalog_ref = format!("{DEFAULT_CATALOG}:{}", latest.tag);
        let annotations = match oci::fetch_manifest_annotations(&catalog_ref) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("    ✗ failed to fetch metadata: {e}");
                continue;
            }
        };

        let repo = match annotations.get("skillforge.skill.repo") {
            Some(r) => r.clone(),
            None => {
                eprintln!("    ✗ catalog entry missing repo reference");
                continue;
            }
        };

        let reference = format!("{repo}:{}", latest.version);
        eprintln!("    pulling {reference}...");

        // Pull the new version
        let dir = match super::pull::fetch_oci(&reference) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("    ✗ pull failed: {e}");
                continue;
            }
        };

        let dir_str = dir.display().to_string();

        // Build the new binary
        if let Err(e) = super::build::run(Some(&dir_str)) {
            eprintln!("    ✗ build failed: {e}");
            continue;
        }

        // Update the registry entry
        let manifest =
            match skillforge_core::Manifest::from_path(dir.join("skill.toml")) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("    ✗ failed to read manifest: {e}");
                    continue;
                }
            };

        let bin = match std::fs::canonicalize(
            dir.join("target/release").join(&manifest.skill.name),
        ) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("    ✗ binary not found after build: {e}");
                continue;
            }
        };

        let (_, new_entry) = registry::entry_from_dir(&dir, bin)?;
        registry::upsert_skill(skill_name, new_entry)?;

        // Re-link if not in mux mode
        if !registry::is_mux_enabled() {
            let _ = super::link::link(Some(&dir_str), None);
        }

        eprintln!("    ✓ upgraded to v{}", latest.version);
        upgraded += 1;
    }

    if check_only {
        if upgraded == 0 {
            eprintln!("\nAll {up_to_date} installed skill(s) are up to date.");
        } else {
            eprintln!(
                "\n{upgraded} upgrade(s) available. Run `skillforge upgrade` to apply."
            );
        }
    } else {
        if upgraded == 0 && up_to_date > 0 {
            eprintln!("\nAll {up_to_date} installed skill(s) are up to date.");
        } else if upgraded > 0 {
            eprintln!("\n{upgraded} skill(s) upgraded.");
        }
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