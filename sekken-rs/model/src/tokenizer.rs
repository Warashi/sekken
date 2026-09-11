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

    /// 表層形だけに分ける。素性を読まず、作業領域を使い回す。
    pub fn surfaces(&self, text: &str) -> Vec<String> {
        let mut worker = self.worker.borrow_mut();
        worker.reset_sentence(text);
        worker.tokenize();
        worker
            .token_iter()
            .map(|t| t.surface().to_string())
            .collect()
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
    fn ipadic_の素性から読みを取る() {
        assert_eq!(
            reading_of("名詞,一般,*,*,*,*,猫,ネコ,ネコ").as_deref(),
            Some("ネコ")
        );
        assert_eq!(reading_of("名詞,一般,*,*,*,*,*"), None);
    }
}
