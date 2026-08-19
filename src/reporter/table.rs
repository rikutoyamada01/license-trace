use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, ContentArrangement, Table};

use crate::model::{DependencyGraph, LicenseCategory, PackageInfo};

pub struct TableReporter;

impl TableReporter {
    pub fn render(graph: &DependencyGraph, prod_only: bool) {
        let mut table = Table::new();
        table.load_preset(UTF8_FULL);
        table.apply_modifier(UTF8_ROUND_CORNERS);
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(vec![
            Cell::new("Package").fg(Color::Cyan),
            Cell::new("Version").fg(Color::Cyan),
            Cell::new("License").fg(Color::Cyan),
            Cell::new("Category").fg(Color::Cyan),
            Cell::new("Dep Type").fg(Color::Cyan),
            Cell::new("Scope").fg(Color::Cyan),
        ]);

        let mut packages: Vec<&PackageInfo> = graph.all_packages().into_iter().collect();
        packages.sort_by(|a, b| a.id.name.cmp(&b.id.name));

        for pkg in packages {
            if pkg.id == graph.root_id {
                continue;
            }
            if prod_only && pkg.scope == crate::model::DependencyScope::Development {
                continue;
            }

            let cat_cell = match pkg.license.category {
                LicenseCategory::Permissive => {
                    Cell::new(pkg.license.category.label()).fg(Color::Green)
                }
                LicenseCategory::WeakCopyleft => {
                    Cell::new(pkg.license.category.label()).fg(Color::Yellow)
                }
                LicenseCategory::StrongCopyleft
                | LicenseCategory::NetworkCopyleft
                | LicenseCategory::NonCommercial => {
                    Cell::new(pkg.license.category.label()).fg(Color::Red)
                }
                _ => Cell::new(pkg.license.category.label()).fg(Color::Cyan),
            };

            let dep_type_str = match pkg.dep_type {
                crate::model::DependencyType::Direct => "Direct",
                crate::model::DependencyType::Transitive => "Transitive",
            };

            let scope_str = match pkg.scope {
                crate::model::DependencyScope::Production => "prod",
                crate::model::DependencyScope::Development => "dev",
                crate::model::DependencyScope::Peer => "peer",
                crate::model::DependencyScope::Optional => "optional",
            };

            table.add_row(vec![
                Cell::new(&pkg.id.name),
                Cell::new(&pkg.id.version),
                Cell::new(&pkg.license.raw),
                cat_cell,
                Cell::new(dep_type_str),
                Cell::new(scope_str),
            ]);
        }

        println!("{table}");
    }
}
