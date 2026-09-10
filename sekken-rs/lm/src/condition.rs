//! 条件付きモデルが採点する文に前置する入力の種類。

use sekken_core::kana::{KanaTable, hira2kata};
use sekken_core::segment::segment;
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
            Condition::Roman => Some(canonical_roman(input)),
            Condition::Katakana => Some(roman_to_katakana(table, input)),
        }
    }
}

/// `;` 境界を、旧モデルが学習した大文字境界へ正規化する。
fn canonical_roman(input: &str) -> String {
    let segmented = segment(input);
    let mut output = segmented.prefix;
    for segment in segmented.segments {
        let mut chars = segment.chars();
        if let Some(first) = chars.next() {
            output.push(first.to_ascii_uppercase());
            output.extend(chars);
        }
    }
    output
}

/// 大文字境界付きのローマ字入力をカタカナ読みにする。学習データを作るときと
/// 採点するときの両方がこれを使い、同じ形になるようにする。
pub fn roman_to_katakana(table: &KanaTable, input: &str) -> String {
    let segmented = segment(input);
    let roman = segmented.prefix + &segmented.segments.concat();
    hira2kata(&table.roman2kana(&roman))
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
        assert_eq!(
            Condition::Roman
                .prefix(&KanaTable::default_table(), ";neko;ga;naku")
                .as_deref(),
            Some("NekoGaNaku")
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
        assert_eq!(
            Condition::Katakana
                .prefix(&t, ";ge-ru;mojiwo;kaxtuta")
                .as_deref(),
            Some("ゲールモジヲカッタ")
        );
    }
}
