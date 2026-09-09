//! 条件付きモデルが採点する文に前置する入力の種類。

use sekken_core::kana::{KanaTable, hira2kata};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Condition {
    /// 入力を見ずに文だけを採点する。
    #[default]
    None,
    /// 大文字境界付きのローマ字入力をそのまま前置する。条件の種類を記録する前の
    /// 旧ファイルを読むためだけに残していて、新しく学習することはない。
    Roman,
    /// `pairs` / `zenz-pairs` が出すカタカナ読みを前置する。ローマ字入力はカタカナに直す。
    Katakana,
}

impl Condition {
    /// 変換前のローマ字入力を、モデルが学習した形にする。
    pub fn prefix(self, table: &KanaTable, input: &str) -> Option<String> {
        match self {
            Condition::None => None,
            Condition::Roman => Some(input.to_string()),
            Condition::Katakana => Some(roman_to_katakana(table, input)),
        }
    }
}

/// 大文字境界付きのローマ字入力をカタカナ読みにする。学習データを作るときと
/// 採点するときの両方がこれを使い、同じ形になるようにする。
pub fn roman_to_katakana(table: &KanaTable, input: &str) -> String {
    hira2kata(&table.roman2kana(&input.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 無条件なら前置しない() {
        assert_eq!(
            Condition::None.prefix(&KanaTable::default_table(), "Neko"),
            None
        );
    }

    #[test]
    fn ローマ字はそのまま前置する() {
        assert_eq!(
            Condition::Roman
                .prefix(&KanaTable::default_table(), "NekogaNaku")
                .as_deref(),
            Some("NekogaNaku")
        );
    }

    #[test]
    fn カタカナは大文字境界を外してカタカナ読みにする() {
        let t = KanaTable::default_table();
        assert_eq!(
            Condition::Katakana
                .prefix(&t, "WagahaihaNekodearu.")
                .as_deref(),
            Some("ワガハイハネコデアル。")
        );
        assert_eq!(
            Condition::Katakana
                .prefix(&t, "Ge-ruMojiwoKaxtuta")
                .as_deref(),
            Some("ゲールモジヲカッタ")
        );
    }
}
