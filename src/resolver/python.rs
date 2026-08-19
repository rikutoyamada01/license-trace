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
                if let Some(n) = parsed.name { root_name = n; }
                if let Some(v) = parsed.version { root_ver = v; }
                if let Some(l) = parsed.license { root_lic = l; }
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

        let root_pkg = PackageInfo::new(
            PackageId::new(&root_name, &root_ver),
            &root_lic,
            DependencyType::Direct,
            DependencyScope::Production,
        );

        let mut graph = DependencyGraph::new(root_pkg);

        // 3. 各依存パッケージのライセンス解決 (ローカル site-packages または PyPI API)
        for (dep_name, dep_ver) in direct_deps {
            let lic = find_python_package_license(project_dir, &dep_name).unwrap_or_else(|| "UNKNOWN".to_string());
            let pkg_info = PackageInfo::new(
                PackageId::new(&dep_name, &dep_ver),
                &lic,
                DependencyType::Direct,
                DependencyScope::Production,
            );
            graph.add_dependency(&graph.root_id.clone(), pkg_info);
        }

        Ok(graph)
    }

    /// 単一パッケージの情報を PyPI JSON API から取得
    #[allow(dead_code)]
    pub async fn fetch_pypi_package_info(pkg_name: &str) -> Result<PackageInfo> {
        let url = format!("https://pypi.org/pypi/{}/json", pkg_name);
        let client = reqwest::Client::builder()
            .user_agent(format!("license-trace/{}", env!("CARGO_PKG_VERSION")))
            .build()?;

        let resp = client.get(&url).send().await.context("Failed to query PyPI")?;
        if !resp.status().is_success() {
            anyhow::bail!("PyPI API returned status {}", resp.status());
        }

        let data: serde_json::Value = resp.json().await?;
        let info = data.get("info").context("Missing info field in PyPI response")?;

        let name = info.get("name").and_then(|v| v.as_str()).unwrap_or(pkg_name);
        let version = info.get("version").and_then(|v| v.as_str()).unwrap_or("0.0.0");
        
        let mut lic_str = info.get("license").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        
        // classifiers からライセンスを抽出 (license フィールドが空または UNKNOWN の場合)
        if lic_str.is_empty() || lic_str.eq_ignore_ascii_case("UNKNOWN") {
            if let Some(classifiers) = info.get("classifiers").and_then(|v| v.as_array()) {
                for c in classifiers {
                    if let Some(s) = c.as_str() {
                        if s.starts_with("License :: OSI Approved :: ") {
                            lic_str = s.trim_start_matches("License :: OSI Approved :: ").to_string();
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

        pkg.description = info.get("summary").and_then(|v| v.as_str()).map(|s| s.to_string());
        pkg.repository = info.get("project_urls")
            .and_then(|v| v.get("Source").or_else(|| v.get("Homepage")).or_else(|| v.get("Repository")))
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
                
                if (lower_entry.starts_with(&normalized_name) || lower_entry.starts_with(&pkg_name.to_lowercase()))
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
                                    if !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("UNKNOWN") {
                                        detected_license = Some(trimmed.to_string());
                                    }
                                } else if let Some(clf) = line.strip_prefix("Classifier: License :: OSI Approved :: ") {
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

    let mut in_dependencies = false;
    let mut in_project = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }

        if trimmed == "[project]" || trimmed == "[tool.poetry]" {
            in_project = true;
            in_dependencies = false;
            continue;
        } else if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if trimmed == "[project.dependencies]" || trimmed == "[tool.poetry.dependencies]" {
                in_dependencies = true;
            } else {
                in_dependencies = false;
            }
            in_project = false;
            continue;
        }

        if in_project {
            if let Some((k, v)) = trimmed.split_once('=') {
                let k = k.trim();
                let v = v.trim().trim_matches('"').trim_matches('\'').trim();
                if k == "name" {
                    name = Some(v.to_string());
                } else if k == "version" {
                    version = Some(v.to_string());
                } else if k == "license" {
                    license = Some(v.to_string());
                } else if k == "dependencies" && v.starts_with('[') {
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
                        in_dependencies = true;
                    }
                }
            }
        } else if in_dependencies {
            if trimmed.starts_with(']') {
                in_dependencies = false;
            } else if trimmed.starts_with('"') || trimmed.starts_with('\'') {
                let dep_str = trimmed.trim_matches(',').trim_matches('"').trim_matches('\'').trim();
                if !dep_str.is_empty() {
                    let (d_name, d_ver) = parse_python_dep_spec(dep_str);
                    dependencies.push((d_name, d_ver));
                }
            } else if let Some((k, v)) = trimmed.split_once('=') {
                let dep_name = k.trim().to_string();
                let dep_ver = v.trim().trim_matches('"').trim_matches('\'').to_string();
                if dep_name != "python" {
                    dependencies.push((dep_name, dep_ver));
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
