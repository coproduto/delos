//! Dioxus user interface (desktop, web and mobile share all of it).

mod dialogs;
mod graph_view;
mod image_view;
mod inspector;
pub mod platform;
pub mod state;

use crate::formats;
use crate::nodes::{self, ids, Category, Value};
use crate::worker::{ExportOutput, Msg, Worker};
use dioxus::prelude::*;
use futures_util::StreamExt;
use state::{App, Busy, FrameView, ImportTarget, Layout};
use std::sync::Arc;

const CSS: &str = include_str!("../../assets/main.css");

/// Shared handle to the background worker.
#[derive(Clone)]
pub struct WorkerHandle(pub Arc<Worker>);

pub fn launch() {
    #[cfg(feature = "desktop")]
    {
        use dioxus::desktop::{Config, WindowBuilder};
        let cfg = Config::new()
            .with_window(WindowBuilder::new().with_title("Delos").with_inner_size(dioxus::desktop::LogicalSize::new(1440.0, 900.0)))
            .with_menu(None)
            .with_background_color((24, 25, 28, 255));
        dioxus::LaunchBuilder::new().with_cfg(cfg).launch(Root);
    }
    #[cfg(not(feature = "desktop"))]
    dioxus::launch(Root);
}

#[component]
fn Root() -> Element {
    let app = use_context_provider(App::new);
    let worker = use_hook(|| {
        let (w, rx) = Worker::new();
        let mut backend = app.backend;
        backend.set(w.backend_name());
        spawn(pump(app, rx));
        WorkerHandle(w)
    });
    use_context_provider(|| worker.clone());
    platform::use_frame_server();

    let layout = *app.layout.read();
    let main_class = match layout {
        Layout::Image => "main layout-image",
        Layout::Split => "main layout-split",
        Layout::Graph => "main layout-graph",
    };

    rsx! {
        style { {CSS} }
        div {
            class: "app",
            tabindex: "0",
            onkeydown: move |e| on_key(app, e),
            ondragover: move |e| e.prevent_default(),
            ondrop: {
                let worker = worker.clone();
                move |e: DragEvent| {
                    e.prevent_default();
                    let files = e.data_transfer().files();
                    start_import(app, worker.clone(), files, ImportTarget::Output);
                }
            },
            Toolbar {}
            div { class: "{main_class}",
                div { class: "views",
                    if layout != Layout::Graph {
                        div { class: "pane image-pane", image_view::ImageView {} }
                    }
                    if layout != Layout::Image {
                        div { class: "pane graph-pane", graph_view::GraphView {} }
                    }
                }
                inspector::Inspector {}
            }
            dialogs::ConflictDialog {}
            dialogs::ExportDialog {}
            Toast {}
        }
    }
}

/// Applies worker messages to the UI state.
async fn pump(mut app: App, mut rx: futures_channel::mpsc::UnboundedReceiver<Msg>) {
    while let Some(msg) = rx.next().await {
        let latest = *app.seq.peek();
        match msg {
            Msg::Frame(f) => {
                let accept = match &*app.frame.peek() {
                    Some(cur) => f.seq > cur.seq || (f.seq == cur.seq && cur.provisional && !f.provisional),
                    None => true,
                };
                if accept {
                    let url = platform::frame_url(f.seq, f.provisional, f.bmp.clone());
                    app.frame.set(Some(FrameView {
                        seq: f.seq,
                        url,
                        region: f.region,
                        extent: f.extent,
                        width: f.width,
                        height: f.height,
                        millis: f.millis,
                        provisional: f.provisional,
                    }));
                }
                if !f.provisional && f.seq == latest {
                    app.busy.set(None);
                    if !app.errors.peek().is_empty() {
                        app.errors.set(Default::default());
                    }
                    if app.global_error.peek().is_some() {
                        app.global_error.set(None);
                    }
                }
            }
            Msg::RenderError { seq, node, message } => {
                if seq == latest {
                    app.busy.set(None);
                    match node {
                        Some(n) => {
                            let mut m = std::collections::HashMap::new();
                            m.insert(n, message);
                            app.errors.set(m);
                        }
                        None => app.global_error.set(Some(message)),
                    }
                }
            }
            Msg::Status { seq, text } => {
                if seq == latest && app.busy.peek().is_some() {
                    app.busy.set(Some(Busy { seq, text }));
                }
            }
            Msg::Imported { name, result, token } => {
                *app.importing.write() -= 1;
                let target = app.imports.write().remove(&token).unwrap_or(ImportTarget::Output);
                match result {
                    Ok(img) => finish_import(app, img, target),
                    Err(e) => app.toast(format!("Could not open {name}: {e}"), true),
                }
            }
            Msg::ExportProgress { id, fraction, status } => {
                let mut ex = app.export.write();
                if let Some(s) = ex.as_mut().filter(|s| s.running == Some(id)) {
                    if fraction >= 0.0 {
                        s.progress = fraction;
                    }
                    if fraction < 0.0 {
                        s.status = status;
                    }
                }
            }
            Msg::ExportDone { id, result } => {
                let result = match result {
                    Ok(ExportOutput::Saved(path)) => Ok(format!("Saved to {path}")),
                    Ok(ExportOutput::Bytes { name, mime, bytes }) => platform::deliver_bytes(&name, &mime, &bytes),
                    Err(e) => Err(e),
                };
                // A successful export closes the dialog (the toast reports where
                // it went); a failure keeps it open with the error.
                let mine = app.export.peek().as_ref().is_some_and(|s| s.running == Some(id));
                if mine {
                    if result.is_ok() {
                        app.export.set(None);
                    } else if let Some(s) = app.export.write().as_mut() {
                        s.running = None;
                        s.result = Some(result.clone());
                    }
                }
                if let Ok(msg) = result {
                    app.toast(msg, false);
                }
            }
        }
    }
}

static IMPORT_TOKEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub fn start_import(mut app: App, worker: WorkerHandle, files: Vec<dioxus::html::FileData>, target: ImportTarget) {
    for f in files {
        let worker = worker.clone();
        *app.importing.write() += 1;
        spawn(async move {
            let name = f.name();
            match f.read_bytes().await {
                Ok(bytes) => {
                    let token = IMPORT_TOKEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    app.imports.write().insert(token, target);
                    worker.0.import(name, bytes.to_vec(), token);
                }
                Err(e) => {
                    *app.importing.write() -= 1;
                    app.toast(format!("Could not read {name}: {e}"), true);
                }
            }
        });
    }
}

fn finish_import(mut app: App, img: Arc<crate::engine::source::SourceImage>, target: ImportTarget) {
    let (w, h) = (img.width, img.height);
    let name = img.name.clone();
    let mut new_id = 0;
    app.edit(|g| {
        let iid = g.add_image(img);
        let count = g.nodes.len() as f64;
        let n = g.add_node(ids::IMAGE, (60.0, 40.0 + 90.0 * (count % 6.0)));
        new_id = n;
        g.set_param(n, nodes::IMAGE_PARAM, Value::Int(iid as i64));
        g.set_param(n, "width", Value::Int(w as i64));
        g.set_param(n, "height", Value::Int(h as i64));
        g.node_mut(n).unwrap().label = Some(name.clone());
        match target {
            ImportTarget::Output => {
                let out = g.output;
                let _ = g.connect(n, out, 0);
            }
            ImportTarget::StackLayer(head) => {
                if let Some(chain) = g.stack_chain(head) {
                    let mut layers = chain.layers.clone();
                    layers.insert(0, Some(n));
                    g.set_stack_layers(chain.head, layers);
                }
            }
        }
    });
    if target == ImportTarget::Output {
        app.pinned.set(None);
        app.select(None);
        app.view.write().fit = true;
    }
    app.toast(format!("Imported {name} ({w}×{h})"), false);
}

fn on_key(app: App, e: KeyboardEvent) {
    let m = e.modifiers();
    let cmd = m.contains(Modifiers::META) || m.contains(Modifiers::CONTROL);
    match e.key() {
        Key::Character(c) if cmd && (c == "z" || c == "Z") => {
            if m.contains(Modifiers::SHIFT) { app.redo() } else { app.undo() }
        }
        Key::Character(c) if cmd && c == "y" => app.redo(),
        Key::Character(c) if cmd && c == "e" => dialogs::open_export(app, None),
        Key::Delete | Key::Backspace => app.delete_selection(),
        Key::Escape => {
            let closable = app.export.peek().as_ref().map(|s| s.running.is_none());
            match closable {
                Some(true) => app.export.clone().set(None),
                Some(false) => {}
                None => app.select(None),
            }
        }
        Key::Character(c) if !cmd && c == "f" => app.view.clone().write().fit = true,
        _ => {}
    }
}

#[component]
fn Toolbar() -> Element {
    let mut app = use_context::<App>();
    let worker = use_context::<WorkerHandle>();
    let mut menu_open = use_signal(|| false);
    let layout = *app.layout.read();
    let follow = *app.follow_selection.read();
    let disp = app.displayed();
    let pinned = *app.pinned.read();
    let pinned_title = pinned.and_then(|p| app.graph.read().node(p).map(|n| n.title()));
    let importing = *app.importing.read();
    let backend = app.backend.read().clone();
    let has_gpu = worker.0.engine.has_gpu();
    let accept = formats::accept_attr();

    let mut cats: Vec<(Category, Vec<&'static dyn nodes::NodeKind>)> = Vec::new();
    for k in nodes::registry().iter().copied().filter(|k| k.user_creatable()) {
        match cats.iter_mut().find(|(c, _)| *c == k.category()) {
            Some((_, v)) => v.push(k),
            None => cats.push((k.category(), vec![k])),
        }
    }
    cats.sort_by_key(|(c, _)| *c);

    let lay_btn = |l: Layout, label: &'static str| {
        let cls = if layout == l { "seg active" } else { "seg" };
        rsx! {
            button { class: "{cls}", onclick: move |_| app.layout.set(l), "{label}" }
        }
    };

    rsx! {
        div { class: "toolbar",
            div { class: "brand", "Delos" }
            label { class: "btn primary",
                title: "Open an image as a Static Image node connected to the output",
                input {
                    r#type: "file",
                    multiple: true,
                    accept: "{accept}",
                    style: "display:none",
                    onchange: {
                        let worker = worker.clone();
                        move |e: FormEvent| start_import(app, worker.clone(), e.files(), ImportTarget::Output)
                    },
                }
                "Import"
            }
            div { class: "menu-wrap",
                button { class: "btn", onclick: move |_| menu_open.toggle(), "Add node ▾" }
                if menu_open() {
                    div { class: "menu-backdrop", onclick: move |_| menu_open.set(false) }
                    div { class: "menu",
                        for (cat , kinds) in cats {
                            div { class: "menu-cat", "{cat.label()}" }
                            for k in kinds {
                                button {
                                    class: "menu-item",
                                    title: "{k.description()}",
                                    onclick: move |_| {
                                        menu_open.set(false);
                                        app.add_node(k.id());
                                    },
                                    "{k.name()}"
                                }
                            }
                        }
                        div { class: "menu-hint", "New nodes are inserted after the selected node." }
                    }
                }
            }
            button {
                class: "btn",
                title: "Export the image currently shown (⌘E)",
                disabled: disp.empty,
                onclick: move |_| dialogs::open_export(app, None),
                "Export"
            }
            div { class: "sep" }
            button { class: "btn icon", title: "Undo (⌘Z)", disabled: !app.can_undo(), onclick: move |_| app.undo(), "↶" }
            button { class: "btn icon", title: "Redo (⇧⌘Z)", disabled: !app.can_redo(), onclick: move |_| app.redo(), "↷" }
            div { class: "sep" }
            div { class: "segmented",
                {lay_btn(Layout::Image, "Image")}
                {lay_btn(Layout::Split, "Split")}
                {lay_btn(Layout::Graph, "Graph")}
            }
            label {
                class: if pinned.is_some() { "check disabled" } else { "check" },
                title: if pinned.is_some() {
                    "The view stays on the relocated output; selected nodes are edited over it"
                } else {
                    "Show the selected node's output in the image view. Off: keep showing the output and edit the selected node over it"
                },
                input {
                    r#type: "checkbox",
                    checked: follow && pinned.is_none(),
                    disabled: pinned.is_some(),
                    onchange: move |e| app.follow_selection.set(e.checked()),
                }
                "Preview selection"
            }
            if let Some(t) = pinned_title {
                div { class: "pin-badge",
                    "Output relocated to "
                    b { "{t}" }
                    button { class: "link", onclick: move |_| app.pinned.set(None), "Reset" }
                }
            }
            div { class: "spacer" }
            if importing > 0 {
                div { class: "importing", span { class: "spinner small" } "Importing…" }
            }
            button {
                class: "badge",
                title: if has_gpu { "Click to switch between GPU and CPU rendering" } else { "No compatible GPU found; rendering on the CPU" },
                onclick: move |_| {
                    if has_gpu {
                        let e = &worker.0.engine;
                        let on = !e.use_gpu.load(std::sync::atomic::Ordering::Relaxed);
                        e.use_gpu.store(on, std::sync::atomic::Ordering::Relaxed);
                        app.backend.set(e.backend_name());
                        // Re-render with the new backend.
                        app.graph.write();
                    }
                },
                "{backend}"
            }
        }
    }
}

#[component]
fn Toast() -> Element {
    let mut app = use_context::<App>();
    let toast = app.toast.read().clone();
    let Some((msg, err)) = toast else { return rsx! {} };
    let cls = if err { "toast error" } else { "toast" };
    let key = format!("{}{}", msg.len(), msg.chars().take(12).collect::<String>());
    rsx! {
        div { key: "{key}", class: "{cls}", onclick: move |_| app.toast.set(None), "{msg}" }
    }
}
