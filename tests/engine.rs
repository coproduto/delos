use delos::engine::geom::{Rect, Request, Size};
use delos::engine::source::SourceImage;
use delos::engine::tile::CpuTile;
use delos::engine::{render_to_cpu, Engine};
use delos::graph::{Graph, NodeId};
use delos::nodes::{Value, IMAGE_PARAM};
use std::sync::Arc;

fn no_cancel() -> bool {
    false
}
fn no_status(_: Option<&str>) {}

fn render(engine: &Engine, g: &Graph, id: NodeId, req: &Request) -> CpuTile {
    render_to_cpu(engine, g, id, req, &no_cancel, &no_status).expect("render")
}

fn gradient_image(w: u32, h: u32) -> Vec<u8> {
    let mut v = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            v.extend_from_slice(&[(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8, 255]);
        }
    }
    v
}

fn add_image(g: &mut Graph, w: u32, h: u32) -> NodeId {
    let img = Arc::new(SourceImage::from_rgba8("test", w, h, gradient_image(w, h)));
    let iid = g.add_image(img);
    let n = g.add_node("image", (0.0, 0.0));
    g.set_param(n, IMAGE_PARAM, Value::Int(iid as i64));
    g.set_param(n, "width", Value::Int(w as i64));
    g.set_param(n, "height", Value::Int(h as i64));
    n
}

fn max_diff(a: &CpuTile, b: &CpuTile) -> f32 {
    assert_eq!((a.w, a.h), (b.w, b.h));
    a.data.iter().zip(&b.data).flat_map(|(p, q)| (0..4).map(move |i| (p[i] - q[i]).abs())).fold(0.0, f32::max)
}

fn mean_diff(a: &CpuTile, b: &CpuTile) -> f32 {
    let s: f32 = a.data.iter().zip(&b.data).map(|(p, q)| (0..4).map(|i| (p[i] - q[i]).abs()).sum::<f32>()).sum();
    s / (a.data.len() * 4) as f32
}

#[test]
fn native_render_reproduces_source() {
    let engine = Engine::new(false);
    let mut g = Graph::new();
    let n = add_image(&mut g, 300, 200);
    let req = Request::native(Size::new(300.0, 200.0));
    let t = render(&engine, &g, n, &req);
    assert_eq!((t.w, t.h), (300, 200));
    let p = t.data[10 * 300 + 20];
    assert!((p[0] - 20.0 / 255.0).abs() < 1e-6 && (p[1] - 10.0 / 255.0).abs() < 1e-6);
}

#[test]
fn invert_and_stack() {
    let engine = Engine::new(false);
    let mut g = Graph::new();
    let a = add_image(&mut g, 64, 64);
    let inv = g.add_node("invert", (0.0, 0.0));
    g.connect(a, inv, 0).unwrap();
    let t = render(&engine, &g, inv, &Request::native(Size::new(64.0, 64.0)));
    let p = t.data[5 * 64 + 7];
    assert!((p[0] - (1.0 - 7.0 / 255.0)).abs() < 1e-5);

    // A disabled front layer lets the back layer through.
    let dis = g.add_node("disable", (0.0, 0.0));
    g.connect(inv, dis, 0).unwrap();
    let st = g.add_node("stack", (0.0, 0.0));
    g.connect(dis, st, 0).unwrap();
    g.connect(a, st, 1).unwrap();
    let t2 = render(&engine, &g, st, &Request::native(Size::new(64.0, 64.0)));
    assert!((t2.data[5 * 64 + 7][0] - 7.0 / 255.0).abs() < 1e-5);
}

#[test]
fn stack_chain_editing() {
    let mut g = Graph::new();
    let a = add_image(&mut g, 8, 8);
    let b = add_image(&mut g, 8, 8);
    let c = add_image(&mut g, 8, 8);
    let s = g.add_node("stack", (0.0, 0.0));
    g.connect(s, g.output, 0).unwrap();
    g.set_stack_layers(s, vec![Some(a), Some(b), Some(c)]);
    let chain = g.stack_chain(s).unwrap();
    assert_eq!(chain.layers, vec![Some(a), Some(b), Some(c)]);
    assert_eq!(chain.stacks.len(), 2);
    assert!(g.is_interior_stack(chain.stacks[1]));
    assert_eq!(g.chain_head(chain.stacks[1]), s);
    g.set_stack_layers(s, vec![Some(c), Some(a)]);
    let chain = g.stack_chain(s).unwrap();
    assert_eq!(chain.layers, vec![Some(c), Some(a)]);
    assert_eq!(chain.stacks, vec![s]);
    assert_eq!(g.node(g.output).unwrap().inputs[0], Some(s));
}

#[test]
fn cycles_are_rejected() {
    let mut g = Graph::new();
    let a = g.add_node("invert", (0.0, 0.0));
    let b = g.add_node("invert", (0.0, 0.0));
    g.connect(a, b, 0).unwrap();
    assert!(g.connect(b, a, 0).is_err());
    assert!(g.connect(a, a, 0).is_err());
}

#[test]
fn size_inference_and_conflicts() {
    let mut g = Graph::new();
    let a = add_image(&mut g, 100, 50);
    let b = add_image(&mut g, 40, 40);
    let noise = g.add_node("noise", (0.0, 0.0));
    let bl = g.add_node("blend", (0.0, 0.0));
    g.connect(a, bl, 0).unwrap();
    g.connect(noise, bl, 1).unwrap();
    assert_eq!(g.size_of(noise), None);
    assert_eq!(g.size_of(bl), Some(Size::new(100.0, 50.0)));
    assert!(g.size_conflict(bl).is_none());
    g.connect(b, bl, 1).unwrap();
    let c = g.size_conflict(bl).expect("conflict");
    g.insert_resize(c.node, c.b.0, c.a.1);
    assert!(g.size_conflict(bl).is_none());
}

#[test]
fn resize_nearest_is_exact_and_fourier_is_close() {
    let engine = Engine::new(false);
    let mut g = Graph::new();
    let a = add_image(&mut g, 32, 32);
    let r = g.add_node("resize", (0.0, 0.0));
    g.connect(a, r, 0).unwrap();
    g.set_param(r, "scale", Value::Float(4.0));
    g.set_param(r, "filter", Value::Choice("nearest".into()));
    let t = render(&engine, &g, r, &Request::native(Size::new(128.0, 128.0)));
    assert_eq!(t.data[(4 * 5 + 2) * 128 + 4 * 7 + 3], t.data[5 * 4 * 128 + 7 * 4]);

    for filter in ["bilinear", "bicubic", "fourier"] {
        g.set_param(r, "filter", Value::Choice(filter.into()));
        let t = render(&engine, &g, r, &Request::native(Size::new(128.0, 128.0)));
        // Green channel encodes y: it must stay monotonic away from wrap.
        let col: Vec<f32> = (0..120).map(|y| t.data[y * 128 + 64][1]).collect();
        assert!(col[100] > col[20], "{filter}");
        assert!(t.data.iter().all(|p| p[3] <= 1.0 + 1e-4 && p[0] <= p[3] + 1e-4));
    }
}

#[test]
fn reflection_mirrors_pixels() {
    let engine = Engine::new(false);
    let mut g = Graph::new();
    let a = add_image(&mut g, 100, 10);
    let r = g.add_node("reflect", (0.0, 0.0));
    g.connect(a, r, 0).unwrap();
    let t = render(&engine, &g, r, &Request::native(Size::new(100.0, 10.0)));
    // Axis at 50: pixel 60 copies pixel 39.
    assert!((t.data[60][0] - 39.0 / 255.0).abs() < 1e-5);
    assert!((t.data[30][0] - 30.0 / 255.0).abs() < 1e-5);
}

#[test]
fn clip_offsets_input() {
    let engine = Engine::new(false);
    let mut g = Graph::new();
    let a = add_image(&mut g, 100, 100);
    let c = g.add_node("clip", (0.0, 0.0));
    g.connect(a, c, 0).unwrap();
    assert_eq!(g.size_of(c), Some(Size::new(80.0, 80.0)));
    let t = render(&engine, &g, c, &Request::native(Size::new(80.0, 80.0)));
    assert!((t.data[0][0] - 10.0 / 255.0).abs() < 1e-5);
}

#[test]
fn space_map_follows_geometry_upstream() {
    // image 200×100 → clip (default 10% margins: 160×80 at 20,10) → resize ×2 → output
    let mut g = Graph::new();
    let a = add_image(&mut g, 200, 100);
    let c = g.add_node("clip", (0.0, 0.0));
    let r = g.add_node("resize", (0.0, 0.0));
    let n = g.add_node("noise", (0.0, 0.0));
    g.connect(a, c, 0).unwrap();
    g.connect(c, r, 0).unwrap();
    g.connect(r, g.output, 0).unwrap();
    // Output space → image space: halve, then offset by the clip origin.
    let (m, e) = g.space_map(g.output, a).unwrap();
    assert_eq!(e, Size::new(200.0, 100.0));
    assert!((m.x.apply(0.0) - 20.0).abs() < 1e-9 && (m.x.apply(320.0) - 180.0).abs() < 1e-9);
    assert!((m.y.apply(0.0) - 10.0).abs() < 1e-9 && (m.y.apply(160.0) - 90.0).abs() < 1e-9);
    // A node that does not feed the output has no mapping.
    assert!(g.space_map(g.output, n).is_none());
    // Mapping to itself is the identity at its own extent.
    let (m, e) = g.space_map(r, r).unwrap();
    assert_eq!((m.x.apply(7.0), e), (7.0, Size::new(320.0, 160.0)));
}

#[test]
fn every_blend_mode_renders() {
    let engine = Engine::new(false);
    let mut g = Graph::new();
    let a = add_image(&mut g, 16, 16);
    let n = g.add_node("noise", (0.0, 0.0));
    let b = g.add_node("blend", (0.0, 0.0));
    g.connect(a, b, 0).unwrap();
    g.connect(n, b, 1).unwrap();
    for (mode, _) in delos::engine::kernels::BLEND_MODES {
        g.set_param(b, "mode", Value::Choice(mode.to_string()));
        let t = render(&engine, &g, b, &Request::native(Size::new(16.0, 16.0)));
        assert!(t.data.iter().all(|p| p.iter().all(|v| v.is_finite() && *v >= 0.0 && *v <= 1.0001)), "{mode}");
    }
}

/// GPU kernels must agree with their CPU counterparts.
#[test]
fn gpu_matches_cpu() {
    let gpu = Engine::new(true);
    if !gpu.has_gpu() {
        eprintln!("no GPU available; skipping");
        return;
    }
    let cpu = Engine::new(false);
    let mut g = Graph::new();
    let img = add_image(&mut g, 257, 131);
    let mut targets = vec![];
    for kind in ["solid", "linear_gradient", "radial_gradient", "checker", "noise", "voronoi", "mandelbrot"] {
        targets.push(g.add_node(kind, (0.0, 0.0)));
    }
    let inv = g.add_node("invert", (0.0, 0.0));
    g.connect(img, inv, 0).unwrap();
    targets.push(inv);
    let refl = g.add_node("reflect", (0.0, 0.0));
    g.connect(img, refl, 0).unwrap();
    g.set_param(refl, "mode", Value::Choice("additive".into()));
    targets.push(refl);
    let bl = g.add_node("blend", (0.0, 0.0));
    g.connect(img, bl, 0).unwrap();
    g.connect(targets[4], bl, 1).unwrap();
    targets.push(bl);
    let rs = g.add_node("resize", (0.0, 0.0));
    g.connect(img, rs, 0).unwrap();
    g.set_param(rs, "scale", Value::Float(3.0));
    targets.push(rs);

    let mut modes: Vec<String> = delos::engine::kernels::BLEND_MODES.iter().map(|m| m.0.to_string()).collect();
    modes.insert(0, "multiply".into());
    for t in &targets {
        let extent = g.extent_of(*t);
        let req = Request::new(extent, Rect::new(0.0, 0.0, extent.w, extent.h), 200, 120);
        let a = render(&cpu, &g, *t, &req);
        let b = render(&gpu, &g, *t, &req);
        let kind = g.node(*t).unwrap().kind;
        if kind == "mandelbrot" {
            // Escape-time iteration is chaotic near the set's boundary, so
            // GPU fused multiply-adds flip isolated pixels; compare the mean.
            let d = mean_diff(&a, &b);
            assert!(d < 0.02, "{kind}: mean diff {d}");
        } else {
            let d = max_diff(&a, &b);
            assert!(d < 0.02, "{kind}: max diff {d}");
        }
    }
    for m in modes {
        g.set_param(bl, "mode", Value::Choice(m.clone()));
        let req = Request::native(Size::new(257.0, 131.0));
        let d = max_diff(&render(&cpu, &g, bl, &req), &render(&gpu, &g, bl, &req));
        // Dissolve and logic modes can flip at exact thresholds.
        assert!(d < 0.02 || m == "dissolve" || m == "hard_mix", "blend {m}: max diff {d}");
    }
}
