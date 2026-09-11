//! 1 回の変換の間、入力側の状態と読んだ候補の節を持ち回る採点の場。
//!
//! 候補は trie にまとめ、節ごとに「そこまでを読み終えた位置の出力」を持つ。
//! 投機的な変換は同じ入力に対して採点を何度も繰り返し、後の回の候補は前の回の
//! 下書きと接頭辞を共有するので、trie を呼び出しをまたいで残し、既に読んだ節は
//! 読み直さない。状態は連鎖の終わりだけ持ち、途中の節から分岐するときは
//! 最も近い状態のある祖先から係数で漸化式を回し直して作る。

use crate::infer::{Chain, Infer};
use crate::trie::Trie;
use crate::vocab::EOS;

pub struct Session<'a> {
    infer: &'a Infer,
    trie: Trie,
    /// 節ごとの最終層の正規化済み出力（d_model）。根は入力側の最後の位置。
    hidden: Vec<f32>,
    /// 節ごとの logits の log-sum-exp。
    lse: Vec<f32>,
    /// 節ごとの各層の係数（n_layers × proj_width）。根の分は使わないので 0。
    proj: Vec<f32>,
    /// 節ごとの読み終えた状態。連鎖の終わりと分岐点だけ持つ。
    states: Vec<Option<Vec<f32>>>,
}

impl<'a> Session<'a> {
    /// 入力側の id 列（BOS から）を読んで場を開く。
    pub fn open(infer: &'a Infer, prefix: &[u32]) -> Session<'a> {
        Session::open_from(infer, &infer.init_state(), prefix).0
    }

    /// `state` の続きとして入力側の `ids` を読んで場を開く。読んだ各位置の
    /// 係数（位置 × n_layers × proj_width）も返し、続きを読むための状態を
    /// 後から作れるようにする。
    pub fn open_from(infer: &'a Infer, state: &[f32], ids: &[u32]) -> (Session<'a>, Vec<f32>) {
        let read = infer.read(&[Chain { state, ids }]);
        let d = infer.config().d_model;
        let last = ids.len() - 1;
        let session = Session {
            infer,
            trie: Trie::new(),
            hidden: read.hidden[last * d..(last + 1) * d].to_vec(),
            lse: vec![read.lse[last]],
            proj: vec![0.0; infer.config().n_layers * infer.proj_width()],
            states: vec![Some(read.states.into_iter().next().unwrap())],
        };
        (session, read.proj)
    }

    /// 候補を読み、各候補が通る節（根を除く、候補と同じ長さ）を返す。
    /// 既に読んだ節は読み直さない。
    pub fn read(&mut self, seqs: &[Vec<u32>]) -> Vec<Vec<usize>> {
        let old_len = self.trie.len();
        let paths: Vec<Vec<usize>> = seqs.iter().map(|s| self.trie.insert(s)).collect();
        let n = self.trie.len();
        let d = self.infer.config().d_model;
        let stride = self.infer.config().n_layers * self.infer.proj_width();
        self.hidden.resize(n * d, 0.0);
        self.lse.resize(n, 0.0);
        self.proj.resize(n * stride, 0.0);
        self.states.resize(n, None);

        // 新しい節のうち親が古い節のものから連鎖を始め、分岐ごとに回を分ける。
        let mut starts: Vec<usize> = (old_len..n)
            .filter(|&node| self.trie.parent(node) < old_len)
            .collect();
        for &start in &starts {
            self.ensure_state(self.trie.parent(start));
        }
        while !starts.is_empty() {
            let mut chains: Vec<Vec<usize>> = Vec::new();
            let mut next = Vec::new();
            for start in starts {
                let mut nodes = vec![start];
                while self.trie.children(*nodes.last().unwrap()).len() == 1 {
                    nodes.push(self.trie.children(*nodes.last().unwrap())[0]);
                }
                next.extend_from_slice(self.trie.children(*nodes.last().unwrap()));
                chains.push(nodes);
            }
            let ids: Vec<Vec<u32>> = chains
                .iter()
                .map(|c| c.iter().map(|&node| self.trie.id(node)).collect())
                .collect();
            let batch: Vec<Chain> = chains
                .iter()
                .zip(&ids)
                .map(|(c, ids)| Chain {
                    state: self.states[self.trie.parent(c[0])].as_deref().unwrap(),
                    ids,
                })
                .collect();
            let read = self.infer.read(&batch);
            let mut r = 0;
            for (c, state) in chains.iter().zip(read.states) {
                for &node in c {
                    self.hidden[node * d..(node + 1) * d]
                        .copy_from_slice(&read.hidden[r * d..(r + 1) * d]);
                    self.lse[node] = read.lse[r];
                    self.proj[node * stride..(node + 1) * stride]
                        .copy_from_slice(&read.proj[r * stride..(r + 1) * stride]);
                    r += 1;
                }
                self.states[*c.last().unwrap()] = Some(state);
            }
            starts = next;
        }
        paths
    }

    /// `node` の状態を、無ければ最も近い状態のある祖先から作る。
    fn ensure_state(&mut self, node: usize) {
        if self.states[node].is_some() {
            return;
        }
        let mut path = vec![node];
        let mut cur = node;
        while self.states[cur].is_none() {
            cur = self.trie.parent(cur);
            path.push(cur);
        }
        let stride = self.infer.config().n_layers * self.infer.proj_width();
        let mut state = self.states[cur].clone().unwrap();
        for &n in path[..path.len() - 1].iter().rev() {
            self.infer
                .rescan(&mut state, &self.proj[n * stride..(n + 1) * stride]);
        }
        self.states[node] = Some(state);
    }

    /// 節 `node` まで読んだ次に `id` が来る対数確率。
    pub fn log_prob(&self, node: usize, id: u32) -> f32 {
        let d = self.infer.config().d_model;
        self.infer
            .log_prob(&self.hidden[node * d..(node + 1) * d], self.lse[node], id)
    }

    /// 候補（`read` が返した節の列と id 列）の EOS までの負の対数尤度。
    pub fn nll(&self, path: &[usize], ids: &[u32]) -> f64 {
        self.nll_counted(path, ids, &vec![true; ids.len()])
    }

    /// `counted[i]` が立つ id と EOS だけを数えた負の対数尤度。読みを挟んだ列で
    /// 出力側だけを数えるのに使う。
    pub fn nll_counted(&self, path: &[usize], ids: &[u32], counted: &[bool]) -> f64 {
        let mut node = 0;
        let mut total = 0.0;
        for ((&next, &id), &counted) in path.iter().zip(ids).zip(counted) {
            if counted {
                total -= f64::from(self.log_prob(node, id));
            }
            node = next;
        }
        total - f64::from(self.log_prob(node, EOS))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ModelConfig;
    use crate::testing::random_weights;

    fn config() -> ModelConfig {
        ModelConfig {
            vocab_size: 10,
            d_model: 16,
            n_layers: 2,
            n_heads: 2,
            head_dim: 8,
            d_state: 4,
            mlp_dim: 32,
        }
    }

    fn infer() -> Infer {
        Infer::new(config(), &random_weights(&config(), 1)).unwrap()
    }

    /// 接頭辞と候補を 1 本の連鎖として読んだ EOS までの負の対数尤度。
    fn reference_nll(infer: &Infer, prefix: &[u32], ids: &[u32]) -> f64 {
        let whole: Vec<u32> = prefix.iter().chain(ids).copied().collect();
        let read = infer.read(&[Chain {
            state: &infer.init_state(),
            ids: &whole,
        }]);
        let d = infer.config().d_model;
        let mut total = 0.0;
        for (i, &id) in ids.iter().chain(std::iter::once(&EOS)).enumerate() {
            let pos = prefix.len() - 1 + i;
            total -=
                f64::from(infer.log_prob(&read.hidden[pos * d..(pos + 1) * d], read.lse[pos], id));
        }
        total
    }

    #[test]
    fn 候補の負の対数尤度は一本の連鎖で読んだのと同じ() {
        let infer = infer();
        let prefix = [0u32, 3, 4];
        let mut s = Session::open(&infer, &prefix);
        let seqs = vec![vec![5u32, 6, 7], vec![5, 6, 8, 9], vec![7]];
        let paths = s.read(&seqs);
        for (path, ids) in paths.iter().zip(&seqs) {
            let got = s.nll(path, ids);
            let want = reference_nll(&infer, &prefix, ids);
            assert!((got - want).abs() < 1e-3, "got={got} want={want}");
        }
    }

    #[test]
    fn 途中の状態から続きを読んで開いても最初から読んだのと同じ() {
        let infer = infer();
        let whole = [0u32, 3, 4, 5, 1];
        let (_, proj) = Session::open_from(&infer, &infer.init_state(), &whole);
        let stride = infer.config().n_layers * infer.proj_width();
        // 先頭 3 位置の係数から状態を作り、残りを続きとして読む。
        let mut state = infer.init_state();
        infer.rescan(&mut state, &proj[..3 * stride]);
        let (resumed, _) = Session::open_from(&infer, &state, &whole[3..]);
        let fresh = Session::open(&infer, &whole);
        assert_eq!(resumed.hidden, fresh.hidden);
        assert_eq!(resumed.lse, fresh.lse);
        assert_eq!(resumed.states, fresh.states);
    }

    #[test]
    fn 後の呼び出しで途中の節から分岐しても同じ() {
        let infer = infer();
        let prefix = [0u32, 3];
        let mut s = Session::open(&infer, &prefix);
        s.read(&[vec![5u32, 6, 7, 8, 9]]);
        // 5,6 まで共有して 7 の代わりに 4 を置く。5,6 の節は連鎖の途中で状態が無い。
        let seqs = vec![vec![5u32, 6, 4, 8], vec![5, 6, 7, 8, 3], vec![6, 6]];
        let paths = s.read(&seqs);
        for (path, ids) in paths.iter().zip(&seqs) {
            let got = s.nll(path, ids);
            let want = reference_nll(&infer, &prefix, ids);
            assert!((got - want).abs() < 1e-3, "got={got} want={want}");
        }
    }

    #[test]
    fn 読んだ節は読み直さない() {
        let infer = infer();
        let mut s = Session::open(&infer, &[0u32, 3]);
        let first = s.read(&[vec![5u32, 6, 7]]);
        let hidden_before = s.hidden.clone();
        let second = s.read(&[vec![5u32, 6, 7], vec![5, 6, 9]]);
        assert_eq!(first[0], second[0]);
        assert_eq!(s.hidden[..hidden_before.len()], hidden_before[..]);
    }
}
