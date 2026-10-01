//! ローマ字からひらがなへの変換。

use std::collections::BTreeMap;

use daachorse::{DoubleArrayAhoCorasick, DoubleArrayAhoCorasickBuilder, MatchKind};

/// ローマ字からかなへの変換表。
pub struct KanaTable {
    matcher: Option<DoubleArrayAhoCorasick<u32>>,
    outputs: Vec<Output>,
    linear_entries: Vec<(String, Output)>,
    /// 2 文字以上のキー。境界の文字がキーを完成させるかを引く。
    long_keys: Vec<String>,
}

/// キーに当たったときに出すかな。
struct Output {
    kana: String,
    /// キーの末尾のうち、次のキーの先頭として読み直すバイト数（`kk` の `k` なら 1）。
    reread: usize,
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
    /// 省略できる 3 列目はキーの末尾のうち次のキーの先頭として読み直す文字で
    /// （`kk\tっ\tk`）、キーの末尾でなければその行を使わない。
    pub fn parse_tsv(tsv: &str) -> KanaTable {
        let mut entries = BTreeMap::new();
        for (roman, rest) in tsv.lines().filter_map(|line| line.split_once('\t')) {
            let (kana, reread) = rest.split_once('\t').unwrap_or((rest, ""));
            // 読み直す文字がキー全体だと位置が進まない。
            if !roman.is_empty() && roman.ends_with(reread) && roman.len() > reread.len() {
                let output = Output {
                    kana: kana.to_string(),
                    reread: reread.len(),
                };
                entries.insert(roman.to_string(), output);
            }
        }
        let entries: Vec<_> = entries.into_iter().collect();
        let long_keys = entries
            .iter()
            .filter(|(roman, _)| roman.chars().count() > 1)
            .map(|(roman, _)| roman.clone())
            .collect();
        // Double-array は固定長の状態ブロックを持つため、小さい独自表では線形表の方が軽い。
        if entries.len() < 32 {
            return KanaTable {
                matcher: None,
                outputs: Vec::new(),
                linear_entries: entries,
                long_keys,
            };
        }
        let matcher = Some(
            DoubleArrayAhoCorasickBuilder::new()
                .match_kind(MatchKind::LeftmostLongest)
                .use_prefilter(false)
                .build(entries.iter().map(|(roman, _)| roman))
                .expect("deduplicated non-empty kana patterns form a valid automaton"),
        );
        let outputs = entries.into_iter().map(|(_, kana)| kana).collect();
        KanaTable {
            matcher,
            outputs,
            linear_entries: Vec::new(),
            long_keys,
        }
    }

    /// `before` の末尾と `after` の先頭にまたがる 2 文字以上のキーがあるか。
    /// 境目が最長一致のキーの途中に当たるかを決める。境界の文字を直前までと
    /// 合わせてキーとして読むか（`z` の後の `/` は境界でなく ・ のキー）に使う。
    /// エディタ側（lisp/sekken-kana.el）も同じ述語を持つ。
    pub fn key_across(&self, before: &str, after: &str) -> bool {
        !before.is_empty()
            && !after.is_empty()
            && self.long_keys.iter().any(|key| {
                key.char_indices().skip(1).any(|(split, _)| {
                    before.ends_with(&key[..split]) && after.starts_with(&key[split..])
                })
            })
    }

    /// ローマ字列をひらがなに変換する。表に無い部分はそのまま残す。
    pub fn roman2kana(&self, roman: &str) -> String {
        let Some(matcher) = &self.matcher else {
            return roman2kana_linear(&self.linear_entries, roman);
        };
        let mut out = String::with_capacity(roman.len());
        let mut end = 0;
        // 読み直す文字の分だけ戻るので、イテレータを続けずに毎回その位置から探す。
        while let Some(matched) = matcher.leftmost_find_iter(&roman[end..]).next() {
            let output = &self.outputs[matched.value() as usize];
            out.push_str(&roman[end..end + matched.start()]);
            out.push_str(&output.kana);
            end += matched.end() - output.reread;
        }
        out.push_str(&roman[end..]);
        out
    }
}

fn roman2kana_linear(entries: &[(String, Output)], roman: &str) -> String {
    let mut out = String::new();
    let mut rest = roman;
    while !rest.is_empty() {
        if let Some((pattern, output)) = entries
            .iter()
            .filter(|(pattern, _)| rest.starts_with(pattern))
            .max_by_key(|(pattern, _)| pattern.len())
        {
            out.push_str(&output.kana);
            rest = &rest[pattern.len() - output.reread..];
        } else {
            let c = rest.chars().next().unwrap();
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
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

    #[test]
    fn 空の変換表では入力をそのまま残す() {
        let t = KanaTable::parse_tsv("");
        assert_eq!(t.roman2kana("nihongo"), "nihongo");
    }

    #[test]
    fn 重複するパターンでは後の定義を使う() {
        let t = KanaTable::parse_tsv("a\tあ\na\tア\n");
        assert_eq!(t.roman2kana("a"), "ア");
    }

    #[test]
    fn 三列目の文字はキーの末尾として読み直す() {
        let linear = KanaTable::parse_tsv("ka\tか\nkk\tっ\tk\n");
        let defaults: String = DEFAULT_TABLE_TSV
            .lines()
            .filter(|line| !line.starts_with("k\t"))
            .map(|line| format!("{line}\n"))
            .collect();
        let automaton = KanaTable::parse_tsv(&format!("{defaults}kk\tっ\tk\n"));
        assert!(automaton.matcher.is_some());
        for t in [linear, automaton] {
            assert_eq!(t.roman2kana("kk"), "っk");
            assert_eq!(t.roman2kana("kka"), "っか");
            assert_eq!(t.roman2kana("kkka"), "っっか");
        }
    }

    #[test]
    fn キーの末尾でない三列目の行は使わない() {
        let t = KanaTable::parse_tsv("ka\tか\nkk\tっ\ta\n");
        assert_eq!(t.roman2kana("kka"), "kか");
    }

    #[test]
    fn 境目にまたがるキーがあるかを引く() {
        let table = KanaTable::default_table();
        assert!(table.key_across("nekoz", "/"));
        assert!(table.key_across("z", "/"));
        assert!(!table.key_across("neko", "/"));
        assert!(!table.key_across("", "/"));
        // 1 文字のキーは境目をまたがない。
        assert!(!table.key_across("neko", "-"));
        // 3 文字のキーはどの位置で切れても当たる。
        assert!(table.key_across("nekok", "ya"));
        assert!(table.key_across("nekoky", "a"));
        assert!(!table.key_across("nekok", "ka"));
    }

    /// エディタ側の変換と同じ結果になることを確かめる共通の fixture。
    /// エディタが送るかなが変換結果を決めるので、Emacs 側のテストも同じ表を読む。
    #[test]
    fn 共通の_fixture_と一致する() {
        let t = KanaTable::default_table();
        for (roman, kana) in include_str!("../kana-cases.tsv")
            .lines()
            .filter_map(|line| line.split_once('\t'))
        {
            assert_eq!(t.roman2kana(roman), kana, "{roman}");
        }
    }

    #[test]
    fn 既定の変換表にはオートマトンを使う() {
        assert!(KanaTable::default_table().matcher.is_some());
    }
}

/// ひらがなをカタカナにする。ひらがな以外はそのまま。
pub fn hira2kata(s: &str) -> String {
    s.chars().map(hira2kata_char).collect()
}

/// ひらがな 1 字をカタカナにする。ひらがな以外はそのまま。
pub fn hira2kata_char(c: char) -> char {
    match c as u32 {
        0x3041..=0x3096 => char::from_u32(c as u32 + 0x60).unwrap_or(c),
        _ => c,
    }
}

#[cfg(test)]
mod hira2kata_tests {
    #[test]
    fn ひらがなをカタカナにする() {
        assert_eq!(super::hira2kata("ねこ。"), "ネコ。");
    }
}
