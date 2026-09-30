//! int8 の行列積。重みは出力の行ごと、入力は行ごとに対称に量子化し、int32 で
//! 内積を取ってから両方の倍率を掛けて戻す。
//!
//! 1 回の読みで 100 行ほどをまとめて行列積に通すと、f32 の行列積は演算の天井の
//! 半分近くまで来ていて、重みを流し読む帯域より積和の数で律速する。aarch64 の
//! dot product 命令（SDOT）は 1 命令で int8 の積和を f32 の FMA の 4 倍こなすので、
//! 同じ時間で大きなモデルを読めるようにするために使う。漸化式と正規化は f32 のまま。
//!
//! SDOT の intrinsic（`vdotq_s32`）は安定版の Rust でまだ使えないので inline asm で書く。
//! x86_64 では AVX-512 VNNI と AVX-VNNI の `vpdpbusd`（符号なし 8 bit × 符号付き 8 bit）を使い、
//! 入力に 128 を足して符号なしにした分を、重みの行の和 × 128 で差し引く。どれも実行時に
//! 命令があるかを確かめる（`kernel`）。どれも無い CPU では int32 に広げた内積に落ちるが、
//! f32 の行列積より遅いので、呼ぶ側は `fast` が偽なら int8 にしない。

/// 内積の長さを揃える単位。AVX-512 の 1 本（64 個の int8）に合わせる。
/// SDOT（16 個）と AVX-VNNI（32 個）はその約数。
const LANE: usize = 64;

/// int8 の内積に使う命令。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kernel {
    /// int32 に広げた内積。f32 の行列積より遅い。
    Portable,
    /// aarch64 の dot product 命令。
    #[cfg(target_arch = "aarch64")]
    Sdot,
    /// x86_64 の AVX-512 VNNI（512 bit）。
    #[cfg(target_arch = "x86_64")]
    Avx512Vnni,
    /// x86_64 の AVX-VNNI（256 bit）。
    #[cfg(target_arch = "x86_64")]
    AvxVnni,
}

/// この CPU で使える最も速い内積の命令。
pub fn kernel() -> Kernel {
    static KERNEL: std::sync::OnceLock<Kernel> = std::sync::OnceLock::new();
    *KERNEL.get_or_init(|| available().pop().unwrap_or(Kernel::Portable))
}

/// int8 の行列積が f32 より速くなる命令があるか。
pub fn fast() -> bool {
    kernel() != Kernel::Portable
}

/// この CPU で使える命令を遅い順に。テストはこれを全部試す。
fn available() -> Vec<Kernel> {
    #[allow(unused_mut)]
    let mut ks = vec![Kernel::Portable];
    #[cfg(target_arch = "aarch64")]
    if std::arch::is_aarch64_feature_detected!("dotprod") {
        ks.push(Kernel::Sdot);
    }
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("avxvnni") {
            ks.push(Kernel::AvxVnni);
        }
        if is_x86_feature_detected!("avx512f")
            && is_x86_feature_detected!("avx512bw")
            && is_x86_feature_detected!("avx512vnni")
        {
            ks.push(Kernel::Avx512Vnni);
        }
    }
    ks
}

/// 行ごとに int8 にした重み（n × k）。k は `LANE` の倍数まで 0 で埋める。
pub struct QMatrix {
    n: usize,
    k: usize,
    /// 埋めた後の k。
    kp: usize,
    data: Vec<i8>,
    /// 行ごとの倍率。元の値 ≈ data × scale。
    scale: Vec<f32>,
    /// 行ごとの data の和。`vpdpbusd` で入力に足した 128 の分を差し引くのに使う。
    #[cfg(target_arch = "x86_64")]
    sum: Vec<i32>,
    /// 行列積に使う命令。`packed` はこの命令に合わせて作る。
    kernel: Kernel,
    /// x86_64 の VNNI 向けに並べ替えた data（`pack`）。他の命令では空。
    #[cfg(target_arch = "x86_64")]
    packed: Vec<i8>,
}

impl QMatrix {
    /// `w`（n × k、行優先）を行ごとに量子化する。行列積はこの CPU で最も速い命令で取る。
    pub fn new(w: &[f32], n: usize, k: usize) -> QMatrix {
        QMatrix::with_kernel(w, n, k, kernel())
    }

    fn with_kernel(w: &[f32], n: usize, k: usize, kernel: Kernel) -> QMatrix {
        assert_eq!(w.len(), n * k);
        let kp = k.div_ceil(LANE) * LANE;
        let mut data = vec![0i8; n * kp];
        let mut scale = vec![0.0f32; n];
        for (r, row) in w.chunks_exact(k).enumerate() {
            scale[r] = quantize(row, &mut data[r * kp..r * kp + k]);
        }
        #[cfg(target_arch = "x86_64")]
        let sum = data
            .chunks_exact(kp)
            .map(|row| row.iter().map(|&v| i32::from(v)).sum())
            .collect();
        #[cfg(target_arch = "x86_64")]
        let packed = match kernel {
            Kernel::Avx512Vnni => pack(&data, n, kp, 16),
            Kernel::AvxVnni => pack(&data, n, kp, 8),
            Kernel::Portable => Vec::new(),
        };
        QMatrix {
            n,
            k,
            kp,
            data,
            scale,
            #[cfg(target_arch = "x86_64")]
            sum,
            kernel,
            #[cfg(target_arch = "x86_64")]
            packed,
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

/// `vpdpbusd` が 1 命令で `lanes` 列 × 4 個の積和を取れるように並べ替える。列を `lanes` ずつの
/// 塊にし、塊ごとに k を 4 個ずつ進めながら、各列の 4 個を並べる（塊 × (kp / 4) × lanes × 4）。
/// 足りない列は 0 で埋める。横方向の和を取らずに済むので、内積が短くても速い。
#[cfg(target_arch = "x86_64")]
fn pack(data: &[i8], n: usize, kp: usize, lanes: usize) -> Vec<i8> {
    let blocks = n.div_ceil(lanes);
    let mut out = vec![0i8; blocks * kp * lanes];
    for c in 0..n {
        let (b, j) = (c / lanes, c % lanes);
        for g in 0..kp / 4 {
            let dst = ((b * (kp / 4) + g) * lanes + j) * 4;
            out[dst..dst + 4].copy_from_slice(&data[c * kp + g * 4..c * kp + g * 4 + 4]);
        }
    }
    out
}

/// `x` を対称に int8 にして `dst` に書き、倍率を返す。全部 0 なら倍率は 0。
/// 丸めは最近接偶数（SIMD の変換命令と同じ値にするため）。
fn quantize(x: &[f32], dst: &mut [i8]) -> f32 {
    let max = x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let (s, inv) = scale_of(max);
    for (d, v) in dst.iter_mut().zip(x) {
        *d = (v * inv).round_ties_even().clamp(-127.0, 127.0) as i8;
    }
    s
}

/// 絶対値の最大から、倍率とその逆数を決める。全部 0 なら両方 0（量子化した値も 0）。
fn scale_of(max: f32) -> (f32, f32) {
    if max == 0.0 {
        (0.0, 0.0)
    } else {
        (max / 127.0, 127.0 / max)
    }
}

/// 入力の各行（m × k）を `xq`（m × kp、埋めた分は 0）に量子化し、倍率を `xs` に書く。
/// 1 回の行列積の半分近くがこれに掛かるので、命令ごとに SIMD で書く。`offset` なら
/// 128 を足して符号なしにする（x86_64 の `vpdpbusd` 向け）。
fn quantize_rows(kernel: Kernel, x: &[f32], k: usize, kp: usize, xq: &mut [i8], xs: &mut [f32]) {
    match kernel {
        #[cfg(target_arch = "aarch64")]
        Kernel::Sdot => {
            for ((row, q), s) in x
                .chunks_exact(k)
                .zip(xq.chunks_exact_mut(kp))
                .zip(xs.iter_mut())
            {
                *s = sdot::quantize(row, &mut q[..k]);
            }
        }
        #[cfg(target_arch = "x86_64")]
        Kernel::Avx512Vnni | Kernel::AvxVnni => {
            for ((row, q), s) in x
                .chunks_exact(k)
                .zip(xq.chunks_exact_mut(kp))
                .zip(xs.iter_mut())
            {
                // VNNI の命令があれば AVX2 もある（`available` で確かめた）。
                *s = unsafe { avx2::quantize_offset(row, q) };
            }
        }
        _ => {
            for ((row, q), s) in x
                .chunks_exact(k)
                .zip(xq.chunks_exact_mut(kp))
                .zip(xs.iter_mut())
            {
                *s = quantize(row, &mut q[..k]);
            }
        }
    }
}

/// `dst` (m × n) = `x` (m × k) · `w`ᵀ。出力の列（重みの行）をスレッドで分ける。
pub fn matmul_q(x: &[f32], m: usize, w: &QMatrix, dst: &mut [f32], threads: usize) {
    let (n, k, kp, kernel) = (w.n, w.k, w.kp, w.kernel);
    assert!(x.len() >= m * k && dst.len() >= m * n);
    let mut xq = vec![0i8; m * kp];
    let mut xs = vec![0.0f32; m];
    quantize_rows(kernel, &x[..m * k], k, kp, &mut xq, &mut xs);
    // 積和が少ない行列積はスレッドに配る手間の方が大きい（64 行 × 256 × 256 で 4 本に分けると
    // 1 本より遅い）。
    let threads = if m * n * kp < PARALLEL_MIN {
        1
    } else {
        threads
    };
    let out = Out {
        ptr: dst.as_mut_ptr(),
        stride: n,
    };
    // 塊の境目を VNNI の 2 塊（32 列）の倍数にして、並べ替えた塊を分けない。
    let chunk = n.div_ceil(threads.max(1)).div_ceil(ALIGN) * ALIGN;
    let run = |c0: usize| {
        let cols = chunk.min(n - c0);
        chunk_q(kernel, &xq, &xs, m, w, c0, cols, out);
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

/// スレッドに分ける列の単位。TILE と VNNI の 2 塊（2 × 16 列）の公倍数。
const ALIGN: usize = 32;

/// スレッドに分ける積和の数の下限。
const PARALLEL_MIN: usize = 1 << 23;

/// 重みの行 `c0..c0 + cols` を出力の同じ列に書く。
#[allow(clippy::too_many_arguments)]
fn chunk_q(
    kernel: Kernel,
    xq: &[i8],
    xs: &[f32],
    m: usize,
    w: &QMatrix,
    c0: usize,
    cols: usize,
    out: Out,
) {
    // 各命令は `available` で CPU にあることを確かめた。
    match kernel {
        Kernel::Portable => {}
        #[cfg(target_arch = "aarch64")]
        Kernel::Sdot => return unsafe { sdot::chunk(xq, xs, m, w, c0, cols, out) },
        #[cfg(target_arch = "x86_64")]
        Kernel::Avx512Vnni => return unsafe { vnni512::chunk(xq, xs, m, w, c0, cols, out) },
        #[cfg(target_arch = "x86_64")]
        Kernel::AvxVnni => return unsafe { vnni256::chunk(xq, xs, m, w, c0, cols, out) },
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

    /// `super::quantize` の NEON 版。
    pub(super) fn quantize(x: &[f32], dst: &mut [i8]) -> f32 {
        let k = x.len();
        let k4 = k / 4 * 4;
        // NEON は aarch64 の基本命令。添字は k4 <= k に収めている。
        unsafe {
            let mut vmax = vdupq_n_f32(0.0);
            for i in (0..k4).step_by(4) {
                vmax = vmaxq_f32(vmax, vabsq_f32(vld1q_f32(x.as_ptr().add(i))));
            }
            let max = x[k4..].iter().fold(vmaxvq_f32(vmax), |m, v| m.max(v.abs()));
            let (s, inv) = super::scale_of(max);
            let vinv = vdupq_n_f32(inv);
            let k8 = k / 8 * 8;
            for i in (0..k8).step_by(8) {
                let a = vcvtnq_s32_f32(vmulq_f32(vld1q_f32(x.as_ptr().add(i)), vinv));
                let b = vcvtnq_s32_f32(vmulq_f32(vld1q_f32(x.as_ptr().add(i + 4)), vinv));
                let h = vcombine_s16(vqmovn_s32(a), vqmovn_s32(b));
                vst1_s8(dst.as_mut_ptr().add(i), vqmovn_s16(h));
            }
            for i in k8..k {
                dst[i] = (x[i] * inv).round_ties_even().clamp(-127.0, 127.0) as i8;
            }
            s
        }
    }

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

#[cfg(target_arch = "x86_64")]
mod avx2 {
    use std::arch::x86_64::*;

    /// `super::quantize` の AVX2 版で、128 を足して符号なしにした値を `dst`（長さは 32 の倍数で
    /// `x` 以上、`x` を超える分は 0 の 128 を書く）に書く。
    #[target_feature(enable = "avx2")]
    pub(super) unsafe fn quantize_offset(x: &[f32], dst: &mut [i8]) -> f32 {
        let k = x.len();
        debug_assert!(dst.len() >= k && dst.len().is_multiple_of(32));
        let k8 = k / 8 * 8;
        // 添字は k8 <= k と dst.len() に収めている。
        unsafe {
            let sign = _mm256_set1_ps(-0.0);
            let mut vmax = _mm256_setzero_ps();
            for i in (0..k8).step_by(8) {
                let v = _mm256_andnot_ps(sign, _mm256_loadu_ps(x.as_ptr().add(i)));
                vmax = _mm256_max_ps(vmax, v);
            }
            let mut lanes = [0.0f32; 8];
            _mm256_storeu_ps(lanes.as_mut_ptr(), vmax);
            let max = lanes
                .iter()
                .chain(x[k8..].iter().map(|v| v.abs()).collect::<Vec<_>>().iter())
                .fold(0.0f32, |m, &v| m.max(v));
            let (s, inv) = super::scale_of(max);
            let vinv = _mm256_set1_ps(inv);
            let bias = _mm256_set1_epi8(i8::MIN);
            // 32 個ずつ: 8 個 × 4 を int32 に丸め、飽和付きで int16、int8 に詰める。
            // packs は 128 bit ごとに詰めるので、最後に 32 bit 単位で並べ直す。
            let order = _mm256_setr_epi32(0, 4, 1, 5, 2, 6, 3, 7);
            let full = k / 32 * 32;
            for i in (0..full).step_by(32) {
                let f = |j: usize| {
                    _mm256_cvtps_epi32(_mm256_mul_ps(_mm256_loadu_ps(x.as_ptr().add(i + j)), vinv))
                };
                let ab = _mm256_packs_epi32(f(0), f(8));
                let cd = _mm256_packs_epi32(f(16), f(24));
                let q = _mm256_permutevar8x32_epi32(_mm256_packs_epi16(ab, cd), order);
                _mm256_storeu_si256(dst.as_mut_ptr().add(i).cast(), _mm256_xor_si256(q, bias));
            }
            for i in full..dst.len() {
                let q = if i < k {
                    (x[i] * inv).round_ties_even().clamp(-127.0, 127.0) as i8
                } else {
                    0
                };
                dst[i] = q ^ i8::MIN;
            }
            s
        }
    }
}

/// x86_64 の `vpdpbusd` で、入力の 4 行 × 重みの 2 塊（`LANES` 列ずつ）を取る。入力の 4 個を
/// 全レーンに配り、並べ替えた重み（`pack`）と積和すると、各レーンに列ごとの内積が溜まる。
/// 512 bit と 256 bit で命令と幅だけが違うので、同じ組み方をマクロで 2 つ作る。入力は
/// 128 を足して符号なしにしてあるので、内積から 128 × 重みの行の和を引いて戻す。
#[cfg(target_arch = "x86_64")]
macro_rules! vnni_kernel {
    ($name:ident, $features:literal, $vec:ty, $lanes:literal, $zero:path, $load:path, $set1:path, $store:path, $dp:path) => {
        mod $name {
            use super::{Out, QMatrix, TILE};
            use std::arch::x86_64::*;

            const LANES: usize = $lanes;

            #[target_feature(enable = $features)]
            pub(super) unsafe fn chunk(
                xq: &[i8],
                xs: &[f32],
                m: usize,
                w: &QMatrix,
                c0: usize,
                cols: usize,
                out: Out,
            ) {
                debug_assert_eq!(c0 % LANES, 0);
                let end = c0 + cols;
                for cb in (c0..end).step_by(2 * LANES) {
                    let blocks = (end - cb).div_ceil(LANES).min(2);
                    for r0 in (0..m).step_by(TILE) {
                        let nr = (m - r0).min(TILE);
                        let acc = match (nr, blocks) {
                            (4, 2) => unsafe { tile::<4, 2>(xq, r0, w, cb) },
                            (3, 2) => unsafe { tile::<3, 2>(xq, r0, w, cb) },
                            (2, 2) => unsafe { tile::<2, 2>(xq, r0, w, cb) },
                            (1, 2) => unsafe { tile::<1, 2>(xq, r0, w, cb) },
                            (4, 1) => unsafe { tile::<4, 1>(xq, r0, w, cb) },
                            (3, 1) => unsafe { tile::<3, 1>(xq, r0, w, cb) },
                            (2, 1) => unsafe { tile::<2, 1>(xq, r0, w, cb) },
                            (1, 1) => unsafe { tile::<1, 1>(xq, r0, w, cb) },
                            _ => unreachable!("tile size"),
                        };
                        for (r, row) in acc.iter().enumerate().take(nr) {
                            for (j, &a) in row.iter().enumerate().take((end - cb).min(2 * LANES)) {
                                let c = cb + j;
                                let a = a - 128 * w.sum[c];
                                // 書く先は matmul_q が確かめた dst の中で、このスレッドの担当の列。
                                unsafe {
                                    *out.ptr.add((r0 + r) * out.stride + c) =
                                        a as f32 * xs[r0 + r] * w.scale[c];
                                }
                            }
                        }
                    }
                }
            }

            /// 入力の行 r0.. R 本（符号なし）と、列 c0 から B 塊の int32 の内積（行 × 列）。
            #[target_feature(enable = $features)]
            unsafe fn tile<const R: usize, const B: usize>(
                xq: &[i8],
                r0: usize,
                w: &QMatrix,
                c0: usize,
            ) -> [[i32; 2 * LANES]; TILE] {
                let kp = w.kp;
                let groups = kp / 4;
                let xs: [*const i8; R] = std::array::from_fn(|r| xq[(r0 + r) * kp..].as_ptr());
                let ws: [*const i8; B] = std::array::from_fn(|b| {
                    w.packed[(c0 / LANES + b) * groups * LANES * 4..].as_ptr()
                });
                let mut out = [[0i32; 2 * LANES]; TILE];
                // 入力の行は kp、重みの塊は groups × LANES × 4 の長さで、添字はそれを超えない。
                unsafe {
                    let mut acc: [[$vec; B]; R] = [[$zero(); B]; R];
                    for g in 0..groups {
                        let wv: [$vec; B] =
                            std::array::from_fn(|b| $load(ws[b].add(g * LANES * 4).cast()));
                        for r in 0..R {
                            let x4 = xs[r].add(g * 4).cast::<i32>().read_unaligned();
                            let xv = $set1(x4);
                            for b in 0..B {
                                acc[r][b] = $dp(acc[r][b], xv, wv[b]);
                            }
                        }
                    }
                    for r in 0..R {
                        for b in 0..B {
                            $store(out[r][b * LANES..].as_mut_ptr().cast(), acc[r][b]);
                        }
                    }
                }
                out
            }
        }
    };
}

#[cfg(target_arch = "x86_64")]
vnni_kernel!(
    vnni512,
    "avx512f,avx512bw,avx512vnni",
    __m512i,
    16,
    _mm512_setzero_si512,
    _mm512_loadu_si512,
    _mm512_set1_epi32,
    _mm512_storeu_si512,
    _mm512_dpbusd_epi32
);

#[cfg(target_arch = "x86_64")]
vnni_kernel!(
    vnni256,
    "avx2,avxvnni",
    __m256i,
    8,
    _mm256_setzero_si256,
    _mm256_loadu_si256,
    _mm256_set1_epi32,
    _mm256_storeu_si256,
    _mm256_dpbusd_avx_epi32
);

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
            for (kernel, threads) in available().into_iter().flat_map(|k| [(k, 1), (k, 3)]) {
                let q = QMatrix::with_kernel(&w, n, k, kernel);
                let mut got = vec![0.0; m * n];
                matmul_q(&x, m, &q, &mut got, threads);
                // 1 要素の誤差は倍率の半分ずつ × k 個。値の大きさ（√k × 0.08 ほど）の数 % に収まる。
                let tol = 0.02 * (k as f32).sqrt() * 0.3;
                for (g, e) in got.iter().zip(&want) {
                    assert!((g - e).abs() < tol, "{kernel:?} {m}x{k}x{n}: {g} vs {e}");
                }
            }
        }
    }

    #[test]
    fn どの命令でも移植版と同じ値になる() {
        let (m, k, n) = (9, 683, 11);
        let x = random(m * k, 6);
        let w = random(n * k, 7);
        let mut want = vec![0.0; m * n];
        matmul_q(
            &x,
            m,
            &QMatrix::with_kernel(&w, n, k, Kernel::Portable),
            &mut want,
            1,
        );
        for kernel in available() {
            let mut got = vec![0.0; m * n];
            matmul_q(&x, m, &QMatrix::with_kernel(&w, n, k, kernel), &mut got, 2);
            assert_eq!(got, want, "{kernel:?}");
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
