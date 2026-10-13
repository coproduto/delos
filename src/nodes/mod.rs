//! Node kinds. Every node type implements [`NodeKind`] and is listed in
//! [`registry`]; nothing else in the application needs to know about a
//! specific kind. Parameter forms and on-canvas (WYSIWYG) controls are
//! generated from the declarative [`ParamDef`] / [`Overlay`] descriptions.

mod basic;
mod bgremove;
mod generators;
mod resize;
mod split;

use crate::engine::geom::{Map2, Request, Size};
use crate::engine::tile::Tile;
use crate::engine::Ctx;
use crate::graph::Node;
use std::collections::BTreeMap;
use std::sync::LazyLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    Source,
    Generator,
    Filter,
    /// Nodes that divide an image into parts, one per output.
    Split,
    Combine,
    Output,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::Source => "Sources",
            Category::Generator => "Generators",
            Category::Filter => "Filters",
            Category::Split => "Split",
            Category::Combine => "Combine",
            Category::Output => "Output",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Float(f64),
    Int(i64),
    Bool(bool),
    Choice(String),
    Color([f32; 4]),
    Text(String),
}

#[derive(Clone, Debug)]
pub enum ParamKind {
    Float { min: f64, max: f64, step: f64 },
    /// A position or length expressed as a fraction (0..1) of the node's
    /// extent along an axis, so it stays valid for size-less generators.
    Fraction { axis: Axis },
    Int { min: i64, max: i64 },
    Bool,
    Choice(&'static [(&'static str, &'static str)]),
    Color,
    Text,
    /// Stored but not shown in the form.
    Hidden,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    /// Fraction of the smaller dimension.
    Min,
}

#[derive(Clone, Debug)]
pub struct ParamDef {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: ParamKind,
    pub default: Value,
    /// Only show this parameter when another (choice) parameter has one of
    /// the given values.
    pub visible_if: Option<(&'static str, &'static [&'static str])>,
}

impl ParamDef {
    pub fn new(key: &'static str, label: &'static str, kind: ParamKind, default: Value) -> Self {
        Self { key, label, kind, default, visible_if: None }
    }
    pub fn float(key: &'static str, label: &'static str, min: f64, max: f64, step: f64, default: f64) -> Self {
        Self::new(key, label, ParamKind::Float { min, max, step }, Value::Float(default))
    }
    pub fn fraction(key: &'static str, label: &'static str, axis: Axis, default: f64) -> Self {
        Self::new(key, label, ParamKind::Fraction { axis }, Value::Float(default))
    }
    pub fn int(key: &'static str, label: &'static str, min: i64, max: i64, default: i64) -> Self {
        Self::new(key, label, ParamKind::Int { min, max }, Value::Int(default))
    }
    pub fn bool(key: &'static str, label: &'static str, default: bool) -> Self {
        Self::new(key, label, ParamKind::Bool, Value::Bool(default))
    }
    pub fn choice(key: &'static str, label: &'static str, options: &'static [(&'static str, &'static str)], default: &str) -> Self {
        Self::new(key, label, ParamKind::Choice(options), Value::Choice(default.into()))
    }
    pub fn color(key: &'static str, label: &'static str, default: [f32; 4]) -> Self {
        Self::new(key, label, ParamKind::Color, Value::Color(default))
    }
    pub fn text(key: &'static str, label: &'static str, default: &str) -> Self {
        Self::new(key, label, ParamKind::Text, Value::Text(default.into()))
    }
    pub fn hidden(key: &'static str, default: Value) -> Self {
        Self::new(key, key, ParamKind::Hidden, default)
    }
    pub fn when(mut self, key: &'static str, values: &'static [&'static str]) -> Self {
        self.visible_if = Some((key, values));
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Params(pub BTreeMap<&'static str, Value>);

impl Params {
    pub fn f(&self, key: &str) -> f64 {
        match self.0.get(key) {
            Some(Value::Float(v)) => *v,
            Some(Value::Int(v)) => *v as f64,
            Some(Value::Bool(b)) => *b as u8 as f64,
            _ => 0.0,
        }
    }
    pub fn i(&self, key: &str) -> i64 {
        match self.0.get(key) {
            Some(Value::Int(v)) => *v,
            Some(Value::Float(v)) => v.round() as i64,
            _ => 0,
        }
    }
    pub fn b(&self, key: &str) -> bool {
        matches!(self.0.get(key), Some(Value::Bool(true)))
    }
    pub fn choice(&self, key: &str) -> &str {
        match self.0.get(key) {
            Some(Value::Choice(s)) | Some(Value::Text(s)) => s,
            _ => "",
        }
    }
    pub fn color(&self, key: &str) -> [f32; 4] {
        match self.0.get(key) {
            Some(Value::Color(c)) => *c,
            _ => [0.0, 0.0, 0.0, 1.0],
        }
    }
    pub fn text(&self, key: &str) -> &str {
        self.choice(key)
    }
    pub fn set(&mut self, key: &'static str, v: Value) {
        self.0.insert(key, v);
    }
    /// Index of the current choice within `options` (for kernel params).
    pub fn choice_index(&self, key: &str, options: &[(&str, &str)]) -> usize {
        let c = self.choice(key);
        options.iter().position(|(k, _)| *k == c).unwrap_or(0)
    }
}

/// On-canvas controls a node kind exposes in the image view. All positions
/// refer to fraction parameters (0..1 of the extent).
#[derive(Clone, Debug)]
pub enum Overlay {
    /// A draggable axis line. `orientation` is a choice param with values
    /// "vertical" / "horizontal".
    Axis { orientation: &'static str, position: &'static str },
    /// A draggable straight line at a fixed orientation, with the names of
    /// the two sides (e.g. a split border).
    Line { position: &'static str, horizontal: bool, sides: [&'static str; 2] },
    /// A draggable/resizable rectangle.
    Rect { x: &'static str, y: &'static str, w: &'static str, h: &'static str },
    /// A draggable point.
    Point { x: &'static str, y: &'static str, label: &'static str },
}

/// Which image the image view shows while the node is being edited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditView {
    /// The node's own output (default).
    Output,
    /// The node's input on the given port (e.g. cropping shows the uncropped
    /// image with the crop rectangle).
    Input(usize),
}

pub trait NodeKind: Send + Sync + 'static {
    /// Stable identifier, used in saved graphs.
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn category(&self) -> Category;
    fn description(&self) -> &'static str;
    /// Names of the input ports.
    fn inputs(&self) -> &'static [&'static str] {
        &[]
    }
    /// Names of the outputs. Most nodes produce one image; nodes such as
    /// splits produce several (all of the node's size).
    fn outputs(&self) -> &'static [&'static str] {
        &["image"]
    }
    fn params(&self) -> Vec<ParamDef> {
        Vec::new()
    }
    /// Output size given the input sizes; `None` means size-less (the node
    /// adapts to whatever size it is requested at).
    fn size(&self, node: &Node, inputs: &[Option<Size>]) -> Option<Size>;
    /// Renders the requested grid (of the first output).
    fn render(&self, ctx: &Ctx, node: &Node, req: &Request) -> anyhow::Result<Tile>;
    /// Renders the requested grid of output `output`. Nodes with several
    /// outputs override this.
    fn render_output(&self, ctx: &Ctx, node: &Node, output: usize, req: &Request) -> anyhow::Result<Tile> {
        debug_assert_eq!(output, 0);
        self.render(ctx, node, req)
    }
    fn overlays(&self, _node: &Node) -> Vec<Overlay> {
        Vec::new()
    }
    fn edit_view(&self) -> EditView {
        EditView::Output
    }
    /// How this node's space maps onto the space of input `port`, given this
    /// node's extent and the input's own size: `(map, input extent)`. Used to
    /// draw an upstream node's controls over a downstream image. The default
    /// is a top-left aligned input, which suits nodes that do not move pixels.
    fn input_map(&self, _node: &Node, _port: usize, extent: Size, input: Option<Size>) -> (Map2, Size) {
        (Map2::IDENTITY, input.unwrap_or(extent))
    }
    /// Whether the node merges several definite-size inputs into one canvas
    /// (triggers the size-mismatch dialog).
    fn combines_inputs(&self) -> bool {
        false
    }
    /// Shown in the UI so users know a node can take a while.
    fn slow_hint(&self) -> Option<&'static str> {
        None
    }
    /// Whether this kind appears in the "Add node" menu.
    fn user_creatable(&self) -> bool {
        true
    }
}

static REGISTRY: LazyLock<Vec<&'static dyn NodeKind>> = LazyLock::new(|| {
    let mut v: Vec<&'static dyn NodeKind> = Vec::new();
    v.extend(basic::kinds());
    v.extend(resize::kinds());
    v.extend(split::kinds());
    v.extend(generators::kinds());
    v.extend(bgremove::kinds());
    v
});

pub fn registry() -> &'static [&'static dyn NodeKind] {
    &REGISTRY
}

pub fn kind(id: &str) -> Option<&'static dyn NodeKind> {
    REGISTRY.iter().copied().find(|k| k.id() == id)
}

pub fn default_params(k: &dyn NodeKind) -> Params {
    let mut p = Params::default();
    for d in k.params() {
        p.set(d.key, d.default);
    }
    p
}

pub use basic::IMAGE_PARAM;
pub use resize::resize_to;

/// Ids of the built-in kinds the application refers to directly.
pub mod ids {
    pub const OUTPUT: &str = "output";
    pub const IMAGE: &str = "image";
    pub const STACK: &str = "stack";
    pub const DISABLE: &str = "disable";
    pub const RESIZE: &str = "resize";
}
