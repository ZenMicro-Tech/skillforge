pub mod add;
pub mod build;
pub mod delegate;
pub mod link;
pub mod mux;
pub mod new;
pub mod publish;
pub mod pull;
pub mod search;
pub mod upgrade;

use anyhow::{Context, Result};
use skillforge_core::Manifest;
use std::path::{Path, PathBuf};

pub fn resolve_skill_dir(path: Option<&str>) -> PathBuf {
    path.map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().expect("cwd"))
}

pub fn load_manifest(dir: &Path) -> Result<Manifest> {
    let manifest_path = dir.join("skill.toml");
    Manifest::from_path(&manifest_path)
        .with_context(|| format!("loading {}", manifest_path.display()))
}
