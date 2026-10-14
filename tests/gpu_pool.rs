//! GPU textures and readback buffers are reused between renders: repeated
//! renders create nothing new, idle memory stays within the cap, and a reused
//! texture never shows what it held before.

#![cfg(feature = "gpu")]

use delos::engine::geom::{Rect, Request, Size};
use delos::engine::gpu::Gpu;
use delos::engine::pool::texture_bytes;
use delos::engine::{render_to_cpu, Engine};
use delos::graph::{Graph, NodeId};
use delos::nodes::Value;

fn no_cancel() -> bool {
    false
}
fn no_status(_: Option<&str>) {}

/// Runs `f` with a GPU engine, or skips the test when there's no GPU.
fn with_gpu(f: impl FnOnce(&Engine, &Gpu)) {
    let engine = Engine::new(true);
    match engine.gpu.clone() {
        Some(g) => f(&engine, &g),
        None => eprintln!("no GPU available; skipping"),
    }
}

/// noise -> invert, blended (multiply) with a checkerboard: several kernels,
/// intermediate tiles and a readback.
fn graph() -> (Graph, NodeId) {
    let mut g = Graph::new();
    let noise = g.add_node("noise", (0.0, 0.0));
    let inv = g.add_node("invert", (0.0, 0.0));
    g.connect(noise, inv, 0).unwrap();
    let checker = g.add_node("checker", (0.0, 0.0));
    let blend = g.add_node("blend", (0.0, 0.0));
    g.connect(inv, blend, 0).unwrap();
    g.connect(checker, blend, 1).unwrap();
    (g, blend)
}

fn request(w: u32, h: u32) -> Request {
    Request::new(Size::new(w as f64, h as f64), Rect::new(0.0, 0.0, w as f64, h as f64), w, h)
}

fn render(engine: &Engine, g: &Graph, node: NodeId, req: &Request) -> Vec<[f32; 4]> {
    render_to_cpu(engine, g, node, req, &no_cancel, &no_status).unwrap().data
}

#[test]
fn repeated_renders_reuse_textures() {
    with_gpu(|engine, gpu| {
        let (g, out) = graph();
        let req = request(640, 480);
        render(engine, &g, out, &req);
        let created = gpu.pool.created();
        assert!(created > 0);
        for _ in 0..5 {
            render(engine, &g, out, &req);
        }
        assert_eq!(gpu.pool.created(), created, "repeated renders created new textures or buffers");
        assert!(gpu.pool.idle_bytes() > 0);
    });
}

#[test]
fn idle_memory_stays_within_the_cap() {
    with_gpu(|engine, gpu| {
        let (g, out) = graph();
        // Room for two idle 256×256 textures, not a whole render's worth.
        let cap = texture_bytes(256, 256) * 2;
        gpu.pool.set_cap(cap);
        for (w, h) in [(256, 256), (300, 200), (256, 256)] {
            render(engine, &g, out, &request(w, h));
            assert!(gpu.pool.idle_bytes() <= cap, "{} > {cap}", gpu.pool.idle_bytes());
        }
        // Lowering the cap releases what no longer fits.
        gpu.pool.set_cap(0);
        assert_eq!(gpu.pool.idle_bytes(), 0);
    });
}

/// Renders through textures that held other images must match renders on a
/// fresh engine.
#[test]
fn reused_textures_do_not_leak_stale_pixels() {
    let fresh = |g: &Graph, node: NodeId, req: &Request| render(&Engine::new(true), g, node, req);
    with_gpu(|engine, gpu| {
        let req = request(320, 240);
        // Fill the pool with textures holding a bright, opaque image.
        let mut bright = Graph::new();
        let solid = bright.add_node("solid", (0.0, 0.0));
        bright.set_param(solid, "color", Value::Color([1.0, 0.5, 0.25, 1.0]));
        let inv = bright.add_node("invert", (0.0, 0.0));
        bright.connect(solid, inv, 0).unwrap();
        render(engine, &bright, inv, &req);
        assert!(gpu.pool.idle_bytes() > 0);

        // Mostly transparent output: any stale texel would show.
        let mut g = Graph::new();
        let radial = g.add_node("radial_gradient", (0.0, 0.0));
        g.set_param(radial, "radius", Value::Float(0.1));
        let inv = g.add_node("invert", (0.0, 0.0));
        g.connect(radial, inv, 0).unwrap();
        let created = gpu.pool.created();
        let reused = render(engine, &g, inv, &req);
        assert_eq!(gpu.pool.created(), created, "the render didn't reuse the pooled textures");
        assert!(reused == fresh(&g, inv, &req), "a reused texture changed the result");

        let (g, out) = graph();
        assert!(render(engine, &g, out, &req) == fresh(&g, out, &req));
    });
}
