use super::license::LicenseAnalysis;
use serde::{Deserialize, Serialize};

/// パッケージの依存種別
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DependencyType {
    /// 直接依存 (Direct dependency)
    Direct,
    /// 推移的依存 (Transitive indirect dependency)
    Transitive,
}

/// 依存スコープ
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DependencyScope {
    /// 本番環境依存 (Production runtime)
    Production,
    /// 開発・テスト環境依存 (Development/Testing)
    Development,
    /// ピア依存 (Peer dependency)
    Peer,
    /// オプショナル依存 (Optional)
    Optional,
}

/// パッケージの一意な識別情報
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PackageId {
    pub name: String,
    pub version: String,
}

impl PackageId {
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
        }
    }

    pub fn to_string_repr(&self) -> String {
        if self.version.is_empty() {
            self.name.clone()
        } else {
            format!("{}@{}", self.name, self.version)
        }
    }
}

impl std::fmt::Display for PackageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string_repr())
    }
}

/// パッケージ情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub id: PackageId,
    pub license: LicenseAnalysis,
    pub dep_type: DependencyType,
    pub scope: DependencyScope,
    pub repository: Option<String>,
    pub homepage: Option<String>,
    pub description: Option<String>,
    pub manifest_path: Option<String>,
    pub license_text: Option<String>,
}

impl PackageInfo {
    pub fn new(
        id: PackageId,
        raw_license: &str,
        dep_type: DependencyType,
        scope: DependencyScope,
    ) -> Self {
        let license = LicenseAnalysis::parse(raw_license);
        Self {
            id,
            license,
            dep_type,
            scope,
            repository: None,
            homepage: None,
            description: None,
            manifest_path: None,
            license_text: None,
        }
    }
}
