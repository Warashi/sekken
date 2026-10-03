//! 確定した文で動かした出力層の保存形式。
//!
//! 動かすのは出力層（`head.weight`、`head.bias`）だけなので、モデル全体ではなく出力層と、元にした配布モデルの
//! 指紋だけを保存する。配布モデルを入れ替えたら指紋が合わなくなり、古い出力層は読まずに配布モデルから
//! 学習し直す（確定した文は保存しないので、新しいモデルで学習し直す元は無い）。

use std::io::{Read, Write};

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};

use crate::config::Weight;

const MAGIC: &[u8; 4] = b"SKAH";
const FORMAT_VERSION: u32 = 1;

/// 配布モデルのファイルの中身の指紋（FNV-1a、64 bit）。入れ替わりを見分けるだけで、改竄は防がない。
pub fn fingerprint(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// 動かした出力層。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptedHead {
    /// 元にした配布モデルの指紋（`fingerprint`）。
    pub base: u64,
    /// 出力層の重み（`Infer::head_weights`）。
    pub weights: Vec<Weight>,
}

impl AdaptedHead {
    pub fn save(&self, w: impl Write) -> Result<()> {
        let mut bytes = MAGIC.to_vec();
        bytes.extend(FORMAT_VERSION.to_le_bytes());
        bytes.extend(postcard::to_stdvec(self).context("serialize adapted head")?);
        let mut enc = zstd::Encoder::new(w, 3).context("zstd encoder")?;
        enc.write_all(&bytes).context("write adapted head")?;
        enc.finish().context("finish zstd")?;
        Ok(())
    }

    /// 読む。この形式でないファイル（2026-10 まではモデル全体の写しを保存していた）はエラー。
    pub fn load(r: impl Read) -> Result<AdaptedHead> {
        let mut bytes = Vec::new();
        zstd::Decoder::new(r)
            .context("zstd decoder")?
            .read_to_end(&mut bytes)
            .context("read adapted head")?;
        if bytes.len() < 8 || &bytes[..4] != MAGIC {
            bail!("not an adapted output layer");
        }
        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        if version != FORMAT_VERSION {
            bail!("adapted output layer format version {version} is not supported");
        }
        postcard::from_bytes(&bytes[8..]).context("deserialize adapted head")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 指紋は中身で決まり_1_バイト違えば変わる() {
        assert_eq!(fingerprint(b"abc"), fingerprint(b"abc"));
        assert_ne!(fingerprint(b"abc"), fingerprint(b"abd"));
        // FNV-1a の既知の値。
        assert_eq!(fingerprint(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn 保存して読み直すと同じ() {
        let head = AdaptedHead {
            base: 42,
            weights: vec![("head.bias".to_string(), vec![2], vec![0.5, -1.0])],
        };
        let mut buf = Vec::new();
        head.save(&mut buf).unwrap();
        assert_eq!(AdaptedHead::load(buf.as_slice()).unwrap(), head);
    }

    #[test]
    fn 別の形式のファイルは読まない() {
        let mut buf = Vec::new();
        let mut enc = zstd::Encoder::new(&mut buf, 3).unwrap();
        enc.write_all(b"not an adapted head").unwrap();
        enc.finish().unwrap();
        assert!(AdaptedHead::load(buf.as_slice()).is_err());
    }
}
