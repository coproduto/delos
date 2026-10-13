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
