//! n-gram モデルを core の Scorer に繋ぐ。

use std::cell::RefCell;
use std::collections::HashMap;

use sekken_core::scorer::Scorer;

use crate::ngram::NgramModel;

/// 表層形を語に分ける。実運用は vibrato、テストは単純な分割で差し替える。
pub trait Segmenter {
    /// `surface` を語に分けて順に `f` に渡す。語を複製せずに済ませるため。
    fn split(&self, surface: &str, f: &mut dyn FnMut(&str));
}

impl Segmenter for crate::tokenizer::Tokenizer {
    fn split(&self, surface: &str, f: &mut dyn FnMut(&str)) {
        self.for_each_surface(surface, f);
    }
}

/// 表層形を分けた 1 語。id の引き当ては二分探索なので、表層形ごとに一度だけ行う。
#[derive(Clone, Copy)]
struct Word {
    id: Option<u32>,
    chars: usize,
}

/// 覚える表層形と語の数の上限。超えたら全部忘れる。1 変換で数百〜数千の
/// 表層形を引くので、上限が無いと長く動く server の常駐メモリが増え続ける。
const CACHE_LIMIT: usize = 1 << 18;

pub struct NgramScorer<S: Segmenter> {
    model: NgramModel,
    segmenter: S,
    /// 語 1 つごとに加えるコスト。正なら少ない語で表せる候補（長い 1 語）を
    /// 有利にし、負なら短い語の列を有利にする。
    word_penalty: f64,
    /// 表層形 → 語の列。
    cache: RefCell<HashMap<String, Vec<Word>>>,
    /// 語 → id。id の引き当ては語彙の二分探索で 1 µs ほどかかり、
    /// 同じ語（助詞など）が多くの表層形に現れるので表層形をまたいで覚える。
    ids: RefCell<HashMap<String, Option<u32>>>,
}

impl<S: Segmenter> NgramScorer<S> {
    pub fn new(model: NgramModel, segmenter: S) -> Self {
        NgramScorer {
            model,
            segmenter,
            word_penalty: 0.0,
            cache: RefCell::new(HashMap::new()),
            ids: RefCell::new(HashMap::new()),
        }
    }

    pub fn with_word_penalty(mut self, word_penalty: f64) -> Self {
        self.word_penalty = word_penalty;
        self
    }

    /// `surface` の語の列に `f` を適用する。列を複製せずに済ませるため。
    fn with_words<R>(&self, surface: &str, f: impl FnOnce(&[Word]) -> R) -> R {
        if let Some(w) = self.cache.borrow().get(surface) {
            return f(w);
        }
        let mut words: Vec<Word> = Vec::new();
        self.segmenter.split(surface, &mut |t| {
            words.push(Word {
                id: self.id(t),
                chars: t.chars().count(),
            })
        });
        let r = f(&words);
        let mut cache = self.cache.borrow_mut();
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(surface.to_string(), words);
        r
    }

    fn id(&self, token: &str) -> Option<u32> {
        if let Some(&id) = self.ids.borrow().get(token) {
            return id;
        }
        let id = self.model.id(token);
        let mut ids = self.ids.borrow_mut();
        if ids.len() >= CACHE_LIMIT {
            ids.clear();
        }
        ids.insert(token.to_string(), id);
        id
    }

    /// `prev2 prev` の後に `next` が来るコスト。`prev` の `None` は文頭、
    /// `prev2` の `None` は 2 つ前が無いこと、`next` の `None` は文末。
    fn cost(&self, prev2: Option<Ctx>, prev: Ctx, next: Option<Word>) -> f64 {
        let (next_id, chars) = match next {
            None => (Some(crate::ngram::EOS), 0),
            Some(n) => (n.id, n.chars),
        };
        self.model
            .transition_cost3(prev2.and_then(Ctx::id), prev.id(), next_id, chars)
    }

    /// 表層形の末尾 2 語。1 語なら前の方は `None`、語が無ければ `None`。
    /// `with_words` は覚えた列を借りたまま閉包を呼ぶので、入れ子にせず複製して返す。
    fn last_two(&self, surface: &str) -> Option<(Option<Word>, Word)> {
        self.with_words(surface, |w| {
            Some((w.len().checked_sub(2).map(|i| w[i]), *w.last()?))
        })
    }

    /// 表層形の先頭 2 語。
    fn first_two(&self, surface: &str) -> (Option<Word>, Option<Word>) {
        self.with_words(surface, |w| (w.first().copied(), w.get(1).copied()))
    }

    /// 表層形の境界をまたぐ遷移のコスト。右の先頭の語は左の末尾 2 語を、右の 2 語目は
    /// 左の末尾の語と右の先頭の語を文脈にするので、右の 2 語目までがここに入る。
    fn boundary(&self, left2: Option<&str>, left: Option<&str>, right: Option<&str>) -> f64 {
        // 左に語が無い（分かち書きが空）ことは無いはずだが、あれば文頭とみなす。
        let (prev2, prev) = match left.and_then(|l| self.last_two(l)) {
            None => (None, Ctx::Bos),
            Some((second_last, last)) => {
                let prev2 = match second_last {
                    Some(w) => Ctx::Word(w),
                    None => match left2.and_then(|l2| self.last_two(l2)) {
                        None => Ctx::Bos,
                        Some((_, w)) => Ctx::Word(w),
                    },
                };
                (Some(prev2), Ctx::Word(last))
            }
        };
        let (first, second) = match right {
            None => (None, None),
            Some(r) => self.first_two(r),
        };
        let mut cost = self.cost(prev2, prev, first);
        if let (Some(first), Some(second)) = (first, second) {
            cost += self.cost(Some(prev), Ctx::Word(first), Some(second));
        }
        cost
    }
}

/// 遷移の文脈になる語。
#[derive(Clone, Copy)]
enum Ctx {
    /// 文頭。
    Bos,
    /// 表層形を分けた語。未知語なら id が無い。
    Word(Word),
}

impl Ctx {
    fn id(self) -> Option<u32> {
        match self {
            Ctx::Bos => Some(crate::ngram::BOS),
            Ctx::Word(w) => w.id,
        }
    }
}

impl<S: Segmenter> Scorer for NgramScorer<S> {
    /// 表層形の中の 3 語目以降の遷移。先頭 2 語は左の文脈に依るので `trigram` に入る。
    fn unigram(&self, surface: &str) -> f64 {
        self.with_words(surface, |words| {
            words
                .windows(3)
                .map(|w| self.cost(Some(Ctx::Word(w[0])), Ctx::Word(w[1]), Some(w[2])))
                .sum::<f64>()
                + self.word_penalty * words.len() as f64
        })
    }

    /// 2 つ前を見ないときの接続コスト。モデルが trigram を持つときは左が 1 語だと
    /// 2 つ前を文頭とみなしてしまうので、そのときは `trigram` を使う。
    fn bigram(&self, left: Option<&str>, right: Option<&str>) -> f64 {
        self.trigram(None, left, right)
    }

    fn trigram(&self, left2: Option<&str>, left: Option<&str>, right: Option<&str>) -> f64 {
        self.boundary(left2, left, right)
    }

    fn uses_left2(&self) -> bool {
        self.model.has_trigram()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ngram::NgramCounter;

    struct CharSegmenter;
    impl Segmenter for CharSegmenter {
        fn split(&self, s: &str, f: &mut dyn FnMut(&str)) {
            for (i, c) in s.char_indices() {
                f(&s[i..i + c.len_utf8()]);
            }
        }
    }

    fn scorer() -> NgramScorer<CharSegmenter> {
        let mut m = NgramCounter::new();
        // 平滑化の擬似頻度（PRIOR）に埋もれない程度に繰り返す。
        for _ in 0..500 {
            m.add_sentence(["猫", "が", "鳴", "く"]);
        }
        for _ in 0..100 {
            m.add_sentence(["書", "く"]);
        }
        NgramScorer::new(m.freeze().unwrap(), CharSegmenter)
    }

    #[test]
    fn 表層形の_3_語目からの内部遷移を_unigram_として足す() {
        let s = scorer();
        assert!(s.unigram("が鳴く") < s.unigram("が書く"));
        // 先頭 2 語は左の文脈に依るので境界の側で数える。
        assert_eq!(s.unigram("猫"), 0.0);
        assert_eq!(s.unigram("鳴く"), 0.0);
    }

    #[test]
    fn 境界の遷移は右の_2_語目まで含み_2_つ前の表層形を文脈にする() {
        let s = scorer();
        // 「猫 が」の後の 鳴 は trigram にあり、「書 が」の後は bigram に落ちる。
        assert!(
            s.trigram(Some("猫"), Some("が"), Some("鳴く"))
                < s.trigram(Some("書"), Some("が"), Some("鳴く"))
        );
        // 右の 2 語目は左の末尾の語と右の先頭の語を文脈にする。
        let whole = s.trigram(None, Some("猫"), Some("が鳴"));
        let split =
            s.trigram(None, Some("猫"), Some("が")) + s.trigram(Some("猫"), Some("が"), Some("鳴"));
        assert!((whole - split).abs() < 1e-9, "{whole} vs {split}");
        // 左が 2 語以上なら 2 つ前は左の中にある。
        assert_eq!(
            s.trigram(Some("犬"), Some("猫が"), Some("鳴く")),
            s.trigram(Some("猫"), Some("が"), Some("鳴く"))
        );
        assert!(s.uses_left2());
    }

    #[test]
    fn trigram_の無いモデルでは_2_つ前を見ない() {
        let mut m = NgramCounter::new();
        for _ in 0..500 {
            m.add_sentence(["猫", "が", "鳴", "く"]);
        }
        let params = crate::ngram::BuildParams {
            trigram: false,
            ..Default::default()
        };
        let model = crate::ngram::NgramModel::build(&m.counts(1), &params).unwrap();
        let s = NgramScorer::new(model, CharSegmenter);
        assert!(!s.uses_left2());
        assert_eq!(
            s.trigram(Some("猫"), Some("が"), Some("鳴く")),
            s.bigram(Some("が"), Some("鳴く"))
        );
    }

    #[test]
    fn 語ごとの罰則は語の数に比例して足す() {
        let s = scorer().with_word_penalty(1.5);
        assert_eq!(s.unigram("猫"), 1.5);
        let base = scorer();
        assert!((s.unigram("鳴く") - base.unigram("鳴く") - 3.0).abs() < 1e-9);
    }

    #[test]
    fn 隣り合う表層形の境界の遷移を_bigram_にする() {
        let s = scorer();
        assert!(s.bigram(Some("猫"), Some("が")) < s.bigram(Some("猫"), Some("く")));
        assert!(s.bigram(None, Some("猫")) < s.bigram(None, Some("く")));
    }
}
