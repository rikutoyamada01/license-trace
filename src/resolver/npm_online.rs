use anyhow::{Context, Result};
use reqwest::Client;
use semver::Version;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::model::graph::DependencyGraph;
use crate::model::package::{DependencyScope, DependencyType, PackageId, PackageInfo};

pub struct NpmOnlineResolver {
    client: Client,
    cache: Arc<Mutex<HashMap<String, Value>>>,
    visited: Arc<Mutex<HashSet<PackageId>>>,
    max_depth: usize,
}

impl NpmOnlineResolver {
    pub fn new(max_depth: usize) -> Self {
        let user_agent = format!("license-trace/{}", env!("CARGO_PKG_VERSION"));
        let client = Client::builder()
            .user_agent(user_agent)
            .build()
            .unwrap_or_default();

        Self {
            client,
            cache: Arc::new(Mutex::new(HashMap::new())),
            visited: Arc::new(Mutex::new(HashSet::new())),
            max_depth,
        }
    }

    /// 単一パッケージの依存ツリーをオンラインから再帰解決
    pub async fn resolve_package(
        &self,
        pkg_name: &str,
        requested_version: Option<&str>,
    ) -> Result<DependencyGraph> {
        let root_version = if let Some(v) = requested_version {
            clean_semver_requirement(v)
        } else {
            self.fetch_latest_version(pkg_name).await?
        };

        let root_info = self.fetch_package_info(pkg_name, &root_version).await?;
        let mut graph = DependencyGraph::new(root_info.clone());

        {
            let mut visited = self.visited.lock().await;
            visited.insert(root_info.id.clone());
        }

        // 再帰探索
        self.resolve_recursive(&root_info.id, 1, &mut graph).await?;

        Ok(graph)
    }

    async fn fetch_latest_version(&self, pkg_name: &str) -> Result<String> {
        let url = format!("https://registry.npmjs.org/{}", urlencoding(pkg_name));
        let data = self.fetch_json(&url).await?;

        if let Some(dist_tags) = data.get("dist-tags") {
            if let Some(latest) = dist_tags.get("latest").and_then(|v| v.as_str()) {
                return Ok(latest.to_string());
            }
        }

        // dist-tags がない場合は versions から最大の semver を探索
        if let Some(versions) = data.get("versions").and_then(|v| v.as_object()) {
            let mut parsed_versions: Vec<(Option<Version>, &str)> = versions
                .keys()
                .map(|k| (Version::parse(k.trim_start_matches('v')).ok(), k.as_str()))
                .collect();

            // semverパース成功分を優先してソート
            parsed_versions.sort_by(|a, b| match (&a.0, &b.0) {
                (Some(v1), Some(v2)) => v1.cmp(v2),
                (Some(_), None) => std::cmp::Ordering::Greater,
                (None, Some(_)) => std::cmp::Ordering::Less,
                (None, None) => a.1.cmp(b.1),
            });

            if let Some((_, best_ver)) = parsed_versions.last() {
                return Ok(best_ver.to_string());
            }
        }

        anyhow::bail!(
            "Could not resolve latest version for package '{}'",
            pkg_name
        );
    }

    async fn fetch_package_info(&self, pkg_name: &str, version: &str) -> Result<PackageInfo> {
        let clean_version = clean_semver_requirement(version);
        let url = format!(
            "https://registry.npmjs.org/{}/{}",
            urlencoding(pkg_name),
            clean_version
        );

        let data = match self.fetch_json(&url).await {
            Ok(d) => d,
            Err(_) => {
                // バージョン指定エンドポイントが失敗した場合、パッケージ全体ドキュメントから検索
                let pkg_url = format!("https://registry.npmjs.org/{}", urlencoding(pkg_name));
                let full_data = self.fetch_json(&pkg_url).await?;
                if let Some(versions) = full_data.get("versions").and_then(|v| v.as_object()) {
                    if let Some(v_data) = versions.get(&clean_version) {
                        v_data.clone()
                    } else if let Some((_, v_data)) = versions.iter().next_back() {
                        v_data.clone()
                    } else {
                        anyhow::bail!("No matching version found for {}@{}", pkg_name, version);
                    }
                } else {
                    anyhow::bail!("Invalid registry response for {}", pkg_name);
                }
            }
        };

        let resolved_version = data
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or(&clean_version);
        let license_str = extract_license_field(&data);

        let mut pkg = PackageInfo::new(
            PackageId::new(pkg_name, resolved_version),
            &license_str,
            DependencyType::Direct,
            DependencyScope::Production,
        );

        pkg.description = data
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if let Some(repo) = data.get("repository") {
            if let Some(url) = repo.as_str() {
                pkg.repository = Some(url.to_string());
            } else if let Some(url) = repo.get("url").and_then(|u| u.as_str()) {
                pkg.repository = Some(url.to_string());
            }
        }

        Ok(pkg)
    }

    async fn fetch_json(&self, url: &str) -> Result<Value> {
        {
            let cache = self.cache.lock().await;
            if let Some(cached) = cache.get(url) {
                return Ok(cached.clone());
            }
        }

        let resp = self
            .client
            .get(url)
            .send()
            .await
            .context(format!("Failed HTTP GET {}", url))?;
        if !resp.status().is_success() {
            anyhow::bail!("HTTP status {} for URL: {}", resp.status(), url);
        }

        let data: Value = resp
            .json()
            .await
            .context(format!("Failed parsing JSON from {}", url))?;

        {
            let mut cache = self.cache.lock().await;
            cache.insert(url.to_string(), data.clone());
        }

        Ok(data)
    }

    #[async_recursion::async_recursion]
    async fn resolve_recursive(
        &self,
        parent_id: &PackageId,
        current_depth: usize,
        graph: &mut DependencyGraph,
    ) -> Result<()> {
        if current_depth > self.max_depth {
            return Ok(());
        }

        let clean_version = clean_semver_requirement(&parent_id.version);
        let url = format!(
            "https://registry.npmjs.org/{}/{}",
            urlencoding(&parent_id.name),
            clean_version
        );
        let data = match self.fetch_json(&url).await {
            Ok(d) => d,
            Err(_) => return Ok(()),
        };

        let deps = match data.get("dependencies").and_then(|v| v.as_object()) {
            Some(d) => d,
            None => return Ok(()),
        };

        for (dep_name, dep_ver_val) in deps {
            let req_version = dep_ver_val.as_str().unwrap_or("*");
            let clean_dep_ver = clean_semver_requirement(req_version);

            // パッケージ情報のフェッチ
            let dep_pkg = match self.fetch_package_info(dep_name, &clean_dep_ver).await {
                Ok(mut p) => {
                    p.dep_type = DependencyType::Transitive;
                    p
                }
                Err(_) => {
                    // フェッチに失敗した場合は仮のUnknownとして追加
                    PackageInfo::new(
                        PackageId::new(dep_name, clean_dep_ver),
                        "UNKNOWN",
                        DependencyType::Transitive,
                        DependencyScope::Production,
                    )
                }
            };

            let dep_id = dep_pkg.id.clone();
            let is_new = {
                let mut visited = self.visited.lock().await;
                visited.insert(dep_id.clone())
            };

            graph.add_dependency(parent_id, dep_pkg);

            if is_new {
                self.resolve_recursive(&dep_id, current_depth + 1, graph)
                    .await?;
            }
        }

        Ok(())
    }
}

fn urlencoding(s: &str) -> String {
    s.replace('/', "%2F")
}

fn clean_semver_requirement(v: &str) -> String {
    let s = v.trim();
    // 範囲指定（スペースや || を含む）は latest に解決
    if s.contains(' ')
        || s.contains("||")
        || s.contains('>')
        || s.contains('<')
        || s.is_empty()
        || s == "*"
    {
        return "latest".to_string();
    }

    let trimmed = s.trim_start_matches(['^', '~', '=', 'v', ' ']);
    if trimmed.is_empty() {
        "latest".to_string()
    } else {
        trimmed.to_string()
    }
}

fn extract_license_field(data: &Value) -> String {
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
                    l.get("type")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                }
            })
            .collect();
        if !names.is_empty() {
            return names.join(" OR ");
        }
    }

    "UNKNOWN".to_string()
}
