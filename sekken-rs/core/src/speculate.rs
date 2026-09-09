//! 検証器の提案で格子を再探索する投機的な変換。
//!
//! 格子の先頭候補を下書きにし、検証器に各位置で「格子が到達できる別の字」の
//! 良さを問う。下書きの字との差が最も大きい位置までを固定し、その字で
//! 始まる候補に絞って格子を再探索した上位 `width` 本をまとめて採点する。
//! 合成コストが改善したときだけ下書きを置き換える。N-best の並べ替えと
//! 違い、格子の上位に無い経路にも届く。
//!
//! 最初に食い違った位置で固定していくと、入力を見ない言語モデルの
//! 1 文字分の先入観（「なお、」の次は「枚」より「マ」）に引きずられて
//! 戻れなくなるので、固定は累積させず、試した提案は記録して繰り返さない。

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::lattice::{Constraint, Lattice, Path};
use crate::scorer::Scorer;
use crate::verify::{Verdict, Verifier, VerifySession};

pub struct Speculator {
    pub verifier: Box<dyn Verifier>,
    /// 検証器のコストに掛ける重み。格子のコストは 1。
    pub weight: f64,
    /// 提案と再探索を繰り返す上限。0 なら格子の先頭候補をそのまま返す。
    pub rounds: usize,
    /// 提案の制約で再探索して採点する経路の本数。
    pub width: usize,
}

/// 下書きの 1 文字位置と、そこに置ける別の字。
struct Site {
    /// 下書きの何番目の候補か。
    segment: usize,
    /// その候補の何文字目か。
    offset: usize,
    /// 下書きの字の問い合わせ番号。
    draft: usize,
    /// (別の字, その問い合わせ番号)。
    alternatives: Vec<(char, usize)>,
}

/// 採点済みの経路。`cost` は格子と検証器の合成コスト。
struct Scored {
    path: Path,
    sites: Vec<Site>,
    verdict: Verdict,
}

impl Speculator {
    /// 反復で得た経路を合成コストの小さい順に返す。`input` は変換前のローマ字入力、
    /// `head` は格子の外で文頭に付くかなで、検証器に渡す文に前置する。
    pub fn decode(
        &self,
        lattice: &Lattice,
        scorer: &dyn Scorer,
        input: &str,
        head: &str,
    ) -> Vec<Path> {
        let head_len = head.chars().count();
        let mut seen: HashSet<Vec<String>> = HashSet::new();
        let mut tried: HashSet<Constraint> = HashSet::new();
        let mut results: Vec<Path> = Vec::new();
        let mut best: Option<Scored> = None;
        let mut constraint = Constraint::default();
        // 制約を変えて同じ格子を何度も探索するので、格子側のコストは覚えておく。
        let scorer = Memo::new(scorer);
        let scorer = &scorer;
        let mut session = self.verifier.begin(input);
        for _ in 0..=self.rounds {
            let paths: Vec<Path> = lattice
                .nbest_constrained(scorer, self.width, &constraint)
                .into_iter()
                .filter(|p| seen.insert(p.surfaces.clone()))
                .collect();
            if !paths.is_empty() {
                let scored = self.score(lattice, &mut *session, head, head_len, paths);
                for s in scored {
                    let cost = s.path.cost + self.weight * s.verdict.cost;
                    results.push(Path {
                        cost,
                        surfaces: s.path.surfaces.clone(),
                        spans: s.path.spans.clone(),
                    });
                    if best
                        .as_ref()
                        .is_none_or(|b| cost < combined(b, self.weight))
                    {
                        best = Some(s);
                    }
                }
            }
            let Some(b) = &best else { break };
            let Some(next) = propose(b, &tried) else {
                break;
            };
            tried.insert(next.clone());
            constraint = next;
        }
        results.sort_by(|a, b| a.cost.total_cmp(&b.cost));
        results
    }

    /// 経路をまとめて採点し、次の提案に使う位置の情報を付ける。
    fn score(
        &self,
        lattice: &Lattice,
        session: &mut dyn VerifySession,
        head: &str,
        head_len: usize,
        paths: Vec<Path>,
    ) -> Vec<Scored> {
        let mut sentences = Vec::new();
        let mut queries = Vec::new();
        let mut sites = Vec::new();
        for path in &paths {
            sentences.push(format!("{head}{}", path.surfaces.concat()));
            let (q, s) = self.sites(lattice, path, head_len);
            queries.push(q);
            sites.push(s);
        }
        let verdicts = session.verify(&sentences, &queries);
        paths
            .into_iter()
            .zip(sites)
            .zip(verdicts)
            .map(|((path, sites), verdict)| Scored {
                path,
                sites,
                verdict,
            })
            .collect()
    }

    /// 下書きの各位置について、格子が到達できる別の字を集める。
    fn sites(
        &self,
        lattice: &Lattice,
        draft: &Path,
        head_len: usize,
    ) -> (Vec<(usize, char)>, Vec<Site>) {
        let mut queries: Vec<(usize, char)> = Vec::new();
        let mut index: HashMap<(usize, char), usize> = HashMap::new();
        let mut query = |pos: usize, c: char| {
            *index.entry((pos, c)).or_insert_with(|| {
                queries.push((pos, c));
                queries.len() - 1
            })
        };
        let mut sites = Vec::new();
        let mut pos = head_len;
        let mut seg_pos = 0;
        for (k, surface) in draft.surfaces.iter().enumerate() {
            let chars: Vec<char> = surface.chars().collect();
            for (j, &c) in chars.iter().enumerate() {
                let mut alternatives: Vec<char> = lattice.candidates[seg_pos]
                    .iter()
                    .filter_map(|cand| {
                        let mut cs = cand.surface.chars();
                        let same_head = cs.by_ref().take(j).eq(chars[..j].iter().copied());
                        match cs.next() {
                            Some(alt) if same_head && alt != c => Some(alt),
                            _ => None,
                        }
                    })
                    .collect();
                alternatives.sort_unstable();
                alternatives.dedup();
                if alternatives.is_empty() {
                    continue;
                }
                let draft = query(pos + j, c);
                let alternatives = alternatives
                    .into_iter()
                    .map(|alt| (alt, query(pos + j, alt)))
                    .collect();
                sites.push(Site {
                    segment: k,
                    offset: j,
                    draft,
                    alternatives,
                });
            }
            pos += chars.len();
            seg_pos += draft.spans[k];
        }
        (queries, sites)
    }
}

/// 格子のスコアラーの結果を覚える。反復のたびに同じ候補と接続を引き直すので、
/// 形態素解析を伴う引き直しを省く。
struct Memo<'a> {
    inner: &'a dyn Scorer,
    unigram: RefCell<HashMap<String, f64>>,
    /// 左 → 右 → コスト。文頭・文末は空文字列で表す（表層形は空にならない）。
    /// 2 段にして、引くときに文字列を作らずに済ませる。
    bigram: RefCell<HashMap<String, HashMap<String, f64>>>,
}

impl<'a> Memo<'a> {
    fn new(inner: &'a dyn Scorer) -> Memo<'a> {
        Memo {
            inner,
            unigram: RefCell::new(HashMap::new()),
            bigram: RefCell::new(HashMap::new()),
        }
    }
}

impl Scorer for Memo<'_> {
    fn unigram(&self, surface: &str) -> f64 {
        if let Some(&c) = self.unigram.borrow().get(surface) {
            return c;
        }
        let c = self.inner.unigram(surface);
        self.unigram.borrow_mut().insert(surface.to_string(), c);
        c
    }

    fn bigram(&self, left: Option<&str>, right: Option<&str>) -> f64 {
        let (l, r) = (left.unwrap_or(""), right.unwrap_or(""));
        if let Some(&c) = self.bigram.borrow().get(l).and_then(|m| m.get(r)) {
            return c;
        }
        let c = self.inner.bigram(left, right);
        self.bigram
            .borrow_mut()
            .entry(l.to_string())
            .or_default()
            .insert(r.to_string(), c);
        c
    }
}

fn combined(s: &Scored, weight: f64) -> f64 {
    s.path.cost + weight * s.verdict.cost
}

/// 下書きの字より良い別の字のうち、差が最も大きいものを選び、
/// そこまでを固定する制約を作る。試した制約は除く。
fn propose(best: &Scored, tried: &HashSet<Constraint>) -> Option<Constraint> {
    let costs = &best.verdict.char_costs;
    let mut proposals: Vec<(f64, Constraint)> = Vec::new();
    for site in &best.sites {
        for &(alt, q) in &site.alternatives {
            let gain = costs[site.draft] - costs[q];
            if gain <= 0.0 {
                continue;
            }
            let mut next_prefix: String = best.path.surfaces[site.segment]
                .chars()
                .take(site.offset)
                .collect();
            next_prefix.push(alt);
            let constraint = Constraint {
                fixed: best.path.surfaces[..site.segment].to_vec(),
                next_prefix,
            };
            if !tried.contains(&constraint) {
                proposals.push((gain, constraint));
            }
        }
    }
    proposals
        .into_iter()
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, c)| c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidates::Candidate;
    use crate::lattice::Lattice;
    use crate::scorer::Scorer;
    use crate::verify::{Stateless, Verdict, Verifier, VerifySession};
    use std::collections::HashMap;

    struct MapScorer(HashMap<&'static str, f64>);
    impl Scorer for MapScorer {
        fn unigram(&self, s: &str) -> f64 {
            *self.0.get(s).unwrap_or(&10.0)
        }
        fn bigram(&self, _: Option<&str>, _: Option<&str>) -> f64 {
            0.0
        }
    }

    /// `prefer` にある (位置, 字) のコストを 0、他を 1 にし、
    /// `good` を含む文のコストを 0、他を 10 にする検証器。
    struct FakeVerifier {
        prefer: Vec<(usize, char)>,
        good: &'static str,
    }
    impl Verifier for FakeVerifier {
        fn begin<'a>(&'a self, _: &str) -> Box<dyn VerifySession + 'a> {
            Box::new(Stateless(
                move |sentences: &[String], queries: &[Vec<(usize, char)>]| {
                    self.verify(sentences, queries)
                },
            ))
        }
    }
    impl FakeVerifier {
        fn verify(&self, sentences: &[String], queries: &[Vec<(usize, char)>]) -> Vec<Verdict> {
            sentences
                .iter()
                .zip(queries)
                .map(|(sentence, queries)| Verdict {
                    cost: if sentence.contains(self.good) {
                        0.0
                    } else {
                        10.0
                    },
                    char_costs: queries
                        .iter()
                        .map(|q| if self.prefer.contains(q) { 0.0 } else { 1.0 })
                        .collect(),
                })
                .collect()
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

    /// 格子は 法貨 を、検証器は 放火 を好む。
    fn lattice() -> Lattice {
        Lattice {
            candidates: vec![
                vec![cand("法貨", 1), cand("放火", 1), cand("ほうか", 1)],
                vec![cand("と", 1)],
            ],
        }
    }

    fn scorer() -> MapScorer {
        MapScorer(HashMap::from([
            ("法貨", 1.0),
            ("放火", 2.0),
            ("ほうか", 3.0),
            ("と", 0.0),
        ]))
    }

    fn speculator(prefer: Vec<(usize, char)>, good: &'static str, rounds: usize) -> Speculator {
        Speculator {
            verifier: Box::new(FakeVerifier { prefer, good }),
            weight: 1.0,
            rounds,
            width: 1,
        }
    }

    #[test]
    fn 検証器が下書きに同意すれば格子の先頭候補のまま() {
        let s = speculator(vec![(0, '法')], "法貨", 3);
        let result = s.decode(&lattice(), &scorer(), "", "");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].surfaces, ["法貨", "と"]);
    }

    #[test]
    fn 検証器が別の字を好めばその字で始まる候補に置き換わる() {
        let s = speculator(vec![(0, '放')], "放火", 3);
        let result = s.decode(&lattice(), &scorer(), "", "");
        assert_eq!(result[0].surfaces, ["放火", "と"]);
        assert_eq!(result[1].surfaces, ["法貨", "と"]);
    }

    #[test]
    fn 反復_0_回なら格子の先頭候補のまま() {
        let s = speculator(vec![(0, '放')], "放火", 0);
        let result = s.decode(&lattice(), &scorer(), "", "");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].surfaces, ["法貨", "と"]);
    }

    #[test]
    fn 提案した経路の合成コストが悪ければ元の下書きを先頭に保つ() {
        // 字の提案は 放 だが、文全体では 法貨 のほうが良いと言う検証器。
        let s = speculator(vec![(0, '放')], "法貨", 3);
        let result = s.decode(&lattice(), &scorer(), "", "");
        assert_eq!(result[0].surfaces, ["法貨", "と"]);
        assert_eq!(result[1].surfaces, ["放火", "と"]);
    }

    #[test]
    fn 退けた提案は繰り返さず次に差の大きい提案に移る() {
        // 放 の提案は文全体で退けられるので、次は ほ の提案を試す。
        let s = speculator(vec![(0, '放'), (0, 'ほ')], "ほうか", 3);
        let result = s.decode(&lattice(), &scorer(), "", "");
        assert_eq!(result[0].surfaces, ["ほうか", "と"]);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn 文頭のかなの分だけ位置がずれる() {
        // 「それは」が前置されるので 放 は位置 3 にある。
        let s = speculator(vec![(3, '放')], "放火", 3);
        let result = s.decode(&lattice(), &scorer(), "", "それは");
        assert_eq!(result[0].surfaces, ["放火", "と"]);
    }

    #[test]
    fn 候補の途中の字でも提案できる() {
        let lattice = Lattice {
            candidates: vec![vec![cand("学習", 1), cand("学修", 1)]],
        };
        let scorer = MapScorer(HashMap::from([("学習", 1.0), ("学修", 2.0)]));
        let s = speculator(vec![(1, '修')], "学修", 3);
        let result = s.decode(&lattice, &scorer, "", "");
        assert_eq!(result[0].surfaces, ["学修"]);
    }

    #[test]
    fn 幅を広げると提案の制約で複数の経路を採点する() {
        let lattice = Lattice {
            candidates: vec![
                vec![cand("法貨", 1), cand("放火", 1)],
                vec![cand("と", 1), cand("戸", 1)],
            ],
        };
        let scorer = MapScorer(HashMap::from([
            ("法貨", 1.0),
            ("放火", 2.0),
            ("と", 0.0),
            ("戸", 1.0),
        ]));
        let mut s = speculator(vec![(0, '放')], "放火戸", 1);
        s.width = 2;
        let result = s.decode(&lattice, &scorer, "", "");
        // 提案 放 の制約で 放火と・放火戸 の 2 本を採点し、文全体で良い 放火戸 が先頭。
        assert_eq!(result[0].surfaces, ["放火", "戸"]);
    }

    #[test]
    fn 経路は重複しない() {
        let s = speculator(vec![(0, '放'), (0, '法')], "法貨", 5);
        let result = s.decode(&lattice(), &scorer(), "", "");
        let mut seen = std::collections::HashSet::new();
        assert!(result.iter().all(|p| seen.insert(p.surfaces.clone())));
    }
}
