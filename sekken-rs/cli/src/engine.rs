//! 辞書・モデル・分かち書きを読み込んで Sekken を組み立てる。

use std::path::PathBuf;
use std::sync::LazyLock;

use anyhow::{Context as _, Result};
use clap::Args;
use sekken_core::dictionary::Dictionary;
use sekken_core::input::Input;
use sekken_core::kana::KanaTable;
use sekken_core::sekken::Sekken;
use sekken_core::speculate::Speculator;
use sekken_core::verify::Verifier;
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
    /// 候補を採点する文字言語モデル（train-lm の出力）
    #[arg(long)]
    pub lm: PathBuf,
    /// 言語モデルのコストに掛ける重み
    #[arg(long, default_value_t = 1.0)]
    pub lm_weight: f64,
    /// 言語モデルの提案で格子を再探索する回数の上限
    #[arg(long, default_value_t = 3, value_parser = clap::builder::RangedU64ValueParser::<usize>::new().range(1..))]
    pub spec_rounds: usize,
    /// 提案の制約で再探索して採点する経路の本数
    #[arg(long, default_value_t = 5)]
    pub spec_width: usize,
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

/// エンジンに渡せる文字言語モデル。投機的な変換の検証器として使う。
/// `Send` は、サーバーが別スレッドで組み立てたエンジンを受け取るため。
pub trait Lm: Verifier + Send {}
impl<T: Verifier + Send> Lm for T {}

impl EngineArgs {
    pub fn build(&self) -> Result<Engine> {
        let saved = self.load_lm()?;
        self.build_with(Box::new(LmScorer::from_saved(&saved)?))
    }

    /// `--lm` のファイルを読む。
    pub fn load_lm(&self) -> Result<SavedModel> {
        let path = &self.lm;
        let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
        let saved = SavedModel::load(std::io::BufReader::new(file)).context("load lm")?;
        eprintln!(
            "lm: vocab={} condition={:?}",
            saved.vocab.len(),
            saved.condition
        );
        Ok(saved)
    }

    /// 辞書とモデルを読み、`lm` を `--lm` の代わりの検証器として組み立てる。
    /// 呼ぶ側が採点器を持ち続けたいとき（確定した文で動かすなど）に使う。
    pub fn build_with(&self, lm: Box<dyn Lm>) -> Result<Engine> {
        let tokenizer = Tokenizer::load(&self.dic).context("load vibrato dictionary")?;
        let file = std::fs::File::open(&self.model)
            .with_context(|| format!("open {}", self.model.display()))?;
        let model = NgramModel::load(std::io::BufReader::new(file)).context("load model")?;
        let dict = Dictionary::load(&self.jisyo).context("load SKK dictionary")?;
        let user = match &self.user_jisyo {
            Some(path) => Dictionary::load_or_empty(path).context("load user dictionary")?,
            None => Dictionary::default(),
        };
        Ok(Sekken {
            dict,
            user,
            scorer: NgramScorer::new(model, tokenizer),
            weights: sekken_core::lattice::Weights {
                rank: self.rank_weight,
                kana_penalty: self.kana_penalty,
            },
            speculator: Speculator {
                verifier: lm,
                weight: self.lm_weight,
                rounds: self.spec_rounds,
                width: self.spec_width,
                stats: Default::default(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::EngineArgs;

    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        engine: EngineArgs,
    }

    fn parse(args: &[&str]) -> Result<EngineArgs, clap::Error> {
        let mut argv = vec!["sekken", "--dic", "d", "--model", "m", "--jisyo", "j"];
        argv.extend_from_slice(args);
        Cli::try_parse_from(argv).map(|cli| cli.engine)
    }

    #[test]
    fn 言語モデルが無ければ起動を断る() {
        assert!(parse(&[]).is_err());
        assert_eq!(parse(&["--lm", "l"]).unwrap().lm.to_str(), Some("l"));
    }

    #[test]
    fn 再探索しない設定は受け付けない() {
        assert!(parse(&["--lm", "l", "--spec-rounds", "0"]).is_err());
        assert_eq!(
            parse(&["--lm", "l", "--spec-rounds", "1"])
                .unwrap()
                .spec_rounds,
            1
        );
    }

    #[test]
    fn 並べ替えの設定は知らないフラグとして断る() {
        assert!(parse(&["--lm", "l", "--rerank-width", "20"]).is_err());
    }
}
