//! 採点経路の要素演算。exp を含む演算は libm を 1 要素ずつ呼ぶと
//! 1 要素 20 サイクルほどかかり、行数 24 の forward で行列積の 1/3 に
//! 相当した。NEON では 4 要素まとめて多項式で exp を求める。
//!
//! exp は Cephes の expf と同じ範囲縮小と 6 次多項式で、誤差は数 ulp。
//! NEON の無い環境は libm のままなので、環境によって最下位の桁が違いうる。

/// `g[i] = silu(g[i]) * u[i]`。
pub fn silu_mul(g: &mut [f32], u: &[f32]) {
    silu_mul_impl(g, u);
}

/// `dst[i] = s[i] * silu(z[i])`。
pub fn mul_silu(dst: &mut [f32], s: &[f32], z: &[f32]) {
    mul_silu_impl(dst, s, z);
}

/// 累積角を進めて sin と cos を求める。`phi[j] += dtheta[j]`。
/// 角は 8192 rad までを想定する（Mamba-3 の累積角は 1 文で 1000 rad ほど）。
pub fn advance_angles(phi: &mut [f32], dtheta: &[f32], sin: &mut [f32], cos: &mut [f32]) {
    advance_angles_impl(phi, dtheta, sin, cos);
}

/// 隣り合う 2 要素を角 j で回す。`v = src * scale + bias` として
/// `out[2j] = v0 cos + v1 sin`、`out[2j + 1] = v1 cos - v0 sin`。
pub fn rotate_pairs(
    src: &[f32],
    scale: f32,
    bias: &[f32],
    sin: &[f32],
    cos: &[f32],
    out: &mut [f32],
) {
    rotate_pairs_impl(src, scale, bias, sin, cos, out);
}

/// `log(sum(exp(row)))`。
pub fn log_sum_exp(row: &[f32]) -> f32 {
    let max = row.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    max + sum_exp_shifted(row, max).ln()
}

fn silu(x: f32) -> f32 {
    x / (1.0 + (-x).exp())
}

#[cfg(target_arch = "aarch64")]
mod neon {
    use std::arch::aarch64::*;

    /// 4 要素の exp。入力は exp が f32 に収まる範囲に丸める。
    ///
    /// # Safety
    /// NEON は aarch64 の基本命令なので、呼び出し側の条件は無い。
    #[inline(always)]
    pub unsafe fn exp4(x: float32x4_t) -> float32x4_t {
        unsafe {
            let x = vminq_f32(
                vmaxq_f32(x, vdupq_n_f32(-87.336_54)),
                vdupq_n_f32(88.376_26),
            );
            // n = round(x / ln2)、r = x - n ln2（ln2 を 2 つに分けて誤差を抑える）
            let n = vrndnq_f32(vmulq_f32(x, vdupq_n_f32(std::f32::consts::LOG2_E)));
            let r = vfmsq_f32(x, n, vdupq_n_f32(0.693_359_4));
            let r = vfmsq_f32(r, n, vdupq_n_f32(-2.121_944_4e-4));
            // 多項式は依存の鎖を短くするため Estrin の形で評価する
            let r2 = vmulq_f32(r, r);
            let r4 = vmulq_f32(r2, r2);
            let p01 = vfmaq_f32(vdupq_n_f32(0.5), vdupq_n_f32(1.666_666_5e-1), r);
            let p23 = vfmaq_f32(vdupq_n_f32(4.166_579_6e-2), vdupq_n_f32(8.333_452e-3), r);
            let p45 = vfmaq_f32(vdupq_n_f32(1.398_2e-3), vdupq_n_f32(1.987_569_2e-4), r);
            let p = vfmaq_f32(vfmaq_f32(p01, p23, r2), p45, r4);
            let p = vfmaq_f32(r, p, r2);
            let p = vaddq_f32(p, vdupq_n_f32(1.0));
            // 2^n を指数部に置く
            let e = vshlq_n_s32::<23>(vaddq_s32(vcvtq_s32_f32(n), vdupq_n_s32(127)));
            vmulq_f32(p, vreinterpretq_f32_s32(e))
        }
    }

    /// 独立な 4 本の exp。依存の鎖が長いので、並べて out-of-order に重ねる。
    #[inline(always)]
    pub unsafe fn exp4x4(x: [float32x4_t; 4]) -> [float32x4_t; 4] {
        unsafe { std::array::from_fn(|i| exp4(x[i])) }
    }

    #[inline(always)]
    pub unsafe fn silu4x4(x: [float32x4_t; 4]) -> [float32x4_t; 4] {
        unsafe { std::array::from_fn(|i| silu4(x[i])) }
    }

    /// 4 要素の silu。exp の丸めで分母が頭打ちになる大きな負の入力は、
    /// 真の値が 1e-36 より小さいので 0 にする。
    /// 4 要素の sin と cos。Cephes の sinf/cosf と同じ π/4 ごとの範囲縮小と多項式。
    /// 範囲縮小は 3 つに分けた π/4 で行い、|x| < 8192 で誤差は数 ulp。
    ///
    /// # Safety
    /// NEON は aarch64 の基本命令なので、呼び出し側の条件は無い。
    #[inline(always)]
    pub unsafe fn sincos4(x: float32x4_t) -> (float32x4_t, float32x4_t) {
        unsafe {
            let sign_bit = vdupq_n_u32(0x8000_0000);
            let sign_x = vandq_u32(vreinterpretq_u32_f32(x), sign_bit);
            let ax = vabsq_f32(x);
            // j = 象限番号 × 2（偶数に丸める）
            let j = vcvtq_u32_f32(vmulq_f32(ax, vdupq_n_f32(1.273_239_5)));
            let j = vandq_u32(vaddq_u32(j, vdupq_n_u32(1)), vdupq_n_u32(!1));
            let y = vcvtq_f32_u32(j);
            let r = vfmsq_f32(ax, y, vdupq_n_f32(0.785_156_25));
            let r = vfmsq_f32(r, y, vdupq_n_f32(2.418_756_5e-4));
            let r = vfmsq_f32(r, y, vdupq_n_f32(3.774_895e-8));
            let z = vmulq_f32(r, r);
            // sin の多項式: ((-1.95e-4 z + 8.33e-3) z - 1.67e-1) z r + r
            let mut ps = vdupq_n_f32(-1.951_529_6e-4);
            ps = vfmaq_f32(vdupq_n_f32(8.332_161e-3), ps, z);
            ps = vfmaq_f32(vdupq_n_f32(-1.666_665_5e-1), ps, z);
            ps = vfmaq_f32(r, vmulq_f32(ps, z), r);
            // cos の多項式: ((2.44e-5 z - 1.39e-3) z + 4.17e-2) z z - 0.5 z + 1
            let mut pc = vdupq_n_f32(2.443_315_7e-5);
            pc = vfmaq_f32(vdupq_n_f32(-1.388_731_6e-3), pc, z);
            pc = vfmaq_f32(vdupq_n_f32(4.166_664_6e-2), pc, z);
            pc = vmulq_f32(pc, vmulq_f32(z, z));
            pc = vfmsq_f32(pc, vdupq_n_f32(0.5), z);
            pc = vaddq_f32(pc, vdupq_n_f32(1.0));
            // 象限が奇数なら sin と cos を入れ替える
            let swap = vtstq_u32(j, vdupq_n_u32(2));
            let sin = vbslq_f32(swap, pc, ps);
            let cos = vbslq_f32(swap, ps, pc);
            // 符号: sin は象限 2・3 と入力の符号、cos は象限 1・2
            let sin_sign = veorq_u32(sign_x, vshlq_n_u32::<29>(vandq_u32(j, vdupq_n_u32(4))));
            let cos_sign =
                vshlq_n_u32::<29>(vandq_u32(vaddq_u32(j, vdupq_n_u32(2)), vdupq_n_u32(4)));
            (
                vreinterpretq_f32_u32(veorq_u32(vreinterpretq_u32_f32(sin), sin_sign)),
                vreinterpretq_f32_u32(veorq_u32(vreinterpretq_u32_f32(cos), cos_sign)),
            )
        }
    }

    #[inline(always)]
    pub unsafe fn silu4(x: float32x4_t) -> float32x4_t {
        unsafe {
            let e = exp4(vnegq_f32(x));
            let y = vdivq_f32(x, vaddq_f32(vdupq_n_f32(1.0), e));
            vbslq_f32(vcltq_f32(x, vdupq_n_f32(-87.0)), vdupq_n_f32(0.0), y)
        }
    }
}

#[cfg(target_arch = "aarch64")]
fn silu_mul_impl(g: &mut [f32], u: &[f32]) {
    use std::arch::aarch64::*;
    // 1 要素ずつの依存の鎖が長いので、4 本を並べて out-of-order に重ねる。
    let (g16, gr) = g.as_chunks_mut::<16>();
    let (u16, ur) = u.as_chunks::<16>();
    unsafe {
        for (a, b) in g16.iter_mut().zip(u16) {
            let x: [float32x4_t; 4] = std::array::from_fn(|i| vld1q_f32(a.as_ptr().add(i * 4)));
            for (i, y) in neon::silu4x4(x).into_iter().enumerate() {
                let v = vmulq_f32(y, vld1q_f32(b.as_ptr().add(i * 4)));
                vst1q_f32(a.as_mut_ptr().add(i * 4), v);
            }
        }
    }
    for (a, b) in gr.iter_mut().zip(ur) {
        *a = silu(*a) * b;
    }
}

#[cfg(target_arch = "aarch64")]
fn mul_silu_impl(dst: &mut [f32], s: &[f32], z: &[f32]) {
    use std::arch::aarch64::*;
    let (d16, dr) = dst.as_chunks_mut::<16>();
    let (s16, sr) = s.as_chunks::<16>();
    let (z16, zr) = z.as_chunks::<16>();
    unsafe {
        for ((d, a), b) in d16.iter_mut().zip(s16).zip(z16) {
            let x: [float32x4_t; 4] = std::array::from_fn(|i| vld1q_f32(b.as_ptr().add(i * 4)));
            for (i, y) in neon::silu4x4(x).into_iter().enumerate() {
                let v = vmulq_f32(vld1q_f32(a.as_ptr().add(i * 4)), y);
                vst1q_f32(d.as_mut_ptr().add(i * 4), v);
            }
        }
    }
    for ((d, a), b) in dr.iter_mut().zip(sr).zip(zr) {
        *d = a * silu(*b);
    }
}

#[cfg(target_arch = "aarch64")]
fn advance_angles_impl(phi: &mut [f32], dtheta: &[f32], sin: &mut [f32], cos: &mut [f32]) {
    use std::arch::aarch64::*;
    let n = phi.len().min(dtheta.len()).min(sin.len()).min(cos.len());
    let n4 = n / 4;
    unsafe {
        for i in 0..n4 {
            let p = vaddq_f32(
                vld1q_f32(phi.as_ptr().add(i * 4)),
                vld1q_f32(dtheta.as_ptr().add(i * 4)),
            );
            vst1q_f32(phi.as_mut_ptr().add(i * 4), p);
            let (s, c) = neon::sincos4(p);
            vst1q_f32(sin.as_mut_ptr().add(i * 4), s);
            vst1q_f32(cos.as_mut_ptr().add(i * 4), c);
        }
    }
    for j in n4 * 4..n {
        phi[j] += dtheta[j];
        (sin[j], cos[j]) = phi[j].sin_cos();
    }
}

#[cfg(target_arch = "aarch64")]
fn rotate_pairs_impl(
    src: &[f32],
    scale: f32,
    bias: &[f32],
    sin: &[f32],
    cos: &[f32],
    out: &mut [f32],
) {
    use std::arch::aarch64::*;
    let n = sin
        .len()
        .min(cos.len())
        .min(src.len() / 2)
        .min(bias.len() / 2)
        .min(out.len() / 2);
    let n4 = n / 4;
    unsafe {
        let sc = vdupq_n_f32(scale);
        for i in 0..n4 {
            // 偶数番と奇数番を分けて読む
            let v = vld2q_f32(src.as_ptr().add(i * 8));
            let b = vld2q_f32(bias.as_ptr().add(i * 8));
            let v0 = vaddq_f32(vmulq_f32(v.0, sc), b.0);
            let v1 = vaddq_f32(vmulq_f32(v.1, sc), b.1);
            let s = vld1q_f32(sin.as_ptr().add(i * 4));
            let c = vld1q_f32(cos.as_ptr().add(i * 4));
            let o0 = vaddq_f32(vmulq_f32(v0, c), vmulq_f32(v1, s));
            let o1 = vsubq_f32(vmulq_f32(v1, c), vmulq_f32(v0, s));
            vst2q_f32(out.as_mut_ptr().add(i * 8), float32x4x2_t(o0, o1));
        }
    }
    for j in n4 * 4..n {
        rotate_pair(src, scale, bias, sin[j], cos[j], out, j);
    }
}

#[cfg(target_arch = "aarch64")]
fn sum_exp_shifted(row: &[f32], max: f32) -> f32 {
    use std::arch::aarch64::*;
    let (r16, rr) = row.as_chunks::<16>();
    let mut total = unsafe {
        let m = vdupq_n_f32(max);
        let mut acc = [vdupq_n_f32(0.0); 4];
        for v in r16 {
            let x: [float32x4_t; 4] =
                std::array::from_fn(|i| vsubq_f32(vld1q_f32(v.as_ptr().add(i * 4)), m));
            for (a, e) in acc.iter_mut().zip(neon::exp4x4(x)) {
                *a = vaddq_f32(*a, e);
            }
        }
        vaddvq_f32(vaddq_f32(
            vaddq_f32(acc[0], acc[1]),
            vaddq_f32(acc[2], acc[3]),
        ))
    };
    for v in rr {
        total += (v - max).exp();
    }
    total
}

#[cfg(not(target_arch = "aarch64"))]
fn silu_mul_impl(g: &mut [f32], u: &[f32]) {
    for (a, b) in g.iter_mut().zip(u) {
        *a = silu(*a) * b;
    }
}

#[cfg(not(target_arch = "aarch64"))]
fn mul_silu_impl(dst: &mut [f32], s: &[f32], z: &[f32]) {
    for ((d, a), b) in dst.iter_mut().zip(s).zip(z) {
        *d = a * silu(*b);
    }
}

#[cfg(not(target_arch = "aarch64"))]
fn advance_angles_impl(phi: &mut [f32], dtheta: &[f32], sin: &mut [f32], cos: &mut [f32]) {
    for (((p, d), s), c) in phi.iter_mut().zip(dtheta).zip(sin).zip(cos) {
        *p += d;
        (*s, *c) = p.sin_cos();
    }
}

#[cfg(not(target_arch = "aarch64"))]
fn rotate_pairs_impl(
    src: &[f32],
    scale: f32,
    bias: &[f32],
    sin: &[f32],
    cos: &[f32],
    out: &mut [f32],
) {
    let n = sin
        .len()
        .min(cos.len())
        .min(src.len() / 2)
        .min(bias.len() / 2)
        .min(out.len() / 2);
    for j in 0..n {
        rotate_pair(src, scale, bias, sin[j], cos[j], out, j);
    }
}

#[cfg(not(target_arch = "aarch64"))]
fn sum_exp_shifted(row: &[f32], max: f32) -> f32 {
    row.iter().map(|v| (v - max).exp()).sum()
}

/// `rotate_pairs` の 1 対分。
#[allow(clippy::too_many_arguments)]
fn rotate_pair(
    src: &[f32],
    scale: f32,
    bias: &[f32],
    sin: f32,
    cos: f32,
    out: &mut [f32],
    j: usize,
) {
    let v0 = src[2 * j] * scale + bias[2 * j];
    let v1 = src[2 * j + 1] * scale + bias[2 * j + 1];
    out[2 * j] = v0 * cos + v1 * sin;
    out[2 * j + 1] = v1 * cos - v0 * sin;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples() -> Vec<f32> {
        (0..1001).map(|i| (i as f32 - 500.0) * 0.2).collect()
    }

    fn assert_rel(a: f32, b: f32, rel: f32) {
        assert!((a - b).abs() <= rel * b.abs() + 1e-30, "{a} vs {b}");
    }

    #[test]
    fn silu_の積は_libm_と数_ulp_で一致する() {
        let x = samples();
        let u: Vec<f32> = x.iter().map(|v| v * 0.5 + 1.0).collect();
        let mut g = x.clone();
        silu_mul(&mut g, &u);
        for ((got, x), u) in g.iter().zip(&x).zip(&u) {
            assert_rel(*got, silu(*x) * u, 1e-6);
        }
        let mut d = vec![0.0; x.len()];
        mul_silu(&mut d, &u, &x);
        for ((got, x), u) in d.iter().zip(&x).zip(&u) {
            assert_rel(*got, u * silu(*x), 1e-6);
        }
    }

    #[test]
    fn 極端な入力でも_nan_にならない() {
        let x = [-1e30f32, -200.0, 0.0, 200.0, 1e30];
        let u = [1.0f32; 5];
        let mut g = x;
        silu_mul(&mut g, &u);
        assert!(g.iter().all(|v| v.is_finite()));
        assert_eq!(g[0], 0.0);
        assert_eq!(g[4], 1e30);
    }

    #[test]
    fn 累積角の_sin_cos_は_libm_と一致する() {
        // 1 文で累積する角（1000 rad ほど）を超える範囲まで、細かい刻みで見る。
        let dtheta: Vec<f32> = (0..1024).map(|i| 0.1 + (i % 7) as f32 * 0.37).collect();
        let mut phi = vec![0.0f32; 1024];
        let mut expected = vec![0.0f32; 1024];
        let (mut sin, mut cos) = (vec![0.0f32; 1024], vec![0.0f32; 1024]);
        for step in 0..8 {
            advance_angles(&mut phi, &dtheta, &mut sin, &mut cos);
            for j in 0..1024 {
                expected[j] += dtheta[j];
                assert_eq!(phi[j], expected[j]);
                let (s, c) = expected[j].sin_cos();
                assert!(
                    (sin[j] - s).abs() < 2e-6,
                    "step {step} j {j}: {} vs {s}",
                    sin[j]
                );
                assert!(
                    (cos[j] - c).abs() < 2e-6,
                    "step {step} j {j}: {} vs {c}",
                    cos[j]
                );
            }
        }
        // 負の角
        let mut phi = vec![-1.0f32, -2.0, -3.0, -1000.5, -0.1];
        let dtheta = vec![0.0f32; 5];
        let (mut sin, mut cos) = (vec![0.0f32; 5], vec![0.0f32; 5]);
        advance_angles(&mut phi, &dtheta, &mut sin, &mut cos);
        for j in 0..5 {
            let (s, c) = phi[j].sin_cos();
            assert!(
                (sin[j] - s).abs() < 2e-6 && (cos[j] - c).abs() < 2e-6,
                "{}",
                phi[j]
            );
        }
    }

    #[test]
    fn 対の回転は素朴な計算と一致する() {
        let src: Vec<f32> = (0..22).map(|i| i as f32 * 0.3 - 3.0).collect();
        let bias: Vec<f32> = (0..22).map(|i| (i as f32).sin()).collect();
        let sin: Vec<f32> = (0..11).map(|j| (j as f32 * 0.7).sin()).collect();
        let cos: Vec<f32> = (0..11).map(|j| (j as f32 * 0.7).cos()).collect();
        let mut out = vec![0.0f32; 22];
        rotate_pairs(&src, 0.5, &bias, &sin, &cos, &mut out);
        let mut want = vec![0.0f32; 22];
        for j in 0..11 {
            rotate_pair(&src, 0.5, &bias, sin[j], cos[j], &mut want, j);
        }
        assert_eq!(out, want);
    }

    #[test]
    fn log_sum_exp_は_f64_の計算と一致する() {
        let row = samples();
        let max = row.iter().copied().fold(f32::NEG_INFINITY, f32::max) as f64;
        let want = max
            + row
                .iter()
                .map(|v| (f64::from(*v) - max).exp())
                .sum::<f64>()
                .ln();
        assert_rel(log_sum_exp(&row), want as f32, 1e-6);
        // 4 の倍数でない長さも扱う
        assert_rel(
            log_sum_exp(&row[..7]),
            {
                let m = row[..7].iter().copied().fold(f32::NEG_INFINITY, f32::max) as f64;
                (m + row[..7]
                    .iter()
                    .map(|v| (f64::from(*v) - m).exp())
                    .sum::<f64>()
                    .ln()) as f32
            },
            1e-6,
        );
    }
}
