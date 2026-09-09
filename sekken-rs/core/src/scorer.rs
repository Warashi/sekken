//! 候補列の良さを測るスコアラーの抽象。

/// 表層形の列にコスト（小さいほど良い）を与える。
pub trait Scorer {
    /// 表層形単体のコスト。
    fn unigram(&self, surface: &str) -> f64;
    /// 左から右への接続コスト。`None` は文頭または文末。
    fn bigram(&self, left: Option<&str>, right: Option<&str>) -> f64;
}
