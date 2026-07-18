use super::{Agent, SkillRef, Status};
use anyhow::Result;
use std::path::{Path, PathBuf};

pub struct VSCode;

const MUX_SERVER_NAME: &str = "skillforge";

fn config_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".vscode/mcp.json"))
}

fn server_key(skill: &SkillRef) -> String {
    format!("skillforge-{}", skill.name)
}

impl Agent for VSCode {
    fn id(&self) -> &'static str {
        "vscode"
    }

    fn display_name(&self) -> &'static str {
        "VS Code"
    }

    fn detect(&self) -> bool {
        let Some(home) = std::env::var_os("HOME") else {
            return false;
        };
        // Detect VS Code by checking for its typical config/data directories
        let vscode_dir = PathBuf::from(&home).join(".vscode");
        let app_support = PathBuf::from(&home).join("Library/Application Support/Code");
        vscode_dir.exists() || app_support.exists()
    }

    fn install(&self, skill: &SkillRef) -> Result<Status> {
        let Some(path) = config_path() else {
            return Ok(Status::Skipped("HOME not set"));
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
            return Ok(Status::Skipped("HOME not set"));
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
            return Ok(Status::Skipped("HOME not set"));
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
            return Ok(Status::Skipped("HOME not set"));
        };
        super::generic::remove_mcp_server(&path, MUX_SERVER_NAME)
    }
}