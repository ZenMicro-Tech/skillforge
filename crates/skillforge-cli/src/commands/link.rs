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
    println!("{:<20} {:<10} {}", "NAME", "VERSION", "DESCRIPTION");
    println!("{:<20} {:<10} {}", "----", "-------", "-----------");
    for (name, entry) in &reg.skills {
        println!("{:<20} {:<10} {}", name, entry.version, entry.description);
    }
    println!("\n{} skill(s) installed.", reg.skills.len());
    Ok(())
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
