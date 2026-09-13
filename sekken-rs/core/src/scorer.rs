//! 候補列の良さを測るスコアラーの抽象。

/// 表層形の列にコスト（小さいほど良い）を与える。
pub trait Scorer {
    /// 表層形単体のコスト。
    fn unigram(&self, surface: &str) -> f64;
    /// 左から右への接続コスト。`None` は文頭または文末。
    fn bigram(&self, left: Option<&str>, right: Option<&str>) -> f64;
    /// 直前 2 つの表層形 `left2 left` から `right` への接続コスト。`left2` の
    /// `None` は `left` が文頭の表層形であること。既定は `left2` を見ない。
    fn trigram(&self, left2: Option<&str>, left: Option<&str>, right: Option<&str>) -> f64 {
        let _ = left2;
        self.bigram(left, right)
    }
    /// 接続コストが直前 2 つ目の表層形にも依るか。依らなければ探索は
    /// `bigram` を使い、接続コストを (左, 右) の組で覚える。
    fn uses_left2(&self) -> bool {
        false
    }
}
