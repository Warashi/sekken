//! 語の unigram / bigram の頻度と、そこから導くコスト。
//!
//! 学習では `Counts`（学習側が数えた頻度）を `NgramModel::build` で実行時の形に
//! 固めて保存する。実行時の `NgramModel` はソート済みの配列だけを持ち、
//! ファイルから読んだ列をそのまま使う。語彙 270 万・bigram 1100 万件を
//! HashMap に挿入し直すと読み込みに数秒かかるので、二分探索で引ける形で
//! 保存しておく。
//!
//! 遷移確率は平滑化の種類によらず `num(prev, next) + λ(prev) · unigram(next)`
//! の形に固めてある。`num` は bigram ごと、`λ` と `unigram` は語ごとの値。

use std::collections::HashMap;
use std::io::{Read, Write};

use anyhow::{Context as _, Result, bail, ensure};

pub const BOS: u32 = 0;
pub const EOS: u32 = 1;

const MAGIC: &[u8; 4] = b"SKNG";
/// 4 で品詞クラスの表と trigram の区画を落とした。
const FORMAT_VERSION: u32 = 4;

/// 学習で数えた頻度。`NgramModel::build` の入力。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Counts {
    pub tokens: Vec<String>,
    pub unigram: Vec<u64>,
    pub total: u64,
    /// 刈り込み前の、語ごとの異なり後続数。
    pub n_succ: Vec<u32>,
    /// 刈り込み前の、語ごとの異なり先行数。
    pub n_pred: Vec<u32>,
    /// 刈り込み前の bigram の種類数のうち、頻度が 1 のものと 2 のもの。
    pub n1: u64,
    pub n2: u64,
    /// 刈り込み後の `(prev, next, count)`。昇順。
    pub bigram: Vec<(u32, u32, u64)>,
}

/// bigram を unigram で平滑化する方法。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Smoothing {
    /// `(c + prior · unigram) / (c(prev) + prior)`。
    Dirichlet { prior: f64 },
    /// 補間 Kneser-Ney。`discount` が `None` なら `n1 / (n1 + 2 n2)` で見積もる。
    KneserNey { discount: Option<f64> },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuildParams {
    pub smoothing: Smoothing,
    /// 未知語 1 文字あたりの追加コスト。
    pub unknown_char_cost: f64,
    /// 組むときにこの頻度未満の bigram を落とす。計数時の刈り込みに重ねて掛かる。
    /// モデルの大きさをここで調整する。
    pub min_bigram: u64,
}

impl Default for BuildParams {
    /// 20 万記事のモデルで評価して選んだ値。PRIOR は 1 から 1000 まで比べて 300、
    /// 小さいと疎な bigram を信じすぎて unigram で見れば明らかな誤りを選ぶ。
    fn default() -> BuildParams {
        BuildParams {
            smoothing: Smoothing::Dirichlet { prior: 300.0 },
            unknown_char_cost: 4.0,
            min_bigram: 1,
        }
    }
}

/// 学習中の頻度表。小さなコーパスを RAM で数えるためのもので、大きな
/// コーパスは学習側がディスクへ退避しながら数える。
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

    pub fn vocab_size(&self) -> usize {
        self.tokens.len()
    }

    /// 頻度 `min_count` 未満の bigram を刈り込んで `Counts` にする。
    pub fn counts(&self, min_count: u64) -> Counts {
        let vocab = self.tokens.len();
        let mut n_succ = vec![0u32; vocab];
        let mut n_pred = vec![0u32; vocab];
        let (mut n1, mut n2) = (0, 0);
        let mut bigram = Vec::new();
        for (&(prev, next), &c) in &self.bigram {
            n_succ[prev as usize] += 1;
            n_pred[next as usize] += 1;
            match c {
                1 => n1 += 1,
                2 => n2 += 1,
                _ => {}
            }
            if c >= min_count {
                bigram.push((prev, next, c));
            }
        }
        bigram.sort_unstable();
        Counts {
            tokens: self.tokens.clone(),
            unigram: self.unigram.clone(),
            total: self.total,
            n_succ,
            n_pred,
            n1,
            n2,
            bigram,
        }
    }

    /// 既定の平滑化で実行時の形に固める。
    pub fn freeze(&self) -> Result<NgramModel> {
        NgramModel::build(&self.counts(1), &BuildParams::default())
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
    /// prev ごとの `next` / `num` の範囲。`row_start[prev]..row_start[prev + 1]`。
    row_start: Vec<u32>,
    /// 各行の中で昇順。
    next: Vec<u32>,
    /// bigram ごとの、unigram に頼らない確率の分子。
    num: Vec<f32>,
    /// 語ごとの、unigram に掛ける重み。
    lambda: Vec<f32>,
    /// 語ごとの unigram 確率。
    unigram: Vec<f32>,
    /// 未知語のコストの底。
    unknown_base: f64,
    unknown_char_cost: f64,
}

impl NgramModel {
    /// 頻度から実行時の形に組む。
    pub fn build(counts: &Counts, params: &BuildParams) -> Result<NgramModel> {
        let vocab = counts.tokens.len();
        ensure!(vocab >= 2, "vocabulary must contain BOS and EOS");
        ensure!(
            counts.unigram.len() == vocab
                && counts.n_succ.len() == vocab
                && counts.n_pred.len() == vocab,
            "per-word sections differ from vocabulary in length"
        );
        let mut text = String::new();
        let mut offsets = Vec::with_capacity(vocab + 1);
        for t in &counts.tokens {
            offsets.push(text.len() as u32);
            text.push_str(t);
        }
        offsets.push(text.len() as u32);
        let mut sorted: Vec<u32> = (0..vocab as u32).collect();
        sorted.sort_by(|&a, &b| counts.tokens[a as usize].cmp(&counts.tokens[b as usize]));

        let unigram: Vec<f32> = match params.smoothing {
            Smoothing::Dirichlet { .. } => counts
                .unigram
                .iter()
                .map(|&c| ((c as f64 + 1.0) / (counts.total as f64 + vocab as f64)) as f32)
                .collect(),
            Smoothing::KneserNey { .. } => {
                let types: u64 = counts.n_pred.iter().map(|&n| n as u64).sum();
                ensure!(types > 0, "no bigram to estimate continuation probability");
                // 一度も後続に現れない語（BOS）にも 0 でない確率を残す。
                counts
                    .n_pred
                    .iter()
                    .map(|&n| ((n as f64).max(0.5) / types as f64) as f32)
                    .collect()
            }
        };

        let discount = match params.smoothing {
            Smoothing::Dirichlet { prior } => {
                ensure!(prior > 0.0, "prior must be positive");
                0.0
            }
            Smoothing::KneserNey { discount } => {
                let d = discount.unwrap_or_else(|| {
                    counts.n1 as f64 / (counts.n1 as f64 + 2.0 * counts.n2 as f64)
                });
                ensure!(
                    d.is_finite() && d > 0.0 && d < 1.0,
                    "discount must be in (0, 1)"
                );
                d
            }
        };
        let mut row_start = vec![0u32; vocab + 1];
        let mut next = Vec::with_capacity(counts.bigram.len());
        let mut num = Vec::with_capacity(counts.bigram.len());
        // λ = 1 − Σ_kept num の分だけ unigram に回す。刈り込んだ対の質量も unigram に行く。
        let mut kept = vec![0f64; vocab];
        let mut last = None;
        for &(prev, nxt, c) in &counts.bigram {
            ensure!(
                last.is_none_or(|l| l < (prev, nxt)),
                "bigram counts must be sorted and unique"
            );
            ensure!(
                (prev as usize) < vocab && (nxt as usize) < vocab,
                "bigram id out of range"
            );
            last = Some((prev, nxt));
            if c < params.min_bigram {
                continue;
            }
            let history = counts.unigram[prev as usize] as f64;
            ensure!(history >= c as f64, "bigram count exceeds unigram count");
            let p = match params.smoothing {
                Smoothing::Dirichlet { prior } => c as f64 / (history + prior),
                Smoothing::KneserNey { .. } => (c as f64 - discount).max(0.0) / history,
            };
            kept[prev as usize] += p;
            row_start[prev as usize + 1] += 1;
            next.push(nxt);
            num.push(p as f32);
        }
        for i in 0..vocab {
            row_start[i + 1] += row_start[i];
        }
        let lambda: Vec<f32> = (0..vocab)
            .map(|w| lambda_of(params.smoothing, counts.unigram[w] as f64, kept[w]))
            .collect();
        Ok(NgramModel {
            text,
            offsets,
            sorted,
            row_start,
            next,
            num,
            lambda,
            unigram,
            unknown_base: (counts.total as f64 + vocab as f64).ln(),
            unknown_char_cost: params.unknown_char_cost,
        })
    }

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

    /// bigram の確率 `num(prev, next) + λ(prev) · unigram(next)`。
    fn bigram_prob(&self, prev: u32, next: u32) -> f64 {
        let row =
            self.row_start[prev as usize] as usize..self.row_start[prev as usize + 1] as usize;
        let num = match self.next[row.clone()].binary_search(&next) {
            Ok(pos) => self.num[row.start + pos] as f64,
            Err(_) => 0.0,
        };
        num + self.lambda[prev as usize] as f64 * self.unigram[next as usize] as f64
    }

    /// `prev` から `next` への遷移コスト（負の対数確率）。
    /// 未知語は `None` で表し、文字数に応じたコストを与える。
    pub fn transition_cost(&self, prev: Option<u32>, next: Option<u32>, next_chars: usize) -> f64 {
        let Some(next) = next else {
            return self.unknown_base + self.unknown_char_cost * next_chars as f64;
        };
        let Some(prev) = prev else {
            return -(self.unigram[next as usize] as f64).ln();
        };
        -self.bigram_prob(prev, next).ln()
    }

    /// zstd で包んだ固定幅リトルエンディアンの列として書く。
    pub fn save(&self, w: impl Write) -> Result<()> {
        let mut enc = zstd::Encoder::new(w, 9).context("zstd encoder")?;
        enc.write_all(MAGIC)?;
        enc.write_all(&FORMAT_VERSION.to_le_bytes())?;
        enc.write_all(&self.unknown_base.to_le_bytes())?;
        enc.write_all(&self.unknown_char_cost.to_le_bytes())?;
        write_bytes(&mut enc, self.text.as_bytes())?;
        for section in [&self.offsets, &self.sorted, &self.row_start, &self.next] {
            write_u32s(&mut enc, section)?;
        }
        for section in [&self.num, &self.lambda, &self.unigram] {
            write_f32s(&mut enc, section)?;
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
        let unknown_base = f64::from_bits(read_u64(&mut r)?);
        let unknown_char_cost = f64::from_bits(read_u64(&mut r)?);
        let text = String::from_utf8(read_bytes(&mut r)?).context("token text")?;
        let offsets = read_u32s(&mut r)?;
        let sorted = read_u32s(&mut r)?;
        let row_start = read_u32s(&mut r)?;
        let next = read_u32s(&mut r)?;
        let num = read_f32s(&mut r)?;
        let lambda = read_f32s(&mut r)?;
        let unigram = read_f32s(&mut r)?;
        ensure!(r.is_empty(), "trailing bytes in model file");
        let model = NgramModel {
            text,
            offsets,
            sorted,
            row_start,
            next,
            num,
            lambda,
            unigram,
            unknown_base,
            unknown_char_cost,
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
            self.sorted.len() == vocab && self.lambda.len() == vocab && self.unigram.len() == vocab,
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
                && self.next.len() == self.num.len()
                && self.next.iter().all(|&id| (id as usize) < vocab),
            "bigram sections are inconsistent"
        );
        Ok(())
    }
}

/// unigram に掛ける重み。Dirichlet は擬似頻度の割合、Kneser-Ney は残した
/// 分子の残り。履歴が一度も現れなければ全部を unigram に回す。
fn lambda_of(smoothing: Smoothing, history: f64, kept: f64) -> f32 {
    let l = match smoothing {
        Smoothing::Dirichlet { prior } => prior / (history + prior),
        Smoothing::KneserNey { .. } if history > 0.0 => 1.0 - kept,
        Smoothing::KneserNey { .. } => 1.0,
    };
    l.clamp(0.0, 1.0) as f32
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

fn write_f32s(w: &mut impl Write, values: &[f32]) -> Result<()> {
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

fn read_chunks(r: &mut &[u8]) -> Result<Vec<[u8; 4]>> {
    let len = usize::try_from(read_u64(r)?).context("section too large")?;
    let bytes = len.checked_mul(4).context("section too large")?;
    ensure!(r.len() >= bytes, "truncated model file");
    let (head, tail) = r.split_at(bytes);
    *r = tail;
    Ok(head.as_chunks::<4>().0.to_vec())
}

fn read_u32s(r: &mut &[u8]) -> Result<Vec<u32>> {
    Ok(read_chunks(r)?
        .into_iter()
        .map(u32::from_le_bytes)
        .collect())
}

fn read_f32s(r: &mut &[u8]) -> Result<Vec<f32>> {
    Ok(read_chunks(r)?
        .into_iter()
        .map(f32::from_le_bytes)
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
        m.add_sentence(["猫", "が", "鳴く"]);
        m
    }

    fn all_params() -> Vec<BuildParams> {
        [
            Smoothing::Dirichlet { prior: 3.0 },
            Smoothing::KneserNey { discount: None },
            Smoothing::KneserNey {
                discount: Some(0.5),
            },
        ]
        .into_iter()
        .map(|smoothing| BuildParams {
            smoothing,
            ..BuildParams::default()
        })
        .collect()
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
        for p in all_params() {
            let m = NgramModel::build(&counter().counts(1), &p).unwrap();
            let neko = m.id("猫");
            let ga = m.id("が");
            let ha = m.id("は");
            assert!(
                m.transition_cost(neko, ga, 1) < m.transition_cost(neko, ha, 1),
                "{p:?}"
            );
        }
    }

    #[test]
    fn 未知語は長いほどコストが高い() {
        let m = model();
        assert!(m.transition_cost(None, None, 1) < m.transition_cost(None, None, 3));
        assert!(m.transition_cost(None, m.id("犬"), 1) < m.transition_cost(None, None, 1));
    }

    #[test]
    fn 既定の平滑化は旧来の_dirichlet_の式と一致する() {
        let c = counter();
        let counts = c.counts(1);
        let m = c.freeze().unwrap();
        let (neko, ga) = (c.vocab["猫"], c.vocab["が"]);
        let vocab = counts.tokens.len() as f64;
        let uni = (counts.unigram[ga as usize] as f64 + 1.0) / (counts.total as f64 + vocab);
        let joint = c.bigram[&(neko, ga)] as f64;
        let expected =
            -((joint + 300.0 * uni) / (counts.unigram[neko as usize] as f64 + 300.0)).ln();
        let got = m.transition_cost(Some(neko), Some(ga), 1);
        assert!((got - expected).abs() < 1e-5, "{got} vs {expected}");
        let expected_unseen = -((300.0 * uni) / (counts.unigram[BOS as usize] as f64 + 300.0)).ln();
        let got_unseen = m.transition_cost(Some(BOS), Some(ga), 1);
        assert!((got_unseen - expected_unseen).abs() < 1e-5);
    }

    #[test]
    fn どの平滑化でも_prev_ごとの遷移確率の和は_1_になる() {
        for p in all_params() {
            let counts = counter().counts(1);
            let m = NgramModel::build(&counts, &p).unwrap();
            let vocab = counts.tokens.len() as u32;
            for prev in 0..vocab {
                if prev == EOS {
                    continue;
                }
                // BOS は後続に現れないので和から外す。加算平滑化の unigram は BOS の
                // 文数分（この小さな例では 1 割強、実物では数%）をそこに残すので、
                // その分だけ和は 1 に届かない。
                let sum: f64 = (1..vocab)
                    .map(|next| (-m.transition_cost(Some(prev), Some(next), 1)).exp())
                    .sum();
                assert!((sum - 1.0).abs() < 0.15, "{p:?} prev={prev} sum={sum}");
            }
        }
    }

    #[test]
    fn 刈り込んでも遷移確率の和は_1_を超えない() {
        for p in all_params() {
            let counts = counter().counts(2);
            assert!(counts.bigram.len() < counter().counts(1).bigram.len());
            let m = NgramModel::build(&counts, &p).unwrap();
            let vocab = counts.tokens.len() as u32;
            for prev in 0..vocab {
                let sum: f64 = (1..vocab)
                    .map(|next| (-m.transition_cost(Some(prev), Some(next), 1)).exp())
                    .sum();
                assert!(sum < 1.0 + 1e-6, "{p:?} prev={prev} sum={sum}");
            }
        }
    }

    #[test]
    fn 頻度の統計は刈り込み前で数える() {
        let counts = counter().counts(2);
        let c = counter();
        assert_eq!(counts.n_succ[c.vocab["猫"] as usize], 1);
        assert_eq!(counts.n_pred[c.vocab["鳴く"] as usize], 1);
        assert_eq!(counts.n_succ[BOS as usize], 2);
        assert!(counts.n1 > 0);
    }

    #[test]
    fn 組むときの閾値で_bigram_を落とす() {
        let counts = counter().counts(1);
        let full = NgramModel::build(&counts, &BuildParams::default()).unwrap();
        let p = BuildParams {
            min_bigram: 2,
            ..BuildParams::default()
        };
        let m = NgramModel::build(&counts, &p).unwrap();
        assert!(m.bigram_size() < full.bigram_size());
        // 「犬 は」は 1 回なので落ち、unigram に戻る。
        let (inu, ha) = (m.id("犬"), m.id("は"));
        assert!(m.transition_cost(inu, ha, 1) > full.transition_cost(inu, ha, 1));
    }

    #[test]
    fn 保存して読み直せる() {
        for p in all_params() {
            let m = NgramModel::build(&counter().counts(1), &p).unwrap();
            let mut buf = Vec::new();
            m.save(&mut buf).unwrap();
            let m2 = NgramModel::load(buf.as_slice()).unwrap();
            assert_eq!(m2.vocab_size(), m.vocab_size());
            assert_eq!(m2.bigram_size(), m.bigram_size());
            assert_eq!(
                m2.transition_cost(m2.id("猫"), m2.id("が"), 1),
                m.transition_cost(m.id("猫"), m.id("が"), 1)
            );
            assert_eq!(
                m2.transition_cost(None, None, 2),
                m.transition_cost(None, None, 2)
            );
            assert_eq!(m2.id("鳴く"), m.id("鳴く"));
        }
    }

    #[test]
    fn 古い形式のファイルは理由を示して拒む() {
        let mut buf = Vec::new();
        let mut enc = zstd::Encoder::new(&mut buf, 1).unwrap();
        enc.write_all(b"\x05\x03<s>\x04</s>").unwrap();
        enc.finish().unwrap();
        let err = NgramModel::load(buf.as_slice()).unwrap_err();
        assert!(err.to_string().contains("current format"), "{err}");

        for old in [1u32, 3] {
            let mut buf = Vec::new();
            let mut enc = zstd::Encoder::new(&mut buf, 1).unwrap();
            enc.write_all(MAGIC).unwrap();
            enc.write_all(&old.to_le_bytes()).unwrap();
            enc.finish().unwrap();
            let err = NgramModel::load(buf.as_slice()).unwrap_err();
            assert!(err.to_string().contains(&format!("version {old}")), "{err}");
        }
    }

    #[test]
    fn 頻度の列が昇順でなければ組めない() {
        let mut counts = counter().counts(1);
        counts.bigram.swap(0, 1);
        assert!(NgramModel::build(&counts, &BuildParams::default()).is_err());
    }
}
