//! 言語モデルで文をまとめて採点する。
//!
//! 採点は入力側（BOS、条件付きなら前置する入力と `\t`）を 1 本だけ読んで状態を取り、
//! 候補側は trie にまとめて、共有する接頭辞を一度だけ読む。上位 20 本の候補は
//! 文字の半分以上が他の候補と接頭辞を共有するので、候補ごとに全文を読むより
//! 計算が半分近くで済む。
//!
//! 入力中は 1 字打つごとに変換が来て、入力側は前回と長い接頭辞を共有する
//! （`Wagah` → `ワガh`、`Wagaha` → `ワガハ` のように末尾は入れ替わる）。
//! 直前の入力側の読み（`\t` の手前まで）の係数を残し、共有する接頭辞の
//! 状態を係数から作って、その先と `\t` だけを読む。
//!
//! 交互形は入力側が BOS だけで、読みは候補の列に区間ごとに混ざる。先頭の
//! 区間の列は入力が伸びても変わらないので、直前の変換で読んだ候補の節を
//! 場ごと持ち越し、次の変換は共有しない分だけ読む。

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::condition::Condition;
use crate::file::SavedModel;
use crate::infer::{Infer, Plastic};
use crate::interleave::{build, loss_mask, output_positions};
use crate::session::{Carried, Session};
use crate::vocab::{BOS, EOS, Vocab};

pub struct LmScorer {
    infer: Infer,
    vocab: Vocab,
    /// 「入力 \t 出力」で学習したモデルなら、採点する文に前置する入力の種類。
    condition: Condition,
    /// 直前の入力側の読み。
    prefix: Mutex<Option<PrefixCache>>,
    /// 直前の変換で読んだ候補の節と、その入力側の id 列。
    carried: Mutex<Option<(Vec<u32>, Carried)>>,
    /// 出力層を動かした回数。動かす前に始めた採点の節は古い出力層の
    /// log-sum-exp を持つので、閉じるときに持ち越さないための印。
    generation: AtomicU64,
}

/// 直前の入力側（BOS から `\t` の手前まで）の読み。
struct PrefixCache {
    ids: Vec<u32>,
    /// `ids` の各位置の係数（位置 × n_layers × proj_width）。
    proj: Vec<f32>,
    /// (`ids` の先頭 n 個を読み終えた状態) を n の昇順に持つ。読んだ直後は
    /// 持たず、続きを読んだときに残す（続きが来ない変換に作る費用を掛けない）。
    /// 末尾が入れ替わる打鍵では 1 つ前の状態から進めるので、直近の 2 つを持つ。
    checkpoints: Vec<(usize, Vec<f32>)>,
}

/// 残す状態の数。
const CHECKPOINTS: usize = 2;

/// 1 回の変換の間の採点。入力側を読み終えた状態と、読んだ候補を持ち回る。
pub struct Scoring<'a> {
    scorer: &'a LmScorer,
    session: Session<'a>,
    /// 入力側の id 列。閉じるときに読んだ節と一緒に残す。
    prefix: Vec<u32>,
    /// 始めたときの出力層の世代。
    generation: u64,
    /// 交互形なら、文ごとに区間に分けて挟むカタカナ読み。
    reading: Option<String>,
}

/// 1 文の採点。`path[i]` は列の i 番目まで読んだ節で、`path[i]` の次の字の
/// 対数確率を `Scoring::log_prob` で引ける（i 番目を予測する節は `path[i - 1]`、
/// 先頭は根）。交互形では列に読みと区切りが混ざる。
pub struct Scored {
    pub ids: Vec<u32>,
    pub path: Vec<usize>,
    /// `ids[i]` をコストに数えるか。交互形では読みの字と出力の区切りが外れる。
    counted: Vec<bool>,
    /// 文の i 文字目（出力の字）の `ids` での位置。
    output: Vec<usize>,
}

impl LmScorer {
    pub fn new(infer: Infer, vocab: Vocab, condition: Condition) -> LmScorer {
        LmScorer {
            infer,
            vocab,
            condition,
            prefix: Mutex::new(None),
            carried: Mutex::new(None),
            generation: AtomicU64::new(0),
        }
    }

    /// ファイルから読んだモデルで採点器を組む。
    pub fn from_saved(saved: &SavedModel) -> anyhow::Result<LmScorer> {
        Ok(LmScorer::new(
            saved.infer()?,
            saved.vocab.clone(),
            saved.condition,
        ))
    }

    pub(crate) fn vocab(&self) -> &Vocab {
        &self.vocab
    }

    #[cfg(test)]
    pub(crate) fn into_infer(self) -> Infer {
        self.infer
    }

    /// 入力側の id 列。条件付きなら BOS、前置する入力、`\t`。無条件なら BOS だけ。
    fn prefix_ids(&self, input: &str) -> Vec<u32> {
        let mut ids = vec![BOS];
        if let Some(prefix) = self.condition.prefix(input) {
            ids.extend(prefix.chars().map(|c| self.vocab.id(c)));
            ids.push(self.vocab.id('\t'));
        }
        ids
    }

    /// 入力側を読み、この入力に対する採点を始める。入力側が直前の変換と同じなら、
    /// 直前に読んだ候補の節を持ち越した場から始める。
    pub fn begin(&self, input: &str) -> Scoring<'_> {
        let prefix = self.prefix_ids(input);
        let session = match self.carried.lock().unwrap().take() {
            Some((ids, carried)) if ids == prefix => Session::reopen(&self.infer, carried),
            _ => self.open(&prefix),
        };
        Scoring {
            scorer: self,
            session,
            prefix,
            generation: self.generation.load(Ordering::Acquire),
            reading: self.condition.interleaved_reading(input),
        }
    }

    /// 入力側の id 列を読んで場を開く。`\t` の手前までが直前の入力側と接頭辞を
    /// 共有するなら、共有する分の状態を係数から作り、その先と `\t` だけを読む。
    /// 結果は最初から読んだのと同じになる。
    fn open(&self, ids: &[u32]) -> Session<'_> {
        let body = &ids[..ids.len() - 1];
        if body.is_empty() {
            return Session::open(&self.infer, ids);
        }
        let stride = self.infer.config().n_layers * self.infer.proj_width();
        let mut cache = self.prefix.lock().unwrap();
        let c = cache.get_or_insert_with(|| PrefixCache {
            ids: Vec::new(),
            proj: Vec::new(),
            checkpoints: Vec::new(),
        });
        let shared = c.ids.iter().zip(body).take_while(|(a, b)| a == b).count();
        // BOS しか共有しなければ最初から読む。
        if shared < 2 {
            let (session, proj) = Session::open_from(&self.infer, &self.infer.init_state(), ids);
            c.ids = body.to_vec();
            c.proj = proj[..body.len() * stride].to_vec();
            c.checkpoints.clear();
            return session;
        }
        // 共有する分を読み終えた状態を、それ以下で最も長い状態から係数で進めて作る。
        let (mut done, mut state) = c
            .checkpoints
            .iter()
            .rev()
            .find(|(n, _)| *n <= shared)
            .map(|(n, s)| (*n, s.clone()))
            .unwrap_or_else(|| (0, self.infer.init_state()));
        self.infer
            .rescan(&mut state, &c.proj[done * stride..shared * stride]);
        done = shared;
        let tail = &ids[shared..];
        let (session, proj) = Session::open_from(&self.infer, &state, tail);
        let grown = tail.len() - 1;
        self.infer.rescan(&mut state, &proj[..grown * stride]);
        c.ids.truncate(done);
        c.ids.extend_from_slice(&tail[..grown]);
        c.proj.truncate(done * stride);
        c.proj.extend_from_slice(&proj[..grown * stride]);
        c.checkpoints.retain(|(n, _)| *n <= done);
        c.checkpoints.push((body.len(), state));
        if c.checkpoints.len() > CHECKPOINTS {
            c.checkpoints.remove(0);
        }
        session
    }

    /// 各文の負の対数尤度（EOS まで含む）。
    pub fn nll(&self, input: &str, sentences: &[String]) -> Vec<f64> {
        let mut scoring = self.begin(input);
        scoring
            .score(sentences)
            .iter()
            .map(|s| scoring.nll(s))
            .collect()
    }

    /// 入力 `input` に対して確定した文 `sentence` で出力層を 1 歩動かす。
    /// 損失に入る位置は採点と同じ（交互形なら出力の字と区間の終わり、それに EOS）。
    /// 更新前の文の負の対数尤度を返す。
    pub fn adapt(&self, input: &str, sentence: &str, lr: f32, plastic: Plastic) -> f64 {
        let d = self.infer.config().d_model;
        let mut hidden = Vec::new();
        let mut targets = Vec::new();
        {
            let mut scoring = self.begin(input);
            let scored = scoring.score(std::slice::from_ref(&sentence.to_string()));
            let scored = &scored[0];
            let session = &scoring.session;
            let mut node = 0;
            for ((&next, &id), &counted) in scored.path.iter().zip(&scored.ids).zip(&scored.counted)
            {
                if counted {
                    hidden.extend_from_slice(session.hidden(node));
                    targets.push(id);
                }
                node = next;
            }
            hidden.extend_from_slice(session.hidden(node));
            targets.push(EOS);
        }
        debug_assert_eq!(hidden.len(), targets.len() * d);
        let nll = self.infer.adapt(&hidden, &targets, lr, plastic);
        // 持ち越した節の log-sum-exp は古い出力層のものなので捨てる。
        // 同時に走っている採点が閉じるときに書き戻さないよう世代も進める。
        // 入力側の cache（係数と状態）は出力層に依らないので残す。
        {
            let mut carried = self.carried.lock().unwrap();
            self.generation.fetch_add(1, Ordering::AcqRel);
            *carried = None;
        }
        nll
    }

    pub fn infer(&self) -> &Infer {
        &self.infer
    }
}

impl<'a> Scoring<'a> {
    pub(crate) fn scorer(&self) -> &'a LmScorer {
        self.scorer
    }

    /// 各文を読む。既に読んだ接頭辞は読み直さない。交互形なら文を読みと
    /// 区間ごとに並べた列にして読む。
    pub fn score(&mut self, sentences: &[String]) -> Vec<Scored> {
        let lines: Vec<String> = match &self.reading {
            Some(reading) => sentences.iter().map(|s| build(reading, s)).collect(),
            None => sentences.to_vec(),
        };
        let ids: Vec<Vec<u32>> = lines
            .iter()
            .map(|s| s.chars().map(|c| self.scorer.vocab.id(c)).collect())
            .collect();
        let paths = self.session.read(&ids);
        ids.into_iter()
            .zip(paths)
            .zip(&lines)
            .map(|((ids, path), line)| {
                let (counted, output) = match self.reading {
                    Some(_) => (loss_mask(line), output_positions(line)),
                    None => (vec![true; ids.len()], (0..ids.len()).collect()),
                };
                Scored {
                    ids,
                    path,
                    counted,
                    output,
                }
            })
            .collect()
    }

    /// 文全体（EOS まで）の負の対数尤度。交互形では出力側の字と区間の終わりだけ数える。
    pub fn nll(&self, scored: &Scored) -> f64 {
        self.session
            .nll_counted(&scored.path, &scored.ids, &scored.counted)
    }

    /// 文の先頭 `pos` 文字の次に `id` が来るコスト。`pos` が文の長さを超えたら無限大。
    pub fn char_cost(&self, scored: &Scored, pos: usize, id: u32) -> f64 {
        let node = match scored.output.get(pos) {
            Some(0) => 0,
            Some(&i) => scored.path[i - 1],
            None if pos == scored.output.len() => scored.path.last().copied().unwrap_or(0),
            None => return f64::INFINITY,
        };
        -f64::from(self.session.log_prob(node, id))
    }
}

impl Drop for Scoring<'_> {
    /// 読んだ候補の節を次の変換に残す。
    fn drop(&mut self) {
        // 採点の途中で panic したら場は壊れているので、残さない。
        if std::thread::panicking() {
            return;
        }
        let mut slot = self.scorer.carried.lock().unwrap();
        // 始めてから出力層が動いていれば、節の log-sum-exp は古いので残さない。
        if self.scorer.generation.load(Ordering::Acquire) != self.generation {
            return;
        }
        let carried = self.session.carry();
        *slot = Some((std::mem::take(&mut self.prefix), carried));
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::config::ModelConfig;
    use crate::infer::Chain;
    use crate::testing::random_weights;

    fn config(vocab: &Vocab) -> ModelConfig {
        ModelConfig {
            vocab_size: vocab.len(),
            d_model: 8,
            n_layers: 1,
            n_heads: 1,
            head_dim: 8,
            d_state: 4,
            mlp_dim: 16,
        }
    }

    /// 形だけ合った重みの採点器。
    pub(crate) fn scorer_with(vocab: Vocab, condition: Condition) -> LmScorer {
        let config = config(&vocab);
        let infer = Infer::new(config.clone(), &random_weights(&config, 1)).unwrap();
        LmScorer::new(infer, vocab, condition)
    }

    pub(crate) fn scorer() -> LmScorer {
        scorer_with(Vocab::build(["猫が鳴く"], 1), Condition::None)
    }

    /// BOS も EOS も付けない id 列。
    fn ids(vocab: &Vocab, s: &str) -> Vec<u32> {
        s.chars().map(|c| vocab.id(c)).collect()
    }

    /// 入力側と候補を 1 本の連鎖として読んだ、候補側（EOS まで）の負の対数尤度。
    fn reference(s: &LmScorer, prefix: &[u32], ids: &[u32]) -> f64 {
        let whole: Vec<u32> = prefix.iter().chain(ids).copied().collect();
        let read = s.infer.read(&[Chain {
            state: &s.infer.init_state(),
            ids: &whole,
        }]);
        let d = s.infer.config().d_model;
        let mut total = 0.0;
        for (i, &id) in ids.iter().chain(std::iter::once(&EOS)).enumerate() {
            let pos = prefix.len() - 1 + i;
            total -= f64::from(s.infer.log_prob(
                &read.hidden[pos * d..(pos + 1) * d],
                read.lse[pos],
                id,
            ));
        }
        total
    }

    #[test]
    fn 文ごとに_1_つのコストを返す() {
        let s = scorer();
        let costs = s.nll("", &["猫が鳴く".to_string(), "猫".to_string()]);
        assert_eq!(costs.len(), 2);
        assert!(costs.iter().all(|c| *c > 0.0));
    }

    #[test]
    fn 無条件のコストは_bos_から一本で読んだ値と一致する() {
        let vocab = Vocab::build(["猫が鳴く"], 1);
        let s = scorer_with(vocab.clone(), Condition::None);
        let cost = s.nll("", &["猫が鳴く".to_string()])[0];
        let expected = reference(&s, &[BOS], &ids(&vocab, "猫が鳴く"));
        assert!((cost - expected).abs() < 1e-4, "{cost} vs {expected}");
    }

    #[test]
    fn 長さの違う文を一緒に採点しても単独と同じ() {
        let s = scorer();
        let together = s.nll("", &["猫が鳴く".to_string(), "猫".to_string()]);
        let alone = s.nll("", &["猫".to_string()]);
        assert!((together[1] - alone[0]).abs() < 1e-4);
    }

    #[test]
    fn 条件付きなら入力を前置して出力側だけのコストになる() {
        let vocab = Vocab::build(["猫が鳴く\tネコ"], 1);
        let s = scorer_with(vocab.clone(), Condition::Katakana);
        // 「ネコ \t 猫」を 1 本で読み、\t の後だけを数えた値と一致する。
        let mut prefix = vec![BOS];
        prefix.extend(ids(&vocab, "ネコ\t"));
        let expected = reference(&s, &prefix, &ids(&vocab, "猫"));
        let cost = s.nll("ねこ", &["猫".to_string()])[0];
        assert!((cost - expected).abs() < 1e-4);
        assert_ne!(cost, s.nll("", &["猫".to_string()])[0]);
    }

    #[test]
    fn カタカナ読みの条件付きは読みをカタカナにして前置する() {
        let vocab = Vocab::build(["猫が鳴く\tネコガナク"], 1);
        let s = scorer_with(vocab.clone(), Condition::Katakana);
        let mut prefix = vec![BOS];
        prefix.extend(ids(&vocab, "ネコガナク\t"));
        let expected = reference(&s, &prefix, &ids(&vocab, "猫が鳴く"));
        let cost = s.nll("ねこがなく", &["猫が鳴く".to_string()])[0];
        assert!((cost - expected).abs() < 1e-4);
    }

    /// 交互形の列を 1 本で読み、数える位置だけの負の対数尤度と、位置ごとの対数確率。
    fn reference_interleaved(s: &LmScorer, line: &str) -> (f64, Vec<f64>) {
        let mut whole = vec![BOS];
        whole.extend(ids(&s.vocab, line));
        let read = s.infer.read(&[Chain {
            state: &s.infer.init_state(),
            ids: &whole,
        }]);
        let d = s.infer.config().d_model;
        let log_prob = |pos: usize, id: u32| {
            f64::from(
                s.infer
                    .log_prob(&read.hidden[pos * d..(pos + 1) * d], read.lse[pos], id),
            )
        };
        let mut total = 0.0;
        let counted = crate::interleave::loss_mask(line);
        let mut per_char = Vec::new();
        for (i, &id) in whole[1..].iter().enumerate() {
            per_char.push(-log_prob(i, id));
            if counted[i] {
                total -= log_prob(i, id);
            }
        }
        total -= log_prob(whole.len() - 1, EOS);
        (total, per_char)
    }

    #[test]
    fn 交互形のコストは読みを区間に挟んだ列の出力側だけの値と一致する() {
        let vocab = Vocab::build(["猫が鳴く\tネコガナク\u{1e}"], 1);
        let s = scorer_with(vocab, Condition::Interleaved);
        let cost = s.nll("ねこがなく", &["猫が鳴く".to_string()])[0];
        let line = "\u{1e}ネコガ\t猫が\u{1e}ナク\t鳴く";
        let (expected, _) = reference_interleaved(&s, line);
        assert!((cost - expected).abs() < 1e-4, "{cost} vs {expected}");
        // 読みが違えばコストも変わる。
        assert_ne!(cost, s.nll("いぬがなく", &["猫が鳴く".to_string()])[0]);
    }

    #[test]
    fn 交互形はそのまま出す英字を読みに当てて採点する() {
        let vocab = Vocab::build(["Emacsで猫が鳴く\tデネコガナク\u{1e}"], 1);
        let s = scorer_with(vocab, Condition::Interleaved);
        // 英字はまずそのまま現れるものとして読みに当たる。
        let cost = s.nll("Emacsでねこがなく", &["Emacsで猫が鳴く".to_string()])[0];
        let line = "\u{1e}Emacsデ\tEmacsで\u{1e}ネコガ\t猫が\u{1e}ナク\t鳴く";
        let (expected, _) = reference_interleaved(&s, line);
        assert!(cost.is_finite());
        assert!((cost - expected).abs() < 1e-4, "{cost} vs {expected}");
    }

    #[test]
    fn 交互形の字の問い合わせは出力の字の位置で引く() {
        let vocab = Vocab::build(["猫が鳴く\tネコガナク\u{1e}"], 1);
        let s = scorer_with(vocab.clone(), Condition::Interleaved);
        let mut scoring = s.begin("ねこがなく");
        let scored = scoring.score(&["猫が鳴く".to_string()]);
        let line = "\u{1e}ネコガ\t猫が\u{1e}ナク\t鳴く";
        let (_, per_char) = reference_interleaved(&s, line);
        // 出力の字は列の 5, 6, 11, 12 番目。
        for (pos, i) in [(0, 5), (1, 6), (2, 11), (3, 12)] {
            let c = line.chars().nth(i).unwrap();
            let got = scoring.char_cost(&scored[0], pos, vocab.id(c));
            assert!(
                (got - per_char[i]).abs() < 1e-4,
                "{pos}: {got} vs {}",
                per_char[i]
            );
        }
        // 文の長さの位置は末尾の次、それより先は無限大。
        assert!(scoring.char_cost(&scored[0], 4, EOS).is_finite());
        assert!(scoring.char_cost(&scored[0], 5, EOS).is_infinite());
    }

    #[test]
    fn 接頭辞を共有する候補も単独で採点したのと同じ() {
        let s = scorer();
        let sentences: Vec<String> = ["猫が鳴く", "猫が", "猫", "鳴く猫が", "猫が鳴く"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let together = s.nll("", &sentences);
        for (sentence, cost) in sentences.iter().zip(&together) {
            let alone = s.nll("", std::slice::from_ref(sentence))[0];
            assert!((cost - alone).abs() < 1e-4, "{sentence}: {cost} vs {alone}");
        }
        assert_eq!(together[0], together[4]);
    }

    #[test]
    fn 接頭辞を多く共有する多数の候補をまとめて採点しても単独と同じ() {
        let s = scorer();
        // 4 文字の語彙から作る短い列は接頭辞を多く共有し、分岐の回数も多い。
        let alphabet = ['猫', 'が', '鳴', 'く'];
        let mut seed = 12345u64;
        let mut next = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as usize
        };
        let sentences: Vec<String> = (0..40)
            .map(|_| {
                let len = next() % 9;
                (0..len).map(|_| alphabet[next() % 4]).collect()
            })
            .collect();
        let together = s.nll("neko", &sentences);
        for (sentence, cost) in sentences.iter().zip(&together) {
            let alone = s.nll("neko", std::slice::from_ref(sentence))[0];
            assert!(
                (cost - alone).abs() < 1e-3,
                "{sentence:?}: {cost} vs {alone}"
            );
        }
    }

    #[test]
    fn 入力が直前の延長なら続きだけ読んでも同じコストになる() {
        let vocab = Vocab::build(["猫が鳴く\tネコガナク"], 1);
        let s = scorer_with(vocab.clone(), Condition::Katakana);
        let fresh = scorer_with(vocab, Condition::Katakana);
        let sentence = ["猫が鳴く".to_string()];
        // 1 字ずつ伸ばす。2 回目以降は直前の延長になる。
        for input in ["ね", "ねこ", "ねこが", "ねこがな", "ねこがなく"] {
            assert_eq!(
                s.nll(input, &sentence),
                fresh.nll(input, &sentence),
                "{input}"
            );
        }
        // 延長でない入力も、短くなる入力も、最初から読む。
        for input in ["いぬ", "ね", "ねこがなく"] {
            assert_eq!(
                s.nll(input, &sentence),
                fresh.nll(input, &sentence),
                "{input}"
            );
        }
    }

    #[test]
    fn 交互形は入力を伸ばして変換を重ねても最初から読んだのと同じ() {
        let vocab = Vocab::build(["猫が鳴く犬\tネコガナクイヌ\u{1e}"], 1);
        let s = scorer_with(vocab.clone(), Condition::Interleaved);
        let fresh = scorer_with(vocab, Condition::Interleaved);
        // 1 字ずつ伸ばす。先頭の区間の列は変わらないので持ち越した節を通る。
        let steps = [
            ("ねこ", vec!["猫"]),
            ("ねこが", vec!["猫が", "猫"]),
            ("ねこがな", vec!["猫が", "猫がな"]),
            ("ねこがなく", vec!["猫が鳴く", "猫がなく", "猫が"]),
            ("ねこがなくいぬ", vec!["猫が鳴く犬", "猫が鳴く", "猫が"]),
            // 短くなっても、別の入力でも同じ。
            ("ねこが", vec!["猫が"]),
            ("いぬ", vec!["犬"]),
        ];
        for (input, sentences) in steps {
            let sentences: Vec<String> = sentences.iter().map(|s| s.to_string()).collect();
            let got = s.nll(input, &sentences);
            let want = fresh_costs(&fresh, input, &sentences);
            for (g, w) in got.iter().zip(&want) {
                assert!((g - w).abs() < 1e-4, "{input}: {g} vs {w}");
            }
        }
    }

    #[test]
    fn 出力層を動かす前に始めた採点は節を持ち越さない() {
        let s = scorer();
        let sentences = ["猫が鳴く".to_string()];
        let want = {
            let mut scoring = s.begin("");
            let scored = scoring.score(&sentences);
            scoring.nll(&scored[0])
        };
        // 採点の途中で別のスレッドが出力層を動かしたのと同じ順序。
        let scoring = s.begin("");
        s.adapt("", "猫が鳴く", 0.1, crate::infer::Plastic::Head);
        drop(scoring);
        assert!(s.carried.lock().unwrap().is_none(), "古い節は残らない");
        let got = s.nll("", &sentences)[0];
        assert!(got != want, "動かした後の出力層で読み直す: {got} vs {want}");
        let fresh = fresh_costs(&s, "", &sentences)[0];
        assert!((got - fresh).abs() < 1e-4, "{got} vs {fresh}");
    }

    /// 持ち越しを使わずに最初から読んだコスト。
    fn fresh_costs(s: &LmScorer, input: &str, sentences: &[String]) -> Vec<f64> {
        *s.carried.lock().unwrap() = None;
        s.nll(input, sentences)
    }

    #[test]
    fn 同じ採点の場で続けて読んでも別々に読んだのと同じ() {
        let s = scorer();
        let mut scoring = s.begin("");
        let first = scoring.score(&["猫が鳴く".to_string()]);
        let second = scoring.score(&["猫がく".to_string(), "鳴く".to_string()]);
        let costs: Vec<f64> = first
            .iter()
            .chain(&second)
            .map(|x| scoring.nll(x))
            .collect();
        let alone = s.nll(
            "",
            &[
                "猫が鳴く".to_string(),
                "猫がく".to_string(),
                "鳴く".to_string(),
            ],
        );
        for (a, b) in costs.iter().zip(&alone) {
            assert!((a - b).abs() < 1e-4, "{a} vs {b}");
        }
    }

    #[test]
    fn 確定した文で動かすとその文のコストが下がり持ち越した場は使わない() {
        let vocab = Vocab::build(["猫が鳴く\tネコガナク\u{1e}"], 1);
        let s = scorer_with(vocab, Condition::Interleaved);
        let sentences = ["猫が鳴く".to_string()];
        let before = s.nll("ねこがなく", &sentences)[0];
        // 直前の変換の節を持ち越した状態で更新する。
        let reported = s.adapt("ねこがなく", "猫が鳴く", 0.05, Plastic::Head);
        assert!((reported - before).abs() < 1e-4, "{reported} vs {before}");
        let after = s.nll("ねこがなく", &sentences)[0];
        assert!(after < before, "{after} >= {before}");
        // 持ち越した節を使った値が、最初から読んだ値と一致する（古い lse を使っていない）。
        let fresh = fresh_costs(&s, "ねこがなく", &sentences)[0];
        assert!((after - fresh).abs() < 1e-4, "{after} vs {fresh}");
        // 交互形で数える位置は出力の字・区間の終わり・EOS の分だけ。
        let line = "\u{1e}ネコガ\t猫が\u{1e}ナク\t鳴く";
        let (expected, _) = reference_interleaved(&s, line);
        assert!((after - expected).abs() < 1e-4, "{after} vs {expected}");
    }
}
