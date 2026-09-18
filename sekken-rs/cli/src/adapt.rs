//! 確定した文で言語モデルの出力層を動かし、書ける場所に保存する。
//!
//! 変換と同じ採点器を共有し、学習は別のスレッドでやる。変換の応答を
//! 待たせないためで、要求を受けた側は文を送るだけで返す。
//!
//! `--lm` のファイルは配布物で書けないことがあるので、上書きはせず
//! `--adapted-lm` に書く。次の起動はそちらを先に読む。

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;

use anyhow::{Context as _, Result};
use sekken_lm::file::SavedModel;
use sekken_lm::infer::Plastic;
use sekken_lm::scorer::LmScorer;

#[derive(clap::Args, Clone)]
pub struct AdaptArgs {
    /// 確定した文で出力層を動かすときの学習率
    #[arg(long, default_value_t = 3e-3)]
    pub adapt_lr: f32,
    /// 動かした出力層の保存先。あれば `--lm` より先に読む
    #[arg(long)]
    pub adapted_lm: Option<PathBuf>,
}

/// 動かした出力層があればそれを、無ければ `--lm` を読む。
/// 動かした側が壊れていても入力はできるべきなので、読めなければ理由を
/// stderr に出して `--lm` に戻る。
pub fn load_model(lm: &Path, adapted: Option<&Path>) -> Result<SavedModel> {
    if let Some(path) = adapted.filter(|p| p.exists()) {
        match crate::engine::load_lm(path) {
            Ok(saved) => return Ok(saved),
            // stderr が閉じていても panic せずに続ける。
            Err(err) => {
                let _ = writeln!(
                    std::io::stderr(),
                    "sekken: failed to load {}, falling back to the base model: {err:#}",
                    path.display()
                );
            }
        }
    }
    crate::engine::load_lm(lm)
}

/// 読み込みが終わった採点器と、その元になったファイルの中身。
/// ファイルの中身は、動かした出力層を書き戻して保存するために持ち続ける。
pub struct Ready {
    pub scorer: Arc<LmScorer>,
    pub saved: SavedModel,
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
        std::thread::spawn(move || match ready.recv() {
            Ok(ready) => work(ready, rx, &args),
            // 読み込みに失敗した。溜まった文は捨てるが、待っている側は放さない。
            Err(_) => {
                for job in rx {
                    if let Job::Flush(ack) = job {
                        let _ = ack.send(());
                    }
                }
            }
        });
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

fn work(ready: Ready, rx: mpsc::Receiver<Job>, args: &AdaptArgs) {
    let Ready { scorer, mut saved } = ready;
    let mut moved = false;
    for job in rx {
        match job {
            Job::Adapt { input, sentence } => {
                scorer.adapt(&input, &sentence, args.adapt_lr, Plastic::Head);
                moved = true;
            }
            Job::Flush(ack) => {
                if let (true, Some(path)) = (moved, args.adapted_lm.as_deref()) {
                    saved.replace_weights(scorer.infer().head_weights());
                    match write(&saved, path) {
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

/// モデルを `path` に保存する。書いている途中で終了しても前のファイルを
/// 失わないよう、隣に書いてから置き換える。
pub fn write(saved: &SavedModel, path: &Path) -> Result<()> {
    let tmp = path.with_extension("tmp");
    let file = std::fs::File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?;
    saved
        .save(std::io::BufWriter::new(file))
        .with_context(|| format!("write {}", tmp.display()))?;
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

    /// 重みの無い、読み書きだけを確かめるためのモデル。`d_model` で見分ける。
    fn model(d_model: usize) -> SavedModel {
        let vocab = Vocab::build(["猫犬"], 1);
        SavedModel {
            config: ModelConfig {
                vocab_size: vocab.len(),
                d_model,
                n_layers: 1,
                n_heads: 1,
                head_dim: 8,
                d_state: 4,
                mlp_dim: 16,
            },
            vocab,
            weights: Vec::new(),
            condition: Condition::None,
        }
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

    #[test]
    fn 保存は隣に書いてから置き換える() {
        let dir = dir();
        let path = dir.join("lm.zst");
        write(&model(8), &path).unwrap();
        assert_eq!(
            crate::engine::load_lm(&path).unwrap().config.d_model,
            8,
            "保存したモデルを読み直せる"
        );
        write(&model(16), &path).unwrap();
        assert_eq!(crate::engine::load_lm(&path).unwrap().config.d_model, 16);
        assert!(
            !path.with_extension("tmp").exists(),
            "途中のファイルは残らない"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn 書けない場所への保存は理由を返す() {
        let err = write(&model(8), Path::new("/nonexistent-dir/lm.zst")).unwrap_err();
        assert!(format!("{err:#}").contains("create"), "{err:#}");
    }

    #[test]
    fn 動かした出力層があればそれを先に読む() {
        let dir = dir();
        let base = dir.join("base.zst");
        let adapted = dir.join("adapted.zst");
        write(&model(8), &base).unwrap();
        write(&model(16), &adapted).unwrap();
        assert_eq!(load_model(&base, None).unwrap().config.d_model, 8);
        assert_eq!(
            load_model(&base, Some(&adapted)).unwrap().config.d_model,
            16
        );
        // 保存先がまだ無い初回は `--lm` を読む。
        assert_eq!(
            load_model(&base, Some(&dir.join("none.zst")))
                .unwrap()
                .config
                .d_model,
            8
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn 壊れた保存先は捨てて元のモデルを読む() {
        let dir = dir();
        let base = dir.join("base.zst");
        let adapted = dir.join("adapted.zst");
        write(&model(8), &base).unwrap();
        std::fs::write(&adapted, b"not a model").unwrap();
        assert_eq!(load_model(&base, Some(&adapted)).unwrap().config.d_model, 8);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn 読み込みに失敗しても送った文と_flush_は返る() {
        let (ready_tx, ready_rx) = mpsc::channel();
        let adapter = Adapter::spawn(
            ready_rx,
            AdaptArgs {
                adapt_lr: 3e-3,
                adapted_lm: None,
            },
        );
        drop(ready_tx);
        adapter.adapt("ねこ".to_string(), "猫".to_string());
        adapter.flush();
    }
}
