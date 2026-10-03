//! 確定した文で言語モデルの出力層を動かし、書ける場所に保存する。
//!
//! 変換と同じ採点器を共有し、学習は別のスレッドでやる。変換の応答を
//! 待たせないためで、要求を受けた側は文を送るだけで返す。
//!
//! `--lm` のファイルは配布物で書けないことがあるので、上書きはせず、動かした出力層だけを
//! `--adapted-lm` に書く（`sekken_lm::adapted`）。次の起動は `--lm` を読んでから出力層を
//! 差し替える。出力層は元にした `--lm` の指紋を持ち、`--lm` が入れ替わっていれば読まずに
//! `--lm` のまま学習し直す。

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;

use anyhow::{Context as _, Result};
use sekken_lm::adapted::{AdaptedHead, fingerprint};
use sekken_lm::file::SavedModel;
use sekken_lm::infer::Plastic;
use sekken_lm::scorer::LmScorer;

#[derive(clap::Args, Clone)]
pub struct AdaptArgs {
    /// 確定した文で出力層を動かすときの学習率
    #[arg(long, default_value_t = 3e-3)]
    pub adapt_lr: f32,
    /// 動かした出力層の保存先。あれば `--lm` の出力層をこれに差し替える（`--lm` が同じときだけ）
    #[arg(long)]
    pub adapted_lm: Option<PathBuf>,
}

/// `--lm` を読み、動かした出力層があれば差し替える。`--lm` の指紋も返す（保存する出力層に付ける）。
/// 出力層が別の `--lm` から作られたもの、読めないもの、旧形式（モデル全体の写し）なら、理由を
/// stderr に出して `--lm` のまま使う。次の保存で上書きされる。
pub fn load_model(lm: &Path, adapted: Option<&Path>) -> Result<(SavedModel, u64)> {
    let bytes = std::fs::read(lm).with_context(|| format!("read {}", lm.display()))?;
    let base = fingerprint(&bytes);
    let mut saved =
        crate::engine::load_lm_bytes(&bytes).with_context(|| format!("load {}", lm.display()))?;
    if let Some(path) = adapted.filter(|p| p.exists()) {
        let head = std::fs::File::open(path)
            .map_err(anyhow::Error::from)
            .and_then(|f| AdaptedHead::load(std::io::BufReader::new(f)));
        // stderr が閉じていても panic せずに続ける。
        match head {
            Ok(head) if head.base == base => saved.replace_weights(head.weights),
            Ok(_) => {
                let _ = writeln!(
                    std::io::stderr(),
                    "sekken: {} was adapted from another model; starting over from {}",
                    path.display(),
                    lm.display()
                );
            }
            Err(err) => {
                let _ = writeln!(
                    std::io::stderr(),
                    "sekken: failed to load {}, starting over from {}: {err:#}",
                    path.display(),
                    lm.display()
                );
            }
        }
    }
    Ok((saved, base))
}

/// 読み込みが終わった採点器と、その元になった `--lm` の指紋。
pub struct Ready {
    pub scorer: Arc<LmScorer>,
    pub base: u64,
}

enum Job {
    Adapt { input: String, sentence: String },
    Flush(mpsc::Sender<()>),
}

/// 確定した文を学習のスレッドに渡す窓口。
pub struct Adapter {
    tx: mpsc::Sender<Job>,
}

impl Adapter {
    /// 学習のスレッドを起こす。`ready` はエンジンの読み込みが終わってから届く。
    /// 届くまでに来た文は溜めておき、届いてから順に学習する。
    pub fn spawn(ready: mpsc::Receiver<Ready>, args: AdaptArgs) -> Adapter {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || work(ready, rx, &args));
        Adapter { tx }
    }

    /// 入力 `input`（全区間のかな）に対して確定した文 `sentence` を学習に回す。
    pub fn adapt(&self, input: String, sentence: String) {
        let _ = self.tx.send(Job::Adapt { input, sentence });
    }

    /// 溜まった文を学習し、保存し終えるまで待つ。
    pub fn flush(&self) {
        let (tx, rx) = mpsc::channel();
        if self.tx.send(Job::Flush(tx)).is_ok() {
            let _ = rx.recv();
        }
    }
}

/// モデルの読み込みを待たない。読み込みが終わるまでに来た文は溜めておき、
/// 終わってから順に学習する。読み込み中に終了しても保存を待たせないため、
/// 溜めたまま `Flush` が来れば学習も保存もせずに返す。読み込みに失敗したら
/// 文は捨てる。
fn work(ready: mpsc::Receiver<Ready>, rx: mpsc::Receiver<Job>, args: &AdaptArgs) {
    let mut model: Option<Ready> = None;
    let mut broken = false;
    let mut pending: Vec<(String, String)> = Vec::new();
    let mut moved = false;
    for job in rx {
        if model.is_none() && !broken {
            match ready.try_recv() {
                Ok(r) => model = Some(r),
                Err(mpsc::TryRecvError::Disconnected) => {
                    broken = true;
                    pending.clear();
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(model) = &model {
            for (input, sentence) in pending.drain(..) {
                model
                    .scorer
                    .adapt(&input, &sentence, args.adapt_lr, Plastic::Head);
                moved = true;
            }
        }
        match job {
            Job::Adapt { input, sentence } => match &model {
                Some(model) => {
                    model
                        .scorer
                        .adapt(&input, &sentence, args.adapt_lr, Plastic::Head);
                    moved = true;
                }
                None if !broken => pending.push((input, sentence)),
                None => {}
            },
            Job::Flush(ack) => {
                if let (true, Some(model), Some(path)) =
                    (moved, model.as_ref(), args.adapted_lm.as_deref())
                {
                    let head = AdaptedHead {
                        base: model.base,
                        weights: model.scorer.infer().head_weights(),
                    };
                    match write(path, |w| head.save(w)) {
                        Ok(()) => moved = false,
                        Err(err) => {
                            let _ = writeln!(
                                std::io::stderr(),
                                "sekken: failed to save {}: {err:#}",
                                path.display()
                            );
                        }
                    }
                }
                let _ = ack.send(());
            }
        }
    }
}

/// `path` に `save` で書く。書いている途中で終了しても前のファイルを
/// 失わないよう、隣に書いてから置き換える。
pub fn write(
    path: &Path,
    save: impl FnOnce(std::io::BufWriter<std::fs::File>) -> Result<()>,
) -> Result<()> {
    let tmp = path.with_extension("tmp");
    let file = std::fs::File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?;
    save(std::io::BufWriter::new(file)).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("rename {} to {}", tmp.display(), path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use sekken_lm::condition::Condition;
    use sekken_lm::config::ModelConfig;
    use sekken_lm::vocab::Vocab;

    use super::*;

    /// 形だけ合った重みの、読み書きと学習ができる小さなモデル。`seed` で重みを変える。
    fn small_model(seed: u64) -> SavedModel {
        let vocab = Vocab::build(["猫が鳴く"], 1);
        let config = ModelConfig {
            vocab_size: vocab.len(),
            d_model: 8,
            n_layers: 1,
            n_heads: 1,
            head_dim: 8,
            d_state: 4,
            mlp_dim: 16,
        };
        SavedModel {
            weights: sekken_lm::testing::random_weights(&config, seed),
            config,
            vocab,
            condition: Condition::None,
        }
    }

    fn save_model(saved: &SavedModel, path: &Path) {
        write(path, |w| saved.save(w)).unwrap();
    }

    fn head_weight(saved: &SavedModel) -> &[f32] {
        &saved
            .weights
            .iter()
            .find(|(n, _, _)| n == "head.weight")
            .unwrap()
            .2
    }

    fn dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sekken-adapt-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// `base` の指紋を付けて、出力層を全部 `value` にした保存先を書く。
    fn save_head(base: &Path, value: f32, path: &Path) {
        let saved = crate::engine::load_lm(base).unwrap();
        let weights = saved
            .weights
            .iter()
            .filter(|(n, _, _)| n.starts_with("head."))
            .map(|(n, shape, w)| (n.clone(), shape.clone(), vec![value; w.len()]))
            .collect();
        let head = AdaptedHead {
            base: fingerprint(&std::fs::read(base).unwrap()),
            weights,
        };
        write(path, |w| head.save(w)).unwrap();
    }

    #[test]
    fn 保存は隣に書いてから置き換える() {
        let dir = dir();
        let path = dir.join("lm.zst");
        save_model(&small_model(1), &path);
        save_model(&small_model(2), &path);
        assert_eq!(
            head_weight(&crate::engine::load_lm(&path).unwrap()),
            head_weight(&small_model(2))
        );
        assert!(
            !path.with_extension("tmp").exists(),
            "途中のファイルは残らない"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn 書けない場所への保存は理由を返す() {
        let err = write(Path::new("/nonexistent-dir/lm.zst"), |_| Ok(())).unwrap_err();
        assert!(format!("{err:#}").contains("create"), "{err:#}");
    }

    #[test]
    fn 同じ_lm_から動かした出力層だけを差し替える() {
        let dir = dir();
        let base = dir.join("base.zst");
        let adapted = dir.join("adapted.zst");
        save_model(&small_model(1), &base);
        save_head(&base, 0.25, &adapted);
        let (saved, fp) = load_model(&base, Some(&adapted)).unwrap();
        assert!(head_weight(&saved).iter().all(|&w| w == 0.25));
        assert_eq!(fp, fingerprint(&std::fs::read(&base).unwrap()));
        // 保存先がまだ無い初回は `--lm` のまま。
        let (saved, _) = load_model(&base, Some(&dir.join("none.zst"))).unwrap();
        assert_eq!(head_weight(&saved), head_weight(&small_model(1)));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn lm_が入れ替わっていれば動かした出力層は読まない() {
        let dir = dir();
        let base = dir.join("base.zst");
        let adapted = dir.join("adapted.zst");
        save_model(&small_model(1), &base);
        save_head(&base, 0.25, &adapted);
        save_model(&small_model(2), &base);
        let (saved, _) = load_model(&base, Some(&adapted)).unwrap();
        assert_eq!(head_weight(&saved), head_weight(&small_model(2)));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn 壊れた保存先と旧形式の写しは捨てて元のモデルを読む() {
        let dir = dir();
        let base = dir.join("base.zst");
        let adapted = dir.join("adapted.zst");
        save_model(&small_model(1), &base);
        std::fs::write(&adapted, b"not a model").unwrap();
        let (saved, _) = load_model(&base, Some(&adapted)).unwrap();
        assert_eq!(head_weight(&saved), head_weight(&small_model(1)));
        // 2026-10 まではモデル全体の写しを保存していた。
        save_model(&small_model(2), &adapted);
        let (saved, _) = load_model(&base, Some(&adapted)).unwrap();
        assert_eq!(head_weight(&saved), head_weight(&small_model(1)));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn adapter(ready: mpsc::Receiver<Ready>) -> Adapter {
        Adapter::spawn(
            ready,
            AdaptArgs {
                adapt_lr: 3e-3,
                adapted_lm: None,
            },
        )
    }

    #[test]
    fn 学習する文が無ければ読み込みを待たずに保存を終える() {
        // 読み込みが終わらないまま終了する場合。`flush` は返らなければならない。
        let (_ready_tx, ready_rx) = mpsc::channel();
        adapter(ready_rx).flush();
    }

    #[test]
    fn 読み込み中に文が来ても_flush_は読み込みを待たない() {
        // 読み込みが終わらないまま文を送って終了する場合。以前は最初の文で
        // 読み込みを待ち始め、その後ろに並んだ flush が返らなかった。
        let (_ready_tx, ready_rx) = mpsc::channel();
        let adapter = adapter(ready_rx);
        adapter.adapt("ねこ".to_string(), "猫".to_string());
        adapter.flush();
    }

    #[test]
    fn 学習した出力層を保存し_次の起動で読み直せる() {
        let dir = dir();
        let base = dir.join("base.zst");
        let adapted = dir.join("adapted.zst");
        save_model(&small_model(1), &base);
        let (ready_tx, ready_rx) = mpsc::channel();
        let adapter = Adapter::spawn(
            ready_rx,
            AdaptArgs {
                adapt_lr: 0.1,
                adapted_lm: Some(adapted.clone()),
            },
        );
        // 読み込みが終わる前に来た文も、終わってから学習される。
        adapter.adapt("ねこ".to_string(), "猫".to_string());
        let (saved, fp) = load_model(&base, Some(&adapted)).unwrap();
        let scorer = Arc::new(LmScorer::from_saved(&saved).unwrap());
        ready_tx
            .send(Ready {
                scorer: scorer.clone(),
                base: fp,
            })
            .unwrap();
        adapter.adapt("ねこ".to_string(), "猫が鳴く".to_string());
        adapter.flush();
        let (reloaded, _) = load_model(&base, Some(&adapted)).unwrap();
        assert_ne!(
            head_weight(&reloaded),
            head_weight(&saved),
            "保存した出力層は学習で動いている"
        );
        let head = scorer.infer().head_weights();
        assert_eq!(head_weight(&reloaded), &head[0].2[..]);
        assert!(
            reloaded.weights.iter().any(|(n, _, _)| n == "head.bias"),
            "bias も一緒に保存する"
        );
        assert!(
            std::fs::metadata(&adapted).unwrap().len() < std::fs::metadata(&base).unwrap().len(),
            "モデル全体ではなく出力層だけを書く"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn 読み込みに失敗しても送った文と_flush_は返る() {
        let (ready_tx, ready_rx) = mpsc::channel();
        let adapter = adapter(ready_rx);
        drop(ready_tx);
        adapter.adapt("ねこ".to_string(), "猫".to_string());
        adapter.flush();
    }
}
