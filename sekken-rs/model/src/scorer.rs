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
        self.tokenize(surface)
            .into_iter()
            .map(|t| t.surface)
            .collect()
    }
}

pub struct NgramScorer<S: Segmenter> {
    model: NgramModel,
    segmenter: S,
    cache: RefCell<HashMap<String, Vec<String>>>,
}

impl<S: Segmenter> NgramScorer<S> {
    pub fn new(model: NgramModel, segmenter: S) -> Self {
        NgramScorer {
            model,
            segmenter,
            cache: RefCell::new(HashMap::new()),
        }
    }

    fn tokens(&self, surface: &str) -> Vec<String> {
        if let Some(t) = self.cache.borrow().get(surface) {
            return t.clone();
        }
        let t = self.segmenter.split(surface);
        self.cache
            .borrow_mut()
            .insert(surface.to_string(), t.clone());
        t
    }

    fn cost(&self, prev: Option<&str>, next: Option<&str>) -> f64 {
        let prev_id = match prev {
            None => Some(crate::ngram::BOS),
            Some(p) => self.model.id(p),
        };
        let (next_id, chars) = match next {
            None => (Some(crate::ngram::EOS), 0),
            Some(n) => (self.model.id(n), n.chars().count()),
        };
        self.model.transition_cost(prev_id, next_id, chars)
    }
}

impl<S: Segmenter> Scorer for NgramScorer<S> {
    fn unigram(&self, surface: &str) -> f64 {
        let tokens = self.tokens(surface);
        tokens
            .windows(2)
            .map(|w| self.cost(Some(&w[0]), Some(&w[1])))
            .sum()
    }

    fn bigram(&self, left: Option<&str>, right: Option<&str>) -> f64 {
        let left_tokens = left.map(|s| self.tokens(s));
        let right_tokens = right.map(|s| self.tokens(s));
        let prev = left_tokens
            .as_ref()
            .and_then(|t| t.last())
            .map(String::as_str);
        let next = right_tokens
            .as_ref()
            .and_then(|t| t.first())
            .map(String::as_str);
        // 左が文頭なら prev は None（BOS）。左があるのに語が無いことは無い。
        self.cost(prev, next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CharSegmenter;
    impl Segmenter for CharSegmenter {
        fn split(&self, s: &str) -> Vec<String> {
            s.chars().map(|c| c.to_string()).collect()
        }
    }

    fn scorer() -> NgramScorer<CharSegmenter> {
        let mut m = NgramModel::new();
        // 平滑化の擬似頻度（PRIOR）に埋もれない程度に繰り返す。
        for _ in 0..500 {
            m.add_sentence(["猫", "が", "鳴", "く"]);
        }
        for _ in 0..100 {
            m.add_sentence(["書", "く"]);
        }
        NgramScorer::new(m, CharSegmenter)
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
