use anyhow::{bail, Context, Result};
use serde_json::Value;
use skillforge_mcp::stdio::{self, ToolDescriptor};
use std::process::Command;

use crate::adapters::{self, SkillRef, Status};
use crate::registry;

pub fn enable() -> Result<()> {
    let exe = std::env::current_exe().context("locating skillforge executable")?;
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    let reg = registry::load()?;

    let mut linked_agents = Vec::new();
    eprintln!("enabling mux mode (single MCP server: skillforge)");

    for agent in adapters::all() {
        if !agent.detect() {
            continue;
        }
        match agent.install_mux(&exe) {
            Ok(Status::Installed) => {
                eprintln!("  ✓ {} — mux registered", agent.display_name());
                linked_agents.push(agent.id().to_string());
            }
            Ok(Status::Skipped(reason)) => {
                eprintln!("  · {} — {reason}", agent.display_name());
                if reason == "mux already registered" {
                    linked_agents.push(agent.id().to_string());
                }
            }
            Ok(Status::NotInstalled) => {}
            Err(e) => eprintln!("  ✗ {} — {e}", agent.display_name()),
        }

        for skill_name in reg.skills.keys() {
            let skill_ref = SkillRef {
                name: skill_name,
                binary: &reg.skills[skill_name].binary,
            };
            let _ = agent.uninstall(&skill_ref);
        }
    }

    registry::set_mux(true, &linked_agents)?;
    eprintln!("mux active. {} skill(s) available via skillforge MCP server.", reg.skills.len());
    Ok(())
}

pub fn disable() -> Result<()> {
    let reg = registry::load()?;
    eprintln!("disabling mux mode; restoring per-skill registration");

    for agent in adapters::all() {
        if !agent.detect() {
            continue;
        }
        match agent.uninstall_mux() {
            Ok(Status::NotInstalled) => eprintln!("  ✓ {} — mux removed", agent.display_name()),
            Ok(Status::Skipped(reason)) => eprintln!("  · {} — {reason}", agent.display_name()),
            Ok(Status::Installed) => {}
            Err(e) => eprintln!("  ✗ {} — {e}", agent.display_name()),
        }

        for (skill_name, entry) in &reg.skills {
            let skill_ref = SkillRef {
                name: skill_name,
                binary: &entry.binary,
            };
            match agent.install(&skill_ref) {
                Ok(Status::Installed) => {
                    eprintln!("    ↳ relinked {skill_name}")
                }
                Ok(Status::Skipped(_)) | Ok(Status::NotInstalled) => {}
                Err(e) => eprintln!("    ↳ {skill_name}: {e}"),
            }
        }
    }

    registry::set_mux(false, &[])?;
    Ok(())
}

pub fn status() -> Result<()> {
    let reg = registry::load()?;
    println!("mux: {}", if reg.mux.enabled { "enabled" } else { "disabled" });
    if !reg.mux.agents.is_empty() {
        println!("agents: {}", reg.mux.agents.join(", "));
    }
    println!("skills: {}", reg.skills.len());
    for (name, entry) in &reg.skills {
        println!("  - {} v{} → {}", name, entry.version, entry.binary.display());
    }
    Ok(())
}

pub fn serve() -> Result<()> {
    let list_tools = || {
        let reg = registry::load().unwrap_or_default();
        reg.skills
            .iter()
            .map(|(name, entry)| ToolDescriptor {
                name: name.clone(),
                description: entry.description.clone(),
                input_schema: entry.input_schema.clone(),
            })
            .collect()
    };

    let mut handler = |name: &str, args: Value| -> Result<Value> { spawn_skill(name, args) };
    stdio::serve_dynamic(list_tools, &mut handler)
}

fn spawn_skill(name: &str, args: Value) -> Result<Value> {
    let reg = registry::load()?;
    let entry = reg
        .skills
        .get(name)
        .with_context(|| format!("skill {name:?} not in registry"))?;
    if !entry.binary.exists() {
        bail!("skill binary {} not found", entry.binary.display());
    }
    let json_arg = serde_json::to_string(&args)?;
    let out = Command::new(&entry.binary)
        .arg("run")
        .arg("--input")
        .arg(&json_arg)
        .output()
        .with_context(|| format!("spawning {}", entry.binary.display()))?;
    if !out.status.success() {
        bail!(
            "skill {name} exited {} — stderr: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    serde_json::from_slice(&out.stdout)
        .with_context(|| format!("skill {name} stdout was not JSON"))
}
