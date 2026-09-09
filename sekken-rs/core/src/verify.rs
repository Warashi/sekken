//! 文の途中で「次の字」を問える採点器。投機的な変換で、下書きの各位置に
//! 別の字を置いたときの良さを 1 回の採点で得るために使う。
//!
//! 投機的な変換は同じ入力に対して採点を繰り返し、後の回の文は前の回の下書きと
//! 接頭辞を共有する。1 回の変換の間だけ生きる `VerifySession` に読んだ分を
//! 持たせ、採点器が読み直しを省けるようにする。

/// 1 文を採点した結果。
#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    /// 文全体のコスト（小さいほど良い）。
    pub cost: f64,
    /// 問い合わせごとのコスト。`verify` の `queries` と同じ順。
    pub char_costs: Vec<f64>,
}

/// 文全体のコストと、任意の位置に別の字を置いたときのコストを返す採点器。
pub trait Verifier {
    /// 1 回の変換の採点を始める。`input` は変換前のローマ字入力で、
    /// 入力を条件にする採点器だけが使う。
    fn begin<'a>(&'a self, input: &str) -> Box<dyn VerifySession + 'a>;
}

/// 1 回の変換の間の採点。同じ入力に対する文だけを渡す。
pub trait VerifySession {
    /// `sentences` をまとめて採点する。`queries[i]` の各 `(pos, ch)` は
    /// 「`sentences[i]` の先頭 `pos` 文字の次に `ch` が来る」ことのコストを問う。
    /// `pos` は文の長さまで取れる。
    fn verify(&mut self, sentences: &[String], queries: &[Vec<(usize, char)>]) -> Vec<Verdict>;
}

/// 状態を持たない検証器をそのまま `VerifySession` にする。
pub struct Stateless<F>(pub F);

impl<F> VerifySession for Stateless<F>
where
    F: Fn(&[String], &[Vec<(usize, char)>]) -> Vec<Verdict>,
{
    fn verify(&mut self, sentences: &[String], queries: &[Vec<(usize, char)>]) -> Vec<Verdict> {
        (self.0)(sentences, queries)
    }
}
