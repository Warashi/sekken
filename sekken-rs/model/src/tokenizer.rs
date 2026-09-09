//! vibrato による分かち書き。

use std::path::Path;

use anyhow::{Context as _, Result};
use vibrato::{Dictionary, Tokenizer as VibratoTokenizer};

/// 分かち書きした 1 語。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub surface: String,
    /// 読み（カタカナ）。辞書に無い語は `None`。
    pub reading: Option<String>,
}

pub struct Tokenizer {
    inner: VibratoTokenizer,
}

impl Tokenizer {
    /// zstd 圧縮された vibrato 辞書（`system.dic.zst`）を読む。
    pub fn load(path: &Path) -> Result<Tokenizer> {
        let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
        let reader = zstd::Decoder::new(file).context("zstd decoder")?;
        let dict = Dictionary::read(reader).context("read vibrato dictionary")?;
        Ok(Tokenizer {
            inner: VibratoTokenizer::new(dict),
        })
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
    fn ipadic_の素性から読みを取る() {
        assert_eq!(
            reading_of("名詞,一般,*,*,*,*,猫,ネコ,ネコ").as_deref(),
            Some("ネコ")
        );
        assert_eq!(reading_of("名詞,一般,*,*,*,*,*"), None);
    }
}
