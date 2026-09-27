//! 確定した文から数える、使う本人の語の n-gram。
//!
//! 自分の文の外れは、技術文の語彙や語義（格子／講師、効く／聞く）で同じ語が
//! 繰り返し外れるものが多い。配布の bigram は一般の文で数えているのでこれを
//! 知らず、言語モデルの出力層を 1 歩ずつ動かす個人化も効きが緩やかなので、
//! 確定した文の語をそのまま数えて足す。
//!
//! 確率は Witten-Bell の補間で、最下位に配布の bigram の確率を置く。
//! 文脈 h の後に c(h) 回・t(h) 種類の語が続いたとき
//! P_k(w | h) = (c(h, w) + t(h) · P_{k-1}(w | h')) / (c(h) + t(h))。
//! 本人の文は少なく毎日増えるので、割引を回数の分布から見積もる Kneser-Ney は
//! 安定せず、正規化しない stupid backoff は次数ごとの重みの最適値がデータの量で
//! 動く。Witten-Bell は文脈ごとの回数と種類数だけで決まり、文を足すたびに
//! 数を増やすだけで正しく保てる。
//!
//! 語と文脈は表層形のハッシュで持ち、文そのものは持たない。保存したファイルから
//! 本人の文を復元できない。

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::io::{Read, Write};
use std::path::Path;

use anyhow::{Context as _, Result, bail, ensure};

/// 語の鍵。表層形のハッシュ。
pub type Key = u64;

/// 文頭と文末の鍵。表層形のハッシュと衝突しないよう、表層形には使わない値にする。
pub const BOS: Key = 0;
pub const EOS: Key = 1;

const MAGIC: &[u8; 4] = b"SKPN";
const FORMAT_VERSION: u32 = 1;

/// 表層形の鍵。FNV-1a を splitmix64 で混ぜ、文頭・文末の鍵を避ける。
pub fn key(surface: &str) -> Key {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in surface.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    mix(h).max(EOS + 1)
}

fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// 文脈の鍵に 1 語を左から足す。文脈は直近の語から遡って畳む。
fn extend(context: u64, word: Key) -> u64 {
    mix(context ^ word.rotate_left(17))
}

/// 文脈の鍵と次の語から n-gram の鍵を作る。文脈とは別の表に置くので形だけ変える。
fn ngram(context: u64, word: Key) -> u64 {
    mix(context.rotate_left(29) ^ word)
}

/// 長さ 0 の文脈の鍵。
const EMPTY: u64 = 0x5bd1_e995_5bd1_e995;

/// 鍵はすでにハッシュなので、表はそのまま使う。
#[derive(Default)]
struct KeyHasher(u64);

impl Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, _: &[u8]) {
        unreachable!("keys are hashed as u64")
    }
    fn write_u64(&mut self, n: u64) {
        self.0 = n;
    }
}

type Table<V> = HashMap<u64, V, BuildHasherDefault<KeyHasher>>;

/// 文脈の後に続いた語の数。
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct Followers {
    /// 続いた回数の計。
    total: u32,
    /// 続いた語の種類数。
    types: u32,
}

pub struct Personal {
    order: usize,
    /// n-gram → 回数。
    counts: Table<u32>,
    /// 文脈 → 続いた語の数。
    contexts: Table<Followers>,
}

impl Personal {
    /// `order` 語までの n-gram を数える。1 なら語の回数だけ。
    pub fn new(order: usize) -> Personal {
        assert!(order >= 1, "order must be at least 1");
        Personal {
            order,
            counts: Table::default(),
            contexts: Table::default(),
        }
    }

    pub fn order(&self) -> usize {
        self.order
    }

    /// 文頭・文末を除いた語の鍵の列を 1 文として数える。
    pub fn add(&mut self, words: &[Key]) {
        let seq: Vec<Key> = std::iter::once(BOS)
            .chain(words.iter().copied())
            .chain(std::iter::once(EOS))
            .collect();
        for i in 1..seq.len() {
            let mut context = EMPTY;
            for k in 0..self.order.min(i + 1) {
                if k > 0 {
                    context = extend(context, seq[i - k]);
                }
                let count = self.counts.entry(ngram(context, seq[i])).or_default();
                *count += 1;
                let followers = self.contexts.entry(context).or_default();
                followers.total += 1;
                if *count == 1 {
                    followers.types += 1;
                }
            }
        }
    }

    /// `history`（直近の語が最後）の後に `word` が来る対数確率。`base` は最下位に
    /// 置く配布の bigram の対数確率。文脈は `max_order - 1` 語まで遡る。
    pub fn log_prob(&self, history: &[Key], word: Key, base: f64, max_order: usize) -> f64 {
        let mut p = base.exp();
        let mut context = EMPTY;
        for k in 0..self.order.min(max_order) {
            if k > 0 {
                let Some(&w) = history.len().checked_sub(k).map(|i| &history[i]) else {
                    break;
                };
                context = extend(context, w);
            }
            // 文脈が無ければ、それより長い文脈も無い。
            let Some(f) = self.contexts.get(&context) else {
                break;
            };
            let n = self.counts.get(&ngram(context, word)).copied().unwrap_or(0);
            let (n, total, types) = (f64::from(n), f64::from(f.total), f64::from(f.types));
            p = (n + types * p) / (total + types);
        }
        p.ln()
    }

    /// `path` があれば読み、無ければ `order` 語までの空のものを作る。
    pub fn load_or_new(path: &Path, order: usize) -> Result<Personal> {
        if !path.exists() {
            return Ok(Personal::new(order));
        }
        let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
        Personal::load(std::io::BufReader::new(file))
            .with_context(|| format!("load {}", path.display()))
    }

    /// `path` に書く。途中で落ちても前のファイルが壊れないよう、別名に書いてから置き換える。
    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        let tmp = path.with_extension("tmp");
        let file =
            std::fs::File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?;
        self.save(std::io::BufWriter::new(file))
            .with_context(|| format!("write {}", tmp.display()))?;
        std::fs::rename(&tmp, path)
            .with_context(|| format!("rename {} to {}", tmp.display(), path.display()))?;
        Ok(())
    }

    /// zstd で包んだ固定幅リトルエンディアンの列として書く。
    pub fn save(&self, w: impl Write) -> Result<()> {
        let mut enc = zstd::Encoder::new(w, 9).context("zstd encoder")?;
        enc.write_all(MAGIC)?;
        enc.write_all(&FORMAT_VERSION.to_le_bytes())?;
        enc.write_all(&(self.order as u32).to_le_bytes())?;
        enc.write_all(&(self.counts.len() as u64).to_le_bytes())?;
        for (&k, &n) in &self.counts {
            enc.write_all(&k.to_le_bytes())?;
            enc.write_all(&n.to_le_bytes())?;
        }
        enc.write_all(&(self.contexts.len() as u64).to_le_bytes())?;
        for (&k, f) in &self.contexts {
            enc.write_all(&k.to_le_bytes())?;
            enc.write_all(&f.total.to_le_bytes())?;
            enc.write_all(&f.types.to_le_bytes())?;
        }
        enc.finish().context("finish zstd")?;
        Ok(())
    }

    pub fn load(r: impl Read) -> Result<Personal> {
        let mut dec = zstd::Decoder::new(r).context("zstd decoder")?;
        let mut magic = [0u8; 4];
        dec.read_exact(&mut magic).context("read magic")?;
        ensure!(&magic == MAGIC, "not a personal n-gram file");
        let version = read_u32(&mut dec)?;
        if version != FORMAT_VERSION {
            bail!("unsupported personal n-gram format version {version}");
        }
        let order = read_u32(&mut dec)? as usize;
        ensure!(order >= 1, "order must be at least 1");
        let mut p = Personal::new(order);
        let n = read_u64(&mut dec)?;
        for _ in 0..n {
            let k = read_u64(&mut dec)?;
            p.counts.insert(k, read_u32(&mut dec)?);
        }
        let n = read_u64(&mut dec)?;
        for _ in 0..n {
            let k = read_u64(&mut dec)?;
            let total = read_u32(&mut dec)?;
            let types = read_u32(&mut dec)?;
            p.contexts.insert(k, Followers { total, types });
        }
        Ok(p)
    }
}

fn read_u32(r: &mut impl Read) -> Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)
        .context("truncated personal n-gram file")?;
    Ok(u32::from_le_bytes(b))
}

fn read_u64(r: &mut impl Read) -> Result<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)
        .context("truncated personal n-gram file")?;
    Ok(u64::from_le_bytes(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(words: &[&str]) -> Vec<Key> {
        words.iter().map(|w| key(w)).collect()
    }

    /// 語彙 `vocab` 上で、文脈 `history` の後の確率の和。基底は一様。
    fn total(p: &Personal, history: &[Key], vocab: &[Key], max_order: usize) -> f64 {
        let base = (1.0 / vocab.len() as f64).ln();
        vocab
            .iter()
            .map(|&w| p.log_prob(history, w, base, max_order).exp())
            .sum()
    }

    #[test]
    fn 何も数えていなければ配布の確率のまま() {
        let p = Personal::new(3);
        let base = 0.25f64.ln();
        assert!((p.log_prob(&[], key("格子"), base, 3) - base).abs() < 1e-12);
    }

    #[test]
    fn 数えた語は上がり数えていない語は下がる() {
        let mut p = Personal::new(1);
        p.add(&keys(&["格子", "を", "探索"]));
        let base = 0.01f64.ln();
        assert!(p.log_prob(&[], key("格子"), base, 1) > base);
        assert!(p.log_prob(&[], key("講師"), base, 1) < base);
    }

    #[test]
    fn どの次数でも確率の和は_1_のまま() {
        let vocab: Vec<Key> = std::iter::once(EOS)
            .chain(keys(&["格子", "講師", "を", "探索", "する"]))
            .collect();
        let mut p = Personal::new(4);
        p.add(&keys(&["格子", "を", "探索", "する"]));
        p.add(&keys(&["講師", "を", "探索"]));
        p.add(&keys(&["格子", "を"]));
        for history in [
            vec![],
            vec![BOS],
            keys(&["格子"]),
            keys(&["格子", "を"]),
            vec![BOS, key("格子"), key("を")],
            keys(&["無い", "文脈"]),
        ] {
            for max_order in 1..=4 {
                let s = total(&p, &history, &vocab, max_order);
                assert!((s - 1.0).abs() < 1e-9, "{history:?} {max_order}: {s}");
            }
        }
    }

    #[test]
    fn 長い文脈で繰り返した続きを選び分ける() {
        let mut p = Personal::new(3);
        for _ in 0..3 {
            p.add(&keys(&["格子", "を", "探索"]));
            p.add(&keys(&["木", "を", "植える"]));
        }
        let base = 0.01f64.ln();
        let after = |h: &[&str], w: &str, order| p.log_prob(&keys(h), key(w), base, order);
        // bigram（「を」の後）では区別できず、trigram で文脈どおりに分かれる。
        assert!((after(&["を"], "探索", 2) - after(&["を"], "植える", 2)).abs() < 1e-12);
        assert!(after(&["格子", "を"], "探索", 3) > after(&["格子", "を"], "植える", 3));
        assert!(after(&["木", "を"], "植える", 3) > after(&["木", "を"], "探索", 3));
    }

    #[test]
    fn 文頭と文末も文脈として数える() {
        let mut p = Personal::new(2);
        p.add(&keys(&["格子"]));
        let base = 0.01f64.ln();
        assert!(p.log_prob(&[BOS], key("格子"), base, 2) > p.log_prob(&[], key("格子"), base, 1));
        assert!(p.log_prob(&keys(&["格子"]), EOS, base, 2) > base);
    }

    #[test]
    fn 保存して読み直すと同じ確率() {
        let mut p = Personal::new(3);
        p.add(&keys(&["格子", "を", "探索"]));
        p.add(&keys(&["投機", "の", "反復"]));
        let mut buf = Vec::new();
        p.save(&mut buf).unwrap();
        let q = Personal::load(buf.as_slice()).unwrap();
        assert_eq!(q.order(), 3);
        let base = 0.01f64.ln();
        for (h, w) in [
            (vec!["格子", "を"], "探索"),
            (vec!["の"], "反復"),
            (vec![], "講師"),
        ] {
            let (h, w) = (keys(&h), key(w));
            assert_eq!(p.log_prob(&h, w, base, 3), q.log_prob(&h, w, base, 3));
        }
    }

    #[test]
    fn 無いファイルは空で作り書いたファイルは読み直せる() {
        let dir = std::env::temp_dir().join(format!("sekken-personal-{}", std::process::id()));
        let path = dir.join("sub/personal.zst");
        let _ = std::fs::remove_dir_all(&dir);
        let mut p = Personal::load_or_new(&path, 2).unwrap();
        assert_eq!(p.order(), 2);
        p.add(&keys(&["格子"]));
        p.save_to(&path).unwrap();
        assert!(!path.with_extension("tmp").exists());
        let q = Personal::load_or_new(&path, 4).unwrap();
        // 読めたファイルの次数が優先する。
        assert_eq!(q.order(), 2);
        let base = 0.01f64.ln();
        assert_eq!(
            p.log_prob(&[BOS], key("格子"), base, 2),
            q.log_prob(&[BOS], key("格子"), base, 2)
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn 形式の違うファイルは読まない() {
        let mut buf = Vec::new();
        let mut enc = zstd::Encoder::new(&mut buf, 1).unwrap();
        enc.write_all(b"SKNG").unwrap();
        enc.finish().unwrap();
        assert!(Personal::load(buf.as_slice()).is_err());
    }

    #[test]
    fn 表層形の鍵は文頭と文末の鍵と重ならない() {
        assert!(key("") > EOS);
        assert_ne!(key("格子"), key("講師"));
    }
}
