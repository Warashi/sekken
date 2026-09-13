//! vibrato による分かち書き。

use std::cell::RefCell;
use std::path::Path;

use anyhow::{Context as _, Result};
use vibrato::tokenizer::worker::Worker;
use vibrato::{Dictionary, Tokenizer as VibratoTokenizer};

/// 分かち書きした 1 語。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub surface: String,
    /// 読み（カタカナ）。辞書に無い語は `None`。
    pub reading: Option<String>,
}

/// これより長い文（UTF-8 のバイト数）を通した後は作業領域を作り直す。
/// 学習コーパスの文は平均 80 バイト程度で、最長は数十 KB に及ぶ。
const LONG_SENTENCE_BYTES: usize = 4096;

pub struct Tokenizer {
    /// vibrato の `Worker` は `Tokenizer` への参照を持つので、同じ構造体に
    /// 入れるには辞書を process の寿命まで持たせる。辞書は 1 process に
    /// 1 度しか読まないので、解放しないことによる損は無い。
    inner: &'static VibratoTokenizer,
    /// 使い回す作業領域。1 語ごとに作ると割り当てが分かち書きと同じだけかかる。
    worker: RefCell<Worker<'static>>,
}

impl Tokenizer {
    /// zstd 圧縮された vibrato 辞書（`system.dic.zst`）を読む。
    pub fn load(path: &Path) -> Result<Tokenizer> {
        let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
        let reader = zstd::Decoder::new(file).context("zstd decoder")?;
        let dict = Dictionary::read(reader).context("read vibrato dictionary")?;
        let inner: &'static VibratoTokenizer = Box::leak(Box::new(VibratoTokenizer::new(dict)));
        Ok(Tokenizer {
            inner,
            worker: RefCell::new(inner.new_worker()),
        })
    }

    /// 表層形だけに分けて順に `f` に渡す。素性を読まず、作業領域を使い回し、
    /// 表層形を複製しない。
    pub fn for_each_surface(&self, text: &str, mut f: impl FnMut(&str)) {
        self.with_worker(text, |worker| {
            for t in worker.token_iter() {
                f(t.surface());
            }
        });
    }

    /// 表層形と素性に分けて順に `f` に渡す。作業領域を使い回し、複製しない。
    pub fn for_each_token(&self, text: &str, mut f: impl FnMut(&str, &str)) {
        self.with_worker(text, |worker| {
            for t in worker.token_iter() {
                f(t.surface(), t.feature());
            }
        });
    }

    /// 共有の作業領域で `text` を分かち書きし、`f` に渡す。
    ///
    /// vibrato の格子は一度伸びた分の容量を縮めず、以後の文ごとにその全量を
    /// 空にするので、長い文を通した後は作業領域を捨てて作り直す。
    fn with_worker(&self, text: &str, f: impl FnOnce(&Worker<'static>)) {
        let mut worker = self.worker.borrow_mut();
        worker.reset_sentence(text);
        worker.tokenize();
        f(&worker);
        if text.len() > LONG_SENTENCE_BYTES {
            *worker = self.inner.new_worker();
        }
    }

    /// 表層形だけに分ける。
    pub fn surfaces(&self, text: &str) -> Vec<String> {
        let mut out = Vec::new();
        self.for_each_surface(text, |s| out.push(s.to_string()));
        out
    }

    pub fn tokenize(&self, text: &str) -> Vec<Token> {
        let mut worker = self.inner.new_worker();
        worker.reset_sentence(text);
        worker.tokenize();
        worker
            .token_iter()
            .map(|t| Token {
                surface: t.surface().to_string(),
                reading: reading_of(t.feature()),
            })
            .collect()
    }
}

/// ipadic の素性 `品詞,品詞細分類1,細分類2,細分類3,活用型,活用形,原形,読み,発音` から読みを取る。
fn reading_of(feature: &str) -> Option<String> {
    let fields: Vec<&str> = feature.split(',').collect();
    match fields.get(7) {
        Some(&"*") | None => None,
        Some(r) => Some(r.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenizer() -> Option<Tokenizer> {
        let path = std::env::var("SEKKEN_VIBRATO_DIC").ok()?;
        Some(Tokenizer::load(Path::new(&path)).unwrap())
    }

    #[test]
    fn 表層形と読みに分ける() {
        let Some(t) = tokenizer() else {
            eprintln!("SEKKEN_VIBRATO_DIC が無いので skip");
            return;
        };
        let tokens = t.tokenize("吾輩は猫である。");
        let surfaces: Vec<&str> = tokens.iter().map(|t| t.surface.as_str()).collect();
        assert_eq!(surfaces, ["吾輩", "は", "猫", "で", "ある", "。"]);
        assert_eq!(tokens[0].reading.as_deref(), Some("ワガハイ"));
        assert_eq!(tokens[2].reading.as_deref(), Some("ネコ"));
    }

    #[test]
    fn 表層形だけに分けても同じ切れ目になる() {
        let Some(t) = tokenizer() else {
            eprintln!("SEKKEN_VIBRATO_DIC が無いので skip");
            return;
        };
        let expected: Vec<String> = t
            .tokenize("吾輩は猫である。")
            .into_iter()
            .map(|t| t.surface)
            .collect();
        assert_eq!(t.surfaces("吾輩は猫である。"), expected);
        // 2 度目も作業領域を使い回して同じ結果になる。
        assert_eq!(t.surfaces("吾輩は猫である。"), expected);
    }

    #[test]
    fn 長い文の後も同じ切れ目になる() {
        let Some(t) = tokenizer() else {
            eprintln!("SEKKEN_VIBRATO_DIC が無いので skip");
            return;
        };
        let long = "吾輩は猫である。".repeat(LONG_SENTENCE_BYTES / 8);
        assert!(long.len() > LONG_SENTENCE_BYTES);
        let expected: Vec<String> = t.tokenize(&long).into_iter().map(|t| t.surface).collect();
        assert_eq!(t.surfaces(&long), expected);
        assert_eq!(
            t.surfaces("名前はまだ無い。"),
            ["名前", "は", "まだ", "無い", "。"]
        );
        let mut tokens = Vec::new();
        t.for_each_token("名前はまだ無い。", |s, _| {
            tokens.push(s.to_string())
        });
        assert_eq!(tokens, ["名前", "は", "まだ", "無い", "。"]);
    }

    #[test]
    fn ipadic_の素性から読みを取る() {
        assert_eq!(
            reading_of("名詞,一般,*,*,*,*,猫,ネコ,ネコ").as_deref(),
            Some("ネコ")
        );
        assert_eq!(reading_of("名詞,一般,*,*,*,*,*"), None);
    }
}
