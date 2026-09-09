//! 言語モデルを core の `SentenceScorer` に繋ぐ。
//!
//! 採点は入力側（BOS、条件付きなら前置する入力と `\t`）を 1 本だけ読んで状態を取り、
//! 候補側は trie にまとめて、共有する接頭辞を一度だけ読む。上位 20 本の候補は
//! 文字の半分以上が他の候補と接頭辞を共有するので、候補ごとに全文を読むより
//! 計算が半分近くで済む。

use sekken_core::kana::KanaTable;
use sekken_core::rerank::SentenceScorer;

use crate::condition::Condition;
use crate::infer::Infer;
use crate::session::Session;
use crate::vocab::{BOS, Vocab};

pub struct LmScorer {
    infer: Infer,
    vocab: Vocab,
    /// 「入力 \t 出力」で学習したモデルなら、採点する文に前置する入力の種類。
    condition: Condition,
    table: KanaTable,
}

/// 1 回の変換の間の採点。入力側を読み終えた状態と、読んだ候補を持ち回る。
pub struct Scoring<'a> {
    scorer: &'a LmScorer,
    session: Session<'a>,
}

/// 1 文の採点。`path[i]` は文の i 文字目まで読んだ節で、`path[i]` の次の字の
/// 対数確率を `Scoring::log_prob` で引ける（i 文字目を予測する節は `path[i - 1]`、
/// 先頭は根）。
pub struct Scored {
    pub ids: Vec<u32>,
    pub path: Vec<usize>,
}

impl LmScorer {
    pub fn new(infer: Infer, vocab: Vocab, condition: Condition) -> LmScorer {
        LmScorer {
            infer,
            vocab,
            condition,
            table: KanaTable::default_table(),
        }
    }

    pub(crate) fn vocab(&self) -> &Vocab {
        &self.vocab
    }

    #[cfg(test)]
    pub(crate) fn into_infer(self) -> Infer {
        self.infer
    }

    /// 入力側の id 列。条件付きなら BOS、前置する入力、`\t`。無条件なら BOS だけ。
    fn prefix_ids(&self, input: &str) -> Vec<u32> {
        let mut ids = vec![BOS];
        if let Some(prefix) = self.condition.prefix(&self.table, input) {
            ids.extend(prefix.chars().map(|c| self.vocab.id(c)));
            ids.push(self.vocab.id('\t'));
        }
        ids
    }

    /// 入力側を読み、この入力に対する採点を始める。
    pub fn begin(&self, input: &str) -> Scoring<'_> {
        Scoring {
            scorer: self,
            session: Session::open(&self.infer, &self.prefix_ids(input)),
        }
    }

    /// 各文の負の対数尤度（EOS まで含む）。
    pub fn nll(&self, input: &str, sentences: &[String]) -> Vec<f64> {
        let mut scoring = self.begin(input);
        scoring
            .score(sentences)
            .iter()
            .map(|s| scoring.nll(s))
            .collect()
    }
}

impl<'a> Scoring<'a> {
    pub(crate) fn scorer(&self) -> &'a LmScorer {
        self.scorer
    }

    /// 各文を読む。既に読んだ接頭辞は読み直さない。
    pub fn score(&mut self, sentences: &[String]) -> Vec<Scored> {
        let ids: Vec<Vec<u32>> = sentences
            .iter()
            .map(|s| s.chars().map(|c| self.scorer.vocab.id(c)).collect())
            .collect();
        let paths = self.session.read(&ids);
        ids.into_iter()
            .zip(paths)
            .map(|(ids, path)| Scored { ids, path })
            .collect()
    }

    /// 文全体（EOS まで）の負の対数尤度。
    pub fn nll(&self, scored: &Scored) -> f64 {
        self.session.nll(&scored.path, &scored.ids)
    }

    /// 文の先頭 `pos` 文字の次に `id` が来るコスト。`pos` が文の長さを超えたら無限大。
    pub fn char_cost(&self, scored: &Scored, pos: usize, id: u32) -> f64 {
        let node = match pos {
            0 => 0,
            _ => match scored.path.get(pos - 1) {
                Some(&node) => node,
                None => return f64::INFINITY,
            },
        };
        -f64::from(self.session.log_prob(node, id))
    }
}

impl SentenceScorer for LmScorer {
    fn costs(&self, input: &str, sentences: &[String]) -> Vec<f64> {
        self.nll(input, sentences)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::config::ModelConfig;
    use crate::infer::Chain;
    use crate::testing::random_weights;
    use crate::vocab::EOS;

    fn config(vocab: &Vocab) -> ModelConfig {
        ModelConfig {
            vocab_size: vocab.len(),
            d_model: 8,
            n_layers: 1,
            n_heads: 1,
            head_dim: 8,
            d_state: 4,
            mlp_dim: 16,
        }
    }

    /// 形だけ合った重みの採点器。
    pub(crate) fn scorer_with(vocab: Vocab, condition: Condition) -> LmScorer {
        let config = config(&vocab);
        let infer = Infer::new(config.clone(), &random_weights(&config, 1)).unwrap();
        LmScorer::new(infer, vocab, condition)
    }

    pub(crate) fn scorer() -> LmScorer {
        scorer_with(Vocab::build(["猫が鳴く"], 1), Condition::None)
    }

    /// BOS も EOS も付けない id 列。
    fn ids(vocab: &Vocab, s: &str) -> Vec<u32> {
        s.chars().map(|c| vocab.id(c)).collect()
    }

    /// 入力側と候補を 1 本の連鎖として読んだ、候補側（EOS まで）の負の対数尤度。
    fn reference(s: &LmScorer, prefix: &[u32], ids: &[u32]) -> f64 {
        let whole: Vec<u32> = prefix.iter().chain(ids).copied().collect();
        let read = s.infer.read(&[Chain {
            state: &s.infer.init_state(),
            ids: &whole,
        }]);
        let d = s.infer.config().d_model;
        let mut total = 0.0;
        for (i, &id) in ids.iter().chain(std::iter::once(&EOS)).enumerate() {
            let pos = prefix.len() - 1 + i;
            total -= f64::from(s.infer.log_prob(
                &read.hidden[pos * d..(pos + 1) * d],
                read.lse[pos],
                id,
            ));
        }
        total
    }

    #[test]
    fn 文ごとに_1_つのコストを返す() {
        let s = scorer();
        let costs = s.costs("", &["猫が鳴く".to_string(), "猫".to_string()]);
        assert_eq!(costs.len(), 2);
        assert!(costs.iter().all(|c| *c > 0.0));
    }

    #[test]
    fn 無条件のコストは_bos_から一本で読んだ値と一致する() {
        let vocab = Vocab::build(["猫が鳴く"], 1);
        let s = scorer_with(vocab.clone(), Condition::None);
        let cost = s.costs("", &["猫が鳴く".to_string()])[0];
        let expected = reference(&s, &[BOS], &ids(&vocab, "猫が鳴く"));
        assert!((cost - expected).abs() < 1e-4, "{cost} vs {expected}");
    }

    #[test]
    fn 長さの違う文を一緒に採点しても単独と同じ() {
        let s = scorer();
        let together = s.costs("", &["猫が鳴く".to_string(), "猫".to_string()]);
        let alone = s.costs("", &["猫".to_string()]);
        assert!((together[1] - alone[0]).abs() < 1e-4);
    }

    #[test]
    fn 条件付きなら入力を前置して出力側だけのコストになる() {
        let vocab = Vocab::build(["猫が鳴く\tneko"], 1);
        let s = scorer_with(vocab.clone(), Condition::Roman);
        // 「neko \t 猫」を 1 本で読み、\t の後だけを数えた値と一致する。
        let mut prefix = vec![BOS];
        prefix.extend(ids(&vocab, "neko\t"));
        let expected = reference(&s, &prefix, &ids(&vocab, "猫"));
        let cost = s.costs("neko", &["猫".to_string()])[0];
        assert!((cost - expected).abs() < 1e-4);
        assert_ne!(cost, s.costs("", &["猫".to_string()])[0]);
    }

    #[test]
    fn カタカナ読みの条件付きはローマ字入力をカタカナにして前置する() {
        let vocab = Vocab::build(["猫が鳴く\tネコガナク"], 1);
        let s = scorer_with(vocab.clone(), Condition::Katakana);
        let mut prefix = vec![BOS];
        prefix.extend(ids(&vocab, "ネコガナク\t"));
        let expected = reference(&s, &prefix, &ids(&vocab, "猫が鳴く"));
        let cost = s.costs("NekogaNaku", &["猫が鳴く".to_string()])[0];
        assert!((cost - expected).abs() < 1e-4);
    }

    #[test]
    fn 接頭辞を共有する候補も単独で採点したのと同じ() {
        let s = scorer();
        let sentences: Vec<String> = ["猫が鳴く", "猫が", "猫", "鳴く猫が", "猫が鳴く"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let together = s.costs("", &sentences);
        for (sentence, cost) in sentences.iter().zip(&together) {
            let alone = s.costs("", std::slice::from_ref(sentence))[0];
            assert!((cost - alone).abs() < 1e-4, "{sentence}: {cost} vs {alone}");
        }
        assert_eq!(together[0], together[4]);
    }

    #[test]
    fn 接頭辞を多く共有する多数の候補をまとめて採点しても単独と同じ() {
        let s = scorer();
        // 4 文字の語彙から作る短い列は接頭辞を多く共有し、分岐の回数も多い。
        let alphabet = ['猫', 'が', '鳴', 'く'];
        let mut seed = 12345u64;
        let mut next = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as usize
        };
        let sentences: Vec<String> = (0..40)
            .map(|_| {
                let len = next() % 9;
                (0..len).map(|_| alphabet[next() % 4]).collect()
            })
            .collect();
        let together = s.costs("neko", &sentences);
        for (sentence, cost) in sentences.iter().zip(&together) {
            let alone = s.costs("neko", std::slice::from_ref(sentence))[0];
            assert!(
                (cost - alone).abs() < 1e-3,
                "{sentence:?}: {cost} vs {alone}"
            );
        }
    }

    #[test]
    fn 同じ採点の場で続けて読んでも別々に読んだのと同じ() {
        let s = scorer();
        let mut scoring = s.begin("");
        let first = scoring.score(&["猫が鳴く".to_string()]);
        let second = scoring.score(&["猫がく".to_string(), "鳴く".to_string()]);
        let costs: Vec<f64> = first
            .iter()
            .chain(&second)
            .map(|x| scoring.nll(x))
            .collect();
        let alone = s.costs(
            "",
            &[
                "猫が鳴く".to_string(),
                "猫がく".to_string(),
                "鳴く".to_string(),
            ],
        );
        for (a, b) in costs.iter().zip(&alone) {
            assert!((a - b).abs() < 1e-4, "{a} vs {b}");
        }
    }
}
