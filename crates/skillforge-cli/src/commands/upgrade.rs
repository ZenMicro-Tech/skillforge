//! `skillforge upgrade` — check for newer versions of installed skills and
//! upgrade them.
//!
//! Skills installed from an OCI reference record their source repository in
//! the registry (`SkillEntry::source`); those are checked against their own
//! repo's tag list, which is authoritative — catalog entries are best-effort
//! and can be missing. Only legacy installs with no recorded source fall
//! back to the default catalog.

use anyhow::{Context, Result};

use crate::oci;
use crate::registry;

const DEFAULT_CATALOG: &str = "ghcr.io/zenmicro-tech/skillforge/skills/catalog";

pub fn upgrade(name: Option<&str>, check_only: bool, verbose: bool) -> Result<()> {
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

    // Fetch the default catalog tags once, but only if at least one target
    // resolves through it — skills with a custom source repo are checked
    // against that repo directly and shouldn't fail if the default catalog
    // is unreachable.
    let needs_catalog = targets
        .iter()
        .any(|(_, e)| matches!(resolution_for(e), Resolution::Catalog));
    let catalog_entries = if needs_catalog {
        eprintln!("fetching catalog from {DEFAULT_CATALOG}...");
        let tags = oci::list_tags(DEFAULT_CATALOG).context("fetching skill catalog")?;
        Some(parse_catalog_tags(&tags))
    } else {
        None
    };

    let mut upgraded = 0u32;
    let mut up_to_date = 0u32;
    let mut skipped = 0u32;

    for (skill_name, entry) in &targets {
        let current_version = &entry.version;

        if matches!(resolution_for(entry), Resolution::Local) {
            eprintln!(
                "  · {skill_name} — installed from disk; no registry to check (skipping)"
            );
            skipped += 1;
            continue;
        }

        // Find the latest available version for this skill, either in its
        // recorded source repo or in the default catalog.
        let Some(latest) = find_latest(skill_name, entry, catalog_entries.as_deref()) else {
            skipped += 1;
            continue;
        };

        if cmp_semver(latest.version(), current_version) != std::cmp::Ordering::Greater {
            up_to_date += 1;
            if verbose || name.is_some() {
                eprintln!(
                    "  ✓ {skill_name} v{current_version} is already up to date"
                );
            }
            continue;
        }

        eprintln!(
            "  ↑ {skill_name} v{current_version} → v{}",
            latest.version()
        );

        if check_only {
            upgraded += 1;
            continue;
        }

        // Resolve the exact reference to pull. Repo-sourced skills already
        // know it; catalog entries resolve their repo via the catalog
        // entry's annotations (deferred to here so up-to-date checks stay
        // cheap).
        let reference = match &latest {
            Latest::Repo { reference, .. } => reference.clone(),
            Latest::Catalog { version, tag } => {
                let catalog_ref = format!("{DEFAULT_CATALOG}:{tag}");
                let annotations = match oci::fetch_manifest_annotations(&catalog_ref) {
                    Ok(a) => a,
                    Err(e) => {
                        eprintln!("    ✗ failed to fetch metadata: {e}");
                        continue;
                    }
                };
                match annotations.get("skillforge.skill.repo") {
                    Some(repo) => format!("{repo}:{version}"),
                    None => {
                        eprintln!("    ✗ catalog entry missing repo reference");
                        continue;
                    }
                }
            }
        };

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

        let (_, mut new_entry) = registry::entry_from_dir(&dir, bin)?;
        // Preserve the recorded source so future upgrades keep checking the
        // same registry.
        new_entry.source = entry.source.clone();
        registry::upsert_skill(skill_name, new_entry)?;

        // Re-link if not in mux mode
        if !registry::is_mux_enabled() {
            let _ = super::link::link(Some(&dir_str), None);
        }

        eprintln!("    ✓ upgraded to v{}", latest.version());
        upgraded += 1;
    }

    let skipped_note = if skipped > 0 {
        format!(" ({skipped} skipped)")
    } else {
        String::new()
    };

    if check_only {
        if upgraded > 0 {
            eprintln!(
                "\n{upgraded} upgrade(s) available. Run `skillforge upgrade` to apply.{skipped_note}"
            );
        } else if up_to_date > 0 {
            eprintln!("\nAll {up_to_date} installed skill(s) are up to date.{skipped_note}");
        } else {
            eprintln!("\nNo upgrades found.{skipped_note}");
        }
    } else {
        if upgraded > 0 {
            eprintln!("\n{upgraded} skill(s) upgraded.{skipped_note}");
        } else if up_to_date > 0 {
            eprintln!("\nAll {up_to_date} installed skill(s) are up to date.{skipped_note}");
        } else if skipped > 0 {
            eprintln!("\nNo skills could be checked.{skipped_note}");
        }
    }

    Ok(())
}

/// Where to look for newer versions of an installed skill.
enum Resolution {
    /// The skill's own OCI repository (any install with a recorded source).
    Repo(String),
    /// The shared default catalog (legacy installs with no recorded source).
    Catalog,
    /// Installed from disk — there is no remote source to check.
    Local,
}

fn resolution_for(entry: &registry::SkillEntry) -> Resolution {
    match &entry.source {
        Some(registry::SkillSource::Oci(repo)) => Resolution::Repo(repo.clone()),
        Some(registry::SkillSource::Local) => Resolution::Local,
        None => Resolution::Catalog,
    }
}

/// The latest available version found for a skill.
enum Latest {
    /// Found in the skill's own repo; the exact pull reference is known.
    Repo { version: String, reference: String },
    /// Found in the default catalog; the pull reference is resolved from the
    /// catalog entry's annotations only when an upgrade is actually applied.
    Catalog { version: String, tag: String },
}

impl Latest {
    fn version(&self) -> &str {
        match self {
            Latest::Repo { version, .. } | Latest::Catalog { version, .. } => version,
        }
    }
}

/// Find the latest available version of a skill. Skills with a recorded
/// custom source repo are checked against that repo's tag list directly;
/// everything else is looked up in the default catalog.
fn find_latest(
    skill_name: &str,
    entry: &registry::SkillEntry,
    catalog: Option<&[CatalogEntry]>,
) -> Option<Latest> {
    match resolution_for(entry) {
        Resolution::Local => {
            // Handled before find_latest is called; nothing to check remotely.
            None
        }
        Resolution::Repo(repo) => {
            let tags = match oci::list_tags(&repo) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("  · {skill_name} — failed to query {repo}: {e} (skipping)");
                    return None;
                }
            };
            match pick_latest_tag(&tags, &super::pull::current_platform()) {
                Some(tag) => Some(Latest::Repo {
                    version: oci::base_version(&tag).to_string(),
                    reference: format!("{repo}:{tag}"),
                }),
                None => {
                    eprintln!(
                        "  · {skill_name} v{} — no versions found in {repo} (skipping)",
                        entry.version
                    );
                    None
                }
            }
        }
        Resolution::Catalog => {
            let latest = catalog
                .unwrap_or(&[])
                .iter()
                .filter(|e| e.name == skill_name)
                .max_by(|a, b| cmp_semver(&a.version, &b.version));
            match latest {
                Some(e) => Some(Latest::Catalog {
                    version: e.version.clone(),
                    tag: e.tag.clone(),
                }),
                None => {
                    eprintln!(
                        "  · {skill_name} v{} — not found in catalog (skipping)",
                        entry.version
                    );
                    None
                }
            }
        }
    }
}

/// Pick the best tag to install from a repo's tag list: highest semver,
/// preferring a platform-specific build for `platform` when one exists.
fn pick_latest_tag(tags: &[String], platform: &str) -> Option<String> {
    let platform_suffix = format!("-{platform}");
    tags.iter()
        .filter(|t| t.ends_with(&platform_suffix))
        .max_by(|a, b| cmp_semver(a, b))
        .or_else(|| {
            tags.iter()
                .filter(|t| oci::is_bare_semver(t))
                .max_by(|a, b| cmp_semver(a, b))
        })
        .cloned()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn entry_with_source(source: Option<&str>) -> registry::SkillEntry {
        registry::SkillEntry {
            version: "0.1.0".to_string(),
            binary: PathBuf::new(),
            source_dir: PathBuf::new(),
            description: String::new(),
            input_schema: serde_json::Value::Null,
            source: source.map(|s| registry::SkillSource::Oci(s.to_string())),
        }
    }

    #[test]
    fn skills_without_recorded_source_use_the_default_catalog() {
        assert!(matches!(
            resolution_for(&entry_with_source(None)),
            Resolution::Catalog
        ));
    }

    #[test]
    fn local_installs_have_no_remote_resolution() {
        let mut entry = entry_with_source(None);
        entry.source = Some(registry::SkillSource::Local);
        assert!(matches!(resolution_for(&entry), Resolution::Local));
    }

    #[test]
    fn any_recorded_source_is_checked_against_its_own_repo() {
        // Repo tags are authoritative — catalog entries are best-effort and
        // can be missing even for default-namespace skills.
        for repo in [
            "ghcr.io/acme/skills/word-count",
            "ghcr.io/zenmicro-tech/skillforge/skills/word-count",
        ] {
            match resolution_for(&entry_with_source(Some(repo))) {
                Resolution::Repo(r) => assert_eq!(r, repo),
                _ => panic!("expected repo resolution for {repo}"),
            }
        }
    }

    #[test]
    fn pick_latest_tag_prefers_platform_specific_builds() {
        let tags = vec![
            "0.1.0".to_string(),
            "0.2.0".to_string(),
            "0.2.0-darwin-arm64".to_string(),
            "0.2.0-linux-amd64".to_string(),
            "latest".to_string(),
        ];
        assert_eq!(
            pick_latest_tag(&tags, "darwin-arm64").as_deref(),
            Some("0.2.0-darwin-arm64")
        );
        assert_eq!(
            pick_latest_tag(&tags, "windows-amd64").as_deref(),
            Some("0.2.0")
        );
    }

    #[test]
    fn pick_latest_tag_ignores_non_semver_tags() {
        let tags = vec!["latest".to_string(), "nightly".to_string()];
        assert_eq!(pick_latest_tag(&tags, "darwin-arm64"), None);
    }
}