# Changelog

All notable changes to this project will be documented in this file.

## [1.0.0] - 2026-07-18

### Added

- Initial release of `deep-dive`, a Rust rewrite of `dive`.
- CLI that accepts image URIs with explicit schemes:
  `docker://`, `docker-archive://`, `oci://`, `registry://`, and `podman://`.
- Docker save tar parser with gzip and uncompressed layer support.
- OCI layout directory parser with gzip and zstd layer support.
- Docker daemon resolver via `bollard` with progress reporting.
- Podman resolver via `podman image save` with progress reporting.
- OCI registry resolver via `oci-client` with Docker config authentication.
- `FileTree` with whiteout and opaque-whiteout handling.
- `Comparer` with stacked layer views, diff marking, and cached indexing.
- Natural and aggregated compare modes.
- Full ratatui TUI with four panes: layers, file tree, layer details, and image details.
- Layer list, file tree navigation, collapse/expand, sorting, and regex filtering.
- Extract-to popup with configurable default directory.
- Open-image dialog to switch images without restarting.
- Analyzer framework with `Report` collector.
- Efficiency analyzer showing wasted bytes and score.
- Layer stats analyzer with per-layer size, unique size, and compression metrics.
- Shaded file analyzer detecting hidden file copies across layers.
- YAML configuration file support.
- Comprehensive unit and integration test suite.
- GitHub Actions CI and cargo-dist release workflow.

## [0.1.0] - 2026-07-17

### Added

- Project skeleton: CLI parsing, CI/CD, and release infrastructure.

[1.0.0]: https://github.com/dmikusa/deep-dive/releases/tag/v1.0.0
[0.1.0]: https://github.com/dmikusa/deep-dive/releases/tag/v0.1.0
