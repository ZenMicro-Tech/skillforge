//! Persistent registry of installed skills + mux state.
//!
//! Single file at `$SKILLFORGE_HOME/registry.json` (default `~/.skillforge/registry.json`).
//! Schema version 1 — bump when changing the on-disk shape.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registry {
    pub version: u32,
    #[serde(default)]
    pub mux: MuxState,
    #[serde(default)]
    pub skills: BTreeMap<String, SkillEntry>,
}

impl Default for Registry {
    fn default() -> Self {
        Self {
            version: 1,
            mux: MuxState::default(),
            skills: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MuxState {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub agents: Vec<String>,
}

/// Where an installed skill came from. Recorded at install time so both
/// `list` and `upgrade` know the provenance explicitly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillSource {
    /// Pulled from an OCI registry repository (e.g.
    /// `ghcr.io/acme/skills/word-count`).
    Oci(String),
    /// Installed from a local directory on disk.
    Local,
}

impl Serialize for SkillSource {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            SkillSource::Oci(repo) => serializer.serialize_str(repo),
            SkillSource::Local => serializer.serialize_str("local"),
        }
    }
}

impl<'de> Deserialize<'de> for SkillSource {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(if s == "local" {
            SkillSource::Local
        } else {
            SkillSource::Oci(s)
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEntry {
    pub version: String,
    pub binary: PathBuf,
    pub source_dir: PathBuf,
    pub description: String,
    pub input_schema: Value,
    /// Where this skill was installed from. `None` only for entries written
    /// before source tracking existed; new installs always record a source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SkillSource>,
}

pub fn home() -> PathBuf {
    if let Some(custom) = std::env::var_os("SKILLFORGE_HOME") {
        return PathBuf::from(custom);
    }
    let home = std::env::var_os("HOME").expect("HOME must be set");
    PathBuf::from(home).join(".skillforge")
}

pub fn path() -> PathBuf {
    home().join("registry.json")
}

pub fn load() -> Result<Registry> {
    let p = path();
    if !p.exists() {
        return Ok(Registry::default());
    }
    let text = std::fs::read_to_string(&p)
        .with_context(|| format!("reading {}", p.display()))?;
    if text.trim().is_empty() {
        return Ok(Registry::default());
    }
    serde_json::from_str(&text).with_context(|| format!("parsing {}", p.display()))
}

pub fn save(reg: &Registry) -> Result<()> {
    let p = path();
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("mkdir {}", parent.display()))?;
    }
    let tmp = p.with_extension("skillforge.tmp");
    let pretty = serde_json::to_string_pretty(reg)?;
    std::fs::write(&tmp, pretty)
        .with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, &p)
        .with_context(|| format!("renaming into {}", p.display()))?;
    Ok(())
}

pub fn upsert_skill(name: &str, entry: SkillEntry) -> Result<()> {
    let mut reg = load()?;
    reg.skills.insert(name.to_string(), entry);
    save(&reg)
}

pub fn remove_skill(name: &str) -> Result<bool> {
    let mut reg = load()?;
    let existed = reg.skills.remove(name).is_some();
    if existed {
        save(&reg)?;
    }
    Ok(existed)
}

pub fn is_mux_enabled() -> bool {
    load().map(|r| r.mux.enabled).unwrap_or(false)
}

pub fn set_mux(enabled: bool, agents: &[String]) -> Result<()> {
    let mut reg = load()?;
    reg.mux.enabled = enabled;
    reg.mux.agents = agents.to_vec();
    save(&reg)
}

/// Build a SkillEntry from a skill source directory by reading its skill.toml + schema.json.
pub fn entry_from_dir(dir: &Path, binary: PathBuf) -> Result<(String, SkillEntry)> {
    let manifest = skillforge_core::Manifest::from_path(dir.join("skill.toml"))?;
    let schema_text = std::fs::read_to_string(dir.join("schema.json"))
        .with_context(|| format!("reading {}/schema.json", dir.display()))?;
    let input_schema: Value = serde_json::from_str(&schema_text)
        .with_context(|| format!("parsing {}/schema.json", dir.display()))?;
    let entry = SkillEntry {
        version: manifest.skill.version,
        binary,
        source_dir: dir.to_path_buf(),
        description: manifest.skill.description,
        input_schema,
        source: None,
    };
    Ok((manifest.skill.name, entry))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_written_before_source_tracking_still_load() {
        let json = r#"{
            "version": "0.1.0",
            "binary": "/tmp/word-count",
            "source_dir": "/tmp/word-count-src",
            "description": "Count words",
            "input_schema": {}
        }"#;
        let entry: SkillEntry = serde_json::from_str(json).expect("deserialize legacy entry");
        assert_eq!(entry.source, None);
    }

    #[test]
    fn oci_source_round_trips_as_a_plain_string() {
        let json = r#"{
            "version": "0.1.0",
            "binary": "/tmp/word-count",
            "source_dir": "/tmp/word-count-src",
            "description": "Count words",
            "input_schema": {},
            "source": "ghcr.io/acme/skills/word-count"
        }"#;
        let entry: SkillEntry = serde_json::from_str(json).expect("deserialize");
        assert_eq!(
            entry.source,
            Some(SkillSource::Oci(
                "ghcr.io/acme/skills/word-count".to_string()
            ))
        );
        let serialized = serde_json::to_string(&entry).expect("serialize");
        assert!(serialized.contains(r#""source":"ghcr.io/acme/skills/word-count""#));
    }

    #[test]
    fn local_source_round_trips() {
        let json = r#"{
            "version": "0.1.0",
            "binary": "/tmp/word-count",
            "source_dir": "/tmp/word-count-src",
            "description": "Count words",
            "input_schema": {},
            "source": "local"
        }"#;
        let entry: SkillEntry = serde_json::from_str(json).expect("deserialize");
        assert_eq!(entry.source, Some(SkillSource::Local));
        let serialized = serde_json::to_string(&entry).expect("serialize");
        assert!(serialized.contains(r#""source":"local""#));
    }

    #[test]
    fn absent_source_is_not_serialized() {
        let entry = SkillEntry {
            version: "0.1.0".to_string(),
            binary: PathBuf::new(),
            source_dir: PathBuf::new(),
            description: String::new(),
            input_schema: Value::Null,
            source: None,
        };
        let serialized = serde_json::to_string(&entry).expect("serialize");
        assert!(!serialized.contains("\"source\":"));
    }
}
