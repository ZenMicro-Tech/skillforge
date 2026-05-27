use anyhow::{bail, Result};
use std::process::Command;

pub fn run(path: Option<&str>) -> Result<()> {
    let dir = super::resolve_skill_dir(path);
    let manifest = super::load_manifest(&dir)?;
    let bin = dir.join("target/release").join(&manifest.skill.name);

    // Source-less skills (e.g. pulled from a registry) have no Cargo.toml — trust the binary.
    if !dir.join("Cargo.toml").is_file() {
        if !bin.exists() {
            bail!(
                "no Cargo.toml and no prebuilt binary at {} — nothing to build",
                bin.display()
            );
        }
        eprintln!("using prebuilt binary at {}", bin.display());
        return Ok(());
    }

    eprintln!("building skill {} v{}", manifest.skill.name, manifest.skill.version);
    let status = Command::new("cargo")
        .arg("build")
        .arg("--release")
        .current_dir(&dir)
        .status()?;
    if !status.success() {
        bail!("cargo build failed");
    }
    eprintln!("built {}", bin.display());
    Ok(())
}
