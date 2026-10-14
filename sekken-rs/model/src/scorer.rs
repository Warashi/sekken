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

/// 文を `segmenter` で語に分けて `personal` に数える。採点と同じ分け方で数えるため、
/// 変換中の確定（`NgramScorer::learn`）と、過去の文をまとめて数える `sekken learn` で共有する。
pub fn learn(personal: &mut Personal, segmenter: &impl Segmenter, sentence: &str) {
    let mut words = Vec::new();
    segmenter.split(sentence, &mut |t| words.push(personal::key(t)));
    personal.add(&words);
}

/// 表層形を分けた 1 語。id の引き当ては二分探索なので、表層形ごとに一度だけ行う。
#[derive(Clone, Copy)]
struct Word {
    id: Option<u32>,
    chars: usize,
    /// 個人の n-gram の鍵。
    key: Key,
    /// 語彙に無いカタカナ語を、語彙のカタカナ語に分けたもの（`katakana_split`）。
    split: Option<Split>,
}

/// 語彙に無いカタカナ語を語彙の語の並びとみなしたときの、最初と最後の語と、その間の接続コスト。
#[derive(Clone, Copy)]
struct Split {
    first: u32,
    last: u32,
    inner: f64,
}

/// 語彙の語に分けるときの 1 語の長さの上限（文字数）。分け方を探す時間を抑える。
const MAX_PIECE_CHARS: usize = 16;

fn is_katakana(c: char) -> bool {
    matches!(c as u32, 0x30A1..=0x30FA | 0x30FC)
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
        learn(&mut personal.borrow_mut(), &self.segmenter, sentence);
    }

    /// `surface` の語の列に `f` を適用する。列を複製せずに済ませるため。
    fn with_words<R>(&self, surface: &str, f: impl FnOnce(&[Word]) -> R) -> R {
        if let Some(w) = self.cache.borrow().get(surface) {
            return f(w);
        }
        let mut words: Vec<Word> = Vec::new();
        self.segmenter
            .split(surface, &mut |t| words.push(self.word(t)));
        let r = f(&words);
        let mut cache = self.cache.borrow_mut();
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(surface.to_string(), words);
        r
    }

    fn word(&self, token: &str) -> Word {
        let id = self.id(token);
        Word {
            id,
            chars: token.chars().count(),
            key: personal::key(token),
            split: if id.is_none() {
                self.katakana_split(token)
            } else {
                None
            },
        }
    }

    /// 語彙に無いカタカナ語（vibrato は辞書に無いカタカナの連なりを 1 語にまとめる）を、
    /// 語彙のカタカナ語の並びに分ける。未知語のコストは文字数に比例するので、長いカタカナ語
    /// （パフォーマンスオーバーヘッド）ほど高くなり、後ろをひらがなにして既知の短い語に
    /// 崩した候補（パフォーマンスおーばーへっど）に負ける。分けた並びのコストで数えれば、
    /// 語彙の語からなるカタカナ語は語彙の語と同じ程度のコストになる。
    /// 分け方は各語の unigram のコストの和が最小のもの。語彙の語で覆えなければ `None`。
    fn katakana_split(&self, token: &str) -> Option<Split> {
        let chars: Vec<char> = token.chars().collect();
        let n = chars.len();
        if n < 2 || !chars.iter().all(|&c| is_katakana(c)) {
            return None;
        }
        // best[i]: chars[..i] を語彙の語で覆う最小コストと、最後の語の (開始位置, id)。
        let mut best: Vec<Option<(f64, usize, u32)>> = vec![None; n + 1];
        best[0] = Some((0.0, 0, 0));
        let mut piece = String::new();
        for end in 1..=n {
            for start in end.saturating_sub(MAX_PIECE_CHARS)..end {
                let Some((cost, _, _)) = best[start] else {
                    continue;
                };
                piece.clear();
                piece.extend(&chars[start..end]);
                let Some(id) = self.id(&piece) else {
                    continue;
                };
                let total = cost + self.model.transition_cost(None, Some(id), end - start);
                if best[end].is_none_or(|(c, _, _)| total < c) {
                    best[end] = Some((total, start, id));
                }
            }
        }
        best[n]?;
        let mut ids = Vec::new();
        let mut end = n;
        while end > 0 {
            let (_, start, id) = best[end]?;
            ids.push((id, end - start));
            end = start;
        }
        ids.reverse();
        let inner = ids
            .windows(2)
            .map(|w| {
                self.model
                    .transition_cost(Some(w[0].0), Some(w[1].0), w[1].1)
            })
            .sum();
        Some(Split {
            first: ids[0].0,
            last: ids[ids.len() - 1].0,
            inner,
        })
    }

    /// `prev`（`None` は文頭）の後に `next` が来るコスト。語彙に無いカタカナ語は、未知語の
    /// コストと語彙の語に分けたコストの小さい方。
    fn next_cost(&self, prev: Option<u32>, next: Word) -> f64 {
        let unknown = self.model.transition_cost(prev, next.id, next.chars);
        match (next.id, next.split) {
            (None, Some(s)) => {
                let split = self.model.transition_cost(prev, Some(s.first), next.chars) + s.inner;
                unknown.min(split)
            }
            _ => unknown,
        }
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
            // 語彙に無いカタカナ語は、分けた最後の語を左の文脈にする。
            Some(p) => p.id.or(p.split.map(|s| s.last)),
        };
        match next {
            None => self
                .model
                .transition_cost(prev_id, Some(crate::ngram::EOS), 0),
            Some(n) => self.next_cost(prev_id, n),
        }
    }

    /// 本人の n-gram の最下位に置く、配布の bigram と unigram の対数確率。
    fn base(&self, prev: Option<Word>, next: Option<Word>) -> Base {
        let unigram = match next {
            None => self.model.transition_cost(None, Some(crate::ngram::EOS), 0),
            Some(n) => self.next_cost(None, n),
        };
        Base {
            bigram: -self.cost_base(prev, next),
            unigram: -unigram,
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
        self.segmenter
            .split(surface, &mut |t| words.push(self.word(t)));
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

    /// カタカナの連なりを 1 語にまとめ、他は 1 字ずつ分ける（vibrato の未知語の扱いに似せる）。
    struct KatakanaRunSegmenter;
    impl Segmenter for KatakanaRunSegmenter {
        fn split(&self, s: &str, f: &mut dyn FnMut(&str)) {
            let mut start = None;
            for (i, c) in s.char_indices() {
                match (is_katakana(c), start) {
                    (true, None) => start = Some(i),
                    (true, Some(_)) => {}
                    (false, st) => {
                        if let Some(st) = st {
                            f(&s[st..i]);
                            start = None;
                        }
                        f(&s[i..i + c.len_utf8()]);
                    }
                }
            }
            if let Some(st) = start {
                f(&s[st..]);
            }
        }
    }

    #[test]
    fn 語彙に無いカタカナ語は語彙の語に分けたコストで数える() {
        let mut m = NgramCounter::new();
        for _ in 0..200 {
            m.add_sentence(["オーバー", "ヘッド", "が"]);
            m.add_sentence(["お", "ば", "が"]);
        }
        let s = NgramScorer::new(m.freeze().unwrap(), KatakanaRunSegmenter);
        // オーバーヘッド は語彙に無いが、オーバー＋ヘッド に分ければ既知の語の並び。
        let katakana = s.bigram(None, Some("オーバーヘッドが")) + s.unigram("オーバーヘッドが");
        let hiragana = s.bigram(None, Some("おーばーへっどが")) + s.unigram("おーばーへっどが");
        assert!(katakana < hiragana, "{katakana} vs {hiragana}");
        let parts = s.bigram(None, Some("オーバー"))
            + s.bigram(Some("オーバー"), Some("ヘッド"))
            + s.bigram(Some("ヘッド"), Some("が"));
        assert!((katakana - parts).abs() < 1e-9, "{katakana} vs {parts}");
        // 語彙の語で覆えないカタカナ語は今までどおり未知語のコスト。
        let unknown = s.bigram(None, Some("ゴゴ"));
        assert_eq!(
            unknown,
            s.model.transition_cost(Some(crate::ngram::BOS), None, 2)
        );
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
    fn 先に数えた本人の_n_gram_は確定で数えたのと同じに効く() {
        let mut confirmed = scorer();
        confirmed.set_personal(Personal::new(2), 1.0);
        confirmed.learn("猫く");
        let mut personal = Personal::new(2);
        learn(&mut personal, &CharSegmenter, "猫く");
        let mut seeded = scorer();
        seeded.set_personal(personal, 1.0);
        assert_eq!(
            seeded.bigram(Some("猫"), Some("く")),
            confirmed.bigram(Some("猫"), Some("く"))
        );
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
