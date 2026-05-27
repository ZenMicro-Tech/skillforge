Ready for review
Select text to add comments on the plan
skillforge — AI Skills Platform: Initial Architecture
Context
Agent authors today choose between two unsatisfying options:

Claude-style local skills (SKILL.md + scripts): trivial to author, but no versioning, no signatures, no remote distribution, no determinism guarantees — every invocation is at the mercy of whatever the host runtime provides.
MCP servers: typed and distributable, but heavyweight — a full JSON-RPC server per tool, 100–500 ms stdio cold start, npm/PyPI-only delivery, no per-tool versioning, no central registry, and no "skill-like" authoring ergonomics.
There is no artifact format that is as cheap to author as a SKILL.md, as typed and callable as an MCP tool, and as distributable and verifiable as a container image.

skillforge closes that gap with one primitive: a skill is a signed, versioned binary artifact that embeds its own prompt, schema, and deterministic code, published to an OCI registry, and invocable by any LLM runtime (local subprocess, remote HTTP, MCP) or directly by a human from a shell.

This plan captures the architecture decisions for the project. Implementation does not start from this plan — it produces the design doc and Phase 1 scope that implementation will work from.

Design Decisions (confirmed with user)
Codename: skillforge
Host language: Rust (platform CLI + runtime library). Skill authors in Phase 1 also write Rust; polyglot skills deferred to Phase 3 via WASM Components.
Phase 1 focus: portability — one artifact, four call sites. Versioning, signing, and sandboxing come in later phases.
MCP compatibility: first-class. Every compiled skill is a valid one-tool MCP stdio server from day one.
Binary role: the whole skill compiles to a single binary with embedded prompt/schema. Runs standalone or as an LLM tool.
Skill Format
A skill is a directory, authored by hand, compiled to a binary:

my-skill/
  skill.toml            # metadata, version, entrypoints
  prompt.md             # LLM-facing instructions (required)
  schema.json           # input/output JSON Schema (required for tool mode)
  src/                  # deterministic Rust code (Phase 1)
  assets/               # static data embedded into the binary
  tests/                # golden-file determinism tests
skill.toml is the source of truth:

[skill]
name = "pdf-extract"
version = "1.4.2"
description = "Extract structured text from PDFs."
license = "Apache-2.0"

[runtime]
kind = "rust"            # rust | wasm-component (later) | script (later)
entrypoint = "src/main.rs"
determinism = "pure"     # pure | io-bounded | llm-assisted

[interfaces]
mcp  = true
cli  = true
http = true
lib  = true
Rationale:

TOML over YAML: unambiguous, no indentation hazards, native to the Rust/Cargo ecosystem.
JSON Schema over WIT for Phase 1: every LLM tool-call API in existence already speaks it. WIT is the right long-term answer for typed polyglot interfaces; it arrives with WASM Components in Phase 3.
The Binary Runtime Model
Each skill compiles to one self-contained binary named <skill>-<version>-<target>. Embedded via include_bytes!:

skill.toml
prompt.md
schema.json
Signed manifest hash (Phase 2+)
The binary exposes four modes, selected by the first argument — this is the core portability thesis:

pdf-extract tool        # MCP stdio server (this one tool)
pdf-extract run ...     # CLI: deterministic invocation with flags
pdf-extract serve       # HTTP/SSE server (MCP Streamable HTTP)
pdf-extract describe    # emits prompt.md + schema.json as JSON
Determinism contract: the determinism field is enforced by the test harness:

pure — no network, no clock, no filesystem outside assets/; must pass golden tests.
io-bounded — declared side effects only.
llm-assisted — binary embeds a minimal Anthropic/OpenAI client; caller supplies the key.
The runtime takes the shortest deterministic path: run invokes code directly, tool/serve expose the prompt to the LLM only when orchestration is needed.

Distribution & Versioning (Phase 2)
Registry: OCI artifacts via ORAS. Skills push to ghcr.io/<publisher>/<skill>:<semver>. No custom registry to operate; works with GHCR, Docker Hub, ECR, Harbor, zot.
Signing: Sigstore (Cosign keyless) as primary; Minisign as offline fallback.
Versioning: strict semver, enforced at publish time. Skills declare platform compatibility ranges.
Trust model: TOFU + pinned publisher identity. First install records the sigstore identity; later upgrades must match or re-prompt.
Auto-update: opt-in per skill via a workspace skillforge.toml. skillforge upgrade resolves, verifies signatures, swaps binaries atomically.
Transport & Interop
                    +---------------- skill binary -----------------+
                    | embedded: prompt.md, schema.json, code        |
Claude Code -----> | tool   (MCP stdio)                            |
Remote agent ----> | serve  (HTTP/SSE, MCP Streamable)             |
Human shell -----> | run    (CLI flags)                            |
Rust program ----> | libskill.a (cdylib, FFI)                      |
                    +-----------------------------------------------+
A separate skillforge-mux binary aggregates many skills behind a single MCP endpoint, avoiding subprocess-per-tool overhead — but it's not required; every skill is a standalone MCP server.

Execution Model
Default: per-invocation cold start on the client box. A 3 MB Rust binary starts in <20 ms — faster than MCP's stdio handshake.
No long-running daemon by default. skillforge-mux exists for callers who want one.
Sandboxing (Phase 3): trusted (in-process) → sandboxed (landlock / sandbox-exec with manifest-declared allow-lists) → wasm (wasm32-wasip2 under wasmtime).
Hosted execution is out of scope. The registry hosts bytes; it does not run skills.
Phased Roadmap
Phase 1 — "One artifact, four call sites" (4–6 weeks)
Proves the portability thesis. No distribution, no signing, no sandboxing.

skillforge CLI: new, build, run, tool, serve, describe.
Rust-only skill template via cargo-dist.
Local-only distribution (~/.skillforge/skills/).
Three reference skills shipped: pdf-extract, git-blame-summary, jq-helper.
First-class MCP stdio mode: a built skill drops into Claude Code / Cursor / Claude Desktop with zero additional runtime.
Validation goal: a Claude Code user can add a skillforge skill as an MCP tool in under 60 seconds.
Phase 2 — Distribution & signing (4–6 weeks)
ORAS push/pull against GHCR.
Cosign keyless signing at publish; verification on install.
skillforge install, upgrade, trust policy file.
Semver resolution.
MCP Streamable HTTP (serve mode) hardened for remote invocation.
Phase 3 — Determinism & sandboxing (6–8 weeks)
Golden-test harness; determinism enforcement in CI.
landlock / sandbox-exec tiers.
WASM Component target; polyglot skills (Go, TS via Deno compile) via wit-bindgen.
Multi-registry support beyond GHCR.
Critical Files (Phase 1)
crates/skillforge-cli/src/main.rs — skillforge CLI entrypoint: new, build, run, tool, serve, describe.
crates/skillforge-core/src/manifest.rs — skill.toml parsing, validation, embedding into the final binary.
crates/skillforge-runtime/src/modes.rs — the four-mode dispatcher every compiled skill links against.
crates/skillforge-mcp/src/stdio.rs — MCP stdio server wrapper exposing a single skill as a one-tool MCP server.
templates/rust-skill/skill.toml — canonical template used by skillforge new.
templates/rust-skill/src/main.rs — template skill entrypoint wired to skillforge-runtime.
skills/pdf-extract/ — first reference skill; exercises all four modes.
Cargo.toml — workspace root.
Verification (Phase 1 exit criteria)
End-to-end tests that must pass before Phase 1 ships:

skillforge new my-skill → skillforge build produces a single binary under 5 MB.
./my-skill run --input ... returns structured output matching schema.json without any LLM call.
./my-skill describe emits the embedded prompt.md and schema.json as JSON on stdout.
./my-skill tool passes MCP conformance: initialize, tools/list, tools/call round-trips over stdio in a harnessed test.
Claude Code integration test: add the binary to ~/.claude/mcp.json as an stdio server; the tool appears in Claude Code and a call succeeds. Measure time-to-working from skillforge build to first successful call — target under 60 seconds.
./my-skill serve + curl round-trips over MCP Streamable HTTP.
Three reference skills (pdf-extract, git-blame-summary, jq-helper) all pass (1)–(6).
Open Questions
These should be resolved before or during Phase 1 implementation:

Registry at MVP: GHCR-only for Phase 2, or design for generic OCI from the start? (Lean: GHCR-only, generalize in Phase 3.)
Prompt templating: is prompt.md static, or does it support variable substitution from inputs? (Lean: static at MVP; templating is a footgun.)
MCP surface: tools are table stakes. Do we expose resources and prompts? (Lean: punt to Phase 2.)
Skill composition: can skill A call skill B? (Lean: no at MVP; composition belongs at the agent layer.)
Sigstore identity shape: email-based, GitHub OIDC, or both? (Phase 2 decision.)
llm-assisted skills: should the embedded LLM client be pluggable (user-supplied at build time) or fixed (Anthropic + OpenAI)? (Phase 3 decision.)