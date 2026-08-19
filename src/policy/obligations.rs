use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

use crate::model::{DependencyGraph, LicenseCategory, PackageInfo, SourceDisclosureLevel};

/// 依存関係全体から導出される義務と制約の集約（下限）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateObligations {
    pub commercial_use_allowed: bool,
    pub distribution_allowed: bool,
    pub notice_required: bool,
    pub notice_package_count: usize,
    pub notice_requiring_licenses: BTreeMap<String, usize>,
    pub worst_source_disclosure: SourceDisclosureLevel,
    pub state_changes_required: bool,
    pub patent_grant_present: bool,
    pub unknown_license_count: usize,
    /// 個別ライセンス名ごとの件数 (例: "MIT" => 65, "Apache-2.0" => 12)
    pub license_counts: BTreeMap<String, usize>,
    /// カテゴリごとの件数
    pub category_counts: BTreeMap<String, usize>,
    pub problematic_packages: Vec<PackageInfo>,
    pub weak_copyleft_packages: Vec<PackageInfo>,
    pub review_needed_packages: Vec<PackageInfo>,
}

impl AggregateObligations {
    pub fn compute_from_graph(graph: &DependencyGraph, prod_only: bool) -> Self {
        let mut commercial_use = true;
        let mut distribution = true;
        let mut notice = false;
        let mut notice_package_count = 0;
        let mut notice_requiring_licenses: BTreeMap<String, usize> = BTreeMap::new();
        let mut worst_disclosure = SourceDisclosureLevel::None;
        let mut state_changes = false;
        let mut patent_grant = false;
        let mut unknown_count = 0;
        let mut license_counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut category_counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut problematic_packages = Vec::new();
        let mut weak_copyleft_packages = Vec::new();
        let mut review_needed_packages = Vec::new();

        let mut problematic_set = HashSet::new();
        let mut weak_copyleft_set = HashSet::new();
        let mut review_needed_set = HashSet::new();

        for pkg in graph.all_packages() {
            if pkg.id == graph.root_id {
                continue;
            }

            if prod_only && pkg.scope == crate::model::DependencyScope::Development {
                continue;
            }

            // ライセンス名（正規化名または生の名前）を集計
            let lic_name = pkg
                .license
                .normalized
                .clone()
                .unwrap_or_else(|| pkg.license.raw.clone());
            *license_counts.entry(lic_name.clone()).or_insert(0) += 1;

            let cat_label = pkg.license.category.label().to_string();
            *category_counts.entry(cat_label).or_insert(0) += 1;

            let obs = &pkg.license.obligations;

            let mut is_problematic = false;
            if !obs.commercial_use_allowed {
                commercial_use = false;
                is_problematic = true;
            }
            if !obs.distribution_allowed {
                distribution = false;
                is_problematic = true;
            }
            if obs.notice_required {
                notice = true;
                notice_package_count += 1;
                *notice_requiring_licenses.entry(lic_name).or_insert(0) += 1;
            }
            if obs.source_disclosure > worst_disclosure {
                worst_disclosure = obs.source_disclosure;
            }
            if obs.source_disclosure >= SourceDisclosureLevel::ProjectLevel {
                is_problematic = true;
            }
            if obs.state_changes_required {
                state_changes = true;
            }
            if obs.patent_grant {
                patent_grant = true;
            }

            if pkg.license.category == LicenseCategory::Unknown || obs.is_unknown {
                unknown_count += 1;
                if review_needed_set.insert(pkg.id.clone()) {
                    review_needed_packages.push(pkg.clone());
                }
            }

            if is_problematic {
                if problematic_set.insert(pkg.id.clone()) {
                    problematic_packages.push(pkg.clone());
                }
            }

            if obs.source_disclosure == SourceDisclosureLevel::LibraryLevel || pkg.license.category == LicenseCategory::WeakCopyleft {
                if weak_copyleft_set.insert(pkg.id.clone()) {
                    weak_copyleft_packages.push(pkg.clone());
                }
            }
        }

        Self {
            commercial_use_allowed: commercial_use,
            distribution_allowed: distribution,
            notice_required: notice,
            notice_package_count,
            notice_requiring_licenses,
            worst_source_disclosure: worst_disclosure,
            state_changes_required: state_changes,
            patent_grant_present: patent_grant,
            unknown_license_count: unknown_count,
            license_counts,
            category_counts,
            problematic_packages,
            weak_copyleft_packages,
            review_needed_packages,
        }
    }
}
