//! 語の unigram / bigram の頻度と、そこから導くコスト。
//!
//! 学習では `NgramCounter` が HashMap で数え、`NgramModel` に固めて保存する。
//! 実行時の `NgramModel` はソート済みの配列だけを持ち、ファイルから読んだ
//! 列をそのまま使う。語彙 270 万・bigram 1100 万件を HashMap に挿入し直すと
//! 読み込みに数秒かかるので、二分探索で引ける形で保存しておく。

use std::collections::HashMap;
use std::io::{Read, Write};

use anyhow::{Context as _, Result, bail, ensure};

pub const BOS: u32 = 0;
pub const EOS: u32 = 1;

/// bigram を unigram で平滑化するときの擬似頻度（Dirichlet prior）。
/// 最尤推定では頻度 1 の遷移と頻度 5 の遷移が同じ確率 1 になってしまう。
/// 値は 1 から 1000 まで評価で比べ、20 万記事のモデルで最も良かった 300 にした。
/// 小さいと疎な bigram を信じすぎて、unigram で見れば明らかな誤りを選ぶ。
const PRIOR: f64 = 300.0;
/// 未知語 1 文字あたりの追加コスト。
const UNKNOWN_CHAR_COST: f64 = 4.0;

const MAGIC: &[u8; 4] = b"SKNG";
const FORMAT_VERSION: u32 = 1;

/// 学習中の頻度表。
#[derive(Debug, Default)]
pub struct NgramCounter {
    tokens: Vec<String>,
    vocab: HashMap<String, u32>,
    unigram: Vec<u64>,
    bigram: HashMap<(u32, u32), u64>,
    total: u64,
}

impl NgramCounter {
    pub fn new() -> NgramCounter {
        let mut m = NgramCounter::default();
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

    /// 実行時の形に固める。頻度は u32 に収まる必要がある。
    pub fn freeze(&self) -> Result<NgramModel> {
        let vocab = self.tokens.len();
        let mut text = String::new();
        let mut offsets = Vec::with_capacity(vocab + 1);
        for t in &self.tokens {
            offsets.push(text.len() as u32);
            text.push_str(t);
        }
        offsets.push(text.len() as u32);
        let mut sorted: Vec<u32> = (0..vocab as u32).collect();
        sorted.sort_by(|&a, &b| self.tokens[a as usize].cmp(&self.tokens[b as usize]));
        let unigram = self
            .unigram
            .iter()
            .map(|&c| u32::try_from(c))
            .collect::<Result<Vec<_>, _>>()
            .context("unigram count exceeds u32")?;
        let mut pairs: Vec<(u32, u32, u32)> = Vec::with_capacity(self.bigram.len());
        for (&(prev, next), &c) in &self.bigram {
            let c = u32::try_from(c).context("bigram count exceeds u32")?;
            pairs.push((prev, next, c));
        }
        pairs.sort_unstable();
        let mut row_start = vec![0u32; vocab + 1];
        for &(prev, _, _) in &pairs {
            row_start[prev as usize + 1] += 1;
        }
        for i in 0..vocab {
            row_start[i + 1] += row_start[i];
        }
        Ok(NgramModel {
            text,
            offsets,
            sorted,
            unigram,
            row_start,
            next: pairs.iter().map(|p| p.1).collect(),
            count: pairs.iter().map(|p| p.2).collect(),
            total: self.total,
        })
    }

    pub fn save(&self, w: impl Write) -> Result<()> {
        self.freeze()?.save(w)
    }
}

/// 実行時の頻度表。
#[derive(Debug)]
pub struct NgramModel {
    /// 全語を id 順に繋いだ文字列。語 i は `text[offsets[i]..offsets[i + 1]]`。
    text: String,
    offsets: Vec<u32>,
    /// 語をバイト順に並べた id 列。文字列からの引き当てに使う。
    sorted: Vec<u32>,
    unigram: Vec<u32>,
    /// prev ごとの `next` / `count` の範囲。`row_start[prev]..row_start[prev + 1]`。
    row_start: Vec<u32>,
    /// 各行の中で昇順。
    next: Vec<u32>,
    count: Vec<u32>,
    total: u64,
}

impl NgramModel {
    fn token(&self, id: u32) -> &str {
        &self.text[self.offsets[id as usize] as usize..self.offsets[id as usize + 1] as usize]
    }

    pub fn id(&self, token: &str) -> Option<u32> {
        self.sorted
            .binary_search_by(|&id| self.token(id).as_bytes().cmp(token.as_bytes()))
            .ok()
            .map(|pos| self.sorted[pos])
    }

    pub fn vocab_size(&self) -> usize {
        self.offsets.len() - 1
    }

    pub fn bigram_size(&self) -> usize {
        self.next.len()
    }

    fn bigram(&self, prev: u32, next: u32) -> u32 {
        let row =
            self.row_start[prev as usize] as usize..self.row_start[prev as usize + 1] as usize;
        match self.next[row.clone()].binary_search(&next) {
            Ok(pos) => self.count[row.start + pos],
            Err(_) => 0,
        }
    }

    fn unigram_prob(&self, id: u32) -> f64 {
        (self.unigram[id as usize] as f64 + 1.0) / (self.total as f64 + self.vocab_size() as f64)
    }

    /// `prev` から `next` への遷移コスト（負の対数確率）。
    /// 未知語は `None` で表し、文字数に応じたコストを与える。
    pub fn transition_cost(&self, prev: Option<u32>, next: Option<u32>, next_chars: usize) -> f64 {
        let Some(next) = next else {
            let base = (self.total as f64 + self.vocab_size() as f64).ln();
            return base + UNKNOWN_CHAR_COST * next_chars as f64;
        };
        let uni = self.unigram_prob(next);
        let prob = match prev {
            Some(prev) if self.unigram[prev as usize] > 0 => {
                let joint = self.bigram(prev, next) as f64;
                (joint + PRIOR * uni) / (self.unigram[prev as usize] as f64 + PRIOR)
            }
            _ => uni,
        };
        -prob.ln()
    }

    /// zstd で包んだ固定幅リトルエンディアンの列として書く。
    pub fn save(&self, w: impl Write) -> Result<()> {
        let mut enc = zstd::Encoder::new(w, 9).context("zstd encoder")?;
        enc.write_all(MAGIC)?;
        enc.write_all(&FORMAT_VERSION.to_le_bytes())?;
        enc.write_all(&self.total.to_le_bytes())?;
        write_bytes(&mut enc, self.text.as_bytes())?;
        for section in [
            &self.offsets,
            &self.sorted,
            &self.unigram,
            &self.row_start,
            &self.next,
            &self.count,
        ] {
            write_u32s(&mut enc, section)?;
        }
        enc.finish().context("finish zstd")?;
        Ok(())
    }

    pub fn load(r: impl Read) -> Result<NgramModel> {
        let mut dec = zstd::Decoder::new(r).context("zstd decoder")?;
        let mut bytes = Vec::new();
        dec.read_to_end(&mut bytes).context("read model")?;
        let mut r = bytes.as_slice();
        let mut magic = [0u8; 4];
        if r.read_exact(&mut magic).is_err() || &magic != MAGIC {
            bail!("model file is not in the current format (download the latest release)");
        }
        let version = read_u32(&mut r)?;
        ensure!(
            version == FORMAT_VERSION,
            "model format version {version} is not supported (expected {FORMAT_VERSION})"
        );
        let total = read_u64(&mut r)?;
        let text = String::from_utf8(read_bytes(&mut r)?).context("token text")?;
        let offsets = read_u32s(&mut r)?;
        let sorted = read_u32s(&mut r)?;
        let unigram = read_u32s(&mut r)?;
        let row_start = read_u32s(&mut r)?;
        let next = read_u32s(&mut r)?;
        let count = read_u32s(&mut r)?;
        ensure!(r.is_empty(), "trailing bytes in model file");
        let model = NgramModel {
            text,
            offsets,
            sorted,
            unigram,
            row_start,
            next,
            count,
            total,
        };
        model.validate()?;
        Ok(model)
    }

    /// 添字が範囲内に収まるかを読み込み時に確かめ、以後の引き当てを境界検査に任せる。
    fn validate(&self) -> Result<()> {
        let vocab = self.vocab_size();
        ensure!(vocab >= 2, "vocabulary must contain BOS and EOS");
        ensure!(
            self.offsets.windows(2).all(|w| w[0] <= w[1])
                && self.offsets[vocab] as usize == self.text.len()
                && self
                    .offsets
                    .iter()
                    .all(|&o| self.text.is_char_boundary(o as usize)),
            "token offsets are inconsistent"
        );
        ensure!(
            self.sorted.len() == vocab && self.unigram.len() == vocab,
            "vocabulary sections differ in length"
        );
        ensure!(
            self.sorted.iter().all(|&id| (id as usize) < vocab),
            "sorted id out of range"
        );
        ensure!(
            self.row_start.len() == vocab + 1
                && self.row_start.windows(2).all(|w| w[0] <= w[1])
                && self.row_start[vocab] as usize == self.next.len()
                && self.next.len() == self.count.len(),
            "bigram sections are inconsistent"
        );
        Ok(())
    }
}

fn write_bytes(w: &mut impl Write, bytes: &[u8]) -> Result<()> {
    w.write_all(&(bytes.len() as u64).to_le_bytes())?;
    w.write_all(bytes)?;
    Ok(())
}

fn write_u32s(w: &mut impl Write, values: &[u32]) -> Result<()> {
    w.write_all(&(values.len() as u64).to_le_bytes())?;
    let mut buf = Vec::with_capacity(values.len() * 4);
    for v in values {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    w.write_all(&buf)?;
    Ok(())
}

fn read_u32(r: &mut &[u8]) -> Result<u32> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf).context("truncated model file")?;
    Ok(u32::from_le_bytes(buf))
}

fn read_u64(r: &mut &[u8]) -> Result<u64> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf).context("truncated model file")?;
    Ok(u64::from_le_bytes(buf))
}

fn read_bytes(r: &mut &[u8]) -> Result<Vec<u8>> {
    let len = usize::try_from(read_u64(r)?).context("section too large")?;
    ensure!(r.len() >= len, "truncated model file");
    let (head, tail) = r.split_at(len);
    *r = tail;
    Ok(head.to_vec())
}

fn read_u32s(r: &mut &[u8]) -> Result<Vec<u32>> {
    let len = usize::try_from(read_u64(r)?).context("section too large")?;
    let bytes = len.checked_mul(4).context("section too large")?;
    ensure!(r.len() >= bytes, "truncated model file");
    let (head, tail) = r.split_at(bytes);
    *r = tail;
    Ok(head
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counter() -> NgramCounter {
        let mut m = NgramCounter::new();
        m.add_sentence(["猫", "が", "鳴く"]);
        m.add_sentence(["猫", "が", "眠る"]);
        m.add_sentence(["犬", "は", "吠える"]);
        m
    }

    fn model() -> NgramModel {
        counter().freeze().unwrap()
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
    fn 固めても_id_と頻度は変わらない() {
        let c = counter();
        let m = c.freeze().unwrap();
        assert_eq!(m.id("<s>"), Some(BOS));
        assert_eq!(m.id("</s>"), Some(EOS));
        for (id, t) in c.tokens.iter().enumerate() {
            assert_eq!(m.id(t), Some(id as u32), "{t}");
        }
        assert_eq!(m.id("鳥"), None);
        assert_eq!(m.vocab_size(), c.vocab_size());
        assert_eq!(m.bigram_size(), c.bigram_size());
        for (&(prev, next), &count) in &c.bigram {
            assert_eq!(m.bigram(prev, next) as u64, count);
        }
        assert_eq!(m.bigram(m.id("猫").unwrap(), m.id("は").unwrap()), 0);
    }

    #[test]
    fn 保存して読み直せる() {
        let m = model();
        let mut buf = Vec::new();
        m.save(&mut buf).unwrap();
        let m2 = NgramModel::load(buf.as_slice()).unwrap();
        assert_eq!(m2.vocab_size(), m.vocab_size());
        assert_eq!(m2.bigram_size(), m.bigram_size());
        assert_eq!(
            m2.transition_cost(m2.id("猫"), m2.id("が"), 1),
            m.transition_cost(m.id("猫"), m.id("が"), 1)
        );
        assert_eq!(m2.id("鳴く"), m.id("鳴く"));
    }

    #[test]
    fn 古い形式のファイルは理由を示して拒む() {
        let mut buf = Vec::new();
        let mut enc = zstd::Encoder::new(&mut buf, 1).unwrap();
        enc.write_all(b"\x05\x03<s>\x04</s>").unwrap();
        enc.finish().unwrap();
        let err = NgramModel::load(buf.as_slice()).unwrap_err();
        assert!(err.to_string().contains("current format"), "{err}");
    }

    #[test]
    fn u32_に収まらない頻度は固められない() {
        let mut c = counter();
        c.unigram[BOS as usize] = u64::from(u32::MAX) + 1;
        assert!(c.freeze().is_err());
    }

    #[test]
    fn 少数の_bigram_を刈り込める() {
        let mut m = counter();
        let before = m.bigram_size();
        m.prune_bigram(2);
        assert!(m.bigram_size() < before);
        assert!(m.bigram.contains_key(&(m.vocab["猫"], m.vocab["が"])));
    }
}
