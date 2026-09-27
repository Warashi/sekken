//! n-gram モデルを core の Scorer に繋ぐ。

use std::cell::RefCell;
use std::collections::HashMap;

use sekken_core::scorer::Scorer;

use crate::ngram::NgramModel;
use crate::personal::{self, Base, Key, Personal};

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
    /// 個人の n-gram の鍵。
    key: Key,
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
    /// 確定した文から数えた本人の n-gram と、その項に掛ける重み。
    personal: Option<(RefCell<Personal>, f64)>,
}

/// 格子の接続で見る個人の n-gram の次数。探索は隣り合う語しか見ないので 2。
/// それより長い文脈は、投機で採点する文全体に `Scorer::sentence` で足す。
const LATTICE_ORDER: usize = 2;

impl<S: Segmenter> NgramScorer<S> {
    pub fn new(model: NgramModel, segmenter: S) -> Self {
        NgramScorer {
            model,
            segmenter,
            cache: RefCell::new(HashMap::new()),
            ids: RefCell::new(HashMap::new()),
            personal: None,
        }
    }

    /// 本人の n-gram を混ぜる。`weight` は配布の bigram との対数確率の差に掛ける重みで、
    /// 1 なら配布の bigram を個人の補間の確率に置き換えるのと同じ。
    pub fn set_personal(&mut self, personal: Personal, weight: f64) {
        self.personal = Some((RefCell::new(personal), weight));
    }

    /// 混ぜている本人の n-gram。保存するため。
    pub fn personal(&self) -> Option<std::cell::Ref<'_, Personal>> {
        self.personal.as_ref().map(|(p, _)| p.borrow())
    }

    /// 確定した文を本人の n-gram に数える。混ぜていなければ何もしない。
    pub fn learn(&self, sentence: &str) {
        let Some((personal, _)) = &self.personal else {
            return;
        };
        let mut words = Vec::new();
        self.segmenter
            .split(sentence, &mut |t| words.push(personal::key(t)));
        personal.borrow_mut().add(&words);
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
                key: personal::key(t),
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

    /// 配布の bigram だけの接続コスト。
    fn cost_base(&self, prev: Option<Word>, next: Option<Word>) -> f64 {
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

    /// 本人の n-gram の最下位に置く、配布の bigram と unigram の対数確率。
    fn base(&self, prev: Option<Word>, next: Option<Word>) -> Base {
        let (next_id, chars) = match next {
            None => (Some(crate::ngram::EOS), 0),
            Some(n) => (n.id, n.chars),
        };
        Base {
            bigram: -self.cost_base(prev, next),
            unigram: -self.model.transition_cost(None, next_id, chars),
        }
    }

    fn cost(&self, prev: Option<Word>, next: Option<Word>) -> f64 {
        let Some((personal, weight)) = &self.personal else {
            return self.cost_base(prev, next);
        };
        let base = self.base(prev, next);
        let history = [prev.map_or(personal::BOS, |w| w.key)];
        let next = next.map_or(personal::EOS, |w| w.key);
        let log_prob = personal
            .borrow()
            .log_prob(&history, next, base, LATTICE_ORDER);
        -base.bigram + weight * (base.bigram - log_prob)
    }
}

impl<S: Segmenter> Scorer for NgramScorer<S> {
    /// 表層形の中の遷移。2 語目から。
    fn unigram(&self, surface: &str) -> f64 {
        self.with_words(surface, |words| {
            words
                .windows(2)
                .map(|w| self.cost(Some(w[0]), Some(w[1])))
                .sum()
        })
    }

    fn bigram(&self, left: Option<&str>, right: Option<&str>) -> f64 {
        // 左が文頭なら prev は None（BOS）。左に語が無い（分かち書きが空）ことは
        // 無いはずだが、あれば文頭とみなす。
        let prev = left.and_then(|s| self.with_words(s, |w| w.last().copied()));
        let next = right.and_then(|s| self.with_words(s, |w| w.first().copied()));
        self.cost(prev, next)
    }

    /// 本人の n-gram の、格子の接続で見た 2 語より長い文脈の分。文全体を分け直し、
    /// 各語で「長い文脈の対数確率 − 2 語の対数確率」を重み付きで引く。
    fn sentence(&self, surface: &str) -> f64 {
        let Some((personal, weight)) = &self.personal else {
            return 0.0;
        };
        let personal = personal.borrow();
        let order = personal.order();
        if order <= LATTICE_ORDER {
            return 0.0;
        }
        let mut words: Vec<Word> = Vec::new();
        self.segmenter.split(surface, &mut |t| {
            words.push(Word {
                id: self.id(t),
                chars: t.chars().count(),
                key: personal::key(t),
            })
        });
        let mut history = vec![personal::BOS];
        let mut prev = None;
        let mut extra = 0.0;
        for next in words.iter().copied().map(Some).chain(std::iter::once(None)) {
            let base = self.base(prev, next);
            let key = next.map_or(personal::EOS, |w| w.key);
            let short = personal.log_prob(&history, key, base, LATTICE_ORDER);
            let long = personal.log_prob(&history, key, base, order);
            extra += weight * (short - long);
            history.push(key);
            prev = next;
        }
        extra
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
    fn 表層形の_2_語目からの内部遷移を_unigram_として足す() {
        let s = scorer();
        assert!(s.unigram("が鳴く") < s.unigram("が書く"));
        // 先頭の語は左の文脈に依るので境界の側で数える。
        assert_eq!(s.unigram("猫"), 0.0);
        assert!(s.unigram("鳴く") > 0.0);
        let path =
            s.bigram(None, Some("猫")) + s.bigram(Some("猫"), Some("が鳴く")) + s.unigram("が鳴く");
        let whole = s.bigram(None, Some("猫が鳴く")) + s.unigram("猫が鳴く");
        assert!((path - whole).abs() < 1e-9, "{path} vs {whole}");
    }

    #[test]
    fn 分かち書きが空の表層形は文頭とみなす() {
        struct EmptySegmenter;
        impl Segmenter for EmptySegmenter {
            fn split(&self, _: &str, _: &mut dyn FnMut(&str)) {}
        }
        let mut m = NgramCounter::new();
        m.add_sentence(["猫", "が"]);
        let s = NgramScorer::new(m.freeze().unwrap(), EmptySegmenter);
        assert_eq!(s.unigram("猫"), 0.0);
        assert_eq!(s.bigram(Some("猫"), Some("が")), s.bigram(None, None));
    }

    #[test]
    fn 本人の_n_gram_を混ぜなければ学習しても変わらない() {
        let s = scorer();
        let before = s.bigram(Some("猫"), Some("く"));
        s.learn("猫く");
        assert_eq!(s.bigram(Some("猫"), Some("く")), before);
        assert!(s.personal().is_none());
        assert_eq!(s.sentence("猫く"), 0.0);
    }

    #[test]
    fn 学習した文の接続は安くなる() {
        let mut s = scorer();
        s.set_personal(Personal::new(2), 1.0);
        let before = s.bigram(Some("猫"), Some("く"));
        let unrelated = s.bigram(Some("猫"), Some("が"));
        s.learn("猫く");
        assert!(s.bigram(Some("猫"), Some("く")) < before);
        // 表層形の中の遷移も同じ接続コストで数える。
        assert!(s.unigram("猫く") < before);
        // 数えていない接続は、本人の文の分だけ確率が減って高くなる。
        assert!(s.bigram(Some("猫"), Some("が")) > unrelated);
    }

    #[test]
    fn 重みが_0_なら配布の_bigram_のまま() {
        let plain = scorer();
        let mut s = scorer();
        s.set_personal(Personal::new(3), 0.0);
        s.learn("猫く");
        assert_eq!(
            s.bigram(Some("猫"), Some("く")),
            plain.bigram(Some("猫"), Some("く"))
        );
        assert_eq!(s.sentence("書く猫"), 0.0);
    }

    #[test]
    fn 格子より長い文脈は文全体のコストに入る() {
        let mut s = scorer();
        s.set_personal(Personal::new(2), 1.0);
        s.learn("書く猫");
        // 次数 2 なら格子の接続で全部数え終わっている。
        assert_eq!(s.sentence("書く猫"), 0.0);

        let mut s = scorer();
        s.set_personal(Personal::new(3), 1.0);
        for _ in 0..3 {
            s.learn("書く猫");
            s.learn("鳴く犬");
        }
        // 「く」の後は 猫 と 犬 が同じ回数なので bigram では分けられず、
        // 3 語の文脈（書 く → 猫）で分かれる。
        assert!(s.sentence("書く猫") < 0.0);
        assert!(s.sentence("書く犬") > 0.0);
    }

    #[test]
    fn 隣り合う表層形の境界の遷移を_bigram_にする() {
        let s = scorer();
        assert!(s.bigram(Some("猫"), Some("が")) < s.bigram(Some("猫"), Some("く")));
        assert!(s.bigram(None, Some("猫")) < s.bigram(None, Some("く")));
    }
}
