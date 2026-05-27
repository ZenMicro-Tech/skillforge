use anyhow::{bail, Context, Result};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Tool,
    Run,
    Serve,
    Describe,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "tool" => Some(Mode::Tool),
            "run" => Some(Mode::Run),
            "serve" => Some(Mode::Serve),
            "describe" => Some(Mode::Describe),
            _ => None,
        }
    }
}

pub struct Embedded {
    pub manifest_toml: &'static str,
    pub prompt_md: &'static str,
    pub schema_json: &'static str,
}

pub trait SkillHandler {
    fn call(&self, input: Value) -> Result<Value>;
}

pub fn dispatch<H: SkillHandler>(embedded: Embedded, handler: H) -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mode_str = args.get(1).map(String::as_str).unwrap_or("");
    let Some(mode) = Mode::parse(mode_str) else {
        print_usage(&args[0]);
        bail!("unknown mode {mode_str:?}");
    };

    match mode {
        Mode::Run => run_mode(&args[2..], &handler),
        Mode::Describe => describe_mode(&embedded),
        Mode::Tool => tool_mode(&embedded, &handler),
        Mode::Serve => serve_mode(),
    }
}

fn print_usage(prog: &str) {
    eprintln!("usage: {prog} <tool|run|serve|describe> [...]");
}

fn run_mode<H: SkillHandler>(args: &[String], handler: &H) -> Result<()> {
    let input = read_input_arg(args)?;
    let output = handler.call(input)?;
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn read_input_arg(args: &[String]) -> Result<Value> {
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--input" => {
                let raw = args
                    .get(i + 1)
                    .context("--input requires a JSON string argument")?;
                return serde_json::from_str(raw).context("--input must be valid JSON");
            }
            "--input-file" => {
                let path = args
                    .get(i + 1)
                    .context("--input-file requires a path argument")?;
                let text = std::fs::read_to_string(path)
                    .with_context(|| format!("reading {path}"))?;
                return serde_json::from_str(&text)
                    .with_context(|| format!("parsing JSON in {path}"));
            }
            "--stdin" => {
                let mut buf = String::new();
                std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)?;
                return serde_json::from_str(&buf).context("stdin must be valid JSON");
            }
            _ => i += 1,
        }
    }
    Ok(Value::Object(Default::default()))
}

fn describe_mode(e: &Embedded) -> Result<()> {
    let manifest: Value = toml::from_str::<toml::Value>(e.manifest_toml)?
        .try_into()
        .context("manifest -> json")?;
    let schema: Value = serde_json::from_str(e.schema_json).context("schema.json")?;
    let out = serde_json::json!({
        "manifest": manifest,
        "prompt": e.prompt_md,
        "schema": schema,
    });
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

fn tool_mode<H: SkillHandler>(e: &Embedded, handler: &H) -> Result<()> {
    let manifest = skillforge_core::Manifest::from_str(e.manifest_toml)?;
    let schema: Value = serde_json::from_str(e.schema_json)?;
    skillforge_mcp::stdio::serve(
        vec![skillforge_mcp::stdio::ToolDescriptor {
            name: manifest.skill.name.clone(),
            description: manifest.skill.description.clone(),
            input_schema: schema,
        }],
        |_name, input| handler.call(input),
    )
}

fn serve_mode() -> Result<()> {
    bail!("`serve` (MCP Streamable HTTP) is not yet implemented — target: Phase 2");
}
