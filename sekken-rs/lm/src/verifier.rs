//! 言語モデルを core の `Verifier` に繋ぐ。

use sekken_core::verify::{Verdict, Verifier, VerifySession};

use crate::scorer::{LmScorer, Scoring};

impl Verifier for LmScorer {
    fn begin<'a>(&'a self, input: &str) -> Box<dyn VerifySession + 'a> {
        Box::new(LmScorer::begin(self, input))
    }
}

impl VerifySession for Scoring<'_> {
    /// 文全体と問い合わせのコストを、並べ替えと同じ採点から拾う。
    fn verify(&mut self, sentences: &[String], queries: &[Vec<(usize, char)>]) -> Vec<Verdict> {
        let scored = self.score(sentences);
        scored
            .iter()
            .zip(queries)
            .map(|(scored, queries)| Verdict {
                cost: self.nll(scored),
                char_costs: queries
                    .iter()
                    .map(|&(pos, c)| self.char_cost(scored, pos, self.scorer().vocab().id(c)))
                    .collect(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use sekken_core::rerank::SentenceScorer;
    use sekken_core::verify::{Verdict, Verifier};

    use crate::condition::Condition;
    use crate::scorer::LmScorer;
    use crate::scorer::tests::{scorer, scorer_with};
    use crate::vocab::Vocab;

    fn verify(
        s: &LmScorer,
        input: &str,
        sentences: &[String],
        queries: &[Vec<(usize, char)>],
    ) -> Vec<Verdict> {
        Verifier::begin(s, input).verify(sentences, queries)
    }

    fn verify_one(s: &LmScorer, sentence: &str, queries: &[(usize, char)]) -> Verdict {
        verify(s, "", &[sentence.to_string()], &[queries.to_vec()])
            .pop()
            .unwrap()
    }

    #[test]
    fn 文全体のコストは並べ替え用の採点と一致する() {
        let s = scorer();
        let verdict = verify_one(&s, "猫が鳴く", &[]);
        let cost = s.costs("", &["猫が鳴く".to_string()])[0];
        assert!((verdict.cost - cost).abs() < 1e-4);
        assert!(verdict.char_costs.is_empty());
    }

    #[test]
    fn 下書きの字を順に問うとその和が文のコストになる() {
        let s = scorer();
        let queries = [(0, '猫'), (1, 'が'), (2, '鳴'), (3, 'く')];
        let verdict = verify_one(&s, "猫が鳴く", &queries);
        assert_eq!(verdict.char_costs.len(), 4);
        // EOS の分だけ文全体のほうが大きい。
        let sum: f64 = verdict.char_costs.iter().sum();
        assert!(verdict.cost > sum);
        assert!(verdict.char_costs.iter().all(|c| *c > 0.0));
    }

    #[test]
    fn 語彙にない字は未知語として有限のコストを持つ() {
        let s = scorer();
        let verdict = verify_one(&s, "猫が鳴く", &[(1, '犬')]);
        assert!(verdict.char_costs[0].is_finite());
    }

    #[test]
    fn 文の長さを超えた位置は無限大() {
        let s = scorer();
        let verdict = verify_one(&s, "猫", &[(5, '猫')]);
        assert!(verdict.char_costs[0].is_infinite());
    }

    #[test]
    fn 長さの違う文をまとめて採点しても単独と同じ() {
        let s = scorer();
        let together = verify(
            &s,
            "",
            &["猫が鳴く".to_string(), "猫".to_string()],
            &[vec![(3, 'く')], vec![(0, '猫'), (2, 'が')]],
        );
        let alone = verify_one(&s, "猫", &[(0, '猫'), (2, 'が')]);
        assert!((together[1].cost - alone.cost).abs() < 1e-4);
        assert!((together[1].char_costs[0] - alone.char_costs[0]).abs() < 1e-4);
        // 短い文の長さを超えた位置は無限大。
        assert!(together[1].char_costs[1].is_infinite());
    }

    #[test]
    fn 同じ場で繰り返し採点しても最初から読んだのと同じ() {
        let s = scorer();
        let mut session = Verifier::begin(&s, "");
        session.verify(&["猫が鳴く".to_string()], &[vec![]]);
        let later = session.verify(
            &["猫がく".to_string(), "猫が鳴く".to_string()],
            &[vec![(2, 'く'), (2, '鳴')], vec![(4, '猫')]],
        );
        let fresh = verify(
            &s,
            "",
            &["猫がく".to_string(), "猫が鳴く".to_string()],
            &[vec![(2, 'く'), (2, '鳴')], vec![(4, '猫')]],
        );
        for (a, b) in later.iter().zip(&fresh) {
            assert!((a.cost - b.cost).abs() < 1e-4);
            for (x, y) in a.char_costs.iter().zip(&b.char_costs) {
                assert!((x - y).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn 条件付きなら文の位置は入力の分だけ後ろの行を読む() {
        let vocab = Vocab::build(["猫が鳴く\tneko"], 1);
        let plain = scorer_with(vocab.clone(), Condition::None);
        // 無条件の採点器に「neko\t猫」を渡し、位置 5（タブの次）に「猫」を置くコスト。
        let raw = verify(&plain, "", &["neko\t猫".to_string()], &[vec![(5, '猫')]]);
        // 別の重みになるので、同じ採点器を条件付きとして使う。
        let cond = LmScorer::new(plain.into_infer(), vocab, Condition::Roman);
        let v = verify(&cond, "neko", &["猫".to_string()], &[vec![(0, '猫')]]);
        assert!((v[0].char_costs[0] - raw[0].char_costs[0]).abs() < 1e-4);
        // 文全体のコストは出力側（猫 と EOS）だけで、全体より小さい。
        assert!(v[0].cost < raw[0].cost);
    }
}
