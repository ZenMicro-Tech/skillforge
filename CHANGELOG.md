# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

## [0.1.6] - 2026-07-15

### Added

- `--create-index` flag on `skillforge publish` to create a multi-arch OCI image index from previously-pushed per-platform manifests without rebuilding.

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

[Unreleased]: https://github.com/ZenMicro-Tech/ai-skills-platform/compare/v0.1.6...HEAD
[0.1.6]: https://github.com/ZenMicro-Tech/ai-skills-platform/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/ZenMicro-Tech/ai-skills-platform/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/ZenMicro-Tech/ai-skills-platform/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/ZenMicro-Tech/ai-skills-platform/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/ZenMicro-Tech/ai-skills-platform/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/ZenMicro-Tech/ai-skills-platform/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/ZenMicro-Tech/ai-skills-platform/releases/tag/v0.1.0
