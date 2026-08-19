pub mod cargo;
pub mod gomod;
pub mod npm_local;
pub mod npm_online;
pub mod python;

pub use cargo::CargoResolver;
pub use gomod::GoModResolver;
pub use npm_local::NpmLocalResolver;
pub use npm_online::NpmOnlineResolver;
pub use python::PythonResolver;

use crate::model::DependencyGraph;
use anyhow::Result;
use std::path::Path;

/// プロジェクトディレクトリのファイル構成からエコシステムを自動判別して解決
pub fn resolve_auto(project_dir: &Path) -> Result<DependencyGraph> {
    if project_dir.join("package.json").exists() {
        NpmLocalResolver::resolve_project(project_dir)
    } else if project_dir.join("Cargo.toml").exists() {
        CargoResolver::resolve_project(project_dir)
    } else if project_dir.join("pyproject.toml").exists()
        || project_dir.join("requirements.txt").exists()
    {
        PythonResolver::resolve_project(project_dir)
    } else if project_dir.join("go.mod").exists() {
        GoModResolver::resolve_project(project_dir)
    } else {
        anyhow::bail!(
            "Could not detect project type at '{}'. Expected 'package.json', 'Cargo.toml', 'pyproject.toml'/'requirements.txt', or 'go.mod'.",
            project_dir.display()
        );
    }
}
