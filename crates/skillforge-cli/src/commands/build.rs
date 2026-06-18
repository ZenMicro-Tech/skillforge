use anyhow::{bail, Result};
use std::path::PathBuf;
use std::process::Command;

pub fn run(path: Option<&str>) -> Result<()> {
    run_for_target(path, None).map(|_| ())
}

pub fn run_for_target(path: Option<&str>, target: Option<&str>) -> Result<PathBuf> {
    let dir = super::resolve_skill_dir(path);
    let manifest = super::load_manifest(&dir)?;

    let bin = match target {
        Some(t) => dir.join("target").join(t).join("release").join(&manifest.skill.name),
        None => dir.join("target/release").join(&manifest.skill.name),
    };

    if !dir.join("Cargo.toml").is_file() {
        if !bin.exists() {
            bail!(
                "no Cargo.toml and no prebuilt binary at {} — nothing to build",
                bin.display()
            );
        }
        eprintln!("using prebuilt binary at {}", bin.display());
        return Ok(bin);
    }

    eprintln!("building skill {} v{}", manifest.skill.name, manifest.skill.version);
    let mut cmd = Command::new("cargo");
    cmd.arg("build").arg("--release");
    if let Some(t) = target {
        cmd.arg("--target").arg(t);
        eprintln!("  target: {t}");
    }
    let status = cmd.current_dir(&dir).status()?;
    if !status.success() {
        bail!("cargo build failed");
    }
    eprintln!("built {}", bin.display());
    Ok(bin)
}
