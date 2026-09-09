//! 文字単位の語彙。SKK 辞書の候補が語彙外になりにくいよう、語ではなく文字を単位にする。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub const BOS: u32 = 0;
pub const EOS: u32 = 1;
pub const UNK: u32 = 2;

/// 符号化した 1 系列。`start` より前の正解位置は損失に入れない。
/// 条件付き学習では入力部分（`\t` まで）がそこにあたる。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seq {
    pub ids: Vec<u32>,
    pub start: usize,
}

impl Seq {
    pub fn new(ids: Vec<u32>) -> Seq {
        Seq { ids, start: 0 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Vocab {
    chars: Vec<char>,
    #[serde(skip)]
    index: HashMap<char, u32>,
}

impl Vocab {
    /// 出現回数が `min_count` 以上の文字を語彙にする。順序は回数の多い順。
    pub fn build<'a>(sentences: impl IntoIterator<Item = &'a str>, min_count: usize) -> Vocab {
        let mut counts: HashMap<char, usize> = HashMap::new();
        for s in sentences {
            for c in s.chars() {
                *counts.entry(c).or_default() += 1;
            }
        }
        let mut chars: Vec<(char, usize)> = counts
            .into_iter()
            .filter(|(_, n)| *n >= min_count)
            .collect();
        chars.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        Vocab::from_chars(chars.into_iter().map(|(c, _)| c).collect())
    }

    pub fn from_chars(chars: Vec<char>) -> Vocab {
        let index = chars
            .iter()
            .enumerate()
            .map(|(i, &c)| (c, i as u32 + 3))
            .collect();
        Vocab { chars, index }
    }

    /// 読み込み後に逆引き表を作り直す。
    pub fn rebuild_index(&mut self) {
        self.index = self
            .chars
            .iter()
            .enumerate()
            .map(|(i, &c)| (c, i as u32 + 3))
            .collect();
    }

    /// 特殊トークン 3 つを含めた語彙数。
    pub fn len(&self) -> usize {
        self.chars.len() + 3
    }

    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    pub fn id(&self, c: char) -> u32 {
        self.index.get(&c).copied().unwrap_or(UNK)
    }

    /// 文を BOS と EOS で挟んだ id 列にする。
    pub fn encode(&self, s: &str) -> Vec<u32> {
        std::iter::once(BOS)
            .chain(s.chars().map(|c| self.id(c)))
            .chain(std::iter::once(EOS))
            .collect()
    }

    /// 1 行を符号化する。`\t` があれば「入力 \t 出力」の条件付きの行として、
    /// 損失を出力から数える。`\t` も普通の文字として id を持つ。
    pub fn encode_line(&self, line: &str) -> Seq {
        let start = line.find('\t').map_or(0, |i| line[..i].chars().count() + 1);
        Seq {
            ids: self.encode(line),
            start,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 回数の多い順に_id_を振る() {
        let v = Vocab::build(["猫猫犬", "猫"], 1);
        assert_eq!(v.len(), 5);
        assert_eq!(v.id('猫'), 3);
        assert_eq!(v.id('犬'), 4);
    }

    #[test]
    fn 回数の少ない文字と未知の文字は_unk() {
        let v = Vocab::build(["猫猫犬"], 2);
        assert_eq!(v.id('犬'), UNK);
        assert_eq!(v.id('鳥'), UNK);
    }

    #[test]
    fn 文を_bos_と_eos_で挟んで符号化する() {
        let v = Vocab::build(["猫犬"], 1);
        assert_eq!(v.encode("犬鳥"), [BOS, v.id('犬'), UNK, EOS]);
    }

    #[test]
    fn タブの無い行は損失を先頭から数える() {
        let v = Vocab::build(["猫犬"], 1);
        assert_eq!(v.encode_line("犬"), Seq::new(vec![BOS, v.id('犬'), EOS]));
    }

    #[test]
    fn タブのある行は損失をタブの次から数える() {
        let v = Vocab::build(["猫犬\t"], 1);
        let seq = v.encode_line("犬猫\t猫");
        assert_eq!(
            seq.ids,
            [BOS, v.id('犬'), v.id('猫'), v.id('\t'), v.id('猫'), EOS]
        );
        // targets = ids[1..] で、index 3 が最初の出力「猫」。
        assert_eq!(seq.start, 3);
    }

    #[test]
    fn 保存して読み直すと同じ_id_になる() {
        let v = Vocab::build(["猫犬"], 1);
        let bytes = postcard::to_stdvec(&v).unwrap();
        let mut v2: Vocab = postcard::from_bytes(&bytes).unwrap();
        v2.rebuild_index();
        assert_eq!(v2.id('犬'), v.id('犬'));
    }
}
