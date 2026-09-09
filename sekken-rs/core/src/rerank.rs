//! 格子探索の後段で、文全体を別の採点器で並べ替える。

/// 文全体にコスト（小さいほど良い）を与える採点器。
/// 格子の `Scorer` は候補の遷移ごとに呼ばれるのに対し、こちらは完成した文を受け取る。
pub trait SentenceScorer {
    /// `sentences` それぞれのコストを同じ順で返す。`input` は変換前のローマ字入力で、
    /// 入力を条件にする採点器だけが使う。
    fn costs(&self, input: &str, sentences: &[String]) -> Vec<f64>;
}

/// 格子のコストと文全体のコストを合成して並べ替える。
pub struct Reranker {
    pub scorer: Box<dyn SentenceScorer>,
    /// 文全体のコストに掛ける重み。格子のコストは重み 1。
    pub weight: f64,
    /// 並べ替えの対象にする N-best の本数。
    pub width: usize,
}

impl Reranker {
    /// `(文, 格子のコスト)` の列を合成コストの小さい順に並べ替える。
    pub fn rerank(&self, input: &str, candidates: Vec<(String, f64)>) -> Vec<String> {
        let sentences: Vec<String> = candidates.iter().map(|(s, _)| s.clone()).collect();
        let costs = self.scorer.costs(input, &sentences);
        let mut scored: Vec<(String, f64)> = candidates
            .into_iter()
            .zip(costs)
            .map(|((s, lattice), lm)| (s, lattice + self.weight * lm))
            .collect();
        scored.sort_by(|a, b| a.1.total_cmp(&b.1));
        scored.into_iter().map(|(s, _)| s).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 「猫」を含む文を好む採点器。
    struct LikesCat;
    impl SentenceScorer for LikesCat {
        fn costs(&self, _: &str, sentences: &[String]) -> Vec<f64> {
            sentences
                .iter()
                .map(|s| if s.contains('猫') { 0.0 } else { 10.0 })
                .collect()
        }
    }

    fn reranker(weight: f64) -> Reranker {
        Reranker {
            scorer: Box::new(LikesCat),
            weight,
            width: 10,
        }
    }

    #[test]
    fn 文全体のコストで順位が入れ替わる() {
        let r = reranker(1.0);
        let result = r.rerank("", vec![("根子".into(), 1.0), ("猫".into(), 2.0)]);
        assert_eq!(result, ["猫", "根子"]);
    }

    #[test]
    fn 重みが_0_なら格子の順位のまま() {
        let r = reranker(0.0);
        let result = r.rerank("", vec![("根子".into(), 1.0), ("猫".into(), 2.0)]);
        assert_eq!(result, ["根子", "猫"]);
    }
}
