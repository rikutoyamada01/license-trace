# license-trace

[English](README.md) | [日本語](README.ja.md)

> 依存関係の再帰的ライセンストラッカー、義務下限の集約評価、およびOSS公開互換性監査ツール。

`license-trace` は、プロジェクトの直接・間接（推移的）依存グラフを再帰的に走査し、各ライセンスから生じる法的な義務・制約の下限（最も厳しい要件）を集約・算定した上で、目的のオープンソースライセンス（MIT や Apache-2.0 など）で安全に公開・配布できるかを自動判定するコンプライアンスツールです。

---

## 主な機能

- **マルチエコシステムの自動判別**: Rust (Cargo)、Node.js (npm)、Python (pip/Poetry)、Go (go modules) のプロジェクトを自動認識して解析。
- **統一された trace インターフェース**: ローカルフォルダ (`.`)、未インストールの公開パッケージ (`express`)、公開 Git リポジトリを単一のコマンドで監査。
- **厳密な SPDX AST 評価**: `OR` 式（利用者が選べる最も有利なライセンス）と `AND` 式（両方の義務を満たす必要がある最悪ケース）をスタックマシンで正確に評価。
- **Outbound ライセンス適合性判定**: 自身が公開する予定のライセンスに対し、Strong Copyleft (GPL)、Network Copyleft (AGPL/SSPL)、非商用制限 (BUSL/CC-BY-NC) が混入していないかを検証。
- **義務・制約の下限集約**: 著作権表示（NOTICE）要件、ソースコード開示義務の範囲（ライブラリ単位 / プロジェクト全体 / ネットワーク経由）、特許許諾を集約。
- **混入経路のピンポイント追跡 (`why`)**: 問題のあるライセンスや要調査ライセンスがどの依存経路（ルートからターゲットまで）を通って混入したかを最短パスおよび全経路で可視化。
- **CI/CD 自動化対応**: 機械可読な JSON 出力、終了コードによるパイプライン制御、厳格な監査ゲートに対応。

---

## 対応エコシステム

| エコシステム | 設定ファイル / ロックファイル | ライセンス取得戦略 | オンライン解決 |
| :--- | :--- | :--- | :--- |
| **Rust / Cargo** | `Cargo.toml`, `Cargo.lock` | `cargo metadata` (正確なパッケージメタデータ) | crates.io API |
| **Node.js / npm** | `package.json`, `package-lock.json` | マニフェストの `license` / `licenses` フィールド | npm Registry API |
| **Python** | `pyproject.toml`, `requirements.txt` | `.dist-info/METADATA`, PEP 621 分類 | PyPI JSON API |
| **Go modules** | `go.mod`, `go.sum` | `go list -m -json all`, 組み込み LICENSE ファイル | Go Proxy / リポジトリ |

---

## インストール

### 前提条件
- Rust 1.75 以上 (`cargo`)

### ソースコードからビルド
```bash
git clone https://github.com/user/license-trace.git
cd license-trace
cargo build --release
```
ビルドされた実行ファイルは `./target/release/license-trace` に配置されます。

---

## 使い方

### 1. 統一された `trace` コマンド

`trace` コマンドは、指定されたターゲットがローカルディレクトリ、レジストリパッケージ名、Git URL のいずれであるかを自動判別します。

#### A. 現在のプロジェクトを監査（ローカル）
カレントディレクトリを監査し、エコシステムを自動判別して MIT ライセンスに対する適合性を検証します：
```bash
license-trace trace .
```

デュアルライセンスなど、別の公開ライセンスを想定して監査する場合：
```bash
license-trace trace . --outbound "MIT OR Apache-2.0"
```

開発用依存（devDependencies 等）を除外し、本番配布物のみを監査する場合：
```bash
license-trace trace . --prod-only
```

#### B. 導入前パッケージの事前調査（レジストリ）
未インストールのパッケージを公開レジストリ API 経由で取得し、依存ツリーとライセンスをインストール前に確認します：
```bash
license-trace trace express
license-trace trace lodash@4.17.21 --max-depth 3
```

#### C. 公開 Git リポジトリの直接監査
リモートリポジトリを一時ディレクトリに浅くクローン（`--depth 1`）して即座に監査します：
```bash
license-trace trace https://github.com/expressjs/express
```

---

### 2. 依存関係の混入経路を特定 (`why`)

特定のライブラリがどのパッケージを経由してプロジェクトに入り込んだのかを追跡します：
```bash
license-trace why <package-name>
```

#### 出力例:
```text
Dependency path search for 'nested-copyleft-tool':

    Found 2 path(s) from root [my-project@1.0.0]:

    Route 01: my-project -> direct-service@2.1.0 -> nested-copyleft-tool@0.4.1
    Route 02: my-project -> helper-utils@1.0.0 -> legacy-core@1.2.0 -> nested-copyleft-tool@0.4.1
```

---

### 3. 出力フォーマット

`license-trace` は複数の表示・出力形式をサポートしています：

```bash
# 標準ターミナル監査レポート（デフォルト）
license-trace trace . --format audit

# 一覧テーブル形式
license-trace trace . --format table

# 階層ツリー形式
license-trace trace . --format tree

# CI/CD パイプライン用 JSON 出力
license-trace trace . --format json

# サードパーティライセンス通知の直接出力
license-trace trace . --format markdown
```

---

### 4. サードパーティライセンス一覧ファイルの自動生成 (`export`)

配布物やリポジトリに同梱する `THIRD_PARTY_LICENSES.md` ファイルを一括自動生成します：
```bash
license-trace export --output THIRD_PARTY_LICENSES.md
```

---

## ターミナル出力例（本リポジトリ自身の監査結果）

本リポジトリ上で `license-trace trace .` を実行した実際の監査レポート例です：

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

## ライセンス分類と互換性マトリクス

| カテゴリ | 代表的なライセンス | 商用利用 | ソース開示義務 | Permissive (MIT / Apache-2.0) との互換性 |
| :--- | :--- | :--- | :--- | :--- |
| **Permissive** (寛容型) | MIT, Apache-2.0, BSD, ISC, 0BSD, Unlicense | 可 | なし | **Compatible** (適合) |
| **Weak Copyleft** (弱コピーレフト) | LGPL, MPL-2.0, EPL, CDDL, CC-BY-SA | 可 | ライブラリ単位 | **Warning** (動的リンク・非改変の確認が必要) |
| **Strong Copyleft** (強コピーレフト) | GPL-2.0, GPL-3.0 | 可 | プロジェクト全体 | **Incompatible** (MIT 単独での配布は不可) |
| **Network Copyleft** (ネットワーク型) | AGPL-3.0, SSPL | 可 | ネットワーク経由でも開示 | **Incompatible** (SaaS 提供時も開示義務が発生) |
| **Non-Commercial** (非商用限定) | CC-BY-NC, BUSL-1.1 | 不可 | 規定による | **Incompatible** (商用利用・無償再配布が制限) |
| **Unknown** (未確定・独自) | 非標準 / 記載なし | 要手動確認 | 不明 | **Needs Review** (手動レビューが必要) |

---

## CI / CD パイプライン連携

終了コードを活用して、継続的インテグレーション（CI）でライセンス違反を自動検知・ブロックできます。

### 終了コード仕様
- `0`: 適合 (監査パス)
- `1`: 非互換なライセンス制約を検知
- `2`: 未知・非標準ライセンスが含まれる（手動レビューが必要）

### GitHub Actions ワークフロー設定例

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

## コントリビューション

プルリクエストを歓迎します。すべての変更において単体テストがパスし、標準のコードフォーマット規則（`cargo fmt` および `cargo clippy`）に準拠していることをご確認ください。

```bash
cargo test
cargo clippy -- -D warnings
```

---

## ライセンス

本プロジェクトは **MIT License** のもとで公開されています。詳細は [LICENSE](LICENSE) ファイルをご参照ください。
