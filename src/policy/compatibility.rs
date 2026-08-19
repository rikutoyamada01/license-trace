use serde::{Deserialize, Serialize};
use crate::model::{DependencyGraph, LicenseAnalysis, LicenseCategory, SourceDisclosureLevel};
use super::obligations::AggregateObligations;

/// 公開ライセンスに対する適合性判定結果
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompatibilityStatus {
    /// 完全に適合（問題なし）
    Compatible,
    /// 警告（条件付き適合、例: LGPL動的リンク、著作権表示・NOTICEファイル同梱が必要）
    Warning,
    /// 非互換（ライセンス違反の恐れ、例: GPL依存があるのにMITで公開・頒布）
    Incompatible,
    /// 要調査（未確定・カスタムライセンスが含まれる）
    NeedsReview,
}

impl CompatibilityStatus {
    pub fn label(&self) -> &'static str {
        match self {
            CompatibilityStatus::Compatible => "COMPATIBLE",
            CompatibilityStatus::Warning => "WARNING",
            CompatibilityStatus::Incompatible => "INCOMPATIBLE",
            CompatibilityStatus::NeedsReview => "NEEDS_REVIEW",
        }
    }
}

/// 適合性チェックの総合レポート
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityReport {
    /// 公開予定/指定ライセンス (例: "MIT")
    pub outbound_license: String,
    /// 総合判定ステータス
    pub status: CompatibilityStatus,
    /// 判定理由・詳細メッセージ
    pub summary: String,
    /// 詳細な検出事項・アドバイス
    pub findings: Vec<String>,
    /// 集約された義務
    pub obligations: AggregateObligations,
}

impl CompatibilityReport {
    pub fn evaluate(outbound_license: &str, graph: &DependencyGraph, prod_only: bool) -> Self {
        let obs = AggregateObligations::compute_from_graph(graph, prod_only);
        let mut status = CompatibilityStatus::Compatible;
        let mut findings = Vec::new();

        let outbound_analysis = LicenseAnalysis::parse(outbound_license);

        // 1. 商用利用制限チェック
        if !obs.commercial_use_allowed {
            // Outbound自体がNonCommercialまたはProprietaryでない限り、商用利用禁止依存はNG
            if outbound_analysis.category != LicenseCategory::NonCommercial {
                status = CompatibilityStatus::Incompatible;
                findings.push("Dependency contains Non-Commercial (e.g., CC-BY-NC or BUSL) license, which prohibits unrestricted commercial distribution.".to_string());
            }
        }

        // 2. コピーレフト・ソース開示義務チェック
        match outbound_analysis.category {
            LicenseCategory::Permissive => {
                if obs.worst_source_disclosure >= SourceDisclosureLevel::ProjectLevel {
                    status = CompatibilityStatus::Incompatible;
                    findings.push(format!(
                        "Incompatible Copyleft: Project contains Strong/Network Copyleft dependencies (e.g., GPL/AGPL). You cannot distribute this project under a permissive '{}' license without complying with project-wide source disclosure requirements.",
                        outbound_license
                    ));
                } else if obs.worst_source_disclosure == SourceDisclosureLevel::LibraryLevel {
                    if status != CompatibilityStatus::Incompatible {
                        status = CompatibilityStatus::Warning;
                    }
                    findings.push(
                        "Weak Copyleft Notice: Dependencies include Weak Copyleft (e.g., LGPL/MPL/CC-BY-SA). Releasing under a permissive license is permitted provided that the weak-copyleft components remain unmodified and dynamic linking/replacement is supported.".to_string()
                    );
                }
            }
            LicenseCategory::WeakCopyleft => {
                if obs.worst_source_disclosure >= SourceDisclosureLevel::ProjectLevel {
                    status = CompatibilityStatus::Incompatible;
                    findings.push(format!(
                        "Incompatible Copyleft: Project contains Strong/Network Copyleft dependencies (e.g., GPL/AGPL). Distributing under Weak Copyleft '{}' is not compatible.",
                        outbound_license
                    ));
                }
            }
            LicenseCategory::StrongCopyleft => {
                if obs.worst_source_disclosure >= SourceDisclosureLevel::NetworkLevel {
                    status = CompatibilityStatus::Incompatible;
                    findings.push(format!(
                        "Network Copyleft Notice: Project contains Network Copyleft dependencies (e.g., AGPL/SSPL). Distributing under '{}' is incompatible if operated over a network (AGPL/SSPL requires network-triggered source disclosure).",
                        outbound_license
                    ));
                }
            }
            _ => {}
        }

        // 3. 不明ライセンスチェック
        if obs.unknown_license_count > 0 {
            if status == CompatibilityStatus::Compatible {
                status = CompatibilityStatus::NeedsReview;
            }
            findings.push(format!(
                "Manual Review Required: Found {} package(s) with UNKNOWN or non-standard licenses. Please inspect them to ensure license compliance.",
                obs.unknown_license_count
            ));
        }

        // 4. 著作権表示の遵守要件
        if obs.notice_required {
            findings.push(
                "Attribution Requirement: Dependencies require copyright notices and original license texts to be included in distributions or documentation.".to_string()
            );
        }

        // 5. 概要サマリー作成
        let summary = match status {
            CompatibilityStatus::Compatible => format!("All dependencies are compatible with '{}'. Safe for distribution!", outbound_license),
            CompatibilityStatus::Warning => format!("Compatible with '{}' under certain conditions (e.g. notices & un-modified weak copyleft).", outbound_license),
            CompatibilityStatus::Incompatible => format!("CRITICAL: Incompatible with '{}' due to strong copyleft or non-commercial constraints.", outbound_license),
            CompatibilityStatus::NeedsReview => format!("Needs manual review: {} unknown licenses found while verifying for '{}'.", obs.unknown_license_count, outbound_license),
        };

        Self {
            outbound_license: outbound_license.to_string(),
            status,
            summary,
            findings,
            obligations: obs,
        }
    }
}
