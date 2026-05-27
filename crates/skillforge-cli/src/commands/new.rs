use anyhow::{bail, Context, Result};
use std::fs;
use std::path::PathBuf;

pub fn run(name: &str, path: Option<&str>) -> Result<()> {
    if !valid_name(name) {
        bail!("invalid skill name {name:?}: must match [a-z][a-z0-9-]*");
    }
    let dir = PathBuf::from(path.unwrap_or(name));
    if dir.exists() {
        bail!("{} already exists", dir.display());
    }
    fs::create_dir_all(dir.join("src")).context("mkdir src")?;
    fs::create_dir_all(dir.join("assets")).context("mkdir assets")?;
    fs::create_dir_all(dir.join("tests")).context("mkdir tests")?;

    fs::write(dir.join("skill.toml"), skill_toml(name))?;
    fs::write(dir.join("prompt.md"), prompt_md(name))?;
    fs::write(dir.join("schema.json"), SCHEMA_JSON)?;
    fs::write(dir.join("Cargo.toml"), cargo_toml(name))?;
    fs::write(dir.join("src/main.rs"), MAIN_RS)?;
    fs::write(dir.join("build.rs"), BUILD_RS)?;
    fs::write(dir.join(".gitignore"), "/target\n")?;

    eprintln!("created skill at {}", dir.display());
    eprintln!("next: cd {} && skillforge build", dir.display());
    Ok(())
}

fn valid_name(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase())
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn skill_toml(name: &str) -> String {
    format!(
        r#"[skill]
name = "{name}"
version = "0.1.0"
description = "A skillforge skill."
license = "Apache-2.0"

[runtime]
kind = "rust"
entrypoint = "src/main.rs"
determinism = "pure"

[interfaces]
mcp  = true
cli  = true
http = true
lib  = true
"#
    )
}

fn prompt_md(name: &str) -> String {
    format!(
        r#"# {name}

Describe what this skill does and when the LLM should call it.
"#
    )
}

const SCHEMA_JSON: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "properties": {
    "input": { "type": "string", "description": "Example input." }
  },
  "required": ["input"],
  "additionalProperties": false
}
"#;

fn cargo_toml(name: &str) -> String {
    format!(
        r#"[workspace]

[package]
name    = "{name}"
version = "0.1.0"
edition = "2024"

# TODO: replace with crates.io version once skillforge-runtime is published.
# Path assumes ai-skills-platform is a sibling of this skill's parent dir.
[dependencies]
skillforge-runtime = {{ path = "../../ai-skills-platform/crates/skillforge-runtime" }}
anyhow             = "1"
serde_json         = "1"
"#
    )
}

const MAIN_RS: &str = r#"use anyhow::Result;
use serde_json::{json, Value};
use skillforge_runtime::{dispatch, Embedded, SkillHandler};

struct Handler;

impl SkillHandler for Handler {
    fn call(&self, input: Value) -> Result<Value> {
        let text = input
            .get("input")
            .and_then(Value::as_str)
            .unwrap_or("");
        Ok(json!({ "echo": text }))
    }
}

fn main() -> Result<()> {
    let embedded = Embedded {
        manifest_toml: include_str!(concat!(env!("OUT_DIR"), "/skill.toml")),
        prompt_md:     include_str!(concat!(env!("OUT_DIR"), "/prompt.md")),
        schema_json:   include_str!(concat!(env!("OUT_DIR"), "/schema.json")),
    };
    dispatch(embedded, Handler)
}
"#;

const BUILD_RS: &str = r#"use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    for f in ["skill.toml", "prompt.md", "schema.json"] {
        std::fs::copy(f, out.join(f)).unwrap_or_else(|e| panic!("copy {f}: {e}"));
        println!("cargo:rerun-if-changed={f}");
    }
}
"#;
