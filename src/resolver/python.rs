use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use crate::model::graph::DependencyGraph;
use crate::model::package::{DependencyScope, DependencyType, PackageId, PackageInfo};

pub struct PythonResolver;

impl PythonResolver {
    /// Python プロジェクトの依存関係とライセンスを解決
    pub fn resolve_project(project_dir: &Path) -> Result<DependencyGraph> {
        let pyproject_path = project_dir.join("pyproject.toml");
        let requirements_path = project_dir.join("requirements.txt");

        if !pyproject_path.exists() && !requirements_path.exists() {
            anyhow::bail!(
                "No Python project configuration found at '{}'. Expected 'pyproject.toml' or 'requirements.txt'.",
                project_dir.display()
            );
        }

        let mut root_name = "my-python-project".to_string();
        let mut root_ver = "0.1.0".to_string();
        let mut root_lic = "UNKNOWN".to_string();
        let mut direct_deps: Vec<(String, String)> = Vec::new();

        // 1. pyproject.toml のパース (存在する場合)
        if pyproject_path.exists() {
            if let Ok(content) = fs::read_to_string(&pyproject_path) {
                let parsed = parse_simple_pyproject(&content);
                if let Some(n) = parsed.name {
                    root_name = n;
                }
                if let Some(v) = parsed.version {
                    root_ver = v;
                }
                if let Some(l) = parsed.license {
                    root_lic = l;
                }
                direct_deps.extend(parsed.dependencies);
            }
        }

        // 2. requirements.txt のパース (存在する場合)
        if requirements_path.exists() {
            if let Ok(content) = fs::read_to_string(&requirements_path) {
                let req_deps = parse_requirements_txt(&content);
                for dep in req_deps {
                    if !direct_deps.iter().any(|(n, _)| n == &dep.0) {
                        direct_deps.push(dep);
                    }
                }
            }
        }

        // 3. uv.lock のパース (存在する場合)
        let uv_lock_path = project_dir.join("uv.lock");
        if uv_lock_path.exists() {
            if let Ok(lock_content) = fs::read_to_string(&uv_lock_path) {
                let lock_deps = parse_uv_lock(&lock_content);
                for dep in lock_deps {
                    if dep.0 != root_name && !direct_deps.iter().any(|(n, _)| n == &dep.0) {
                        direct_deps.push(dep);
                    }
                }
            }
        }

        let root_pkg = PackageInfo::new(
            PackageId::new(&root_name, &root_ver),
            &root_lic,
            DependencyType::Direct,
            DependencyScope::Production,
        );

        let mut graph = DependencyGraph::new(root_pkg);

        // 4. 各依存パッケージのライセンス解決 (ローカル site-packages または PyPI API)
        for (dep_name, dep_ver) in direct_deps {
            let mut lic = find_python_package_license(project_dir, &dep_name);

            // ローカルに見つからない場合、PyPI JSON API からオンライン補完
            if lic.is_none() {
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    let online_lic = tokio::task::block_in_place(|| {
                        handle.block_on(async {
                            Self::fetch_pypi_package_info(&dep_name)
                                .await
                                .ok()
                                .map(|p| p.license.raw)
                        })
                    });
                    if let Some(l) = online_lic {
                        if !l.is_empty() && !l.eq_ignore_ascii_case("UNKNOWN") {
                            lic = Some(l);
                        }
                    }
                }
            }

            let final_lic = lic.unwrap_or_else(|| "UNKNOWN".to_string());
            let pkg_info = PackageInfo::new(
                PackageId::new(&dep_name, &dep_ver),
                &final_lic,
                DependencyType::Direct,
                DependencyScope::Production,
            );
            graph.add_dependency(&graph.root_id.clone(), pkg_info);
        }

        Ok(graph)
    }

    /// 単一パッケージの情報を PyPI JSON API から取得
    pub async fn fetch_pypi_package_info(pkg_name: &str) -> Result<PackageInfo> {
        let url = format!("https://pypi.org/pypi/{}/json", pkg_name);
        let client = reqwest::Client::builder()
            .user_agent(format!("license-trace/{}", env!("CARGO_PKG_VERSION")))
            .build()?;

        let resp = client
            .get(&url)
            .send()
            .await
            .context("Failed to query PyPI")?;
        if !resp.status().is_success() {
            anyhow::bail!("PyPI API returned status {}", resp.status());
        }

        let data: serde_json::Value = resp.json().await?;
        let info = data
            .get("info")
            .context("Missing info field in PyPI response")?;

        let name = info
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(pkg_name);
        let version = info
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("0.0.0");

        let license_expr = info
            .get("license_expression")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();

        let raw_lic = info
            .get("license")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();

        let mut lic_str = if !license_expr.is_empty() {
            license_expr.to_string()
        } else if !raw_lic.is_empty()
            && !raw_lic.eq_ignore_ascii_case("UNKNOWN")
            && !raw_lic.contains('\n')
            && raw_lic.len() <= 40
        {
            raw_lic.to_string()
        } else {
            String::new()
        };

        // classifiers からライセンスを抽出
        if lic_str.is_empty() {
            if let Some(classifiers) = info.get("classifiers").and_then(|v| v.as_array()) {
                for c in classifiers {
                    if let Some(s) = c.as_str() {
                        if s.starts_with("License :: OSI Approved :: ") {
                            let spdx_cand = s
                                .trim_start_matches("License :: OSI Approved :: ")
                                .trim();
                            let mapped = match spdx_cand {
                                "Apache Software License" => "Apache-2.0",
                                "MIT License" => "MIT",
                                "BSD License" => "BSD-3-Clause",
                                "Mozilla Public License 2.0 (MPL 2.0)" => "MPL-2.0",
                                "GNU General Public License v3 (GPLv3)" => "GPL-3.0-only",
                                "GNU General Public License v2 (GPLv2)" => "GPL-2.0-only",
                                "GNU Lesser General Public License v3 (LGPLv3)" => "LGPL-3.0-only",
                                "ISC License (ISCL)" => "ISC",
                                other => other,
                            };
                            lic_str = mapped.to_string();
                            break;
                        }
                    }
                }
            }
        }

        if lic_str.is_empty() {
            lic_str = "UNKNOWN".to_string();
        }

        let mut pkg = PackageInfo::new(
            PackageId::new(name, version),
            &lic_str,
            DependencyType::Direct,
            DependencyScope::Production,
        );

        pkg.description = info
            .get("summary")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        pkg.repository = info
            .get("project_urls")
            .and_then(|v| {
                v.get("Source")
                    .or_else(|| v.get("Homepage"))
                    .or_else(|| v.get("Repository"))
            })
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        Ok(pkg)
    }
}

/// ローカルの .venv または site-packages からライセンス情報を検索
fn find_python_package_license(project_dir: &Path, pkg_name: &str) -> Option<String> {
    let normalized_name = pkg_name.replace('-', "_").to_lowercase();

    // 検索候補ディレクトリ
    let search_dirs = [
        project_dir.join(".venv").join("Lib").join("site-packages"),
        project_dir.join(".venv").join("lib"),
        project_dir.join("venv").join("Lib").join("site-packages"),
        project_dir.join("venv").join("lib"),
    ];

    for base in &search_dirs {
        if !base.exists() {
            continue;
        }

        // site-packages 内のエントリを走査
        if let Ok(entries) = fs::read_dir(base) {
            for entry in entries.flatten() {
                let name_str = entry.file_name().to_string_lossy().to_string();
                let lower_entry = name_str.to_lowercase();

                if (lower_entry.starts_with(&normalized_name)
                    || lower_entry.starts_with(&pkg_name.to_lowercase()))
                    && (lower_entry.ends_with(".dist-info") || lower_entry.ends_with(".egg-info"))
                {
                    let metadata_file = entry.path().join("METADATA");
                    let pkg_info_file = entry.path().join("PKG-INFO");

                    let target_file = if metadata_file.exists() {
                        Some(metadata_file)
                    } else if pkg_info_file.exists() {
                        Some(pkg_info_file)
                    } else {
                        None
                    };

                    if let Some(file_path) = target_file {
                        if let Ok(meta_content) = fs::read_to_string(file_path) {
                            let mut detected_license = None;
                            let mut classifier_license = None;

                            for line in meta_content.lines() {
                                if let Some(lic) = line.strip_prefix("License: ") {
                                    let trimmed = lic.trim();
                                    if !trimmed.is_empty()
                                        && !trimmed.eq_ignore_ascii_case("UNKNOWN")
                                    {
                                        detected_license = Some(trimmed.to_string());
                                    }
                                } else if let Some(clf) =
                                    line.strip_prefix("Classifier: License :: OSI Approved :: ")
                                {
                                    classifier_license = Some(clf.trim().to_string());
                                }
                            }

                            if let Some(l) = detected_license {
                                return Some(l);
                            }
                            if let Some(l) = classifier_license {
                                return Some(l);
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

struct SimplePyproject {
    name: Option<String>,
    version: Option<String>,
    license: Option<String>,
    dependencies: Vec<(String, String)>,
}

fn parse_simple_pyproject(content: &str) -> SimplePyproject {
    let mut name = None;
    let mut version = None;
    let mut license = None;
    let mut dependencies = Vec::new();

    let mut in_dependencies_array = false;
    let mut in_project_section = false;
    let mut in_dep_table = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }

        // Section header
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_dependencies_array = false;
            in_project_section = trimmed == "[project]" || trimmed == "[tool.poetry]";
            in_dep_table = trimmed == "[project.dependencies]" || trimmed == "[tool.poetry.dependencies]";
            continue;
        }

        if in_dependencies_array {
            if trimmed.starts_with(']') {
                in_dependencies_array = false;
            } else if trimmed.starts_with('"') || trimmed.starts_with('\'') {
                let dep_str = trimmed
                    .trim_matches(',')
                    .trim_matches('"')
                    .trim_matches('\'')
                    .trim();
                if !dep_str.is_empty() {
                    let (d_name, d_ver) = parse_python_dep_spec(dep_str);
                    dependencies.push((d_name, d_ver));
                }
            }
            continue;
        }

        if in_dep_table {
            if let Some((k, v)) = trimmed.split_once('=') {
                let dep_name = k.trim().to_string();
                let dep_ver = v.trim().trim_matches('"').trim_matches('\'').to_string();
                if dep_name != "python" {
                    dependencies.push((dep_name, dep_ver));
                }
            }
            continue;
        }

        if in_project_section {
            if let Some((k, v)) = trimmed.split_once('=') {
                let k = k.trim();
                let v = v.trim();
                let v_unquoted = v.trim_matches('"').trim_matches('\'').trim();

                if k == "name" {
                    name = Some(v_unquoted.to_string());
                } else if k == "version" {
                    version = Some(v_unquoted.to_string());
                } else if k == "license" {
                    if v.starts_with('{') {
                        if let Some((_, val)) = v.split_once("text =") {
                            let lic_val = val.trim_matches('}').trim().trim_matches('"').trim_matches('\'').trim();
                            license = Some(lic_val.to_string());
                        }
                    } else {
                        license = Some(v_unquoted.to_string());
                    }
                } else if k == "dependencies" {
                    if v.starts_with('[') {
                        if v.ends_with(']') {
                            let inner = &v[1..v.len() - 1];
                            for item in inner.split(',') {
                                let dep_str = item.trim().trim_matches('"').trim_matches('\'').trim();
                                if !dep_str.is_empty() {
                                    let (d_name, d_ver) = parse_python_dep_spec(dep_str);
                                    dependencies.push((d_name, d_ver));
                                }
                            }
                        } else {
                            in_dependencies_array = true;
                        }
                    }
                }
            }
        }
    }

    SimplePyproject {
        name,
        version,
        license,
        dependencies,
    }
}

fn parse_requirements_txt(content: &str) -> Vec<(String, String)> {
    let mut deps = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('-') {
            continue;
        }
        let (name, ver) = parse_python_dep_spec(trimmed);
        if !name.is_empty() {
            deps.push((name, ver));
        }
    }
    deps
}

fn parse_python_dep_spec(spec: &str) -> (String, String) {
    let separators = ["==", ">=", "<=", "~=", "!=", ">", "<", "@", ";"];
    for sep in separators {
        if let Some((name, ver)) = spec.split_once(sep) {
            let clean_name = name.trim().to_string();
            let clean_ver = ver.split(';').next().unwrap_or(ver).trim().to_string();
            return (clean_name, clean_ver);
        }
    }
    (spec.trim().to_string(), "*".to_string())
}

fn parse_uv_lock(content: &str) -> Vec<(String, String)> {
    let mut packages = Vec::new();
    let mut current_name = None;
    let mut current_version = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[[package]]" {
            if let (Some(n), Some(v)) = (current_name.take(), current_version.take()) {
                packages.push((n, v));
            }
            continue;
        }

        if let Some((k, v)) = trimmed.split_once('=') {
            let k = k.trim();
            let v = v.trim().trim_matches('"').trim_matches('\'').trim();
            if k == "name" {
                current_name = Some(v.to_string());
            } else if k == "version" {
                current_version = Some(v.to_string());
            }
        }
    }

    if let (Some(n), Some(v)) = (current_name, current_version) {
        packages.push((n, v));
    }

    packages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_pyproject_multiline() {
        let content = r#"
[project]
name = "voicerecognizer"
version = "0.1.0"
description = "Speech recognition"
readme = "README.md"
requires-python = ">=3.11"
license = { text = "MIT" }
dependencies = [
    "librosa>=0.11.0",
    "matplotlib>=3.11.0",
    "numpy>=2.4.6",
    "torch>=2.4.0",
]
"#;
        let parsed = parse_simple_pyproject(content);
        assert_eq!(parsed.name.as_deref(), Some("voicerecognizer"));
        assert_eq!(parsed.version.as_deref(), Some("0.1.0"));
        assert_eq!(parsed.license.as_deref(), Some("MIT"));
        assert_eq!(parsed.dependencies.len(), 4);
        assert_eq!(parsed.dependencies[0].0, "librosa");
        assert_eq!(parsed.dependencies[1].0, "matplotlib");
        assert_eq!(parsed.dependencies[2].0, "numpy");
        assert_eq!(parsed.dependencies[3].0, "torch");
    }

    #[test]
    fn test_parse_uv_lock() {
        let content = r#"
version = 1
revision = 1

[[package]]
name = "numpy"
version = "2.4.6"
source = { registry = "https://pypi.org/simple" }

[[package]]
name = "torch"
version = "2.4.0"
source = { registry = "https://pypi.org/simple" }
"#;
        let packages = parse_uv_lock(content);
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].0, "numpy");
        assert_eq!(packages[0].1, "2.4.6");
        assert_eq!(packages[1].0, "torch");
        assert_eq!(packages[1].1, "2.4.0");
    }
}

