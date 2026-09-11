//! セグメント列に対する候補の格子と N-best 探索。

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

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

/// 途中経路を残す本数の下限。同点や後段の逆転に備え、要求数が少なくても
/// これだけは残す。
pub const BEAM: usize = 20;

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
        self.search(scorer).nbest(top_n)
    }

    /// `constraint` を満たす経路に限って `nbest` と同じ探索をする。
    pub fn nbest_constrained(
        &self,
        scorer: &dyn Scorer,
        top_n: usize,
        constraint: &Constraint,
    ) -> Vec<Path> {
        self.search(scorer).nbest_constrained(top_n, constraint)
    }

    /// この格子を `scorer` で探索する場を作る。同じ格子を制約を変えて何度も
    /// 探索するなら、1 つの場で行うとスコアラーを引き直さない。
    pub fn search<'a>(&'a self, scorer: &'a dyn Scorer) -> Search<'a> {
        Search::new(self, scorer)
    }
}

/// 文頭・文末を表す表層形の番号。
const EDGE: u32 = u32::MAX;

/// 1 つの格子に対する探索の場。表層形を番号で扱い、スコアラーの結果を
/// 番号で覚える。1 セグメントごとに候補数 × ビーム幅の接続コストを引くので、
/// 文字列を鍵にすると引く時間が探索の時間を決めてしまう。
pub struct Search<'a> {
    lattice: &'a Lattice,
    scorer: &'a dyn Scorer,
    /// 候補ごとの表層形の番号。同じ表層形は同じ番号。
    ids: Vec<Vec<u32>>,
    /// 番号ごとの表層形。
    surfaces: Vec<&'a str>,
    /// 候補ごとの unigram に順位とかなのコストを足したもの。
    unigram: Vec<Vec<f64>>,
    /// (左の番号, 右の番号) → 接続コスト。文頭・文末は `EDGE`。
    bigram: RefCell<HashMap<(u32, u32), f64, BuildHasherDefault<IdHasher>>>,
    /// 候補が消費するセグメント数の最大。
    max_span: usize,
}

impl<'a> Search<'a> {
    fn new(lattice: &'a Lattice, scorer: &'a dyn Scorer) -> Search<'a> {
        let mut index: HashMap<&str, u32> = HashMap::new();
        let mut surfaces: Vec<&'a str> = Vec::new();
        let mut ids = Vec::with_capacity(lattice.candidates.len());
        let mut unigram = Vec::with_capacity(lattice.candidates.len());
        let mut max_span = 1;
        for cands in &lattice.candidates {
            let mut pos_ids = Vec::with_capacity(cands.len());
            let mut pos_uni = Vec::with_capacity(cands.len());
            for cand in cands {
                let id = *index.entry(cand.surface.as_str()).or_insert_with(|| {
                    surfaces.push(cand.surface.as_str());
                    (surfaces.len() - 1) as u32
                });
                pos_ids.push(id);
                pos_uni.push(
                    scorer.unigram(&cand.surface)
                        + RANK_WEIGHT * (1.0 + cand.rank as f64).ln()
                        + if cand.kana { KANA_PENALTY } else { 0.0 },
                );
                max_span = max_span.max(cand.span);
            }
            ids.push(pos_ids);
            unigram.push(pos_uni);
        }
        Search {
            lattice,
            scorer,
            ids,
            surfaces,
            unigram,
            bigram: RefCell::new(HashMap::default()),
            max_span,
        }
    }

    /// コストの小さい順に最大 `top_n` 本の経路をコスト付きで返す。
    pub fn nbest(&self, top_n: usize) -> Vec<Path> {
        self.nbest_constrained(top_n, &Constraint::default())
    }

    /// `constraint` を満たす経路に限って `nbest` と同じ探索をする。
    ///
    /// 途中の経路は親を指す節として持ち、表層形の列は最後に組み立てる。
    /// 1 セグメントごとに候補数 × ビーム幅の経路を作るので、そのたびに
    /// 表層形の列を複製すると割り当てが探索の時間を決めてしまう。
    pub fn nbest_constrained(&self, top_n: usize, constraint: &Constraint) -> Vec<Path> {
        let n = self.lattice.candidates.len();
        let beam = top_n.max(BEAM);
        let mut nodes = vec![Node {
            cost: 0.0,
            len: 0,
            parent: 0,
            pos: 0,
            cand: 0,
            id: EDGE,
        }];
        let mut paths: Vec<Vec<usize>> = vec![Vec::new(); n + 1];
        paths[0].push(0);

        for pos in 0..n {
            let incoming = std::mem::take(&mut paths[pos]);
            for (ci, cand) in self.lattice.candidates[pos].iter().enumerate() {
                let next = pos + cand.span;
                if next > n {
                    continue;
                }
                let uni = self.unigram[pos][ci];
                let id = self.ids[pos][ci];
                for &node in &incoming {
                    let len = nodes[node].len;
                    if !constraint.allows(len, &cand.surface) {
                        continue;
                    }
                    let cost = nodes[node].cost + uni + self.bigram(nodes[node].id, id);
                    nodes.push(Node {
                        cost,
                        len: len + 1,
                        parent: node,
                        pos,
                        cand: ci,
                        id,
                    });
                    paths[next].push(nodes.len() - 1);
                }
            }
            // この位置から届く先だけに経路が増えている。
            for p in &mut paths[pos + 1..(pos + self.max_span + 1).min(n + 1)] {
                prune(&nodes, p, beam);
            }
        }

        let mut finals = std::mem::take(&mut paths[n]);
        for &node in &finals {
            let extra = self.bigram(nodes[node].id, EDGE);
            nodes[node].cost += extra;
        }
        prune(&nodes, &mut finals, top_n);
        finals
            .into_iter()
            .map(|node| self.materialize(&nodes, node))
            .collect()
    }

    /// 表層形 `left` から `right` への接続コスト。`EDGE` は文頭・文末。
    fn bigram(&self, left: u32, right: u32) -> f64 {
        if let Some(&c) = self.bigram.borrow().get(&(left, right)) {
            return c;
        }
        let c = self.scorer.bigram(self.surface(left), self.surface(right));
        self.bigram.borrow_mut().insert((left, right), c);
        c
    }

    /// 番号の表層形。`EDGE` なら `None`。
    fn surface(&self, id: u32) -> Option<&'a str> {
        (id != EDGE).then(|| self.surfaces[id as usize])
    }

    fn materialize(&self, nodes: &[Node], node: usize) -> Path {
        let mut surfaces = Vec::with_capacity(nodes[node].len);
        let mut spans = Vec::with_capacity(nodes[node].len);
        let mut cur = node;
        while nodes[cur].len > 0 {
            let c = &self.lattice.candidates[nodes[cur].pos][nodes[cur].cand];
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

/// 節から根までの表層形が同じか。
fn same_surfaces(nodes: &[Node], a: usize, b: usize) -> bool {
    let (mut a, mut b) = (a, b);
    if nodes[a].len != nodes[b].len {
        return false;
    }
    while nodes[a].len > 0 {
        if nodes[a].id != nodes[b].id {
            return false;
        }
        a = nodes[a].parent;
        b = nodes[b].parent;
    }
    true
}

/// コストの小さい順に並べ、表層形の同じ経路を 1 本にまとめて `keep` 本残す。
///
/// 経路は同じコストなら節の番号が小さいほうを先にする。節の番号は作った順で、
/// 途中で並べ替えても同じコストの相対順は変わらないので、安定ソートと同じ順になる。
/// 1 セグメントごとに候補数 × ビーム幅の経路が入るので、全部を並べ替えずに
/// 小さいほうから余裕を持って取り出し、まとめた後に足りなければ全部を並べ替える。
fn prune(nodes: &[Node], paths: &mut Vec<usize>, keep: usize) {
    let by = |&a: &usize, &b: &usize| nodes[a].cost.total_cmp(&nodes[b].cost).then(a.cmp(&b));
    let margin = keep * 2;
    if paths.len() > margin {
        paths.select_nth_unstable_by(margin - 1, by);
        let mut top = paths[..margin].to_vec();
        top.sort_unstable_by(by);
        top.dedup_by(|&mut a, &mut b| same_surfaces(nodes, a, b));
        if top.len() >= keep {
            top.truncate(keep);
            *paths = top;
            return;
        }
    }
    paths.sort_unstable_by(by);
    paths.dedup_by(|&mut a, &mut b| same_surfaces(nodes, a, b));
    paths.truncate(keep);
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
    /// 最後の候補の表層形の番号。根では `EDGE`。
    id: u32,
}

/// 表層形の番号の組を鍵にする軽いハッシュ。SipHash は短い鍵には重い。
#[derive(Default)]
struct IdHasher(u64);

impl Hasher for IdHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u64(u64::from(b));
        }
    }

    fn write_u32(&mut self, v: u32) {
        self.write_u64(u64::from(v));
    }

    fn write_u64(&mut self, v: u64) {
        self.0 = (self.0.rotate_left(29) ^ v).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

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

    #[test]
    fn 刈り込みは同じコストなら先に作った経路を残す() {
        // 全候補のコストが同じ格子。ビーム幅より多い経路ができ、残るのは
        // 作成順（2 番目の候補が外側、1 番目の経路が内側）の先頭になる。
        let cands: Vec<Candidate> = (0..30)
            .map(|i| Candidate {
                surface: format!("候補{i}"),
                span: 1,
                rank: 0,
                kana: false,
            })
            .collect();
        let lattice = Lattice {
            candidates: vec![cands.clone(), cands],
        };
        let scorer = MapScorer {
            uni: HashMap::new(),
            bi: HashMap::new(),
        };
        let result = lattice.nbest(&scorer, 5);
        let want: Vec<Vec<String>> = (0..5)
            .map(|i| vec![format!("候補{i}"), "候補0".to_string()])
            .collect();
        let got: Vec<Vec<String>> = result.into_iter().map(|p| p.surfaces).collect();
        assert_eq!(got, want);
    }

    #[test]
    fn 同じ表層形の経路は_1_本にまとめる() {
        // 位置 0 に同じ表層形の候補が 2 つあると、同じ表層形の列が 2 本できる。
        let lattice = Lattice {
            candidates: vec![vec![cand("猫", 1), cand("猫", 1)], vec![cand("だ", 1)]],
        };
        let scorer = MapScorer {
            uni: HashMap::new(),
            bi: HashMap::new(),
        };
        let result = lattice.nbest(&scorer, 5);
        assert_eq!(result.len(), 1);
    }

    /// 呼ばれた回数を数えるスコアラー。
    struct Counting(Cell<usize>);
    impl Scorer for Counting {
        fn unigram(&self, _: &str) -> f64 {
            self.0.set(self.0.get() + 1);
            1.0
        }
        fn bigram(&self, _: Option<&str>, _: Option<&str>) -> f64 {
            self.0.set(self.0.get() + 1);
            1.0
        }
    }

    #[test]
    fn 同じ場で制約を変えて探索してもスコアラーを引き直さない() {
        let lattice = Lattice {
            candidates: vec![
                vec![cand("猫", 1), cand("ねこ", 1)],
                vec![cand("だ", 1), cand("である", 1)],
            ],
        };
        let scorer = Counting(Cell::new(0));
        let search = lattice.search(&scorer);
        let first = search.nbest(5);
        let calls = scorer.0.get();
        assert!(calls > 0);
        let constraint = Constraint {
            fixed: vec!["ねこ".into()],
            next_prefix: String::new(),
        };
        let second = search.nbest_constrained(5, &constraint);
        assert_eq!(scorer.0.get(), calls);
        assert_eq!(second[0].surfaces, ["ねこ", "だ"]);
        assert_eq!(first.len(), 4);
    }
}
