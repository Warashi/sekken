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
            let mut p = vdupq_n_f32(1.987_569_2e-4);
            p = vfmaq_f32(vdupq_n_f32(1.398_2e-3), p, r);
            p = vfmaq_f32(vdupq_n_f32(8.333_452e-3), p, r);
            p = vfmaq_f32(vdupq_n_f32(4.166_579_6e-2), p, r);
            p = vfmaq_f32(vdupq_n_f32(1.666_666_5e-1), p, r);
            p = vfmaq_f32(vdupq_n_f32(0.5), p, r);
            let r2 = vmulq_f32(r, r);
            p = vfmaq_f32(r, p, r2);
            p = vaddq_f32(p, vdupq_n_f32(1.0));
            // 2^n を指数部に置く
            let e = vshlq_n_s32::<23>(vaddq_s32(vcvtq_s32_f32(n), vdupq_n_s32(127)));
            vmulq_f32(p, vreinterpretq_f32_s32(e))
        }
    }

    /// 4 要素の silu。exp の丸めで分母が頭打ちになる大きな負の入力は、
    /// 真の値が 1e-36 より小さいので 0 にする。
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
    let (g4, gr) = g.as_chunks_mut::<4>();
    let (u4, ur) = u.as_chunks::<4>();
    // 添字は両方の配列の長さに収めている。
    unsafe {
        for (a, b) in g4.iter_mut().zip(u4) {
            let v = vmulq_f32(neon::silu4(vld1q_f32(a.as_ptr())), vld1q_f32(b.as_ptr()));
            vst1q_f32(a.as_mut_ptr(), v);
        }
    }
    for (a, b) in gr.iter_mut().zip(ur) {
        *a = silu(*a) * b;
    }
}

#[cfg(target_arch = "aarch64")]
fn mul_silu_impl(dst: &mut [f32], s: &[f32], z: &[f32]) {
    use std::arch::aarch64::*;
    let (d4, dr) = dst.as_chunks_mut::<4>();
    let (s4, sr) = s.as_chunks::<4>();
    let (z4, zr) = z.as_chunks::<4>();
    unsafe {
        for ((d, a), b) in d4.iter_mut().zip(s4).zip(z4) {
            let v = vmulq_f32(vld1q_f32(a.as_ptr()), neon::silu4(vld1q_f32(b.as_ptr())));
            vst1q_f32(d.as_mut_ptr(), v);
        }
    }
    for ((d, a), b) in dr.iter_mut().zip(sr).zip(zr) {
        *d = a * silu(*b);
    }
}

#[cfg(target_arch = "aarch64")]
fn sum_exp_shifted(row: &[f32], max: f32) -> f32 {
    use std::arch::aarch64::*;
    let (r4, rr) = row.as_chunks::<4>();
    let mut total = unsafe {
        let m = vdupq_n_f32(max);
        let mut acc = vdupq_n_f32(0.0);
        for v in r4 {
            acc = vaddq_f32(acc, neon::exp4(vsubq_f32(vld1q_f32(v.as_ptr()), m)));
        }
        vaddvq_f32(acc)
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
fn sum_exp_shifted(row: &[f32], max: f32) -> f32 {
    row.iter().map(|v| (v - max).exp()).sum()
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
