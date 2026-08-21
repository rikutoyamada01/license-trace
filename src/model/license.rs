use serde::{Deserialize, Serialize};
use spdx::Expression;

/// ライセンスの分類カテゴリ
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum LicenseCategory {
    /// 不明 / 要調査 (Non-standard, Missing, etc.)
    Unknown,
    /// 寛容型 (MIT, Apache-2.0, BSD, ISC等) - 商用利用・改変・再配布が自由
    Permissive,
    /// 弱コピーレフト (LGPL, MPL, EPL等) - ライブラリ単体の改変は開示必要だが結合物は開示不要
    WeakCopyleft,
    /// 強コピーレフト (GPL等) - 静的/動的リンクした派生物全体のソース開示義務
    StrongCopyleft,
    /// ネットワークコピーレフト (AGPL, SSPL等) - SaaS/ネットワーク利用でもソース開示義務
    NetworkCopyleft,
    /// 非商用限定 (CC-BY-NC等) - 商用利用が明示的に禁止
    NonCommercial,
    /// プロプライエタリ / 商用専用
    Proprietary,
}

impl LicenseCategory {
    pub fn label(&self) -> &'static str {
        match self {
            LicenseCategory::Permissive => "Permissive",
            LicenseCategory::WeakCopyleft => "Weak Copyleft",
            LicenseCategory::StrongCopyleft => "Strong Copyleft",
            LicenseCategory::NetworkCopyleft => "Network Copyleft",
            LicenseCategory::NonCommercial => "Non-Commercial",
            LicenseCategory::Proprietary => "Proprietary",
            LicenseCategory::Unknown => "Unknown (Needs Review)",
        }
    }
}

/// ソースコード開示義務のレベル
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SourceDisclosureLevel {
    /// なし (Permissive)
    None,
    /// 改変したファイル/ライブラリ単位 (MPL, LGPL等)
    LibraryLevel,
    /// 結合・派生したプロジェクト全体 (GPL等)
    ProjectLevel,
    /// ネットワークサービス経由でも開示 (AGPL等)
    NetworkLevel,
}

/// 依存関係から導出される義務・制約の評価
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseObligations {
    /// 商用利用可能か
    pub commercial_use_allowed: bool,
    /// 再配布可能か
    pub distribution_allowed: bool,
    /// 著作権・ライセンス表示義務があるか
    pub notice_required: bool,
    /// ソースコード開示義務レベル
    pub source_disclosure: SourceDisclosureLevel,
    /// 変更履歴の記載義務
    pub state_changes_required: bool,
    /// 明示的な特許許諾が含まれるか
    pub patent_grant: bool,
    /// 要調査 (UNKNOWN) フラグ
    pub is_unknown: bool,
}

impl Default for LicenseObligations {
    fn default() -> Self {
        Self {
            commercial_use_allowed: true,
            distribution_allowed: true,
            notice_required: false,
            source_disclosure: SourceDisclosureLevel::None,
            state_changes_required: false,
            patent_grant: false,
            is_unknown: false,
        }
    }
}

/// ライセンス情報の解析結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseAnalysis {
    /// 生のライセンス文字列
    pub raw: String,
    /// 正規化されたSPDX識別子または式（パース可能な場合）
    pub normalized: Option<String>,
    /// 分類カテゴリ
    pub category: LicenseCategory,
    /// 義務・制約
    pub obligations: LicenseObligations,
    /// 含まれる個別SPDXライセンスIDのセット
    pub identifiers: Vec<String>,
}

impl LicenseAnalysis {
    /// 生のライセンス文字列を解析して分類と義務を判定
    /// 生のライセンス文字列を解析して分類と義務を判定
    pub fn parse(raw_license: &str) -> Self {
        let trimmed = raw_license.trim();
        if trimmed.is_empty()
            || trimmed.eq_ignore_ascii_case("unknown")
            || trimmed.eq_ignore_ascii_case("unlicensed")
        {
            return Self::unknown(trimmed);
        }

        // SPDX 式としてパースを試みる
        match Expression::parse(trimmed) {
            Ok(expr) => {
                let mut ids = Vec::new();
                for item in expr.requirements() {
                    if let Some(lic_id) = item.req.license.id() {
                        ids.push(lic_id.name.to_string());
                    }
                }

                if ids.is_empty() {
                    return Self::fallback_parse(trimmed);
                }

                // AST (RPN) による厳密な評価 (ORはbest、ANDはworst)
                let (category, obligations) = evaluate_expression_ast(&expr);

                Self {
                    raw: trimmed.to_string(),
                    normalized: Some(expr.to_string()),
                    category,
                    obligations,
                    identifiers: ids,
                }
            }
            Err(_) => {
                // 一般的な文字列マッチングによるフォールバック
                Self::fallback_parse(trimmed)
            }
        }
    }

    pub fn unknown(raw: &str) -> Self {
        Self {
            raw: if raw.is_empty() {
                "UNKNOWN".to_string()
            } else {
                raw.to_string()
            },
            normalized: None,
            category: LicenseCategory::Unknown,
            obligations: LicenseObligations {
                commercial_use_allowed: true, // 不確定だが要調査
                distribution_allowed: true,
                notice_required: true,
                source_disclosure: SourceDisclosureLevel::None,
                state_changes_required: false,
                patent_grant: false,
                is_unknown: true,
            },
            identifiers: Vec::new(),
        }
    }

    fn fallback_parse(raw: &str) -> Self {
        let upper = raw.to_uppercase();
        let cleaned = upper.replace(['(', ')', '/'], " ");
        let tokens: Vec<&str> = cleaned.split_whitespace().collect();

        // 1. BUSL (Business Source License)
        if upper.contains("BUSL") {
            let (cat, obs) = categorize_single_spdx("BUSL-1.1");
            return Self {
                raw: raw.to_string(),
                normalized: Some("BUSL-1.1".to_string()),
                category: cat,
                obligations: obs,
                identifiers: vec!["BUSL-1.1".to_string()],
            };
        }

        // 2. AGPL / SSPL
        if upper.contains("AGPL") || upper.contains("SSPL") {
            let spdx = if upper.contains("1.0") {
                "AGPL-1.0-only"
            } else {
                "AGPL-3.0-only"
            };
            return Self::single_license(raw, spdx, LicenseCategory::NetworkCopyleft);
        }

        // 3. LGPL (GPL より先に判定)
        if upper.contains("LGPL") {
            let spdx = if upper.contains("2.1") {
                "LGPL-2.1-only"
            } else if upper.contains("2.0") || upper.contains("2") {
                "LGPL-2.0-only"
            } else {
                "LGPL-3.0-only"
            };
            return Self::single_license(raw, spdx, LicenseCategory::WeakCopyleft);
        }

        // 4. GPL
        if upper.contains("GPL") {
            let spdx = if upper.contains("2.0") || upper.contains("2") {
                "GPL-2.0-only"
            } else {
                "GPL-3.0-only"
            };
            return Self::single_license(raw, spdx, LicenseCategory::StrongCopyleft);
        }

        // 5. MPL / EPL / CDDL
        if upper.contains("MPL") {
            let spdx = if upper.contains("1.1") {
                "MPL-1.1"
            } else {
                "MPL-2.0"
            };
            return Self::single_license(raw, spdx, LicenseCategory::WeakCopyleft);
        }
        if upper.contains("EPL") {
            let spdx = if upper.contains("1.0") {
                "EPL-1.0"
            } else {
                "EPL-2.0"
            };
            return Self::single_license(raw, spdx, LicenseCategory::WeakCopyleft);
        }

        // 6. CC-BY-NC / CC-BY-SA
        if upper.contains("CC-BY-NC") || upper.contains("CC BY NC") {
            return Self::single_license(raw, "CC-BY-NC-4.0", LicenseCategory::NonCommercial);
        }
        if upper.contains("CC-BY-SA") || upper.contains("CC BY SA") {
            return Self::single_license(raw, "CC-BY-SA-4.0", LicenseCategory::WeakCopyleft);
        }

        // 7. Apache
        if tokens.contains(&"APACHE-2.0")
            || tokens.contains(&"APACHE")
            || tokens.contains(&"APACHE2")
        {
            let mut analysis = Self::single_license(raw, "Apache-2.0", LicenseCategory::Permissive);
            analysis.obligations.patent_grant = true;
            analysis.obligations.state_changes_required = true;
            return analysis;
        }

        // 8. BSD
        if tokens.contains(&"BSD-3-CLAUSE")
            || tokens.contains(&"BSD-2-CLAUSE")
            || tokens.contains(&"BSD")
        {
            return Self::single_license(raw, "BSD-3-Clause", LicenseCategory::Permissive);
        }

        // 9. Permissive (MIT, ISC, 0BSD, etc.)
        if tokens.contains(&"MIT")
            || tokens.contains(&"ISC")
            || tokens.contains(&"0BSD")
            || tokens.contains(&"UNLICENSE")
        {
            let spdx = if tokens.contains(&"ISC") {
                "ISC"
            } else if tokens.contains(&"0BSD") {
                "0BSD"
            } else if tokens.contains(&"UNLICENSE") {
                "Unlicense"
            } else {
                "MIT"
            };
            return Self::single_license(raw, spdx, LicenseCategory::Permissive);
        }

        Self::unknown(raw)
    }

    fn single_license(raw: &str, spdx_id: &str, category: LicenseCategory) -> Self {
        let (cat, obligations) = categorize_single_spdx(spdx_id);
        Self {
            raw: raw.to_string(),
            normalized: Some(spdx_id.to_string()),
            category: if category == LicenseCategory::Unknown {
                cat
            } else {
                category
            },
            obligations,
            identifiers: vec![spdx_id.to_string()],
        }
    }
}

pub fn categorize_single_spdx(id: &str) -> (LicenseCategory, LicenseObligations) {
    let upper = id.to_uppercase();

    // 1. BUSL (Business Source License) -> NonCommercial
    if upper.starts_with("BUSL") {
        return (
            LicenseCategory::NonCommercial,
            LicenseObligations {
                commercial_use_allowed: false,
                distribution_allowed: true,
                notice_required: true,
                source_disclosure: SourceDisclosureLevel::None,
                state_changes_required: true,
                patent_grant: true,
                is_unknown: false,
            },
        );
    }

    // 2. AGPL / SSPL (Network Copyleft)
    if upper.starts_with("AGPL") || upper.starts_with("SSPL") {
        return (
            LicenseCategory::NetworkCopyleft,
            LicenseObligations {
                commercial_use_allowed: true,
                distribution_allowed: true,
                notice_required: true,
                source_disclosure: SourceDisclosureLevel::NetworkLevel,
                state_changes_required: true,
                patent_grant: true,
                is_unknown: false,
            },
        );
    }

    // 3. LGPL / MPL / EPL / CDDL / CC-BY-SA (Weak Copyleft)
    if upper.starts_with("LGPL")
        || upper.starts_with("MPL")
        || upper.starts_with("EPL")
        || upper.starts_with("CDDL")
        || upper.starts_with("CC-BY-SA")
        || upper.starts_with("CC-BY-NC-SA")
    {
        let is_nc = upper.contains("NC");
        let cat = if is_nc {
            LicenseCategory::NonCommercial
        } else {
            LicenseCategory::WeakCopyleft
        };
        return (
            cat,
            LicenseObligations {
                commercial_use_allowed: !is_nc,
                distribution_allowed: true,
                notice_required: true,
                source_disclosure: SourceDisclosureLevel::LibraryLevel,
                state_changes_required: true,
                patent_grant: upper.starts_with("MPL") || upper.starts_with("LGPL-3"),
                is_unknown: false,
            },
        );
    }

    // 4. GPL (Strong Copyleft)
    if upper.starts_with("GPL") {
        return (
            LicenseCategory::StrongCopyleft,
            LicenseObligations {
                commercial_use_allowed: true,
                distribution_allowed: true,
                notice_required: true,
                source_disclosure: SourceDisclosureLevel::ProjectLevel,
                state_changes_required: true,
                patent_grant: upper.starts_with("GPL-3"),
                is_unknown: false,
            },
        );
    }

    // 5. Non-Commercial (CC-BY-NC etc.)
    if upper.contains("NC") && (upper.starts_with("CC-BY") || upper.starts_with("CC BY")) {
        return (
            LicenseCategory::NonCommercial,
            LicenseObligations {
                commercial_use_allowed: false,
                distribution_allowed: true,
                notice_required: true,
                source_disclosure: SourceDisclosureLevel::None,
                state_changes_required: true,
                patent_grant: false,
                is_unknown: false,
            },
        );
    }

    // 6. Proprietary / Commercial restriction
    if upper.contains("PROPRIETARY") || upper.starts_with("LICENSEREF-NVIDIA") {
        return (
            LicenseCategory::Proprietary,
            LicenseObligations {
                commercial_use_allowed: true,
                distribution_allowed: false,
                notice_required: true,
                source_disclosure: SourceDisclosureLevel::None,
                state_changes_required: false,
                patent_grant: false,
                is_unknown: false,
            },
        );
    }

    // 7. Apache-2.0
    if upper.starts_with("APACHE") {
        return (
            LicenseCategory::Permissive,
            LicenseObligations {
                commercial_use_allowed: true,
                distribution_allowed: true,
                notice_required: true,
                source_disclosure: SourceDisclosureLevel::None,
                state_changes_required: true,
                patent_grant: true,
                is_unknown: false,
            },
        );
    }

    // 8. BSD Licenses
    if upper.starts_with("BSD") {
        return (
            LicenseCategory::Permissive,
            LicenseObligations {
                commercial_use_allowed: true,
                distribution_allowed: true,
                notice_required: true,
                source_disclosure: SourceDisclosureLevel::None,
                state_changes_required: false,
                patent_grant: false,
                is_unknown: false,
            },
        );
    }

    // 9. Permissive (MIT-*, PSF, Python, ISC, 0BSD, Unlicense, CC0, Zlib, BSL, Unicode, etc.)
    if upper.starts_with("MIT")
        || upper.starts_with("PSF")
        || upper.starts_with("PYTHON")
        || upper.starts_with("CNRI-PYTHON")
        || upper.starts_with("UNICODE")
        || upper.starts_with("CDLA-PERMISSIVE")
        || upper.starts_with("0BSD")
        || upper.starts_with("CC0")
        || upper.starts_with("UNLICENSE")
        || upper.starts_with("ZLIB")
        || upper.starts_with("WTFPL")
        || upper.starts_with("POSTGRESQL")
        || upper.starts_with("RUBY")
        || upper.starts_with("BSL-1.0")
        || upper.starts_with("OPENSSL")
        || upper.starts_with("CURL")
        || upper.starts_with("W3C")
        || upper.starts_with("ISC")
        || upper == "DUAL LICENSE"
    {
        return (
            LicenseCategory::Permissive,
            LicenseObligations {
                commercial_use_allowed: true,
                distribution_allowed: true,
                notice_required: !upper.starts_with("0BSD")
                    && !upper.starts_with("UNLICENSE")
                    && !upper.starts_with("CC0")
                    && upper != "MIT-0",
                source_disclosure: SourceDisclosureLevel::None,
                state_changes_required: false,
                patent_grant: false,
                is_unknown: false,
            },
        );
    }

    (
        LicenseCategory::Unknown,
        LicenseObligations {
            commercial_use_allowed: true,
            distribution_allowed: true,
            notice_required: true,
            source_disclosure: SourceDisclosureLevel::None,
            state_changes_required: false,
            patent_grant: false,
            is_unknown: true,
        },
    )
}

/// AST (RPN) による SPDX 式の再帰評価
fn evaluate_expression_ast(expr: &Expression) -> (LicenseCategory, LicenseObligations) {
    use spdx::expression::{ExprNode, Operator};

    let mut stack: Vec<(LicenseCategory, LicenseObligations)> = Vec::new();

    for node in expr.iter() {
        match node {
            ExprNode::Req(req) => {
                let name = req.req.license.id().map(|id| id.name).unwrap_or("UNKNOWN");
                let eval = categorize_single_spdx(name);
                stack.push(eval);
            }
            ExprNode::Op(Operator::Or) => {
                if stack.len() >= 2 {
                    let right = stack.pop().unwrap();
                    let left = stack.pop().unwrap();
                    stack.push(combine_or(left, right));
                }
            }
            ExprNode::Op(Operator::And) => {
                if stack.len() >= 2 {
                    let right = stack.pop().unwrap();
                    let left = stack.pop().unwrap();
                    stack.push(combine_and(left, right));
                }
            }
        }
    }

    stack
        .pop()
        .unwrap_or_else(|| categorize_single_spdx("UNKNOWN"))
}

/// OR 結合: 利用者は一方を選択できる（最も有利な best を採用）
fn combine_or(
    left: (LicenseCategory, LicenseObligations),
    right: (LicenseCategory, LicenseObligations),
) -> (LicenseCategory, LicenseObligations) {
    let (l_cat, l_obs) = left;
    let (r_cat, r_obs) = right;

    // カテゴリ: 有利な側（最小カテゴリ）を選択可能
    // ただし Unknown と Permissive の場合、既知の Permissive を選択可能
    let final_cat = match (l_cat, r_cat) {
        (LicenseCategory::Unknown, other) | (other, LicenseCategory::Unknown) => other,
        (a, b) => {
            if a < b {
                a
            } else {
                b
            }
        }
    };

    let final_obs = LicenseObligations {
        commercial_use_allowed: l_obs.commercial_use_allowed || r_obs.commercial_use_allowed,
        distribution_allowed: l_obs.distribution_allowed || r_obs.distribution_allowed,
        source_disclosure: std::cmp::min(l_obs.source_disclosure, r_obs.source_disclosure),
        notice_required: l_obs.notice_required && r_obs.notice_required,
        state_changes_required: l_obs.state_changes_required && r_obs.state_changes_required,
        patent_grant: l_obs.patent_grant || r_obs.patent_grant,
        is_unknown: l_obs.is_unknown && r_obs.is_unknown,
    };

    (final_cat, final_obs)
}

/// AND 結合: 利用者は両方の義務を同時に果たす必要がある（最も制約の重い worst を採用）
fn combine_and(
    left: (LicenseCategory, LicenseObligations),
    right: (LicenseCategory, LicenseObligations),
) -> (LicenseCategory, LicenseObligations) {
    let (l_cat, l_obs) = left;
    let (r_cat, r_obs) = right;

    let final_cat = match (l_cat, r_cat) {
        (LicenseCategory::Unknown, LicenseCategory::Permissive)
        | (LicenseCategory::Permissive, LicenseCategory::Unknown) => LicenseCategory::Unknown,
        (a, b) => {
            if a > b {
                a
            } else {
                b
            }
        }
    };

    let final_obs = LicenseObligations {
        commercial_use_allowed: l_obs.commercial_use_allowed && r_obs.commercial_use_allowed,
        distribution_allowed: l_obs.distribution_allowed && r_obs.distribution_allowed,
        source_disclosure: std::cmp::max(l_obs.source_disclosure, r_obs.source_disclosure),
        notice_required: l_obs.notice_required || r_obs.notice_required,
        state_changes_required: l_obs.state_changes_required || r_obs.state_changes_required,
        patent_grant: l_obs.patent_grant || r_obs.patent_grant,
        is_unknown: l_obs.is_unknown || r_obs.is_unknown,
    };

    (final_cat, final_obs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mit() {
        let analysis = LicenseAnalysis::parse("MIT");
        assert_eq!(analysis.category, LicenseCategory::Permissive);
        assert!(analysis.obligations.commercial_use_allowed);
        assert_eq!(
            analysis.obligations.source_disclosure,
            SourceDisclosureLevel::None
        );
    }

    #[test]
    fn test_parse_gpl() {
        let analysis = LicenseAnalysis::parse("GPL-3.0-only");
        assert_eq!(analysis.category, LicenseCategory::StrongCopyleft);
        assert_eq!(
            analysis.obligations.source_disclosure,
            SourceDisclosureLevel::ProjectLevel
        );
    }

    #[test]
    fn test_parse_unknown() {
        let analysis = LicenseAnalysis::parse("Custom-Proprietary-Foo");
        assert_eq!(analysis.category, LicenseCategory::Unknown);
        assert!(analysis.obligations.is_unknown);
    }

    #[test]
    fn test_or_expr_takes_best() {
        let analysis = LicenseAnalysis::parse("GPL-2.0-only OR MIT");
        assert_eq!(analysis.category, LicenseCategory::Permissive);
        assert_eq!(
            analysis.obligations.source_disclosure,
            SourceDisclosureLevel::None
        );
        assert!(analysis.obligations.commercial_use_allowed);
    }

    #[test]
    fn test_and_expr_takes_worst() {
        let analysis = LicenseAnalysis::parse("GPL-2.0-only AND MIT");
        assert_eq!(analysis.category, LicenseCategory::StrongCopyleft);
        assert_eq!(
            analysis.obligations.source_disclosure,
            SourceDisclosureLevel::ProjectLevel
        );
    }

    #[test]
    fn test_busl_is_noncommercial() {
        let analysis = LicenseAnalysis::parse("BUSL-1.1");
        assert_eq!(analysis.category, LicenseCategory::NonCommercial);
        assert!(!analysis.obligations.commercial_use_allowed);
    }

    #[test]
    fn test_bsl_is_permissive() {
        let analysis = LicenseAnalysis::parse("BSL-1.0");
        assert_eq!(analysis.category, LicenseCategory::Permissive);
        assert!(analysis.obligations.commercial_use_allowed);
    }

    #[test]
    fn test_torch_composite_is_permissive() {
        let analysis = LicenseAnalysis::parse(
            "Apache-2.0 AND Apache-2.0 WITH LLVM-exception AND BSD-2-Clause AND BSD-3-Clause AND BSL-1.0 AND MIT",
        );
        assert_eq!(analysis.category, LicenseCategory::Permissive);
        assert!(analysis.obligations.commercial_use_allowed);
    }

    #[test]
    fn test_cc_by_sa_is_weak_copyleft() {
        let analysis = LicenseAnalysis::parse("CC-BY-SA-4.0");
        assert_eq!(analysis.category, LicenseCategory::WeakCopyleft);
        assert_eq!(
            analysis.obligations.source_disclosure,
            SourceDisclosureLevel::LibraryLevel
        );
    }
}
