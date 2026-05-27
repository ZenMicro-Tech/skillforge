use super::{Agent, SkillRef, Status};
use anyhow::Result;
use std::path::{Path, PathBuf};

pub struct ClaudeDesktop;

const MUX_SERVER_NAME: &str = "skillforge";

fn config_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let home = PathBuf::from(home);
    #[cfg(target_os = "macos")]
    {
        Some(home.join("Library/Application Support/Claude/claude_desktop_config.json"))
    }
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var_os("APPDATA")?;
        Some(PathBuf::from(appdata).join("Claude/claude_desktop_config.json"))
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let _ = home;
        None
    }
}

fn server_key(skill: &SkillRef) -> String {
    format!("skillforge-{}", skill.name)
}

impl Agent for ClaudeDesktop {
    fn id(&self) -> &'static str {
        "claude-desktop"
    }

    fn display_name(&self) -> &'static str {
        "Claude Desktop"
    }

    fn detect(&self) -> bool {
        config_path().map(|p| p.parent().is_some_and(|d| d.exists())).unwrap_or(false)
    }

    fn install(&self, skill: &SkillRef) -> Result<Status> {
        let Some(path) = config_path() else {
            return Ok(Status::Skipped("unsupported OS"));
        };
        super::generic::upsert_mcp_server(
            &path,
            &server_key(skill),
            skill.binary,
            &["tool".to_string()],
        )
    }

    fn uninstall(&self, skill: &SkillRef) -> Result<Status> {
        let Some(path) = config_path() else {
            return Ok(Status::Skipped("unsupported OS"));
        };
        super::generic::remove_mcp_server(&path, &server_key(skill))
    }

    fn is_linked(&self, skill: &SkillRef) -> Result<bool> {
        let Some(path) = config_path() else {
            return Ok(false);
        };
        super::generic::has_mcp_server(&path, &server_key(skill))
    }

    fn install_mux(&self, exe: &Path) -> Result<Status> {
        let Some(path) = config_path() else {
            return Ok(Status::Skipped("unsupported OS"));
        };
        super::generic::upsert_mcp_server(
            &path,
            MUX_SERVER_NAME,
            exe,
            &["mux".to_string(), "serve".to_string()],
        )
    }

    fn uninstall_mux(&self) -> Result<Status> {
        let Some(path) = config_path() else {
            return Ok(Status::Skipped("unsupported OS"));
        };
        super::generic::remove_mcp_server(&path, MUX_SERVER_NAME)
    }
}
