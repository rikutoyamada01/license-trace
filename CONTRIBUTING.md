# Contributing to license-trace

Thank you for your interest in contributing to `license-trace`! We welcome contributions from the community.

## Development Setup

### Prerequisites
- [Rust](https://www.rust-lang.org/tools/install) 1.75 or higher (with `cargo`, `rustfmt`, and `clippy`)
- Git

### Getting Started

```bash
git clone https://github.com/rikutoyamada01/license-trace.git
cd license-trace
cargo test
```

## Pull Request Guidelines

1. **Create a branch:** Create a feature or bugfix branch off `master` (e.g. `feat/new-ecosystem-resolver` or `fix/spdx-eval-edge-case`).
2. **Code Standards:**
   - Format code using standard rustfmt:
     ```bash
     cargo fmt --all -- --check
     ```
   - Ensure zero Clippy warnings:
     ```bash
     cargo clippy --all-targets --all-features -- -D warnings
     ```
3. **Tests:**
   - Add unit tests for all new resolvers, parser edge-cases, or policy rules.
   - Run the full test suite:
     ```bash
     cargo test --all-targets --all-features
     ```
4. **Third-Party Notices:**
   - If adding dependencies to `Cargo.toml`, ensure they are Permissive and run `license-trace export --output THIRD_PARTY_LICENSES.md` to refresh the notice bundle.

## Commit Message Convention

We follow conventional commit guidelines (e.g., `feat: ...`, `fix: ...`, `docs: ...`, `refactor: ...`, `test: ...`).
Include GitHub issue reference keywords (e.g. `Closes #123`) when resolving open issues.
