# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

## [0.1.10] - 2026-07-28

### Added

- `skillforge login [registry]` / `skillforge logout [registry]` to authenticate against an OCI registry (default `ghcr.io`) without depending on Docker or `oras`. Credentials are verified against the registry and stored in `~/.skillforge/credentials.json` (mode `0600`), and are checked first by `publish`/`add`/`search` before falling back to Docker config or `gh auth`.

## [0.1.9] - 2026-07-23

### Added

- `skillforge search --info <name>` now lists every available version in newest-first order and shows commands for installing the latest or a pinned release.
- Docker-style bare public-skill references are supported: `skillforge add <name>:<version>` installs a specific version from the default Skillforge GHCR namespace.

### Changed

- `skillforge add <name>` now resolves a matching local skill first, then falls back directly to `ghcr.io/zenmicro-tech/skillforge/skills/<name>` and selects the latest compatible version.

## [0.1.8] - 2026-07-20

### Added

- Agent adapters for Visual Studio Code, GitHub Copilot, and Windsurf.
- `skillforge upgrade [name] [--check]` to check for and install available catalog updates for all installed skills or a named skill.
- `skillforge add` now accepts multiple local skill names and OCI references in one invocation.
- `skillforge remove` now accepts multiple skill names in one invocation (for example, `skillforge remove git github`).

### Changed

- OCI pulls now select the image variant matching the current operating system and CPU architecture from multi-platform image indexes.

## [0.1.7] - 2026-07-16

### Added

- `--create-index` flag on `skillforge publish` to create a multi-arch OCI image index from previously-pushed per-platform manifests without rebuilding.
- Automatic `:latest` tag pushed alongside versioned tags on publish.

### Fixed

- `skillforge publish --target <triple>` now pushes to a platform-specific tag (e.g. `:0.1.0-darwin-arm64`) instead of the bare version tag, so parallel CI runners no longer overwrite each other.

## [0.1.6] - 2026-07-15

### Changed

- Version bump (no user-facing changes).

## [0.1.5] - 2026-07-13

### Changed

- Renamed `repo` to `registry` in publish command and manifest structure for clarity.

## [0.1.4] - 2026-07-09

### Added

- `skillforge search` command for discovering skills in a remote catalog registry.
- Documentation site (`index.html`).
- Catalog metadata serialization in `push_catalog_entry` for richer registry listings.

### Fixed

- Installation instructions updated to use `~/.local/bin`.

### Changed

- OCI registry authentication now supports Docker credential helpers, credential stores, and static `~/.docker/config.json` auths in addition to GitHub token fallback.

## [0.1.3] - 2026-06-18

### Fixed

- Repository links and version references in Cargo.toml and README.

## [0.1.2] - 2026-06-18

### Fixed

- `skillforge --version` now reports the correct version.

## [0.1.1] - 2026-06-17

### Added

- Multi-arch publish support (`--target` flag, OCI image index creation).
- Full OCI spec integration for artifact push/pull.

### Changed

- Updated Rust edition and aligned workspace package versions.

## [0.1.0] - 2026-06-08

### Added

- Initial release of `skillforge` CLI.
- OCI artifact push/pull using `oci-client` (no external `oras` dependency).
- GitHub Actions workflow for release automation.
- Skill scaffolding (`skillforge new`), build, run, and link/unlink commands.
- MCP stdio server mode (`skillforge tool`).

[Unreleased]: https://github.com/ZenMicro-Tech/ai-skills-platform/compare/v0.1.10...HEAD
[0.1.10]: https://github.com/ZenMicro-Tech/ai-skills-platform/compare/v0.1.9...v0.1.10
[0.1.9]: https://github.com/ZenMicro-Tech/ai-skills-platform/compare/v0.1.8...v0.1.9
[0.1.8]: https://github.com/ZenMicro-Tech/ai-skills-platform/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/ZenMicro-Tech/ai-skills-platform/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/ZenMicro-Tech/ai-skills-platform/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/ZenMicro-Tech/ai-skills-platform/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/ZenMicro-Tech/ai-skills-platform/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/ZenMicro-Tech/ai-skills-platform/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/ZenMicro-Tech/ai-skills-platform/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/ZenMicro-Tech/ai-skills-platform/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/ZenMicro-Tech/ai-skills-platform/releases/tag/v0.1.0
