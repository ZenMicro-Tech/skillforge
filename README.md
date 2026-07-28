# Skillforge

**Package AI capabilities once. Install them in any agent.**

Skillforge bundles execution logic, prompts, and schemas into versioned skills you can distribute through OCI registries and install with one command.

## Quickstart

### 1. Install Skillforge

**Homebrew (macOS/Linux):**

```sh
brew install ZenMicro-Tech/tap/skillforge
```

Or install from source:

```sh
git clone https://github.com/ZenMicro-Tech/skillforge.git
cd skillforge
cargo install --path crates/skillforge-cli
```

Verify the CLI:

```sh
skillforge --version
```

### 2. Find and install a capability

```sh
# Browse the public catalog
skillforge search

# Inspect a skill and its available versions
skillforge search --info web-fetch

# Install the latest version into every detected agent
skillforge add web-fetch
```

Skillforge resolves the skill, builds it, records the installation, and configures each directly supported agent: Claude Code, Claude Desktop, GitHub Copilot, Cursor, Visual Studio Code, and Windsurf. For other MCP clients, it prints a paste-ready configuration snippet. Launch a supported agent in a new session, then ask it to use the new tool:

```text
Use skillforge-web-fetch to fetch https://www.rust-lang.org and return the page as simplified markdown.
```

To make an install reproducible, pin the release:

```sh
skillforge add web-fetch:0.1.1
```

To install a skill from your own OCI registry instead:

```sh
skillforge add ghcr.io/acme/skills/document-redactor:1.2.0
```

### 3. Confirm, update, or remove it

```sh
skillforge list
skillforge upgrade --check
skillforge upgrade web-fetch
skillforge remove web-fetch
```

## Create and distribute your own capability

Use this workflow when you want to turn internal expertise, API access, or repeatable automation into a capability the whole team can install.

### 1. Scaffold and define the skill

```sh
skillforge new document-redactor
cd document-redactor
```

The generated project keeps the skill contract and implementation together:

```text
document-redactor/
├── skill.toml    # name, version, description, and interfaces
├── prompt.md     # instructions presented to the LLM
├── schema.json   # JSON Schema for tool input
├── src/main.rs   # deterministic execution logic
├── build.rs      # embeds the prompt and schema in the binary
└── Cargo.toml
```

Implement the logic, describe its input in `schema.json`, and make the prompt explain when and how an agent should call it.

### 2. Build and test locally

```sh
skillforge build --path .
skillforge describe --path .
skillforge run --path . -- --input '{"text":"Remove personal data from this document."}'
```

`describe` lets you inspect the exact manifest, prompt, and schema embedded in the built skill. `run` invokes its deterministic CLI mode, which is useful for local checks and CI.

To use a local skill in detected agents before publishing it:

```sh
skillforge add document-redactor
```

### 3. Publish a versioned artifact

Authenticate to your OCI registry with ORAS, then publish:

```sh
oras login ghcr.io
skillforge publish document-redactor --registry ghcr.io/acme/skills
```

Skillforge builds the release binary and publishes it with `skill.toml`, `prompt.md`, and `schema.json` to:

```text
ghcr.io/acme/skills/document-redactor:<version>
```

Anyone with registry access can now install that exact capability:

```sh
skillforge add ghcr.io/acme/skills/document-redactor:1.0.0
```

For multi-platform distribution, provide each Rust target:

```sh
skillforge publish document-redactor \
  --registry ghcr.io/acme/skills \
  --target aarch64-apple-darwin \
  --target x86_64-unknown-linux-gnu
```

## Supported agent integrations

Skillforge detects and writes the MCP configuration for these installed applications:

- Claude Code
- Claude Desktop
- GitHub Copilot
- Cursor
- Visual Studio Code
- Windsurf

For another MCP-compatible client, Skillforge prints a JSON configuration snippet that you can paste into that client's configuration.

By default, each installed skill is registered as its own MCP server. If your client benefits from one server that exposes all installed skills, enable mux mode:

```sh
skillforge mux enable
skillforge mux status
```

Mux mode registers a single `skillforge` MCP server and dynamically exposes all skills in the local registry.

## Use a skill outside a managed agent

Skills are portable executables, not only agent integrations.

```sh
# Run as a one-tool MCP stdio server
skillforge tool --path ./document-redactor

# Invoke deterministic CLI mode from a shell or CI
skillforge run --path ./document-redactor -- --input '{"text":"..."}'

# Inspect the contract embedded in a built binary
skillforge describe --path ./document-redactor
```

The [`skills/word-count`](skills/word-count/) directory contains a complete example. See [`examples/s3-agent`](examples/s3-agent/) for an example that retrieves a skill from an OCI registry during a Docker build and uses it from a Python agent.

## Common commands

| Command | Purpose |
|---|---|
| `skillforge search [query]` | Browse the public skill catalog |
| `skillforge search --info <name>` | View a skill's details, versions, and install commands |
| `skillforge add <name-or-ref>...` | Install local skills, catalog skills, or OCI references |
| `skillforge list` | List installed skills |
| `skillforge remove <name>...` | Remove skills from the registry and detected agents |
| `skillforge upgrade [name] [--check]` | Check for and install newer skill versions |
| `skillforge new <name>` | Scaffold a Rust skill project |
| `skillforge build [--path <dir>]` | Build a skill without installing it |
| `skillforge publish <name> [--registry <repo>]` | Publish a skill to an OCI registry |
| `skillforge run [--path <dir>] -- --input '<json>'` | Invoke deterministic CLI mode |
| `skillforge tool [--path <dir>]` | Run a skill as an MCP stdio server |
| `skillforge describe [--path <dir>]` | Print the embedded manifest, prompt, and schema |
| `skillforge mux enable\|disable\|status` | Manage the single-server MCP aggregator |

## Installation options

If Homebrew is not available, download a prebuilt binary from the [latest release](https://github.com/ZenMicro-Tech/skillforge/releases/latest):

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

Ensure `~/.local/bin` is on your `PATH`:

```sh
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc  # or ~/.bashrc
```

## Prerequisites

- Rust 1.85+ to build skills on the local machine
- At least one MCP-compatible agent to use Skillforge-managed integrations
- [ORAS](https://oras.land/docs/installation) for OCI publishing and installing OCI references (`brew install oras`)

## Configuration

- `SKILLFORGE_HOME` — overrides `~/.skillforge`; useful for CI and isolated environments.

## Architecture and roadmap

See [plan.md](plan.md) for the architecture and planned work, including signing and sandboxing.