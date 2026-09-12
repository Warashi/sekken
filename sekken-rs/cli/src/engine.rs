//! 辞書・モデル・分かち書きを読み込んで Sekken を組み立てる。

use std::path::PathBuf;

use anyhow::{Context as _, Result};
use clap::Args;
use sekken_core::dictionary::Dictionary;
use sekken_core::kana::KanaTable;
use sekken_core::rerank::Reranker;
use sekken_core::sekken::Sekken;
use sekken_core::speculate::Speculator;
use sekken_lm::file::SavedModel;
use sekken_lm::scorer::LmScorer;
use sekken_model::ngram::NgramModel;
use sekken_model::scorer::NgramScorer;
use sekken_model::tokenizer::Tokenizer;

#[derive(Args)]
pub struct EngineArgs {
    /// vibrato 辞書（system.dic.zst）
    #[arg(long)]
    pub dic: PathBuf,
    /// 学習済み n-gram モデル
    #[arg(long)]
    pub model: PathBuf,
    /// SKK 辞書（SKK-JISYO.L など）
    #[arg(long)]
    pub jisyo: PathBuf,
    /// N-best を並べ替える文字言語モデル（train-lm の出力）。無ければ並べ替えない
    #[arg(long)]
    pub lm: Option<PathBuf>,
    /// 言語モデルのコストに掛ける重み
    #[arg(long, default_value_t = 1.0)]
    pub lm_weight: f64,
    /// 並べ替える N-best の本数
    #[arg(long, default_value_t = 20)]
    pub rerank_width: usize,
    /// 並べ替えの代わりに、言語モデルの提案で格子を再探索する回数の上限
    #[arg(long)]
    pub spec_rounds: Option<usize>,
    /// 提案の制約で再探索して採点する経路の本数
    #[arg(long, default_value_t = 1)]
    pub spec_width: usize,
    /// 辞書での候補順位の対数に掛ける重み
    #[arg(long, default_value_t = sekken_core::lattice::RANK_WEIGHT)]
    pub rank_weight: f64,
    /// 読みをそのままかなにした候補に加えるコスト
    #[arg(long, default_value_t = sekken_core::lattice::KANA_PENALTY)]
    pub kana_penalty: f64,
    /// 同じ位置に送りあり候補があるかな候補に、かな罰則に上乗せするコスト
    #[arg(long, default_value_t = sekken_core::lattice::OKURI_KANA_PENALTY)]
    pub okuri_kana_penalty: f64,
    /// 候補を分かち書きした語 1 つごとに加えるコスト（正なら長い 1 語を有利にする）
    #[arg(long, default_value_t = 0.0)]
    pub word_penalty: f64,
}

pub type Engine = Sekken<NgramScorer<Tokenizer>>;

impl EngineArgs {
    pub fn build(&self) -> Result<Engine> {
        let tokenizer = Tokenizer::load(&self.dic).context("load vibrato dictionary")?;
        let file = std::fs::File::open(&self.model)
            .with_context(|| format!("open {}", self.model.display()))?;
        let model = NgramModel::load(std::io::BufReader::new(file)).context("load model")?;
        let dict = Dictionary::load(&self.jisyo).context("load SKK dictionary")?;
        let mut reranker = None;
        let mut speculator = None;
        if let Some(path) = &self.lm {
            let file =
                std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
            let saved = SavedModel::load(std::io::BufReader::new(file)).context("load lm")?;
            let infer = saved.infer()?;
            eprintln!(
                "lm: vocab={} condition={:?}",
                saved.vocab.len(),
                saved.condition
            );
            let scorer = LmScorer::new(infer, saved.vocab, saved.condition);
            match self.spec_rounds {
                Some(rounds) => {
                    speculator = Some(Speculator {
                        verifier: Box::new(scorer),
                        weight: self.lm_weight,
                        rounds,
                        width: self.spec_width,
                    })
                }
                None => {
                    reranker = Some(Reranker {
                        scorer: Box::new(scorer),
                        weight: self.lm_weight,
                        width: self.rerank_width,
                    })
                }
            }
        }
        Ok(Sekken {
            table: KanaTable::default_table(),
            dict,
            scorer: NgramScorer::new(model, tokenizer).with_word_penalty(self.word_penalty),
            weights: sekken_core::lattice::Weights {
                rank: self.rank_weight,
                kana_penalty: self.kana_penalty,
                okuri_kana_penalty: self.okuri_kana_penalty,
            },
            reranker,
            speculator,
        })
    }
}
