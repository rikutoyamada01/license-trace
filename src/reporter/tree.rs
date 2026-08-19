use owo_colors::OwoColorize;
use std::collections::HashSet;

use crate::model::{DependencyGraph, LicenseCategory, PackageId};

pub struct TreeReporter;

impl TreeReporter {
    pub fn render(graph: &DependencyGraph) {
        if let Some(root) = graph.root_package() {
            println!(
                "{} ({})",
                root.id.to_string_repr().bold().cyan(),
                root.license.raw.green()
            );
            let mut visited = HashSet::new();
            visited.insert(root.id.clone());
            Self::render_children(graph, &root.id, "", &mut visited);
        }
    }

    fn render_children(
        graph: &DependencyGraph,
        parent_id: &PackageId,
        prefix: &str,
        visited: &mut HashSet<PackageId>,
    ) {
        let children = graph.direct_dependencies_of(parent_id);
        let count = children.len();

        for (i, child) in children.iter().enumerate() {
            let is_last = i + 1 == count;
            let branch = if is_last { "└── " } else { "├── " };
            let child_prefix = if is_last { "    " } else { "│   " };

            let lic_badge = match child.license.category {
                LicenseCategory::Permissive => child.license.raw.green().to_string(),
                LicenseCategory::WeakCopyleft => child.license.raw.yellow().to_string(),
                LicenseCategory::StrongCopyleft | LicenseCategory::NetworkCopyleft | LicenseCategory::NonCommercial => {
                    child.license.raw.red().bold().to_string()
                }
                _ => child.license.raw.cyan().to_string(),
            };

            let already_visited = visited.contains(&child.id);
            if already_visited {
                println!(
                    "{}{}{} ({}) {}",
                    prefix,
                    branch,
                    child.id.to_string_repr().white(),
                    lic_badge,
                    "(deduped)".dimmed()
                );
            } else {
                println!(
                    "{}{}{} ({})",
                    prefix,
                    branch,
                    child.id.to_string_repr().white(),
                    lic_badge
                );
                visited.insert(child.id.clone());
                let new_prefix = format!("{}{}", prefix, child_prefix);
                Self::render_children(graph, &child.id, &new_prefix, visited);
            }
        }
    }
}
