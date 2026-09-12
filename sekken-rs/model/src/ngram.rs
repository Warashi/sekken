//! 語の unigram / bigram の頻度と、そこから導くコスト。
//!
//! 学習では `Counts`（学習側が数えた頻度）を `NgramModel::build` で実行時の形に
//! 固めて保存する。実行時の `NgramModel` はソート済みの配列だけを持ち、
//! ファイルから読んだ列をそのまま使う。語彙 270 万・bigram 1100 万件を
//! HashMap に挿入し直すと読み込みに数秒かかるので、二分探索で引ける形で
//! 保存しておく。
//!
//! 遷移確率は平滑化の種類によらず `num(prev, next) + λ(prev) · low(prev, next)`
//! の形に固めてある。`num` は bigram ごと、`λ` は語ごとの値で、`low` は
//! 語の unigram 分布か、品詞クラスの遷移 × クラス内の語の分布。

use std::collections::HashMap;
use std::io::{Read, Write};

use anyhow::{Context as _, Result, bail, ensure};

pub const BOS: u32 = 0;
pub const EOS: u32 = 1;

const MAGIC: &[u8; 4] = b"SKNG";
const FORMAT_VERSION: u32 = 2;

/// 学習で数えた頻度。`NgramModel::build` の入力。
///
/// 品詞クラスは最細の粒度（`品詞,細分類1,細分類2,細分類3,活用型,活用形`）で
/// 数えておき、組むときに前方の欄だけ取って畳む。文頭・文末はそれぞれ
/// `<s>` / `</s>` というクラスを持つ。
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
    pub class_names: Vec<String>,
    /// 語ごとの多数決のクラス。
    pub class_of: Vec<u32>,
    /// 文中のクラスの遷移 `(prev, next, count)`。昇順。
    pub class_bigram: Vec<(u32, u32, u64)>,
}

/// bigram を下位分布で平滑化する方法。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Smoothing {
    /// `(c + prior · low) / (c(prev) + prior)`。
    Dirichlet { prior: f64 },
    /// 補間 Kneser-Ney。`discount` が `None` なら `n1 / (n1 + 2 n2)` で見積もる。
    KneserNey { discount: Option<f64> },
}

/// 平滑化の下位分布。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Lower {
    /// 語の unigram（Dirichlet では加算平滑化、Kneser-Ney では継続確率）。
    Unigram,
    /// `P(class(next) | class(prev)) · P(next | class(next))`。`fields` はクラス名の
    /// 先頭から使う欄の数、`prior` はクラス遷移の加算平滑化。
    Class { fields: usize, prior: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuildParams {
    pub smoothing: Smoothing,
    pub lower: Lower,
    /// 未知語 1 文字あたりの追加コスト。
    pub unknown_char_cost: f64,
}

impl Default for BuildParams {
    /// 20 万記事のモデルで評価して選んだ値。PRIOR は 1 から 1000 まで比べて 300、
    /// 小さいと疎な bigram を信じすぎて unigram で見れば明らかな誤りを選ぶ。
    fn default() -> BuildParams {
        BuildParams {
            smoothing: Smoothing::Dirichlet { prior: 300.0 },
            lower: Lower::Unigram,
            unknown_char_cost: 4.0,
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
    class_names: Vec<String>,
    classes: HashMap<String, u32>,
    /// (語, クラス) ごとの回数。多数決に使う。
    word_class: HashMap<(u32, u32), u64>,
    class_bigram: HashMap<(u32, u32), u64>,
}

impl NgramCounter {
    pub fn new() -> NgramCounter {
        let mut m = NgramCounter::default();
        m.intern("<s>");
        m.intern("</s>");
        m.class("<s>");
        m.class("</s>");
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

    fn class(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.classes.get(name) {
            return id;
        }
        let id = self.class_names.len() as u32;
        self.class_names.push(name.to_string());
        self.classes.insert(name.to_string(), id);
        id
    }

    /// 1 文分の語列を数える。文頭・文末の遷移も含める。クラスは `*` 一つ。
    pub fn add_sentence<'a>(&mut self, tokens: impl IntoIterator<Item = &'a str>) {
        self.add_classified(tokens.into_iter().map(|t| (t, "*")));
    }

    /// 1 文分の (語, クラス) の列を数える。
    pub fn add_classified<'a>(&mut self, tokens: impl IntoIterator<Item = (&'a str, &'a str)>) {
        let mut prev = BOS;
        let mut prev_class = BOS;
        self.unigram[BOS as usize] += 1;
        *self.word_class.entry((BOS, BOS)).or_default() += 1;
        for (t, c) in tokens {
            let id = self.intern(t);
            let class = self.class(c);
            self.unigram[id as usize] += 1;
            self.total += 1;
            *self.bigram.entry((prev, id)).or_default() += 1;
            *self.word_class.entry((id, class)).or_default() += 1;
            *self.class_bigram.entry((prev_class, class)).or_default() += 1;
            prev = id;
            prev_class = class;
        }
        *self.bigram.entry((prev, EOS)).or_default() += 1;
        *self.class_bigram.entry((prev_class, EOS)).or_default() += 1;
        *self.word_class.entry((EOS, EOS)).or_default() += 1;
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
        let mut class_bigram: Vec<_> = self
            .class_bigram
            .iter()
            .map(|(&(p, n), &c)| (p, n, c))
            .collect();
        class_bigram.sort_unstable();
        Counts {
            tokens: self.tokens.clone(),
            unigram: self.unigram.clone(),
            total: self.total,
            n_succ,
            n_pred,
            n1,
            n2,
            bigram,
            class_names: self.class_names.clone(),
            class_of: majority_class(vocab, self.word_class.iter().map(|(&k, &c)| (k, c))),
            class_bigram,
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

/// (語, クラス) の回数から語ごとの多数決のクラスを決める。同数ならクラス id の小さい方。
pub fn majority_class(
    vocab: usize,
    word_class: impl Iterator<Item = ((u32, u32), u64)>,
) -> Vec<u32> {
    let mut best: Vec<(u64, u32)> = vec![(0, 0); vocab];
    for ((word, class), c) in word_class {
        let slot = &mut best[word as usize];
        if c > slot.0 || (c == slot.0 && class < slot.1) {
            *slot = (c, class);
        }
    }
    best.into_iter().map(|(_, class)| class).collect()
}

/// 品詞クラスの遷移と、クラス内の語の分布。
#[derive(Debug)]
struct ClassTable {
    class_of: Vec<u32>,
    n: usize,
    /// `P(next_class | prev_class)`。`n × n` の行優先。
    trans: Vec<f32>,
    /// `P(word | class_of(word))`。
    word_in_class: Vec<f32>,
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
    /// bigram ごとの、下位分布に頼らない確率の分子。
    num: Vec<f32>,
    /// 語ごとの、下位分布に掛ける重み。
    lambda: Vec<f32>,
    /// 語ごとの unigram 確率。先行が未知語のときと、クラスを使わない下位分布。
    unigram: Vec<f32>,
    class: Option<ClassTable>,
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
                && counts.n_pred.len() == vocab
                && counts.class_of.len() == vocab,
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
        // λ = 1 − Σ_kept num の分だけ下位分布に回す。刈り込んだ対の質量も下位分布に行く。
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
            .map(|w| {
                let history = counts.unigram[w] as f64;
                let l = match params.smoothing {
                    Smoothing::Dirichlet { prior } => prior / (history + prior),
                    Smoothing::KneserNey { .. } if history > 0.0 => 1.0 - kept[w],
                    Smoothing::KneserNey { .. } => 1.0,
                };
                l.clamp(0.0, 1.0) as f32
            })
            .collect();

        let class = match params.lower {
            Lower::Unigram => None,
            Lower::Class { fields, prior } => Some(build_class_table(counts, fields, prior)?),
        };
        Ok(NgramModel {
            text,
            offsets,
            sorted,
            row_start,
            next,
            num,
            lambda,
            unigram,
            class,
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

    pub fn class_size(&self) -> usize {
        self.class.as_ref().map_or(0, |c| c.n)
    }

    fn num(&self, prev: u32, next: u32) -> f64 {
        let row =
            self.row_start[prev as usize] as usize..self.row_start[prev as usize + 1] as usize;
        match self.next[row.clone()].binary_search(&next) {
            Ok(pos) => self.num[row.start + pos] as f64,
            Err(_) => 0.0,
        }
    }

    /// `prev` から `next` への遷移コスト（負の対数確率）。
    /// 未知語は `None` で表し、文字数に応じたコストを与える。
    pub fn transition_cost(&self, prev: Option<u32>, next: Option<u32>, next_chars: usize) -> f64 {
        let Some(next) = next else {
            return self.unknown_base + self.unknown_char_cost * next_chars as f64;
        };
        let uni = self.unigram[next as usize] as f64;
        let Some(prev) = prev else {
            return -uni.ln();
        };
        let low = match &self.class {
            None => uni,
            Some(t) => {
                let (pc, nc) = (t.class_of[prev as usize], t.class_of[next as usize]);
                t.trans[pc as usize * t.n + nc as usize] as f64
                    * t.word_in_class[next as usize] as f64
            }
        };
        -(self.num(prev, next) + self.lambda[prev as usize] as f64 * low).ln()
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
        match &self.class {
            None => enc.write_all(&0u32.to_le_bytes())?,
            Some(t) => {
                enc.write_all(&(t.n as u32).to_le_bytes())?;
                write_u32s(&mut enc, &t.class_of)?;
                write_f32s(&mut enc, &t.trans)?;
                write_f32s(&mut enc, &t.word_in_class)?;
            }
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
        let n = read_u32(&mut r)? as usize;
        let class = if n == 0 {
            None
        } else {
            Some(ClassTable {
                class_of: read_u32s(&mut r)?,
                n,
                trans: read_f32s(&mut r)?,
                word_in_class: read_f32s(&mut r)?,
            })
        };
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
            class,
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
        if let Some(t) = &self.class {
            ensure!(
                t.class_of.len() == vocab
                    && t.class_of.iter().all(|&c| (c as usize) < t.n)
                    && t.trans.len() == t.n * t.n
                    && t.word_in_class.len() == vocab,
                "class sections are inconsistent"
            );
        }
        Ok(())
    }
}

/// クラス名の先頭 `fields` 欄で畳み、遷移とクラス内の語の分布を作る。
fn build_class_table(counts: &Counts, fields: usize, prior: f64) -> Result<ClassTable> {
    ensure!(fields >= 1, "class must use at least one field");
    ensure!(prior > 0.0, "class prior must be positive");
    ensure!(
        counts.class_names.len() >= 2
            && counts.class_names[0] == "<s>"
            && counts.class_names[1] == "</s>",
        "class table must start with <s> and </s>"
    );
    let mut coarse_names: Vec<String> = Vec::new();
    let mut coarse_ids: HashMap<String, u32> = HashMap::new();
    let mut fine_to_coarse = Vec::with_capacity(counts.class_names.len());
    for name in &counts.class_names {
        let coarse = if name == "<s>" || name == "</s>" {
            name.clone()
        } else {
            name.split(',').take(fields).collect::<Vec<_>>().join(",")
        };
        let id = *coarse_ids.entry(coarse.clone()).or_insert_with(|| {
            coarse_names.push(coarse);
            (coarse_names.len() - 1) as u32
        });
        fine_to_coarse.push(id);
    }
    let n = coarse_names.len();
    let class_of: Vec<u32> = counts
        .class_of
        .iter()
        .map(|&c| {
            fine_to_coarse
                .get(c as usize)
                .copied()
                .context("class id out of range")
        })
        .collect::<Result<_>>()?;
    let mut trans_count = vec![0f64; n * n];
    let mut as_next = vec![0f64; n];
    let mut history = vec![0f64; n];
    let mut total = 0f64;
    for &(p, q, c) in &counts.class_bigram {
        let (p, q) = (
            *fine_to_coarse
                .get(p as usize)
                .context("class id out of range")? as usize,
            *fine_to_coarse
                .get(q as usize)
                .context("class id out of range")? as usize,
        );
        trans_count[p * n + q] += c as f64;
        as_next[q] += c as f64;
        history[p] += c as f64;
        total += c as f64;
    }
    ensure!(total > 0.0, "no class transition");
    let trans: Vec<f32> = (0..n * n)
        .map(|i| {
            let (p, q) = (i / n, i % n);
            ((trans_count[i] + prior * as_next[q] / total) / (history[p] + prior)) as f32
        })
        .collect();
    let mut class_mass = vec![0f64; n];
    for (w, &c) in class_of.iter().enumerate() {
        class_mass[c as usize] += counts.unigram[w] as f64;
    }
    let word_in_class: Vec<f32> = class_of
        .iter()
        .enumerate()
        .map(|(w, &c)| (counts.unigram[w] as f64 / class_mass[c as usize].max(1.0)) as f32)
        .collect();
    Ok(ClassTable {
        class_of,
        n,
        trans,
        word_in_class,
    })
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
        m.add_classified([
            ("猫", "名詞,一般"),
            ("が", "助詞,格助詞"),
            ("鳴く", "動詞,自立"),
        ]);
        m.add_classified([
            ("猫", "名詞,一般"),
            ("が", "助詞,格助詞"),
            ("眠る", "動詞,自立"),
        ]);
        m.add_classified([
            ("犬", "名詞,一般"),
            ("は", "助詞,係助詞"),
            ("吠える", "動詞,自立"),
        ]);
        m.add_classified([
            ("猫", "名詞,一般"),
            ("が", "助詞,格助詞"),
            ("鳴く", "動詞,自立"),
        ]);
        m
    }

    fn all_params() -> Vec<BuildParams> {
        let mut out = Vec::new();
        for smoothing in [
            Smoothing::Dirichlet { prior: 3.0 },
            Smoothing::KneserNey { discount: None },
            Smoothing::KneserNey {
                discount: Some(0.5),
            },
        ] {
            for lower in [
                Lower::Unigram,
                Lower::Class {
                    fields: 1,
                    prior: 1.0,
                },
                Lower::Class {
                    fields: 2,
                    prior: 1.0,
                },
            ] {
                out.push(BuildParams {
                    smoothing,
                    lower,
                    unknown_char_cost: 4.0,
                });
            }
        }
        out
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
    fn クラスへの_backoff_は同じ品詞の未学習の語を有利にする() {
        let counts = counter().counts(1);
        let p = BuildParams {
            smoothing: Smoothing::Dirichlet { prior: 3.0 },
            lower: Lower::Class {
                fields: 1,
                prior: 1.0,
            },
            unknown_char_cost: 4.0,
        };
        let m = NgramModel::build(&counts, &p).unwrap();
        assert_eq!(m.class_size(), 5); // <s> </s> 名詞 助詞 動詞
        // 「は」の後に動詞が来る遷移は学習していないが、「は 眠る」と「は 猫」なら
        // 助詞 → 動詞 の遷移がある分だけ「眠る」が有利。
        let ha = m.id("は");
        assert!(m.transition_cost(ha, m.id("眠る"), 1) < m.transition_cost(ha, m.id("猫"), 1));
    }

    #[test]
    fn 多数決のクラスは同数なら_id_の小さい方() {
        let mut c = NgramCounter::new();
        c.add_classified([("ない", "助動詞")]);
        c.add_classified([("ない", "形容詞")]);
        c.add_classified([("ない", "形容詞")]);
        let counts = c.counts(1);
        assert_eq!(
            counts.class_names[counts.class_of[c.vocab["ない"] as usize] as usize],
            "形容詞"
        );
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
    fn 保存して読み直せる() {
        for p in all_params() {
            let m = NgramModel::build(&counter().counts(1), &p).unwrap();
            let mut buf = Vec::new();
            m.save(&mut buf).unwrap();
            let m2 = NgramModel::load(buf.as_slice()).unwrap();
            assert_eq!(m2.vocab_size(), m.vocab_size());
            assert_eq!(m2.bigram_size(), m.bigram_size());
            assert_eq!(m2.class_size(), m.class_size());
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

        let mut buf = Vec::new();
        let mut enc = zstd::Encoder::new(&mut buf, 1).unwrap();
        enc.write_all(MAGIC).unwrap();
        enc.write_all(&1u32.to_le_bytes()).unwrap();
        enc.finish().unwrap();
        let err = NgramModel::load(buf.as_slice()).unwrap_err();
        assert!(err.to_string().contains("version 1"), "{err}");
    }

    #[test]
    fn 頻度の列が昇順でなければ組めない() {
        let mut counts = counter().counts(1);
        counts.bigram.swap(0, 1);
        assert!(NgramModel::build(&counts, &BuildParams::default()).is_err());
    }
}
