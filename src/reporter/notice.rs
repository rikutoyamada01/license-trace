use crate::model::DependencyGraph;

pub struct NoticeReporter;

impl NoticeReporter {
    pub fn generate_markdown(graph: &DependencyGraph, prod_only: bool) -> String {
        let mut out = String::new();

        out.push_str("# Third-Party Software Notices and Licenses\n\n");
        out.push_str("This document lists the open-source dependencies used by this project, along with their declared licenses, repository links, and full license/copyright texts.\n\n");

        // 1. 全パッケージ一覧テーブル
        out.push_str("## Package Summary\n\n");
        out.push_str("| Package | Version | License | Scope | Repository |\n");
        out.push_str("| :--- | :--- | :--- | :--- | :--- |\n");

        let mut packages = graph.all_packages();
        packages.sort_by(|a, b| a.id.name.to_lowercase().cmp(&b.id.name.to_lowercase()));

        for pkg in &packages {
            if pkg.id == graph.root_id {
                continue;
            }
            if prod_only && pkg.scope == crate::model::DependencyScope::Development {
                continue;
            }

            let scope_str = match pkg.scope {
                crate::model::DependencyScope::Production => "Production",
                crate::model::DependencyScope::Development => "Development",
                crate::model::DependencyScope::Peer => "Peer",
                crate::model::DependencyScope::Optional => "Optional",
            };

            let repo_str = pkg.repository.as_deref().unwrap_or("-");

            out.push_str(&format!(
                "| [**{}**](#{}) | `{}` | `{}` | {} | {} |\n",
                pkg.id.name,
                pkg.id.to_string_repr().replace('@', "-").replace('.', "-"),
                pkg.id.version,
                pkg.license.raw,
                scope_str,
                repo_str
            ));
        }

        out.push_str("\n---\n\n");
        out.push_str("## Package License Notices & Copyright Texts\n\n");

        for pkg in &packages {
            if pkg.id == graph.root_id {
                continue;
            }
            if prod_only && pkg.scope == crate::model::DependencyScope::Development {
                continue;
            }

            let anchor = pkg.id.to_string_repr().replace('@', "-").replace('.', "-");
            out.push_str(&format!("<a id=\"{}\"></a>\n", anchor));
            out.push_str(&format!("### {} `{}`\n\n", pkg.id.name, pkg.id.version));
            out.push_str(&format!("- **Declared License:** `{}`\n", pkg.license.raw));
            if let Some(repo) = &pkg.repository {
                out.push_str(&format!("- **Repository:** {}\n", repo));
            }
            if let Some(desc) = &pkg.description {
                out.push_str(&format!("- **Description:** {}\n", desc));
            }
            out.push_str("\n");

            if let Some(text) = &pkg.license_text {
                out.push_str("```text\n");
                out.push_str(text);
                out.push_str("\n```\n\n");
            } else {
                out.push_str(&format!("*Full license text not bundled in crate distribution. Released under `{}`.*\n\n", pkg.license.raw));
            }
        }

        out
    }
}
