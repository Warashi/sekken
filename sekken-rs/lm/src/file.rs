//! 言語モデルのファイル形式。設定・語彙・重みを 1 つにまとめて zstd で圧縮する。

use std::io::{Read, Write};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

use crate::condition::Condition;
use crate::infer::Infer;
use crate::config::{ModelConfig, Weight};
use crate::vocab::Vocab;

#[derive(Serialize, Deserialize)]
pub struct SavedModel {
    pub config: ModelConfig,
    pub vocab: Vocab,
    pub weights: Vec<Weight>,
    /// 「入力 \t 出力」で学習した条件付きモデルなら、採点時に前置する入力の種類。
    /// postcard は欠けた欄を既定値にできないので、本体の後ろに追加の欄として
    /// 別に書き、無ければ無条件と読む（旧ファイルとの互換のため）。
    #[serde(skip)]
    pub condition: Condition,
}

/// 本体の後ろに付ける追加の欄。旧ファイルは `Trailer1` までしか持たない。
#[derive(Serialize, Deserialize, Default)]
struct Trailer1 {
    conditional: bool,
}

/// `Trailer1` の後ろに付ける欄。新しい欄はここに足す。
#[derive(Serialize, Deserialize)]
struct Trailer2 {
    condition: Condition,
}

/// 本体の後ろの欄から条件の種類を読む。欄が無ければ無条件、
/// `Trailer1` までなら条件付きはローマ字入力（追加の欄より前の形式）。
fn read_condition(rest: &[u8]) -> Result<Condition> {
    if rest.is_empty() {
        return Ok(Condition::None);
    }
    let (trailer1, rest): (Trailer1, &[u8]) =
        postcard::take_from_bytes(rest).context("deserialize lm trailer")?;
    if !rest.is_empty() {
        let trailer2: Trailer2 = postcard::from_bytes(rest).context("deserialize lm trailer")?;
        return Ok(trailer2.condition);
    }
    Ok(if trailer1.conditional {
        Condition::Roman
    } else {
        Condition::None
    })
}

impl SavedModel {
    pub fn save(&self, w: impl Write) -> Result<()> {
        let mut bytes = postcard::to_stdvec(self).context("serialize lm")?;
        let trailer1 = Trailer1 {
            conditional: self.condition != Condition::None,
        };
        bytes.extend(postcard::to_stdvec(&trailer1).context("serialize lm trailer")?);
        let trailer2 = Trailer2 {
            condition: self.condition,
        };
        bytes.extend(postcard::to_stdvec(&trailer2).context("serialize lm trailer")?);
        let mut enc = zstd::Encoder::new(w, 9).context("zstd encoder")?;
        enc.write_all(&bytes).context("write lm")?;
        enc.finish().context("finish zstd")?;
        Ok(())
    }

    pub fn load(r: impl Read) -> Result<SavedModel> {
        let mut dec = zstd::Decoder::new(r).context("zstd decoder")?;
        let mut bytes = Vec::new();
        dec.read_to_end(&mut bytes).context("read lm")?;
        let (mut saved, rest): (SavedModel, &[u8]) =
            postcard::take_from_bytes(&bytes).context("deserialize lm")?;
        saved.condition = read_condition(rest)?;
        saved.vocab.rebuild_index();
        Ok(saved)
    }

    /// 採点専用の推論経路を組み立てる。行列積は使える CPU の数だけ並列にする。
    pub fn infer(&self) -> Result<Infer> {
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
        Ok(Infer::new(self.config.clone(), &self.weights)
            .context("build lm")?
            .with_threads(threads))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infer::{Chain, Infer};
    use crate::testing::random_weights;

    #[test]
    fn 保存して読み直すと同じ重みと語彙() {
        let vocab = Vocab::build(["猫犬"], 1);
        let config = ModelConfig {
            vocab_size: vocab.len(),
            d_model: 8,
            n_layers: 1,
            n_heads: 1,
            head_dim: 8,
            d_state: 4,
            mlp_dim: 16,
        };
        let weights = random_weights(&config, 1);
        let ids = [0u32, 3, 4];
        let log_prob = |infer: &Infer| {
            let read = infer.read(&[Chain {
                state: &infer.init_state(),
                ids: &ids,
            }]);
            let d = config.d_model;
            infer.log_prob(&read.hidden[2 * d..3 * d], read.lse[2], 3)
        };
        let before = log_prob(&Infer::new(config.clone(), &weights).unwrap());

        let saved = SavedModel {
            config: config.clone(),
            vocab: vocab.clone(),
            weights,
            condition: Condition::Katakana,
        };
        let mut buf = Vec::new();
        saved.save(&mut buf).unwrap();
        let loaded = SavedModel::load(buf.as_slice()).unwrap();
        assert_eq!(loaded.condition, Condition::Katakana);
        assert_eq!(loaded.weights, saved.weights);
        assert_eq!(before, log_prob(&loaded.infer().unwrap()));
        assert_eq!(loaded.vocab.id('犬'), vocab.id('犬'));
    }

    #[test]
    fn 追加の欄が無い旧ファイルは無条件として読む() {
        let vocab = Vocab::build(["猫犬"], 1);
        let saved = SavedModel {
            config: ModelConfig {
                vocab_size: vocab.len(),
                d_model: 8,
                n_layers: 1,
                n_heads: 1,
                head_dim: 8,
                d_state: 4,
                mlp_dim: 16,
            },
            vocab,
            weights: Vec::new(),
            condition: Condition::Katakana,
        };
        // 旧形式は本体だけを zstd で包んだもの。
        let loaded =
            SavedModel::load(zstd_wrap(postcard::to_stdvec(&saved).unwrap()).as_slice()).unwrap();
        assert_eq!(loaded.condition, Condition::None);
        assert_eq!(loaded.config.d_model, 8);
    }

    #[test]
    fn 追加の欄が_1_つだけの旧ファイルはローマ字入力の条件付きとして読む() {
        let vocab = Vocab::build(["猫犬"], 1);
        let saved = SavedModel {
            config: ModelConfig {
                vocab_size: vocab.len(),
                d_model: 8,
                n_layers: 1,
                n_heads: 1,
                head_dim: 8,
                d_state: 4,
                mlp_dim: 16,
            },
            vocab,
            weights: Vec::new(),
            condition: Condition::None,
        };
        let mut body = postcard::to_stdvec(&saved).unwrap();
        body.extend(postcard::to_stdvec(&Trailer1 { conditional: true }).unwrap());
        let loaded = SavedModel::load(zstd_wrap(body).as_slice()).unwrap();
        assert_eq!(loaded.condition, Condition::Roman);
    }

    fn zstd_wrap(body: Vec<u8>) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut enc = zstd::Encoder::new(&mut buf, 9).unwrap();
        enc.write_all(&body).unwrap();
        enc.finish().unwrap();
        buf
    }
}
