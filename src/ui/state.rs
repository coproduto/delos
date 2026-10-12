//! Application state shared by all UI components (all fields are signals, so
//! the struct is `Copy` and lives in a Dioxus context).

use crate::engine::geom::{Map2, Rect, Size};
use crate::graph::{Graph, NodeId, SizeConflict, StackChain};
use crate::nodes::{self, ids, Category, EditView, Value};
use crate::worker::now_ms;
use dioxus::prelude::*;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    Image,
    Split,
    Graph,
}

/// Image view transform: `zoom` screen px per image px, (`cx`, `cy`) the
/// image point at the viewport center. `fit` keeps the image fitted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewState {
    pub zoom: f64,
    pub cx: f64,
    pub cy: f64,
    pub fit: bool,
}

impl Default for ViewState {
    fn default() -> Self {
        Self { zoom: 1.0, cx: 0.0, cy: 0.0, fit: true }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FrameView {
    pub seq: u64,
    pub url: String,
    pub region: Rect,
    pub extent: Size,
    pub width: u32,
    pub height: u32,
    pub millis: f64,
    pub provisional: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Busy {
    pub seq: u64,
    pub text: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExportState {
    pub target: NodeId,
    pub title: String,
    pub size: (u32, u32),
    pub codec: String,
    pub quality: u8,
    pub name: String,
    pub running: Option<u64>,
    pub progress: f32,
    pub status: Option<String>,
    pub result: Option<Result<String, String>>,
}

/// What a finished import should be attached to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImportTarget {
    /// Connected directly to the output (spec 1.1).
    Output,
    /// Added as the front layer of a layer stack.
    StackLayer(NodeId),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Displayed {
    /// Node whose image is shown.
    pub target: NodeId,
    /// Node being edited (its overlays are drawn), if any.
    pub editing: Option<NodeId>,
    /// Maps the shown node's space into the space the editing node's
    /// overlays live in, plus that space's extent. `None` when the editing
    /// node does not contribute to the shown image.
    pub overlay_space: Option<(Map2, Size)>,
    /// The view stays on the output (or relocated output) while another node
    /// is selected, whose controls are drawn over it.
    pub locked: bool,
    /// Node exported by "Export" (the editing node itself when its edit view
    /// shows an input).
    pub export_target: NodeId,
    pub extent: Size,
    /// The shown node has no size of its own (uses the output canvas).
    pub sizeless: bool,
    /// The shown node has nothing to show yet.
    pub empty: bool,
}

#[derive(Default, Clone)]
pub struct History {
    undo: Vec<Graph>,
    redo: Vec<Graph>,
    last_key: Option<String>,
    last_time: f64,
}

#[derive(Clone, Copy)]
pub struct App {
    pub graph: Signal<Graph>,
    pub history: Signal<History>,
    pub selected: Signal<Option<NodeId>>,
    pub selected_edge: Signal<Option<(NodeId, usize)>>,
    pub pinned: Signal<Option<NodeId>>,
    pub follow_selection: Signal<bool>,
    pub layout: Signal<Layout>,
    pub view: Signal<ViewState>,
    pub viewport: Signal<(f64, f64)>,
    pub frame: Signal<Option<FrameView>>,
    pub busy: Signal<Option<Busy>>,
    pub errors: Signal<HashMap<NodeId, String>>,
    pub global_error: Signal<Option<String>>,
    pub conflict: Signal<Option<SizeConflict>>,
    pub ignored_conflicts: Signal<HashSet<(NodeId, u64, u64)>>,
    pub export: Signal<Option<ExportState>>,
    pub toast: Signal<Option<(String, bool)>>,
    pub canvas_user_set: Signal<bool>,
    pub imports: Signal<HashMap<u64, ImportTarget>>,
    pub importing: Signal<u32>,
    pub seq: Signal<u64>,
    pub backend: Signal<String>,
    pub session: Signal<super::session::Session>,
    pub save_state: Signal<super::session::SaveState>,
    pub project_prompt: Signal<Option<super::session::ProjectPrompt>>,
    /// A project is being created or opened in the background.
    pub project_busy: Signal<Option<String>>,
    /// Where new projects (and imported `.delosproject` files) go.
    pub projects_root: Signal<std::path::PathBuf>,
}

const HISTORY_LIMIT: usize = 60;

impl App {
    pub fn new() -> Self {
        App {
            graph: Signal::new(Graph::new()),
            history: Signal::new(History::default()),
            selected: Signal::new(None),
            selected_edge: Signal::new(None),
            pinned: Signal::new(None),
            follow_selection: Signal::new(true),
            layout: Signal::new(Layout::Split),
            view: Signal::new(ViewState::default()),
            viewport: Signal::new((0.0, 0.0)),
            frame: Signal::new(None),
            busy: Signal::new(None),
            errors: Signal::new(HashMap::new()),
            global_error: Signal::new(None),
            conflict: Signal::new(None),
            ignored_conflicts: Signal::new(HashSet::new()),
            export: Signal::new(None),
            toast: Signal::new(None),
            canvas_user_set: Signal::new(false),
            imports: Signal::new(HashMap::new()),
            importing: Signal::new(0),
            seq: Signal::new(0),
            backend: Signal::new(String::new()),
            session: Signal::new(Default::default()),
            save_state: Signal::new(Default::default()),
            project_prompt: Signal::new(None),
            project_busy: Signal::new(None),
            projects_root: Signal::new(super::session::default_projects_root()),
        }
    }

    // ----- editing ---------------------------------------------------------

    fn push_history(mut self) {
        let snapshot = self.graph.peek().clone();
        let mut h = self.history.write();
        h.undo.push(snapshot);
        if h.undo.len() > HISTORY_LIMIT {
            h.undo.remove(0);
        }
        h.redo.clear();
    }

    /// A discrete edit (one undo step).
    pub fn edit(mut self, f: impl FnOnce(&mut Graph)) {
        self.push_history();
        self.history.write().last_key = None;
        f(&mut self.graph.write());
        self.after_edit();
    }

    /// A continuous edit (slider drag, handle drag): consecutive edits with
    /// the same key within a second share one undo step.
    pub fn edit_coalesced(mut self, key: &str, f: impl FnOnce(&mut Graph)) {
        let now = now_ms();
        let (same, recent) = {
            let h = self.history.peek();
            (h.last_key.as_deref() == Some(key), now - h.last_time < 1000.0)
        };
        if !(same && recent) {
            self.push_history();
        }
        {
            let mut h = self.history.write();
            h.last_key = Some(key.to_string());
            h.last_time = now;
        }
        f(&mut self.graph.write());
        self.after_edit();
    }

    pub fn set_param(self, node: NodeId, key: &'static str, v: Value) {
        self.edit_coalesced(&format!("{node}:{key}"), |g| g.set_param(node, key, v));
    }

    pub fn can_undo(&self) -> bool {
        !self.history.read().undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.history.read().redo.is_empty()
    }

    /// Every image held by the document or its undo history.
    pub fn all_images(&self) -> Vec<std::sync::Arc<crate::engine::source::SourceImage>> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        let g = self.graph.peek();
        let h = self.history.peek();
        for graph in std::iter::once(&*g).chain(h.undo.iter()).chain(h.redo.iter()) {
            for img in graph.images.values() {
                if seen.insert(std::sync::Arc::as_ptr(img)) {
                    out.push(img.clone());
                }
            }
        }
        out
    }

    /// Starts over with `graph` (opening or closing a project): clears the
    /// history, selection and per-document view state.
    pub fn replace_document(mut self, graph: Graph) {
        self.graph.set(graph);
        self.history.set(History::default());
        self.selected.set(None);
        self.selected_edge.set(None);
        self.pinned.set(None);
        self.errors.write().clear();
        self.global_error.set(None);
        self.conflict.set(None);
        self.ignored_conflicts.write().clear();
        self.canvas_user_set.set(false);
        self.view.write().fit = true;
        self.after_edit();
    }

    pub fn undo(mut self) {
        let prev = self.history.write().undo.pop();
        if let Some(prev) = prev {
            let cur = std::mem::replace(&mut *self.graph.write(), prev);
            let mut h = self.history.write();
            h.redo.push(cur);
            h.last_key = None;
            drop(h);
            self.fix_selection();
        }
    }

    pub fn redo(mut self) {
        let next = self.history.write().redo.pop();
        if let Some(next) = next {
            let cur = std::mem::replace(&mut *self.graph.write(), next);
            let mut h = self.history.write();
            h.undo.push(cur);
            h.last_key = None;
            drop(h);
            self.fix_selection();
        }
    }

    fn fix_selection(mut self) {
        let g = self.graph.peek();
        let sel = *self.selected.peek();
        let pin = *self.pinned.peek();
        let sel_ok = sel.is_none_or(|s| g.node(s).is_some());
        let pin_ok = pin.is_none_or(|p| g.node(p).is_some());
        drop(g);
        if !sel_ok {
            self.selected.set(None);
        }
        if !pin_ok {
            self.pinned.set(None);
        }
        self.selected_edge.set(None);
    }

    /// Post-edit housekeeping: detect size mismatches and size the canvas for
    /// generators.
    fn after_edit(mut self) {
        let g = self.graph.peek();
        let ignored = self.ignored_conflicts.peek();
        let conflict = g.nodes.keys().filter_map(|id| g.size_conflict(*id)).find(|c| !ignored.contains(&conflict_key(c)));
        drop(ignored);
        drop(g);
        if *self.conflict.peek() != conflict {
            self.conflict.set(conflict);
        }
    }

    pub fn ignore_conflict(mut self, c: &SizeConflict) {
        self.ignored_conflicts.write().insert(conflict_key(c));
        self.conflict.set(None);
        self.after_edit();
    }

    /// Sets the output canvas to fit the current image view (used when a
    /// size-less generator is first shown or connected to the output).
    pub fn fit_canvas_to_window(mut self, force: bool) {
        if *self.canvas_user_set.peek() && !force {
            return;
        }
        let (w, h) = *self.viewport.peek();
        if w < 16.0 || h < 16.0 {
            return;
        }
        let (w, h) = ((w * 0.92).round() as i64, (h * 0.92).round() as i64);
        let out = self.graph.peek().output;
        let cur = self.graph.peek().canvas();
        if cur.w as i64 == w && cur.h as i64 == h {
            return;
        }
        let mut g = self.graph.write();
        g.set_param(out, "canvas_w", Value::Int(w));
        g.set_param(out, "canvas_h", Value::Int(h));
        drop(g);
        if force {
            self.canvas_user_set.set(true);
        }
    }

    // ----- what is shown ---------------------------------------------------

    /// What the image view shows. Normally the selected node is previewed;
    /// with the output relocated (or "Preview selection" off) the view stays
    /// put and the selected node's controls are drawn over it instead.
    pub fn displayed(&self) -> Displayed {
        let g = self.graph.read();
        let pinned = self.pinned.read().filter(|p| g.node(*p).is_some());
        let base = pinned.unwrap_or(g.output);
        let sel = self.selected.read().filter(|s| g.node(*s).is_some() && *s != g.output);
        let locked = pinned.is_some() || !*self.follow_selection.read();
        let (target, editing, overlay_space) = match sel {
            Some(s) if !locked => {
                let n = g.node(s).unwrap();
                let target = match n.kind().edit_view() {
                    EditView::Output => s,
                    EditView::Input(p) => n.inputs.get(p).copied().flatten().unwrap_or(s),
                };
                (target, Some(s), Some((Map2::IDENTITY, g.extent_of(target))))
            }
            Some(s) => {
                let n = g.node(s).unwrap();
                let space = g.space_map(base, s).map(|(m, e)| match n.kind().edit_view() {
                    EditView::Output => (m, e),
                    EditView::Input(p) => {
                        let input = n.inputs.get(p).copied().flatten().and_then(|i| g.size_of(i));
                        let (im, ie) = n.kind().input_map(n, p, e, input);
                        (im.after(&m), ie)
                    }
                });
                (base, Some(s), space)
            }
            None => (base, None, None),
        };
        let export_target = if locked { target } else { editing.unwrap_or(target) };
        let size = g.size_of(target);
        let empty = {
            let n = g.node(target).unwrap();
            n.kind == ids::OUTPUT && n.inputs[0].is_none()
        };
        let sizeless = size.is_none() || (g.node(target).unwrap().kind == ids::OUTPUT && g.node(target).unwrap().inputs[0].is_some_and(|i| g.size_of(i).is_none()));
        Displayed { target, editing, overlay_space, locked, export_target, extent: g.extent_of(target), sizeless, empty }
    }

    // ----- graph operations used by several components ----------------------

    pub fn select(mut self, id: Option<NodeId>) {
        self.selected.set(id);
        self.selected_edge.set(None);
    }

    /// Adds a node. When a node is selected, the new node is inserted after
    /// it (taking over its consumers); when the output is empty, the new node
    /// feeds the output.
    pub fn add_node(self, kind: &'static str) {
        let Some(k) = nodes::kind(kind) else { return };
        let sel = *self.selected.peek();
        let mut new_id = 0;
        self.edit(|g| {
            let anchor = sel.filter(|s| *s != g.output && g.node(*s).is_some());
            let pos = match anchor {
                Some(a) => {
                    let p = g.node(a).unwrap().pos;
                    (p.0 + 220.0, p.1 + 40.0)
                }
                None => {
                    let n = g.nodes.len() as f64;
                    (80.0 + 30.0 * (n % 6.0), 60.0 + 70.0 * (n % 5.0))
                }
            };
            let id = g.add_node(kind, pos);
            new_id = id;
            if let Some(a) = anchor {
                if !k.inputs().is_empty() {
                    for (c, port) in g.consumers(a) {
                        let _ = g.connect(id, c, port);
                    }
                    let _ = g.connect(a, id, 0);
                    return;
                }
            }
            let out = g.output;
            if g.node(out).unwrap().inputs[0].is_none() {
                let _ = g.connect(id, out, 0);
            }
        });
        if k.category() == Category::Generator {
            self.fit_canvas_to_window(false);
        }
        self.select(Some(new_id));
    }

    /// Deletes the selected edge, or else the selected node (Delete key).
    pub fn delete_selection(self) {
        // Copy the signals out first: a borrow taken in an `if let` condition
        // lives through its body, and deleting writes to them.
        let edge = *self.selected_edge.peek();
        let sel = *self.selected.peek();
        if let Some((to, port)) = edge {
            super::graph_view::disconnect_port(self, to, port);
        } else if let Some(sel) = sel {
            self.delete_node(sel);
        }
    }

    /// Deletes a node, bridging single-input nodes so the chain stays intact.
    /// Deleting a layer stack deletes all of its links.
    pub fn delete_node(mut self, id: NodeId) {
        if id == self.graph.peek().output {
            return;
        }
        self.edit(|g| {
            if let Some(chain) = g.stack_chain(id) {
                let consumers = g.consumers(chain.head);
                for s in chain.stacks {
                    g.remove_node(s);
                }
                let _ = consumers;
                return;
            }
            let n = g.node(id).unwrap().clone();
            let connected: Vec<NodeId> = n.inputs.iter().flatten().copied().collect();
            let consumers = g.consumers(id);
            g.remove_node(id);
            if connected.len() == 1 {
                for (c, p) in consumers {
                    let _ = g.connect(connected[0], c, p);
                }
            }
        });
        if *self.selected.peek() == Some(id) {
            self.selected.set(None);
        }
        if *self.pinned.peek() == Some(id) {
            self.pinned.set(None);
        }
    }

    pub fn stack_chain(&self, id: NodeId) -> Option<StackChain> {
        self.graph.read().stack_chain(id)
    }

    pub fn toast(mut self, msg: impl Into<String>, error: bool) {
        self.toast.set(Some((msg.into(), error)));
    }
}

fn conflict_key(c: &SizeConflict) -> (NodeId, u64, u64) {
    let k = |s: Size| s.w.to_bits() ^ s.h.to_bits().rotate_left(17);
    (c.node, k(c.a.1), k(c.b.1))
}

pub fn fmt_size(s: Size) -> String {
    format!("{}×{}", s.w.round() as i64, s.h.round() as i64)
}
