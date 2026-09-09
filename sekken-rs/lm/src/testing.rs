//! テスト用に、学習せずに形だけ合った重みを作る。
//! 値は決定的な擬似乱数で、学習側の初期化と同じ範囲に散らす。

use crate::config::{ModelConfig, Weight};

struct Lcg(u64);

impl Lcg {
    /// [lo, up) の一様乱数。
    fn uniform(&mut self, lo: f32, up: f32) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        lo + (up - lo) * ((self.0 >> 40) as f32 / (1u64 << 24) as f32)
    }
}

/// `cfg` に合う名前と形の重みを、種 `seed` から作る。
pub fn random_weights(cfg: &ModelConfig, seed: u64) -> Vec<Weight> {
    let mut rng = Lcg(seed);
    let (h, n, d, p) = (cfg.n_heads, cfg.d_state, cfg.d_model, cfg.head_dim);
    let inner = h * p;
    let proj_width = 2 * inner + 2 * n + h + h * n / 2 + h;
    let mut out = Vec::new();
    let mut push = |name: String, shape: Vec<usize>, lo: f32, up: f32| {
        let len = shape.iter().product();
        let values = (0..len).map(|_| rng.uniform(lo, up)).collect();
        out.push((name, shape, values));
    };
    // 線形層は candle の既定（Kaiming 一様）と同じ幅にする。
    let linear = |fan_in: usize| (3.0f32 / fan_in as f32).sqrt();
    push("embed.weight".into(), vec![cfg.vocab_size, d], -1.0, 1.0);
    for i in 0..cfg.n_layers {
        let p = |s: &str| format!("block{i}.{s}");
        push(p("norm1.weight"), vec![d], 1.0, 1.0);
        push(p("mixer.in_proj.weight"), vec![proj_width, d], -linear(d), linear(d));
        push(p("mixer.dt_bias"), vec![h], -6.9, -2.25);
        push(p("mixer.a_log"), vec![h], 0.0, 16f32.ln());
        push(p("mixer.theta_bias"), vec![h, n / 2], 0.5, 20.0);
        push(p("mixer.lambda_bias"), vec![h], 0.0, 0.0);
        push(p("mixer.b_bias"), vec![h, n], 1.0, 1.0);
        push(p("mixer.c_bias"), vec![h, n], 1.0, 1.0);
        push(p("mixer.d_skip"), vec![h], 1.0, 1.0);
        push(
            p("mixer.out_proj.weight"),
            vec![d, inner],
            -linear(inner),
            linear(inner),
        );
        push(p("norm2.weight"), vec![d], 1.0, 1.0);
        push(p("mlp.gate.weight"), vec![cfg.mlp_dim, d], -linear(d), linear(d));
        push(p("mlp.up.weight"), vec![cfg.mlp_dim, d], -linear(d), linear(d));
        push(
            p("mlp.down.weight"),
            vec![d, cfg.mlp_dim],
            -linear(cfg.mlp_dim),
            linear(cfg.mlp_dim),
        );
    }
    push("norm.weight".into(), vec![d], 1.0, 1.0);
    push("head.weight".into(), vec![cfg.vocab_size, d], -linear(d), linear(d));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infer::Infer;

    #[test]
    fn 作った重みで推論経路が組める() {
        let cfg = ModelConfig {
            vocab_size: 10,
            d_model: 16,
            n_layers: 2,
            n_heads: 2,
            head_dim: 8,
            d_state: 4,
            mlp_dim: 32,
        };
        let infer = Infer::new(cfg.clone(), &random_weights(&cfg, 1)).unwrap();
        assert_eq!(infer.config(), &cfg);
    }

    #[test]
    fn 同じ種なら同じ重み() {
        let cfg = ModelConfig {
            vocab_size: 4,
            d_model: 8,
            n_layers: 1,
            n_heads: 1,
            head_dim: 8,
            d_state: 4,
            mlp_dim: 16,
        };
        assert_eq!(random_weights(&cfg, 7), random_weights(&cfg, 7));
        assert_ne!(random_weights(&cfg, 7), random_weights(&cfg, 8));
    }
}
