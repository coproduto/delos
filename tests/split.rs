//! Split nodes: two outputs that partition the input along a border.

use delos::engine::geom::{Rect, Request, Size};
use delos::engine::source::SourceImage;
use delos::engine::tile::CpuTile;
use delos::engine::{render_to_cpu, Engine};
use delos::graph::{Graph, Link, NodeId};
use delos::nodes::{self, Category, Value, IMAGE_PARAM};
use std::sync::Arc;

const W: u32 = 100;
const H: u32 = 60;

fn render(engine: &Engine, g: &Graph, target: impl Into<Link>, req: &Request) -> CpuTile {
    render_to_cpu(engine, g, target, req, &|| false, &|_| {}).expect("render")
}

/// A semi-transparent gradient image, so alpha is exercised too.
fn add_image(g: &mut Graph) -> NodeId {
    let mut px = Vec::new();
    for y in 0..H {
        for x in 0..W {
            px.extend_from_slice(&[(x * 2) as u8, (y * 4) as u8, 200, 128 + (x % 100) as u8]);
        }
    }
    let iid = g.add_image(Arc::new(SourceImage::from_rgba8("img", W, H, px)));
    let n = g.add_node("image", (0.0, 0.0));
    g.set_param(n, IMAGE_PARAM, Value::Int(iid as i64));
    g.set_param(n, "width", Value::Int(W as i64));
    g.set_param(n, "height", Value::Int(H as i64));
    n
}

fn split(g: &mut Graph, kind: &str, input: NodeId) -> NodeId {
    let s = g.add_node(kind, (0.0, 0.0));
    g.connect(input, s, 0).unwrap();
    s
}

fn native() -> Request {
    Request::native(Size::new(W as f64, H as f64))
}

const CLEAR: [f32; 4] = [0.0; 4];

/// Every pixel is in exactly one output, unchanged; `inside(x, y)` says
/// which (for native-resolution pixels).
fn assert_partition(engine: &Engine, g: &Graph, s: NodeId, src: NodeId, inside: impl Fn(u32, u32) -> bool) {
    let req = native();
    let full = render(engine, g, src, &req);
    let a = render(engine, g, Link::new(s, 0), &req);
    let b = render(engine, g, Link::new(s, 1), &req);
    for y in 0..H {
        for x in 0..W {
            let i = (y * W + x) as usize;
            let (expect_a, expect_b) = if inside(x, y) { (full.data[i], CLEAR) } else { (CLEAR, full.data[i]) };
            assert_eq!((a.data[i], b.data[i]), (expect_a, expect_b), "pixel ({x}, {y})");
        }
    }
}

#[test]
fn splits_are_registered_with_two_outputs() {
    for (id, outs) in [("split_vertical", ["left", "right"]), ("split_horizontal", ["top", "bottom"]), ("split_rect", ["inside", "outside"])] {
        let k = nodes::kind(id).unwrap();
        assert_eq!(k.outputs(), outs);
        assert_eq!(k.category(), Category::Split);
        assert!(k.user_creatable());
        assert!(!k.overlays(&Graph::new().nodes.values().next().unwrap().clone()).is_empty());
    }
}

#[test]
fn vertical_split_partitions_at_the_border() {
    let engine = Engine::new(false);
    let mut g = Graph::new();
    let img = add_image(&mut g);
    let s = split(&mut g, "split_vertical", img);
    g.set_param(s, "position", Value::Float(0.3));
    assert_partition(&engine, &g, s, img, |x, _| x < 30);
    // Both outputs have the input's size.
    assert_eq!(g.size_of(s), Some(Size::new(W as f64, H as f64)));
}

#[test]
fn horizontal_split_partitions_at_the_border() {
    let engine = Engine::new(false);
    let mut g = Graph::new();
    let img = add_image(&mut g);
    let s = split(&mut g, "split_horizontal", img);
    g.set_param(s, "position", Value::Float(0.75));
    assert_partition(&engine, &g, s, img, |_, y| y < 45);
}

#[test]
fn rectangle_split_partitions_inside_and_outside() {
    let engine = Engine::new(false);
    let mut g = Graph::new();
    let img = add_image(&mut g);
    let s = split(&mut g, "split_rect", img);
    for (k, v) in [("x", 0.1), ("y", 0.2), ("w", 0.5), ("h", 0.5)] {
        g.set_param(s, k, Value::Float(v));
    }
    // x in [10, 60), y in [12, 42)
    assert_partition(&engine, &g, s, img, |x, y| (10..60).contains(&x) && (12..42).contains(&y));
}

#[test]
fn borders_at_the_edges_give_everything_to_one_side() {
    let engine = Engine::new(false);
    let mut g = Graph::new();
    let img = add_image(&mut g);
    let s = split(&mut g, "split_vertical", img);
    g.set_param(s, "position", Value::Float(0.0));
    assert_partition(&engine, &g, s, img, |_, _| false);
    g.set_param(s, "position", Value::Float(1.0));
    assert_partition(&engine, &g, s, img, |_, _| true);
}

/// Stacking the two parts gives back the input, at any preview scale.
#[test]
fn parts_stack_back_into_the_original() {
    let engine = Engine::new(false);
    for kind in ["split_vertical", "split_horizontal", "split_rect"] {
        let mut g = Graph::new();
        let img = add_image(&mut g);
        let s = split(&mut g, kind, img);
        let stack = g.add_node("stack", (0.0, 0.0));
        g.connect(Link::new(s, 0), stack, 0).unwrap();
        g.connect(Link::new(s, 1), stack, 1).unwrap();
        let e = Size::new(W as f64, H as f64);
        for req in [native(), Request::new(e, e.rect(), 37, 23), Request::new(e, Rect::new(20.0, 10.0, 40.0, 30.0), 80, 60)] {
            let a = render(&engine, &g, img, &req);
            let b = render(&engine, &g, stack, &req);
            let diff = a.data.iter().zip(&b.data).flat_map(|(p, q)| (0..4).map(move |i| (p[i] - q[i]).abs())).fold(0.0f32, f32::max);
            assert!(diff < 1e-6, "{kind} at {}×{}: {diff}", req.width, req.height);
        }
    }
}

/// The point of splitting: an effect on one part only.
#[test]
fn localized_effect_on_one_side() {
    let engine = Engine::new(false);
    let mut g = Graph::new();
    let img = add_image(&mut g);
    let s = split(&mut g, "split_vertical", img);
    let inv = g.add_node("invert", (0.0, 0.0));
    g.connect(Link::new(s, 0), inv, 0).unwrap();
    let stack = g.add_node("stack", (0.0, 0.0));
    g.connect(inv, stack, 0).unwrap();
    g.connect(Link::new(s, 1), stack, 1).unwrap();
    let req = native();
    let orig = render(&engine, &g, img, &req);
    let out = render(&engine, &g, stack, &req);
    for (i, (o, r)) in orig.data.iter().zip(&out.data).enumerate() {
        let x = i as u32 % W;
        let expected = if x < 50 { [o[3] - o[0], o[3] - o[1], o[3] - o[2], o[3]] } else { *o };
        for c in 0..4 {
            assert!((expected[c] - r[c]).abs() < 1e-6, "pixel {i}");
        }
    }
}

#[test]
fn size_less_inputs_are_split_at_the_requested_extent() {
    let engine = Engine::new(false);
    let mut g = Graph::new();
    let noise = g.add_node("noise", (0.0, 0.0));
    let s = split(&mut g, "split_horizontal", noise);
    assert_eq!(g.size_of(s), None);
    let req = Request::native(Size::new(40.0, 20.0));
    let top = render(&engine, &g, Link::new(s, 0), &req);
    assert!(top.data[..40 * 10].iter().all(|p| p[3] > 0.0));
    assert!(top.data[40 * 10..].iter().all(|p| *p == CLEAR));
}

#[test]
fn outputs_are_distinct_links() {
    let mut g = Graph::new();
    let img = add_image(&mut g);
    let s = split(&mut g, "split_rect", img);
    let inv = g.add_node("invert", (0.0, 0.0));
    assert!(g.connect(Link::new(s, 2), inv, 0).unwrap_err().contains("no such output"));
    g.connect(Link::new(s, 1), inv, 0).unwrap();
    assert_eq!(g.node(inv).unwrap().inputs[0], Some(Link::new(s, 1)));
    assert_eq!(g.link_title(Link::new(s, 1)), "Rectangle Split · outside");
    let mut memo = Default::default();
    assert_ne!(g.link_hash(Link::new(s, 0), &mut memo), g.link_hash(Link::new(s, 1), &mut memo));
    // Removing the split disconnects consumers of either output.
    g.remove_node(s);
    assert_eq!(g.node(inv).unwrap().inputs[0], None);
}

#[test]
fn gpu_split_matches_cpu() {
    let gpu = Engine::new(true);
    if !gpu.has_gpu() {
        eprintln!("no GPU available; skipping");
        return;
    }
    let cpu = Engine::new(false);
    let mut g = Graph::new();
    let img = add_image(&mut g);
    let s = split(&mut g, "split_rect", img);
    let e = Size::new(W as f64, H as f64);
    for req in [native(), Request::new(e, e.rect(), 33, 21)] {
        for out in 0..2 {
            let a = render(&cpu, &g, Link::new(s, out), &req);
            let b = render(&gpu, &g, Link::new(s, out), &req);
            assert_eq!(a.data.len(), b.data.len());
            let diff = a.data.iter().zip(&b.data).flat_map(|(p, q)| (0..4).map(move |i| (p[i] - q[i]).abs())).fold(0.0f32, f32::max);
            assert!(diff < 1e-5, "output {out}: {diff}");
        }
    }
}
