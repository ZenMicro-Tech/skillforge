//! Generic MCP adapter — prints a JSON snippet the user can paste anywhere
//! (Windsurf, Continue, etc.). Also exports the shared JSON config helpers
//! used by the Claude Desktop and Cursor adapters.

use super::{Agent, SkillRef, Status};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::Path;

pub struct Generic;

impl Agent for Generic {
    fn id(&self) -> &'static str {
        "generic"
    }

    fn display_name(&self) -> &'static str {
        "Generic MCP (print snippet)"
    }

    fn detect(&self) -> bool {
        true
    }

    fn install(&self, skill: &SkillRef) -> Result<Status> {
        let snippet = json!({
            "mcpServers": {
                format!("skillforge-{}", skill.name): {
                    "command": skill.binary.display().to_string(),
                    "args": ["tool"],
                }
            }
        });
        eprintln!("Paste into any MCP-compatible agent's config:");
        println!("{}", serde_json::to_string_pretty(&snippet)?);
        Ok(Status::Skipped("config snippet printed"))
    }

    fn uninstall(&self, _skill: &SkillRef) -> Result<Status> {
        Ok(Status::Skipped("nothing to remove — generic adapter only prints"))
    }

    fn is_linked(&self, _skill: &SkillRef) -> Result<bool> {
        Ok(false)
    }

    fn install_mux(&self, exe: &Path) -> Result<Status> {
        let snippet = json!({
            "mcpServers": {
                "skillforge": {
                    "command": exe.display().to_string(),
                    "args": ["mux", "serve"],
                }
            }
        });
        eprintln!("Paste into any MCP-compatible agent's config:");
        println!("{}", serde_json::to_string_pretty(&snippet)?);
        Ok(Status::Skipped("config snippet printed"))
    }

    fn uninstall_mux(&self) -> Result<Status> {
        Ok(Status::Skipped("nothing to remove — generic adapter only prints"))
    }
}

pub fn upsert_mcp_server(
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
        }),
    );

    write_atomic(path, &root)?;
    Ok(Status::Installed)
}

pub fn remove_mcp_server(path: &Path, name: &str) -> Result<Status> {
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

pub fn has_mcp_server(path: &Path, name: &str) -> Result<bool> {
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
