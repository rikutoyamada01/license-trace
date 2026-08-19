# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-08-19

### Added
- Multi-ecosystem dependency license tracking and compliance analysis (Cargo, npm, PyPI, Go modules).
- Unified CLI interface supporting local directories, remote registry packages, and Git URLs (`trace`).
- Strict SPDX AST evaluation engine (`OR` chooses best permissible branch; `AND` combines strict requirements).
- Outbound license compatibility policy engine (evaluating against MIT, Apache-2.0, dual-licenses).
- Aggregated lower-bound legal obligations calculator (commercial use, source disclosure level, copyright notice attribution).
- Deep dependency path tracing (`why <package>`) via shortest-path BFS and all-simple-paths graph traversal.
- Multi-format reporting: Audit terminal view, Comfy-Table view, Tree structure, JSON, and Markdown.
- Third-party notices and licenses generator (`export --output THIRD_PARTY_LICENSES.md`).
- Multi-OS GitHub Actions CI/CD workflows and full test coverage suite.
