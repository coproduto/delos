//! Probes for the two invariant violations in `../findings.md`.
//!
//! Both tests FAIL on the code as analysed (commit 197e30c): each one asserts
//! the invariant and shows that it is broken. To run them:
//!
//!   cp system-models/invariants/probes/invariant_probes.rs tests/
//!   cargo test --test invariant_probes -- --nocapture
//!
//! They could become regression tests once the invariants are restored.

use delos::engine::source::SourceImage;
use delos::graph::{Graph, Link};
use delos::nodes::{ids, Value, IMAGE_PARAM};
use delos::ui::state::App;
use dioxus::prelude::*;
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::Arc;

/// Imports a 2×2 image of one color and connects it to the output, the way
/// `ui::finish_import` does.
fn import(g: &mut Graph, px: [u8; 4]) {
    let img = Arc::new(SourceImage::from_rgba8("x.png", 2, 2, px.repeat(4)));
    let iid = g.add_image(img);
    let n = g.add_node(ids::IMAGE, (60.0, 40.0));
    g.set_param(n, IMAGE_PARAM, Value::Int(iid as i64));
    g.set_param(n, "width", Value::Int(2));
    g.set_param(n, "height", Value::Int(2));
    let out = g.output;
    g.connect(n, out, 0).unwrap();
}

/// Invariant I1 (`04-structural-identity.md`): equal structural hashes mean
/// equal images. Undo restores a snapshot with its older id counter, so an
/// import after the undo reuses the image id of the import that was undone.
#[test]
fn structural_hash_distinguishes_images_after_undo() {
    let mut g = Graph::new();
    let before_import = g.clone(); // what undo restores
    import(&mut g, [255, 0, 0, 255]); // red
    let red = g.hash_of(g.output, &mut HashMap::new());

    let mut g = before_import; // undo
    import(&mut g, [0, 0, 255, 255]); // blue
    let blue = g.hash_of(g.output, &mut HashMap::new());

    assert_ne!(red, blue, "a red and a blue image have the same structural hash");
}

thread_local! {
    static FAILURE: Cell<Option<&'static str>> = const { Cell::new(None) };
}

#[derive(Props, Clone)]
struct P {
    run: fn(App),
}
impl PartialEq for P {
    fn eq(&self, _: &Self) -> bool {
        false
    }
}
#[allow(non_snake_case)]
fn H(p: P) -> Element {
    use_hook(|| (p.run)(App::new()));
    rsx! {}
}

/// Invariant U6 (`05-concurrency-and-ui.md`): "Add node" with a node selected
/// inserts the new node after it while keeping the picture's meaning. With a
/// split selected, consumers of its second output are rewired onto the new
/// node, which is fed the split's first output.
#[test]
fn add_node_after_a_split_keeps_each_consumer_on_its_output() {
    FAILURE.set(None);
    let mut dom = VirtualDom::new_with_props(
        H,
        P {
            run: |mut app| {
                let (split, right_consumer);
                {
                    let mut g = app.graph.write();
                    let noise = g.add_node("noise", (0.0, 0.0));
                    split = g.add_node("split_vertical", (200.0, 0.0));
                    g.connect(noise, split, 0).unwrap();
                    let left_consumer = g.add_node("invert", (400.0, 0.0));
                    right_consumer = g.add_node("invert", (400.0, 200.0));
                    g.connect(Link::new(split, 0), left_consumer, 0).unwrap();
                    g.connect(Link::new(split, 1), right_consumer, 0).unwrap();
                }
                app.select(Some(split));
                app.add_node("invert");
                let g = app.graph.peek();
                let inserted = g.node(right_consumer).unwrap().inputs[0].unwrap().node;
                let fed_by = g.node(inserted).unwrap().inputs[0];
                if fed_by != Some(Link::new(split, 1)) {
                    FAILURE.set(Some("the right-hand consumer now reads the split's LEFT output"));
                }
            },
        },
    );
    dom.rebuild_in_place();
    // Dioxus catches panics inside components, so the result is reported here.
    if let Some(msg) = FAILURE.get() {
        panic!("{msg}");
    }
}
