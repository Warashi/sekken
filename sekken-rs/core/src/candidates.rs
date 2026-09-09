//! セグメントごとの変換候補の生成。

use crate::dictionary::Dictionary;
use crate::kana::{KanaTable, hira2kata};

/// 1 つのセグメント位置から始まる変換候補。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// 変換後の表層形。
    pub surface: String,
    /// 消費するセグメント数。送りありで次のセグメントを使うときは 2。
    pub span: usize,
    /// 辞書での候補の順位（0 始まり）。かな候補は 0。
    pub rank: usize,
    /// 辞書を引かず読みをそのままかなにした候補か。
    pub kana: bool,
}

impl Candidate {
    fn kana(surface: impl Into<String>) -> Candidate {
        Candidate {
            surface: surface.into(),
            span: 1,
            rank: 0,
            kana: true,
        }
    }

    fn ranked(surface: impl Into<String>, span: usize, rank: usize) -> Candidate {
        Candidate {
            surface: surface.into(),
            span,
            rank,
            kana: false,
        }
    }
}

/// セグメントを「読みになるローマ字」と「末尾の記号」に分ける。
/// 途中の記号（`ta-getto` の `-` → ー）は読みの一部なので末尾だけを見る。
fn split_trailing_symbols(segment: &str) -> (&str, &str) {
    let roman = segment.trim_end_matches(|c: char| !c.is_ascii_lowercase() && c != '\'');
    segment.split_at(roman.len())
}

/// 位置 `index` のセグメントから始まる候補をすべて列挙する。
///
/// セグメント `wagahaiha` は「辞書の読み `わがはい` + 残りのかな `は`」のように
/// 読みの前方一致で分けて引く。送り仮名は残りの先頭（`kaku` の `ku`）でも、
/// 次のセグメント（`Ka` `Ku`）でも表せる。
pub fn candidates_at(
    table: &KanaTable,
    dict: &Dictionary,
    segments: &[String],
    index: usize,
) -> Vec<Candidate> {
    let (roman, symbols) = split_trailing_symbols(&segments[index]);
    let whole = table.roman2kana(roman);
    let mut out = Vec::new();

    if symbols.is_empty()
        && let Some(next) = segments.get(index + 1)
    {
        let (okuri_roman, okuri_symbols) = split_trailing_symbols(next);
        if let Some(consonant) = okuri_roman.chars().next() {
            let okuri = table.roman2kana(okuri_roman) + &table.roman2kana(okuri_symbols);
            for (rank, surface) in dict.okuri_ari(&whole, consonant).iter().enumerate() {
                out.push(Candidate::ranked(format!("{surface}{okuri}"), 2, rank));
            }
        }
    }

    let whole_with_symbols = table.roman2kana(&segments[index]);
    for split in (1..=roman.len()).rev() {
        let (head, rest) = roman.split_at(split);
        let yomi = table.roman2kana(head);
        // 残りと記号はまとめて変換する。`z/` → `・` のように記号を含む組があるため。
        let suffix = table.roman2kana(&format!("{rest}{symbols}"));
        // かなの切れ目でない位置（`nek` + `o` → ねっ + お）は読みとして成り立たない。
        if yomi.chars().any(|c| c.is_ascii()) || yomi.clone() + &suffix != whole_with_symbols {
            continue;
        }
        if let Some(consonant) = rest.chars().next() {
            for (rank, surface) in dict.okuri_ari(&yomi, consonant).iter().enumerate() {
                out.push(Candidate::ranked(format!("{surface}{suffix}"), 1, rank));
            }
        }
        for (rank, surface) in dict.okuri_nasi(&yomi).iter().enumerate() {
            out.push(Candidate::ranked(format!("{surface}{suffix}"), 1, rank));
        }
        // カタカナ語に助詞が続く `sukottorandoha` のような入力のための候補。
        if !rest.is_empty() && yomi.chars().count() >= 2 {
            out.push(Candidate::kana(format!("{}{suffix}", hira2kata(&yomi))));
        }
    }

    out.push(Candidate::kana(whole_with_symbols.clone()));
    out.push(Candidate::kana(hira2kata(&whole_with_symbols)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "\
;; okuri-ari entries.
かk /書/掛/
;; okuri-nasi entries.
か /可/
ねこ /猫/
";

    fn setup() -> (KanaTable, Dictionary) {
        (KanaTable::default_table(), Dictionary::parse(FIXTURE))
    }

    fn segs(s: &[&str]) -> Vec<String> {
        s.iter().map(|s| s.to_string()).collect()
    }

    fn surfaces(c: &[Candidate]) -> Vec<&str> {
        c.iter().map(|c| c.surface.as_str()).collect()
    }

    #[test]
    fn 送りなし候補とかな候補を出す() {
        let (t, d) = setup();
        let c = candidates_at(&t, &d, &segs(&["neko"]), 0);
        assert_eq!(surfaces(&c), ["猫", "ねこ", "ネコ"]);
    }

    #[test]
    fn 次のセグメントを送り仮名として送りあり候補を出す() {
        let (t, d) = setup();
        let c = candidates_at(&t, &d, &segs(&["ka", "ku"]), 0);
        assert_eq!(c[0], Candidate::ranked("書く", 2, 0));
        assert_eq!(c[1], Candidate::ranked("掛く", 2, 1));
        assert_eq!(c[2], Candidate::ranked("可", 1, 0));
    }

    #[test]
    fn セグメント内の残りを送り仮名として送りあり候補を出す() {
        let (t, d) = setup();
        let c = candidates_at(&t, &d, &segs(&["kakimasu"]), 0);
        assert!(c.contains(&Candidate::ranked("書きます", 1, 0)));
        assert!(c.contains(&Candidate::ranked("可きます", 1, 0)));
    }

    #[test]
    fn 読みの前方一致で辞書を引き残りをかなにする() {
        let (t, d) = setup();
        let c = candidates_at(&t, &d, &segs(&["nekoha"]), 0);
        assert_eq!(surfaces(&c), ["猫は", "ネコは", "ねこは", "ネコハ"]);
    }

    #[test]
    fn カタカナの前方一致にかなを続けた候補を出す() {
        let (t, d) = setup();
        let c = candidates_at(&t, &d, &segs(&["sukottorandoha"]), 0);
        assert!(c.contains(&Candidate::kana("スコットランドは")));
    }

    #[test]
    fn 記号を含む組はまとめてかなにする() {
        let (t, d) = setup();
        let c = candidates_at(&t, &d, &segs(&["nekoz/"]), 0);
        assert!(c.contains(&Candidate::ranked("猫・", 1, 0)));
        assert!(c.contains(&Candidate::kana("ねこ・")));
    }

    #[test]
    fn 途中の長音記号は読みの一部として扱う() {
        let (t, d) = setup();
        let c = candidates_at(&t, &d, &segs(&["ta-gettonisuru"]), 0);
        assert!(c.contains(&Candidate::kana("ターゲットにする")));
    }

    #[test]
    fn 末尾の記号は読みから外して候補に付け直す() {
        let (t, d) = setup();
        let c = candidates_at(&t, &d, &segs(&["neko."]), 0);
        assert_eq!(surfaces(&c), ["猫。", "ねこ。", "ネコ。"]);
    }

    #[test]
    fn 記号で終わるセグメントは次のセグメントを送り仮名にしない() {
        let (t, d) = setup();
        let c = candidates_at(&t, &d, &segs(&["ka.", "ku"]), 0);
        assert!(c.iter().all(|c| c.span == 1));
    }
}
