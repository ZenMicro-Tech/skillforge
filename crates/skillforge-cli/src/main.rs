use anyhow::Result;
use clap::{Parser, Subcommand};

mod adapters;
mod commands;
mod oci;
mod registry;
mod sources;

#[derive(Parser)]
#[command(name = "skillforge", version, about = "Author, build, and run AI skill binaries.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build and install a skill into every detected agent.
    ///
    /// Accepts either a local skill name (resolved from `./skills/<name>` or
    /// `~/.skillforge/skills/<name>`) or an OCI reference (e.g.
    /// `ghcr.io/owner/skills/example-skill:0.1.0`). OCI refs are detected by
    /// the presence of `:` or `/`.
    Add {
        name_or_ref: String,
    },
    /// Remove a skill from every detected agent.
    Remove {
        name: String,
    },
    /// Publish a skill to an OCI registry (via ORAS).
    Publish {
        name: String,
        /// Override the `[publish].registry` from skill.toml.
        #[arg(long)]
        registry: Option<String>,
        /// Path to skill directory (overrides name-based resolution).
        #[arg(long)]
        path: Option<String>,
        /// Rust target triple(s) to build for. Repeat for multi-arch.
        /// e.g. --target aarch64-apple-darwin --target x86_64-unknown-linux-gnu
        #[arg(long)]
        target: Vec<String>,
    },
    /// Scaffold a new skill directory from the rust-skill template.
    New {
        /// Skill name (must match [a-z][a-z0-9-]*).
        name: String,
        /// Directory to create. Defaults to the skill name.
        #[arg(long)]
        path: Option<String>,
    },
    /// Build the skill in the current directory (or --path) into a release binary.
    Build {
        #[arg(long)]
        path: Option<String>,
    },
    /// Invoke a built skill in `run` mode. Forwards remaining args to the binary.
    Run {
        #[arg(long)]
        path: Option<String>,
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Invoke a built skill as an MCP stdio server (one-tool MCP).
    Tool {
        #[arg(long)]
        path: Option<String>,
    },
    /// Invoke a built skill as an HTTP/SSE server (MCP Streamable). Phase 2.
    Serve {
        #[arg(long)]
        path: Option<String>,
    },
    /// Print the embedded manifest, prompt, and schema of a built skill.
    Describe {
        #[arg(long)]
        path: Option<String>,
    },
    /// (advanced) Link a built skill in a directory to all detected agents.
    #[command(hide = true)]
    Link {
        #[arg(long)]
        path: Option<String>,
        #[arg(long = "agent")]
        agents: Vec<String>,
    },
    /// (advanced) Unlink a skill in a directory from all detected agents.
    #[command(hide = true)]
    Unlink {
        #[arg(long)]
        path: Option<String>,
        #[arg(long = "agent")]
        agents: Vec<String>,
    },
    /// Search for available skills in a remote catalog registry.
    Search {
        /// Filter skills by name (substring match). Omit to list all.
        query: Option<String>,
        /// Show detailed info for a specific skill name.
        #[arg(long)]
        info: Option<String>,
        /// OCI catalog repository to query. Defaults to the public skillforge catalog.
        #[arg(long)]
        registry: Option<String>,
    },
    /// List installed skills, or show detail for a specific skill.
    List {
        /// Show detail (linked agents) for a specific skill directory.
        #[arg(long)]
        path: Option<String>,
    },
    /// Manage mux mode — a single MCP server that aggregates all installed skills.
    Mux {
        #[command(subcommand)]
        action: MuxAction,
    },
}

#[derive(Subcommand)]
enum MuxAction {
    /// Switch to mux mode: register `skillforge` as a single MCP server with detected agents.
    Enable,
    /// Switch back to per-skill registration.
    Disable,
    /// Show whether mux is enabled and which skills are registered.
    Status,
    /// (internal) Run the mux MCP stdio server. Invoked by agents, not users.
    #[command(hide = true)]
    Serve,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Add { name_or_ref } => commands::add::add(&name_or_ref),
        Command::Remove { name } => commands::add::remove(&name),
        Command::Publish {
            name,
            registry,
            path,
            target,
        } => commands::publish::publish(&name, registry.as_deref(), path.as_deref(), &target),
        Command::New { name, path } => commands::new::run(&name, path.as_deref()),
        Command::Build { path } => commands::build::run(path.as_deref()),
        Command::Run { path, args } => commands::delegate::run(path.as_deref(), "run", &args),
        Command::Tool { path } => commands::delegate::run(path.as_deref(), "tool", &[]),
        Command::Serve { path } => commands::delegate::run(path.as_deref(), "serve", &[]),
        Command::Describe { path } => commands::delegate::run(path.as_deref(), "describe", &[]),
        Command::Link { path, agents } => {
            commands::link::link(path.as_deref(), filter(&agents))
        }
        Command::Unlink { path, agents } => {
            commands::link::unlink(path.as_deref(), filter(&agents))
        }
        Command::Search {
            query,
            info,
            registry,
        } => {
            if let Some(name) = info {
                commands::search::search_detail(&name, registry.as_deref())
            } else {
                commands::search::search(query.as_deref(), registry.as_deref())
            }
        }
        Command::List { path } => commands::link::list(path.as_deref()),
        Command::Mux { action } => match action {
            MuxAction::Enable => commands::mux::enable(),
            MuxAction::Disable => commands::mux::disable(),
            MuxAction::Status => commands::mux::status(),
            MuxAction::Serve => commands::mux::serve(),
        },
    }
}

fn filter(v: &[String]) -> Option<&[String]> {
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}
