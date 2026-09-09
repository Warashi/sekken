//! ローマ字からひらがなへの変換。

/// ローマ字からかなへの変換表。
pub struct KanaTable {
    entries: Vec<(String, String)>,
}

/// 既定の変換表（`kana-table.tsv`）の本文。逆引き表を作る側もこれを読む。
// git 依存として vendor されるときは crate のディレクトリしか写されないので、
// 表の実体は crate の中に置き、share/ からは symlink で指す。
pub const DEFAULT_TABLE_TSV: &str = include_str!("../kana-table.tsv");

impl KanaTable {
    /// 既定の変換表を読み込む。
    pub fn default_table() -> KanaTable {
        Self::parse_tsv(DEFAULT_TABLE_TSV)
    }

    /// TSV（`roman\tkana` 1 行 1 組）から変換表を作る。
    pub fn parse_tsv(tsv: &str) -> KanaTable {
        let entries = tsv
            .lines()
            .filter_map(|line| line.split_once('\t'))
            .map(|(r, k)| (r.to_string(), k.to_string()))
            .collect();
        KanaTable { entries }
    }

    /// ローマ字列をひらがなに変換する。表に無い部分はそのまま残す。
    pub fn roman2kana(&self, roman: &str) -> String {
        let mut out = String::new();
        let mut rest = roman;
        while !rest.is_empty() {
            match self.longest_match(rest) {
                Some((pat, kana)) => {
                    out.push_str(kana);
                    rest = &rest[pat.len()..];
                }
                None => {
                    let c = rest.chars().next().unwrap();
                    out.push(c);
                    rest = &rest[c.len_utf8()..];
                }
            }
        }
        out
    }

    fn longest_match(&self, s: &str) -> Option<(&str, &str)> {
        self.entries
            .iter()
            .filter(|(pat, _)| s.starts_with(pat.as_str()))
            .max_by_key(|(pat, _)| pat.len())
            .map(|(pat, kana)| (pat.as_str(), kana.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 基本的なローマ字をひらがなにする() {
        let t = KanaTable::default_table();
        assert_eq!(t.roman2kana("nihongo"), "にほんご");
        assert_eq!(t.roman2kana("wagahai"), "わがはい");
    }

    #[test]
    fn 促音と撥音を扱う() {
        let t = KanaTable::default_table();
        assert_eq!(t.roman2kana("kitte"), "きって");
        assert_eq!(t.roman2kana("kannji"), "かんじ");
        assert_eq!(t.roman2kana("kanji"), "かんじ");
    }

    #[test]
    fn 記号を全角にする() {
        let t = KanaTable::default_table();
        assert_eq!(t.roman2kana("dearu."), "である。");
    }

    #[test]
    fn 表に無い文字はそのまま残す() {
        let t = KanaTable::default_table();
        assert_eq!(t.roman2kana("q"), "q");
    }
}

/// ひらがなをカタカナにする。ひらがな以外はそのまま。
pub fn hira2kata(s: &str) -> String {
    s.chars()
        .map(|c| match c as u32 {
            0x3041..=0x3096 => char::from_u32(c as u32 + 0x60).unwrap_or(c),
            _ => c,
        })
        .collect()
}

#[cfg(test)]
mod hira2kata_tests {
    #[test]
    fn ひらがなをカタカナにする() {
        assert_eq!(super::hira2kata("ねこ。"), "ネコ。");
    }
}
