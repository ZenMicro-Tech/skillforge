use super::{Agent, SkillRef, Status};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub struct Windsurf;

const MUX_SERVER_NAME: &str = "skillforge";

fn config_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| {
        PathBuf::from(h).join(".codeium/windsurf/mcp_config.json")
    })
}

fn server_key(skill: &SkillRef) -> String {
    format!("skillforge-{}", skill.name)
}

impl Agent for Windsurf {
    fn id(&self) -> &'static str {
        "windsurf"
    }

    fn display_name(&self) -> &'static str {
        "Windsurf"
    }

    fn detect(&self) -> bool {
        if let Some(home) = std::env::var_os("HOME") {
            PathBuf::from(home).join(".codeium/windsurf").exists()
        } else {
            false
        }
    }

    fn install(&self, skill: &SkillRef) -> Result<Status> {
        let Some(path) = config_path() else {
            return Ok(Status::Skipped("HOME not set"));
        };
        upsert_mcp_server_windsurf(
            &path,
            &server_key(skill),
            skill.binary,
            &["tool".to_string()],
        )
    }

    fn uninstall(&self, skill: &SkillRef) -> Result<Status> {
        let Some(path) = config_path() else {
            return Ok(Status::Skipped("HOME not set"));
        };
        remove_mcp_server_windsurf(&path, &server_key(skill))
    }

    fn is_linked(&self, skill: &SkillRef) -> Result<bool> {
        let Some(path) = config_path() else {
            return Ok(false);
        };
        has_mcp_server_windsurf(&path, &server_key(skill))
    }

    fn install_mux(&self, exe: &Path) -> Result<Status> {
        let Some(path) = config_path() else {
            return Ok(Status::Skipped("HOME not set"));
        };
        upsert_mcp_server_windsurf(
            &path,
            MUX_SERVER_NAME,
            exe,
            &["mux".to_string(), "serve".to_string()],
        )
    }

    fn uninstall_mux(&self) -> Result<Status> {
        let Some(path) = config_path() else {
            return Ok(Status::Skipped("HOME not set"));
        };
        remove_mcp_server_windsurf(&path, MUX_SERVER_NAME)
    }
}

/// Upsert an MCP server into Windsurf's config.
/// Windsurf uses ~./codeium/windsurf/mcp_config.json with standard MCP format.
fn upsert_mcp_server_windsurf(
    path: &Path,
    name: &str,
    binary: &Path,
    args: &[String],
) -> Result<Status> {
    let mut root = read_or_empty(path)?;
    let servers = root
        .as_object_mut()
        .context("root must be a JSON object")?
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Default::default()));
    let servers = servers
        .as_object_mut()
        .context("mcpServers must be an object")?;

    let already = servers.get(name).is_some_and(|existing| {
        existing.get("command").and_then(Value::as_str) == Some(&binary.display().to_string())
    });
    if already {
        return Ok(Status::Skipped("already linked"));
    }

    servers.insert(
        name.to_string(),
        json!({
            "command": binary.display().to_string(),
            "args": args,
            "env": {},
        }),
    );

    write_atomic(path, &root)?;
    Ok(Status::Installed)
}

fn remove_mcp_server_windsurf(path: &Path, name: &str) -> Result<Status> {
    if !path.exists() {
        return Ok(Status::Skipped("config file not found"));
    }
    let mut root = read_or_empty(path)?;
    let Some(servers) = root
        .as_object_mut()
        .and_then(|o| o.get_mut("mcpServers"))
        .and_then(Value::as_object_mut)
    else {
        return Ok(Status::Skipped("no mcpServers section"));
    };
    if servers.remove(name).is_none() {
        return Ok(Status::Skipped("not linked"));
    }
    write_atomic(path, &root)?;
    Ok(Status::NotInstalled)
}

fn has_mcp_server_windsurf(path: &Path, name: &str) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let root = read_or_empty(path)?;
    Ok(root
        .get("mcpServers")
        .and_then(Value::as_object)
        .is_some_and(|o| o.contains_key(name)))
}

fn read_or_empty(path: &Path) -> Result<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    if text.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(&text).with_context(|| format!("parsing JSON in {}", path.display()))
}

fn write_atomic(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let tmp = path.with_extension("skillforge.tmp");
    let pretty = serde_json::to_string_pretty(value)?;
    std::fs::write(&tmp, pretty).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("renaming into {}", path.display()))?;
    Ok(())
}
