//! UI state operations, run inside a headless Dioxus runtime (signals need
//! one). Regression tests for signal borrow conflicts, which panic with
//! `AlreadyBorrowed` and abort the desktop app.

use delos::graph::{Link, NodeId};
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
        app.pinned.set(Some(invert.into()));
        app.select(Some(noise));
        app.delete_selection();
        let g = app.graph.peek();
        assert!(g.node(noise).is_none());
        assert_eq!(g.node(invert).unwrap().inputs[0], None);
        drop(g);
        assert_eq!(*app.selected.peek(), None);
        assert_eq!(*app.pinned.peek(), Some(invert.into()));
    });
}

#[test]
fn delete_relocated_node_resets_output() {
    with_app(|mut app| {
        let (noise, invert) = chain(app);
        app.pinned.set(Some(invert.into()));
        app.select(Some(invert));
        app.delete_selection();
        let g = app.graph.peek();
        assert!(g.node(invert).is_none());
        // Single-input nodes are bridged: noise now feeds the output.
        assert_eq!(g.node(g.output).unwrap().input_node(0), Some(noise));
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
        app.pinned.set(Some(invert.into()));
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

/// noise → split_vertical → (left → invert → stack front, right → stack back) → output
fn split_graph(mut app: App) -> (NodeId, NodeId, NodeId, NodeId) {
    let mut g = app.graph.write();
    let noise = g.add_node("noise", (0.0, 0.0));
    let split = g.add_node("split_vertical", (200.0, 0.0));
    let invert = g.add_node("invert", (400.0, 0.0));
    let stack = g.add_node("stack", (600.0, 0.0));
    let out = g.output;
    g.connect(noise, split, 0).unwrap();
    g.connect(Link::new(split, 0), invert, 0).unwrap();
    g.connect(invert, stack, 0).unwrap();
    g.connect(Link::new(split, 1), stack, 1).unwrap();
    g.connect(stack, out, 0).unwrap();
    (noise, split, invert, stack)
}

#[test]
fn previewing_a_split_shows_its_input_with_the_border() {
    with_app(|app| {
        let (noise, split, _, _) = split_graph(app);
        app.select(Some(split));
        let d = app.displayed();
        assert_eq!(d.target, Link::from(noise), "the border is placed on the whole input");
        assert_eq!(d.editing, Some(split));
        let overlays = app.graph.peek().node(split).unwrap().kind().overlays(app.graph.peek().node(split).unwrap());
        assert!(matches!(overlays[0], delos::nodes::Overlay::Line { horizontal: false, .. }));
    });
}

#[test]
fn a_split_output_can_be_viewed_and_exported() {
    with_app(|mut app| {
        let (_, split, _, _) = split_graph(app);
        let right = Link::new(split, 1);
        app.pinned.set(Some(right));
        let d = app.displayed();
        assert_eq!(d.target, right);
        assert_eq!(d.export_target, right);
        assert_eq!(app.graph.peek().link_title(right), "Vertical Split · right");
        // Editing the split while viewing its right side keeps the view and
        // places the border over it.
        app.select(Some(split));
        let d = app.displayed();
        assert_eq!(d.target, right);
        assert!(d.overlay_space.is_some());
    });
}

#[test]
fn deleting_a_split_reconnects_both_sides_to_its_input() {
    with_app(|mut app| {
        let (noise, split, invert, stack) = split_graph(app);
        app.pinned.set(Some(Link::new(split, 1)));
        app.select(Some(split));
        app.delete_selection();
        let g = app.graph.peek();
        assert!(g.node(split).is_none());
        assert_eq!(g.node(invert).unwrap().inputs[0], Some(Link::from(noise)));
        assert_eq!(g.node(stack).unwrap().inputs[1], Some(Link::from(noise)));
        drop(g);
        assert_eq!(*app.pinned.peek(), None, "the view returns to the output");
    });
}

#[test]
fn adding_a_split_after_a_node_takes_over_its_consumers() {
    with_app(|app| {
        let (noise, invert) = chain(app);
        app.select(Some(noise));
        app.add_node("split_rect");
        let g = app.graph.peek();
        let split = g.nodes.values().find(|n| n.kind == "split_rect").unwrap().id;
        assert_eq!(g.node(split).unwrap().inputs[0], Some(Link::from(noise)));
        assert_eq!(g.node(invert).unwrap().inputs[0], Some(Link::new(split, 0)), "consumers read the inside part");
    });
}
