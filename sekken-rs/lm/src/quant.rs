//! int8 の行列積。重みは出力の行ごと、入力は行ごとに対称に量子化し、int32 で
//! 内積を取ってから両方の倍率を掛けて戻す。
//!
//! 1 回の読みで 100 行ほどをまとめて行列積に通すと、f32 の行列積は演算の天井の
//! 半分近くまで来ていて、重みを流し読む帯域より積和の数で律速する。aarch64 の
//! dot product 命令（SDOT）は 1 命令で int8 の積和を f32 の FMA の 4 倍こなすので、
//! 同じ時間で大きなモデルを読めるようにするために使う。漸化式と正規化は f32 のまま。
//!
//! SDOT の intrinsic（`vdotq_s32`）は安定版の Rust でまだ使えないので inline asm で書き、
//! 実行時に命令があるかを確かめる。命令が無い環境（x86 を含む）では int32 に広げた
//! 内積をコンパイラのベクトル化に任せる。

/// 内積の長さを揃える単位。SDOT は 16 個の int8 を 1 度に読む。
const LANE: usize = 16;

/// 行ごとに int8 にした重み（n × k）。k は `LANE` の倍数まで 0 で埋める。
pub struct QMatrix {
    n: usize,
    k: usize,
    /// 埋めた後の k。
    kp: usize,
    data: Vec<i8>,
    /// 行ごとの倍率。元の値 ≈ data × scale。
    scale: Vec<f32>,
}

impl QMatrix {
    /// `w`（n × k、行優先）を行ごとに量子化する。
    pub fn new(w: &[f32], n: usize, k: usize) -> QMatrix {
        assert_eq!(w.len(), n * k);
        let kp = k.div_ceil(LANE) * LANE;
        let mut data = vec![0i8; n * kp];
        let mut scale = vec![0.0f32; n];
        for (r, row) in w.chunks_exact(k).enumerate() {
            scale[r] = quantize(row, &mut data[r * kp..r * kp + k]);
        }
        QMatrix {
            n,
            k,
            kp,
            data,
            scale,
        }
    }

    pub fn rows(&self) -> usize {
        self.n
    }

    /// 行 `r` と `x`（長さ k）の内積。`x` も量子化して、行列積と同じ値にする。
    pub fn dot_row(&self, r: usize, x: &[f32]) -> f32 {
        debug_assert_eq!(x.len(), self.k);
        let mut q = vec![0i8; self.kp];
        let s = quantize(x, &mut q[..self.k]);
        dot_i8(&q, &self.data[r * self.kp..(r + 1) * self.kp]) as f32 * s * self.scale[r]
    }
}

/// `x` を対称に int8 にして `dst` に書き、倍率を返す。全部 0 なら倍率は 0。
fn quantize(x: &[f32], dst: &mut [i8]) -> f32 {
    let max = x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    if max == 0.0 {
        dst.fill(0);
        return 0.0;
    }
    let s = max / 127.0;
    let inv = 1.0 / s;
    for (d, v) in dst.iter_mut().zip(x) {
        *d = (v * inv).round().clamp(-127.0, 127.0) as i8;
    }
    s
}

/// `dst` (m × n) = `x` (m × k) · `w`ᵀ。出力の列（重みの行）をスレッドで分ける。
pub fn matmul_q(x: &[f32], m: usize, w: &QMatrix, dst: &mut [f32], threads: usize) {
    let (n, k, kp) = (w.n, w.k, w.kp);
    assert!(x.len() >= m * k && dst.len() >= m * n);
    let mut xq = vec![0i8; m * kp];
    let mut xs = vec![0.0f32; m];
    for r in 0..m {
        xs[r] = quantize(&x[r * k..(r + 1) * k], &mut xq[r * kp..r * kp + k]);
    }
    let out = Out {
        ptr: dst.as_mut_ptr(),
        stride: n,
    };
    let chunk = n.div_ceil(threads.max(1)).div_ceil(TILE) * TILE;
    let run = |c0: usize| {
        let cols = chunk.min(n - c0);
        chunk_q(&xq, &xs, m, w, c0, cols, out);
    };
    if threads <= 1 || chunk >= n {
        run(0);
        return;
    }
    use rayon::prelude::*;
    (0..n).into_par_iter().step_by(chunk).for_each(run);
}

/// 行列積の出力先。スレッドごとに互いに重ならない列の範囲だけに書く。
#[derive(Clone, Copy)]
struct Out {
    ptr: *mut f32,
    stride: usize,
}

// 書く範囲は matmul_q が列で分けており、スレッド間で重ならない。
unsafe impl Send for Out {}
unsafe impl Sync for Out {}

/// 1 つの組で扱う行数と列数。
const TILE: usize = 4;

/// 重みの行 `c0..c0 + cols` を出力の同じ列に書く。
fn chunk_q(xq: &[i8], xs: &[f32], m: usize, w: &QMatrix, c0: usize, cols: usize, out: Out) {
    #[cfg(target_arch = "aarch64")]
    if std::arch::is_aarch64_feature_detected!("dotprod") {
        // 命令があることを確かめた。
        unsafe { sdot::chunk(xq, xs, m, w, c0, cols, out) };
        return;
    }
    let kp = w.kp;
    for c in c0..c0 + cols {
        let wr = &w.data[c * kp..(c + 1) * kp];
        for r in 0..m {
            let acc = dot_i8(&xq[r * kp..(r + 1) * kp], wr);
            // 書く先は matmul_q が確かめた dst の中で、列 c はこのスレッドの担当。
            unsafe {
                *out.ptr.add(r * out.stride + c) = acc as f32 * xs[r] * w.scale[c];
            }
        }
    }
}

fn dot_i8(a: &[i8], b: &[i8]) -> i32 {
    a.iter()
        .zip(b)
        .map(|(&x, &y)| i32::from(x) * i32::from(y))
        .sum()
}

#[cfg(target_arch = "aarch64")]
mod sdot {
    use super::{Out, QMatrix, TILE};
    use std::arch::aarch64::*;

    /// `super::chunk_q` の SDOT 版。行と列を 4 ずつの組にし、累算器 16 本と
    /// 入力 1 本・重み 4 本をレジスタに置いて、重みの組を 1 度だけ読む。
    #[target_feature(enable = "dotprod")]
    pub(super) unsafe fn chunk(
        xq: &[i8],
        xs: &[f32],
        m: usize,
        w: &QMatrix,
        c0: usize,
        cols: usize,
        out: Out,
    ) {
        let kp = w.kp;
        for cb in (c0..c0 + cols).step_by(TILE) {
            let nc = (c0 + cols - cb).min(TILE);
            for r0 in (0..m).step_by(TILE) {
                let nr = (m - r0).min(TILE);
                let acc = match (nr, nc) {
                    (4, 4) => unsafe { tile::<4, 4>(xq, r0, w, cb, kp) },
                    (4, 3) => unsafe { tile::<4, 3>(xq, r0, w, cb, kp) },
                    (4, 2) => unsafe { tile::<4, 2>(xq, r0, w, cb, kp) },
                    (4, 1) => unsafe { tile::<4, 1>(xq, r0, w, cb, kp) },
                    (3, 4) => unsafe { tile::<3, 4>(xq, r0, w, cb, kp) },
                    (3, 3) => unsafe { tile::<3, 3>(xq, r0, w, cb, kp) },
                    (3, 2) => unsafe { tile::<3, 2>(xq, r0, w, cb, kp) },
                    (3, 1) => unsafe { tile::<3, 1>(xq, r0, w, cb, kp) },
                    (2, 4) => unsafe { tile::<2, 4>(xq, r0, w, cb, kp) },
                    (2, 3) => unsafe { tile::<2, 3>(xq, r0, w, cb, kp) },
                    (2, 2) => unsafe { tile::<2, 2>(xq, r0, w, cb, kp) },
                    (2, 1) => unsafe { tile::<2, 1>(xq, r0, w, cb, kp) },
                    (1, 4) => unsafe { tile::<1, 4>(xq, r0, w, cb, kp) },
                    (1, 3) => unsafe { tile::<1, 3>(xq, r0, w, cb, kp) },
                    (1, 2) => unsafe { tile::<1, 2>(xq, r0, w, cb, kp) },
                    (1, 1) => unsafe { tile::<1, 1>(xq, r0, w, cb, kp) },
                    _ => unreachable!("tile size"),
                };
                for (r, row) in acc.iter().enumerate().take(nr) {
                    for (c, &a) in row.iter().enumerate().take(nc) {
                        // 書く先は matmul_q が確かめた dst の中で、このスレッドの担当の列。
                        unsafe {
                            *out.ptr.add((r0 + r) * out.stride + cb + c) =
                                a as f32 * xs[r0 + r] * w.scale[cb + c];
                        }
                    }
                }
            }
        }
    }

    /// 入力の行 r0.. R 本と重みの行 c0.. C 本の int32 の内積。
    #[target_feature(enable = "dotprod")]
    unsafe fn tile<const R: usize, const C: usize>(
        xq: &[i8],
        r0: usize,
        w: &QMatrix,
        c0: usize,
        kp: usize,
    ) -> [[i32; TILE]; TILE] {
        let xs: [*const i8; R] = std::array::from_fn(|r| xq[(r0 + r) * kp..].as_ptr());
        let ws: [*const i8; C] = std::array::from_fn(|c| w.data[(c0 + c) * kp..].as_ptr());
        let mut out = [[0i32; TILE]; TILE];
        // 行は kp（16 の倍数）の長さで、添字は kp を超えない。
        unsafe {
            let mut acc = [[vdupq_n_s32(0); C]; R];
            for i in (0..kp).step_by(16) {
                let wv: [int8x16_t; C] = std::array::from_fn(|c| vld1q_s8(ws[c].add(i)));
                for r in 0..R {
                    let xv = vld1q_s8(xs[r].add(i));
                    for c in 0..C {
                        std::arch::asm!(
                            "sdot {acc:v}.4s, {x:v}.16b, {w:v}.16b",
                            acc = inout(vreg) acc[r][c],
                            x = in(vreg) xv,
                            w = in(vreg) wv[c],
                            options(pure, nomem, nostack),
                        );
                    }
                }
            }
            for r in 0..R {
                for c in 0..C {
                    out[r][c] = vaddvq_s32(acc[r][c]);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn random(len: usize, seed: u64) -> Vec<f32> {
        let mut s = seed;
        (0..len)
            .map(|_| {
                s = s
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                ((s >> 33) as f32 / (1u64 << 31) as f32) - 0.5
            })
            .collect()
    }

    /// f32 の素朴な行列積。
    fn reference(x: &[f32], m: usize, k: usize, w: &[f32], n: usize) -> Vec<f32> {
        let mut y = vec![0.0; m * n];
        for r in 0..m {
            for c in 0..n {
                y[r * n + c] = (0..k).map(|i| x[r * k + i] * w[c * k + i]).sum();
            }
        }
        y
    }

    #[test]
    fn 量子化した行列積は_f32_の行列積に近い() {
        // k は 16 の倍数でない長さ（mlp_dim = 683 のような）、行と列は組の端数を含める。
        for (m, k, n) in [(1, 7, 1), (5, 683, 9), (13, 256, 18), (4, 16, 4)] {
            let x = random(m * k, 1);
            let w = random(n * k, 2);
            let want = reference(&x, m, k, &w, n);
            let q = QMatrix::new(&w, n, k);
            for threads in [1, 3] {
                let mut got = vec![0.0; m * n];
                matmul_q(&x, m, &q, &mut got, threads);
                // 1 要素の誤差は倍率の半分ずつ × k 個。値の大きさ（√k × 0.08 ほど）の数 % に収まる。
                let tol = 0.02 * (k as f32).sqrt() * 0.3;
                for (g, e) in got.iter().zip(&want) {
                    assert!((g - e).abs() < tol, "{m}x{k}x{n}: {g} vs {e}");
                }
            }
        }
    }

    #[test]
    fn 行の内積は行列積と同じ値() {
        let (m, k, n) = (3, 40, 5);
        let x = random(m * k, 3);
        let w = random(n * k, 4);
        let q = QMatrix::new(&w, n, k);
        let mut y = vec![0.0; m * n];
        matmul_q(&x, m, &q, &mut y, 1);
        for r in 0..m {
            for c in 0..n {
                assert_eq!(q.dot_row(c, &x[r * k..(r + 1) * k]), y[r * n + c]);
            }
        }
    }

    #[test]
    fn 全部_0_の行は_0_になる() {
        let q = QMatrix::new(&[0.0; 32], 2, 16);
        let mut y = vec![1.0; 2];
        matmul_q(&[0.5; 16], 1, &q, &mut y, 1);
        assert_eq!(y, [0.0, 0.0]);
        let mut y = vec![1.0; 2];
        let q = QMatrix::new(&random(32, 5), 2, 16);
        matmul_q(&[0.0; 16], 1, &q, &mut y, 1);
        assert_eq!(y, [0.0, 0.0]);
    }
}
