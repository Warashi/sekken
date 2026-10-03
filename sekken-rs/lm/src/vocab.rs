//! 文字単位の語彙。SKK 辞書の候補が語彙外になりにくいよう、語ではなく文字を単位にする。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::interleave::{Sym, loss_mask};

pub const BOS: u32 = 0;
pub const EOS: u32 = 1;
pub const UNK: u32 = 2;
/// 区間の読みの始まり（`Sym::Reading`）の id。`Layout::Separate` のとき。
pub const READING: u32 = 3;
/// 出力の始まり（`Sym::Output`）の id。`Layout::Separate` のとき。
pub const OUTPUT: u32 = 4;

/// 符号化した 1 系列。`mask[i]` は `ids[i + 1]` を正解として損失に入れるか。
/// 条件付き学習では読みの字がそこから外れる。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seq {
    pub ids: Vec<u32>,
    pub mask: Vec<bool>,
}

impl Seq {
    /// 全部の正解位置を損失に入れる系列。
    pub fn new(ids: Vec<u32>) -> Seq {
        let mask = vec![true; ids.len().saturating_sub(1)];
        Seq { ids, mask }
    }

    /// 先頭 `len` 個の id に切り詰める。
    pub fn truncate(&mut self, len: usize) {
        self.ids.truncate(len);
        self.mask.truncate(len.saturating_sub(1));
    }

    /// 損失に入る正解位置の数。
    pub fn loss_count(&self) -> usize {
        self.mask.iter().filter(|&&m| m).count()
    }
}

/// 区切りの id の置き方。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Layout {
    /// 区切りを字と別の id（`READING`、`OUTPUT`）に置き、字は 5 から振る。
    #[default]
    Separate,
    /// 2026-10 までの形式。区切りは語彙の字 U+001E とタブが兼ね、字は 3 から振る。配布中の旧モデルを
    /// 読むためだけに残し、新しいモデルを配布したら消す。本文のその 2 字は区切りと取り違えないよう UNK にする。
    Legacy,
}

impl Layout {
    /// 字より前に置く特殊な id の数。
    fn specials(self) -> u32 {
        match self {
            Layout::Separate => 5,
            Layout::Legacy => 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Vocab {
    chars: Vec<char>,
    #[serde(skip)]
    index: HashMap<char, u32>,
    /// ファイルの本体には書かず、モデルファイルの追加の欄で持つ（`file.rs`）。
    #[serde(skip)]
    layout: Layout,
    /// `Sym::Reading` と `Sym::Output` の id。
    #[serde(skip)]
    separators: (u32, u32),
}

impl Vocab {
    /// 出現回数が `min_count` 以上の文字を語彙にする。順序は回数の多い順。
    pub fn build<'a>(texts: impl IntoIterator<Item = &'a str>, min_count: usize) -> Vocab {
        let mut counts: HashMap<char, usize> = HashMap::new();
        for s in texts {
            for c in s.chars() {
                *counts.entry(c).or_default() += 1;
            }
        }
        Vocab::from_counts(counts, min_count)
    }

    /// 字の出現回数から語彙を作る。回数が `min_count` 以上の字を、回数の多い順に並べる。
    pub fn from_counts(counts: HashMap<char, usize>, min_count: usize) -> Vocab {
        let mut chars: Vec<(char, usize)> = counts
            .into_iter()
            .filter(|(_, n)| *n >= min_count)
            .collect();
        chars.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        Vocab::from_chars(chars.into_iter().map(|(c, _)| c).collect())
    }

    pub fn from_chars(chars: Vec<char>) -> Vocab {
        let mut vocab = Vocab {
            chars,
            index: HashMap::new(),
            layout: Layout::Separate,
            separators: (READING, OUTPUT),
        };
        vocab.rebuild_index();
        vocab
    }

    /// 区切りの置き方を決め、逆引き表を作り直す。モデルファイルを読んだ後に呼ぶ。
    pub fn set_layout(&mut self, layout: Layout) {
        self.layout = layout;
        self.rebuild_index();
    }

    pub fn layout(&self) -> Layout {
        self.layout
    }

    /// 読み込み後に逆引き表を作り直す。
    pub fn rebuild_index(&mut self) {
        let offset = self.layout.specials();
        let ids = self
            .chars
            .iter()
            .enumerate()
            .map(|(i, &c)| (c, i as u32 + offset));
        match self.layout {
            Layout::Separate => {
                self.index = ids.collect();
                self.separators = (READING, OUTPUT);
            }
            Layout::Legacy => {
                let all: HashMap<char, u32> = ids.collect();
                let sep = |c: char| all.get(&c).copied().unwrap_or(UNK);
                self.separators = (sep('\u{1e}'), sep('\t'));
                self.index = all
                    .into_iter()
                    .filter(|(c, _)| !matches!(c, '\u{1e}' | '\t'))
                    .collect();
            }
        }
    }

    /// 特殊な id を含めた語彙数。
    pub fn len(&self) -> usize {
        self.chars.len() + self.layout.specials() as usize
    }

    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    pub fn id(&self, c: char) -> u32 {
        self.index.get(&c).copied().unwrap_or(UNK)
    }

    /// 行の要素の id。
    pub fn sym(&self, s: Sym) -> u32 {
        match s {
            Sym::Reading => self.separators.0,
            Sym::Output => self.separators.1,
            Sym::Char(c) => self.id(c),
        }
    }

    /// 行を BOS と EOS で挟んだ id 列にする。
    pub fn encode(&self, line: &[Sym]) -> Vec<u32> {
        std::iter::once(BOS)
            .chain(line.iter().map(|&s| self.sym(s)))
            .chain(std::iter::once(EOS))
            .collect()
    }

    /// 1 行を符号化する。損失に入る位置は行の形（`interleave::loss_mask`）で決まり、
    /// 末尾の EOS は常に入る。
    pub fn encode_line(&self, line: &[Sym]) -> Seq {
        let mut mask = loss_mask(line);
        mask.push(true);
        Seq {
            ids: self.encode(line),
            mask,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interleave::{plain, prefixed, tests::line};

    #[test]
    fn 回数の多い順に区切りの後ろから_id_を振る() {
        let v = Vocab::build(["猫猫犬", "猫"], 1);
        assert_eq!(v.len(), 7);
        assert_eq!(v.id('猫'), 5);
        assert_eq!(v.id('犬'), 6);
    }

    #[test]
    fn 回数の少ない文字と未知の文字は_unk() {
        let v = Vocab::build(["猫猫犬"], 2);
        assert_eq!(v.id('犬'), UNK);
        assert_eq!(v.id('鳥'), UNK);
    }

    #[test]
    fn 行を_bos_と_eos_で挟んで符号化する() {
        let v = Vocab::build(["猫犬"], 1);
        assert_eq!(v.encode(&plain("犬鳥")), [BOS, v.id('犬'), UNK, EOS]);
    }

    #[test]
    fn 区切りの無い行は全部を損失に入れる() {
        let v = Vocab::build(["猫犬"], 1);
        assert_eq!(
            v.encode_line(&plain("犬")),
            Seq::new(vec![BOS, v.id('犬'), EOS])
        );
    }

    #[test]
    fn 読みを前置した行は損失を出力の始まりの次から数える() {
        let v = Vocab::build(["猫犬"], 1);
        let seq = v.encode_line(&prefixed("犬猫", "猫"));
        assert_eq!(
            seq.ids,
            [BOS, v.id('犬'), v.id('猫'), OUTPUT, v.id('猫'), EOS]
        );
        // targets = ids[1..] で、index 3 が最初の出力「猫」、index 4 が EOS。
        assert_eq!(seq.mask, [false, false, false, true, true]);
        assert_eq!(seq.loss_count(), 2);
    }

    #[test]
    fn 交互の行は出力の字と区間の終わりと_eos_を損失に入れる() {
        let v = Vocab::build(["猫犬"], 1);
        let seq = v.encode_line(&line("\u{1e}犬\t猫\u{1e}犬\t犬"));
        assert_eq!(seq.ids[1], READING);
        assert_eq!(seq.ids.len(), 10);
        assert_eq!(
            seq.mask,
            [true, false, false, true, true, false, false, true, true]
        );
    }

    #[test]
    fn 本文のタブは区切りと別の_id_になる() {
        let v = Vocab::build(["猫\t"], 1);
        assert_ne!(v.sym(Sym::Char('\t')), v.sym(Sym::Output));
    }

    #[test]
    fn 旧形式は区切りの字の_id_を区切りに使い本文のその字は_unk() {
        let mut v = Vocab::from_chars(vec!['猫', '\t', '\u{1e}']);
        v.set_layout(Layout::Legacy);
        assert_eq!(v.len(), 6);
        assert_eq!(v.id('猫'), 3);
        assert_eq!(v.sym(Sym::Output), 4);
        assert_eq!(v.sym(Sym::Reading), 5);
        assert_eq!(v.sym(Sym::Char('\t')), UNK);
    }

    #[test]
    fn 切り詰めると損失の印も揃って短くなる() {
        let mut seq = Seq::new(vec![BOS, 3, 4, 5, EOS]);
        seq.truncate(3);
        assert_eq!(seq.ids, [BOS, 3, 4]);
        assert_eq!(seq.mask, [true, true]);
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
