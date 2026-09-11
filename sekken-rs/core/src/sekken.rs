//! 入力から変換候補列までを束ねる入口。

use crate::candidates::candidates_at;
use crate::dictionary::Dictionary;
use crate::kana::{KanaTable, hira2kata};
use crate::lattice::Lattice;
use crate::rerank::Reranker;
use crate::scorer::Scorer;
use crate::segment::segment;
use crate::speculate::Speculator;

pub struct Sekken<S: Scorer> {
    pub table: KanaTable,
    pub dict: Dictionary,
    pub scorer: S,
    /// 文全体の採点で N-best を並べ替える。`None` なら格子の順位のまま。
    pub reranker: Option<Reranker>,
    /// 検証器の提案で格子を再探索する。`Some` なら `reranker` より優先する。
    pub speculator: Option<Speculator>,
}

impl<S: Scorer> Sekken<S> {
    /// ローマ字入力を変換し、良い順に最大 `top_n` 個の文字列を返す。
    pub fn henkan(&self, roman: &str, top_n: usize) -> Vec<String> {
        let seg = segment(roman);
        let prefix = self.table.roman2kana(&seg.prefix);
        if seg.segments.is_empty() {
            return vec![prefix.clone(), hira2kata(&prefix)];
        }
        let lattice = Lattice {
            candidates: (0..seg.segments.len())
                .map(|i| candidates_at(&self.table, &self.dict, &seg.segments, i))
                .collect(),
        };
        if let Some(speculator) = &self.speculator {
            let decoded = speculator.decode(&lattice, &self.scorer, roman, &prefix, top_n);
            let mut result: Vec<String> = decoded
                .scored
                .into_iter()
                .map(|path| prefix.clone() + &path.surfaces.concat())
                .collect();
            // 反復で得た経路だけでは足りないので、残りは格子の順位で埋める。
            for path in decoded.lattice {
                let s = prefix.clone() + &path.surfaces.concat();
                if !result.contains(&s) {
                    result.push(s);
                }
            }
            result.truncate(top_n);
            return result;
        }
        let Some(reranker) = &self.reranker else {
            return lattice
                .nbest(&self.scorer, top_n)
                .into_iter()
                .map(|path| prefix.clone() + &path.surfaces.concat())
                .collect();
        };
        let paths = lattice.nbest(&self.scorer, top_n.max(reranker.width));
        let candidates = paths
            .into_iter()
            .map(|path| (prefix.clone() + &path.surfaces.concat(), path.cost))
            .collect();
        let mut result = reranker.rerank(roman, candidates);
        result.truncate(top_n);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 漢字 < ひらがな < カタカナ の順に優先するスコアラー。
    struct PreferKanji;
    impl Scorer for PreferKanji {
        fn unigram(&self, s: &str) -> f64 {
            if s.chars()
                .any(|c| !matches!(c as u32, 0x3041..=0x30FF | 0x3000..=0x303F))
            {
                0.0
            } else if s.chars().any(|c| matches!(c as u32, 0x30A1..=0x30F6)) {
                2.0
            } else {
                1.0
            }
        }
        fn bigram(&self, _: Option<&str>, _: Option<&str>) -> f64 {
            0.0
        }
    }

    const FIXTURE: &str = "\
;; okuri-ari entries.
;; okuri-nasi entries.
わがはい /我輩/
ねこ /猫/
";

    fn sekken() -> Sekken<PreferKanji> {
        Sekken {
            table: KanaTable::default_table(),
            dict: Dictionary::parse(FIXTURE),
            scorer: PreferKanji,
            reranker: None,
            speculator: None,
        }
    }

    /// 「わ」で始まる文を好む検証器。
    struct PrefersWa;
    impl crate::verify::Verifier for PrefersWa {
        fn begin<'a>(&'a self, _: &str) -> Box<dyn crate::verify::VerifySession + 'a> {
            Box::new(crate::verify::Stateless(verify))
        }
    }
    fn verify(sentences: &[String], queries: &[Vec<(usize, char)>]) -> Vec<crate::verify::Verdict> {
        sentences
            .iter()
            .zip(queries)
            .map(|(sentence, queries)| crate::verify::Verdict {
                cost: if sentence.starts_with('わ') {
                    0.0
                } else {
                    10.0
                },
                char_costs: queries
                    .iter()
                    .map(|q| if *q == (0, 'わ') { 0.0 } else { 1.0 })
                    .collect(),
            })
            .collect()
    }

    #[test]
    fn 投機的な変換で検証器が好む経路が先頭になる() {
        let mut s = sekken();
        s.speculator = Some(crate::speculate::Speculator {
            verifier: Box::new(PrefersWa),
            weight: 100.0,
            rounds: 3,
            width: 1,
        });
        let r = s.henkan("WagahaiHaNekoDa", 3);
        assert_eq!(r[0], "わがはいは猫だ");
        // 反復で得た経路の後ろは格子の順位で埋める。
        assert_eq!(r.len(), 3);
        assert!(r.contains(&"我輩は猫だ".to_string()));
    }

    /// 「我輩」を含む文を嫌う採点器。
    struct DislikesWagahai;
    impl crate::rerank::SentenceScorer for DislikesWagahai {
        fn costs(&self, _: &str, sentences: &[String]) -> Vec<f64> {
            sentences
                .iter()
                .map(|s| if s.contains("我輩") { 100.0 } else { 0.0 })
                .collect()
        }
    }

    #[test]
    fn 文全体の採点で先頭候補が入れ替わる() {
        let mut s = sekken();
        s.reranker = Some(Reranker {
            scorer: Box::new(DislikesWagahai),
            weight: 1.0,
            width: 10,
        });
        let r = s.henkan("WagahaiHaNekoDa", 2);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0], "わがはいは猫だ");
    }

    #[test]
    fn 大文字を含まなければひらがなとカタカナを返す() {
        assert_eq!(sekken().henkan("neko", 5), ["ねこ", "ネコ"]);
    }

    #[test]
    fn 大文字境界の文を変換する() {
        let r = sekken().henkan("WagahaiHaNekoDearu.", 3);
        assert_eq!(r[0], "我輩は猫である。");
    }

    #[test]
    fn セミコロン境界の文を変換する() {
        let r = sekken().henkan(";wagahai;ha;neko;dearu.", 3);
        assert_eq!(r[0], "我輩は猫である。");
    }

    #[test]
    fn 二重セミコロンをリテラルとしてかなにする() {
        assert_eq!(sekken().henkan("semi;;koron", 1)[0], "せみ;ころん");
    }

    #[test]
    fn 先頭の小文字はかなとして先頭に付く() {
        let r = sekken().henkan("soreHaNekoDa", 1);
        assert_eq!(r[0], "それは猫だ");
    }
}
