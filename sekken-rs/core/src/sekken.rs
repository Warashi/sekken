//! 入力から変換候補列までを束ねる入口。

use crate::candidates::candidates_at;
use crate::dictionary::Dictionary;
use crate::input::Input;
use crate::kana::hira2kata;
use crate::lattice::{Lattice, Weights};
use crate::scorer::Scorer;
use crate::speculate::Speculator;

pub struct Sekken<S: Scorer> {
    pub dict: Dictionary,
    /// 使う本人が登録した語。送りなしの見出しだけを持ち、`dict` より先に引く。
    pub user: Dictionary,
    pub scorer: S,
    /// 格子の候補に付ける順位とかなのコストの重み。
    pub weights: Weights,
    /// 検証器の提案で格子を再探索する。変換の方式はこれだけ。
    pub speculator: Speculator,
}

impl<S: Scorer> Sekken<S> {
    /// 入力を変換し、良い順に最大 `top_n` 個の文字列を返す。
    pub fn henkan(&self, input: &Input, top_n: usize) -> Vec<String> {
        // 先頭の変換しない区間は格子に入れず、文頭のかなとして候補に前置する。
        let head_len = input
            .pieces
            .iter()
            .take_while(|p| !p.kind.converts())
            .count();
        let head: String = input.pieces[..head_len]
            .iter()
            .map(|p| p.text.as_str())
            .collect();
        let pieces = &input.pieces[head_len..];
        if pieces.is_empty() {
            let kata = hira2kata(&head);
            return if kata == head {
                vec![head]
            } else {
                vec![head, kata]
            };
        }
        let reading = input.reading();
        let lattice = Lattice {
            weights: self.weights,
            candidates: (0..pieces.len())
                .map(|i| candidates_at(&self.dict, &self.user, pieces, i))
                .collect(),
        };
        let decoded = self
            .speculator
            .decode(&lattice, &self.scorer, &reading, &head, top_n);
        let mut result: Vec<String> = decoded
            .scored
            .into_iter()
            .map(|path| head.clone() + &path.surfaces.concat())
            .collect();
        // 反復で得た経路だけでは足りないので、残りは格子の順位で埋める。
        for path in decoded.lattice {
            let s = head.clone() + &path.surfaces.concat();
            if !result.contains(&s) {
                result.push(s);
            }
        }
        result.truncate(top_n);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Piece;
    use crate::kana::KanaTable;

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

    /// どの文も同じに採点する検証器。格子の順位だけで決まる変換を確かめる。
    struct Indifferent;
    impl crate::verify::Verifier for Indifferent {
        fn begin<'a>(&'a self, _: &str) -> Box<dyn crate::verify::VerifySession + 'a> {
            Box::new(crate::verify::Stateless(
                |sentences: &[String], queries: &[Vec<(usize, char)>]| {
                    sentences
                        .iter()
                        .zip(queries)
                        .map(|(_, queries)| crate::verify::Verdict {
                            cost: 0.0,
                            char_costs: vec![0.0; queries.len()],
                        })
                        .collect()
                },
            ))
        }
    }

    fn speculator(verifier: Box<dyn crate::verify::Verifier + Send>) -> Speculator {
        Speculator {
            verifier,
            weight: 100.0,
            rounds: 3,
            width: 2,
            stats: Default::default(),
        }
    }

    fn sekken() -> Sekken<PreferKanji> {
        Sekken {
            dict: Dictionary::parse(FIXTURE),
            user: Dictionary::default(),
            scorer: PreferKanji,
            weights: Weights::default(),
            speculator: speculator(Box::new(Indifferent)),
        }
    }

    fn roman(s: &str) -> Input {
        Input::from_roman(&KanaTable::default_table(), s)
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
        s.speculator = speculator(Box::new(PrefersWa));
        s.speculator.width = 1;
        let r = s.henkan(&roman("WagahaiHaNekoDa"), 3);
        assert_eq!(r[0], "わがはいは猫だ");
        // 反復で得た経路の後ろは格子の順位で埋める。
        assert_eq!(r.len(), 3);
        assert!(r.contains(&"我輩は猫だ".to_string()));
    }

    #[test]
    fn 変換区間が無ければひらがなとカタカナを返す() {
        assert_eq!(sekken().henkan(&roman("neko"), 5), ["ねこ", "ネコ"]);
        // literal だけならカタカナにしても同じなので重ねない。
        assert_eq!(sekken().henkan(&roman("'emacs"), 5), ["emacs"]);
    }

    #[test]
    fn 大文字境界の文を変換する() {
        let r = sekken().henkan(&roman("WagahaihaNekodearu."), 3);
        assert_eq!(r[0], "我輩は猫である。");
    }

    #[test]
    fn セミコロン境界の文を変換する() {
        let r = sekken().henkan(&roman(";wagahai;ha;neko;dearu."), 3);
        assert_eq!(r[0], "我輩は猫である。");
    }

    #[test]
    fn 二重セミコロンをリテラルとしてかなにする() {
        assert_eq!(sekken().henkan(&roman("semi;;koron"), 1)[0], "せみ;ころん");
    }

    #[test]
    fn 先頭のかな区間はかなとして先頭に付く() {
        let r = sekken().henkan(&roman("soreHaNekoDa"), 1);
        assert_eq!(r[0], "それは猫だ");
    }

    #[test]
    fn そのまま出す区間は変換せず文に入る() {
        let input = Input::new(vec![
            Piece::literal("Emacs"),
            Piece::convert("で"),
            Piece::convert("ねこ"),
            Piece::literal("Vim"),
            Piece::convert("だ"),
        ]);
        assert_eq!(sekken().henkan(&input, 1)[0], "Emacsで猫Vimだ");
    }

    #[test]
    fn そのまま出す区間は投機の位置合わせを経ても崩れない() {
        let mut s = sekken();
        s.speculator = speculator(Box::new(PrefersWa));
        let input = Input::new(vec![
            Piece::literal("Emacs"),
            Piece::convert("で"),
            Piece::convert("わがはい"),
            Piece::literal("Vim"),
            Piece::convert("だ"),
        ]);
        // 文頭の英字は格子の外の head になり、検証器の問い合わせ位置はその分ずれる。
        let r = s.henkan(&input, 3);
        assert_eq!(r[0], "Emacsで我輩Vimだ", "{r:?}");
        assert_eq!(r.len(), 3);
        // どの候補も英字の区間はそのまま、間の変換区間だけが変わる。
        assert!(
            r.iter()
                .all(|c| c.starts_with("Emacs") && c.contains("Vim")),
            "{r:?}"
        );
    }

    #[test]
    fn 途中のかな区間は辞書を引かずかなのまま入る() {
        let input = Input::new(vec![
            Piece::convert("ねこ"),
            Piece::kana("わがはい"),
            Piece::convert("だ"),
        ]);
        assert_eq!(sekken().henkan(&input, 1)[0], "猫わがはいだ");
    }

    #[test]
    fn 文頭の_abbrev_区間と山括弧の印を通して辞書を引く() {
        let mut s = sekken();
        s.dict =
            Dictionary::parse(";; okuri-nasi entries.\nemacs /Ｅｍａｃｓ/\nお> /御/\n>かい /会/\n");
        assert_eq!(s.henkan(&roman("/emacs"), 1)[0], "Ｅｍａｃｓ");
        assert_eq!(s.henkan(&roman("O>Kai"), 1)[0], "御会");
    }

    #[test]
    fn 空の入力は空の候補を返す() {
        assert_eq!(sekken().henkan(&Input::default(), 3), [""]);
    }
}
