use anyhow::{Context, Result};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::model::graph::DependencyGraph;
use crate::model::package::{DependencyScope, DependencyType, PackageId, PackageInfo};

pub struct GoModResolver;

impl GoModResolver {
    /// Go modules プロジェクトの依存関係とライセンスを解決
    pub fn resolve_project(project_dir: &Path) -> Result<DependencyGraph> {
        let go_mod_path = project_dir.join("go.mod");
        if !go_mod_path.exists() {
            anyhow::bail!("No go.mod found at '{}'", project_dir.display());
        }

        // 1. go list -m -json all コマンドでの解決を試行
        if let Ok(graph) = Self::resolve_via_go_list(project_dir) {
            return Ok(graph);
        }

        // 2. フォールバック: go.mod の静的パース
        Self::resolve_via_files(project_dir)
    }

    /// `go list -m -json all` によるモジュールパスとローカルディレクトリ解析
    fn resolve_via_go_list(project_dir: &Path) -> Result<DependencyGraph> {
        let output = Command::new("go")
            .args(["list", "-m", "-json", "all"])
            .current_dir(project_dir)
            .output()
            .context("Failed to execute go list")?;

        if !output.status.success() {
            anyhow::bail!(
                "go list exited with non-zero status: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut modules: Vec<Value> = Vec::new();

        // go list -m -json は連続した JSON オブジェクトを出力する
        let deserializer = serde_json::Deserializer::from_str(&stdout);
        for val in deserializer.into_iter::<Value>().flatten() {
            modules.push(val);
        }

        if modules.is_empty() {
            anyhow::bail!("No modules found from go list");
        }

        let main_mod = &modules[0];
        let main_path = main_mod
            .get("Path")
            .and_then(|v| v.as_str())
            .unwrap_or("main");
        let main_ver = main_mod
            .get("Version")
            .and_then(|v| v.as_str())
            .unwrap_or("0.0.0");
        let main_dir = main_mod.get("Dir").and_then(|v| v.as_str());

        let main_lic = if let Some(d) = main_dir {
            detect_license_in_directory(Path::new(d)).unwrap_or_else(|| "UNKNOWN".to_string())
        } else {
            detect_license_in_directory(project_dir).unwrap_or_else(|| "UNKNOWN".to_string())
        };

        let root_pkg = PackageInfo::new(
            PackageId::new(main_path, main_ver),
            &main_lic,
            DependencyType::Direct,
            DependencyScope::Production,
        );

        let mut graph = DependencyGraph::new(root_pkg);

        for m in &modules[1..] {
            let path = m.get("Path").and_then(|v| v.as_str()).unwrap_or("unknown");
            let ver = m.get("Version").and_then(|v| v.as_str()).unwrap_or("0.0.0");
            let is_indirect = m.get("Indirect").and_then(|v| v.as_bool()).unwrap_or(false);
            let dir = m.get("Dir").and_then(|v| v.as_str());

            let lic = if let Some(d) = dir {
                detect_license_in_directory(Path::new(d)).unwrap_or_else(|| "UNKNOWN".to_string())
            } else {
                "UNKNOWN".to_string()
            };

            let dep_type = if is_indirect {
                DependencyType::Transitive
            } else {
                DependencyType::Direct
            };
            let pkg = PackageInfo::new(
                PackageId::new(path, ver),
                &lic,
                dep_type,
                DependencyScope::Production,
            );

            graph.add_dependency(&graph.root_id.clone(), pkg);
        }

        Ok(graph)
    }

    /// go.mod 手動パースフォールバック
    fn resolve_via_files(project_dir: &Path) -> Result<DependencyGraph> {
        let go_mod_path = project_dir.join("go.mod");
        let content = fs::read_to_string(&go_mod_path)?;

        let mut root_name = "main".to_string();
        let mut deps: Vec<(String, String, bool)> = Vec::new(); // (name, version, indirect)

        let mut in_require = false;

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.is_empty() {
                continue;
            }

            if let Some(mod_name) = trimmed.strip_prefix("module ") {
                root_name = mod_name.trim().to_string();
                continue;
            }

            if trimmed == "require (" {
                in_require = true;
                continue;
            }

            if in_require {
                if trimmed == ")" {
                    in_require = false;
                    continue;
                }
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 2 {
                    let name = parts[0].to_string();
                    let ver = parts[1].to_string();
                    let is_indirect = trimmed.contains("// indirect");
                    deps.push((name, ver, is_indirect));
                }
            } else if let Some(req_line) = trimmed.strip_prefix("require ") {
                let parts: Vec<&str> = req_line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let name = parts[0].to_string();
                    let ver = parts[1].to_string();
                    let is_indirect = trimmed.contains("// indirect");
                    deps.push((name, ver, is_indirect));
                }
            }
        }

        let root_lic =
            detect_license_in_directory(project_dir).unwrap_or_else(|| "UNKNOWN".to_string());
        let root_pkg = PackageInfo::new(
            PackageId::new(&root_name, "0.0.0"),
            &root_lic,
            DependencyType::Direct,
            DependencyScope::Production,
        );

        let mut graph = DependencyGraph::new(root_pkg);

        for (name, ver, is_indirect) in deps {
            let dep_type = if is_indirect {
                DependencyType::Transitive
            } else {
                DependencyType::Direct
            };
            let pkg = PackageInfo::new(
                PackageId::new(&name, &ver),
                "UNKNOWN",
                dep_type,
                DependencyScope::Production,
            );
            graph.add_dependency(&graph.root_id.clone(), pkg);
        }

        Ok(graph)
    }
}

/// ディレクトリ内の LICENSE ファイルからライセンスを特定
fn detect_license_in_directory(dir: &Path) -> Option<String> {
    let license_filenames = [
        "LICENSE",
        "LICENSE.txt",
        "LICENSE.md",
        "LICENSE.rst",
        "LICENCE",
        "LICENCE.txt",
        "LICENCE.md",
        "COPYING",
    ];

    for name in &license_filenames {
        let p = dir.join(name);
        if p.exists() {
            if let Ok(content) = fs::read_to_string(&p) {
                let upper = content.to_uppercase();
                if upper.contains("MIT LICENSE")
                    || upper.contains("PERMISSION IS HEREBY GRANTED, FREE OF CHARGE")
                {
                    return Some("MIT".to_string());
                } else if upper.contains("APACHE LICENSE") && upper.contains("VERSION 2.0") {
                    return Some("Apache-2.0".to_string());
                } else if upper.contains("BSD 3-CLAUSE")
                    || upper.contains("REDISTRIBUTION AND USE IN SOURCE AND BINARY")
                {
                    return Some("BSD-3-Clause".to_string());
                } else if upper.contains("BSD 2-CLAUSE") {
                    return Some("BSD-2-Clause".to_string());
                } else if upper.contains("ISC LICENSE") {
                    return Some("ISC".to_string());
                } else if upper.contains("MOZILLA PUBLIC LICENSE") {
                    return Some("MPL-2.0".to_string());
                } else if upper.contains("GNU GENERAL PUBLIC LICENSE") {
                    if upper.contains("VERSION 3") {
                        return Some("GPL-3.0-only".to_string());
                    } else if upper.contains("VERSION 2") {
                        return Some("GPL-2.0-only".to_string());
                    }
                    return Some("GPL-3.0-only".to_string());
                } else if upper.contains("GNU LESSER GENERAL PUBLIC LICENSE") {
                    return Some("LGPL-3.0-only".to_string());
                } else if upper.contains("GNU AFFERO GENERAL PUBLIC LICENSE") {
                    return Some("AGPL-3.0-only".to_string());
                }
            }
        }
    }

    None
}
