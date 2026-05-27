# skillforge

A package manager for AI skills. Build a skill once as a Rust binary; install it into Claude Code, Claude Desktop, Cursor, or any MCP-compatible agent with one command.

A **skill** is a self-contained binary that embeds its own prompt, JSON schema, and code. The same artifact is callable as an MCP tool, a CLI command, an HTTP server, or a library.

This repo contains the **`skillforge` CLI and runtime** only. Skills themselves live in their own repos — see [skillforge-skills](https://github.com/zenmicro-tech/skillforge-skills) for example skills.

## Quickstart

### Prerequisites

- Rust 1.90+
- At least one MCP-compatible agent (Claude Code, Claude Desktop, or Cursor)
- [ORAS](https://oras.land/docs/installation) for `publish` and OCI-ref `add` (`brew install oras`)

### Build

```sh
git clone <this-repo>
cd ai-skills-platform
cargo build --workspace
```

Binary at `./target/debug/skillforge`. Add to PATH or alias it.

### Install a skill

`add` accepts either a **local skill name** (resolved from `./skills/<name>` or `~/.skillforge/skills/<name>`) or an **OCI reference**:

```sh
# From a registry
skillforge add ghcr.io/zenmicro-tech/skills/example-skill:0.1.0

# From a local checkout (e.g. cloned skillforge-skills next to this repo)
cd ../skillforge-skills
skillforge add example-skill
```

`add` builds in release mode (or trusts a pulled binary), writes to the registry at `~/.skillforge/registry.json`, and registers with every detected agent. Open a new Claude Code session, run `/mcp`, and the skill appears as a connected tool.

OCI refs are detected by the presence of `:` or `/`.

### Author a new skill

```sh
skillforge new my-skill
cd my-skill
# Edit src/main.rs, prompt.md, schema.json
skillforge add ./my-skill         # build + install
skillforge publish my-skill       # push to OCI (requires [publish].repo in skill.toml)
```

### Mux mode (single-server aggregator)

By default each skill is registered as its own MCP server. **Mux mode** registers a single `skillforge` server that aggregates all installed skills:

```sh
skillforge mux enable    # one server, many tools
skillforge mux status
skillforge mux disable   # back to per-skill registration
```

Mux mode re-reads the registry on every `tools/list` call, so newly-added skills appear without restarting agents.

## Commands

| Command | What it does |
|---|---|
| `skillforge new <name>` | Scaffold a new skill directory |
| `skillforge add <name-or-ref>` | Resolve (locally or via OCI), build, register, link |
| `skillforge remove <name>` | Unlink and remove from registry |
| `skillforge publish <name> [--repo R]` | Push to OCI registry via ORAS |
| `skillforge build [--path]` | Build without installing |
| `skillforge run [--path] -- --input '...'` | Invoke a skill's deterministic CLI mode |
| `skillforge tool [--path]` | Run a skill as an MCP stdio server |
| `skillforge describe [--path]` | Print embedded manifest, prompt, and schema |
| `skillforge mux enable\|disable\|status` | Toggle the single-server aggregator |

## Skill anatomy

```
my-skill/
  skill.toml      # name, version, runtime, interfaces, [publish].repo
  prompt.md       # LLM-facing instructions
  schema.json     # JSON Schema for tool input
  Cargo.toml      # Rust package (with [workspace] opt-out)
  build.rs        # embeds toml/md/json into the binary
  src/main.rs     # implements skillforge_runtime::SkillHandler
```

The compiled binary supports four modes selected by the first argument:

| Mode | Caller | Purpose |
|---|---|---|
| `tool` | MCP-compatible agent | JSON-RPC stdio server |
| `run --input <json>` | shell, CI | Deterministic CLI invocation |
| `serve` | remote agent | HTTP/SSE (Phase 2) |
| `describe` | tooling | Dump embedded prompt + schema |

## Configuration

- `SKILLFORGE_HOME` — overrides `~/.skillforge`. Useful for tests and CI.

## Repos

- **ai-skills-platform** (this repo) — `skillforge` CLI, `skillforge-runtime`, `skillforge-mcp`, `skillforge-core`.
- **[skillforge-skills](https://github.com/zenmicro-tech/skillforge-skills)** — example and reference skills.

## Architecture

See [plan.md](plan.md) for the full architecture document — design rationale, distribution and signing plans for Phase 2, and sandboxing for Phase 3.
