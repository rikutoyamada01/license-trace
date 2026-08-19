use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::model::graph::DependencyGraph;
use crate::model::package::{DependencyScope, DependencyType, PackageId, PackageInfo};

pub struct NpmLocalResolver;

impl NpmLocalResolver {
    /// ローカルのプロジェクトパスから依存関係グラフを解決
    pub fn resolve_project(project_dir: &Path) -> Result<DependencyGraph> {
        let pkg_json_path = project_dir.join("package.json");
        if !pkg_json_path.exists() {
            anyhow::bail!("No package.json found at '{}'", project_dir.display());
        }

        let pkg_json_content = fs::read_to_string(&pkg_json_path)
            .context("Failed to read package.json")?;
        let pkg_json: Value = serde_json::from_str(&pkg_json_content)
            .context("Failed to parse package.json")?;

        let root_name = pkg_json.get("name").and_then(|v| v.as_str()).unwrap_or("my-project");
        let root_ver = pkg_json.get("version").and_then(|v| v.as_str()).unwrap_or("1.0.0");
        let root_lic = extract_license_str(&pkg_json);

        let root_pkg = PackageInfo::new(
            PackageId::new(root_name, root_ver),
            &root_lic,
            DependencyType::Direct,
            DependencyScope::Production,
        );

        let mut graph = DependencyGraph::new(root_pkg);

        // 1. package-lock.json が存在する場合はそれを優先解析
        let lock_path = project_dir.join("package-lock.json");
        if lock_path.exists() {
            Self::parse_package_lock(&lock_path, &mut graph, project_dir)?;
            return Ok(graph);
        }

        // 2. なければ package.json の直接依存 + node_modules の探索
        Self::parse_package_json_fallback(&pkg_json, &mut graph, project_dir)?;

        Ok(graph)
    }

    fn parse_package_lock(
        lock_path: &Path,
        graph: &mut DependencyGraph,
        project_dir: &Path,
    ) -> Result<()> {
        let content = fs::read_to_string(lock_path)?;
        let lock: Value = serde_json::from_str(&content)?;

        // lockfileVersion 2 & 3 の "packages" フィールド
        if let Some(packages) = lock.get("packages").and_then(|v| v.as_object()) {
            let mut path_to_id: HashMap<String, PackageId> = HashMap::new();

            // まず全ノードを登録
            for (rel_path, data) in packages {
                if rel_path.is_empty() {
                    continue; // root package
                }

                let pkg_name = if let Some(n) = data.get("name").and_then(|v| v.as_str()) {
                    n.to_string()
                } else {
                    rel_path.split("node_modules/").last().unwrap_or(rel_path).to_string()
                };

                let version = data.get("version").and_then(|v| v.as_str()).unwrap_or("0.0.0");
                let is_dev = data.get("dev").and_then(|v| v.as_bool()).unwrap_or(false);
                let is_peer = data.get("peer").and_then(|v| v.as_bool()).unwrap_or(false);

                let scope = if is_dev {
                    DependencyScope::Development
                } else if is_peer {
                    DependencyScope::Peer
                } else {
                    DependencyScope::Production
                };

                // ライセンス取得（lockfile内のlicense、なければnode_modules内のpackage.jsonから取得）
                let mut license_str = extract_license_str(data);
                if license_str == "UNKNOWN" {
                    let node_pkg_path = project_dir.join(rel_path).join("package.json");
                    if let Ok(content) = fs::read_to_string(&node_pkg_path) {
                        if let Ok(parsed) = serde_json::from_str::<Value>(&content) {
                            license_str = extract_license_str(&parsed);
                        }
                    }
                }

                let is_direct = !rel_path.contains("node_modules/") || rel_path.matches("node_modules/").count() == 1;
                let dep_type = if is_direct {
                    DependencyType::Direct
                } else {
                    DependencyType::Transitive
                };

                let id = PackageId::new(&pkg_name, version);
                let info = PackageInfo::new(id.clone(), &license_str, dep_type, scope);
                graph.get_or_add_node(info);
                path_to_id.insert(rel_path.clone(), id);
            }

            // ルート依存と子依存関係をエッジで結ぶ
            if let Some(root_pkg_data) = packages.get("") {
                if let Some(deps) = root_pkg_data.get("dependencies").and_then(|v| v.as_object()) {
                    for dep_name in deps.keys() {
                        let expected_path = format!("node_modules/{}", dep_name);
                        if let Some(dep_id) = path_to_id.get(&expected_path) {
                            if let Some(dep_info) = graph.all_packages().into_iter().find(|p| &p.id == dep_id).cloned() {
                                graph.add_dependency(&graph.root_id.clone(), dep_info);
                            }
                        }
                    }
                }
            }

            // 各パッケージの子依存関係を解決
            for (rel_path, data) in packages {
                if rel_path.is_empty() {
                    continue;
                }
                if let Some(parent_id) = path_to_id.get(rel_path) {
                    if let Some(deps) = data.get("dependencies").and_then(|v| v.as_object()) {
                        for dep_name in deps.keys() {
                            // ネストされた node_modules/ を優先、なければトップレベル
                            let nested_path = format!("{}/node_modules/{}", rel_path, dep_name);
                            let top_path = format!("node_modules/{}", dep_name);

                            let target_id = path_to_id.get(&nested_path).or_else(|| path_to_id.get(&top_path));
                            if let Some(t_id) = target_id {
                                if let Some(dep_info) = graph.all_packages().into_iter().find(|p| &p.id == t_id).cloned() {
                                    graph.add_dependency(&parent_id.clone(), dep_info);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn parse_package_json_fallback(
        pkg_json: &Value,
        graph: &mut DependencyGraph,
        project_dir: &Path,
    ) -> Result<()> {
        if let Some(deps) = pkg_json.get("dependencies").and_then(|v| v.as_object()) {
            for (name, ver_val) in deps {
                let ver_req = ver_val.as_str().unwrap_or("*");
                let (actual_ver, lic) = find_local_package_details(project_dir, name).unwrap_or((ver_req.to_string(), "UNKNOWN".to_string()));
                let pkg = PackageInfo::new(
                    PackageId::new(name, actual_ver),
                    &lic,
                    DependencyType::Direct,
                    DependencyScope::Production,
                );
                graph.add_dependency(&graph.root_id.clone(), pkg);
            }
        }

        if let Some(dev_deps) = pkg_json.get("devDependencies").and_then(|v| v.as_object()) {
            for (name, ver_val) in dev_deps {
                let ver_req = ver_val.as_str().unwrap_or("*");
                let (actual_ver, lic) = find_local_package_details(project_dir, name).unwrap_or((ver_req.to_string(), "UNKNOWN".to_string()));
                let pkg = PackageInfo::new(
                    PackageId::new(name, actual_ver),
                    &lic,
                    DependencyType::Direct,
                    DependencyScope::Development,
                );
                graph.add_dependency(&graph.root_id.clone(), pkg);
            }
        }

        Ok(())
    }
}

fn find_local_package_details(project_dir: &Path, name: &str) -> Option<(String, String)> {
    let node_pkg = project_dir.join("node_modules").join(name).join("package.json");
    if let Ok(content) = fs::read_to_string(&node_pkg) {
        if let Ok(data) = serde_json::from_str::<Value>(&content) {
            let ver = data.get("version").and_then(|v| v.as_str()).unwrap_or("0.0.0").to_string();
            let lic = extract_license_str(&data);
            return Some((ver, lic));
        }
    }
    None
}

fn extract_license_str(data: &Value) -> String {
    if let Some(lic) = data.get("license") {
        if let Some(s) = lic.as_str() {
            return s.to_string();
        } else if let Some(obj) = lic.as_object() {
            if let Some(t) = obj.get("type").and_then(|v| v.as_str()) {
                return t.to_string();
            }
        }
    }

    if let Some(licenses) = data.get("licenses").and_then(|v| v.as_array()) {
        let names: Vec<String> = licenses
            .iter()
            .filter_map(|l| {
                if let Some(s) = l.as_str() {
                    Some(s.to_string())
                } else {
                    l.get("type").and_then(|v| v.as_str()).map(|s| s.to_string())
                }
            })
            .collect();
        if !names.is_empty() {
            return names.join(" OR ");
        }
    }

    "UNKNOWN".to_string()
}
