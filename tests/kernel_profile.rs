//! Times every kernel on the CPU and the GPU at sizes the app renders at,
//! with the parameters their nodes default to. GPU kernel times come from
//! timestamp queries (execution on the GPU alone); "GPU + readback" is the
//! wall time to run the kernel and have its result back on the CPU.
//!
//! A benchmark rather than a check, so it only runs when asked for:
//! `cargo test --release --test kernel_profile -- --ignored --nocapture`

#![cfg(feature = "gpu")]

use delos::engine::geom::{Map2, Rect, Request, Size};
use delos::engine::kernels::{self, hash_u, Kernel};
use delos::engine::profile::{Backend, Sample};
use delos::engine::tile::{CpuTile, SampleMode, Tile};
use delos::engine::{Ctx, Engine, KernelInput};
use delos::graph::Graph;
use std::time::{Duration, Instant};

const WARMUP: usize = 2;
const REPS: usize = 7;

/// Preview-sized, an export band (16384 wide, 4 Mpx), and a large tile.
const SIZES: &[(u32, u32)] = &[(512, 512), (1920, 1080), (16384, 256), (4096, 4096)];

const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

fn no_cancel() -> bool {
    false
}
fn no_status(_: Option<&str>) {}

/// Premultiplied pixels with varied colour and alpha.
fn noisy(w: u32, h: u32, seed: u32) -> CpuTile {
    let mut t = CpuTile::new(w, h);
    for (i, px) in t.data.iter_mut().enumerate() {
        let r = |k: u32| (hash_u(i as u32 ^ hash_u(seed * 4 + k)) >> 8) as f32 / (1 << 24) as f32;
        let a = r(3);
        *px = [r(0) * a, r(1) * a, r(2) * a, a];
    }
    t
}

/// How a kernel's input is laid out relative to the output.
#[derive(Clone, Copy)]
enum Input {
    /// Same grid as the output.
    Aligned,
    /// Half the output's resolution, upscaled bilinearly (a resized image,
    /// a background-removal mask).
    Half,
    /// Mirrored about the vertical centre line (Reflect's second input).
    Mirrored,
}

/// A kernel as its node uses it: inputs and default parameters.
struct Case {
    kernel: &'static Kernel,
    inputs: &'static [Input],
    params: fn(w: f32, h: f32) -> Vec<f32>,
}

fn cat(parts: &[&[f32]]) -> Vec<f32> {
    parts.concat()
}

const CASES: &[Case] = &[
    Case { kernel: &kernels::RESAMPLE, inputs: &[Input::Half], params: |_, _| vec![] },
    Case { kernel: &kernels::INVERT, inputs: &[Input::Aligned], params: |_, _| vec![] },
    Case { kernel: &kernels::OVER, inputs: &[Input::Aligned, Input::Aligned], params: |_, _| vec![] },
    Case { kernel: &kernels::MASK, inputs: &[Input::Aligned, Input::Half], params: |_, _| vec![0.0] },
    Case { kernel: &kernels::SPLIT, inputs: &[Input::Aligned], params: |w, h| vec![w / 4.0, h / 4.0, w * 0.75, h * 0.75, 0.0] },
    Case { kernel: &kernels::REFLECT, inputs: &[Input::Aligned, Input::Mirrored], params: |w, _| vec![0.0, w / 2.0, 0.0, 0.0] },
    // Multiply, the node's default mode.
    Case { kernel: &kernels::BLEND, inputs: &[Input::Aligned, Input::Aligned], params: |_, _| vec![2.0, 1.0] },
    Case { kernel: &kernels::SOLID, inputs: &[], params: |_, _| vec![0.18, 0.45, 0.85, 1.0] },
    Case { kernel: &kernels::LINEAR_GRADIENT, inputs: &[], params: |_, _| cat(&[&BLACK, &WHITE, &[0.0, 0.5, 1.0, 0.5]]) },
    Case { kernel: &kernels::RADIAL_GRADIENT, inputs: &[], params: |_, _| cat(&[&WHITE, &[0.0; 4], &[0.5, 0.5, 0.5]]) },
    Case { kernel: &kernels::CHECKER, inputs: &[], params: |_, _| cat(&[&WHITE, &BLACK, &[0.1]]) },
    Case { kernel: &kernels::NOISE, inputs: &[], params: |_, _| cat(&[&BLACK, &WHITE, &[0.25, 6.0, 0.5, 2.0, 1.0, 0.0, 1.2]]) },
    Case { kernel: &kernels::VORONOI, inputs: &[], params: |_, _| cat(&[&BLACK, &WHITE, &[0.1, 1.0, 1.0, 0.0]]) },
    Case { kernel: &kernels::MANDELBROT, inputs: &[], params: |_, _| vec![-0.6, 0.0, 1.0, 256.0, 0.0, 0.1, 0.2, 0.0] },
];

/// Input tiles on the CPU, and the same tiles uploaded to the GPU.
fn inputs(case: &Case, w: u32, h: u32, gpu: &Engine) -> (Vec<KernelInput>, Vec<KernelInput>) {
    let g = gpu.gpu.as_ref().unwrap();
    let mut cpu = vec![];
    let mut on_gpu = vec![];
    for (n, kind) in case.inputs.iter().enumerate() {
        let (tile, map, mode) = match kind {
            Input::Aligned => (noisy(w, h, n as u32), Map2::IDENTITY, SampleMode::Nearest),
            Input::Half => (noisy(w.div_ceil(2), h.div_ceil(2), n as u32), Map2::new(0.5, 0.0, 0.5, 0.0), SampleMode::Bilinear),
            Input::Mirrored => (noisy(w, h, n as u32), Map2::new(-1.0, w as f64, 1.0, 0.0), SampleMode::Nearest),
        };
        on_gpu.push(KernelInput { tile: Tile::Gpu(g.upload(&tile)), map, mode });
        cpu.push(KernelInput { tile: Tile::Cpu(tile), map, mode });
    }
    (cpu, on_gpu)
}

fn copy(inputs: &[KernelInput]) -> Vec<KernelInput> {
    inputs
        .iter()
        .map(|i| KernelInput {
            tile: match &i.tile {
                Tile::Cpu(c) => Tile::Cpu(c.clone()),
                Tile::Gpu(g) => Tile::Gpu(g.clone()),
                Tile::Clear { w, h } => Tile::Clear { w: *w, h: *h },
            },
            map: i.map,
            mode: i.mode,
        })
        .collect()
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

/// Median recorded time of `op` on `backend`.
fn recorded(samples: &[Sample], op: &str, backend: Backend) -> Duration {
    median(samples.iter().filter(|s| s.op == op && s.backend == backend).map(|s| s.time).collect())
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

#[test]
#[ignore = "benchmark; run with --release -- --ignored --nocapture"]
fn kernels_on_cpu_and_gpu() {
    let cpu = Engine::new(false);
    let gpu = Engine::new(true);
    if gpu.gpu.is_none() {
        eprintln!("no GPU available; skipping");
        return;
    }
    gpu.warm_up();
    let graph = Graph::new();
    let cpu_ctx = Ctx::new(&cpu, &graph, &no_cancel, &no_status);
    let gpu_ctx = Ctx::new(&gpu, &graph, &no_cancel, &no_status);

    println!("\n| kernel | size | CPU ms | GPU kernel ms | GPU synced ms | GPU + readback ms | wait | copy | GPU speedup (kernel) |");
    println!("|---|---|---:|---:|---:|---:|---:|---:|---:|");
    for case in CASES {
        for &(w, h) in SIZES {
            let req = Request::new(Size::new(w as f64, h as f64), Rect::new(0.0, 0.0, w as f64, h as f64), w, h);
            let params = (case.params)(w as f32, h as f32);
            let (cin, gin) = inputs(case, w, h, &gpu);
            gpu.profile_samples();

            for rep in 0..WARMUP + REPS {
                cpu.profile.enable(rep >= WARMUP);
                let t = cpu_ctx.kernel(case.kernel, copy(&cin), &req, &params).unwrap();
                assert!(matches!(t, Tile::Cpu(_)));
            }
            let cs = cpu.profile_samples();

            // Kernel alone, waiting for the GPU: checks the timestamps.
            let mut synced = vec![];
            for rep in 0..WARMUP + REPS {
                let start = Instant::now();
                let t = gpu_ctx.kernel(case.kernel, copy(&gin), &req, &params).unwrap();
                gpu.gpu.as_ref().unwrap().finish();
                if rep >= WARMUP {
                    synced.push(start.elapsed());
                }
                drop(t);
            }

            let mut walls = vec![];
            for rep in 0..WARMUP + REPS {
                gpu.profile.enable(rep >= WARMUP);
                let start = Instant::now();
                let t = gpu_ctx.kernel(case.kernel, copy(&gin), &req, &params).unwrap();
                assert!(matches!(t, Tile::Gpu(_)), "{} didn't run on the GPU", case.kernel.name);
                gpu_ctx.to_cpu(t).unwrap();
                if rep >= WARMUP {
                    walls.push(start.elapsed());
                }
            }
            gpu.profile.enable(false);
            let gs = gpu.profile_samples();

            let c = recorded(&cs, case.kernel.name, Backend::Cpu);
            let g = recorded(&gs, case.kernel.name, Backend::Gpu);
            println!(
                "| {} | {w}×{h} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.1}× |",
                case.kernel.name,
                ms(c),
                ms(g),
                ms(median(synced)),
                ms(median(walls)),
                ms(recorded(&gs, "readback wait", Backend::Gpu)),
                ms(recorded(&gs, "readback copy", Backend::Gpu)),
                c.as_secs_f64() / g.as_secs_f64().max(1e-9)
            );
        }
    }
}
