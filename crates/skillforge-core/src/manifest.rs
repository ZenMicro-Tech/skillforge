use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("failed to read manifest at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse manifest: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("invalid skill name {0:?}: must match [a-z][a-z0-9-]*")]
    InvalidName(String),
    #[error("invalid semver {0:?}: {1}")]
    InvalidVersion(String, String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub skill: Skill,
    pub runtime: Runtime,
    #[serde(default)]
    pub interfaces: Interfaces,
    #[serde(default)]
    pub publish: Option<Publish>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Publish {
    /// OCI registry namespace, e.g. "ghcr.io/owner/skills".
    /// The skill name is appended automatically: {registry}/{name}:{version}.
    pub registry: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub license: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Runtime {
    pub kind: RuntimeKind,
    pub entrypoint: String,
    #[serde(default)]
    pub determinism: Determinism,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeKind {
    Rust,
    WasmComponent,
    Script,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Determinism {
    #[default]
    Pure,
    IoBounded,
    LlmAssisted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interfaces {
    #[serde(default = "yes")]
    pub mcp: bool,
    #[serde(default = "yes")]
    pub cli: bool,
    #[serde(default = "yes")]
    pub http: bool,
    #[serde(default = "yes")]
    pub lib: bool,
}

impl Default for Interfaces {
    fn default() -> Self {
        Self {
            mcp: true,
            cli: true,
            http: true,
            lib: true,
        }
    }
}

fn yes() -> bool {
    true
}

impl Manifest {
    pub fn from_str(s: &str) -> Result<Self, ManifestError> {
        let m: Manifest = toml::from_str(s)?;
        m.validate()?;
        Ok(m)
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, ManifestError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|e| ManifestError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        Self::from_str(&text)
    }

    fn validate(&self) -> Result<(), ManifestError> {
        if !is_valid_name(&self.skill.name) {
            return Err(ManifestError::InvalidName(self.skill.name.clone()));
        }
        validate_semver(&self.skill.version)
            .map_err(|msg| ManifestError::InvalidVersion(self.skill.version.clone(), msg))?;
        Ok(())
    }
}

fn is_valid_name(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn validate_semver(s: &str) -> Result<(), String> {
    let core = s.split(['-', '+']).next().unwrap_or("");
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() != 3 {
        return Err("expected MAJOR.MINOR.PATCH".to_string());
    }
    for p in parts {
        if p.is_empty() || !p.chars().all(|c| c.is_ascii_digit()) {
            return Err(format!("non-numeric component {p:?}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[skill]
name = "pdf-extract"
version = "1.4.2"
description = "Extract structured text from PDFs."
license = "Apache-2.0"

[runtime]
kind = "rust"
entrypoint = "src/main.rs"
determinism = "pure"
"#;

    #[test]
    fn parses_sample_manifest() {
        let m = Manifest::from_str(SAMPLE).expect("parse");
        assert_eq!(m.skill.name, "pdf-extract");
        assert_eq!(m.skill.version, "1.4.2");
        assert_eq!(m.runtime.kind, RuntimeKind::Rust);
        assert_eq!(m.runtime.determinism, Determinism::Pure);
        assert!(m.interfaces.mcp);
    }

    #[test]
    fn rejects_invalid_name() {
        let bad = SAMPLE.replace(r#"name = "pdf-extract""#, r#"name = "PDF_Extract""#);
        assert!(matches!(
            Manifest::from_str(&bad),
            Err(ManifestError::InvalidName(_))
        ));
    }

    #[test]
    fn rejects_invalid_version() {
        let bad = SAMPLE.replace(r#"version = "1.4.2""#, r#"version = "1.4""#);
        assert!(matches!(
            Manifest::from_str(&bad),
            Err(ManifestError::InvalidVersion(_, _))
        ));
    }

    #[test]
    fn interfaces_default_to_all_enabled() {
        let m = Manifest::from_str(SAMPLE).expect("parse");
        assert!(m.interfaces.mcp && m.interfaces.cli && m.interfaces.http && m.interfaces.lib);
    }
}
