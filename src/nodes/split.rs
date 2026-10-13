//! Splits: nodes with two outputs that divide their input along a border.
//! One output keeps the pixels on one side (the rest transparent), the
//! other keeps the complement, so effects can be applied to one part only
//! (like a selection) and the parts stacked back together.

use super::basic::input_request;
use super::{Axis, Category, EditView, NodeKind, Overlay, ParamDef};
use crate::engine::geom::{Rect, Request, Size};
use crate::engine::kernels;
use crate::engine::tile::Tile;
use crate::engine::{Ctx, KernelInput};
use crate::graph::Node;

pub fn kinds() -> Vec<&'static dyn NodeKind> {
    vec![&VERTICAL, &HORIZONTAL, &RECTANGLE]
}

/// Far enough to mean "unbounded" in node units.
const FAR: f64 = 1e9;

pub struct Split {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    /// Output 0 is the part inside the region, output 1 the rest.
    outputs: &'static [&'static str],
    params: fn() -> Vec<ParamDef>,
    overlays: fn() -> Vec<Overlay>,
    /// The region kept by output 0, in node units, for a node of `extent`.
    region: fn(&Node, Size) -> Rect,
}

static VERTICAL: Split = Split {
    id: "split_vertical",
    name: "Vertical Split",
    description: "Splits the image along a vertical line: one output keeps the left part, the other the right part. Drag the line in the image view.",
    outputs: &["left", "right"],
    params: || vec![ParamDef::fraction("position", "Border", Axis::X, 0.5)],
    overlays: || vec![Overlay::Line { position: "position", horizontal: false, sides: ["left", "right"] }],
    region: |n, e| Rect::new(-FAR, -FAR, FAR + (n.params.f("position") * e.w).round(), 2.0 * FAR),
};

static HORIZONTAL: Split = Split {
    id: "split_horizontal",
    name: "Horizontal Split",
    description: "Splits the image along a horizontal line: one output keeps the top part, the other the bottom part. Drag the line in the image view.",
    outputs: &["top", "bottom"],
    params: || vec![ParamDef::fraction("position", "Border", Axis::Y, 0.5)],
    overlays: || vec![Overlay::Line { position: "position", horizontal: true, sides: ["top", "bottom"] }],
    region: |n, e| Rect::new(-FAR, -FAR, 2.0 * FAR, FAR + (n.params.f("position") * e.h).round()),
};

static RECTANGLE: Split = Split {
    id: "split_rect",
    name: "Rectangle Split",
    description: "Splits the image into the part inside a rectangle and the part outside it. Drag or resize the rectangle in the image view.",
    outputs: &["inside", "outside"],
    params: || {
        vec![
            ParamDef::fraction("x", "Left", Axis::X, 0.25),
            ParamDef::fraction("y", "Top", Axis::Y, 0.25),
            ParamDef::fraction("w", "Width", Axis::X, 0.5),
            ParamDef::fraction("h", "Height", Axis::Y, 0.5),
        ]
    },
    overlays: || vec![Overlay::Rect { x: "x", y: "y", w: "w", h: "h" }],
    region: |n, e| {
        let p = &n.params;
        let (x0, y0) = ((p.f("x") * e.w).round(), (p.f("y") * e.h).round());
        let (x1, y1) = (((p.f("x") + p.f("w")) * e.w).round(), ((p.f("y") + p.f("h")) * e.h).round());
        Rect::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
    },
};

impl NodeKind for Split {
    fn id(&self) -> &'static str {
        self.id
    }
    fn name(&self) -> &'static str {
        self.name
    }
    fn category(&self) -> Category {
        Category::Split
    }
    fn description(&self) -> &'static str {
        self.description
    }
    fn inputs(&self) -> &'static [&'static str] {
        &["image"]
    }
    fn outputs(&self) -> &'static [&'static str] {
        self.outputs
    }
    fn params(&self) -> Vec<ParamDef> {
        (self.params)()
    }
    fn size(&self, _node: &Node, inputs: &[Option<Size>]) -> Option<Size> {
        inputs[0]
    }
    fn overlays(&self, _node: &Node) -> Vec<Overlay> {
        (self.overlays)()
    }
    /// The border is placed on the whole input.
    fn edit_view(&self) -> EditView {
        EditView::Input(0)
    }
    fn render(&self, ctx: &Ctx, node: &Node, req: &Request) -> anyhow::Result<Tile> {
        self.render_output(ctx, node, 0, req)
    }
    fn render_output(&self, ctx: &Ctx, node: &Node, output: usize, req: &Request) -> anyhow::Result<Tile> {
        let region = (self.region)(node, req.extent);
        let keep_inside = output == 0;
        // Pixel centres the request covers, in node units.
        let (sx, sy) = (req.step_x(), req.step_y());
        let first = (req.region.x + sx / 2.0, req.region.y + sy / 2.0);
        let last = (req.region.right() - sx / 2.0, req.region.bottom() - sy / 2.0);
        let contains = |x: f64, y: f64| x >= region.x && x < region.right() && y >= region.y && y < region.bottom();
        let all_inside = contains(first.0, first.1) && contains(last.0, last.1);
        let none_inside = last.0 < region.x || first.0 >= region.right() || last.1 < region.y || first.1 >= region.bottom();
        // Whole tile on one side: no per-pixel work.
        if (all_inside && !keep_inside) || (none_inside && keep_inside) {
            return Ok(Tile::Clear { w: req.width, h: req.height });
        }
        let input = ctx.render_input(node, 0, &input_request(ctx, node, 0, req))?;
        if all_inside || none_inside || input.is_clear() {
            return Ok(input);
        }
        let r = region;
        let params = [r.x as f32, r.y as f32, r.right() as f32, r.bottom() as f32, if keep_inside { 0.0 } else { 1.0 }];
        ctx.kernel(&kernels::SPLIT, vec![KernelInput::aligned(input)], req, &params)
    }
}
