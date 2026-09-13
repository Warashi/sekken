//! 条件付きモデルが採点する文に前置する入力の種類。

use sekken_core::input::Input;
use sekken_core::kana::{KanaTable, hira2kata};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Condition {
    /// 入力を見ずに文だけを採点する。
    #[default]
    None,
    /// 大文字境界付きのローマ字入力をそのまま前置する形。エンジンは綴りを
    /// 受け取らなくなったので採点できず、この形のファイルは読み込みで拒む。
    /// 番号を詰めると後の種類が読めなくなるので、変種だけ残す。
    Roman,
    /// `pairs` / `zenz-pairs` が出すカタカナ読みを前置する。
    Katakana,
    /// カタカナ読みと出力を漢字の並びごとに交互に並べる（`interleave`）。
    /// 前置する入力は無く、採点する文ごとに読みを区間に分けて挟む。
    Interleaved,
}

impl Condition {
    /// 入力のかな読みを、モデルが学習した形で前置する文字列にする。
    /// 前置しない種類なら `None`。
    pub fn prefix(self, reading: &str) -> Option<String> {
        match self {
            Condition::None | Condition::Roman | Condition::Interleaved => None,
            Condition::Katakana => Some(hira2kata(reading)),
        }
    }

    /// 交互形なら、採点する文に区間ごとに挟むカタカナ読み。
    pub fn interleaved_reading(self, reading: &str) -> Option<String> {
        match self {
            Condition::Interleaved => Some(hira2kata(reading)),
            _ => None,
        }
    }
}

/// 大文字境界付きのローマ字入力をカタカナ読みにする。学習データを作る側が使い、
/// エディタが区間ごとにかな化して送る読みと同じ形になるようにする。
pub fn roman_to_katakana(table: &KanaTable, input: &str) -> String {
    hira2kata(&Input::from_roman(table, input).reading())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 無条件なら前置しない() {
        assert_eq!(Condition::None.prefix("ねこ"), None);
    }

    #[test]
    fn 交互形は前置せず区間に挟む読みを返す() {
        assert_eq!(Condition::Interleaved.prefix("ねこ"), None);
        assert_eq!(
            Condition::Interleaved
                .interleaved_reading("ねこがなく")
                .as_deref(),
            Some("ネコガナク")
        );
        assert_eq!(Condition::Katakana.interleaved_reading("ねこ"), None);
    }

    #[test]
    fn カタカナは読みをカタカナにして前置する() {
        assert_eq!(
            Condition::Katakana
                .prefix("わがはいはねこである。")
                .as_deref(),
            Some("ワガハイハネコデアル。")
        );
        assert_eq!(
            Condition::Katakana.prefix("げーるもじをかった").as_deref(),
            Some("ゲールモジヲカッタ")
        );
    }

    #[test]
    fn ローマ字を区間ごとにかな化してカタカナ読みにする() {
        let t = KanaTable::default_table();
        assert_eq!(
            roman_to_katakana(&t, "WagahaihaNekodearu."),
            "ワガハイハネコデアル。"
        );
        assert_eq!(roman_to_katakana(&t, ";neko;ga;naku"), "ネコガナク");
        // 区間をまたいで組にならない。続けて変換すると「カニ」になる。
        assert_eq!(roman_to_katakana(&t, "KanI"), "カンイ");
    }
}
