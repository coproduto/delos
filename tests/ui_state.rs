//! UI state operations, run inside a headless Dioxus runtime (signals need
//! one). Regression tests for signal borrow conflicts, which panic with
//! `AlreadyBorrowed` and abort the desktop app.

use delos::graph::NodeId;
use delos::ui::state::App;
use dioxus::prelude::*;
use std::cell::Cell;

thread_local! {
    static DONE: Cell<bool> = const { Cell::new(false) };
}

#[derive(Props, Clone, PartialEq)]
struct HarnessProps {
    run: fn(App),
}

#[allow(non_snake_case)]
fn Harness(p: HarnessProps) -> Element {
    use_hook(|| {
        (p.run)(App::new());
        DONE.set(true);
    });
    rsx! {}
}

/// Runs `scenario` with a fresh `App` and fails if it panics or does not
/// finish.
fn with_app(scenario: fn(App)) {
    DONE.set(false);
    let mut dom = VirtualDom::new_with_props(Harness, HarnessProps { run: scenario });
    dom.rebuild_in_place();
    assert!(DONE.get(), "scenario did not complete");
}

/// noise → invert → output
fn chain(mut app: App) -> (NodeId, NodeId) {
    let mut g = app.graph.write();
    let noise = g.add_node("noise", (0.0, 0.0));
    let invert = g.add_node("invert", (200.0, 0.0));
    let out = g.output;
    g.connect(noise, invert, 0).unwrap();
    g.connect(invert, out, 0).unwrap();
    (noise, invert)
}

#[test]
fn delete_selected_node_while_output_is_relocated() {
    // The reported crash: Delete on a selected node with the output pinned
    // to a different node.
    with_app(|mut app| {
        let (noise, invert) = chain(app);
        app.pinned.set(Some(invert));
        app.select(Some(noise));
        app.delete_selection();
        let g = app.graph.peek();
        assert!(g.node(noise).is_none());
        assert_eq!(g.node(invert).unwrap().inputs[0], None);
        drop(g);
        assert_eq!(*app.selected.peek(), None);
        assert_eq!(*app.pinned.peek(), Some(invert));
    });
}

#[test]
fn delete_relocated_node_resets_output() {
    with_app(|mut app| {
        let (noise, invert) = chain(app);
        app.pinned.set(Some(invert));
        app.select(Some(invert));
        app.delete_selection();
        let g = app.graph.peek();
        assert!(g.node(invert).is_none());
        // Single-input nodes are bridged: noise now feeds the output.
        assert_eq!(g.node(g.output).unwrap().inputs[0], Some(noise));
        drop(g);
        assert_eq!(*app.selected.peek(), None);
        assert_eq!(*app.pinned.peek(), None);
    });
}

#[test]
fn delete_selected_node_without_relocation() {
    with_app(|app| {
        let (noise, invert) = chain(app);
        app.select(Some(invert));
        app.delete_selection();
        assert!(app.graph.peek().node(invert).is_none());
        assert_eq!(app.graph.peek().node(noise).map(|n| n.id), Some(noise));
        assert_eq!(*app.selected.peek(), None);
    });
}

#[test]
fn delete_selected_edge() {
    with_app(|mut app| {
        let (noise, invert) = chain(app);
        app.pinned.set(Some(invert));
        app.select(Some(invert));
        app.selected_edge.set(Some((invert, 0)));
        app.delete_selection();
        let g = app.graph.peek();
        assert!(g.node(noise).is_some() && g.node(invert).is_some());
        assert_eq!(g.node(invert).unwrap().inputs[0], None);
        drop(g);
        assert_eq!(*app.selected_edge.peek(), None);
    });
}

#[test]
fn delete_with_nothing_selected_is_a_no_op() {
    with_app(|app| {
        chain(app);
        let before = app.graph.peek().nodes.len();
        app.delete_selection();
        assert_eq!(app.graph.peek().nodes.len(), before);
    });
}
