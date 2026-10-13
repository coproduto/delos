//! 16K (16384×16384) workloads. Previews must stay interactive because they
//! only ever render viewport-sized grids; exports stream in bands.
//!
//! Timing assertions are only enforced in optimised builds:
//! `cargo test --release --test large_images -- --nocapture`

use delos::engine::geom::{Rect, Request, Size};
use delos::engine::source::SourceImage;
use delos::engine::{render_to_cpu, Engine};
use delos::formats::{BandSource, GraphBands};
use delos::graph::Graph;
use delos::nodes::{Value, IMAGE_PARAM};
use std::sync::Arc;
use std::time::{Duration, Instant};

const N: u32 = 16384;

/// These tests time themselves and each holds 16K images on the CPU and the
/// GPU, so running them concurrently skews timings and memory: they take turns.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

fn no_cancel() -> bool {
    false
}
fn no_status(_: Option<&str>) {}

fn budget(ms: u64) -> Duration {
    if cfg!(debug_assertions) { Duration::from_millis(ms * 50) } else { Duration::from_millis(ms) }
}

fn timed<T>(label: &str, f: impl FnOnce() -> T) -> (T, Duration) {
    let t = Instant::now();
    let r = f();
    let d = t.elapsed();
    println!("{label}: {d:?}");
    (r, d)
}

fn build_graph(engine: &Engine) -> (Graph, u64) {
    let mut g = Graph::new();
    let (img, _) = timed("16K import (premultiply + pyramid)", || {
        let mut data = vec![0u8; N as usize * N as usize * 4];
        use rayon::prelude::*;
        data.par_chunks_mut(N as usize * 4).enumerate().for_each(|(y, row)| {
            for x in 0..N as usize {
                row[x * 4..x * 4 + 4].copy_from_slice(&[(x >> 6) as u8, (y >> 6) as u8, ((x ^ y) & 255) as u8, 255]);
            }
        });
        Arc::new(SourceImage::from_rgba8("16k", N, N, data))
    });
    // Like an import in the app. The upload runs in the background there;
    // waiting for it here makes the timings below steady-state ones.
    engine.make_resident(&img);
    #[cfg(feature = "gpu")]
    if let Some(r) = engine.resident() {
        timed("16K upload to the GPU", || {
            let t = Instant::now();
            while !(0..img.level_count()).all(|l| r.is_resident(&img, l)) && t.elapsed() < Duration::from_secs(30) {
                std::thread::sleep(Duration::from_millis(1));
            }
        });
    }
    let iid = g.add_image(img);
    let image = g.add_node("image", (0.0, 0.0));
    g.set_param(image, IMAGE_PARAM, Value::Int(iid as i64));
    g.set_param(image, "width", Value::Int(N as i64));
    g.set_param(image, "height", Value::Int(N as i64));
    let inv = g.add_node("invert", (0.0, 0.0));
    g.connect(image, inv, 0).unwrap();
    let refl = g.add_node("reflect", (0.0, 0.0));
    g.connect(inv, refl, 0).unwrap();
    let noise = g.add_node("noise", (0.0, 0.0));
    let blend = g.add_node("blend", (0.0, 0.0));
    g.connect(refl, blend, 0).unwrap();
    g.connect(noise, blend, 1).unwrap();
    g.set_param(blend, "mode", Value::Choice("overlay".into()));
    let grad = g.add_node("radial_gradient", (0.0, 0.0));
    g.set_param(grad, "color_b", Value::Color([0.0, 0.0, 0.0, 0.0]));
    let stack = g.add_node("stack", (0.0, 0.0));
    g.connect(grad, stack, 0).unwrap();
    g.connect(blend, stack, 1).unwrap();
    g.connect(stack, g.output, 0).unwrap();
    (g, stack)
}

#[test]
fn preview_16k_is_interactive() {
    let _serial = serial();
    for gpu in [false, true] {
        let engine = Engine::new(gpu);
        if gpu && !engine.has_gpu() {
            continue;
        }
        timed("warm-up", || engine.warm_up());
        println!("--- {}", engine.backend_name());
        let (g, out) = build_graph(&engine);
        assert_eq!(g.size_of(out), Some(Size::new(N as f64, N as f64)));
        let extent = g.extent_of(out);

        // Whole image fitted into a 1920×1080 viewport (1080×1080 grid).
        let fit = Request::new(extent, extent.rect(), 1080, 1080);
        let (t, d) = timed("fit preview 1080²", || render_to_cpu(&engine, &g, out, &fit, &no_cancel, &no_status).unwrap());
        assert_eq!((t.w, t.h), (1080, 1080));
        assert!(d < budget(400), "fit preview too slow: {d:?}");

        // 1:1 zoom into the middle.
        let zoom = Request::new(extent, Rect::new(8000.0, 8000.0, 1920.0, 1080.0), 1920, 1080);
        let (_, d) = timed("1:1 preview 1920×1080", || render_to_cpu(&engine, &g, out, &zoom, &no_cancel, &no_status).unwrap());
        assert!(d < budget(400), "1:1 preview too slow: {d:?}");

        // Repeated parameter tweaks (what dragging a slider does).
        let (_, d) = timed("10 re-renders", || {
            for _ in 0..10 {
                render_to_cpu(&engine, &g, out, &fit, &no_cancel, &no_status).unwrap();
            }
        });
        assert!(d < budget(3000), "re-render too slow: {d:?}");
    }
}

/// A full PNG export to disk, the way the desktop app does it (buffered file
/// writer), including compression.
#[test]
#[cfg_attr(debug_assertions, ignore = "takes minutes unoptimised; run with --release")]
fn export_16k_png() {
    let _serial = serial();
    let codec = delos::formats::codec_by_name("PNG").unwrap();
    for gpu in [false, true] {
        let engine = Engine::new(gpu);
        if gpu && !engine.has_gpu() {
            continue;
        }
        println!("--- {}", engine.backend_name());
        let (g, out) = build_graph(&engine);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("export.png");
        let progress = |_p: f32| {};
        let (_, d) = timed("16K PNG export", || {
            let mut w = std::io::BufWriter::with_capacity(1 << 20, std::fs::File::create(&path).unwrap());
            delos::formats::export(&engine, &g, out.into(), codec, &Default::default(), &mut w, &no_cancel, &progress, &no_status).unwrap();
            w.into_inner().unwrap().sync_all().unwrap();
        });
        println!("16K PNG size: {} MiB", std::fs::metadata(&path).unwrap().len() >> 20);
        let info = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&path).unwrap())).read_info().unwrap();
        assert_eq!((info.info().width, info.info().height), (N, N));
        assert!(d < budget(60_000), "16K PNG export too slow: {d:?}");
    }
}

#[test]
fn export_16k_streams_in_bands() {
    let _serial = serial();
    let engine = Engine::new(true);
    let mut g = Graph::new();
    let noise = g.add_node("noise", (0.0, 0.0));
    g.connect(noise, g.output, 0).unwrap();
    g.set_param(g.output, "canvas_w", Value::Int(N as i64));
    g.set_param(g.output, "canvas_h", Value::Int(N as i64));
    let progress = |_p: f32| {};
    let mut bands = GraphBands::new(&engine, &g, g.output.into(), &no_cancel, &progress, &no_status);
    assert_eq!((bands.width(), bands.height()), (N, N));
    let (rows, d) = timed(&format!("16K generator band render ({})", engine.backend_name()), || {
        let mut rows = 0u64;
        let mut peak = 0usize;
        while let Some((r, band)) = bands.next_band().unwrap() {
            rows += r as u64;
            peak = peak.max(band.len());
        }
        // A band never exceeds ~4 Mpx regardless of the image size.
        assert!(peak <= (4 << 20) * 4 + N as usize * 4);
        rows
    });
    assert_eq!(rows, N as u64);
    assert!(d < budget(30_000), "16K export render too slow: {d:?}");
}
