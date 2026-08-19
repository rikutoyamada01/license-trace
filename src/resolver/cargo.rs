use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::model::graph::DependencyGraph;
use crate::model::package::{DependencyScope, DependencyType, PackageId, PackageInfo};

pub struct CargoResolver;

impl CargoResolver {
    /// Cargo プロジェクトの依存関係とライセンスを解決
    pub fn resolve_project(project_dir: &Path) -> Result<DependencyGraph> {
        let cargo_toml_path = project_dir.join("Cargo.toml");
        if !cargo_toml_path.exists() {
            anyhow::bail!("No Cargo.toml found at '{}'", project_dir.display());
        }

        // 1. まず cargo metadata コマンドの実行を試行 (最も正確)
        if let Ok(graph) = Self::resolve_via_cargo_metadata(project_dir) {
            return Ok(graph);
        }

        // 2. フォールバック: Cargo.toml + Cargo.lock 手動解析
        Self::resolve_via_files(project_dir)
    }

    /// `cargo metadata --format-version 1` による正確な依存関係・ライセンス解析
    fn resolve_via_cargo_metadata(project_dir: &Path) -> Result<DependencyGraph> {
        let output = Command::new("cargo")
            .arg("metadata")
            .arg("--format-version")
            .arg("1")
            .current_dir(project_dir)
            .output()
            .context("Failed to execute cargo metadata")?;

        if !output.status.success() {
            anyhow::bail!(
                "cargo metadata exited with non-zero status: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let meta: Value = serde_json::from_slice(&output.stdout)
            .context("Failed to parse cargo metadata JSON")?;

        let packages = meta.get("packages").and_then(|v| v.as_array())
            .context("No packages in metadata")?;

        let resolve = meta.get("resolve").and_then(|v| v.as_object())
            .context("No resolve in metadata")?;

        let root_resolve_id = resolve.get("root").and_then(|v| v.as_str());

        // パッケージ情報マップ (id -> PackageInfo)
        let mut pkg_map: HashMap<String, (PackageInfo, String, String)> = HashMap::new();
        let mut root_pkg_id: Option<PackageId> = None;
        let mut root_full_id: Option<String> = None;

        for pkg in packages {
            let full_id = pkg.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let name = pkg.get("name").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let version = pkg.get("version").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let manifest_path = pkg.get("manifest_path").and_then(|v| v.as_str());

            let mut license = pkg.get("license").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
            let mut license_text: Option<String> = None;

            if let Some(m_path) = manifest_path {
                let crate_dir = Path::new(m_path).parent();
                if let Some(dir) = crate_dir {
                    // 1. 実ファイルの全文を読み取る
                    license_text = load_crate_license_text(dir);

                    // 2. license フィールドが空の場合のみファイルから特定
                    if license.is_empty() || license.eq_ignore_ascii_case("unknown") {
                        if let Some(detected) = detect_crate_license_files(dir) {
                            license = detected;
                        }
                    }
                }
            }

            if license.is_empty() {
                license = "UNKNOWN".to_string();
            }

            let is_root = root_resolve_id == Some(&full_id) || (root_resolve_id.is_none() && root_pkg_id.is_none());

            let pkg_id = PackageId::new(&name, &version);
            let dep_type = if is_root { DependencyType::Direct } else { DependencyType::Transitive };
            let mut info = PackageInfo::new(pkg_id.clone(), &license, dep_type, DependencyScope::Production);
            
            info.description = pkg.get("description").and_then(|v| v.as_str()).map(|s| s.to_string());
            info.repository = pkg.get("repository").and_then(|v| v.as_str()).map(|s| s.to_string());
            info.manifest_path = manifest_path.map(|s| s.to_string());
            info.license_text = license_text;

            if is_root {
                root_pkg_id = Some(pkg_id);
                root_full_id = Some(full_id.clone());
            }

            pkg_map.insert(full_id, (info, name, version));
        }

        let root_pkg = if let Some(r_id) = &root_full_id {
            pkg_map.get(r_id).map(|(p, _, _)| p.clone()).context("Root package not found")?
        } else {
            anyhow::bail!("Could not identify root package");
        };

        let mut graph = DependencyGraph::new(root_pkg);

        // 全パッケージを事前にグラフへ登録
        for (info, _, _) in pkg_map.values() {
            graph.get_or_add_node(info.clone());
        }

        // ノード解決グラフの nodes 配列からエッジを追加
        if let Some(nodes) = resolve.get("nodes").and_then(|v| v.as_array()) {
            for node in nodes {
                let parent_full_id = node.get("id").and_then(|v| v.as_str()).unwrap_or_default();
                let parent_pkg_id = if let Some((p, _, _)) = pkg_map.get(parent_full_id) {
                    p.id.clone()
                } else {
                    continue;
                };

                // deps 配列から依存先を取得
                if let Some(deps) = node.get("deps").and_then(|v| v.as_array()) {
                    for dep in deps {
                        let dep_full_id = dep.get("pkg").and_then(|v| v.as_str()).unwrap_or_default();
                        if let Some((dep_info, _, _)) = pkg_map.get(dep_full_id) {
                            let mut dep_info_cloned = dep_info.clone();
                            if parent_pkg_id == graph.root_id {
                                dep_info_cloned.dep_type = DependencyType::Direct;
                            }
                            graph.add_dependency(&parent_pkg_id, dep_info_cloned);
                        }
                    }
                }
            }
        }

        Ok(graph)
    }

    /// ファイル直接読み込みフォールバック
    fn resolve_via_files(project_dir: &Path) -> Result<DependencyGraph> {
        let cargo_toml_path = project_dir.join("Cargo.toml");
        let toml_content = fs::read_to_string(&cargo_toml_path)?;
        let toml_val: toml_simple::SimpleToml = toml_simple::parse(&toml_content);

        let root_name = toml_val.get("package.name").unwrap_or_else(|| "my-crate".to_string());
        let root_ver = toml_val.get("package.version").unwrap_or_else(|| "0.1.0".to_string());
        let root_lic = toml_val.get("package.license").unwrap_or_else(|| "UNKNOWN".to_string());

        let root_pkg = PackageInfo::new(
            PackageId::new(&root_name, &root_ver),
            &root_lic,
            DependencyType::Direct,
            DependencyScope::Production,
        );

        let mut graph = DependencyGraph::new(root_pkg);

        let cargo_lock_path = project_dir.join("Cargo.lock");
        if cargo_lock_path.exists() {
            let lock_content = fs::read_to_string(&cargo_lock_path)?;
            Self::parse_cargo_lock_fallback(&lock_content, &mut graph, &root_name)?;
        }

        Ok(graph)
    }

    fn parse_cargo_lock_fallback(lock_content: &str, graph: &mut DependencyGraph, root_name: &str) -> Result<()> {
        let packages = toml_simple::parse_cargo_lock_packages(lock_content);

        // 各パッケージを追加（推測ではなく UNKNOWN を付与し安全にレビュー要求）
        for pkg_entry in &packages {
            if pkg_entry.name == root_name {
                continue;
            }

            let info = PackageInfo::new(
                PackageId::new(&pkg_entry.name, &pkg_entry.version),
                "UNKNOWN",
                DependencyType::Transitive,
                DependencyScope::Production,
            );
            graph.get_or_add_node(info);
        }

        // 依存関係エッジを結ぶ
        for pkg_entry in &packages {
            let parent_id = PackageId::new(&pkg_entry.name, &pkg_entry.version);
            for dep_str in &pkg_entry.dependencies {
                let dep_name = dep_str.split_whitespace().next().unwrap_or(dep_str);
                if let Some(dep_pkg) = graph.all_packages().into_iter().find(|p| p.id.name == dep_name).cloned() {
                    if pkg_entry.name == root_name {
                        graph.add_dependency(&graph.root_id.clone(), dep_pkg);
                    } else {
                        graph.add_dependency(&parent_id, dep_pkg);
                    }
                }
            }
        }

        Ok(())
    }
}

/// クレートのディレクトリから実際のライセンスファイルの中身を読み込む
fn load_crate_license_text(crate_dir: &Path) -> Option<String> {
    let mut texts = Vec::new();

    let candidate_files = [
        "LICENSE", "LICENSE.txt", "LICENSE.md", "LICENCE", "LICENCE.txt",
        "LICENSE-MIT", "LICENSE-MIT.txt", "LICENSE-MIT.md",
        "LICENSE-APACHE", "LICENSE-APACHE.txt", "LICENSE-APACHE.md",
        "LICENSE-BOOST", "COPYING", "COPYING.txt", "UNLICENSE",
    ];

    for fname in &candidate_files {
        let p = crate_dir.join(fname);
        if p.is_file() {
            if let Ok(content) = fs::read_to_string(&p) {
                let trimmed = content.trim();
                if !trimmed.is_empty() {
                    texts.push(format!("--- File: {} ---\n{}", fname, trimmed));
                }
            }
        }
    }

    if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n\n"))
    }
}

/// クレートのディレクトリからライセンスファイルを検索して特定
fn detect_crate_license_files(crate_dir: &Path) -> Option<String> {
    let has_mit = crate_dir.join("LICENSE-MIT").exists() || crate_dir.join("LICENSE-MIT.md").exists() || crate_dir.join("LICENSE-MIT.txt").exists();
    let has_apache = crate_dir.join("LICENSE-APACHE").exists() || crate_dir.join("LICENSE-APACHE.md").exists() || crate_dir.join("LICENSE-APACHE.txt").exists();

    if has_mit && has_apache {
        return Some("MIT OR Apache-2.0".to_string());
    }
    if has_mit {
        return Some("MIT".to_string());
    }
    if has_apache {
        return Some("Apache-2.0".to_string());
    }

    let general_licenses = ["LICENSE", "LICENSE.txt", "LICENSE.md", "LICENCE", "COPYING", "UNLICENSE"];
    for fname in &general_licenses {
        let p = crate_dir.join(fname);
        if p.exists() {
            if let Ok(content) = fs::read_to_string(&p) {
                let upper = content.to_uppercase();
                if upper.contains("MIT LICENSE") || upper.contains("PERMISSION IS HEREBY GRANTED, FREE OF CHARGE") {
                    return Some("MIT".to_string());
                } else if upper.contains("APACHE LICENSE") && upper.contains("VERSION 2.0") {
                    return Some("Apache-2.0".to_string());
                } else if upper.contains("BSD 3-CLAUSE") || upper.contains("REDISTRIBUTION AND USE IN SOURCE AND BINARY") {
                    return Some("BSD-3-Clause".to_string());
                } else if upper.contains("BSD 2-CLAUSE") {
                    return Some("BSD-2-Clause".to_string());
                } else if upper.contains("ISC LICENSE") {
                    return Some("ISC".to_string());
                } else if upper.contains("MOZILLA PUBLIC LICENSE") {
                    return Some("MPL-2.0".to_string());
                } else if upper.contains("PUBLIC DOMAIN") || upper.contains("UNLICENSE") {
                    return Some("Unlicense".to_string());
                } else if upper.contains("BOOST SOFTWARE LICENSE") {
                    return Some("BSL-1.0".to_string());
                }
            }
        }
    }

    // Cargo.toml の中身から license を再度パース
    let cargo_toml_path = crate_dir.join("Cargo.toml");
    if let Ok(content) = fs::read_to_string(&cargo_toml_path) {
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("license") {
                let rest_trimmed = rest.trim();
                if let Some(val) = rest_trimmed.strip_prefix('=') {
                    let lic = val.trim().trim_matches('"').trim_matches('\'').trim();
                    if !lic.is_empty() && !lic.eq_ignore_ascii_case("unknown") {
                        return Some(lic.to_string());
                    }
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inspect_cargo_metadata() {
        let output = Command::new("cargo")
            .args(["metadata", "--format-version", "1"])
            .output()
            .unwrap();
        let meta: Value = serde_json::from_slice(&output.stdout).unwrap();
        let packages = meta.get("packages").and_then(|v| v.as_array()).unwrap();
        for p in packages {
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or_default();
            if name == "clap" || name == "comfy-table" || name == "futures" || name == "mio" {
                println!(
                    "CRATE: {} => license: {:?}, license_file: {:?}, manifest_path: {:?}",
                    name,
                    p.get("license"),
                    p.get("license_file"),
                    p.get("manifest_path")
                );
            }
        }
    }
}

mod toml_simple {
    use std::collections::HashMap;

    pub struct SimpleToml {
        values: HashMap<String, String>,
    }

    impl SimpleToml {
        pub fn get(&self, key: &str) -> Option<String> {
            self.values.get(key).cloned()
        }
    }

    pub fn parse(content: &str) -> SimpleToml {
        let mut values = HashMap::new();
        let mut current_section = "".to_string();

        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('[') && line.ends_with(']') {
                current_section = line[1..line.len() - 1].trim().to_string();
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                let k = k.trim();
                let v = v.trim().trim_matches('"').trim_matches('\'').trim();
                let full_key = if current_section.is_empty() {
                    k.to_string()
                } else {
                    format!("{}.{}", current_section, k)
                };
                values.insert(full_key, v.to_string());
            }
        }

        SimpleToml { values }
    }

    #[derive(Debug)]
    pub struct CargoLockPackage {
        pub name: String,
        pub version: String,
        pub dependencies: Vec<String>,
    }

    pub fn parse_cargo_lock_packages(content: &str) -> Vec<CargoLockPackage> {
        let mut packages = Vec::new();
        let mut in_package = false;
        let mut in_dependencies = false;

        let mut curr_name = String::new();
        let mut curr_ver = String::new();
        let mut curr_deps = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed == "[[package]]" {
                if in_package && !curr_name.is_empty() {
                    packages.push(CargoLockPackage {
                        name: curr_name.clone(),
                        version: curr_ver.clone(),
                        dependencies: curr_deps.clone(),
                    });
                }
                in_package = true;
                in_dependencies = false;
                curr_name.clear();
                curr_ver.clear();
                curr_deps.clear();
                continue;
            }

            if !in_package {
                continue;
            }

            if in_dependencies {
                if trimmed.starts_with(']') {
                    in_dependencies = false;
                } else if trimmed.starts_with('"') {
                    let dep = trimmed.trim_matches(',').trim_matches('"').trim().to_string();
                    curr_deps.push(dep);
                }
                continue;
            }

            if let Some((k, v)) = trimmed.split_once('=') {
                let k = k.trim();
                let v = v.trim();
                if k == "name" {
                    curr_name = v.trim_matches('"').to_string();
                } else if k == "version" {
                    curr_ver = v.trim_matches('"').to_string();
                } else if k == "dependencies" {
                    if v.starts_with('[') && v.ends_with(']') {
                        let inner = &v[1..v.len() - 1].trim();
                        if !inner.is_empty() {
                            for item in inner.split(',') {
                                let dep = item.trim().trim_matches('"').trim().to_string();
                                if !dep.is_empty() {
                                    curr_deps.push(dep);
                                }
                            }
                        }
                        in_dependencies = false;
                    } else if v.starts_with('[') {
                        in_dependencies = true;
                    }
                }
            }
        }

        if in_package && !curr_name.is_empty() {
            packages.push(CargoLockPackage {
                name: curr_name,
                version: curr_ver,
                dependencies: curr_deps,
            });
        }

        packages
    }
}
