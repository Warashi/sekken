//! セグメント列に対する候補の格子と N-best 探索。

use crate::candidates::Candidate;
use crate::scorer::Scorer;

/// 辞書での候補順位に掛けるコストの重み。SKK 辞書の並びは大まかな
/// 頻度順なので、モデルが知らない語同士の順位付けに使う。
pub const RANK_WEIGHT: f64 = 1.0;

/// 読みをそのままかなにした候補に加えるコスト。大文字で始めた
/// セグメントは「ここを変換したい」という指示なので、かなのままは
/// 辞書候補より不利にする。0 / 1 / 2 / 4 を比べ、top5 まで含めて
/// 最も良かった 2 にした。
pub const KANA_PENALTY: f64 = 2.0;

/// 各セグメント位置から始まる候補の一覧。
pub struct Lattice {
    pub candidates: Vec<Vec<Candidate>>,
}

/// N-best 探索で得た 1 本の経路。
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    pub cost: f64,
    pub surfaces: Vec<String>,
    /// `surfaces` の各候補が消費したセグメント数。
    pub spans: Vec<usize>,
}

/// 探索する経路の先頭を縛る制約。
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Constraint {
    /// 先頭から順にこの表層形を選んだ経路だけを残す。
    pub fixed: Vec<String>,
    /// `fixed` の直後の候補に要求する先頭の文字列。
    pub next_prefix: String,
}

impl Constraint {
    /// `len` 個選んだ経路が次に `surface` を選ぶのを許すか。固定した候補は
    /// 選んだ時点で照合済みなので、経路の長さと次の候補だけで決まる。
    /// span ではなく表層形で照合するので、同じ位置に同じ表層形で span の違う
    /// 候補があると固定した経路と別の切り方を通りうる。`Path` の同一性も
    /// 表層形で決めており（`prune` の dedup）、そこまで区別する必要が無い。
    fn allows(&self, len: usize, surface: &str) -> bool {
        match self.fixed.get(len) {
            Some(fixed) => fixed == surface,
            None if len == self.fixed.len() => surface.starts_with(&self.next_prefix),
            None => true,
        }
    }
}

impl Lattice {
    /// コストの小さい順に最大 `top_n` 本の経路をコスト付きで返す。
    pub fn nbest(&self, scorer: &dyn Scorer, top_n: usize) -> Vec<Path> {
        self.nbest_constrained(scorer, top_n, &Constraint::default())
    }

    /// `constraint` を満たす経路に限って `nbest` と同じ探索をする。
    ///
    /// 途中の経路は親を指す節として持ち、表層形の列は最後に組み立てる。
    /// 1 セグメントごとに候補数 × ビーム幅の経路を作るので、そのたびに
    /// 表層形の列を複製すると割り当てが探索の時間を決めてしまう。
    pub fn nbest_constrained(
        &self,
        scorer: &dyn Scorer,
        top_n: usize,
        constraint: &Constraint,
    ) -> Vec<Path> {
        let n = self.candidates.len();
        // 同点や後段の逆転に備え、途中経路は要求数より多めに残す。
        let beam = top_n.max(20);
        let mut nodes = vec![Node {
            cost: 0.0,
            len: 0,
            parent: 0,
            pos: 0,
            cand: 0,
        }];
        let mut paths: Vec<Vec<usize>> = vec![Vec::new(); n + 1];
        paths[0].push(0);

        for pos in 0..n {
            let incoming = std::mem::take(&mut paths[pos]);
            for (ci, cand) in self.candidates[pos].iter().enumerate() {
                let next = pos + cand.span;
                if next > n {
                    continue;
                }
                let uni = scorer.unigram(&cand.surface)
                    + RANK_WEIGHT * (1.0 + cand.rank as f64).ln()
                    + if cand.kana { KANA_PENALTY } else { 0.0 };
                for &node in &incoming {
                    let len = nodes[node].len;
                    if !constraint.allows(len, &cand.surface) {
                        continue;
                    }
                    let left = self.last_surface(&nodes, node);
                    let cost = nodes[node].cost + uni + scorer.bigram(left, Some(&cand.surface));
                    nodes.push(Node {
                        cost,
                        len: len + 1,
                        parent: node,
                        pos,
                        cand: ci,
                    });
                    paths[next].push(nodes.len() - 1);
                }
            }
            for p in paths.iter_mut().skip(pos + 1) {
                self.prune(&nodes, p, beam);
            }
        }

        let mut finals = std::mem::take(&mut paths[n]);
        for &node in &finals {
            let extra = scorer.bigram(self.last_surface(&nodes, node), None);
            nodes[node].cost += extra;
        }
        self.prune(&nodes, &mut finals, top_n);
        finals
            .into_iter()
            .map(|node| self.materialize(&nodes, node))
            .collect()
    }

    /// 節の最後の候補の表層形。根なら `None`（文頭）。
    fn last_surface<'a>(&'a self, nodes: &[Node], node: usize) -> Option<&'a str> {
        let n = &nodes[node];
        (n.len > 0).then(|| self.candidates[n.pos][n.cand].surface.as_str())
    }

    /// 節から根までの表層形が同じか。
    fn same_surfaces(&self, nodes: &[Node], a: usize, b: usize) -> bool {
        let (mut a, mut b) = (a, b);
        if nodes[a].len != nodes[b].len {
            return false;
        }
        while nodes[a].len > 0 {
            if self.last_surface(nodes, a) != self.last_surface(nodes, b) {
                return false;
            }
            a = nodes[a].parent;
            b = nodes[b].parent;
        }
        true
    }

    fn prune(&self, nodes: &[Node], paths: &mut Vec<usize>, keep: usize) {
        paths.sort_by(|&a, &b| nodes[a].cost.total_cmp(&nodes[b].cost));
        paths.dedup_by(|&mut a, &mut b| self.same_surfaces(nodes, a, b));
        paths.truncate(keep);
    }

    fn materialize(&self, nodes: &[Node], node: usize) -> Path {
        let mut surfaces = Vec::with_capacity(nodes[node].len);
        let mut spans = Vec::with_capacity(nodes[node].len);
        let mut cur = node;
        while nodes[cur].len > 0 {
            let c = &self.candidates[nodes[cur].pos][nodes[cur].cand];
            surfaces.push(c.surface.clone());
            spans.push(c.span);
            cur = nodes[cur].parent;
        }
        surfaces.reverse();
        spans.reverse();
        Path {
            cost: nodes[node].cost,
            surfaces,
            spans,
        }
    }
}

/// 探索の途中の経路。親を辿ると表層形の列になる。
struct Node {
    cost: f64,
    /// 根からの候補の数。
    len: usize,
    parent: usize,
    /// 最後の候補の位置と番号。根では使わない。
    pos: usize,
    cand: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct MapScorer {
        uni: HashMap<&'static str, f64>,
        bi: HashMap<(&'static str, &'static str), f64>,
    }

    impl Scorer for MapScorer {
        fn unigram(&self, s: &str) -> f64 {
            *self.uni.get(s).unwrap_or(&10.0)
        }
        fn bigram(&self, l: Option<&str>, r: Option<&str>) -> f64 {
            *self
                .bi
                .get(&(l.unwrap_or(""), r.unwrap_or("")))
                .unwrap_or(&0.0)
        }
    }

    fn cand(s: &str, span: usize) -> Candidate {
        Candidate {
            surface: s.to_string(),
            span,
            rank: 0,
            kana: false,
        }
    }

    #[test]
    fn unigram_の小さい候補列を先頭に返す() {
        let lattice = Lattice {
            candidates: vec![
                vec![cand("猫", 1), cand("ねこ", 1)],
                vec![cand("である", 1), cand("デアル", 1)],
            ],
        };
        let scorer = MapScorer {
            uni: HashMap::from([("猫", 1.0), ("ねこ", 3.0), ("である", 1.0)]),
            bi: HashMap::new(),
        };
        let result = lattice.nbest(&scorer, 2);
        assert_eq!(result[0].surfaces, ["猫", "である"]);
        assert_eq!(result[1].surfaces, ["ねこ", "である"]);
        assert!(result[0].cost < result[1].cost);
    }

    #[test]
    fn 経路のコストは_unigram_と接続コストの和() {
        let lattice = Lattice {
            candidates: vec![vec![cand("書", 1)], vec![cand("く", 1)]],
        };
        let scorer = MapScorer {
            uni: HashMap::from([("書", 1.0), ("く", 2.0)]),
            bi: HashMap::from([(("書", "く"), 0.5), (("く", ""), 0.25)]),
        };
        let result = lattice.nbest(&scorer, 1);
        assert_eq!(result[0].cost, 1.0 + 2.0 + 0.5 + 0.25);
    }

    #[test]
    fn 接続コストで順位が入れ替わる() {
        let lattice = Lattice {
            candidates: vec![vec![cand("書", 1), cand("か", 1)], vec![cand("く", 1)]],
        };
        let scorer = MapScorer {
            uni: HashMap::from([("書", 1.0), ("か", 1.0), ("く", 1.0)]),
            bi: HashMap::from([(("書", "く"), 5.0), (("か", "く"), 0.5)]),
        };
        assert_eq!(lattice.nbest(&scorer, 1)[0].surfaces, ["か", "く"]);
    }

    #[test]
    fn 同じコストなら辞書の順位が高い候補を先にする() {
        let lattice = Lattice {
            candidates: vec![vec![
                Candidate {
                    surface: "法家".into(),
                    span: 1,
                    rank: 1,
                    kana: false,
                },
                Candidate {
                    surface: "法貨".into(),
                    span: 1,
                    rank: 0,
                    kana: false,
                },
            ]],
        };
        let scorer = MapScorer {
            uni: HashMap::new(),
            bi: HashMap::new(),
        };
        assert_eq!(lattice.nbest(&scorer, 1)[0].surfaces, ["法貨"]);
    }

    #[test]
    fn 固定した候補を通る経路だけを返す() {
        let lattice = Lattice {
            candidates: vec![
                vec![cand("猫", 1), cand("ねこ", 1)],
                vec![cand("である", 1), cand("デアル", 1)],
            ],
        };
        let scorer = MapScorer {
            uni: HashMap::from([("猫", 1.0), ("ねこ", 3.0), ("である", 1.0)]),
            bi: HashMap::new(),
        };
        let constraint = Constraint {
            fixed: vec!["ねこ".into()],
            next_prefix: String::new(),
        };
        let result = lattice.nbest_constrained(&scorer, 5, &constraint);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].surfaces, ["ねこ", "である"]);
        assert_eq!(result[1].surfaces, ["ねこ", "デアル"]);
    }

    #[test]
    fn 固定の直後は指定した文字列で始まる候補だけを通す() {
        let lattice = Lattice {
            candidates: vec![
                vec![cand("猫", 1)],
                vec![cand("である", 1), cand("デアル", 1), cand("だ", 1)],
            ],
        };
        let scorer = MapScorer {
            uni: HashMap::from([("である", 1.0), ("デアル", 2.0), ("だ", 3.0)]),
            bi: HashMap::new(),
        };
        let constraint = Constraint {
            fixed: vec!["猫".into()],
            next_prefix: "デ".into(),
        };
        let result = lattice.nbest_constrained(&scorer, 5, &constraint);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].surfaces, ["猫", "デアル"]);
    }

    #[test]
    fn 固定の先のセグメントは自由に選ぶ() {
        let lattice = Lattice {
            candidates: vec![
                vec![cand("猫", 1)],
                vec![cand("が", 1)],
                vec![cand("鳴く", 1), cand("泣く", 1)],
            ],
        };
        let scorer = MapScorer {
            uni: HashMap::from([("鳴く", 2.0), ("泣く", 1.0)]),
            bi: HashMap::new(),
        };
        let constraint = Constraint {
            fixed: vec!["猫".into()],
            next_prefix: "が".into(),
        };
        let result = lattice.nbest_constrained(&scorer, 5, &constraint);
        assert_eq!(result[0].surfaces, ["猫", "が", "泣く"]);
        assert_eq!(result[1].surfaces, ["猫", "が", "鳴く"]);
    }

    #[test]
    fn span_2_の候補は次のセグメントを飛ばす() {
        let lattice = Lattice {
            candidates: vec![vec![cand("書く", 2), cand("か", 1)], vec![cand("く", 1)]],
        };
        let scorer = MapScorer {
            uni: HashMap::from([("書く", 1.0), ("か", 1.0), ("く", 1.0)]),
            bi: HashMap::new(),
        };
        let result = lattice.nbest(&scorer, 5);
        assert_eq!(result[0].surfaces, ["書く"]);
        assert_eq!(result[0].spans, [2]);
        assert_eq!(result[1].surfaces, ["か", "く"]);
        assert_eq!(result[1].spans, [1, 1]);
        assert_eq!(result.len(), 2);
    }
}
