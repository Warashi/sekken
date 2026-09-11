//! 採点経路のルーフライン計測。天井（FMA のピークと流し読みの帯域）と、
//! 行列積・forward の実測を同じ機械で出す。
//!
//! 使い方: `cargo run --release -p sekken-lm --example roofline -- lm.zst [threads]`

use std::time::Instant;

use sekken_lm::file::SavedModel;
use sekken_lm::infer::{Chain, Infer, matmul_t};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("lm.zst");
    let threads: usize = args
        .next()
        .map(|s| s.parse().unwrap())
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get()));
    let saved =
        SavedModel::load(std::io::BufReader::new(std::fs::File::open(&path).unwrap())).unwrap();
    let cfg = saved.config.clone();
    println!("config: {cfg:?} threads={threads}");
    let total: usize = saved.weights.iter().map(|(_, _, v)| v.len()).sum();
    println!("params={} bytes={:.1}MB", total, total as f64 * 4.0 / 1e6);
    for (name, shape, v) in &saved.weights {
        if v.len() >= 1 << 16 {
            println!("  {name} {shape:?} {:.2}MB", v.len() as f64 * 4.0 / 1e6);
        }
    }

    peak_fma(1);
    peak_fma(threads);
    for &mb in &[16usize, 64, 256] {
        bandwidth(mb, 1);
        bandwidth(mb, threads);
    }

    let infer = Infer::new(cfg.clone(), &saved.weights)
        .unwrap()
        .with_threads(threads);
    let rows_list = [1usize, 2, 3, 4, 8, 16, 21, 24, 32, 64];
    let shapes: Vec<(&str, usize, usize)> = {
        let d = cfg.d_model;
        let w = infer.proj_width();
        let inner = cfg.n_heads * cfg.head_dim;
        vec![
            ("in_proj", w, d),
            ("out_proj", d, inner),
            ("gate", cfg.mlp_dim, d),
            ("down", d, cfg.mlp_dim),
            ("head", cfg.vocab_size, d),
        ]
    };
    println!("\n== matmul_t (n × k) per rows: ms, GFLOPS, weight GB/s");
    for &(name, n, k) in &shapes {
        let w: Vec<f32> = (0..n * k).map(|i| (i % 7) as f32 * 0.1).collect();
        for &m in &rows_list {
            let x: Vec<f32> = (0..m * k).map(|i| (i % 5) as f32 * 0.1).collect();
            let mut dst = vec![0.0f32; m * n];
            let t = bench(|| matmul_t(&x, m, k, &w, n, &mut dst, threads));
            let flop = 2.0 * m as f64 * n as f64 * k as f64;
            println!(
                "{name:8} n={n:5} k={k:4} rows={m:3} {:.4}ms {:6.1}GFLOPS {:5.1}GB/s",
                t * 1e3,
                flop / t / 1e9,
                (n * k * 4) as f64 / t / 1e9
            );
        }
    }

    println!("\n== read(): total ms and per-row; weight bytes / total as GB/s");
    let state = infer.init_state();
    for &m in &rows_list {
        let ids: Vec<u32> = (0..m as u32).map(|i| i % cfg.vocab_size as u32).collect();
        let t = bench(|| {
            infer.read(&[Chain {
                state: &state,
                ids: &ids,
            }]);
        });
        let flop = 2.0 * m as f64 * total as f64;
        println!(
            "rows={m:3} {:.4}ms {:.4}ms/row {:6.1}GFLOPS(param) {:5.1}GB/s(weights once)",
            t * 1e3,
            t * 1e3 / m as f64,
            flop / t / 1e9,
            (total * 4) as f64 / t / 1e9
        );
    }
    let ids: Vec<u32> = (0..21u32).collect();
    let read = infer.read(&[Chain {
        state: &state,
        ids: &ids,
    }]);
    let stride = cfg.n_layers * infer.proj_width();
    let mut st = infer.init_state();
    let t = bench(|| infer.rescan(&mut st, &read.proj[..21 * stride]));
    println!(
        "rescan 21 positions: {:.4}ms ({:.4}ms/pos)",
        t * 1e3,
        t * 1e3 / 21.0
    );
}

fn bench(mut f: impl FnMut()) -> f64 {
    f();
    let mut best = f64::INFINITY;
    for _ in 0..5 {
        let n = 5;
        let t = Instant::now();
        for _ in 0..n {
            f();
        }
        best = best.min(t.elapsed().as_secs_f64() / n as f64);
    }
    best
}

/// レジスタだけで FMA を回し、演算の天井を測る。
#[cfg(target_arch = "aarch64")]
fn peak_fma(threads: usize) {
    use std::arch::aarch64::*;
    let iters = 50_000_000u64;
    let run = || unsafe {
        let mut acc = [vdupq_n_f32(0.0); 16];
        let a = vdupq_n_f32(1.000001);
        let b = vdupq_n_f32(0.999999);
        for _ in 0..iters {
            for v in acc.iter_mut() {
                *v = vfmaq_f32(*v, a, b);
            }
        }
        acc.iter().map(|v| vaddvq_f32(*v)).sum::<f32>()
    };
    let t = Instant::now();
    let sink: f32 = std::thread::scope(|s| {
        let hs: Vec<_> = (0..threads).map(|_| s.spawn(run)).collect();
        hs.into_iter().map(|h| h.join().unwrap()).sum()
    });
    let el = t.elapsed().as_secs_f64();
    let flop = iters as f64 * 16.0 * 4.0 * 2.0 * threads as f64;
    println!(
        "peak fma threads={threads}: {:.1} GFLOPS ({:.2} GFLOPS/thread, sink={sink:e})",
        flop / el / 1e9,
        flop / el / 1e9 / threads as f64
    );
}

#[cfg(not(target_arch = "aarch64"))]
fn peak_fma(_threads: usize) {
    println!("peak fma: NEON の無い環境では測らない");
}

/// `mb` MB の配列を繰り返し流し読みし、帯域を測る。
#[cfg(target_arch = "aarch64")]
fn bandwidth(mb: usize, threads: usize) {
    use std::arch::aarch64::*;
    let len = mb * 1_000_000 / 4;
    let buf: Vec<f32> = (0..len).map(|i| (i % 13) as f32).collect();
    let passes = (2000 / mb).max(4);
    let chunk = len / threads;
    let t = Instant::now();
    let sink: f32 = std::thread::scope(|s| {
        let hs: Vec<_> = buf
            .chunks(chunk)
            .map(|c| {
                s.spawn(move || unsafe {
                    let mut acc = [vdupq_n_f32(0.0); 4];
                    for _ in 0..passes {
                        let (c4, _) = c.as_chunks::<16>();
                        for blk in c4 {
                            for (j, a) in acc.iter_mut().enumerate() {
                                *a = vaddq_f32(*a, vld1q_f32(blk.as_ptr().add(j * 4)));
                            }
                        }
                    }
                    acc.iter().map(|v| vaddvq_f32(*v)).sum::<f32>()
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).sum()
    });
    let el = t.elapsed().as_secs_f64();
    println!(
        "bandwidth {mb}MB threads={threads}: {:.1} GB/s (sink={sink:e})",
        (len * 4 * passes) as f64 / el / 1e9
    );
}

#[cfg(not(target_arch = "aarch64"))]
fn bandwidth(_mb: usize, _threads: usize) {
    println!("bandwidth: NEON の無い環境では測らない");
}
