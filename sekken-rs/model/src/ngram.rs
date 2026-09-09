//! 語の unigram / bigram の頻度と、そこから導くコスト。

use std::collections::HashMap;
use std::io::{Read, Write};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

pub const BOS: u32 = 0;
pub const EOS: u32 = 1;

/// bigram を unigram で平滑化するときの擬似頻度（Dirichlet prior）。
/// 最尤推定では頻度 1 の遷移と頻度 5 の遷移が同じ確率 1 になってしまう。
/// 値は 1 から 1000 まで評価で比べ、20 万記事のモデルで最も良かった 300 にした。
/// 小さいと疎な bigram を信じすぎて、unigram で見れば明らかな誤りを選ぶ。
const PRIOR: f64 = 300.0;
/// 未知語 1 文字あたりの追加コスト。
const UNKNOWN_CHAR_COST: f64 = 4.0;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct NgramModel {
    tokens: Vec<String>,
    vocab: HashMap<String, u32>,
    unigram: Vec<u64>,
    bigram: HashMap<(u32, u32), u64>,
    total: u64,
}

impl NgramModel {
    pub fn new() -> NgramModel {
        let mut m = NgramModel::default();
        m.intern("<s>");
        m.intern("</s>");
        m
    }

    fn intern(&mut self, token: &str) -> u32 {
        if let Some(&id) = self.vocab.get(token) {
            return id;
        }
        let id = self.tokens.len() as u32;
        self.tokens.push(token.to_string());
        self.vocab.insert(token.to_string(), id);
        self.unigram.push(0);
        id
    }

    pub fn id(&self, token: &str) -> Option<u32> {
        self.vocab.get(token).copied()
    }

    /// 1 文分の語列を数える。文頭・文末の遷移も含める。
    pub fn add_sentence<'a>(&mut self, tokens: impl IntoIterator<Item = &'a str>) {
        let mut prev = BOS;
        self.unigram[BOS as usize] += 1;
        for t in tokens {
            let id = self.intern(t);
            self.unigram[id as usize] += 1;
            self.total += 1;
            *self.bigram.entry((prev, id)).or_default() += 1;
            prev = id;
        }
        *self.bigram.entry((prev, EOS)).or_default() += 1;
        self.unigram[EOS as usize] += 1;
        self.total += 1;
    }

    /// 出現回数が `min_count` 未満の bigram を捨てる。
    pub fn prune_bigram(&mut self, min_count: u64) {
        self.bigram.retain(|_, c| *c >= min_count);
    }

    pub fn vocab_size(&self) -> usize {
        self.tokens.len()
    }

    pub fn bigram_size(&self) -> usize {
        self.bigram.len()
    }

    fn unigram_prob(&self, id: u32) -> f64 {
        (self.unigram[id as usize] as f64 + 1.0) / (self.total as f64 + self.tokens.len() as f64)
    }

    /// `prev` から `next` への遷移コスト（負の対数確率）。
    /// 未知語は `None` で表し、文字数に応じたコストを与える。
    pub fn transition_cost(&self, prev: Option<u32>, next: Option<u32>, next_chars: usize) -> f64 {
        let Some(next) = next else {
            let base = (self.total as f64 + self.tokens.len() as f64).ln();
            return base + UNKNOWN_CHAR_COST * next_chars as f64;
        };
        let uni = self.unigram_prob(next);
        let prob = match prev {
            Some(prev) if self.unigram[prev as usize] > 0 => {
                let joint = self.bigram.get(&(prev, next)).copied().unwrap_or(0) as f64;
                (joint + PRIOR * uni) / (self.unigram[prev as usize] as f64 + PRIOR)
            }
            _ => uni,
        };
        -prob.ln()
    }

    pub fn save(&self, w: impl Write) -> Result<()> {
        let bytes = postcard::to_stdvec(self).context("serialize model")?;
        let mut enc = zstd::Encoder::new(w, 9).context("zstd encoder")?;
        enc.write_all(&bytes).context("write model")?;
        enc.finish().context("finish zstd")?;
        Ok(())
    }

    pub fn load(r: impl Read) -> Result<NgramModel> {
        let mut dec = zstd::Decoder::new(r).context("zstd decoder")?;
        let mut bytes = Vec::new();
        dec.read_to_end(&mut bytes).context("read model")?;
        postcard::from_bytes(&bytes).context("deserialize model")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> NgramModel {
        let mut m = NgramModel::new();
        m.add_sentence(["猫", "が", "鳴く"]);
        m.add_sentence(["猫", "が", "眠る"]);
        m.add_sentence(["犬", "は", "吠える"]);
        m
    }

    #[test]
    fn よく出る語の方がコストが低い() {
        let m = model();
        let neko = m.id("猫");
        let inu = m.id("犬");
        assert!(m.transition_cost(None, neko, 1) < m.transition_cost(None, inu, 1));
    }

    #[test]
    fn 学習した遷移は未学習の遷移よりコストが低い() {
        let m = model();
        let neko = m.id("猫");
        let ga = m.id("が");
        let ha = m.id("は");
        assert!(m.transition_cost(neko, ga, 1) < m.transition_cost(neko, ha, 1));
    }

    #[test]
    fn 未知語は長いほどコストが高い() {
        let m = model();
        assert!(m.transition_cost(None, None, 1) < m.transition_cost(None, None, 3));
        assert!(m.transition_cost(None, m.id("犬"), 1) < m.transition_cost(None, None, 1));
    }

    #[test]
    fn 保存して読み直せる() {
        let m = model();
        let mut buf = Vec::new();
        m.save(&mut buf).unwrap();
        let m2 = NgramModel::load(buf.as_slice()).unwrap();
        assert_eq!(m2.vocab_size(), m.vocab_size());
        assert_eq!(
            m2.transition_cost(m2.id("猫"), m2.id("が"), 1),
            m.transition_cost(m.id("猫"), m.id("が"), 1)
        );
    }

    #[test]
    fn 少数の_bigram_を刈り込める() {
        let mut m = model();
        let before = m.bigram_size();
        m.prune_bigram(2);
        assert!(m.bigram_size() < before);
        assert!(
            m.bigram
                .contains_key(&(m.id("猫").unwrap(), m.id("が").unwrap()))
        );
    }
}
