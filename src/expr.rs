//! Delos expressions: the plain-text, Lisp-like form of a node graph, used
//! as the project file format.
//!
//! ```text
//! ; Nodes used more than once are bound with `def`; everything else nests.
//! (def photo (image "inputs/photo.png" :at (60 40)))
//! (output
//!   (stack
//!     (invert photo)
//!     (blend photo (noise :scale 0.3) :mode multiply))   ; written by hand:
//!   :canvas_w 1920 :canvas_h 1080)                       ; other params default
//! (noise :seed 4)   ; a node not connected to anything
//! ```
//!
//! - `(kind input… :param value…)`: inputs in port order (`_` leaves a port
//!   empty), parameters by key. Every parameter is written when saving;
//!   when reading, omitted parameters take their defaults.
//! - `(stack front … back)`: a layer stack (a chain of stack nodes).
//! - Nodes with several outputs (splits) are always bound with `def`, and
//!   each output is referred to as `name.output` (e.g. `s.left`).
//! - `(image "path")`: a static image; the path is relative to the project.
//! - `:at (x y)` is the position in the graph view and `:title "…"` a custom
//!   title. Nodes without `:at` are laid out automatically.
//!
//! Values: numbers, `true`/`false`, choices as symbols (`multiply`), colors
//! as `(r g b a)` lists in 0..1, and text as strings.

use crate::engine::source::SourceImage;
use crate::graph::{Graph, Link, NodeId};
use crate::nodes::{self, ids, ParamKind, Value, IMAGE_PARAM};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

/// Parameters of static image nodes that are derived from the file.
const IMAGE_DERIVED: &[&str] = &[IMAGE_PARAM, "width", "height"];

/// The project path of a static image node's image, if it has one.
pub fn image_file(g: &Graph, node: &crate::graph::Node) -> Option<String> {
    g.images.get(&(node.params.i(IMAGE_PARAM) as u64)).and_then(|img| img.file().project)
}

const WIDTH: usize = 88;

// ---------------------------------------------------------------------------
// Errors

#[derive(Clone, Debug, PartialEq)]
pub struct ExprError {
    pub line: usize,
    pub col: usize,
    pub message: String,
}

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}:{}: {}", self.line, self.col, self.message)
    }
}

impl std::error::Error for ExprError {}

fn err<T>(pos: Pos, message: impl Into<String>) -> Result<T, ExprError> {
    Err(ExprError { line: pos.line, col: pos.col, message: message.into() })
}

// ---------------------------------------------------------------------------
// Syntax

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Sexp {
    List(Vec<Sexp>, Pos),
    Sym(String, Pos),
    Str(String, Pos),
    /// Kept as written so each parameter type can parse it exactly.
    Num(String, Pos),
}

impl Sexp {
    fn pos(&self) -> Pos {
        match self {
            Sexp::List(_, p) | Sexp::Sym(_, p) | Sexp::Str(_, p) | Sexp::Num(_, p) => *p,
        }
    }
    fn describe(&self) -> String {
        match self {
            Sexp::List(..) => "a list".into(),
            Sexp::Sym(s, _) => format!("`{s}`"),
            Sexp::Str(s, _) => format!("\"{s}\""),
            Sexp::Num(n, _) => n.clone(),
        }
    }
}

/// Parses expression text into top-level forms.
pub fn parse(text: &str) -> Result<Vec<Sexp>, ExprError> {
    let mut p = Parser { chars: text.chars().collect(), i: 0, line: 1, col: 1 };
    let mut forms = Vec::new();
    loop {
        p.skip_space();
        if p.peek().is_none() {
            return Ok(forms);
        }
        forms.push(p.form()?);
    }
}

struct Parser {
    chars: Vec<char>,
    i: usize,
    line: usize,
    col: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.i).copied()
    }
    fn pos(&self) -> Pos {
        Pos { line: self.line, col: self.col }
    }
    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.i += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }
    fn skip_space(&mut self) {
        while let Some(c) = self.peek() {
            if c == ';' {
                while self.peek().is_some_and(|c| c != '\n') {
                    self.bump();
                }
            } else if c.is_whitespace() {
                self.bump();
            } else {
                break;
            }
        }
    }
    fn form(&mut self) -> Result<Sexp, ExprError> {
        let pos = self.pos();
        match self.peek() {
            Some('(') => {
                self.bump();
                let mut items = Vec::new();
                loop {
                    self.skip_space();
                    match self.peek() {
                        Some(')') => {
                            self.bump();
                            return Ok(Sexp::List(items, pos));
                        }
                        None => return err(pos, "unclosed `(`"),
                        _ => items.push(self.form()?),
                    }
                }
            }
            Some(')') => err(pos, "unexpected `)`"),
            Some('"') => {
                self.bump();
                let mut s = String::new();
                loop {
                    match self.bump() {
                        Some('"') => return Ok(Sexp::Str(s, pos)),
                        Some('\\') => match self.bump() {
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            Some(c @ ('"' | '\\')) => s.push(c),
                            Some(c) => return err(self.pos(), format!("unknown escape `\\{c}`")),
                            None => return err(pos, "unterminated string"),
                        },
                        Some(c) => s.push(c),
                        None => return err(pos, "unterminated string"),
                    }
                }
            }
            _ => {
                let mut s = String::new();
                while let Some(c) = self.peek() {
                    if c.is_whitespace() || matches!(c, '(' | ')' | '"' | ';') {
                        break;
                    }
                    s.push(c);
                    self.bump();
                }
                let numeric = s.starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '+' || c == '.') && s.parse::<f64>().is_ok();
                Ok(if numeric { Sexp::Num(s, pos) } else { Sexp::Sym(s, pos) })
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Graph → text

/// Pretty-printing tree: an atom, or a list whose items stay together.
enum Doc {
    Atom(String),
    List(Vec<Doc>),
}

impl Doc {
    fn flat(&self) -> String {
        match self {
            Doc::Atom(s) => s.clone(),
            Doc::List(items) => format!("({})", items.iter().map(Doc::flat).collect::<Vec<_>>().join(" ")),
        }
    }
    /// Flat width, or anything above `limit` once it is known not to fit
    /// (keeps pretty-printing linear in the size of the graph).
    fn width(&self, limit: usize) -> usize {
        match self {
            Doc::Atom(s) => s.len(),
            Doc::List(items) => {
                let mut w = 1 + items.len();
                for it in items {
                    if w > limit {
                        break;
                    }
                    w += it.width(limit - w);
                }
                w
            }
        }
    }
    fn write(&self, indent: usize, out: &mut String) {
        let fits = indent + self.width(WIDTH.saturating_sub(indent)) <= WIDTH;
        match self {
            Doc::List(items) if !fits && items.len() > 1 => {
                out.push('(');
                items[0].write(indent + 1, out);
                // `(def name` stays on one line; the definition follows.
                let sticky = match &items[0] {
                    Doc::Atom(a) if a == "def" && items.len() > 2 => 2,
                    _ => 1,
                };
                for it in &items[1..sticky] {
                    out.push(' ');
                    it.write(indent + 1, out);
                }
                for it in &items[sticky..] {
                    out.push('\n');
                    out.push_str(&" ".repeat(indent + 2));
                    it.write(indent + 2, out);
                }
                out.push(')');
            }
            _ => out.push_str(&self.flat()),
        }
    }
}

fn quote(s: &str) -> String {
    let mut q = String::with_capacity(s.len() + 2);
    q.push('"');
    for c in s.chars() {
        match c {
            '"' => q.push_str("\\\""),
            '\\' => q.push_str("\\\\"),
            '\n' => q.push_str("\\n"),
            '\t' => q.push_str("\\t"),
            c => q.push(c),
        }
    }
    q.push('"');
    q
}

fn is_symbol(s: &str) -> bool {
    !s.is_empty()
        && s.starts_with(|c: char| c.is_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '+'))
        && !matches!(s, "_" | "true" | "false" | "def" | "output")
}

/// A name `def` can bind: a symbol without `.` (which selects outputs).
fn is_name(s: &str) -> bool {
    is_symbol(s) && !s.contains('.')
}

fn value_text(v: &Value) -> String {
    match v {
        Value::Float(f) => format!("{f}"),
        Value::Int(i) => format!("{i}"),
        Value::Bool(b) => format!("{b}"),
        Value::Choice(s) if is_symbol(s) => s.clone(),
        Value::Choice(s) | Value::Text(s) => quote(s),
        Value::Color(c) => format!("({} {} {} {})", c[0], c[1], c[2], c[3]),
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

struct Printer<'a> {
    g: &'a Graph,
    names: HashMap<NodeId, String>,
    collapsed: HashSet<NodeId>,
}

impl Printer<'_> {
    fn reference(&self, link: Option<Link>) -> Doc {
        match link {
            None => Doc::Atom("_".into()),
            Some(l) => match self.names.get(&l.node) {
                Some(name) => {
                    let outputs = self.g.nodes[&l.node].kind().outputs();
                    if outputs.len() > 1 {
                        Doc::Atom(format!("{name}.{}", outputs.get(l.output).copied().unwrap_or("?")))
                    } else {
                        Doc::Atom(name.clone())
                    }
                }
                None => self.node(l.node),
            },
        }
    }

    fn node(&self, id: NodeId) -> Doc {
        let n = &self.g.nodes[&id];
        let kind = n.kind();
        let mut items = vec![Doc::Atom(n.kind.to_string())];
        if n.kind == ids::STACK {
            // Flatten a chain of single-use stacks into one list of layers.
            let mut cur = n;
            loop {
                items.push(self.reference(cur.inputs[0]));
                match cur.inputs[1] {
                    Some(next) if next.output == 0 && self.collapsed.contains(&next.node) => cur = &self.g.nodes[&next.node],
                    back => {
                        items.push(self.reference(back));
                        break;
                    }
                }
            }
        } else if n.kind == ids::IMAGE {
            items.push(Doc::Atom(quote(&image_file(self.g, n).unwrap_or_default())));
        } else {
            let last = n.inputs.iter().rposition(|i| i.is_some()).map_or(0, |p| p + 1);
            items.extend(n.inputs[..last].iter().map(|i| self.reference(*i)));
        }
        for def in kind.params() {
            if n.kind == ids::IMAGE && IMAGE_DERIVED.contains(&def.key) {
                continue;
            }
            // Every parameter is written, defaults included, so a saved graph
            // does not change meaning if a default changes later.
            let v = n.params.0.get(def.key).unwrap_or(&def.default);
            items.push(Doc::Atom(format!(":{} {}", def.key, value_text(v))));
        }
        if let Some(label) = &n.label {
            let implicit = n.kind == ids::IMAGE && image_file(self.g, n).is_some_and(|f| label == file_name(&f));
            if !implicit {
                items.push(Doc::Atom(format!(":title {}", quote(label))));
            }
        }
        items.push(Doc::Atom(format!(":at ({} {})", n.pos.0.round(), n.pos.1.round())));
        Doc::List(items)
    }
}

/// Serializes a graph. Every static image must have a file path.
pub fn to_text(g: &Graph) -> Result<String, String> {
    for n in g.nodes.values().filter(|n| n.kind == ids::IMAGE) {
        if image_file(g, n).is_none() {
            return Err(format!("the image “{}” has no file in the project", n.title()));
        }
    }
    let mut consumers: HashMap<NodeId, usize> = HashMap::new();
    for n in g.nodes.values() {
        for i in n.input_nodes() {
            *consumers.entry(i).or_default() += 1;
        }
    }
    let uses = |id: NodeId| consumers.get(&id).copied().unwrap_or(0);
    let collapsed: HashSet<NodeId> = g.nodes.keys().copied().filter(|id| g.is_interior_stack(*id) && g.nodes[id].label.is_none()).collect();

    // Shared nodes get names, defined dependencies first.
    let mut names = HashMap::new();
    let mut taken: HashSet<String> = HashSet::new();
    let mut order = Vec::new();
    let mut seen = HashSet::new();
    fn visit(g: &Graph, id: NodeId, seen: &mut HashSet<NodeId>, order: &mut Vec<NodeId>) {
        if !seen.insert(id) {
            return;
        }
        for i in g.nodes[&id].input_nodes() {
            visit(g, i, seen, order);
        }
        order.push(id);
    }
    for id in g.nodes.keys() {
        visit(g, *id, &mut seen, &mut order);
    }
    // Shared nodes, and nodes with several outputs (whose outputs are
    // referred to by name), are bound with def.
    let multi = |id: NodeId| g.nodes[&id].kind().outputs().len() > 1;
    let shared: Vec<NodeId> = order.into_iter().filter(|id| uses(*id) >= 2 || (uses(*id) >= 1 && multi(*id))).collect();
    for id in &shared {
        let n = &g.nodes[id];
        let base = n.label.as_deref().map(|l| l.split('.').next().unwrap_or(l)).unwrap_or(n.kind);
        let mut base: String = base.chars().map(|c| if c.is_alphanumeric() || c == '-' { c.to_ascii_lowercase() } else { '-' }).collect();
        if !is_name(&base) {
            base = format!("{}-{base}", n.kind);
        }
        let mut name = base.clone();
        let mut k = 2;
        while !taken.insert(name.clone()) {
            name = format!("{base}-{k}");
            k += 1;
        }
        names.insert(*id, name);
    }
    let p = Printer { g, names, collapsed };

    let mut out = String::from("; Delos expression\n");
    for id in &shared {
        let doc = Doc::List(vec![Doc::Atom("def".into()), Doc::Atom(p.names[id].clone()), p.node(*id)]);
        doc.write(0, &mut out);
        out.push('\n');
    }
    p.node(g.output).write(0, &mut out);
    out.push('\n');
    for (id, _) in g.nodes.iter().filter(|(id, _)| **id != g.output && uses(**id) == 0) {
        p.node(*id).write(0, &mut out);
        out.push('\n');
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Text → graph

/// Paths of every image a parsed expression refers to.
pub fn image_files(forms: &[Sexp]) -> Vec<(String, Pos)> {
    fn walk(s: &Sexp, out: &mut Vec<(String, Pos)>) {
        if let Sexp::List(items, _) = s {
            if let [Sexp::Sym(head, _), Sexp::Str(file, pos), ..] = items.as_slice() {
                if head == ids::IMAGE {
                    out.push((file.clone(), *pos));
                }
            }
            for it in items {
                walk(it, out);
            }
        }
    }
    let mut out = Vec::new();
    for f in forms {
        walk(f, &mut out);
    }
    out
}

/// Builds a graph from parsed forms; `image` provides the decoded image for
/// a file path.
pub fn build(forms: &[Sexp], image: &dyn Fn(&str) -> Result<Arc<SourceImage>, String>) -> Result<Graph, ExprError> {
    let mut b = Builder { g: Graph::new(), env: HashMap::new(), placed: HashSet::new(), image };
    let mut output_seen = false;
    for form in forms {
        let Sexp::List(items, pos) = form else {
            return err(form.pos(), format!("expected a form like `(output …)`, found {}", form.describe()));
        };
        match items.first() {
            Some(Sexp::Sym(head, _)) if head == "def" => {
                let [_, Sexp::Sym(name, npos), expr] = items.as_slice() else {
                    return err(*pos, "expected `(def name expression)`");
                };
                if !is_name(name) {
                    return err(*npos, format!("`{name}` cannot be used as a name"));
                }
                if b.env.contains_key(name) {
                    return err(*npos, format!("`{name}` is already defined"));
                }
                let id = b.node(expr)?;
                b.env.insert(name.clone(), id);
            }
            Some(Sexp::Sym(head, _)) if head == ids::OUTPUT => {
                if output_seen {
                    return err(*pos, "only one `(output …)` is allowed");
                }
                output_seen = true;
                let out = b.g.output;
                b.fill(out, &items[1..], *pos)?;
            }
            _ => {
                b.node(form)?;
            }
        }
    }
    b.layout();
    Ok(b.g)
}

/// Parses and builds in one step.
pub fn from_text(text: &str, image: &dyn Fn(&str) -> Result<Arc<SourceImage>, String>) -> Result<Graph, ExprError> {
    build(&parse(text)?, image)
}

struct Builder<'a> {
    g: Graph,
    env: HashMap<String, NodeId>,
    /// Nodes with an explicit `:at`.
    placed: HashSet<NodeId>,
    image: &'a dyn Fn(&str) -> Result<Arc<SourceImage>, String>,
}

impl Builder<'_> {
    /// An input: `_`, a name (`photo`, or `s.left` for one output of a
    /// node with several), or a nested node.
    fn input(&mut self, s: &Sexp) -> Result<Option<Link>, ExprError> {
        match s {
            Sexp::Sym(name, _) if name == "_" => Ok(None),
            Sexp::Sym(name, pos) => self.reference(name, *pos).map(Some),
            Sexp::List(..) => {
                let id = self.node(s)?;
                let kind = self.g.nodes[&id].kind();
                if kind.outputs().len() > 1 {
                    return err(s.pos(), format!("`{}` has several outputs: bind it with `def` and use `name.{}`", kind.id(), kind.outputs()[0]));
                }
                Ok(Some(id.into()))
            }
            other => err(other.pos(), format!("expected a node, found {}", other.describe())),
        }
    }

    fn reference(&self, name: &str, pos: Pos) -> Result<Link, ExprError> {
        let (base, output) = match name.rsplit_once('.') {
            Some((b, o)) => (b, Some(o)),
            None => (name, None),
        };
        let Some(&id) = self.env.get(base) else { return err(pos, format!("unknown name `{base}`")) };
        let outputs = self.g.nodes[&id].kind().outputs();
        match output {
            None if outputs.len() == 1 => Ok(id.into()),
            None => err(pos, format!("`{base}` has several outputs: write {}", outputs.iter().map(|o| format!("`{base}.{o}`")).collect::<Vec<_>>().join(" or "))),
            Some(o) => match outputs.iter().position(|x| *x == o) {
                Some(k) => Ok(Link::new(id, k)),
                None => err(pos, format!("`{base}` has no output `{o}` (outputs: {})", outputs.join(", "))),
            },
        }
    }

    /// Creates the node a list form describes.
    fn node(&mut self, s: &Sexp) -> Result<NodeId, ExprError> {
        match s {
            Sexp::List(items, pos) => {
                let Some(Sexp::Sym(head, hpos)) = items.first() else {
                    return err(*pos, "expected a node like `(invert …)`");
                };
                if head == ids::OUTPUT || head == "def" {
                    return err(*hpos, format!("`{head}` is only allowed at the top level"));
                }
                let Some(kind) = nodes::kind(head) else {
                    return err(*hpos, format!("unknown node type `{head}`"));
                };
                let id = self.g.add_node_at(kind.id(), (0.0, 0.0));
                self.fill(id, &items[1..], *pos)?;
                Ok(id)
            }
            other => err(other.pos(), format!("expected a node like `(invert …)`, found {}", other.describe())),
        }
    }

    /// Applies inputs, parameters and attributes to a created node.
    fn fill(&mut self, id: NodeId, args: &[Sexp], pos: Pos) -> Result<(), ExprError> {
        let kind = self.g.nodes[&id].kind();
        let mut positional = Vec::new();
        let mut seen_keys = HashSet::new();
        let mut i = 0;
        while i < args.len() {
            match &args[i] {
                Sexp::Sym(k, kpos) if k.starts_with(':') => {
                    let key = &k[1..];
                    let Some(value) = args.get(i + 1) else {
                        return err(*kpos, format!("`{k}` needs a value"));
                    };
                    if !seen_keys.insert(key.to_string()) {
                        return err(*kpos, format!("`{k}` is given twice"));
                    }
                    self.attribute(id, key, value, *kpos)?;
                    i += 2;
                }
                other => {
                    positional.push(other);
                    i += 1;
                }
            }
        }
        if kind.id() == ids::IMAGE {
            let [Sexp::Str(file, fpos)] = positional.as_slice() else {
                return err(pos, "expected `(image \"inputs/file.png\" …)`");
            };
            let img = (self.image)(file).or_else(|e| err(*fpos, e))?;
            img.set_project_file(file.clone());
            let (w, h) = (img.width, img.height);
            let iid = self.g.add_image(img);
            let n = self.g.node_mut(id).unwrap();
            n.params.set(IMAGE_PARAM, Value::Int(iid as i64));
            n.params.set("width", Value::Int(w as i64));
            n.params.set("height", Value::Int(h as i64));
            n.label.get_or_insert_with(|| file_name(file).to_string());
            return Ok(());
        }
        if kind.id() == ids::STACK {
            let layers = positional.iter().map(|s| self.input(s)).collect::<Result<Vec<_>, _>>()?;
            if layers.len() > 2 {
                // Builds the chain from this (still unconnected) head.
                self.g.set_stack_layers(id, layers);
            } else {
                self.g.node_mut(id).unwrap().inputs = vec![layers.first().copied().flatten(), layers.get(1).copied().flatten()];
            }
            return Ok(());
        }
        let ports = kind.inputs().len();
        if positional.len() > ports {
            return err(positional[ports].pos(), format!("`{}` takes {ports} input(s)", kind.id()));
        }
        for (port, s) in positional.into_iter().enumerate() {
            // Inputs are built before their consumer and names only refer to
            // earlier definitions, so no connection can form a cycle.
            let input = self.input(s)?;
            self.g.node_mut(id).unwrap().inputs[port] = input;
        }
        Ok(())
    }

    fn attribute(&mut self, id: NodeId, key: &str, v: &Sexp, pos: Pos) -> Result<(), ExprError> {
        let kind = self.g.nodes[&id].kind();
        match key {
            "at" => {
                let Sexp::List(xy, _) = v else { return err(v.pos(), "expected `(x y)`") };
                let [Sexp::Num(x, _), Sexp::Num(y, _)] = xy.as_slice() else { return err(v.pos(), "expected `(x y)`") };
                self.g.node_mut(id).unwrap().pos = (x.parse().unwrap(), y.parse().unwrap());
                self.placed.insert(id);
            }
            "title" => {
                let Sexp::Str(t, _) = v else { return err(v.pos(), "expected a string") };
                self.g.node_mut(id).unwrap().label = Some(t.clone());
            }
            _ => {
                let def = kind
                    .params()
                    .into_iter()
                    .find(|d| d.key == key && !(kind.id() == ids::IMAGE && IMAGE_DERIVED.contains(&d.key)))
                    .map_or_else(|| err(pos, format!("`{}` has no parameter `:{key}`", kind.id())), Ok)?;
                let value = parse_value(&def.kind, &def.default, v)?;
                self.g.node_mut(id).unwrap().params.set(def.key, value);
            }
        }
        Ok(())
    }

    /// Places nodes without `:at` in columns by distance from the output.
    fn layout(&mut self) {
        let g = &self.g;
        let unplaced: Vec<NodeId> = g.nodes.keys().copied().filter(|id| !self.placed.contains(id)).collect();
        if unplaced.is_empty() {
            return;
        }
        let consumed: HashSet<NodeId> = g.nodes.values().flat_map(|n| n.input_nodes()).collect();
        let mut depth: HashMap<NodeId, usize> = HashMap::new();
        let mut stack: Vec<(NodeId, usize)> = g.nodes.keys().filter(|id| !consumed.contains(id)).map(|id| (*id, 0)).collect();
        while let Some((id, d)) = stack.pop() {
            if depth.get(&id).is_some_and(|old| *old >= d) {
                continue;
            }
            depth.insert(id, d);
            for i in g.nodes[&id].input_nodes() {
                stack.push((i, d + 1));
            }
        }
        let origin = if self.placed.contains(&g.output) { g.nodes[&g.output].pos } else { (900.0, 200.0) };
        let mut rows: HashMap<usize, usize> = HashMap::new();
        for id in unplaced {
            let d = depth.get(&id).copied().unwrap_or(0);
            let row = rows.entry(d).or_default();
            let pos = (origin.0 - 230.0 * d as f64, 40.0 + 110.0 * *row as f64);
            *row += 1;
            self.g.node_mut(id).unwrap().pos = pos;
        }
    }
}

fn parse_value(kind: &ParamKind, default: &Value, v: &Sexp) -> Result<Value, ExprError> {
    let num = |v: &Sexp| match v {
        Sexp::Num(n, _) => Ok(n.clone()),
        other => err(other.pos(), format!("expected a number, found {}", other.describe())),
    };
    Ok(match default {
        Value::Float(_) => Value::Float(num(v)?.parse().unwrap()),
        Value::Int(_) => {
            let f: f64 = num(v)?.parse().unwrap();
            if f.fract() != 0.0 {
                return err(v.pos(), "expected a whole number");
            }
            Value::Int(f as i64)
        }
        Value::Bool(_) => match v {
            Sexp::Sym(s, _) if s == "true" => Value::Bool(true),
            Sexp::Sym(s, _) if s == "false" => Value::Bool(false),
            other => return err(other.pos(), format!("expected true or false, found {}", other.describe())),
        },
        Value::Choice(_) => {
            let (Sexp::Sym(s, pos) | Sexp::Str(s, pos)) = v else {
                return err(v.pos(), format!("expected a choice, found {}", v.describe()));
            };
            if let ParamKind::Choice(options) = kind {
                if !options.iter().any(|(k, _)| k == s) {
                    let list: Vec<&str> = options.iter().map(|(k, _)| *k).collect();
                    return err(*pos, format!("`{s}` is not one of: {}", list.join(", ")));
                }
            }
            Value::Choice(s.clone())
        }
        Value::Color(_) => {
            let Sexp::List(items, pos) = v else { return err(v.pos(), "expected a color `(r g b a)`") };
            if items.len() != 3 && items.len() != 4 {
                return err(*pos, "expected a color `(r g b)` or `(r g b a)`");
            }
            let mut c = [1.0f32; 4];
            for (k, it) in items.iter().enumerate() {
                c[k] = num(it)?.parse().unwrap();
            }
            Value::Color(c)
        }
        Value::Text(_) => match v {
            Sexp::Str(s, _) => Value::Text(s.clone()),
            other => return err(other.pos(), format!("expected a string, found {}", other.describe())),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Link;

    fn tiny(name: &str) -> Arc<SourceImage> {
        Arc::new(SourceImage::from_rgba8(name, 2, 1, vec![255, 0, 0, 255, 0, 0, 255, 128]))
    }
    fn images(path: &str) -> Result<Arc<SourceImage>, String> {
        if path.contains("missing") { Err(format!("{path} not found")) } else { Ok(tiny(path)) }
    }
    fn round_trip(text: &str) -> String {
        to_text(&from_text(text, &images).unwrap()).unwrap()
    }
    /// Collapses line breaks and indentation (the layout depends on width).
    fn flat(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn parses_atoms_strings_comments_and_positions() {
        let forms = parse("; c\n(a 1 -2.5 :k \"x\\\"y\\n\") ; trailing\n(b)").unwrap();
        assert_eq!(forms.len(), 2);
        let Sexp::List(items, pos) = &forms[0] else { panic!() };
        assert_eq!(*pos, Pos { line: 2, col: 1 });
        assert_eq!(items[1], Sexp::Num("1".into(), Pos { line: 2, col: 4 }));
        assert!(matches!(&items[2], Sexp::Num(n, _) if n == "-2.5"));
        assert!(matches!(&items[3], Sexp::Sym(s, _) if s == ":k"));
        assert!(matches!(&items[4], Sexp::Str(s, _) if s == "x\"y\n"));
    }

    #[test]
    fn syntax_errors_report_positions() {
        assert_eq!(parse("(a (b)").unwrap_err().message, "unclosed `(`");
        let e = parse("(a))").unwrap_err();
        assert_eq!((e.line, e.col, e.message.as_str()), (1, 4, "unexpected `)`"));
        assert!(parse("(a \"x)").unwrap_err().message.contains("unterminated"));
        assert!(parse("(a \"\\q\")").unwrap_err().message.contains("escape"));
    }

    #[test]
    fn nested_expression_builds_graph() {
        let g = from_text("(output (invert (noise :seed 7)))", &images).unwrap();
        let inv = g.nodes[&g.output].input_node(0).unwrap();
        assert_eq!(g.nodes[&inv].kind, "invert");
        let noise = g.nodes[&inv].input_node(0).unwrap();
        assert_eq!(g.nodes[&noise].params.i("seed"), 7);
        assert_eq!(g.nodes.len(), 3);
    }

    #[test]
    fn shared_nodes_round_trip_through_def() {
        let text = "(def p (image \"inputs/p.png\" :at (0 0)))\n(output (blend p (invert p :at (1 1)) :mode screen :at (2 2)) :at (3 3))\n";
        let g = from_text(text, &images).unwrap();
        assert_eq!(g.nodes.len(), 4);
        let out = round_trip(text);
        assert!(out.contains("(def p (image \"inputs/p.png\" :at (0 0)))"), "{out}");
        assert!(out.contains("(blend p (invert p :at (1 1)) :mode screen :opacity 1 :at (2 2))"), "{out}");
        assert_eq!(round_trip(&out), out);
    }

    #[test]
    fn stacks_are_lists_of_layers() {
        let g = from_text("(output (stack (noise) (checker) (solid) _))", &images).unwrap();
        let head = g.nodes[&g.output].input_node(0).unwrap();
        let chain = g.stack_chain(head).unwrap();
        // Three stack nodes; the empty back slot is not counted as a layer.
        assert_eq!(chain.stacks.len(), 3);
        assert_eq!(chain.layers.len(), 3);
        assert_eq!(g.nodes[&chain.stacks[2]].inputs[1], None);
        let text = to_text(&g).unwrap();
        assert!(flat(&text).contains("(stack (noise"), "{text}");
        let g2 = from_text(&text, &images).unwrap();
        let chain2 = g2.stack_chain(g2.nodes[&g2.output].input_node(0).unwrap()).unwrap();
        assert_eq!((chain2.stacks.len(), chain2.layers.len()), (3, 3));
        // A single layer is a one-node stack.
        let g = from_text("(output (stack (noise)))", &images).unwrap();
        let chain = g.stack_chain(g.nodes[&g.output].input_node(0).unwrap()).unwrap();
        assert_eq!((chain.stacks.len(), chain.layers.len()), (1, 1));
        // Three layers without an empty slot use two stack nodes.
        let g = from_text("(output (stack (noise) (checker) (solid)))", &images).unwrap();
        let chain = g.stack_chain(g.nodes[&g.output].input_node(0).unwrap()).unwrap();
        assert_eq!((chain.stacks.len(), chain.layers.len()), (2, 3));
        assert!(flat(&to_text(&g).unwrap()).contains("(stack (noise"));
    }

    #[test]
    fn detached_nodes_and_empty_ports_are_kept() {
        let text = "(output :at (0 0))\n(blend _ (solid :at (5 5)) :at (9 9))\n(invert :at (7 7))\n";
        let out = round_trip(text);
        assert_eq!(
            flat(&out),
            "; Delos expression (output :canvas_w 1920 :canvas_h 1080 :at (0 0)) \
             (blend _ (solid :color (0.18 0.45 0.85 1) :at (5 5)) :mode multiply :opacity 1 :at (9 9)) \
             (invert :at (7 7))"
        );
        assert_eq!(round_trip(&out), out);
    }

    #[test]
    fn all_value_types_round_trip_exactly() {
        let text = concat!(
            "(output (blend (solid :color (0.1 0.2 0.30000001 0.5) :at (0 0)) ",
            "(remove_background :threshold 123 :binary true :model \"a \\\"b\\\"\" :at (0 0)) ",
            ":mode hard_light :opacity 0.3333333333333333 :at (0 0)) :canvas_w 640 :at (0 0))\n"
        );
        let g = from_text(text, &images).unwrap();
        let printed = to_text(&g).unwrap();
        let g2 = from_text(&printed, &images).unwrap();
        let ps = |g: &Graph| g.nodes.values().map(|n| (n.kind, n.params.clone())).collect::<Vec<_>>();
        assert_eq!(ps(&g), ps(&g2));
        assert_eq!(to_text(&g2).unwrap(), printed);
    }

    #[test]
    fn every_parameter_is_written_and_titles_kept() {
        let out = round_trip("(output (noise :title \"Clouds\" :at (1 2)) :at (0 0))");
        let noise = crate::nodes::kind("noise").unwrap();
        for def in noise.params() {
            assert!(out.contains(&format!(":{} ", def.key)), "missing :{} in {out}", def.key);
        }
        assert!(out.contains(":octaves 6"), "defaults are written: {out}");
        assert!(out.contains(":title \"Clouds\""), "{out}");
        // Omitted parameters read back as defaults.
        let g = from_text("(output (noise))", &images).unwrap();
        let n = g.nodes.values().find(|n| n.kind == "noise").unwrap();
        assert_eq!(n.params, crate::nodes::default_params(noise));
    }

    #[test]
    fn semantic_errors_are_reported() {
        let e = |t: &str| from_text(t, &images).err().expect("should fail");
        assert!(e("(output (sparkle))").message.contains("unknown node type `sparkle`"));
        assert!(e("(output (invert :zoom 2))").message.contains("no parameter `:zoom`"));
        assert!(e("(output (invert x))").message.contains("unknown name `x`"));
        assert!(e("(output (invert (noise) (noise)))").message.contains("takes 1 input"));
        assert!(e("(output (noise :octaves 2.5))").message.contains("whole number"));
        assert!(e("(output (blend _ _ :mode sparkly))").message.contains("not one of"));
        assert!(e("(output) (output)").message.contains("only one"));
        assert!(e("(def a (noise)) (def a (noise))").message.contains("already defined"));
        assert!(e("(output (image \"inputs/missing.png\"))").message.contains("not found"));
        assert!(e("(output (image))").message.contains("expected `(image"));
        assert!(e("(output (invert (output)))").message.contains("top level"));
        assert!(e("(output (noise :seed))").message.contains("needs a value"));
        assert!(e("(output (noise :seed 1 :seed 2))").message.contains("twice"));
        let pos = e("(output\n  (sparkle))");
        assert_eq!((pos.line, pos.col), (2, 4));
    }

    #[test]
    fn split_outputs_are_referred_to_by_name() {
        let text = "(def s (split_vertical (image \"inputs/p.png\") :position 0.3))\n(output (stack (invert s.left) s.right))";
        let g = from_text(text, &images).unwrap();
        let split = g.nodes.values().find(|n| n.kind == "split_vertical").unwrap().id;
        let stack = g.nodes.values().find(|n| n.kind == "stack").unwrap();
        assert_eq!(stack.inputs[1], Some(Link::new(split, 1)));
        let inv = stack.input_node(0).unwrap();
        assert_eq!(g.nodes[&inv].inputs[0], Some(Link::new(split, 0)));
        let out = to_text(&g).unwrap();
        assert!(flat(&out).contains("(stack (invert split-vertical.left"), "{out}");
        assert!(flat(&out).contains("split-vertical.right"), "{out}");
        assert_eq!(round_trip(&out), out);
    }

    #[test]
    fn a_split_used_once_is_still_named() {
        let g = from_text("(def s (split_rect (noise)))\n(output s.outside)", &images).unwrap();
        let out = to_text(&g).unwrap();
        assert!(out.starts_with("; Delos expression\n(def split-rect\n  (split_rect"), "{out}");
        assert!(flat(&out).contains("(output split-rect.outside"), "{out}");
        // An unused split is written like any detached node.
        let g = from_text("(split_horizontal (noise))", &images).unwrap();
        assert!(flat(&to_text(&g).unwrap()).contains("(split_horizontal (noise"));
    }

    #[test]
    fn split_reference_errors() {
        let e = |t: &str| from_text(t, &images).err().expect("should fail").message;
        assert!(e("(def s (split_vertical (noise))) (output s)").contains("write `s.left` or `s.right`"));
        assert!(e("(def s (split_vertical (noise))) (output s.middle)").contains("no output `middle` (outputs: left, right)"));
        assert!(e("(output (invert (split_vertical (noise))))").contains("bind it with `def`"));
        assert!(e("(def a.b (noise))").contains("cannot be used as a name"));
        // A single output may also be named explicitly.
        assert!(from_text("(def n (noise)) (output n.image)", &images).is_ok());
    }

    #[test]
    fn images_need_files_to_serialize() {
        let mut g = Graph::new();
        let iid = g.add_image(tiny("x"));
        let n = g.add_node(ids::IMAGE, (0.0, 0.0));
        g.set_param(n, IMAGE_PARAM, Value::Int(iid as i64));
        assert!(to_text(&g).unwrap_err().contains("no file"));
    }

    #[test]
    fn nodes_without_positions_are_laid_out() {
        let g = from_text("(output (invert (noise)))", &images).unwrap();
        let xs: HashSet<i64> = g.nodes.values().map(|n| n.pos.0 as i64).collect();
        assert_eq!(xs.len(), 3, "each depth gets its own column");
    }
}
