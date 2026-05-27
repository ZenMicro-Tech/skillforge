use super::{Agent, SkillRef, Status};
use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

pub struct ClaudeCode;

const MUX_SERVER_NAME: &str = "skillforge";

fn server_name(skill: &SkillRef) -> String {
    format!("skillforge-{}", skill.name)
}

impl Agent for ClaudeCode {
    fn id(&self) -> &'static str {
        "claude-code"
    }

    fn display_name(&self) -> &'static str {
        "Claude Code"
    }

    fn detect(&self) -> bool {
        which("claude").is_some()
    }

    fn install(&self, skill: &SkillRef) -> Result<Status> {
        let name = server_name(skill);
        let want = skill.binary.display().to_string();
        if let Some(current) = current_command(&name)? {
            if current == want {
                return Ok(Status::Skipped("already linked"));
            }
            // Stored path differs — remove and re-add so the registration tracks the real binary.
            let _ = Command::new("claude")
                .args(["mcp", "remove", &name, "-s", "user"])
                .status();
        }
        let status = Command::new("claude")
            .args(["mcp", "add", "--scope", "user", &name])
            .arg(skill.binary)
            .arg("tool")
            .status()
            .context("invoking `claude mcp add`")?;
        if !status.success() {
            anyhow::bail!("`claude mcp add` exited with {status}");
        }
        Ok(Status::Installed)
    }

    fn uninstall(&self, skill: &SkillRef) -> Result<Status> {
        if !self.is_linked(skill)? {
            return Ok(Status::Skipped("not linked"));
        }
        let name = server_name(skill);
        let status = Command::new("claude")
            .args(["mcp", "remove", &name, "-s", "user"])
            .status()
            .context("invoking `claude mcp remove`")?;
        if !status.success() {
            anyhow::bail!("`claude mcp remove` exited with {status}");
        }
        Ok(Status::NotInstalled)
    }

    fn is_linked(&self, skill: &SkillRef) -> Result<bool> {
        let name = server_name(skill);
        let output = Command::new("claude")
            .args(["mcp", "get", &name])
            .output()
            .context("invoking `claude mcp get`")?;
        Ok(output.status.success())
    }

    fn install_mux(&self, exe: &Path) -> Result<Status> {
        let exists = Command::new("claude")
            .args(["mcp", "get", MUX_SERVER_NAME])
            .output()
            .context("invoking `claude mcp get`")?
            .status
            .success();
        if exists {
            return Ok(Status::Skipped("mux already registered"));
        }
        let status = Command::new("claude")
            .args(["mcp", "add", "--scope", "user", MUX_SERVER_NAME])
            .arg(exe)
            .args(["mux", "serve"])
            .status()
            .context("invoking `claude mcp add`")?;
        if !status.success() {
            anyhow::bail!("`claude mcp add` exited with {status}");
        }
        Ok(Status::Installed)
    }

    fn uninstall_mux(&self) -> Result<Status> {
        let exists = Command::new("claude")
            .args(["mcp", "get", MUX_SERVER_NAME])
            .output()
            .context("invoking `claude mcp get`")?
            .status
            .success();
        if !exists {
            return Ok(Status::Skipped("mux not registered"));
        }
        let status = Command::new("claude")
            .args(["mcp", "remove", MUX_SERVER_NAME, "-s", "user"])
            .status()
            .context("invoking `claude mcp remove`")?;
        if !status.success() {
            anyhow::bail!("`claude mcp remove` exited with {status}");
        }
        Ok(Status::NotInstalled)
    }
}

/// Parse the `Command:` line out of `claude mcp get <name>`. Returns None if the
/// server isn't registered.
fn current_command(name: &str) -> Result<Option<String>> {
    let out = Command::new("claude")
        .args(["mcp", "get", name])
        .output()
        .context("invoking `claude mcp get`")?;
    if !out.status.success() {
        return Ok(None);
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    for line in stdout.lines() {
        if let Some(rest) = line.trim().strip_prefix("Command:") {
            return Ok(Some(rest.trim().to_string()));
        }
    }
    Ok(None)
}

fn which(cmd: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(cmd);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
