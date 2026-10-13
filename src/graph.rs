//! The document model: a DAG of nodes. Edges are stored on the consumer
//! side (`Node::inputs[port] = Some(link)`), where a [`Link`] names one
//! output of the producing node (most nodes have a single output).

use crate::engine::geom::{Map2, Size};
use crate::engine::source::SourceImage;
use crate::nodes::{self, ids, NodeKind, Params, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

pub type NodeId = u64;
pub type ImageId = u64;

/// One output of a node: what an input port is connected to, and what the
/// image view and export show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Link {
    pub node: NodeId,
    pub output: usize,
}

impl Link {
    pub const fn new(node: NodeId, output: usize) -> Self {
        Self { node, output }
    }
}

/// A node's first (usually only) output.
impl From<NodeId> for Link {
    fn from(node: NodeId) -> Self {
        Link { node, output: 0 }
    }
}

#[derive(Clone)]
pub struct Node {
    pub id: NodeId,
    pub kind: &'static str,
    pub params: Params,
    pub inputs: Vec<Option<Link>>,
    pub pos: (f64, f64),
    pub label: Option<String>,
}

impl Node {
    pub fn kind(&self) -> &'static dyn NodeKind {
        nodes::kind(self.kind).expect("unknown node kind")
    }

    pub fn title(&self) -> String {
        self.label.clone().unwrap_or_else(|| self.kind().name().to_string())
    }

    /// The nodes this node reads from (one entry per connected port).
    pub fn input_nodes(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.inputs.iter().flatten().map(|l| l.node)
    }

    /// The node connected to input `port`, if any.
    pub fn input_node(&self, port: usize) -> Option<NodeId> {
        self.inputs.get(port).copied().flatten().map(|l| l.node)
    }
}

#[derive(Clone)]
pub struct Graph {
    pub nodes: BTreeMap<NodeId, Node>,
    pub output: NodeId,
    pub images: HashMap<ImageId, Arc<SourceImage>>,
    next_id: u64,
}

/// A chain of nested stack operators presented as one layer stack:
/// `layers[0]` is the frontmost layer.
#[derive(Clone, Debug, PartialEq)]
pub struct StackChain {
    pub head: NodeId,
    pub stacks: Vec<NodeId>,
    pub layers: Vec<Option<Link>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SizeConflict {
    pub node: NodeId,
    pub a: (usize, Size),
    pub b: (usize, Size),
}

impl std::fmt::Debug for Graph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Graph({} nodes, {} images)", self.nodes.len(), self.images.len())
    }
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

impl Graph {
    pub fn new() -> Self {
        let mut g = Graph { nodes: BTreeMap::new(), output: 0, images: HashMap::new(), next_id: 1 };
        g.output = g.add_node(ids::OUTPUT, (640.0, 200.0));
        g
    }

    fn fresh_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Nearest position at or below `pos` not covered by another node.
    pub fn free_position(&self, mut pos: (f64, f64)) -> (f64, f64) {
        for _ in 0..50 {
            let hit = self.nodes.values().any(|n| (n.pos.0 - pos.0).abs() < 196.0 && (n.pos.1 - pos.1).abs() < 90.0);
            if !hit {
                break;
            }
            pos.1 += 40.0;
        }
        pos
    }

    pub fn add_node(&mut self, kind: &str, pos: (f64, f64)) -> NodeId {
        let pos = self.free_position(pos);
        self.add_node_at(kind, pos)
    }

    /// Adds a node at exactly `pos` (used when loading saved graphs).
    pub fn add_node_at(&mut self, kind: &str, pos: (f64, f64)) -> NodeId {
        let k = nodes::kind(kind).unwrap_or_else(|| panic!("unknown node kind {kind}"));
        let id = self.fresh_id();
        let node = Node { id, kind: k.id(), params: nodes::default_params(k), inputs: vec![None; k.inputs().len()], pos, label: None };
        self.nodes.insert(id, node);
        id
    }

    pub fn add_image(&mut self, img: Arc<SourceImage>) -> ImageId {
        let id = self.fresh_id();
        self.images.insert(id, img);
        id
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    /// Display name of an output: the node's title, plus the output's name
    /// for nodes with several outputs ("Vertical Split · left").
    pub fn link_title(&self, link: Link) -> String {
        let Some(n) = self.nodes.get(&link.node) else { return String::new() };
        let outputs = n.kind().outputs();
        match outputs.get(link.output) {
            Some(o) if outputs.len() > 1 => format!("{} · {o}", n.title()),
            _ => n.title(),
        }
    }

    /// Whether `link` names an existing output.
    pub fn has_link(&self, link: Link) -> bool {
        self.nodes.get(&link.node).is_some_and(|n| link.output < n.kind().outputs().len())
    }

    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(&id)
    }

    pub fn set_param(&mut self, id: NodeId, key: &'static str, v: Value) {
        if let Some(n) = self.nodes.get_mut(&id) {
            n.params.set(key, v);
        }
    }

    /// Removes a node and every edge touching it. The output node cannot be
    /// removed. Image data no longer referenced is dropped.
    pub fn remove_node(&mut self, id: NodeId) {
        if id == self.output {
            return;
        }
        self.nodes.remove(&id);
        self.unlink(id);
        self.gc_images();
    }

    /// Disconnects every input reading from any output of `id`.
    fn unlink(&mut self, id: NodeId) {
        for n in self.nodes.values_mut() {
            for i in n.inputs.iter_mut() {
                if i.is_some_and(|l| l.node == id) {
                    *i = None;
                }
            }
        }
    }

    fn gc_images(&mut self) {
        let used: std::collections::HashSet<i64> =
            self.nodes.values().filter(|n| n.kind == ids::IMAGE).map(|n| n.params.i(nodes::IMAGE_PARAM)).collect();
        self.images.retain(|k, _| used.contains(&(*k as i64)));
    }

    /// `(consumer, port)` for every input reading any output of `id`.
    pub fn consumers(&self, id: NodeId) -> Vec<(NodeId, usize)> {
        self.links_from(id).into_iter().map(|(_, c, p)| (c, p)).collect()
    }

    /// `(link, consumer, port)` for every input reading any output of `id`.
    pub fn links_from(&self, id: NodeId) -> Vec<(Link, NodeId, usize)> {
        let mut v = Vec::new();
        for n in self.nodes.values() {
            for (port, i) in n.inputs.iter().enumerate() {
                if let Some(l) = i.filter(|l| l.node == id) {
                    v.push((l, n.id, port));
                }
            }
        }
        v
    }

    /// True if `target` is `from` or one of its (transitive) inputs.
    pub fn depends_on(&self, from: NodeId, target: NodeId) -> bool {
        let mut stack = vec![from];
        let mut seen = std::collections::HashSet::new();
        while let Some(n) = stack.pop() {
            if n == target {
                return true;
            }
            if !seen.insert(n) {
                continue;
            }
            if let Some(node) = self.nodes.get(&n) {
                stack.extend(node.input_nodes());
            }
        }
        false
    }

    /// Connects an output (a node's first one, or a [`Link`]) to `to`'s
    /// input `port`, refusing cycles.
    pub fn connect(&mut self, from: impl Into<Link>, to: NodeId, port: usize) -> Result<(), String> {
        let from = from.into();
        let Some(src) = self.nodes.get(&from.node) else { return Err("unknown node".into()) };
        if !self.nodes.contains_key(&to) {
            return Err("unknown node".into());
        }
        if from.node == self.output {
            return Err("the output node has no outputs".into());
        }
        if from.output >= src.kind().outputs().len() {
            return Err("no such output".into());
        }
        if self.depends_on(from.node, to) {
            return Err("that connection would create a cycle".into());
        }
        let n = self.nodes.get_mut(&to).unwrap();
        if port >= n.inputs.len() {
            return Err("no such input".into());
        }
        n.inputs[port] = Some(from);
        Ok(())
    }

    pub fn disconnect(&mut self, to: NodeId, port: usize) {
        if let Some(n) = self.nodes.get_mut(&to) {
            if port < n.inputs.len() {
                n.inputs[port] = None;
            }
        }
    }

    // ----- sizes ---------------------------------------------------------

    pub fn sizes(&self) -> HashMap<NodeId, Option<Size>> {
        let mut memo = HashMap::new();
        for id in self.nodes.keys() {
            self.size_memo(*id, &mut memo);
        }
        memo
    }

    /// A node's size; all outputs of a node share it.
    pub fn size_of(&self, id: NodeId) -> Option<Size> {
        self.size_memo(id, &mut HashMap::new())
    }

    fn size_memo(&self, id: NodeId, memo: &mut HashMap<NodeId, Option<Size>>) -> Option<Size> {
        if let Some(s) = memo.get(&id) {
            return *s;
        }
        let Some(n) = self.nodes.get(&id) else { return None };
        let inputs: Vec<Option<Size>> = n.inputs.iter().map(|i| i.and_then(|i| self.size_memo(i.node, memo))).collect();
        let s = n.kind().size(n, &inputs);
        memo.insert(id, s);
        s
    }

    /// Canvas size used when a size-less node is shown or exported directly.
    pub fn canvas(&self) -> Size {
        let p = &self.nodes[&self.output].params;
        Size::new(p.f("canvas_w").max(1.0), p.f("canvas_h").max(1.0))
    }

    pub fn extent_of(&self, id: NodeId) -> Size {
        self.size_of(id).unwrap_or_else(|| self.canvas())
    }

    /// Maps `from`'s space into the space of `to`, an upstream node, along
    /// the first input path found: `(map, extent of to as seen from from)`.
    /// `None` when `to` does not feed `from`.
    pub fn space_map(&self, from: NodeId, to: NodeId) -> Option<(Map2, Size)> {
        let sizes = self.sizes();
        self.space_map_rec(from, to, Map2::IDENTITY, self.extent_of(from), &sizes, &mut HashSet::new())
    }

    fn space_map_rec(
        &self,
        at: NodeId,
        to: NodeId,
        m: Map2,
        extent: Size,
        sizes: &HashMap<NodeId, Option<Size>>,
        dead_ends: &mut HashSet<NodeId>,
    ) -> Option<(Map2, Size)> {
        if at == to {
            return Some((m, extent));
        }
        if dead_ends.contains(&at) {
            return None;
        }
        let n = self.nodes.get(&at)?;
        for (port, input) in n.inputs.iter().enumerate() {
            let Some(i) = input.map(|l| l.node) else { continue };
            let (im, ie) = n.kind().input_map(n, port, extent, sizes.get(&i).copied().flatten());
            if let Some(r) = self.space_map_rec(i, to, im.after(&m), ie, sizes, dead_ends) {
                return Some(r);
            }
        }
        dead_ends.insert(at);
        None
    }

    /// The first pair of definite input sizes that disagree on a combining
    /// node.
    pub fn size_conflict(&self, id: NodeId) -> Option<SizeConflict> {
        let n = self.nodes.get(&id)?;
        if !n.kind().combines_inputs() {
            return None;
        }
        let sizes: Vec<(usize, Size)> =
            n.inputs.iter().enumerate().filter_map(|(p, i)| i.and_then(|i| self.size_of(i.node)).map(|s| (p, s))).collect();
        for a in 0..sizes.len() {
            for b in a + 1..sizes.len() {
                let (sa, sb) = (sizes[a].1, sizes[b].1);
                if (sa.w - sb.w).abs() > 0.5 || (sa.h - sb.h).abs() > 0.5 {
                    return Some(SizeConflict { node: id, a: sizes[a], b: sizes[b] });
                }
            }
        }
        None
    }

    /// Inserts a resize node in front of `node`'s input `port` so that input
    /// is resized to `size`.
    pub fn insert_resize(&mut self, node: NodeId, port: usize, size: Size) -> Option<NodeId> {
        let src = self.nodes.get(&node)?.inputs.get(port).copied().flatten()?;
        let (sp, np) = (self.nodes[&src.node].pos, self.nodes[&node].pos);
        let r = self.add_node(ids::RESIZE, ((sp.0 + np.0) / 2.0, (sp.1 + np.1) / 2.0 + 40.0));
        nodes::resize_to(&mut self.nodes.get_mut(&r).unwrap().params, size);
        self.nodes.get_mut(&r).unwrap().inputs[0] = Some(src);
        self.nodes.get_mut(&node).unwrap().inputs[port] = Some(r.into());
        Some(r)
    }

    // ----- hashing (cache keys) ------------------------------------------

    /// Structural hash of the subgraph producing `id`; equal hashes mean
    /// equal images, which lets expensive results be cached safely.
    pub fn hash_of(&self, id: NodeId, memo: &mut HashMap<NodeId, u64>) -> u64 {
        if let Some(h) = memo.get(&id) {
            return *h;
        }
        let mut hs = std::collections::hash_map::DefaultHasher::new();
        if let Some(n) = self.nodes.get(&id) {
            n.kind.hash(&mut hs);
            for (k, v) in &n.params.0 {
                k.hash(&mut hs);
                hash_value(v, &mut hs);
            }
            if n.kind == ids::OUTPUT {
                // The canvas size of the output does not change its input.
            }
            for i in &n.inputs {
                match i {
                    Some(i) => self.link_hash(*i, memo).hash(&mut hs),
                    None => 0u64.hash(&mut hs),
                }
            }
        }
        let h = hs.finish();
        memo.insert(id, h);
        h
    }

    /// Structural hash of one output's image.
    pub fn link_hash(&self, link: Link, memo: &mut HashMap<NodeId, u64>) -> u64 {
        let h = self.hash_of(link.node, memo);
        if link.output == 0 {
            return h;
        }
        let mut hs = std::collections::hash_map::DefaultHasher::new();
        (h, link.output).hash(&mut hs);
        hs.finish()
    }

    // ----- layer stacks --------------------------------------------------

    fn is_stack(&self, id: NodeId) -> bool {
        self.nodes.get(&id).is_some_and(|n| n.kind == ids::STACK)
    }

    /// A stack that is only used as the "rest" input of another stack is an
    /// interior link of a layer stack and is not shown on its own.
    pub fn is_interior_stack(&self, id: NodeId) -> bool {
        if !self.is_stack(id) {
            return false;
        }
        let c = self.consumers(id);
        c.len() == 1 && c[0].1 == 1 && self.is_stack(c[0].0)
    }

    pub fn chain_head(&self, mut id: NodeId) -> NodeId {
        while self.is_interior_stack(id) {
            id = self.consumers(id)[0].0;
        }
        id
    }

    pub fn stack_chain(&self, id: NodeId) -> Option<StackChain> {
        if !self.is_stack(id) {
            return None;
        }
        let head = self.chain_head(id);
        let mut stacks = vec![head];
        let mut layers = Vec::new();
        let mut cur = head;
        loop {
            let n = &self.nodes[&cur];
            layers.push(n.inputs[0]);
            match n.inputs[1] {
                Some(next) if self.is_interior_stack(next.node) => {
                    stacks.push(next.node);
                    cur = next.node;
                }
                Some(other) => {
                    layers.push(Some(other));
                    break;
                }
                None => break,
            }
        }
        Some(StackChain { head, stacks, layers })
    }

    /// Rewrites a layer stack so it contains exactly `layers` (front first),
    /// reusing the existing stack nodes. The head keeps its id so whatever
    /// consumes the stack stays connected.
    pub fn set_stack_layers(&mut self, head: NodeId, layers: Vec<Option<Link>>) {
        let Some(chain) = self.stack_chain(head) else { return };
        let head = chain.head;
        let needed = layers.len().saturating_sub(1).max(1);
        let mut stacks = chain.stacks.clone();
        let base = self.nodes[&head].pos;
        while stacks.len() < needed {
            let k = stacks.len() as f64;
            let id = self.add_node(ids::STACK, (base.0 - 20.0 * k, base.1 + 60.0 * k));
            stacks.push(id);
        }
        for extra in stacks.split_off(needed) {
            self.nodes.remove(&extra);
            self.unlink(extra);
        }
        for (k, s) in stacks.iter().enumerate() {
            let front = layers.get(k).copied().flatten();
            let back = if k + 1 < stacks.len() { Some(stacks[k + 1].into()) } else { layers.get(k + 1).copied().flatten() };
            let n = self.nodes.get_mut(s).unwrap();
            n.inputs = vec![front, back];
        }
    }
}

fn hash_value(v: &Value, h: &mut impl Hasher) {
    match v {
        Value::Float(f) => f.to_bits().hash(h),
        Value::Int(i) => i.hash(h),
        Value::Bool(b) => b.hash(h),
        Value::Choice(s) | Value::Text(s) => s.hash(h),
        Value::Color(c) => {
            for x in c {
                x.to_bits().hash(h)
            }
        }
    }
}
