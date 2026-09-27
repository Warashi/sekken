//! 候補列の良さを測るスコアラーの抽象。

/// 表層形の列にコスト（小さいほど良い）を与える。
pub trait Scorer {
    /// 表層形単体のコスト。
    fn unigram(&self, surface: &str) -> f64;
    /// 左から右への接続コスト。`None` は文頭または文末。
    fn bigram(&self, left: Option<&str>, right: Option<&str>) -> f64;
    /// 格子の接続では見えない、文全体に掛かるコスト。投機で採点する文にだけ足す。
    /// 格子の探索は隣り合う語しか見ないので、それより長い文脈の分をここで数える。
    fn sentence(&self, _surface: &str) -> f64 {
        0.0
    }
}
