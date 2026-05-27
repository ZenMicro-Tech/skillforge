//! OCI fetch: download a skill artifact and stage it on disk at
//! `~/.skillforge/skills/<name>/` so the rest of the `add` flow finds it
//! through the regular search path.
//!
//! Implementation note: uses the `oci-client` Rust crate directly (no
//! external `oras` CLI dependency) so the skillforge binary is self-contained.

use anyhow::{anyhow, bail, Context, Result};
use std::path::PathBuf;

use crate::oci;
use crate::registry;

/// Download an OCI skill artifact and return the directory containing the
/// staged skill (with `target/release/<name>` already chmod'ed +x).
pub fn fetch_oci(reference: &str) -> Result<PathBuf> {
    let stage = std::env::temp_dir().join(format!("skillforge-pull-{}", std::process::id()));
    if stage.exists() {
        std::fs::remove_dir_all(&stage)?;
    }
    std::fs::create_dir_all(&stage)?;

    eprintln!("pulling {reference}");
    let result = oci::pull(reference, &stage)?;
    eprintln!("  manifest digest: {}", result.manifest_digest);

    let manifest_path = stage.join("skill.toml");
    if !manifest_path.is_file() {
        bail!("pulled artifact missing skill.toml — is {reference} a skillforge skill?");
    }
    let manifest = skillforge_core::Manifest::from_path(&manifest_path)?;
    let skill_name = manifest.skill.name.clone();

    let bin_src = stage.join(&skill_name);
    if !bin_src.is_file() {
        bail!("pulled artifact missing binary `{skill_name}`");
    }

    let dest_dir = registry::home().join("skills").join(&skill_name);
    if dest_dir.exists() {
        std::fs::remove_dir_all(&dest_dir)
            .with_context(|| format!("removing existing {}", dest_dir.display()))?;
    }
    let bin_dest_dir = dest_dir.join("target/release");
    std::fs::create_dir_all(&bin_dest_dir)?;

    move_or_copy(&bin_src, &bin_dest_dir.join(&skill_name))?;
    for f in ["skill.toml", "prompt.md", "schema.json"] {
        let src = stage.join(f);
        if src.is_file() {
            move_or_copy(&src, &dest_dir.join(f))?;
        }
    }

    chmod_executable(&bin_dest_dir.join(&skill_name))?;

    let _ = std::fs::remove_dir_all(&stage);

    eprintln!(
        "✓ pulled {skill_name} v{} → {}",
        manifest.skill.version,
        dest_dir.display()
    );
    Ok(dest_dir)
}

fn move_or_copy(src: &PathBuf, dest: &PathBuf) -> Result<()> {
    if std::fs::rename(src, dest).is_ok() {
        return Ok(());
    }
    std::fs::copy(src, dest)
        .map(|_| ())
        .with_context(|| format!("copying {} → {}", src.display(), dest.display()))
}

#[cfg(unix)]
fn chmod_executable(path: &PathBuf) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(perms.mode() | 0o111);
    std::fs::set_permissions(path, perms)
        .map_err(|e| anyhow!("chmod +x {}: {e}", path.display()))
}

#[cfg(not(unix))]
fn chmod_executable(_path: &PathBuf) -> Result<()> {
    Ok(())
}
