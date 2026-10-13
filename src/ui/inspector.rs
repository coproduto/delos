//! The inspector: parameter forms (generated from each kind's `ParamDef`s),
//! input pickers, and the layer-stack editor.

use super::state::{fmt_size, App, ImportTarget};
use super::session::{self, ImportSource};
use super::WorkerHandle;
use crate::engine::geom::Size;
use crate::formats;
use crate::graph::{Link, NodeId};
use crate::nodes::{self, ids, Axis, Category, ParamDef, ParamKind, Params, Value};
use dioxus::prelude::*;

#[component]
pub fn Inspector() -> Element {
    let app = use_context::<App>();
    let sel = *app.selected.read();
    let exists = sel.is_some_and(|s| app.graph.read().node(s).is_some());
    rsx! {
        div { class: "inspector", onkeydown: move |e| e.stop_propagation(),
            if let (Some(id), true) = (sel, exists) {
                NodePanel { id }
            } else {
                DocumentPanel {}
            }
        }
    }
}

#[component]
fn DocumentPanel() -> Element {
    let app = use_context::<App>();
    let out = app.graph.read().output;
    rsx! {
        div { class: "panel",
            h3 { "Document" }
            p { class: "muted",
                "Select a node to edit it. The image view shows the selected node (\"Preview selection\") or the output."
            }
            NodePanel { id: out }
            div { class: "help",
                h4 { "Tips" }
                ul {
                    li { "Drop image files anywhere to import them." }
                    li { "With a node selected, " b { "Add node" } " inserts the new node after it." }
                    li { "Double-click a node in the graph to relocate the output to it; double-click it again or the Output node to reset." }
                    li { "While the output is relocated (or " b { "Preview selection" } " is off), the view stays put: select an upstream node to edit it directly on that image." }
                    li { "Scroll to zoom, drag to pan; F fits the image." }
                    li { "⌘Z / ⇧⌘Z undo and redo, Delete removes the selection." }
                }
            }
        }
    }
}

#[component]
fn NodePanel(id: NodeId) -> Element {
    let mut app = use_context::<App>();
    let graph = app.graph.read();
    let Some(node) = graph.node(id).cloned() else { return rsx! {} };
    let k = node.kind();
    let size = graph.size_of(id);
    let extent = graph.extent_of(id);
    // Fractions are relative to the space the node's controls live in (its
    // input for nodes like Clip).
    let param_extent = match k.edit_view() {
        nodes::EditView::Output => extent,
        nodes::EditView::Input(p) => {
            let input = node.input_node(p).and_then(|i| graph.size_of(i));
            k.input_map(&node, p, extent, input).1
        }
    };
    let is_output = id == graph.output;
    let chain = graph.stack_chain(id);
    let consumers = graph.consumers(id).len();
    // Input candidates: every node output that would not create a cycle.
    let candidates: Vec<(Link, String)> = graph
        .nodes
        .values()
        .filter(|n| n.id != id && n.kind != ids::OUTPUT && !graph.depends_on(n.id, id) && !graph.is_interior_stack(n.id))
        .flat_map(|n| (0..n.kind().outputs().len()).map(move |o| Link::new(n.id, o)))
        .map(|l| (l, graph.link_title(l)))
        .collect();
    let input_titles: Vec<Option<Link>> = node.inputs.clone();
    drop(graph);
    let error = app.errors.read().get(&id).cloned();
    // Which of this node's outputs the output is relocated to, if any.
    let pinned_output = app.pinned.read().filter(|p| p.node == id).map(|p| p.output);
    let outputs = k.outputs();
    let multi = outputs.len() > 1;
    let export_link = Link::new(id, pinned_output.unwrap_or(0));
    let export_label = if multi { format!("Export {}…", outputs[export_link.output]) } else { "Export…".to_string() };
    let title = node.title();
    let size_text = match size {
        Some(s) if is_output => format!("Output size {}", fmt_size(s)),
        Some(s) => fmt_size(s),
        None => format!("Size-less · rendered at {} when shown", fmt_size(extent)),
    };
    let defs: Vec<ParamDef> = k.params().into_iter().filter(|d| !matches!(d.kind, ParamKind::Hidden)).collect();
    let params = node.params.clone();

    rsx! {
        div { class: "panel",
            if !is_output {
                div { class: "panel-head",
                    input {
                        class: "title-input",
                        value: "{title}",
                        onchange: move |e| {
                            let v = e.value();
                            app.edit(|g| {
                                if let Some(n) = g.node_mut(id) {
                                    n.label = if v.trim().is_empty() { None } else { Some(v.clone()) };
                                }
                            });
                        },
                    }
                    span { class: "chip", "{k.name()}" }
                }
            } else {
                h3 { "Output" }
            }
            p { class: "muted", "{k.description()}" }
            div { class: "size-line", "{size_text}" }
            if let Some(h) = k.slow_hint() {
                div { class: "hint", "⏱ {h}" }
            }
            if let Some(err) = error {
                div { class: "error-box inline", "{err}" }
            }
            if !is_output {
                div { class: "actions",
                    for (o , name) in outputs.iter().enumerate() {
                        {
                            let viewing = pinned_output == Some(o);
                            let label = match (multi, viewing) {
                                (false, false) => "View as output".to_string(),
                                (false, true) => "Viewing ✓".to_string(),
                                (true, false) => format!("View {name}"),
                                (true, true) => format!("Viewing {name} ✓"),
                            };
                            rsx! {
                                button {
                                    key: "{o}",
                                    class: if viewing { "btn active" } else { "btn" },
                                    title: "Temporarily relocate the output to this node",
                                    onclick: move |_| {
                                        if viewing { app.pinned.set(None) } else { app.pinned.set(Some(Link::new(id, o))) }
                                    },
                                    "{label}"
                                }
                            }
                        }
                    }
                    button { class: "btn", onclick: move |_| app.open_export(Some(export_link)), "{export_label}" }
                    button { class: "btn danger", onclick: move |_| app.delete_node(id), "Delete" }
                }
            }
            if let Some(chain) = chain {
                LayerStack { head: chain.head }
            } else if !k.inputs().is_empty() {
                div { class: "section",
                    h4 { "Inputs" }
                    for (port , name) in k.inputs().iter().enumerate() {
                        div { class: "field", key: "{port}",
                            label { "{name}" }
                            select {
                                onchange: move |e| {
                                    let v = e.value();
                                    let link = v.split_once(':').and_then(|(n, o)| Some(Link::new(n.parse().ok()?, o.parse().ok()?)));
                                    match link {
                                        Some(src) => super::graph_view::connect_port(app, src, super::graph_view::PortRef::Input(id, port)),
                                        None => app.edit(|g| g.disconnect(id, port)),
                                    }
                                },
                                option { value: "", selected: input_titles[port].is_none(), "— none —" }
                                for (cl , ct) in candidates.iter().cloned() {
                                    option {
                                        key: "{cl.node}:{cl.output}",
                                        value: "{cl.node}:{cl.output}",
                                        selected: input_titles[port] == Some(cl),
                                        "{ct}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if !defs.is_empty() {
                div { class: "section",
                    h4 { "Parameters" }
                    for def in defs {
                        if visible(&def, &params) {
                            ParamField { key: "{def.key}", id, def: ParamDefProp(def.clone()), params: ParamsProp(params.clone()), extent: param_extent }
                        }
                    }
                }
            }
            if is_output {
                div { class: "actions",
                    button {
                        class: "btn",
                        title: "Use the image view's size as the canvas for size-less content",
                        onclick: move |_| app.fit_canvas_to_window(true),
                        "Canvas = window size"
                    }
                }
            }
            if !is_output && k.category() != Category::Output {
                div { class: "muted small", "Used by {consumers} node(s)." }
            }
        }
    }
}

fn visible(def: &ParamDef, p: &Params) -> bool {
    match def.visible_if {
        Some((k, values)) => values.contains(&p.choice(k)),
        None => true,
    }
}

#[derive(Clone)]
struct ParamDefProp(ParamDef);
impl PartialEq for ParamDefProp {
    fn eq(&self, o: &Self) -> bool {
        self.0.key == o.0.key && self.0.label == o.0.label
    }
}

#[derive(Clone, PartialEq)]
struct ParamsProp(Params);

fn hex(c: [f32; 4]) -> String {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]))
}

fn parse_hex(s: &str, alpha: f32) -> Option<[f32; 4]> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let v = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok().map(|x| x as f32 / 255.0);
    Some([v(0)?, v(2)?, v(4)?, alpha])
}

#[component]
fn ParamField(id: NodeId, def: ParamDefProp, params: ParamsProp, extent: Size) -> Element {
    let app = use_context::<App>();
    let def = def.0;
    let p = params.0;
    let key = def.key;
    let label = def.label;
    let set = move |v: Value| app.set_param(id, key, v);
    match def.kind {
        ParamKind::Float { min, max, step } => {
            let v = p.f(key);
            rsx! {
                div { class: "field",
                    label { "{label}" }
                    div { class: "slider-row",
                        input {
                            r#type: "range",
                            min: "{min}",
                            max: "{max}",
                            step: "{step}",
                            value: "{v}",
                            oninput: move |e| if let Ok(x) = e.value().parse::<f64>() { set(Value::Float(x)) },
                        }
                        input {
                            r#type: "number",
                            class: "num",
                            step: "{step}",
                            value: "{v:.3}",
                            onchange: move |e| if let Ok(x) = e.value().parse::<f64>() { set(Value::Float(x)) },
                        }
                    }
                }
            }
        }
        ParamKind::Fraction { axis } => {
            let v = p.f(key);
            let len = match axis {
                Axis::X => extent.w,
                Axis::Y => extent.h,
                Axis::Min => extent.w.min(extent.h),
            };
            let px = (v * len).round();
            rsx! {
                div { class: "field",
                    label { "{label}" }
                    div { class: "slider-row",
                        input {
                            r#type: "range",
                            min: "0",
                            max: "1",
                            step: "0.001",
                            value: "{v}",
                            oninput: move |e| if let Ok(x) = e.value().parse::<f64>() { set(Value::Float(x)) },
                        }
                        input {
                            r#type: "number",
                            class: "num",
                            title: "pixels",
                            value: "{px}",
                            onchange: move |e| if let Ok(x) = e.value().parse::<f64>() { set(Value::Float(x / len.max(1.0))) },
                        }
                        span { class: "unit", "px" }
                    }
                }
            }
        }
        ParamKind::Int { min, max } => {
            let v = p.i(key);
            let slider = max - min <= 64;
            rsx! {
                div { class: "field",
                    label { "{label}" }
                    div { class: "slider-row",
                        if slider {
                            input {
                                r#type: "range",
                                min: "{min}",
                                max: "{max}",
                                step: "1",
                                value: "{v}",
                                oninput: move |e| if let Ok(x) = e.value().parse::<i64>() { set(Value::Int(x)) },
                            }
                        }
                        input {
                            r#type: "number",
                            class: if slider { "num" } else { "num wide" },
                            min: "{min}",
                            max: "{max}",
                            value: "{v}",
                            onchange: move |e| if let Ok(x) = e.value().parse::<i64>() { set(Value::Int(x.clamp(min, max))) },
                        }
                    }
                }
            }
        }
        ParamKind::Bool => {
            let v = p.b(key);
            rsx! {
                label { class: "field check",
                    input { r#type: "checkbox", checked: v, onchange: move |e| set(Value::Bool(e.checked())) }
                    "{label}"
                }
            }
        }
        ParamKind::Choice(options) => {
            let v = p.choice(key).to_string();
            rsx! {
                div { class: "field",
                    label { "{label}" }
                    select { onchange: move |e| set(Value::Choice(e.value())),
                        for (k , l) in options.iter() {
                            option { key: "{k}", value: "{k}", selected: v == *k, "{l}" }
                        }
                    }
                }
            }
        }
        ParamKind::Color => {
            let c = p.color(key);
            let h = hex(c);
            rsx! {
                div { class: "field",
                    label { "{label}" }
                    div { class: "slider-row",
                        input {
                            r#type: "color",
                            value: "{h}",
                            oninput: move |e| if let Some(nc) = parse_hex(&e.value(), c[3]) { set(Value::Color(nc)) },
                        }
                        input {
                            r#type: "range",
                            min: "0",
                            max: "1",
                            step: "0.01",
                            title: "opacity",
                            value: "{c[3]}",
                            oninput: move |e| if let Ok(a) = e.value().parse::<f32>() { set(Value::Color([c[0], c[1], c[2], a])) },
                        }
                    }
                }
            }
        }
        ParamKind::Text => {
            let v = p.text(key).to_string();
            rsx! {
                div { class: "field",
                    label { "{label}" }
                    input { r#type: "text", value: "{v}", onchange: move |e| set(Value::Text(e.value())) }
                }
            }
        }
        ParamKind::Hidden => rsx! {},
    }
}

#[component]
fn LayerStack(head: NodeId) -> Element {
    let app = use_context::<App>();
    let worker = use_context::<WorkerHandle>();
    let Some(chain) = app.stack_chain(head) else { return rsx! {} };
    let graph = app.graph.read();
    let rows: Vec<(usize, Option<Link>, String, bool)> = chain
        .layers
        .iter()
        .enumerate()
        .map(|(i, l)| match l.and_then(|l| graph.node(l.node)) {
            Some(n) if n.kind == ids::DISABLE => {
                let inner = n.inputs[0].map(|x| graph.link_title(x)).unwrap_or("(empty)".into());
                (i, *l, inner, false)
            }
            Some(_) => (i, *l, graph.link_title(l.unwrap()), true),
            None => (i, None, "(empty slot)".into(), true),
        })
        .collect();
    drop(graph);
    let count = rows.len();
    let accept = formats::accept_attr();
    let generators: Vec<&'static dyn nodes::NodeKind> =
        nodes::registry().iter().copied().filter(|k| k.category() == Category::Generator).collect();

    let set_layers = move |f: Box<dyn FnOnce(&mut Vec<Option<Link>>, &mut crate::graph::Graph)>| {
        app.edit(|g| {
            if let Some(chain) = g.stack_chain(head) {
                let mut layers = chain.layers.clone();
                f(&mut layers, g);
                g.set_stack_layers(chain.head, layers);
            }
        });
    };

    rsx! {
        div { class: "section",
            h4 { "Layers (front to back)" }
            div { class: "layers",
                for (i , layer , name , enabled) in rows {
                    div { key: "{i}", class: if enabled { "layer" } else { "layer disabled" },
                        button {
                            class: "icon-btn",
                            title: if enabled { "Disable layer" } else { "Enable layer" },
                            disabled: layer.is_none(),
                            onclick: move |_| {
                                set_layers(Box::new(move |layers, g| {
                                    let Some(l) = layers[i] else { return };
                                    let n = g.node(l.node).unwrap().clone();
                                    if n.kind == ids::DISABLE {
                                        layers[i] = n.inputs[0];
                                        g.remove_node(l.node);
                                    } else {
                                        let d = g.add_node(ids::DISABLE, (n.pos.0 + 100.0, n.pos.1 + 20.0));
                                        let _ = g.connect(l, d, 0);
                                        layers[i] = Some(d.into());
                                    }
                                }));
                            },
                            if enabled { "👁" } else { "◌" }
                        }
                        span {
                            class: "layer-name",
                            onclick: move |_| if let Some(l) = layer { app.clone().select(Some(l.node)) },
                            "{name}"
                        }
                        button {
                            class: "icon-btn",
                            title: "Move towards the front",
                            disabled: i == 0,
                            onclick: move |_| set_layers(Box::new(move |l, _| l.swap(i, i - 1))),
                            "▲"
                        }
                        button {
                            class: "icon-btn",
                            title: "Move towards the back",
                            disabled: i + 1 >= count,
                            onclick: move |_| set_layers(Box::new(move |l, _| l.swap(i, i + 1))),
                            "▼"
                        }
                        button {
                            class: "icon-btn",
                            title: "Remove from stack",
                            onclick: move |_| set_layers(Box::new(move |l, _| { l.remove(i); })),
                            "✕"
                        }
                    }
                }
            }
            div { class: "actions",
                label { class: "btn small",
                    input {
                        r#type: "file",
                        multiple: true,
                        accept: "{accept}",
                        style: "display:none",
                        onchange: move |e: FormEvent| {
                            let files = e.files().into_iter().map(ImportSource::File).collect();
                            session::import(app, worker.clone(), files, ImportTarget::StackLayer(head))
                        },
                    }
                    "+ Image layer"
                }
                select {
                    class: "small",
                    onchange: move |e| {
                        let kind = e.value();
                        if let Some(k) = nodes::kind(&kind) {
                            set_layers(Box::new(move |l, g| {
                                let p = g.node(head).map(|n| n.pos).unwrap_or_default();
                                let n = g.add_node(k.id(), (p.0 - 220.0, p.1 - 60.0));
                                l.insert(0, Some(n.into()));
                            }));
                        }
                    },
                    option { value: "", selected: true, "+ Generator layer…" }
                    for k in generators {
                        option { key: "{k.id()}", value: "{k.id()}", "{k.name()}" }
                    }
                }
                button { class: "btn small", onclick: move |_| set_layers(Box::new(|l, _| l.push(None))), "+ Empty slot" }
            }
        }
    }
}
