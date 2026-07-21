//! Agent adapters: wire a built skill into each installed AI agent's config.
//!
//! Each adapter implements `Agent`. `all()` returns every known adapter;
//! `link` and `unlink` commands iterate over those that report `detect() == true`.

use anyhow::Result;
use std::path::Path;

pub mod claude_code;
pub mod claude_desktop;
pub mod copilot;
pub mod cursor;
pub mod generic;
pub mod vscode;
pub mod windsurf;

pub struct SkillRef<'a> {
    pub name: &'a str,
    pub binary: &'a Path,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Installed,
    NotInstalled,
    Skipped(&'static str),
}

pub trait Agent {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    /// Is the agent present on this machine?
    fn detect(&self) -> bool;
    /// Register the skill with this agent.
    fn install(&self, skill: &SkillRef) -> Result<Status>;
    /// Remove the skill from this agent.
    fn uninstall(&self, skill: &SkillRef) -> Result<Status>;
    /// Is the skill currently linked to this agent?
    fn is_linked(&self, skill: &SkillRef) -> Result<bool>;
    /// Register the skillforge mux (single MCP server exposing all skills).
    fn install_mux(&self, exe: &Path) -> Result<Status>;
    /// Remove the skillforge mux.
    fn uninstall_mux(&self) -> Result<Status>;
}

pub fn all() -> Vec<Box<dyn Agent>> {
    vec![
        Box::new(claude_code::ClaudeCode),
        Box::new(claude_desktop::ClaudeDesktop),
        Box::new(copilot::Copilot),
        Box::new(cursor::Cursor),
        Box::new(vscode::VSCode),
        Box::new(windsurf::Windsurf),
        Box::new(generic::Generic),
    ]
}
