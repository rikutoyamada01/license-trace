# license-trace

[English](README.md) | [日本語](README.ja.md)

> Recursive Dependency License Tracer, Obligations Evaluator, and Outbound Compliance Auditor.

`license-trace` is a high-performance compliance tool designed to recursively inspect direct and transitive dependency graphs, compute aggregate legal obligations (lower-bound requirements), and verify whether a project can be safely distributed under a target open-source license (such as MIT or Apache-2.0).

---

## Key Features

- **Multi-Ecosystem Auto-Detection**: Seamlessly inspects Rust (Cargo), Node.js (npm), Python (pip/Poetry), and Go (go.mod) projects.
- **Unified Tracing Interface**: Inspect local folders (`.`), uninstalled registry packages (`express`), or public Git repositories from a single command.
- **Strict SPDX AST Evaluation**: Accurately processes complex expressions (`OR` takes the best permissible branch; `AND` combines strict requirements).
- **Outbound License Compatibility Check**: Detects Strong Copyleft (GPL), Network Copyleft (AGPL/SSPL), or Non-Commercial (BUSL/CC-BY-NC) conflicts against your intended release license.
- **Obligations Lower-Bound Analysis**: Calculates aggregated distribution requirements, including copyright notice attribution, source disclosure levels, and patent grants.
- **Deep Path Tracing (`why`)**: Uses graph traversal to pinpoint every route through which an incompatible or unknown dependency entered your dependency tree.
- **CI/CD Ready**: Supports machine-readable JSON output, customizable exit codes, and automated compliance gates.

---

## Supported Ecosystems

| Ecosystem | Manifest / Lockfile | License Discovery Strategy | Online Resolution |
| :--- | :--- | :--- | :--- |
| **Rust / Cargo** | `Cargo.toml`, `Cargo.lock` | `cargo metadata` (exact package fields) | crates.io API |
| **Node.js / npm** | `package.json`, `package-lock.json` | Manifest `license` / `licenses` fields | npm Registry API |
| **Python** | `pyproject.toml`, `requirements.txt` | `.dist-info/METADATA`, PEP 621 classifiers | PyPI JSON API |
| **Go modules** | `go.mod`, `go.sum` | `go list -m -json all`, embedded LICENSE files | Go Proxy / Repo |

---

## Installation

### Prerequisites
- Rust 1.75 or later (with `cargo`)

### Build from Source
```bash
git clone https://github.com/rikutoyamada01/license-trace.git
cd license-trace
cargo build --release
```
The compiled binary will be available at `./target/release/license-trace`.

---

## Usage

### 1. Unified `trace` Command

The `trace` command automatically identifies whether the target is a local directory, a remote registry package, or a Git URL.

#### A. Audit Current Project (Local)
Inspects the current working directory, detects the ecosystem, and evaluates compliance against MIT:
```bash
license-trace trace .
```

To evaluate against a different outbound license (e.g. dual license):
```bash
license-trace trace . --outbound "MIT OR Apache-2.0"
```

To exclude development dependencies:
```bash
license-trace trace . --prod-only
```

#### B. Pre-screen Remote Packages (Registry)
Inspects an uninstalled package and its transitive dependency tree via registry APIs before adding it to your codebase:
```bash
license-trace trace express
license-trace trace lodash@4.17.21 --max-depth 3
```

#### C. Inspect Public Git Repositories
Clones a remote repository shallowly into a temporary directory and audits its dependency tree:
```bash
license-trace trace https://github.com/expressjs/express
```

---

### 2. Trace Dependency Path (`why`)

Pinpoints exactly how a specific library was brought into your project:
```bash
license-trace why <package-name>
```

#### Example Output:
```text
Dependency path search for 'nested-copyleft-tool':

    Found 2 path(s) from root [my-project@1.0.0]:

    Route 01: my-project -> direct-service@2.1.0 -> nested-copyleft-tool@0.4.1
    Route 02: my-project -> helper-utils@1.0.0 -> legacy-core@1.2.0 -> nested-copyleft-tool@0.4.1
```

---

### 3. Output Formats

`license-trace` supports multiple visualization and export formats:

```bash
# Standard Terminal Audit Report (Default)
license-trace trace . --format audit

# Tabular Overview
license-trace trace . --format table

# Pure Dependency Tree
license-trace trace . --format tree

# Machine-readable JSON (for CI/CD pipelines)
license-trace trace . --format json

# Export third-party notice markdown directly
license-trace trace . --format markdown
```

---

### 4. Export Third-Party Licenses File (`export`)

Generate a compliant `THIRD_PARTY_LICENSES.md` file for distribution and release artifacts:
```bash
license-trace export --output THIRD_PARTY_LICENSES.md
```

---

## Visual Report Sample (Auditing `license-trace` Itself)

Running `license-trace trace .` on this repository produces the following real-world audit:

```text
$ license-trace trace .

=== License Trace & Compliance Audit ===
Target Outbound License : MIT
Audit Status            : [COMPATIBLE]
Summary                 : All dependencies are compatible with 'MIT'. Safe for distribution!

--- Dependency Tree ---
license-trace@0.1.0 [MIT]
|-- tokio@1.53.1 [MIT]
|   |-- windows-sys@0.61.2 [MIT OR Apache-2.0]
|   |   `-- windows-link@0.2.1 [MIT OR Apache-2.0]
|   |-- tokio-macros@2.7.2 [MIT]
|   |   |-- syn@3.0.3 [MIT OR Apache-2.0]
|   |   |   |-- unicode-ident@1.0.24 [(MIT OR Apache-2.0) AND Unicode-3.0]
|   |   |   `-- quote@1.0.47 [MIT OR Apache-2.0]
|   |   `-- quote@1.0.47 [MIT OR Apache-2.0] (deduped)
|   |-- socket2@0.6.5 [MIT OR Apache-2.0]
|   |-- pin-project-lite@0.2.17 [Apache-2.0 OR MIT]
|   |-- parking_lot@0.12.5 [MIT OR Apache-2.0]
|   |   `-- parking_lot_core@0.9.12 [MIT OR Apache-2.0]
|   `-- bytes@1.12.1 [MIT]
|-- reqwest@0.12.28 [MIT OR Apache-2.0]
|   |-- webpki-roots@1.0.9 [CDLA-Permissive-2.0]
|   |   `-- rustls-pki-types@1.15.1 [MIT OR Apache-2.0]
|   |-- url@2.5.8 [MIT OR Apache-2.0]
|   |   `-- idna@1.1.0 [Apache-2.0 OR MIT]
|   |       `-- idna_adapter@1.2.2 [Apache-2.0 OR MIT]
|   |           `-- icu_properties@2.3.0 [Unicode-3.0]
|   |-- tokio-rustls@0.26.4 [MIT OR Apache-2.0]
|   |   `-- rustls@0.23.43 [Apache-2.0 OR ISC OR MIT]
|   |       |-- rustls-webpki@0.103.14 [ISC]
|   |       `-- ring@0.17.14 [Apache-2.0 AND ISC]
|   `-- hyper-util@0.1.20 [MIT]
|-- petgraph@0.7.1 [MIT OR Apache-2.0]
|   `-- fixedbitset@0.5.7 [MIT OR Apache-2.0]
|-- serde_json@1.0.151 [MIT OR Apache-2.0]
|   `-- memchr@2.8.3 [Unlicense OR MIT]
|-- owo-colors@4.3.0 [MIT]
|-- comfy-table@7.2.2 [MIT]
|-- clap@4.6.6 [MIT OR Apache-2.0]
|-- spdx@0.10.9 [MIT OR Apache-2.0]
`-- anyhow@1.0.104 [MIT OR Apache-2.0]

--- License Breakdown ---
  - (MIT OR Apache-2.0) AND Unicode-3.0 : 1 package(s)
  - Apache-2.0                   : 5 package(s)
  - Apache-2.0 AND ISC           : 1 package(s)
  - Apache-2.0 OR BSL-1.0        : 1 package(s)
  - Apache-2.0 OR ISC OR MIT     : 2 package(s)
  - Apache-2.0 OR MIT            : 9 package(s)
  - Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT : 3 package(s)
  - BSD-3-Clause                 : 1 package(s)
  - CDLA-Permissive-2.0          : 1 package(s)
  - ISC                          : 2 package(s)
  - MIT                          : 27 package(s)
  - MIT OR Apache-2.0            : 106 package(s)
  - MIT OR Apache-2.0 OR LGPL-2.1-or-later : 1 package(s)
  - MIT OR Apache-2.0 OR Zlib    : 2 package(s)
  - Unicode-3.0                  : 18 package(s)
  - Unlicense OR MIT             : 1 package(s)
  - Zlib OR Apache-2.0 OR MIT    : 1 package(s)

--- Obligations (Lower Bound) ---
  Commercial Use       : Allowed
  Distribution         : Allowed
  Copyright / Notice   : Required
  Source Disclosure    : None (Permissive)
```

---

## License Categories & Compatibility Matrix

| Category | Typical Licenses | Commercial Use | Source Disclosure | Compatibility with Permissive (MIT / Apache-2.0) |
| :--- | :--- | :--- | :--- | :--- |
| **Permissive** | MIT, Apache-2.0, BSD, ISC, 0BSD, Unlicense | Yes | None | **Compatible** |
| **Weak Copyleft** | LGPL, MPL-2.0, EPL, CDDL, CC-BY-SA | Yes | Library-level | **Warning** (Must retain unmodified dynamic links) |
| **Strong Copyleft** | GPL-2.0, GPL-3.0 | Yes | Project-wide | **Incompatible** (Viral source disclosure) |
| **Network Copyleft** | AGPL-3.0, SSPL | Yes | Network-triggered | **Incompatible** (SaaS disclosure trigger) |
| **Non-Commercial** | CC-BY-NC, BUSL-1.1 | No | Varies | **Incompatible** (Commercial distribution forbidden) |
| **Unknown** | Unrecognized / Custom | Manual Review | Unknown | **Needs Review** |

---

## CI / CD Integration

Use exit codes to enforce compliance gates in continuous integration:

### Exit Codes
- `0`: Compatible (Audit passed)
- `1`: Incompatible license constraints detected
- `2`: Unknown licenses require manual review

### GitHub Actions Workflow Example

```yaml
name: License Compliance Audit

on:
  push:
    branches: [ main ]
  pull_request:
    branches: [ main ]

jobs:
  license-check:
    runs-on: ubuntu-latest
    steps:
      - name: Checkout Code
        uses: actions/checkout@v4

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Install license-trace
        run: cargo install --path .

      - name: Run Compliance Audit
        run: |
          license-trace trace . \
            --outbound MIT \
            --prod-only \
            --fail-on-incompatible \
            --fail-on-unknown
```

---

## Contributing

Contributions are welcome. Please ensure that all pull requests maintain 100% test coverage and follow Rust standard formatting (`cargo fmt` and `cargo clippy`).

```bash
cargo test
cargo clippy -- -D warnings
```

---

## License

This project is licensed under the **MIT License**. See the [LICENSE](LICENSE) file for details.
