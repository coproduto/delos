use super::platform::{self, SaveTarget};
use super::session;
use super::state::{fmt_size, App, ExportState};
use super::WorkerHandle;
use crate::formats::{self, EncodeOptions};
use crate::graph::NodeId;
use crate::worker::{ExportDest, ExportJob};
use dioxus::prelude::*;

/// Asks how to reconcile two inputs of different sizes on a combining node,
/// and inserts a Resize node accordingly.
#[component]
pub fn ConflictDialog() -> Element {
    let mut app = use_context::<App>();
    let Some(c) = app.conflict.read().clone() else { return rsx! {} };
    let g = app.graph.read();
    let Some(node) = g.node(c.node) else { return rsx! {} };
    let ports = node.kind().inputs();
    let title = node.title();
    let name = |p: usize| {
        let src = node.inputs[p].and_then(|s| g.node(s)).map(|n| n.title()).unwrap_or_default();
        format!("{} ({src})", ports.get(p).copied().unwrap_or("input"))
    };
    let (na, nb) = (name(c.a.0), name(c.b.0));
    drop(g);
    let (sa, sb) = (c.a.1, c.b.1);
    let (c1, c2, c3) = (c.clone(), c.clone(), c.clone());
    rsx! {
        div { class: "modal-backdrop",
            div { class: "modal",
                h3 { "Different input sizes" }
                p {
                    b { "{title}" }
                    " combines "
                    b { "{na}" }
                    " at {fmt_size(sa)} with "
                    b { "{nb}" }
                    " at {fmt_size(sb)}. Which size should the result use?"
                }
                div { class: "choices",
                    button {
                        class: "btn primary",
                        onclick: move |_| {
                            let c = c1.clone();
                            app.edit(|g| { g.insert_resize(c.node, c.b.0, c.a.1); });
                            app.conflict.set(None);
                        },
                        "Use {fmt_size(sa)} (resize {nb})"
                    }
                    button {
                        class: "btn primary",
                        onclick: move |_| {
                            let c = c2.clone();
                            app.edit(|g| { g.insert_resize(c.node, c.a.0, c.b.1); });
                            app.conflict.set(None);
                        },
                        "Use {fmt_size(sb)} (resize {na})"
                    }
                    button { class: "btn", onclick: move |_| app.ignore_conflict(&c3), "Keep both sizes (align top-left)" }
                }
                p { class: "muted small", "A Resize node is inserted; you can change its size or interpolation later." }
            }
        }
    }
}

pub fn open_export(mut app: App, node: Option<NodeId>) {
    let d = app.displayed();
    if node.is_none() && d.empty {
        return;
    }
    let target = node.unwrap_or(d.export_target);
    let g = app.graph.peek();
    let Some(n) = g.node(target) else { return };
    let title = if target == g.output { "Output".to_string() } else { n.title() };
    let size = g.extent_of(target).pixels();
    drop(g);
    let stem: String = title.rsplit_once('.').map(|(s, _)| s.to_string()).unwrap_or(title.clone());
    let stem: String = stem.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    app.export.set(Some(ExportState {
        target,
        title,
        size,
        codec: "PNG".into(),
        quality: 90,
        name: format!("{stem}.png"),
        running: None,
        progress: 0.0,
        status: None,
        result: None,
    }));
}

static EXPORT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[component]
pub fn ExportDialog() -> Element {
    let mut app = use_context::<App>();
    let worker = use_context::<WorkerHandle>();
    let Some(st) = app.export.read().clone() else { return rsx! {} };
    let codec = formats::codec_by_name(&st.codec).unwrap_or(formats::encoders().next().unwrap());
    let lossy = st.codec == "JPEG";
    let running = st.running.is_some();
    let pct = (st.progress * 100.0).round();
    let megapixels = st.size.0 as f64 * st.size.1 as f64 / 1e6;
    let w_cancel = worker.clone();

    let start = move |_| {
        let Some(st) = app.export.peek().clone() else { return };
        let worker = worker.clone();
        spawn(async move {
            let codec = formats::codec_by_name(&st.codec).unwrap();
            let dest = match platform::choose_save_target(&st.name, codec.extensions(), codec.name()).await {
                #[cfg(feature = "native")]
                SaveTarget::Path(p) => ExportDest::File(p),
                SaveTarget::Memory => ExportDest::Memory,
                SaveTarget::Cancelled => return,
            };
            let id = EXPORT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if let Some(s) = app.export.write().as_mut() {
                s.running = Some(id);
                s.progress = 0.0;
                s.result = None;
                s.status = None;
            }
            let graph = app.graph.peek().clone();
            worker.0.export(ExportJob {
                id,
                graph,
                target: st.target,
                codec: st.codec.clone(),
                opts: EncodeOptions { quality: st.quality },
                dest,
                file_name: st.name.clone(),
            });
        });
    };

    rsx! {
        div {
            class: "modal-backdrop",
            onclick: move |_| if !running { app.export.set(None) },
            div { class: "modal",
                onclick: move |e| e.stop_propagation(),
                div { class: "modal-header",
                    h3 { "Export {st.title}" }
                    if !running {
                        button { class: "modal-close", title: "Close (Esc)", onclick: move |_| app.export.set(None), "×" }
                    }
                }
                p { class: "muted", "{st.size.0}×{st.size.1} px ({megapixels:.1} MP), rendered at full resolution in bands." }
                div { class: "field",
                    label { "Format" }
                    select {
                        disabled: running,
                        onchange: move |e| {
                            let v = e.value();
                            if let Some(s) = app.export.write().as_mut() {
                                if let Some(c) = formats::codec_by_name(&v) {
                                    let stem = s.name.rsplit_once('.').map(|(a, _)| a.to_string()).unwrap_or(s.name.clone());
                                    s.name = format!("{stem}.{}", c.extensions()[0]);
                                }
                                s.codec = v;
                            }
                        },
                        for c in formats::encoders() {
                            option { key: "{c.name()}", value: "{c.name()}", selected: c.name() == codec.name(), "{c.name()}" }
                        }
                    }
                }
                div { class: "field",
                    label { "File name" }
                    input {
                        r#type: "text",
                        value: "{st.name}",
                        disabled: running,
                        onchange: move |e| if let Some(s) = app.export.write().as_mut() { s.name = e.value() },
                    }
                }
                if lossy {
                    div { class: "field",
                        label { "Quality {st.quality}" }
                        input {
                            r#type: "range",
                            min: "1",
                            max: "100",
                            value: "{st.quality}",
                            disabled: running,
                            oninput: move |e| if let (Some(s), Ok(q)) = (app.export.write().as_mut(), e.value().parse()) { s.quality = q },
                        }
                    }
                }
                if running {
                    div { class: "progress", div { class: "bar", style: "width:{pct}%" } }
                    p { class: "muted small", {st.status.clone().unwrap_or_else(|| format!("Rendering… {pct}%"))} }
                }
                if let Some(r) = &st.result {
                    match r {
                        Ok(m) => rsx! { p { class: "ok", "{m}" } },
                        Err(e) => rsx! { p { class: "error-box inline", "{e}" } },
                    }
                }
                div { class: "choices horizontal",
                    if running {
                        button {
                            class: "btn danger",
                            onclick: move |_| {
                                let running = app.export.peek().as_ref().and_then(|s| s.running);
                                if let Some(id) = running {
                                    w_cancel.0.cancel_export(id);
                                }
                            },
                            "Cancel export"
                        }
                    } else {
                        button { class: "btn primary", onclick: start, "Export…" }
                        button { class: "btn", onclick: move |_| app.export.set(None), "Close" }
                    }
                }
            }
        }
    }
}

/// "Start a project?" (on the first import) or "New project".
#[component]
pub fn ProjectDialog() -> Element {
    #[cfg_attr(not(feature = "native"), allow(unused_mut))]
    let mut app = use_context::<App>();
    let worker = use_context::<WorkerHandle>();
    let Some(prompt) = app.project_prompt.read().clone() else { return rsx! {} };
    let busy = app.project_busy.read().is_some();
    let from_import = !prompt.pending.is_empty();
    let n = prompt.pending.len();
    let dir = prompt.parent.join(prompt.name.trim());
    let w_quick = worker.clone();
    let w_create = worker.clone();
    let count = if n == 1 { "this image".to_string() } else { format!("these {n} images") };

    rsx! {
        div { class: "modal-backdrop",
            div { class: "modal project-modal",
                div { class: "modal-header",
                    h3 { if from_import { "Start a project?" } else { "New project" } }
                    if !busy {
                        button {
                            class: "modal-close",
                            title: "Close (Esc)",
                            onclick: move |_| session::cancel_prompt(app),
                            "×"
                        }
                    }
                }
                p {
                    "A project keeps your work in a folder you can reopen and share. "
                    "Every image you import is "
                    b { "copied" }
                    " into the project's "
                    code { "inputs/" }
                    " folder, so projects take more disk space than the original files."
                }
                if from_import {
                    p { class: "muted", b { "Quick mode" } " opens {count} for one-off editing without saving or copying anything." }
                } else if app.graph.peek().images.len() > 0 {
                    p { class: "muted", "The images already in use are copied into the new project." }
                }
                div { class: "field",
                    label { "Project name" }
                    input {
                        r#type: "text",
                        value: "{prompt.name}",
                        disabled: busy,
                        oninput: move |e| session::set_prompt_name(app, &e.value()),
                    }
                }
                div { class: "field",
                    label { "Location" }
                    div { class: "path-row",
                        code { class: "path", title: "{dir.display()}", "{dir.display()}" }
                        if platform::HAS_PICKERS {
                            button {
                                class: "btn small",
                                disabled: busy,
                                onclick: move |_| {
                                    #[cfg(feature = "native")]
                                    spawn(async move {
                                        if let Some(parent) = platform::pick_folder("Choose where to create the project").await {
                                            if let Some(p) = app.project_prompt.write().as_mut() {
                                                p.parent = parent;
                                                p.error = None;
                                            }
                                        }
                                    });
                                },
                                "Change…"
                            }
                        }
                    }
                }
                if let Some(e) = &prompt.error {
                    p { class: "error-box inline", "{e}" }
                }
                div { class: "choices horizontal",
                    if from_import {
                        button {
                            class: "btn",
                            disabled: busy,
                            title: "Edit without creating a project",
                            onclick: move |_| session::choose_quick(app, w_quick.clone()),
                            "Quick mode"
                        }
                    } else {
                        button { class: "btn", disabled: busy, onclick: move |_| session::cancel_prompt(app), "Cancel" }
                    }
                    button {
                        class: "btn primary",
                        disabled: busy || prompt.name.trim().is_empty(),
                        onclick: move |_| {
                            #[cfg(feature = "native")]
                            session::create_project(app, w_create.clone());
                            #[cfg(not(feature = "native"))]
                            let _ = &w_create;
                        },
                        if busy { "Creating…" } else { "Create project" }
                    }
                }
            }
        }
    }
}
