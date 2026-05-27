use anyhow::Result;

use crate::registry;
use crate::sources;

pub fn add(name_or_ref: &str) -> Result<()> {
    let dir = if is_oci_ref(name_or_ref) {
        super::pull::fetch_oci(name_or_ref)?
    } else {
        sources::resolve(name_or_ref)?
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
