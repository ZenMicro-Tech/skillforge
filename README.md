# skillforge

A package manager for AI skills. Build a skill once as a compiled binary; install it into Claude Code, Claude Desktop, Cursor, or any MCP-compatible agent with one command.

A **skill** is a self-contained binary that embeds its own prompt, JSON schema, and execution logic. The same artifact works as an MCP tool, a CLI command, an HTTP server, or a library — no wrappers, no adapters.

## Quick Start

### 1. Install skillforge

**Homebrew (macOS/Linux):**

```sh
brew install ZenMicro-Tech/tap/skillforge
```

**Prebuilt binary:**

```sh
mkdir -p ~/.local/bin

# macOS (Apple Silicon)
curl -fSL https://github.com/ZenMicro-Tech/skillforge/releases/latest/download/skillforge-aarch64-apple-darwin \
  -o ~/.local/bin/skillforge && chmod +x ~/.local/bin/skillforge

# macOS (Intel)
curl -fSL https://github.com/ZenMicro-Tech/skillforge/releases/latest/download/skillforge-x86_64-apple-darwin \
  -o ~/.local/bin/skillforge && chmod +x ~/.local/bin/skillforge

# Linux (x86_64)
curl -fSL https://github.com/ZenMicro-Tech/skillforge/releases/latest/download/skillforge-x86_64-unknown-linux-gnu \
  -o ~/.local/bin/skillforge && chmod +x ~/.local/bin/skillforge

# Linux (aarch64)
curl -fSL https://github.com/ZenMicro-Tech/skillforge/releases/latest/download/skillforge-aarch64-unknown-linux-gnu \
  -o ~/.local/bin/skillforge && chmod +x ~/.local/bin/skillforge
```

Make sure `~/.local/bin` is on your PATH:

```sh
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc  # or ~/.bashrc
```

**From source:**

```sh
git clone https://github.com/ZenMicro-Tech/skillforge.git
cd skillforge
cargo install --path crates/skillforge-cli
```

Verify the install:

```sh
skillforge --version
```

### 2. Create your first skill

```sh
skillforge new my-skill
cd my-skill
```

This scaffolds a ready-to-build skill:

```
my-skill/
  skill.toml      # metadata: name, version, interfaces
  prompt.md       # LLM-facing instructions
  schema.json     # JSON Schema for tool input
  src/main.rs     # your skill logic
  build.rs        # embeds prompt + schema into the binary
  Cargo.toml
```

### 3. Build and install

```sh
skillforge add my-skill
```

This compiles the skill, registers it with every detected agent (Claude Code, Claude Desktop, Cursor), and you're done. Open a new agent session and the skill appears as a tool.

Install several skills in one command by listing each local name or OCI reference:

```sh
skillforge add git github ghcr.io/yourname/skills/aws-s3:0.1.0
```

Skills are processed in the order supplied. The command stops at the first failed installation and reports which skill failed.

### 4. Use it

```sh
# As a CLI
skillforge run --path ./my-skill -- --input '{"text": "hello world"}'

# As an MCP server (agents call this automatically)
skillforge tool --path ./my-skill

# Inspect what's embedded
skillforge describe --path ./my-skill
```

### 5. Publish to a registry

```sh
skillforge publish my-skill
```

Others can then install it with:

```sh
skillforge add ghcr.io/yourname/skills/my-skill:0.1.0
```

---

## Example: word-count skill with a Python agent

The [`skills/word-count`](skills/word-count/) directory shows a complete skill. Here's how to wire it up to a [Strands](https://github.com/strands-agents/sdk-python) agent:

```sh
cd skills/word-count
skillforge build --path .
python agent.py "How many words are in the Gettysburg Address?"
```

The agent calls the compiled binary over MCP stdio — no HTTP, no containers, no config files:

```python
from strands import Agent
from strands.tools.mcp import MCPClient
from mcp import StdioServerParameters
from mcp.client.stdio import stdio_client

mcp = MCPClient(lambda: stdio_client(
    StdioServerParameters(command="./target/release/word-count", args=["tool"])
))

with mcp:
    agent = Agent(model=model, tools=[mcp])
    print(agent("Count the words in this text: ..."))
```

See [`examples/s3-agent`](examples/s3-agent/) for a Dockerized example that pulls a skill from an OCI registry at build time.

---

## Commands

| Command | What it does |
|---|---|
| `skillforge new <name>` | Scaffold a new skill directory |
| `skillforge add <name-or-ref>...` | Resolve one or more skills (locally or via OCI), build, register, link |
| `skillforge remove <name>` | Unlink and remove from registry |
| `skillforge publish <name> [--repo R]` | Push to OCI registry via ORAS |
| `skillforge build [--path]` | Build without installing |
| `skillforge run [--path] -- --input '...'` | Invoke a skill's deterministic CLI mode |
| `skillforge tool [--path]` | Run a skill as an MCP stdio server |
| `skillforge describe [--path]` | Print embedded manifest, prompt, and schema |
| `skillforge mux enable\|disable\|status` | Toggle the single-server aggregator |
| `skillforge upgrade [name] [--check]` | Check for and apply newer versions of installed skills |

## Upgrading skills

Check for available updates:

```sh
skillforge upgrade --check          # check all installed skills
skillforge upgrade --check my-skill # check a specific skill
```

Apply upgrades:

```sh
skillforge upgrade                  # upgrade all installed skills
skillforge upgrade my-skill         # upgrade a specific skill
```

The `upgrade` command queries the OCI catalog for newer versions, pulls the latest artifact, rebuilds, and re-links — all without touching your existing install until the new version is ready.

> **Note on `add` idempotency:** Running `skillforge add` for an already-installed skill is safe — the linking step deduplicates across all adapters and will skip with "already linked." However, the command is not fully idempotent: it will re-fetch OCI artifacts (deleting the existing install first), re-build, and re-register on every invocation. If a re-run fails mid-way (e.g., network error), a previously working OCI install may be lost. Use `skillforge upgrade` to safely update to a newer version.

## Mux mode

By default each skill is its own MCP server. **Mux mode** registers a single `skillforge` server that aggregates all installed skills:

```sh
skillforge mux enable    # one server, many tools
skillforge mux status
skillforge mux disable   # back to per-skill registration
```

Mux mode re-reads the registry on every `tools/list` call, so newly-added skills appear without restarting agents.

## How skills work

The compiled binary supports four modes selected by the first argument:

| Mode | Caller | Purpose |
|---|---|---|
| `tool` | MCP-compatible agent | JSON-RPC stdio server |
| `run --input <json>` | shell, CI | Deterministic CLI invocation |
| `serve` | remote agent | HTTP/SSE (Phase 2) |
| `describe` | tooling | Dump embedded prompt + schema |

## Prerequisites

- Rust 1.85+
- At least one MCP-compatible agent (Claude Code, Claude Desktop, or Cursor)
- [ORAS](https://oras.land/docs/installation) for `publish` and OCI-ref `add` (`brew install oras`)

## Configuration

- `SKILLFORGE_HOME` — overrides `~/.skillforge`. Useful for tests and CI.

## Architecture

See [plan.md](plan.md) for the full design document — distribution, signing (Phase 2), and sandboxing (Phase 3).
