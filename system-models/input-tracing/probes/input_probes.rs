//! Probes for findings from the input traces (see ../findings.md). Each test
//! replays exactly the calls one input path makes, using the same headless
//! harness as `tests/ui_state.rs`.
//!
//! These tests FAIL on commit 197e30c: they describe the behaviour a user would
//! expect, not the current one. To run them:
//!
//! ```sh
//! cp system-models/input-tracing/probes/input_probes.rs tests/
//! cargo test --test input_probes
//! rm tests/input_probes.rs
//! ```

use delos::graph::NodeId;
use delos::nodes::Value;
use delos::ui::state::App;
use dioxus::prelude::*;
use std::cell::Cell;

thread_local! {
    static DONE: Cell<bool> = const { Cell::new(false) };
}

#[derive(Props, Clone)]
struct HarnessProps {
    run: fn(App),
}

impl PartialEq for HarnessProps {
    fn eq(&self, _: &Self) -> bool {
        false
    }
}

#[allow(non_snake_case)]
fn Harness(p: HarnessProps) -> Element {
    use_hook(|| {
        (p.run)(App::new());
        DONE.set(true);
    });
    rsx! {}
}

fn with_app(scenario: fn(App)) {
    DONE.set(false);
    let mut dom = VirtualDom::new_with_props(Harness, HarnessProps { run: scenario });
    dom.rebuild_in_place();
    assert!(DONE.get(), "scenario did not complete");
}

/// What a pointer press on a node in the graph view does
/// (src/ui/graph_view.rs:337-345), minus the drag bookkeeping.
fn press_node_in_graph_view(app: App, id: NodeId) {
    app.select(Some(id));
    app.edit_coalesced(&format!("move:{id}"), |_| {});
}

/// An image node of a given size (sizes come from its parameters, so no
/// pixel data is needed to reach the size-conflict check).
fn sized(app: App, w: i64, h: i64) -> NodeId {
    let mut id = 0;
    app.edit(|g| {
        id = g.add_node("image", (0.0, 0.0));
        g.set_param(id, "width", Value::Int(w));
        g.set_param(id, "height", Value::Int(h));
    });
    id
}

// ---------------------------------------------------------------------------
// Finding 1: selecting a node in the graph view erases the redo history.

#[test]
fn selecting_a_node_in_the_graph_view_keeps_redo() {
    with_app(|app| {
        app.add_node("noise");
        let noise = app.selected.peek().unwrap();
        app.add_node("invert"); // the edit we will undo
        app.undo();
        assert!(app.can_redo(), "precondition: one step to redo");
        // Click (press without moving) on a node to select it.
        press_node_in_graph_view(app, noise);
        assert!(app.can_redo(), "selecting a node discarded the redo step");
    });
}

#[test]
fn selecting_a_node_in_the_graph_view_adds_no_undo_step() {
    with_app(|app| {
        app.add_node("noise");
        let noise = app.selected.peek().unwrap();
        press_node_in_graph_view(app, noise);
        // Expected: a click is not an edit, so one undo takes back Add node.
        app.undo();
        assert!(app.graph.peek().node(noise).is_none(), "the first undo after a click did nothing (it restored an identical graph)");
    });
}

// ---------------------------------------------------------------------------
// Finding 4: dragging a point handle pushes two undo steps per pointer move.

/// What a pointer move does while a Point overlay is dragged
/// (src/ui/image_view.rs:249-252).
fn drag_point_to(app: App, node: NodeId, kx: &'static str, ky: &'static str, fx: f64, fy: f64) {
    app.set_param(node, kx, Value::Float(fx.clamp(-1.0, 2.0)));
    app.set_param(node, ky, Value::Float(fy.clamp(-1.0, 2.0)));
}

#[test]
fn one_undo_reverts_a_whole_point_drag() {
    with_app(|app| {
        app.add_node("radial_gradient");
        let rg = app.selected.peek().unwrap();
        let start = |app: App| {
            let g = app.graph.peek();
            let p = &g.node(rg).unwrap().params;
            (p.f("cx"), p.f("cy"))
        };
        let before = start(app);
        for k in 1..=10 {
            drag_point_to(app, rg, "cx", "cy", 0.5 + 0.01 * k as f64, 0.5 - 0.01 * k as f64);
        }
        app.undo();
        assert_eq!(start(app), before, "one undo did not take back the drag");
    });
}

#[test]
fn a_point_drag_does_not_evict_earlier_undo_history() {
    with_app(|app| {
        app.add_node("noise"); // an edit made before the drag
        let noise = app.selected.peek().unwrap();
        app.select(None);
        app.add_node("radial_gradient");
        let rg = app.selected.peek().unwrap();
        // A short drag: 40 pointer moves (well under a second of dragging).
        for k in 1..=40 {
            drag_point_to(app, rg, "cx", "cy", 0.5 + 0.005 * k as f64, 0.5);
        }
        // Undo everything that can be undone.
        let mut steps = 0;
        while app.can_undo() && steps < 1000 {
            app.undo();
            steps += 1;
        }
        assert!(
            app.graph.peek().node(noise).is_none(),
            "the drag pushed {steps} undo steps and the history limit evicted the Add node edit before it"
        );
    });
}

// ---------------------------------------------------------------------------
// Finding 2: undo and redo do not recompute the size-conflict prompt.

#[test]
fn undoing_a_conflicting_connection_dismisses_the_prompt() {
    with_app(|app| {
        let a = sized(app, 100, 100);
        let b = sized(app, 50, 50);
        let mut blend = 0;
        app.edit(|g| blend = g.add_node("blend", (300.0, 0.0)));
        app.edit(|g| g.connect(a, blend, 0).unwrap());
        app.edit(|g| g.connect(b, blend, 1).unwrap());
        assert!(app.conflict.peek().is_some(), "precondition: the prompt opens");
        app.undo(); // takes back the second connection
        assert_eq!(app.graph.peek().size_conflict(blend), None, "test setup");
        assert!(app.conflict.peek().is_none(), "the prompt is still open for a conflict that no longer exists");
    });
}

#[test]
fn undoing_a_resolution_does_not_reopen_the_prompt_on_a_later_unrelated_edit() {
    with_app(|app| {
        let a = sized(app, 100, 100);
        let b = sized(app, 50, 50);
        let other = sized(app, 10, 10);
        let mut blend = 0;
        app.edit(|g| blend = g.add_node("blend", (300.0, 0.0)));
        app.edit(|g| g.connect(a, blend, 0).unwrap());
        app.edit(|g| g.connect(b, blend, 1).unwrap());
        // "Use 100×100 (resize …)" in the conflict dialog (src/ui/dialogs.rs:42-45).
        let c = app.conflict.peek().clone().unwrap();
        app.edit(|g| {
            g.insert_resize(c.node, c.b.0, c.a.1);
        });
        let mut conflict = app.conflict;
        conflict.set(None);
        app.undo(); // takes the Resize node back out: the conflict is back
        assert!(app.graph.peek().size_conflict(blend).is_some(), "test setup");
        let shown_after_undo = app.conflict.peek().is_some();
        // An edit somewhere else entirely.
        app.set_param(other, "width", Value::Int(11));
        let shown_after_unrelated_edit = app.conflict.peek().is_some();
        assert_eq!(
            shown_after_undo, shown_after_unrelated_edit,
            "the prompt appeared on an unrelated edit instead of on the undo that caused the conflict"
        );
    });
}
