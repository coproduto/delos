//! The node graph editor. Chains of stack operators are displayed as a
//! single "Layer Stack" node with one port per layer instead of a nested
//! binary tree.

use super::state::{fmt_size, App};
use crate::graph::{Graph, NodeId};
use crate::nodes::{ids, Category};
use dioxus::prelude::*;
use std::collections::HashMap;
use std::rc::Rc;

const NODE_W: f64 = 184.0;
const HEADER: f64 = 44.0;
const ROW: f64 = 24.0;
const PAD: f64 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PortRef {
    Input(NodeId, usize),
    Layer(NodeId, usize),
    AddLayer(NodeId),
}

#[derive(Clone, Debug, PartialEq)]
struct Row {
    label: String,
    port: PortRef,
    source: Option<NodeId>,
}

#[derive(Clone, Debug, PartialEq)]
struct DNode {
    id: NodeId,
    title: String,
    sub: String,
    x: f64,
    y: f64,
    rows: Vec<Row>,
    has_output: bool,
    class: &'static str,
}

impl DNode {
    fn height(&self) -> f64 {
        HEADER + ROW * self.rows.len() as f64 + PAD
    }
    fn out_port(&self) -> (f64, f64) {
        (self.x + NODE_W, self.y + HEADER / 2.0)
    }
    fn in_port(&self, row: usize) -> (f64, f64) {
        (self.x, self.y + HEADER + ROW * row as f64 + ROW / 2.0)
    }
}

fn build(g: &Graph) -> Vec<DNode> {
    let sizes = g.sizes();
    let mut out = Vec::new();
    for n in g.nodes.values() {
        if g.is_interior_stack(n.id) {
            continue;
        }
        let k = n.kind();
        let size = sizes.get(&n.id).copied().flatten();
        let sub = match size {
            Some(s) => fmt_size(s),
            None => "size-less".into(),
        };
        let (title, rows) = if let Some(chain) = g.stack_chain(n.id) {
            let mut rows: Vec<Row> = chain
                .layers
                .iter()
                .enumerate()
                .map(|(i, l)| Row {
                    label: if i == 0 { "Layer 1 (front)".into() } else { format!("Layer {}", i + 1) },
                    port: PortRef::Layer(chain.head, i),
                    source: *l,
                })
                .collect();
            rows.push(Row { label: "+ add layer".into(), port: PortRef::AddLayer(chain.head), source: None });
            (n.label.clone().unwrap_or_else(|| "Layer Stack".into()), rows)
        } else {
            let rows = k
                .inputs()
                .iter()
                .enumerate()
                .map(|(i, name)| Row { label: name.to_string(), port: PortRef::Input(n.id, i), source: n.inputs[i] })
                .collect();
            (n.title(), rows)
        };
        let class = match k.category() {
            Category::Source => "cat-source",
            Category::Generator => "cat-generator",
            Category::Filter => "cat-filter",
            Category::Combine => "cat-combine",
            Category::Output => "cat-output",
        };
        out.push(DNode { id: n.id, title, sub, x: n.pos.0, y: n.pos.1, rows, has_output: n.kind != ids::OUTPUT, class });
    }
    out
}

/// Connects `from` to a port (handling layer-stack ports).
pub fn connect_port(app: App, from: NodeId, port: PortRef) {
    let mut err = None;
    app.edit(|g| match port {
        PortRef::Input(n, p) => {
            if let Err(e) = g.connect(from, n, p) {
                err = Some(e);
            }
        }
        PortRef::Layer(head, _) | PortRef::AddLayer(head) => {
            if g.depends_on(from, head) {
                err = Some("that connection would create a cycle".into());
                return;
            }
            if let Some(chain) = g.stack_chain(head) {
                let mut layers = chain.layers.clone();
                match port {
                    PortRef::Layer(_, i) if i < layers.len() => layers[i] = Some(from),
                    _ => layers.push(Some(from)),
                }
                g.set_stack_layers(chain.head, layers);
            }
        }
    });
    if let Some(e) = err {
        app.toast(e, true);
    }
}

/// Removes the edge into a port; for layer-stack ports the layer is removed.
pub fn disconnect_port(app: App, to: NodeId, port: usize) {
    // Bind first: the graph borrow must end before the edit writes to it.
    let edge = decode_edge(&app.graph.peek(), to, port);
    disconnect_ref(app, edge);
}

fn decode_edge(g: &Graph, to: NodeId, port: usize) -> PortRef {
    if g.stack_chain(to).is_some() && port >= 1_000_000 {
        PortRef::Layer(to, port - 1_000_000)
    } else {
        PortRef::Input(to, port)
    }
}

fn encode_edge(p: PortRef) -> Option<(NodeId, usize)> {
    match p {
        PortRef::Input(n, i) => Some((n, i)),
        PortRef::Layer(h, i) => Some((h, 1_000_000 + i)),
        PortRef::AddLayer(_) => None,
    }
}

fn disconnect_ref(mut app: App, p: PortRef) {
    app.edit(|g| match p {
        PortRef::Input(n, i) => g.disconnect(n, i),
        PortRef::Layer(head, i) => {
            if let Some(chain) = g.stack_chain(head) {
                let mut layers = chain.layers.clone();
                if i < layers.len() {
                    layers.remove(i);
                }
                g.set_stack_layers(chain.head, layers);
            }
        }
        PortRef::AddLayer(_) => {}
    });
    app.selected_edge.set(None);
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum GDrag {
    None,
    Pan { x: f64, y: f64, px: f64, py: f64 },
    Node { id: NodeId, dx: f64, dy: f64 },
    Wire { from: NodeId, x: f64, y: f64 },
}

#[component]
pub fn GraphView() -> Element {
    let mut app = use_context::<App>();
    let mut pan = use_signal(|| (-40.0f64, -40.0f64));
    let mut zoom = use_signal(|| 1.0f64);
    let mut origin = use_signal(|| (0.0f64, 0.0f64));
    let mut mounted = use_signal(|| None::<Rc<MountedData>>);
    let mut drag = use_signal(|| GDrag::None);

    let to_graph = move |cx: f64, cy: f64| {
        let (ox, oy) = origin();
        let (px, py) = pan();
        let z = zoom();
        ((cx - ox) / z + px, (cy - oy) / z + py)
    };
    let refresh_origin = move || async move {
        let m = mounted.peek().clone();
        if let Some(m) = m {
            if let Ok(r) = m.get_client_rect().await {
                origin.set((r.origin.x, r.origin.y));
            }
        }
    };

    let graph = app.graph.read();
    let nodes = build(&graph);
    let output = graph.output;
    drop(graph);
    let by_id: HashMap<NodeId, &DNode> = nodes.iter().map(|n| (n.id, n)).collect();
    let selected = *app.selected.read();
    let selected_edge = *app.selected_edge.read();
    let pinned = *app.pinned.read();
    let errors = app.errors.read().clone();
    let (px, py) = pan();
    let z = zoom();

    // Edges.
    let mut edges = Vec::new();
    for n in &nodes {
        for (r, row) in n.rows.iter().enumerate() {
            if let Some(src) = row.source.and_then(|s| by_id.get(&s)) {
                let (x1, y1) = src.out_port();
                let (x2, y2) = n.in_port(r);
                let dx = ((x2 - x1).abs() / 2.0).max(50.0);
                let d = format!("M{x1},{y1} C{},{y1} {},{y2} {x2},{y2}", x1 + dx, x2 - dx);
                let sel = encode_edge(row.port) == selected_edge && selected_edge.is_some();
                edges.push((d, row.port, sel));
            }
        }
    }
    let wire = match drag() {
        GDrag::Wire { from, x, y } => by_id.get(&from).map(|s| {
            let (x1, y1) = s.out_port();
            let dx = ((x - x1).abs() / 2.0).max(50.0);
            format!("M{x1},{y1} C{},{y1} {},{y} {x},{y}", x1 + dx, x - dx)
        }),
        _ => None,
    };
    let wiring = matches!(drag(), GDrag::Wire { .. });

    rsx! {
        div {
            class: if wiring { "graph wiring" } else { "graph" },
            onmounted: move |e| async move {
                mounted.set(Some(e.data()));
                refresh_origin().await;
            },
            onresize: move |_| async move { refresh_origin().await },
            onwheel: move |e: WheelEvent| {
                e.prevent_default();
                let c = e.client_coordinates();
                let (gx, gy) = to_graph(c.x, c.y);
                let f = (-e.delta().strip_units().y * 0.0015).exp().clamp(0.5, 2.0);
                let nz = (zoom() * f).clamp(0.2, 3.0);
                let (ox, oy) = origin();
                zoom.set(nz);
                pan.set((gx - (c.x - ox) / nz, gy - (c.y - oy) / nz));
            },
            onpointerdown: move |e: PointerEvent| {
                let c = e.client_coordinates();
                let (px, py) = pan();
                drag.set(GDrag::Pan { x: c.x, y: c.y, px, py });
                app.select(None);
            },
            onpointermove: move |e: PointerEvent| {
                let c = e.client_coordinates();
                match drag() {
                    GDrag::None => {}
                    GDrag::Pan { x, y, px, py } => {
                        let z = zoom();
                        pan.set((px - (c.x - x) / z, py - (c.y - y) / z));
                    }
                    GDrag::Node { id, dx, dy } => {
                        let (gx, gy) = to_graph(c.x, c.y);
                        let mut g = app.graph.write();
                        if let Some(n) = g.node_mut(id) {
                            n.pos = ((gx - dx).round(), (gy - dy).round());
                        }
                    }
                    GDrag::Wire { from, .. } => {
                        let (gx, gy) = to_graph(c.x, c.y);
                        drag.set(GDrag::Wire { from, x: gx, y: gy });
                    }
                }
            },
            onpointerup: move |_| drag.set(GDrag::None),
            onpointerleave: move |_| drag.set(GDrag::None),

            div {
                class: "graph-layer",
                style: "transform: scale({z}) translate({-px}px, {-py}px);",
                svg { class: "edges", width: "1", height: "1",
                    for (i , (d , port , sel)) in edges.into_iter().enumerate() {
                        g { key: "{i}",
                            path { class: if sel { "edge selected" } else { "edge" }, d: "{d}" }
                            path {
                                class: "edge-hit",
                                d: "{d}",
                                onpointerdown: move |e: PointerEvent| {
                                    e.stop_propagation();
                                    app.selected.set(None);
                                    app.selected_edge.set(encode_edge(port));
                                },
                            }
                        }
                    }
                    if let Some(w) = wire {
                        path { class: "edge wire", d: "{w}" }
                    }
                }
                for n in nodes.iter().cloned() {
                    {
                        let id = n.id;
                        let mut cls = format!("node {}", n.class);
                        if selected == Some(id) { cls.push_str(" selected"); }
                        if errors.contains_key(&id) { cls.push_str(" error"); }
                        let is_pinned = pinned == Some(id);
                        let h = n.height();
                        rsx! {
                            div {
                                key: "{id}",
                                class: "{cls}",
                                style: "left:{n.x}px;top:{n.y}px;width:{NODE_W}px;height:{h}px;",
                                onpointerdown: move |e: PointerEvent| {
                                    e.stop_propagation();
                                    let c = e.client_coordinates();
                                    let (gx, gy) = to_graph(c.x, c.y);
                                    let pos = app.graph.peek().node(id).map(|n| n.pos).unwrap_or_default();
                                    app.select(Some(id));
                                    app.edit_coalesced(&format!("move:{id}"), |_| {});
                                    drag.set(GDrag::Node { id, dx: gx - pos.0, dy: gy - pos.1 });
                                },
                                ondoubleclick: move |e| {
                                    e.stop_propagation();
                                    // Toggle: the output node or the relocated node
                                    // moves the output back to its default.
                                    if id == output || is_pinned {
                                        app.pinned.set(None);
                                    } else {
                                        app.pinned.set(Some(id));
                                    }
                                    app.select(None);
                                },
                                div { class: "node-header",
                                    div { class: "node-title", "{n.title}" }
                                    div { class: "node-sub",
                                        "{n.sub}"
                                        if is_pinned { span { class: "viewing", " · viewing" } }
                                    }
                                }
                                if n.has_output {
                                    div {
                                        class: "port out",
                                        style: "top:{HEADER / 2.0 - 7.0}px;",
                                        title: "Drag to an input to connect",
                                        onpointerdown: move |e: PointerEvent| {
                                            e.stop_propagation();
                                            let c = e.client_coordinates();
                                            let (gx, gy) = to_graph(c.x, c.y);
                                            drag.set(GDrag::Wire { from: id, x: gx, y: gy });
                                        },
                                    }
                                }
                                for (r , row) in n.rows.iter().cloned().enumerate() {
                                    {
                                        let top = HEADER + ROW * r as f64;
                                        let port = row.port;
                                        let src = row.source;
                                        let pcls = if src.is_some() { "port in connected" } else { "port in" };
                                        let rcls = if matches!(port, PortRef::AddLayer(_)) { "row add" } else { "row" };
                                        rsx! {
                                            div { key: "{r}", class: "{rcls}", style: "top:{top}px;height:{ROW}px;",
                                                div {
                                                    class: "{pcls}",
                                                    onpointerdown: move |e: PointerEvent| {
                                                        e.stop_propagation();
                                                        // Pick up an existing wire to move it elsewhere.
                                                        if let Some(s) = src {
                                                            disconnect_ref(app, port);
                                                            let c = e.client_coordinates();
                                                            let (gx, gy) = to_graph(c.x, c.y);
                                                            drag.set(GDrag::Wire { from: s, x: gx, y: gy });
                                                        }
                                                    },
                                                    onpointerup: move |e: PointerEvent| {
                                                        if let GDrag::Wire { from, .. } = drag() {
                                                            e.stop_propagation();
                                                            drag.set(GDrag::None);
                                                            connect_port(app, from, port);
                                                        }
                                                    },
                                                }
                                                span { class: "row-label", "{row.label}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "graph-tools",
                onpointerdown: move |e| e.stop_propagation(),
                button {
                    class: "btn small",
                    onclick: move |_| {
                        let g = app.graph.peek();
                        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
                        for n in g.nodes.values() {
                            x0 = x0.min(n.pos.0);
                            y0 = y0.min(n.pos.1);
                            x1 = x1.max(n.pos.0 + NODE_W);
                            y1 = y1.max(n.pos.1 + 140.0);
                        }
                        drop(g);
                        zoom.set(1.0);
                        pan.set((x0 - 40.0, y0 - 40.0));
                        let _ = (x1, y1);
                    },
                    "Home"
                }
                button { class: "btn small", onclick: move |_| zoom.set((zoom() * 1.2).min(3.0)), "+" }
                button { class: "btn small", onclick: move |_| zoom.set((zoom() / 1.2).max(0.2)), "−" }
                span { class: "muted", "Drag from ● to connect · double-click a node to relocate the output, again (or the Output) to reset" }
            }
        }
    }
}
