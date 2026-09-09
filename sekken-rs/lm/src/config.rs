//! モデルの形と重みの表現。学習側と採点側の両方が読む。

use serde::{Deserialize, Serialize};

// 欄の順序と型はファイル形式（postcard）そのもの。変えると既存の lm.zst が読めなくなる。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelConfig {
    pub vocab_size: usize,
    pub d_model: usize,
    pub n_layers: usize,
    pub n_heads: usize,
    pub head_dim: usize,
    pub d_state: usize,
    pub mlp_dim: usize,
}

/// 名前・形・値で表した 1 つの重み。
pub type Weight = (String, Vec<usize>, Vec<f32>);
