use crate::adapters::{self, SkillRef, Status};
use crate::registry;
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

pub fn link(path: Option<&str>, only: Option<&[String]>) -> Result<()> {
    let (dir, name, binary) = resolve(path)?;
    let skill = SkillRef {
        name: &name,
        binary: &binary,
    };
    eprintln!(
        "linking {} (from {}) to installed agents",
        skill.name,
        dir.display()
    );

    let mut any = false;
    for agent in adapters::all() {
        if let Some(filter) = only {
            if !filter.iter().any(|s| s == agent.id()) {
                continue;
            }
        }
        if !agent.detect() {
            continue;
        }
        any = true;
        match agent.install(&skill) {
            Ok(Status::Installed) => eprintln!("  ✓ {} — linked", agent.display_name()),
            Ok(Status::Skipped(reason)) => {
                eprintln!("  · {} — skipped ({reason})", agent.display_name())
            }
            Ok(Status::NotInstalled) => {}
            Err(e) => eprintln!("  ✗ {} — {e}", agent.display_name()),
        }
    }
    if !any {
        eprintln!("no supported agents detected");
    }
    Ok(())
}

pub fn unlink(path: Option<&str>, only: Option<&[String]>) -> Result<()> {
    let (_dir, name, binary) = resolve(path)?;
    let skill = SkillRef {
        name: &name,
        binary: &binary,
    };
    eprintln!("unlinking {} from installed agents", skill.name);

    for agent in adapters::all() {
        if let Some(filter) = only {
            if !filter.iter().any(|s| s == agent.id()) {
                continue;
            }
        }
        if !agent.detect() {
            continue;
        }
        match agent.uninstall(&skill) {
            Ok(Status::NotInstalled) => eprintln!("  ✓ {} — removed", agent.display_name()),
            Ok(Status::Skipped(reason)) => {
                eprintln!("  · {} — skipped ({reason})", agent.display_name())
            }
            Ok(Status::Installed) => {}
            Err(e) => eprintln!("  ✗ {} — {e}", agent.display_name()),
        }
    }
    Ok(())
}

pub fn list(path: Option<&str>) -> Result<()> {
    if let Some(p) = path {
        return list_detail(Some(p));
    }
    list_all()
}

fn list_all() -> Result<()> {
    let reg = registry::load()?;
    if reg.skills.is_empty() {
        println!("No skills installed.");
        println!("  Use `skillforge add <name>` to install a skill.");
        return Ok(());
    }
    println!(
        "{:<20} {:<10} {:<40} {}",
        "NAME", "VERSION", "DESCRIPTION", "SOURCE"
    );
    println!(
        "{:<20} {:<10} {:<40} {}",
        "----", "-------", "-----------", "------"
    );
    for (name, entry) in &reg.skills {
        let staged_dir = registry::home().join("skills");
        let source = display_source(name, entry, &staged_dir);
        println!(
            "{:<20} {:<10} {:<40} {}",
            name, entry.version, entry.description, source
        );
    }
    println!("\n{} skill(s) installed.", reg.skills.len());
    Ok(())
}

const DEFAULT_REGISTRY: &str = "ghcr.io/zenmicro-tech/skillforge/skills";

/// What to show in the SOURCE column. Recorded sources are shown verbatim.
/// Entries predating source tracking need inference: registry pulls are
/// staged under `~/.skillforge/skills/` (and before tracking, almost always
/// came from the default registry), anything else was installed from disk.
fn display_source(
    name: &str,
    entry: &registry::SkillEntry,
    staged_dir: &std::path::Path,
) -> String {
    match &entry.source {
        Some(registry::SkillSource::Oci(repo)) => repo.clone(),
        Some(registry::SkillSource::Local) => "local".to_string(),
        None if entry.source_dir.starts_with(staged_dir) => {
            format!("{DEFAULT_REGISTRY}/{name} (assumed)")
        }
        None => "local".to_string(),
    }
}

fn list_detail(path: Option<&str>) -> Result<()> {
    let (_dir, name, binary) = resolve(path)?;
    let skill = SkillRef {
        name: &name,
        binary: &binary,
    };
    println!("skill: {}", skill.name);
    println!("binary: {}", binary.display());
    println!("agents:");
    for agent in adapters::all() {
        let detected = agent.detect();
        let linked = if detected {
            agent.is_linked(&skill).unwrap_or(false)
        } else {
            false
        };
        let mark = match (detected, linked) {
            (true, true) => "linked",
            (true, false) => "installed, not linked",
            (false, _) => "not installed",
        };
        println!("  - {:<30} {}", agent.display_name(), mark);
    }
    Ok(())
}

fn resolve(path: Option<&str>) -> Result<(PathBuf, String, PathBuf)> {
    let dir = super::resolve_skill_dir(path);
    let manifest = super::load_manifest(&dir)?;
    let bin = binary_path(&dir, &manifest.skill.name);
    if !bin.exists() {
        bail!(
            "binary {} not found — run `skillforge build` first",
            bin.display()
        );
    }
    let bin = std::fs::canonicalize(&bin)?;
    Ok((dir, manifest.skill.name, bin))
}

fn binary_path(dir: &Path, name: &str) -> PathBuf {
    dir.join("target/release").join(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(source: Option<&str>, source_dir: &str) -> registry::SkillEntry {
        registry::SkillEntry {
            version: "0.1.0".to_string(),
            binary: PathBuf::new(),
            source_dir: PathBuf::from(source_dir),
            description: String::new(),
            input_schema: serde_json::Value::Null,
            source: source.map(|s| registry::SkillSource::Oci(s.to_string())),
        }
    }

    #[test]
    fn recorded_sources_are_shown_verbatim() {
        let staged = Path::new("/home/u/.skillforge/skills");
        let entry = entry(Some("ghcr.io/acme/skills/word-count"), "/anywhere");
        assert_eq!(
            display_source("word-count", &entry, staged),
            "ghcr.io/acme/skills/word-count"
        );
    }

    #[test]
    fn staged_entries_without_source_assume_the_default_registry() {
        let staged = Path::new("/home/u/.skillforge/skills");
        let entry = entry(None, "/home/u/.skillforge/skills/word-count");
        assert_eq!(
            display_source("word-count", &entry, staged),
            "ghcr.io/zenmicro-tech/skillforge/skills/word-count (assumed)"
        );
    }

    #[test]
    fn unstaged_entries_without_source_are_local() {
        let staged = Path::new("/home/u/.skillforge/skills");
        let entry = entry(None, "/home/u/projects/word-count");
        assert_eq!(display_source("word-count", &entry, staged), "local");
    }

    #[test]
    fn tracked_local_sources_are_shown_as_local_without_assumption() {
        // Even if a local install happens to be staged under the skills dir,
        // an explicitly tracked local source wins over inference.
        let staged = Path::new("/home/u/.skillforge/skills");
        let mut entry = entry(None, "/home/u/.skillforge/skills/word-count");
        entry.source = Some(registry::SkillSource::Local);
        assert_eq!(display_source("word-count", &entry, staged), "local");
    }
}
