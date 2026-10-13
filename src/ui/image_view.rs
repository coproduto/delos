//! The image view: shows the displayed node at the current zoom/pan by
//! requesting exactly the visible region at screen resolution, and draws the
//! WYSIWYG controls of the node being edited.

use super::platform;
use super::state::{fmt_size, App, Busy, ViewState};
use super::WorkerHandle;
use crate::engine::geom::{Map1, Map2, Rect, Request, Size};
use crate::graph::NodeId;
use crate::nodes::{Overlay, Value};
use crate::worker::PreviewJob;
use dioxus::prelude::*;
use std::rc::Rc;

/// Upper bound on preview pixels (keeps 5K/8K displays responsive).
const MAX_PREVIEW_PIXELS: f64 = 10.0e6;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    None,
    Pan { x: f64, y: f64, cx: f64, cy: f64 },
    Axis { node: NodeId, key: &'static str, horizontal: bool },
    Point { node: NodeId, kx: &'static str, ky: &'static str },
    /// `fx`/`fy`: where the drag started, in fractions of the overlay space.
    RectMove { node: NodeId, keys: [&'static str; 4], fx: f64, fy: f64, start: [f64; 4] },
    RectCorner { node: NodeId, keys: [&'static str; 4], corner: u8, start: [f64; 4] },
    Canvas,
}

#[component]
pub fn ImageView() -> Element {
    let mut app = use_context::<App>();
    let worker = use_context::<WorkerHandle>();
    let mut mounted = use_signal(|| None::<Rc<MountedData>>);
    let mut origin = use_signal(|| (0.0f64, 0.0f64));
    let mut dpr = use_signal(|| 1.0f64);
    let mut drag = use_signal(|| Drag::None);
    // Viewport coordinates → overlay fractions, frozen when a control drag
    // starts: the mapping (and the fitted zoom) can depend on the parameter
    // being dragged.
    let frac_map = use_signal(|| Map2::IDENTITY);
    let disp = use_memo(move || app.displayed());

    use_hook(|| {
        spawn(async move {
            dpr.set(platform::device_pixel_ratio().await);
        })
    });

    // Keep the image fitted while in fit mode.
    use_effect(move || {
        let d = disp();
        let (vw, vh) = *app.viewport.read();
        let v = *app.view.read();
        // Hold the fit while a control is dragged so it stays under the cursor.
        let dragging_control = !matches!(*drag.read(), Drag::None | Drag::Pan { .. });
        if !v.fit || vw < 2.0 || vh < 2.0 || dragging_control {
            return;
        }
        let e = d.extent;
        // Leave room for the status bar (28px) at the bottom.
        let zoom = ((vw - 32.0) / e.w).min((vh - 60.0) / e.h).max(1e-4);
        let next = ViewState { zoom, cx: e.w / 2.0, cy: e.h / 2.0 + 14.0 / zoom, fit: true };
        if next != v {
            app.view.set(next);
        }
    });

    // Size-less output: default the canvas to the window size.
    use_effect(move || {
        let d = disp();
        let (vw, _) = *app.viewport.read();
        if d.sizeless && vw > 16.0 {
            app.fit_canvas_to_window(false);
        }
    });

    // Request a render whenever anything that affects the picture changes.
    // Edits that do not change the shown image (moving nodes, editing other
    // branches) are filtered out by the structural hash.
    let last_key = use_hook(|| Rc::new(std::cell::Cell::new(0u64)));
    let w2 = worker.clone();
    use_effect(move || {
        let graph = app.graph.read().clone();
        let backend = app.backend.read().clone();
        let d = disp();
        let v = *app.view.read();
        let (vw, vh) = *app.viewport.read();
        let dpr = dpr();
        if vw < 2.0 || vh < 2.0 || d.empty {
            return;
        }
        let e = d.extent;
        let visible = Rect::new(v.cx - vw / 2.0 / v.zoom, v.cy - vh / 2.0 / v.zoom, vw / v.zoom, vh / v.zoom);
        let region = visible.intersect(&e.rect());
        if region.is_empty() {
            return;
        }
        let mut pw = region.w * v.zoom * dpr;
        let mut ph = region.h * v.zoom * dpr;
        let k = (MAX_PREVIEW_PIXELS / (pw * ph)).sqrt();
        if k < 1.0 {
            pw *= k;
            ph *= k;
        }
        let req = Request::new(e, region, pw.round().max(1.0) as u32, ph.round().max(1.0) as u32);
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            graph.link_hash(d.target, &mut Default::default()).hash(&mut h);
            (d.target, req.width, req.height, backend).hash(&mut h);
            for f in [e.w, e.h, region.x, region.y, region.w, region.h] {
                f.to_bits().hash(&mut h);
            }
            h.finish()
        };
        if key == last_key.get() {
            return;
        }
        last_key.set(key);
        let seq = *app.seq.peek() + 1;
        app.seq.set(seq);
        app.busy.set(Some(Busy { seq, text: None }));
        w2.0.preview(PreviewJob { seq, graph, target: d.target, req });
    });

    let refresh_origin = move || async move {
        let m = mounted.peek().clone();
        if let Some(m) = m {
            if let Ok(r) = m.get_client_rect().await {
                origin.set((r.origin.x, r.origin.y));
            }
        }
    };

    let d = disp();
    let v = *app.view.read();
    let (vw, vh) = *app.viewport.read();
    let frame = app.frame.read().clone();
    let busy = app.busy.read().clone();
    let errors = app.errors.read().clone();
    let global_error = app.global_error.read().clone();
    let graph = app.graph.read();

    let sx = move |ix: f64| (ix - v.cx) * v.zoom + vw / 2.0;
    let sy = move |iy: f64| (iy - v.cy) * v.zoom + vh / 2.0;
    let e = d.extent;
    let (ex0, ey0, ew, eh) = (sx(0.0), sy(0.0), e.w * v.zoom, e.h * v.zoom);

    let to_image = move |cx: f64, cy: f64| {
        let (ox, oy) = origin();
        let v = *app.view.peek();
        let (vw, vh) = *app.viewport.peek();
        ((cx - ox - vw / 2.0) / v.zoom + v.cx, (cy - oy - vh / 2.0) / v.zoom + v.cy)
    };

    // Overlays of the node being edited, placed through the mapping from the
    // shown node's space into the editing node's space.
    let overlays: Vec<Overlay> =
        if d.overlay_space.is_some() { d.editing.and_then(|n| graph.node(n)).map(|n| n.kind().overlays(n)).unwrap_or_default() } else { Vec::new() };
    let (space, oe) = d.overlay_space.unwrap_or((Map2::IDENTITY, e));
    let to_frac = Map2::new(1.0 / oe.w, 0.0, 1.0 / oe.h, 0.0).after(&space);
    // Overlay fractions → viewport coordinates.
    let to_screen = Map2 { x: Map1 { a: v.zoom, b: vw / 2.0 - v.cx * v.zoom }, y: Map1 { a: v.zoom, b: vh / 2.0 - v.cy * v.zoom } };
    let frac_to_screen = to_screen.after(&to_frac.inverse());
    let editing = d.editing;
    let params = d.editing.and_then(|n| graph.node(n)).map(|n| n.params.clone()).unwrap_or_default();
    let error_line = errors.iter().next().map(|(n, m)| (graph.node(*n).map(|n| n.title()).unwrap_or_default(), m.clone())).or(global_error.map(|m| (String::new(), m)));
    let editing_title = editing.and_then(|n| graph.node(n)).map(|n| n.title());
    let target_title = if d.target.node == graph.output { "the output".to_string() } else { graph.link_title(d.target) };
    let badge = editing_title.map(|t| match (d.locked, d.overlay_space.is_some()) {
        (true, true) => format!("Editing {t} · showing {target_title}"),
        (true, false) => format!("{t} does not feed {target_title} · double-click it in the graph to view it"),
        _ if editing != Some(d.target.node) => format!("Editing {t} · showing its input"),
        _ => format!("Previewing {t}"),
    });
    drop(graph);

    let frame_el = frame.filter(|f| (f.extent.w - e.w).abs() < 0.5 && (f.extent.h - e.h).abs() < 0.5 && !d.empty).map(|f| {
        let (l, t) = (sx(f.region.x), sy(f.region.y));
        let (w, h) = (f.region.w * v.zoom, f.region.h * v.zoom);
        let pixelated = if f.width as f64 * 1.5 < w { "pixelated" } else { "auto" };
        rsx! {
            img {
                class: "frame",
                src: "{f.url}",
                draggable: false,
                style: "left:{l}px;top:{t}px;width:{w}px;height:{h}px;image-rendering:{pixelated};",
            }
        }
    });
    let info = app.frame.read().as_ref().map(|f| format!("{:.0} ms{}", f.millis, if f.provisional { " (preview)" } else { "" }));

    rsx! {
        div {
            class: "viewport",
            onmounted: move |e| async move {
                mounted.set(Some(e.data()));
                refresh_origin().await;
            },
            onresize: move |e| async move {
                if let Ok(s) = e.get_content_box_size() {
                    app.viewport.set((s.width, s.height));
                }
                refresh_origin().await;
            },
            onwheel: move |e: WheelEvent| {
                e.prevent_default();
                let dy = e.delta().strip_units().y;
                let factor = (-dy * 0.0015).exp().clamp(0.5, 2.0);
                let c = e.client_coordinates();
                let (ix, iy) = to_image(c.x, c.y);
                let mut v = app.view.write();
                let z = (v.zoom * factor).clamp(1e-4, 64.0);
                let (ox, oy) = origin();
                let (vw, vh) = *app.viewport.peek();
                v.cx = ix - (c.x - ox - vw / 2.0) / z;
                v.cy = iy - (c.y - oy - vh / 2.0) / z;
                v.zoom = z;
                v.fit = false;
            },
            onpointerdown: move |e: PointerEvent| {
                let c = e.client_coordinates();
                let v = *app.view.peek();
                drag.set(Drag::Pan { x: c.x, y: c.y, cx: v.cx, cy: v.cy });
            },
            onpointermove: move |e: PointerEvent| {
                let dr = *drag.peek();
                if dr == Drag::None {
                    return;
                }
                let c = e.client_coordinates();
                let (ix, iy) = to_image(c.x, c.y);
                let m = *frac_map.peek();
                let (ox, oy) = origin();
                let (fx, fy) = (m.x.apply(c.x - ox), m.y.apply(c.y - oy));
                match dr {
                    Drag::None => {}
                    Drag::Pan { x, y, cx, cy } => {
                        let mut v = app.view.write();
                        v.cx = cx - (c.x - x) / v.zoom;
                        v.cy = cy - (c.y - y) / v.zoom;
                        v.fit = false;
                    }
                    Drag::Axis { node, key, horizontal } => {
                        let f = if horizontal { fy } else { fx };
                        app.set_param(node, key, Value::Float(f.clamp(0.0, 1.0)));
                    }
                    Drag::Point { node, kx, ky } => {
                        app.set_param(node, kx, Value::Float(fx.clamp(-1.0, 2.0)));
                        app.set_param(node, ky, Value::Float(fy.clamp(-1.0, 2.0)));
                    }
                    Drag::RectMove { node, keys, fx: x0, fy: y0, start } => {
                        if x0.is_nan() {
                            drag.set(Drag::RectMove { node, keys, fx, fy, start });
                            return;
                        }
                        let nx = (start[0] + fx - x0).clamp(0.0, 1.0 - start[2]);
                        let ny = (start[1] + fy - y0).clamp(0.0, 1.0 - start[3]);
                        app.edit_coalesced(&format!("{node}:rect"), |g| {
                            g.set_param(node, keys[0], Value::Float(nx));
                            g.set_param(node, keys[1], Value::Float(ny));
                        });
                    }
                    Drag::RectCorner { node, keys, corner, start } => {
                        let (mut x0, mut y0, mut x1, mut y1) = (start[0], start[1], start[0] + start[2], start[1] + start[3]);
                        let (fx, fy) = (fx.clamp(0.0, 1.0), fy.clamp(0.0, 1.0));
                        if corner & 1 == 0 { x0 = fx.min(x1 - 0.001) } else { x1 = fx.max(x0 + 0.001) }
                        if corner & 2 == 0 { y0 = fy.min(y1 - 0.001) } else { y1 = fy.max(y0 + 0.001) }
                        app.edit_coalesced(&format!("{node}:rect"), |g| {
                            g.set_param(node, keys[0], Value::Float(x0));
                            g.set_param(node, keys[1], Value::Float(y0));
                            g.set_param(node, keys[2], Value::Float(x1 - x0));
                            g.set_param(node, keys[3], Value::Float(y1 - y0));
                        });
                    }
                    Drag::Canvas => {
                        let out = app.graph.peek().output;
                        let (w, h) = (ix.round().max(16.0) as i64, iy.round().max(16.0) as i64);
                        app.canvas_user_set.set(true);
                        app.view.write().fit = false;
                        app.edit_coalesced("canvas", |g| {
                            g.set_param(out, "canvas_w", Value::Int(w));
                            g.set_param(out, "canvas_h", Value::Int(h));
                        });
                    }
                }
            },
            onpointerup: move |_| drag.set(Drag::None),
            onpointerleave: move |_| drag.set(Drag::None),
            ondoubleclick: move |_| app.view.write().fit = true,

            if d.empty {
                div { class: "empty-state",
                    h2 { "Nothing connected to the output yet" }
                    p { "Import an image (or drop files here), or add a generator from " b { "Add node" } "." }
                }
            } else {
                div { class: "canvas-bounds", style: "left:{ex0}px;top:{ey0}px;width:{ew}px;height:{eh}px;" }
                {frame_el}
                svg { class: "overlay", width: "{vw}", height: "{vh}",
                    for (i , o) in overlays.iter().enumerate() {
                        {render_overlay(i, o, &params, editing.unwrap(), oe, frac_to_screen, drag, frac_map)}
                    }
                    if d.sizeless {
                        g {
                            rect {
                                class: "handle canvas-handle",
                                x: "{ex0 + ew - 7.0}",
                                y: "{ey0 + eh - 7.0}",
                                width: "14",
                                height: "14",
                                onpointerdown: move |ev: PointerEvent| {
                                    ev.stop_propagation();
                                    drag.set(Drag::Canvas);
                                },
                            }
                            text { class: "overlay-label", x: "{ex0 + ew - 4.0}", y: "{ey0 + eh + 18.0}", text_anchor: "end",
                                "Canvas {fmt_size(e)} · drag to resize"
                            }
                        }
                    }
                }
            }
            if let Some(b) = badge {
                div { class: "edit-badge", "{b}" }
            }
            if let Some((node, msg)) = error_line {
                div { class: "error-box",
                    if !node.is_empty() { b { "{node}: " } }
                    "{msg}"
                }
            }
            if let Some(b) = busy {
                div { class: "busy",
                    span { class: "spinner" }
                    span { {b.text.unwrap_or_else(|| "Rendering…".into())} }
                }
            }
            div { class: "statusbar",
                onpointerdown: move |e| e.stop_propagation(),
                span { "{fmt_size(e)}" }
                span { "{(v.zoom * 100.0):.1}%" }
                if let Some(i) = info { span { class: "muted", "{i}" } }
                div { class: "spacer" }
                button { class: "btn small", onclick: move |_| app.view.write().fit = true, "Fit" }
                button {
                    class: "btn small",
                    onclick: move |_| {
                        let mut v = app.view.write();
                        v.zoom = 1.0 / dpr();
                        v.fit = false;
                    },
                    "1:1"
                }
                button { class: "btn small", onclick: move |_| zoom_by(app, 1.25), "+" }
                button { class: "btn small", onclick: move |_| zoom_by(app, 0.8), "−" }
            }
        }
    }
}

fn zoom_by(mut app: App, f: f64) {
    let mut v = app.view.write();
    v.zoom = (v.zoom * f).clamp(1e-4, 64.0);
    v.fit = false;
}

#[allow(clippy::too_many_arguments)]
fn render_overlay(
    i: usize,
    o: &Overlay,
    p: &crate::nodes::Params,
    node: NodeId,
    e: Size,
    frac_to_screen: Map2,
    mut drag: Signal<Drag>,
    mut frac_map: Signal<Map2>,
) -> Element {
    let to_frac = frac_to_screen.inverse();
    let sx = |f: f64| frac_to_screen.x.apply(f);
    let sy = |f: f64| frac_to_screen.y.apply(f);
    match o.clone() {
        Overlay::Axis { orientation, position } => {
            let horizontal = p.choice(orientation) == "horizontal";
            let pos = p.f(position);
            let (x1, y1, x2, y2) = if horizontal {
                (sx(0.0) - 20.0, sy(pos), sx(1.0) + 20.0, sy(pos))
            } else {
                (sx(pos), sy(0.0) - 20.0, sx(pos), sy(1.0) + 20.0)
            };
            let px = (pos * if horizontal { e.h } else { e.w }).round();
            rsx! {
                g { key: "{i}",
                    line { class: "axis-line", x1: "{x1}", y1: "{y1}", x2: "{x2}", y2: "{y2}" }
                    line {
                        class: if horizontal { "axis-hit h" } else { "axis-hit v" },
                        x1: "{x1}",
                        y1: "{y1}",
                        x2: "{x2}",
                        y2: "{y2}",
                        onpointerdown: move |ev: PointerEvent| {
                            ev.stop_propagation();
                            frac_map.set(to_frac);
                            drag.set(Drag::Axis { node, key: position, horizontal });
                        },
                    }
                    text { class: "overlay-label", x: "{x1 + 6.0}", y: "{y1 + 16.0}", "axis {px} px" }
                }
            }
        }
        Overlay::Line { position, horizontal, sides } => {
            let pos = p.f(position);
            let (x1, y1, x2, y2) = if horizontal {
                (sx(0.0) - 20.0, sy(pos), sx(1.0) + 20.0, sy(pos))
            } else {
                (sx(pos), sy(0.0) - 20.0, sx(pos), sy(1.0) + 20.0)
            };
            let px = (pos * if horizontal { e.h } else { e.w }).round();
            // Side names next to the line, near its start.
            let (before, after) = if horizontal {
                ((x1 + 26.0, y1 - 8.0, "start"), (x1 + 26.0, y1 + 18.0, "start"))
            } else {
                ((x1 - 8.0, y1 + 38.0, "end"), (x1 + 8.0, y1 + 38.0, "start"))
            };
            let (b_text, a_text) = if horizontal { (format!("▲ {}", sides[0]), format!("▼ {}", sides[1])) } else { (format!("◀ {}", sides[0]), format!("{} ▶", sides[1])) };
            rsx! {
                g { key: "{i}",
                    line { class: "axis-line split-line", x1: "{x1}", y1: "{y1}", x2: "{x2}", y2: "{y2}" }
                    line {
                        class: if horizontal { "axis-hit h" } else { "axis-hit v" },
                        x1: "{x1}",
                        y1: "{y1}",
                        x2: "{x2}",
                        y2: "{y2}",
                        onpointerdown: move |ev: PointerEvent| {
                            ev.stop_propagation();
                            frac_map.set(to_frac);
                            drag.set(Drag::Axis { node, key: position, horizontal });
                        },
                    }
                    text { class: "overlay-label", x: "{before.0}", y: "{before.1}", text_anchor: "{before.2}", "{b_text}" }
                    text { class: "overlay-label", x: "{after.0}", y: "{after.1}", text_anchor: "{after.2}", "{a_text}" }
                    text { class: "overlay-label muted-label", x: if horizontal { x2 - 26.0 } else { x1 + 8.0 }, y: if horizontal { y1 - 8.0 } else { y2 - 8.0 }, text_anchor: if horizontal { "end" } else { "start" }, "{px} px" }
                }
            }
        }
        Overlay::Point { x, y, label } => {
            let (cx, cy) = (sx(p.f(x)), sy(p.f(y)));
            rsx! {
                g { key: "{i}",
                    circle {
                        class: "handle point",
                        cx: "{cx}",
                        cy: "{cy}",
                        r: "8",
                        onpointerdown: move |ev: PointerEvent| {
                            ev.stop_propagation();
                            frac_map.set(to_frac);
                            drag.set(Drag::Point { node, kx: x, ky: y });
                        },
                    }
                    text { class: "overlay-label", x: "{cx + 12.0}", y: "{cy - 10.0}", "{label}" }
                }
            }
        }
        Overlay::Rect { x, y, w, h } => {
            let keys = [x, y, w, h];
            let start = [p.f(x), p.f(y), p.f(w), p.f(h)];
            let (l, t, r, b) = (sx(start[0]), sy(start[1]), sx(start[0] + start[2]), sy(start[1] + start[3]));
            let (ox0, oy0, ox1, oy1) = (sx(0.0), sy(0.0), sx(1.0), sy(1.0));
            let shade = format!("M{ox0},{oy0}H{ox1}V{oy1}H{ox0}Z M{l},{t}V{b}H{r}V{t}Z");
            let label = format!("{}×{} px", (start[2] * e.w).round(), (start[3] * e.h).round());
            let corners = [(0u8, l, t), (1, r, t), (2, l, b), (3, r, b)];
            rsx! {
                g { key: "{i}",
                    path { class: "shade", d: "{shade}", fill_rule: "evenodd" }
                    rect {
                        class: "clip-rect",
                        x: "{l}",
                        y: "{t}",
                        width: "{r - l}",
                        height: "{b - t}",
                        onpointerdown: move |ev: PointerEvent| {
                            ev.stop_propagation();
                            frac_map.set(to_frac);
                            drag.set(Drag::RectMove { node, keys, fx: f64::NAN, fy: f64::NAN, start });
                        },
                    }
                    for (c , cx , cy) in corners {
                        rect {
                            key: "{c}",
                            class: "handle",
                            x: "{cx - 6.0}",
                            y: "{cy - 6.0}",
                            width: "12",
                            height: "12",
                            onpointerdown: move |ev: PointerEvent| {
                                ev.stop_propagation();
                                frac_map.set(to_frac);
                                drag.set(Drag::RectCorner { node, keys, corner: c, start });
                            },
                        }
                    }
                    text { class: "overlay-label", x: "{l + 4.0}", y: "{t - 8.0}", "{label}" }
                }
            }
        }
    }
}
