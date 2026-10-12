//! The document model: a DAG of nodes. Edges are stored on the consumer
//! side (`Node::inputs[port] = Some(producer)`).

use crate::engine::geom::{Map2, Size};
use crate::engine::source::SourceImage;
use crate::nodes::{self, ids, NodeKind, Params, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

pub type NodeId = u64;
pub type ImageId = u64;

#[derive(Clone)]
pub struct Node {
    pub id: NodeId,
    pub kind: &'static str,
    pub params: Params,
    pub inputs: Vec<Option<NodeId>>,
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
    pub layers: Vec<Option<NodeId>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SizeConflict {
    pub node: NodeId,
    pub a: (usize, Size),
    pub b: (usize, Size),
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
        let k = nodes::kind(kind).unwrap_or_else(|| panic!("unknown node kind {kind}"));
        let pos = self.free_position(pos);
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
        for n in self.nodes.values_mut() {
            for i in n.inputs.iter_mut() {
                if *i == Some(id) {
                    *i = None;
                }
            }
        }
        self.gc_images();
    }

    fn gc_images(&mut self) {
        let used: std::collections::HashSet<i64> =
            self.nodes.values().filter(|n| n.kind == ids::IMAGE).map(|n| n.params.i(nodes::IMAGE_PARAM)).collect();
        self.images.retain(|k, _| used.contains(&(*k as i64)));
    }

    pub fn consumers(&self, id: NodeId) -> Vec<(NodeId, usize)> {
        let mut v = Vec::new();
        for n in self.nodes.values() {
            for (port, i) in n.inputs.iter().enumerate() {
                if *i == Some(id) {
                    v.push((n.id, port));
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
                stack.extend(node.inputs.iter().flatten());
            }
        }
        false
    }

    /// Connects `from`'s output to `to`'s input `port`, refusing cycles.
    pub fn connect(&mut self, from: NodeId, to: NodeId, port: usize) -> Result<(), String> {
        if !self.nodes.contains_key(&from) || !self.nodes.contains_key(&to) {
            return Err("unknown node".into());
        }
        if from == self.output {
            return Err("the output node has no outputs".into());
        }
        if self.depends_on(from, to) {
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

    pub fn size_of(&self, id: NodeId) -> Option<Size> {
        self.size_memo(id, &mut HashMap::new())
    }

    fn size_memo(&self, id: NodeId, memo: &mut HashMap<NodeId, Option<Size>>) -> Option<Size> {
        if let Some(s) = memo.get(&id) {
            return *s;
        }
        let Some(n) = self.nodes.get(&id) else { return None };
        let inputs: Vec<Option<Size>> = n.inputs.iter().map(|i| i.and_then(|i| self.size_memo(i, memo))).collect();
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
            let Some(i) = *input else { continue };
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
            n.inputs.iter().enumerate().filter_map(|(p, i)| i.and_then(|i| self.size_of(i)).map(|s| (p, s))).collect();
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
        let (sp, np) = (self.nodes[&src].pos, self.nodes[&node].pos);
        let r = self.add_node(ids::RESIZE, ((sp.0 + np.0) / 2.0, (sp.1 + np.1) / 2.0 + 40.0));
        nodes::resize_to(&mut self.nodes.get_mut(&r).unwrap().params, size);
        self.nodes.get_mut(&r).unwrap().inputs[0] = Some(src);
        self.nodes.get_mut(&node).unwrap().inputs[port] = Some(r);
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
                    Some(i) => self.hash_of(*i, memo).hash(&mut hs),
                    None => 0u64.hash(&mut hs),
                }
            }
        }
        let h = hs.finish();
        memo.insert(id, h);
        h
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
                Some(next) if self.is_interior_stack(next) => {
                    stacks.push(next);
                    cur = next;
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
    pub fn set_stack_layers(&mut self, head: NodeId, layers: Vec<Option<NodeId>>) {
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
            for n in self.nodes.values_mut() {
                for i in n.inputs.iter_mut() {
                    if *i == Some(extra) {
                        *i = None;
                    }
                }
            }
        }
        for (k, s) in stacks.iter().enumerate() {
            let front = layers.get(k).copied().flatten();
            let back = if k + 1 < stacks.len() { Some(stacks[k + 1]) } else { layers.get(k + 1).copied().flatten() };
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
