use serde::{Deserialize, Serialize};
use crate::model::LicenseCategory;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyPreset {
    /// 寛容型のみ（商用利用・再配布・改変が最も自由）
    Permissive,
    /// 商用フレンドリー（Permissive + 弱コピーレフトの動的利用）
    CommercialFriendly,
    /// 厳格（GPL, AGPL, SSPLなどのコピーレフトおよび非商用を拒絶）
    Strict,
}

#[allow(dead_code)]
impl PolicyPreset {
    pub fn is_allowed(&self, category: LicenseCategory) -> bool {
        match self {
            PolicyPreset::Permissive => matches!(category, LicenseCategory::Permissive),
            PolicyPreset::CommercialFriendly => {
                matches!(category, LicenseCategory::Permissive | LicenseCategory::WeakCopyleft)
            }
            PolicyPreset::Strict => {
                !matches!(
                    category,
                    LicenseCategory::StrongCopyleft
                        | LicenseCategory::NetworkCopyleft
                        | LicenseCategory::NonCommercial
                        | LicenseCategory::Proprietary
                )
            }
        }
    }
}
