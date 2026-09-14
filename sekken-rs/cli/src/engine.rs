//! 辞書・モデル・分かち書きを読み込んで Sekken を組み立てる。

use std::path::PathBuf;
use std::sync::LazyLock;

use anyhow::{Context as _, Result};
use clap::Args;
use sekken_core::dictionary::Dictionary;
use sekken_core::input::Input;
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
    /// ユーザー辞書。`register` で登録した語を書き、無ければ最初の登録で作る
    #[arg(long)]
    pub user_jisyo: Option<PathBuf>,
    /// 候補を採点する文字言語モデル（train-lm の出力）。無ければ格子の順のまま
    #[arg(long)]
    pub lm: Option<PathBuf>,
    /// 言語モデルのコストに掛ける重み
    #[arg(long, default_value_t = 1.0)]
    pub lm_weight: f64,
    /// 言語モデルの提案で格子を再探索する回数の上限。0 なら再探索せず N-best を並べ替える
    #[arg(long, default_value_t = 3)]
    pub spec_rounds: usize,
    /// 提案の制約で再探索して採点する経路の本数
    #[arg(long, default_value_t = 5)]
    pub spec_width: usize,
    /// --spec-rounds 0 のとき、並べ替える N-best の本数
    #[arg(long, default_value_t = 20)]
    pub rerank_width: usize,
    /// 辞書での候補順位の対数に掛ける重み
    #[arg(long, default_value_t = sekken_core::lattice::RANK_WEIGHT)]
    pub rank_weight: f64,
    /// 読みをそのままかなにした候補に加えるコスト
    #[arg(long, default_value_t = sekken_core::lattice::KANA_PENALTY)]
    pub kana_penalty: f64,
}

pub type Engine = Sekken<NgramScorer<Tokenizer>>;

/// ローマ字入力の入口が使う変換表。エディタは自前の表で同じかなを作って送るので、
/// エンジン本体は表を持たない。
static TABLE: LazyLock<KanaTable> = LazyLock::new(KanaTable::default_table);

/// 大文字・`;` 境界付きのローマ字を変換する。コマンドラインと評価のための入口。
pub fn henkan_roman(engine: &Engine, roman: &str, top_n: usize) -> Vec<String> {
    engine.henkan(&Input::from_roman(&TABLE, roman), top_n)
}

impl EngineArgs {
    pub fn build(&self) -> Result<Engine> {
        let tokenizer = Tokenizer::load(&self.dic).context("load vibrato dictionary")?;
        let file = std::fs::File::open(&self.model)
            .with_context(|| format!("open {}", self.model.display()))?;
        let model = NgramModel::load(std::io::BufReader::new(file)).context("load model")?;
        let dict = Dictionary::load(&self.jisyo).context("load SKK dictionary")?;
        let user = match &self.user_jisyo {
            Some(path) => Dictionary::load_or_empty(path).context("load user dictionary")?,
            None => Dictionary::default(),
        };
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
            if self.spec_rounds > 0 {
                speculator = Some(Speculator {
                    verifier: Box::new(scorer),
                    weight: self.lm_weight,
                    rounds: self.spec_rounds,
                    width: self.spec_width,
                    stats: Default::default(),
                })
            } else {
                reranker = Some(Reranker {
                    scorer: Box::new(scorer),
                    weight: self.lm_weight,
                    width: self.rerank_width,
                })
            }
        }
        Ok(Sekken {
            dict,
            user,
            scorer: NgramScorer::new(model, tokenizer),
            weights: sekken_core::lattice::Weights {
                rank: self.rank_weight,
                kana_penalty: self.kana_penalty,
            },
            reranker,
            speculator,
        })
    }
}
