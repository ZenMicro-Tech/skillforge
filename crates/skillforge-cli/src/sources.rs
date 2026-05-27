//! Source resolution: turn `<name>` into a skill directory on disk.
//!
//! Phase 1 search path:
//!   1. `./skills/<name>/`     — for working in the platform repo
//!   2. `~/.skillforge/skills/<name>/` — user-installed skills
//!
//! Phase 2 will prepend a registry lookup that downloads into
//! `~/.skillforge/skills/<name>/` before falling through to (2).

use anyhow::{bail, Result};
use std::path::PathBuf;

pub fn resolve(name: &str) -> Result<PathBuf> {
    let candidates = search_path(name);
    for dir in &candidates {
        if dir.join("skill.toml").is_file() {
            return Ok(dir.clone());
        }
    }
    bail!(
        "skill {name:?} not found in any source.\nSearched:\n{}",
        candidates
            .iter()
            .map(|p| format!("  - {}", p.display()))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

fn search_path(name: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        out.push(cwd.join("skills").join(name));
    }
    if let Some(home) = std::env::var_os("HOME") {
        out.push(PathBuf::from(home).join(".skillforge/skills").join(name));
    }
    out
}
