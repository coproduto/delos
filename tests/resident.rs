//! Source images kept resident on the GPU: uploads happen in the background,
//! memory stays within the budget (least recently used levels go first), and
//! levels of images nothing holds any more are released.

#![cfg(feature = "gpu")]

use delos::engine::resident::Resident;
use delos::engine::source::SourceImage;
use delos::engine::Engine;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn image(w: u32, h: u32) -> Arc<SourceImage> {
    Arc::new(SourceImage::from_rgba8("t", w, h, vec![128; (w * h * 4) as usize]))
}

fn all_resident(r: &Resident, img: &SourceImage) -> bool {
    (0..img.level_count()).all(|l| r.is_resident(img, l))
}

fn wait_for(what: &str, cond: impl Fn() -> bool) {
    let t = Instant::now();
    while !cond() {
        assert!(t.elapsed() < Duration::from_secs(20), "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Runs `f` with a GPU engine, or skips the test when there's no GPU.
fn with_gpu(f: impl FnOnce(&Engine, &Resident)) {
    let engine = Engine::new(true);
    match engine.resident() {
        Some(r) => f(&engine, r),
        None => eprintln!("no GPU available; skipping"),
    }
}

#[test]
fn every_level_is_uploaded() {
    with_gpu(|engine, r| {
        let img = image(1000, 700);
        engine.make_resident(&img);
        wait_for("upload", || all_resident(r, &img));
        assert_eq!(r.bytes(), img.bytes());
    });
}

#[test]
fn least_recently_used_levels_are_evicted_within_budget() {
    with_gpu(|engine, r| {
        let (a, b, c) = (image(512, 512), image(512, 512), image(512, 512));
        let budget = a.bytes() * 5 / 2;
        r.set_budget(budget);
        engine.make_resident(&a);
        wait_for("a", || all_resident(r, &a));
        engine.make_resident(&b);
        wait_for("b", || all_resident(r, &b));
        // Using `a` makes `b` the least recently used.
        for l in 0..a.level_count() {
            assert!(r.get(&a, l).is_some());
        }
        engine.make_resident(&c);
        wait_for("c", || all_resident(r, &c));
        assert!(all_resident(r, &a), "recently used levels were evicted");
        assert!(!all_resident(r, &b), "nothing was evicted");
        assert!(r.bytes() <= budget, "{} > {budget}", r.bytes());

        // Needing an evicted level brings it back (evicting something else).
        assert!(r.get(&b, 0).is_none());
        wait_for("b level 0 again", || r.is_resident(&b, 0));
        assert!(r.bytes() <= budget, "{} > {budget}", r.bytes());
    });
}

#[test]
fn dropped_images_release_their_levels() {
    with_gpu(|engine, r| {
        let img = image(800, 600);
        engine.make_resident(&img);
        wait_for("upload", || all_resident(r, &img));
        drop(img);
        assert_eq!(r.bytes(), 0);
    });
}

#[test]
fn levels_larger_than_the_budget_are_skipped() {
    with_gpu(|engine, r| {
        let img = image(1024, 1024);
        let (w, h, _) = img.level(0);
        r.set_budget((w * h * 4) as usize - 1);
        engine.make_resident(&img);
        wait_for("smaller levels", || (1..img.level_count()).all(|l| r.is_resident(&img, l)));
        assert!(r.get(&img, 0).is_none());
        std::thread::sleep(Duration::from_millis(100));
        assert!(!r.is_resident(&img, 0));
        assert!(r.bytes() < r.budget());
    });
}

/// Renders the image node of a graph holding `img` on `engine`.
fn render_image(engine: &Engine, img: &Arc<SourceImage>, req: &delos::engine::geom::Request) -> delos::engine::tile::CpuTile {
    use delos::nodes::{Value, IMAGE_PARAM};
    let mut g = delos::graph::Graph::new();
    let id = g.add_image(img.clone());
    let n = g.add_node("image", (0.0, 0.0));
    g.set_param(n, IMAGE_PARAM, Value::Int(id as i64));
    g.set_param(n, "width", Value::Int(img.width as i64));
    g.set_param(n, "height", Value::Int(img.height as i64));
    delos::engine::render_to_cpu(engine, &g, n, req, &|| false, &|_| {}).unwrap()
}

fn noisy(w: u32, h: u32) -> Arc<SourceImage> {
    let data = (0..w * h * 4).map(|i| (delos::engine::kernels::hash_u(i) >> 9) as u8).collect();
    Arc::new(SourceImage::from_rgba8("noisy", w, h, data))
}

/// Requests covering every level, nearest and bilinear sampling, regions
/// partly or wholly outside the image, and point sampling.
fn requests(img: &SourceImage) -> Vec<(String, delos::engine::geom::Request)> {
    use delos::engine::geom::{Rect, Request, Sampling, Size};
    let (w, h) = (img.width as f64, img.height as f64);
    let extent = Size::new(w, h);
    let mut out = vec![];
    for (name, r, gw, gh) in [
        ("fit", Rect::new(0.0, 0.0, w, h), 300, 200),
        ("fit small", Rect::new(0.0, 0.0, w, h), 37, 23),
        ("1:1", Rect::new(13.0, 7.0, 120.0, 90.0), 120, 90),
        ("zoom in", Rect::new(10.25, 20.5, 30.0, 20.0), 240, 160),
        ("partly outside", Rect::new(-0.3 * w, -0.2 * h, 1.7 * w, 1.5 * h), 400, 300),
        ("outside", Rect::new(1.5 * w, 0.0, w, h), 50, 50),
        ("offset fractional", Rect::new(0.37, 0.91, w * 0.6, h * 0.7), 211, 97),
    ] {
        out.push((name.to_string(), Request::new(extent, r, gw, gh)));
        let mut p = Request::new(extent, r, gw, gh);
        p.sampling = Sampling::Point;
        out.push((format!("{name} (point)"), p));
    }
    out
}

/// The GPU maps pixels to texels in `f32` (the CPU in `f64`), so a pixel
/// whose position falls within rounding distance of a texel boundary can
/// pick the neighbouring texel; everything else matches to rounding.
#[test]
fn gpu_sampling_matches_cpu() {
    let cpu = Engine::new(false);
    with_gpu(|engine, r| {
        for img in [noisy(257, 131), noisy(1001, 333), noisy(2048, 1536)] {
            engine.make_resident(&img);
            wait_for("upload", || all_resident(r, &img));
            for (name, req) in requests(&img) {
                let a = render_image(&cpu, &img, &req);
                let b = render_image(engine, &img, &req);
                let what = format!("{}x{} {name}", img.width, img.height);
                assert_eq!((a.w, a.h), (b.w, b.h), "{what}");
                let flipped = a.data.iter().zip(&b.data).filter(|(p, q)| (0..4).any(|i| (p[i] - q[i]).abs() > 1e-4)).count();
                assert!(flipped * 200 <= a.data.len(), "{what}: {flipped} of {} pixels differ", a.data.len());
            }
        }
    });
}

#[test]
fn rendering_uploads_missing_levels() {
    with_gpu(|engine, r| {
        let img = noisy(640, 480);
        let req = &requests(&img)[0].1;
        let level = img.level_for(req);
        // Not resident yet: rendered on the CPU, and the level gets queued.
        render_image(engine, &img, req);
        wait_for("level", || r.is_resident(&img, level));
    });
}
