//! Self-contained registry credential store, independent of Docker.
//!
//! Single file at `$SKILLFORGE_HOME/credentials.json` (default
//! `~/.skillforge/credentials.json`), written with `0600` permissions on
//! Unix. Populated by `skillforge login` / `skillforge logout` and consumed
//! by [`crate::oci`]'s auth resolution so publishing/pulling never requires
//! Docker or `oras` to be installed.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub registries: BTreeMap<String, Entry>,
}

impl Default for Credentials {
    fn default() -> Self {
        Self {
            version: default_version(),
            registries: BTreeMap::new(),
        }
    }
}

fn default_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub username: String,
    pub secret: String,
}

pub fn path() -> PathBuf {
    crate::registry::home().join("credentials.json")
}

pub fn load() -> Result<Credentials> {
    let p = path();
    if !p.exists() {
        return Ok(Credentials::default());
    }
    let text =
        std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?;
    if text.trim().is_empty() {
        return Ok(Credentials::default());
    }
    serde_json::from_str(&text).with_context(|| format!("parsing {}", p.display()))
}

pub fn save(creds: &Credentials) -> Result<()> {
    let p = path();
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("mkdir {}", parent.display()))?;
    }
    let tmp = p.with_extension("skillforge.tmp");
    let pretty = serde_json::to_string_pretty(creds)?;
    std::fs::write(&tmp, pretty).with_context(|| format!("writing {}", tmp.display()))?;
    restrict_permissions(&tmp);
    std::fs::rename(&tmp, &p).with_context(|| format!("renaming into {}", p.display()))?;
    Ok(())
}

#[cfg(unix)]
fn restrict_permissions(p: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_permissions(_p: &std::path::Path) {}

/// Look up stored credentials for a registry host (e.g. `ghcr.io`).
pub fn get(registry: &str) -> Option<Entry> {
    load().ok()?.registries.get(registry).cloned()
}

/// Store (or overwrite) credentials for a registry host.
pub fn set(registry: &str, username: &str, secret: &str) -> Result<()> {
    let mut creds = load()?;
    creds.registries.insert(
        registry.to_string(),
        Entry {
            username: username.to_string(),
            secret: secret.to_string(),
        },
    );
    save(&creds)
}

/// Remove stored credentials for a registry host. Returns whether an entry existed.
pub fn remove(registry: &str) -> Result<bool> {
    let mut creds = load()?;
    let existed = creds.registries.remove(registry).is_some();
    if existed {
        save(&creds)?;
    }
    Ok(existed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // SKILLFORGE_HOME is process-global state; serialize tests that touch it.
    static LOCK: Mutex<()> = Mutex::new(());

    fn with_temp_home<T>(f: impl FnOnce() -> T) -> T {
        let _guard = LOCK.lock().unwrap();
        let dir = tempdir();
        let prev = std::env::var_os("SKILLFORGE_HOME");
        unsafe {
            std::env::set_var("SKILLFORGE_HOME", &dir);
        }
        let result = f();
        unsafe {
            match prev {
                Some(p) => std::env::set_var("SKILLFORGE_HOME", p),
                None => std::env::remove_var("SKILLFORGE_HOME"),
            }
        }
        std::fs::remove_dir_all(&dir).ok();
        result
    }

    fn tempdir() -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("skillforge-credentials-test-{}", std::process::id()));
        p.push(uniqueish());
        p
    }

    fn uniqueish() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        format!(
            "{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }

    #[test]
    fn set_get_remove_roundtrip() {
        with_temp_home(|| {
            assert!(get("ghcr.io").is_none());

            set("ghcr.io", "alice", "sekret").unwrap();
            let entry = get("ghcr.io").expect("entry should exist");
            assert_eq!(entry.username, "alice");
            assert_eq!(entry.secret, "sekret");

            // File permissions should be restricted on unix.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let meta = std::fs::metadata(path()).unwrap();
                assert_eq!(meta.permissions().mode() & 0o777, 0o600);
            }

            assert!(remove("ghcr.io").unwrap());
            assert!(get("ghcr.io").is_none());
            assert!(!remove("ghcr.io").unwrap());
        });
    }

    #[test]
    fn missing_file_loads_default() {
        with_temp_home(|| {
            let creds = load().unwrap();
            assert!(creds.registries.is_empty());
            assert_eq!(creds.version, 1);
        });
    }
}
