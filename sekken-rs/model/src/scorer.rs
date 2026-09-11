//! n-gram モデルを core の Scorer に繋ぐ。

use std::cell::RefCell;
use std::collections::HashMap;

use sekken_core::scorer::Scorer;

use crate::ngram::NgramModel;

/// 表層形を語に分ける。実運用は vibrato、テストは単純な分割で差し替える。
pub trait Segmenter {
    fn split(&self, surface: &str) -> Vec<String>;
}

impl Segmenter for crate::tokenizer::Tokenizer {
    fn split(&self, surface: &str) -> Vec<String> {
        self.surfaces(surface)
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
            cache: RefCell::new(HashMap::new()),
            ids: RefCell::new(HashMap::new()),
        }
    }

    fn words(&self, surface: &str) -> Vec<Word> {
        if let Some(w) = self.cache.borrow().get(surface) {
            return w.clone();
        }
        let words: Vec<Word> = self
            .segmenter
            .split(surface)
            .iter()
            .map(|t| Word {
                id: self.id(t),
                chars: t.chars().count(),
            })
            .collect();
        let mut cache = self.cache.borrow_mut();
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(surface.to_string(), words.clone());
        words
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

    fn cost(&self, prev: Option<Word>, next: Option<Word>) -> f64 {
        let prev_id = match prev {
            None => Some(crate::ngram::BOS),
            Some(p) => p.id,
        };
        let (next_id, chars) = match next {
            None => (Some(crate::ngram::EOS), 0),
            Some(n) => (n.id, n.chars),
        };
        self.model.transition_cost(prev_id, next_id, chars)
    }
}

impl<S: Segmenter> Scorer for NgramScorer<S> {
    fn unigram(&self, surface: &str) -> f64 {
        self.words(surface)
            .windows(2)
            .map(|w| self.cost(Some(w[0]), Some(w[1])))
            .sum()
    }

    fn bigram(&self, left: Option<&str>, right: Option<&str>) -> f64 {
        let prev = left.and_then(|s| self.words(s).last().copied());
        let next = right.and_then(|s| self.words(s).first().copied());
        // 左が文頭なら prev は None（BOS）。左があるのに語が無いことは無い。
        self.cost(prev, next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ngram::NgramCounter;

    struct CharSegmenter;
    impl Segmenter for CharSegmenter {
        fn split(&self, s: &str) -> Vec<String> {
            s.chars().map(|c| c.to_string()).collect()
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
    fn 表層形の内部遷移を_unigram_として足す() {
        let s = scorer();
        assert!(s.unigram("鳴く") < s.unigram("書く"));
        assert_eq!(s.unigram("猫"), 0.0);
    }

    #[test]
    fn 隣り合う表層形の境界の遷移を_bigram_にする() {
        let s = scorer();
        assert!(s.bigram(Some("猫"), Some("が")) < s.bigram(Some("猫"), Some("く")));
        assert!(s.bigram(None, Some("猫")) < s.bigram(None, Some("く")));
    }
}
