//! 採点専用の推論経路。重みを `Vec<f32>` に展開し、線形層は 1 回の読みの全行を
//! 1 つの行列積に通し、Mamba-3 の漸化式だけを行ごとに回す。
//!
//! candle の forward は、行列積のたびに gemm crate が行数に関わらず重みを
//! 詰め直すのと、演算ごとにテンソルを確保して dispatch するのとで、
//! 数文字の候補を数本読むだけでも forward 1 回に数 ms の固定費が付く。
//! ここでは重みをそのまま流し読みし、1 回の読みの費用を行列積の計算量だけにする。
//! 1 字ずつ行列積を回すと 15 MB の重みを毎字読んで帯域律速になるので、
//! 同じ回に読む連鎖の全位置をまとめて行列積に通し、重みは 1 回だけ読む。
//!
//! 各位置について in_proj の出力（係数の元）を残しておけば、漸化式だけを
//! 後から回し直して途中の節の状態を作れる。状態（層ごとに 2 × H × P × N）は
//! 連鎖の終わりだけ持ち、途中の節は係数から作る。

use anyhow::{Context as _, Result, bail};

use crate::config::{ModelConfig, Weight};
use crate::simd;

struct Block {
    norm1: Vec<f32>,
    /// (proj_width, d_model)
    in_proj: Vec<f32>,
    dt_bias: Vec<f32>,
    /// -exp(a_log)
    a: Vec<f32>,
    /// (heads, d_state / 2)
    theta_bias: Vec<f32>,
    lambda_bias: Vec<f32>,
    /// (heads, d_state)
    b_bias: Vec<f32>,
    c_bias: Vec<f32>,
    d_skip: Vec<f32>,
    /// (d_model, inner)
    out_proj: Vec<f32>,
    norm2: Vec<f32>,
    /// (mlp_dim, d_model)
    gate: Vec<f32>,
    up: Vec<f32>,
    /// (d_model, mlp_dim)
    down: Vec<f32>,
}

pub struct Infer {
    cfg: ModelConfig,
    /// (vocab, d_model)
    embed: Vec<f32>,
    blocks: Vec<Block>,
    norm: Vec<f32>,
    /// (vocab, d_model)
    head: Vec<f32>,
    /// 行列積を分けるスレッド数。
    threads: usize,
}

/// 1 本の連鎖。`state` の続きとして `ids` を順に読む。
pub struct Chain<'a> {
    pub state: &'a [f32],
    pub ids: &'a [u32],
}

/// 連鎖をまとめて読んだ結果。位置は連鎖の順に並ぶ。
pub struct Read {
    /// 位置ごとの各層の in_proj 出力（n_layers × proj_width）。状態の作り直しに使う。
    pub proj: Vec<f32>,
    /// 位置ごとの最終層の正規化済み出力（d_model）。logits はこれと head の内積。
    pub hidden: Vec<f32>,
    /// 位置ごとの logits の log-sum-exp。
    pub lse: Vec<f32>,
    /// 連鎖ごとの読み終えた状態。
    pub states: Vec<Vec<f32>>,
}

impl Infer {
    pub fn new(cfg: ModelConfig, weights: &[Weight]) -> Result<Infer> {
        let get = |name: &str| -> Result<Vec<f32>> {
            weights
                .iter()
                .find(|(n, _, _)| n == name)
                .map(|(_, _, v)| v.clone())
                .with_context(|| format!("weight {name} not found"))
        };
        let mut blocks = Vec::with_capacity(cfg.n_layers);
        for i in 0..cfg.n_layers {
            let p = |s: &str| format!("block{i}.{s}");
            blocks.push(Block {
                norm1: get(&p("norm1.weight"))?,
                in_proj: get(&p("mixer.in_proj.weight"))?,
                dt_bias: get(&p("mixer.dt_bias"))?,
                a: get(&p("mixer.a_log"))?.iter().map(|a| -a.exp()).collect(),
                theta_bias: get(&p("mixer.theta_bias"))?,
                lambda_bias: get(&p("mixer.lambda_bias"))?,
                b_bias: get(&p("mixer.b_bias"))?,
                c_bias: get(&p("mixer.c_bias"))?,
                d_skip: get(&p("mixer.d_skip"))?,
                out_proj: get(&p("mixer.out_proj.weight"))?,
                norm2: get(&p("norm2.weight"))?,
                gate: get(&p("mlp.gate.weight"))?,
                up: get(&p("mlp.up.weight"))?,
                down: get(&p("mlp.down.weight"))?,
            });
        }
        let infer = Infer {
            embed: get("embed.weight")?,
            norm: get("norm.weight")?,
            head: get("head.weight")?,
            blocks,
            threads: 1,
            cfg,
        };
        if infer.blocks[0].in_proj.len() != infer.proj_width() * infer.cfg.d_model {
            bail!("in_proj shape does not match config");
        }
        if !infer.cfg.d_state.is_multiple_of(2) || infer.cfg.d_state > MAX_STATE {
            bail!("d_state must be even and at most {MAX_STATE}");
        }
        Ok(infer)
    }

    /// 行列積を rayon で `threads` 本に分ける。0 なら 1 本。
    pub fn with_threads(mut self, threads: usize) -> Infer {
        self.threads = threads.max(1);
        self
    }

    pub fn config(&self) -> &ModelConfig {
        &self.cfg
    }

    fn inner(&self) -> usize {
        self.cfg.n_heads * self.cfg.head_dim
    }

    pub fn proj_width(&self) -> usize {
        let h = self.cfg.n_heads;
        let n = self.cfg.d_state;
        2 * self.inner() + 2 * n + h + h * n / 2 + h
    }

    /// 1 層分の状態の長さ: head ごとに h と v_prev が P × N ずつ、累積角が N / 2。
    fn layer_state_len(&self) -> usize {
        self.cfg.n_heads * self.head_state_len()
    }

    /// 1 head 分の状態の長さ。層の状態は head ごとにこの長さで並ぶ。
    fn head_state_len(&self) -> usize {
        let (p, n) = (self.cfg.head_dim, self.cfg.d_state);
        2 * p * n + n / 2
    }

    pub fn state_len(&self) -> usize {
        self.cfg.n_layers * self.layer_state_len()
    }

    pub fn init_state(&self) -> Vec<f32> {
        vec![0.0; self.state_len()]
    }

    /// `dst` (rows × out) = `x` (rows × in) · `w` (out × in)^T
    fn linear(&self, x: &[f32], rows: usize, w: &[f32], dst: &mut [f32]) {
        let k = x.len() / rows.max(1);
        let n = w.len() / k;
        debug_assert_eq!(dst.len(), rows * n);
        if rows == 0 {
            return;
        }
        matmul_t(x, rows, k, w, n, dst, self.threads);
    }

    /// 連鎖をまとめて読む。連鎖同士は独立でなければならない。
    pub fn read(&self, chains: &[Chain]) -> Read {
        let d = self.cfg.d_model;
        let inner = self.inner();
        let w = self.proj_width();
        let m = self.cfg.mlp_dim;
        let v = self.cfg.vocab_size;
        let rows: usize = chains.iter().map(|c| c.ids.len()).sum();
        let mut states: Vec<Vec<f32>> = chains.iter().map(|c| c.state.to_vec()).collect();

        let mut x = vec![0.0f32; rows * d];
        let mut r = 0;
        for c in chains {
            for &id in c.ids {
                let id = id as usize;
                x[r * d..(r + 1) * d].copy_from_slice(&self.embed[id * d..(id + 1) * d]);
                r += 1;
            }
        }
        let mut proj_all = vec![0.0f32; rows * self.cfg.n_layers * w];
        let mut xn = vec![0.0f32; rows * d];
        let mut proj = vec![0.0f32; rows * w];
        let mut y = vec![0.0f32; rows * inner];
        let mut out = vec![0.0f32; rows * d];
        let mut g = vec![0.0f32; rows * m];
        let mut u = vec![0.0f32; rows * m];
        let lsl = self.layer_state_len();
        let hl = self.head_state_len();
        let (h, p) = (self.cfg.n_heads, self.cfg.head_dim);
        // head ごとの mixer の出力 (heads × rows × head_dim)。head を並列に回すために
        // 行ではなく head を外側にし、後で (rows × inner) に並べ直す。
        let mut y_heads = vec![0.0f32; h * rows * p];
        for (l, blk) in self.blocks.iter().enumerate() {
            rms_norm_rows(&x, &blk.norm1, &mut xn, d);
            self.linear(&xn, rows, &blk.in_proj, &mut proj);
            proj_all
                .chunks_exact_mut(self.cfg.n_layers * w)
                .zip(proj.chunks_exact(w))
                .for_each(|(all, row)| all[l * w..(l + 1) * w].copy_from_slice(row));
            // 各連鎖のこの層の状態を head ごとに分けて、head を並列に進める。
            let mut per_head: Vec<Vec<&mut [f32]>> =
                (0..h).map(|_| Vec::with_capacity(chains.len())).collect();
            for state in states.iter_mut() {
                for (hd, sl) in state[l * lsl..(l + 1) * lsl]
                    .chunks_exact_mut(hl)
                    .enumerate()
                {
                    per_head[hd].push(sl);
                }
            }
            let lens: Vec<usize> = chains.iter().map(|c| c.ids.len()).collect();
            let run = |hd: usize, sts: Vec<&mut [f32]>, yh: &mut [f32]| {
                let mut r = 0;
                for (st, &len) in sts.into_iter().zip(&lens) {
                    for _ in 0..len {
                        self.scan_head(
                            blk,
                            hd,
                            st,
                            &proj[r * w..(r + 1) * w],
                            Some(&mut yh[r * p..(r + 1) * p]),
                        );
                        r += 1;
                    }
                }
            };
            if self.threads > 1 {
                use rayon::prelude::*;
                per_head
                    .into_par_iter()
                    .zip(y_heads.par_chunks_exact_mut(rows * p))
                    .enumerate()
                    .for_each(|(hd, (sts, yh))| run(hd, sts, yh));
            } else {
                for (hd, (sts, yh)) in per_head
                    .into_iter()
                    .zip(y_heads.chunks_exact_mut(rows * p))
                    .enumerate()
                {
                    run(hd, sts, yh);
                }
            }
            for r in 0..rows {
                let z = &proj[r * w..r * w + inner];
                for hd in 0..h {
                    let src = &y_heads[(hd * rows + r) * p..(hd * rows + r + 1) * p];
                    let dst = &mut y[r * inner + hd * p..r * inner + (hd + 1) * p];
                    simd::mul_silu(dst, src, &z[hd * p..(hd + 1) * p]);
                }
            }
            self.linear(&y, rows, &blk.out_proj, &mut out);
            for (a, b) in x.iter_mut().zip(&out) {
                *a += b;
            }
            rms_norm_rows(&x, &blk.norm2, &mut xn, d);
            self.linear(&xn, rows, &blk.gate, &mut g);
            self.linear(&xn, rows, &blk.up, &mut u);
            simd::silu_mul(&mut g, &u);
            self.linear(&g, rows, &blk.down, &mut out);
            for (a, b) in x.iter_mut().zip(&out) {
                *a += b;
            }
        }
        rms_norm_rows(&x, &self.norm, &mut xn, d);
        let mut logits = vec![0.0f32; rows * v];
        self.linear(&xn, rows, &self.head, &mut logits);
        let lse: Vec<f32> = if self.threads > 1 {
            use rayon::prelude::*;
            logits.par_chunks_exact(v).map(simd::log_sum_exp).collect()
        } else {
            logits.chunks_exact(v).map(simd::log_sum_exp).collect()
        };
        Read {
            proj: proj_all,
            hidden: xn,
            lse,
            states,
        }
    }

    /// 読み終えた位置の係数 `proj`（位置 × n_layers × proj_width）を使って、
    /// `state` を出力を作らずに進める。
    pub fn rescan(&self, state: &mut [f32], proj: &[f32]) {
        let w = self.proj_width();
        let lsl = self.layer_state_len();
        let hl = self.head_state_len();
        for pos in proj.chunks_exact(self.cfg.n_layers * w) {
            for (l, blk) in self.blocks.iter().enumerate() {
                for (hd, st) in state[l * lsl..(l + 1) * lsl]
                    .chunks_exact_mut(hl)
                    .enumerate()
                {
                    self.scan_head(blk, hd, st, &pos[l * w..(l + 1) * w], None);
                }
            }
        }
    }

    /// `hidden` に対する語 `id` の対数確率。`lse` はその位置の log-sum-exp。
    pub fn log_prob(&self, hidden: &[f32], lse: f32, id: u32) -> f32 {
        let d = self.cfg.d_model;
        let id = id as usize;
        dot(&self.head[id * d..(id + 1) * d], hidden) - lse
    }

    /// 1 つの head の漸化式を 1 位置進める。`state` はその head の分
    /// （h、v_prev、累積角の順）。`y` を渡すと mixer の出力のうち head の分
    /// （head_dim、z を掛ける前）を書く。
    fn scan_head(
        &self,
        blk: &Block,
        hd: usize,
        state: &mut [f32],
        proj: &[f32],
        y: Option<&mut [f32]>,
    ) {
        let h = self.cfg.n_heads;
        let p = self.cfg.head_dim;
        let n = self.cfg.d_state;
        let inner = h * p;
        let (_z, rest) = proj.split_at(inner);
        let (x, rest) = rest.split_at(inner);
        let (b, rest) = rest.split_at(n);
        let (c, rest) = rest.split_at(n);
        let (dt, rest) = rest.split_at(h);
        let (theta, lam) = rest.split_at(h * n / 2);
        let (hs, rest) = state.split_at_mut(p * n);
        let (vp, phi) = rest.split_at_mut(p * n);

        let mut b_rot = [0.0f32; MAX_STATE];
        let mut c_rot = [0.0f32; MAX_STATE];
        let delta = softplus(dt[hd] + blk.dt_bias[hd]);
        let alpha = (delta * blk.a[hd]).exp();
        let lambda = sigmoid(lam[hd] + blk.lambda_bias[hd]);
        let gamma = lambda * delta;
        let beta = (1.0 - lambda) * delta * alpha;
        let mut dtheta = [0.0f32; MAX_STATE / 2];
        for (d, (t, tb)) in dtheta.iter_mut().zip(
            theta[hd * n / 2..(hd + 1) * n / 2]
                .iter()
                .zip(&blk.theta_bias[hd * n / 2..]),
        ) {
            *d = (t + tb) * delta;
        }
        let mut sin = [0.0f32; MAX_STATE / 2];
        let mut cos = [0.0f32; MAX_STATE / 2];
        simd::advance_angles(phi, &dtheta[..n / 2], &mut sin[..n / 2], &mut cos[..n / 2]);
        let bias = hd * n..(hd + 1) * n;
        simd::rotate_pairs(
            b,
            rms_scale(b),
            &blk.b_bias[bias.clone()],
            &sin[..n / 2],
            &cos[..n / 2],
            &mut b_rot[..n],
        );
        simd::rotate_pairs(
            c,
            rms_scale(c),
            &blk.c_bias[bias],
            &sin[..n / 2],
            &cos[..n / 2],
            &mut c_rot[..n],
        );
        let xh = &x[hd * p..(hd + 1) * p];
        let mut y = y;
        for pi in 0..p {
            let xv = xh[pi];
            let hrow = &mut hs[pi * n..(pi + 1) * n];
            let vrow = &mut vp[pi * n..(pi + 1) * n];
            let acc = step_row(hrow, vrow, &b_rot[..n], &c_rot[..n], xv, alpha, beta, gamma);
            if let Some(y) = y.as_deref_mut() {
                y[pi] = acc + xv * blk.d_skip[hd];
            }
        }
    }
}

/// d_state の上限。回した B、C をスタックに置くため。
const MAX_STATE: usize = 256;

/// 状態の 1 行（d_state 個）を 1 位置進め、C との内積を返す。
/// 4 個ずつの組で書いて自動ベクトル化に乗せる。d_state は偶数だが 4 の倍数とは
/// 限らないので、余りは素朴に回す。
#[allow(clippy::too_many_arguments)]
fn step_row(
    h: &mut [f32],
    vp: &mut [f32],
    b: &[f32],
    c: &[f32],
    x: f32,
    alpha: f32,
    beta: f32,
    gamma: f32,
) -> f32 {
    let (h4, hr) = h.as_chunks_mut::<4>();
    let (v4, vr) = vp.as_chunks_mut::<4>();
    let (b4, br) = b.as_chunks::<4>();
    let (c4, cr) = c.as_chunks::<4>();
    let mut total = step_row4(h4, v4, b4, c4, x, alpha, beta, gamma);
    for i in 0..hr.len().min(vr.len()).min(br.len()).min(cr.len()) {
        let v = x * br[i];
        let hn = alpha * hr[i] + beta * vr[i] + gamma * v;
        hr[i] = hn;
        vr[i] = v;
        total += hn * cr[i];
    }
    total
}

#[cfg(target_arch = "aarch64")]
#[allow(clippy::too_many_arguments)]
fn step_row4(
    h: &mut [[f32; 4]],
    vp: &mut [[f32; 4]],
    b: &[[f32; 4]],
    c: &[[f32; 4]],
    x: f32,
    alpha: f32,
    beta: f32,
    gamma: f32,
) -> f32 {
    use std::arch::aarch64::*;
    let len = h.len().min(vp.len()).min(b.len()).min(c.len());
    // NEON は aarch64 の基本命令なので実行時の判定は要らない。
    // 添字は len 未満に収めているので読み書きは各配列の中に収まる。
    unsafe {
        let (xv, av, bv, gv) = (
            vdupq_n_f32(x),
            vdupq_n_f32(alpha),
            vdupq_n_f32(beta),
            vdupq_n_f32(gamma),
        );
        let mut acc = vdupq_n_f32(0.0);
        for i in 0..len {
            let v = vmulq_f32(xv, vld1q_f32(b[i].as_ptr()));
            let hn = vfmaq_f32(
                vfmaq_f32(
                    vmulq_f32(av, vld1q_f32(h[i].as_ptr())),
                    bv,
                    vld1q_f32(vp[i].as_ptr()),
                ),
                gv,
                v,
            );
            vst1q_f32(h[i].as_mut_ptr(), hn);
            vst1q_f32(vp[i].as_mut_ptr(), v);
            acc = vfmaq_f32(acc, hn, vld1q_f32(c[i].as_ptr()));
        }
        vaddvq_f32(acc)
    }
}

#[cfg(not(target_arch = "aarch64"))]
#[allow(clippy::too_many_arguments)]
fn step_row4(
    h: &mut [[f32; 4]],
    vp: &mut [[f32; 4]],
    b: &[[f32; 4]],
    c: &[[f32; 4]],
    x: f32,
    alpha: f32,
    beta: f32,
    gamma: f32,
) -> f32 {
    let len = h.len().min(vp.len()).min(b.len()).min(c.len());
    let mut acc = [0.0f32; 4];
    for i in 0..len {
        for l in 0..4 {
            let v = x * b[i][l];
            let hn = alpha * h[i][l] + beta * vp[i][l] + gamma * v;
            h[i][l] = hn;
            vp[i][l] = v;
            acc[l] += hn * c[i][l];
        }
    }
    acc[0] + acc[1] + acc[2] + acc[3]
}

/// `dst` (m × n) = `x` (m × k) · `w` (n × k)^T。出力の列（重みの行）をスレッドで分け、
/// 各スレッドは自分の列の塊に全行を掛けて `dst` の自分の列に直接書く。
#[doc(hidden)]
pub fn matmul_t(
    x: &[f32],
    m: usize,
    k: usize,
    w: &[f32],
    n: usize,
    dst: &mut [f32],
    threads: usize,
) {
    assert!(dst.len() >= m * n && w.len() >= n * k && x.len() >= m * k);
    let out = Out {
        ptr: dst.as_mut_ptr(),
        stride: n,
    };
    // 塊は TILE の倍数にして、塊の境目で端数の組を作らない。
    let chunk = n.div_ceil(threads).div_ceil(TILE) * TILE;
    if threads <= 1 || chunk >= n {
        matmul_t_chunk(x, m, k, w, n, out, 0);
        return;
    }
    use rayon::prelude::*;
    w.par_chunks(chunk * k).enumerate().for_each(|(i, wp)| {
        matmul_t_chunk(x, m, k, wp, wp.len() / k, out, i * chunk);
    });
}

/// 行列積の出力先。行の間隔が `stride` の行優先で、スレッドごとに互いに
/// 重ならない列の範囲だけに書くので、同じ出力先を複数のスレッドが持てる。
#[derive(Clone, Copy)]
struct Out {
    ptr: *mut f32,
    stride: usize,
}

// 書く範囲は matmul_t が列で分けており、スレッド間で重ならない。
unsafe impl Send for Out {}
unsafe impl Sync for Out {}

/// 1 つの組で扱う行数と列数。4 × 4 の累算器 16 本と入力・重み各 4 本でレジスタに収まる。
const TILE: usize = 4;

/// `matmul_t` の 1 スレッド分。`w` の `n` 行を `out` の列 `c0..c0 + n` に書く。
/// 行と列を TILE ずつの組に分けて内積を取る。列の組を外側にし、重みの各行は
/// 1 度だけ読む。内側で回る入力の全行は L1 に収まる大きさで、重みの組
/// （4 行 × k）は L1 に留まったまま入力の行の組を順に掛ける。
#[cfg(target_arch = "aarch64")]
fn matmul_t_chunk(x: &[f32], m: usize, k: usize, w: &[f32], n: usize, out: Out, c0: usize) {
    for cb in (0..n).step_by(TILE) {
        let cols = (n - cb).min(TILE);
        for r0 in (0..m).step_by(TILE) {
            let rows = (m - r0).min(TILE);
            let mut acc = [[0.0f32; TILE]; TILE];
            match (rows, cols) {
                (4, 4) => tile::<4, 4>(x, r0, k, w, cb, &mut acc),
                (4, 3) => tile::<4, 3>(x, r0, k, w, cb, &mut acc),
                (4, 2) => tile::<4, 2>(x, r0, k, w, cb, &mut acc),
                (4, 1) => tile::<4, 1>(x, r0, k, w, cb, &mut acc),
                (3, 4) => tile::<3, 4>(x, r0, k, w, cb, &mut acc),
                (3, 3) => tile::<3, 3>(x, r0, k, w, cb, &mut acc),
                (3, 2) => tile::<3, 2>(x, r0, k, w, cb, &mut acc),
                (3, 1) => tile::<3, 1>(x, r0, k, w, cb, &mut acc),
                (2, 4) => tile::<2, 4>(x, r0, k, w, cb, &mut acc),
                (2, 3) => tile::<2, 3>(x, r0, k, w, cb, &mut acc),
                (2, 2) => tile::<2, 2>(x, r0, k, w, cb, &mut acc),
                (2, 1) => tile::<2, 1>(x, r0, k, w, cb, &mut acc),
                (1, 4) => tile::<1, 4>(x, r0, k, w, cb, &mut acc),
                (1, 3) => tile::<1, 3>(x, r0, k, w, cb, &mut acc),
                (1, 2) => tile::<1, 2>(x, r0, k, w, cb, &mut acc),
                (1, 1) => tile::<1, 1>(x, r0, k, w, cb, &mut acc),
                _ => unreachable!("tile size"),
            }
            for (r, row) in acc.iter().enumerate().take(rows) {
                // 書く先は matmul_t が確かめた dst の中で、列 c0 + cb.. は
                // このスレッドの担当。
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        row.as_ptr(),
                        out.ptr.add((r0 + r) * out.stride + c0 + cb),
                        cols,
                    );
                }
            }
        }
    }
}

/// NEON の無い環境では gemm crate（candle の CPU 行列積と同じもの）に任せる。
#[cfg(not(target_arch = "aarch64"))]
fn matmul_t_chunk(x: &[f32], m: usize, k: usize, w: &[f32], n: usize, out: Out, c0: usize) {
    unsafe {
        gemm::gemm(
            m,
            n,
            k,
            out.ptr.add(c0),
            1,
            out.stride as isize,
            false,
            x.as_ptr(),
            1,
            k as isize,
            w.as_ptr(),
            k as isize,
            1,
            0.0,
            1.0,
            false,
            false,
            false,
            gemm::Parallelism::None,
        )
    }
}

/// `x` の行 r0.. R 本と `w` の行 c0.. C 本の内積を `out[r][c]` に書く。
#[cfg(target_arch = "aarch64")]
fn tile<const R: usize, const C: usize>(
    x: &[f32],
    r0: usize,
    k: usize,
    w: &[f32],
    c0: usize,
    out: &mut [[f32; TILE]; TILE],
) {
    use std::arch::aarch64::*;
    let xs: [&[f32]; R] = std::array::from_fn(|r| &x[(r0 + r) * k..(r0 + r + 1) * k]);
    let ws: [&[f32]; C] = std::array::from_fn(|c| &w[(c0 + c) * k..(c0 + c + 1) * k]);
    let k4 = k / 4;
    // NEON は aarch64 の基本命令。添字は k4 * 4 <= k に収めている。
    unsafe {
        let mut acc = [[vdupq_n_f32(0.0); C]; R];
        for i in 0..k4 {
            let wv: [float32x4_t; C] =
                std::array::from_fn(|c| vld1q_f32(ws[c].as_ptr().add(i * 4)));
            for r in 0..R {
                let xv = vld1q_f32(xs[r].as_ptr().add(i * 4));
                for c in 0..C {
                    acc[r][c] = vfmaq_f32(acc[r][c], xv, wv[c]);
                }
            }
        }
        for r in 0..R {
            for c in 0..C {
                let tail: f32 = xs[r][k4 * 4..]
                    .iter()
                    .zip(&ws[c][k4 * 4..])
                    .map(|(a, b)| a * b)
                    .sum();
                out[r][c] = vaddvq_f32(acc[r][c]) + tail;
            }
        }
    }
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    let (a4, ar) = a.as_chunks::<4>();
    let (b4, br) = b.as_chunks::<4>();
    let mut acc = [0.0f32; 4];
    for (x, y) in a4.iter().zip(b4) {
        for l in 0..4 {
            acc[l] += x[l] * y[l];
        }
    }
    acc[0] + acc[1] + acc[2] + acc[3] + ar.iter().zip(br).map(|(x, y)| x * y).sum::<f32>()
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

/// log(1 + exp(x))。candle 側と同じく max(x, 0) + log(1 + exp(-|x|))。
fn softplus(x: f32) -> f32 {
    x.max(0.0) + (1.0 + (-x.abs()).exp()).ln()
}

/// RMS を 1 にする倍率（candle 側の `rms_normalize` と同じ式）。
fn rms_scale(x: &[f32]) -> f32 {
    let ms = x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32;
    1.0 / (ms + 1e-6).sqrt()
}

/// 行ごとに RMS を 1 にして重みを掛ける。
fn rms_norm_rows(x: &[f32], weight: &[f32], dst: &mut [f32], d: usize) {
    for (row, out) in x.chunks_exact(d).zip(dst.chunks_exact_mut(d)) {
        let ms = row.iter().map(|v| v * v).sum::<f32>() / d as f32;
        let s = 1.0 / (ms + 1e-6).sqrt();
        for ((o, v), w) in out.iter_mut().zip(row).zip(weight) {
            *o = v * s * w;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::random_weights;

    fn config() -> ModelConfig {
        ModelConfig {
            vocab_size: 10,
            d_model: 16,
            n_layers: 2,
            n_heads: 2,
            head_dim: 8,
            d_state: 4,
            mlp_dim: 32,
        }
    }

    fn infer() -> Infer {
        Infer::new(config(), &random_weights(&config(), 1)).unwrap()
    }

    fn log_probs(infer: &Infer, read: &Read, pos: usize) -> Vec<f32> {
        let d = infer.cfg.d_model;
        (0..infer.cfg.vocab_size as u32)
            .map(|id| infer.log_prob(&read.hidden[pos * d..(pos + 1) * d], read.lse[pos], id))
            .collect()
    }

    fn assert_close(a: &[f32], b: &[f32], tol: f32) {
        let diff = a
            .iter()
            .zip(b)
            .map(|(x, y)| (x - y).abs())
            .fold(0.0, f32::max);
        assert!(diff < tol, "diff={diff}\n{a:?}\n{b:?}");
    }

    #[test]
    fn 各位置の対数確率は正規化されている() {
        let infer = infer();
        let ids = [0u32, 3, 4, 5, 6, 1];
        let read = infer.read(&[Chain {
            state: &infer.init_state(),
            ids: &ids,
        }]);
        for pos in 0..ids.len() {
            let total: f32 = log_probs(&infer, &read, pos).iter().map(|p| p.exp()).sum();
            assert!((total - 1.0).abs() < 1e-4, "pos={pos} total={total}");
        }
    }

    #[test]
    fn 親の状態から続けた連鎖も一度に読んだのと同じ() {
        let infer = infer();
        let head = [0u32, 3, 4];
        let first = infer.read(&[Chain {
            state: &infer.init_state(),
            ids: &head,
        }]);
        let tails = [[5u32, 6, 1], [7, 8, 2]];
        let read = infer.read(&[
            Chain {
                state: &first.states[0],
                ids: &tails[0],
            },
            Chain {
                state: &first.states[0],
                ids: &tails[1],
            },
        ]);
        for (c, tail) in tails.iter().enumerate() {
            let whole: Vec<u32> = head.iter().chain(tail).copied().collect();
            let expected = infer.read(&[Chain {
                state: &infer.init_state(),
                ids: &whole,
            }]);
            for pos in 0..tail.len() {
                assert_close(
                    &log_probs(&infer, &read, c * tail.len() + pos),
                    &log_probs(&infer, &expected, head.len() + pos),
                    1e-4,
                );
            }
        }
    }

    #[test]
    fn 係数から作り直した途中の状態は読んだ状態と同じ() {
        let infer = infer();
        let ids = [0u32, 3, 4, 5, 6, 1];
        let whole = infer.read(&[Chain {
            state: &infer.init_state(),
            ids: &ids,
        }]);
        let part = infer.read(&[Chain {
            state: &infer.init_state(),
            ids: &ids[..3],
        }]);
        let stride = infer.cfg.n_layers * infer.proj_width();
        let mut state = infer.init_state();
        infer.rescan(&mut state, &whole.proj[..3 * stride]);
        assert_close(&state, &part.states[0], 1e-5);
    }
}
