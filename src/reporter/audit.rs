use owo_colors::OwoColorize;
use std::collections::HashSet;

use crate::model::{DependencyGraph, LicenseCategory, PackageId, SourceDisclosureLevel};
use crate::policy::{CompatibilityReport, CompatibilityStatus};

pub struct AuditReporter;

impl AuditReporter {
    pub fn render_terminal(report: &CompatibilityReport, graph: &DependencyGraph) {
        println!();
        println!("=== License Trace & Compliance Audit ===");
        println!(
            "Target Outbound License : {}",
            report.outbound_license.bold().cyan()
        );

        let status_str = match report.status {
            CompatibilityStatus::Compatible => "[COMPATIBLE]".green().bold().to_string(),
            CompatibilityStatus::Warning => "[WARNING]".yellow().bold().to_string(),
            CompatibilityStatus::Incompatible => "[INCOMPATIBLE]".red().bold().to_string(),
            CompatibilityStatus::NeedsReview => "[NEEDS REVIEW]".cyan().bold().to_string(),
        };

        println!("Audit Status            : {}", status_str);
        println!("Summary                 : {}", report.summary);
        println!();

        // 1. 依存ツリー表示 (Tree structure with license for each node)
        println!("--- Dependency Tree ---");
        if let Some(root) = graph.root_package() {
            println!(
                "{} [{}]",
                root.id.to_string_repr().bold().cyan(),
                root.license.raw.green()
            );
            let mut visited = HashSet::new();
            visited.insert(root.id.clone());
            Self::render_children(graph, &root.id, "", &mut visited);
        }
        println!();

        // 2. ライセンス別件数の集計 (License counts breakdown)
        println!("--- License Breakdown ---");
        for (lic, count) in &report.obligations.license_counts {
            println!("  - {:<28} : {} package(s)", lic.bold(), count);
        }
        println!();

        // 3. 義務・制約の集約
        println!("--- Obligations (Lower Bound) ---");
        let comm_str = if report.obligations.commercial_use_allowed {
            "Allowed".green().to_string()
        } else {
            "PROHIBITED".red().to_string()
        };
        let dist_str = if report.obligations.distribution_allowed {
            "Allowed".green().to_string()
        } else {
            "Restricted".red().to_string()
        };
        let notice_str = if report.obligations.notice_required {
            format!(
                "Required ({} packages require attribution)",
                report.obligations.notice_package_count
            )
            .yellow()
            .to_string()
        } else {
            "Not required".green().to_string()
        };
        let disc_str = match report.obligations.worst_source_disclosure {
            SourceDisclosureLevel::None => "None (Permissive)".green().to_string(),
            SourceDisclosureLevel::LibraryLevel => {
                "Library Level (Weak Copyleft)".yellow().to_string()
            }
            SourceDisclosureLevel::ProjectLevel => {
                "Project Level (Strong Copyleft)".red().to_string()
            }
            SourceDisclosureLevel::NetworkLevel => {
                "Network Level (AGPL/Network Copyleft)".red().to_string()
            }
        };

        println!("  Commercial Use       : {}", comm_str);
        println!("  Distribution         : {}", dist_str);
        println!("  Copyright / Notice   : {}", notice_str);
        if report.obligations.notice_required {
            // 代表的なライセンス内訳を表示
            let sample_lics = report
                .obligations
                .notice_requiring_licenses
                .iter()
                .take(6)
                .map(|(lic, count)| format!("{} ({})", lic, count))
                .collect::<Vec<_>>()
                .join(", ");
            let more_str = if report.obligations.notice_requiring_licenses.len() > 6 {
                ", ..."
            } else {
                ""
            };
            println!(
                "    -> Required by     : {}{}",
                sample_lics.dimmed(),
                more_str.dimmed()
            );
            println!("    -> Action          : {}", "Include original copyright notices & license texts in your distributed binary/release (e.g. THIRD_PARTY_LICENSES)".cyan());
        }
        println!("  Source Disclosure    : {}", disc_str);
        println!();

        // 4. 問題のあるパッケージと到達パス
        if !report.obligations.problematic_packages.is_empty() {
            println!("--- Problematic Copyleft / Non-commercial Dependencies ---");
            for pkg in &report.obligations.problematic_packages {
                println!(
                    "  * {} [License: {} ({})]",
                    pkg.id.to_string_repr().bold().red(),
                    pkg.license.raw.red(),
                    pkg.license.category.label()
                );
                let paths = graph.find_all_paths_to(&pkg.id.name);
                for (i, p) in paths.iter().enumerate() {
                    let path_str = p
                        .iter()
                        .map(|id| id.to_string_repr())
                        .collect::<Vec<_>>()
                        .join(" -> ");
                    println!("    Path {:02}: {}", i + 1, path_str);
                }
            }
            println!();
        }

        // 5. 弱コピーレフト (LGPL, MPL, CC-BY-SA等) パッケージ
        if !report.obligations.weak_copyleft_packages.is_empty() {
            println!("--- Weak Copyleft Dependencies (Check Linking / Modification) ---");
            for pkg in &report.obligations.weak_copyleft_packages {
                println!(
                    "  * {} [License: {} ({})]",
                    pkg.id.to_string_repr().bold().yellow(),
                    pkg.license.raw.yellow(),
                    pkg.license.category.label()
                );
                if let Some(shortest) = graph.find_shortest_path_to(&pkg.id.name) {
                    let path_str = shortest
                        .iter()
                        .map(|id| id.to_string_repr())
                        .collect::<Vec<_>>()
                        .join(" -> ");
                    println!("    Path: {}", path_str.dimmed());
                }
            }
            println!();
        }

        // 6. 要調査 (UNKNOWN) パッケージ
        if !report.obligations.review_needed_packages.is_empty() {
            println!("--- Unknown / Non-standard Licenses (Review Required) ---");
            for pkg in &report.obligations.review_needed_packages {
                println!(
                    "  * {} [License: {}]",
                    pkg.id.to_string_repr().bold().cyan(),
                    pkg.license.raw.yellow()
                );
                if let Some(shortest) = graph.find_shortest_path_to(&pkg.id.name) {
                    let path_str = shortest
                        .iter()
                        .map(|id| id.to_string_repr())
                        .collect::<Vec<_>>()
                        .join(" -> ");
                    println!("    Path: {}", path_str.dimmed());
                }
            }
            println!();
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
            let branch = if is_last { "`-- " } else { "|-- " };
            let child_prefix = if is_last { "    " } else { "|   " };

            let lic_str = match child.license.category {
                LicenseCategory::Permissive => child.license.raw.green().to_string(),
                LicenseCategory::WeakCopyleft => child.license.raw.yellow().to_string(),
                LicenseCategory::StrongCopyleft
                | LicenseCategory::NetworkCopyleft
                | LicenseCategory::NonCommercial => child.license.raw.red().bold().to_string(),
                _ => child.license.raw.cyan().to_string(),
            };

            let already_visited = visited.contains(&child.id);
            if already_visited {
                println!(
                    "{}{}{} [{}] (deduped)",
                    prefix,
                    branch,
                    child.id.to_string_repr().white(),
                    lic_str
                );
            } else {
                println!(
                    "{}{}{} [{}]",
                    prefix,
                    branch,
                    child.id.to_string_repr().white(),
                    lic_str
                );
                visited.insert(child.id.clone());
                let new_prefix = format!("{}{}", prefix, child_prefix);
                Self::render_children(graph, &child.id, &new_prefix, visited);
            }
        }
    }
}
