use super::{Agent, SkillRef, Status};
use anyhow::Result;
use std::path::{Path, PathBuf};

pub struct Cursor;

const MUX_SERVER_NAME: &str = "skillforge";

fn config_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cursor/mcp.json"))
}

fn server_key(skill: &SkillRef) -> String {
    format!("skillforge-{}", skill.name)
}

impl Agent for Cursor {
    fn id(&self) -> &'static str {
        "cursor"
    }

    fn display_name(&self) -> &'static str {
        "Cursor"
    }

    fn detect(&self) -> bool {
        let Some(home) = std::env::var_os("HOME") else {
            return false;
        };
        PathBuf::from(home).join(".cursor").exists()
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
