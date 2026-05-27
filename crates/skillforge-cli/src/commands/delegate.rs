use anyhow::{bail, Result};
use std::process::Command;

pub fn run(path: Option<&str>, mode: &str, extra: &[String]) -> Result<()> {
    let dir = super::resolve_skill_dir(path);
    let manifest = super::load_manifest(&dir)?;
    let bin = dir.join("target/release").join(&manifest.skill.name);
    if !bin.exists() {
        bail!(
            "binary {} not found — run `skillforge build` first",
            bin.display()
        );
    }
    let status = Command::new(&bin).arg(mode).args(extra).status()?;
    if !status.success() {
        bail!("skill exited with {status}");
    }
    Ok(())
}
