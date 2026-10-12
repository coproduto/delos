use super::{Axis, Category, EditView, NodeKind, Overlay, ParamDef, Value};
use crate::engine::geom::{Map2, Request, Size};
use crate::engine::kernels::{self, BLEND_MODES};
use crate::engine::tile::Tile;
use crate::engine::{Ctx, KernelInput};
use crate::graph::Node;

pub const IMAGE_PARAM: &str = "image";

pub fn kinds() -> Vec<&'static dyn NodeKind> {
    vec![&Output, &StaticImage, &Stack, &Disable, &Invert, &Blend, &Reflect, &Clip]
}

/// Request for input `port`: same grid, but with the input's own extent when
/// it has a definite size.
pub fn input_request(ctx: &Ctx, node: &Node, port: usize, req: &Request) -> Request {
    Request { extent: ctx.input_size(node, port).unwrap_or(req.extent), ..*req }
}

/// Size of a node that places its inputs on a shared, top-left aligned
/// canvas: the union of all definite input sizes.
fn union_size(inputs: &[Option<Size>]) -> Option<Size> {
    inputs.iter().flatten().fold(None, |acc: Option<Size>, s| Some(acc.map_or(*s, |a| Size::new(a.w.max(s.w), a.h.max(s.h)))))
}

// ---------------------------------------------------------------------------

pub struct Output;

impl NodeKind for Output {
    fn id(&self) -> &'static str {
        "output"
    }
    fn name(&self) -> &'static str {
        "Output"
    }
    fn category(&self) -> Category {
        Category::Output
    }
    fn description(&self) -> &'static str {
        "The final image. The canvas size applies when the connected node has no size of its own (generators)."
    }
    fn inputs(&self) -> &'static [&'static str] {
        &["image"]
    }
    fn params(&self) -> Vec<ParamDef> {
        vec![ParamDef::int("canvas_w", "Canvas width", 1, 65536, 1920), ParamDef::int("canvas_h", "Canvas height", 1, 65536, 1080)]
    }
    fn size(&self, node: &Node, inputs: &[Option<Size>]) -> Option<Size> {
        Some(inputs[0].unwrap_or(Size::new(node.params.f("canvas_w").max(1.0), node.params.f("canvas_h").max(1.0))))
    }
    fn render(&self, ctx: &Ctx, node: &Node, req: &Request) -> anyhow::Result<Tile> {
        ctx.render_input(node, 0, &input_request(ctx, node, 0, req))
    }
    fn user_creatable(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------

pub struct StaticImage;

impl NodeKind for StaticImage {
    fn id(&self) -> &'static str {
        "image"
    }
    fn name(&self) -> &'static str {
        "Static Image"
    }
    fn category(&self) -> Category {
        Category::Source
    }
    fn description(&self) -> &'static str {
        "An imported image."
    }
    fn params(&self) -> Vec<ParamDef> {
        vec![ParamDef::hidden(IMAGE_PARAM, Value::Int(0)), ParamDef::hidden("width", Value::Int(1)), ParamDef::hidden("height", Value::Int(1))]
    }
    fn size(&self, node: &Node, _inputs: &[Option<Size>]) -> Option<Size> {
        Some(Size::new(node.params.f("width"), node.params.f("height")))
    }
    fn render(&self, ctx: &Ctx, node: &Node, req: &Request) -> anyhow::Result<Tile> {
        let id = node.params.i(IMAGE_PARAM) as u64;
        let img = ctx.graph.images.get(&id).ok_or_else(|| anyhow::anyhow!("image data missing"))?;
        Ok(Tile::Cpu(img.render(req)))
    }
    fn user_creatable(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------

pub struct Stack;

impl NodeKind for Stack {
    fn id(&self) -> &'static str {
        "stack"
    }
    fn name(&self) -> &'static str {
        "Layer Stack"
    }
    fn category(&self) -> Category {
        Category::Combine
    }
    fn description(&self) -> &'static str {
        "Places the front layer over the back layer. Chains of stacks are shown as a single layer stack."
    }
    fn inputs(&self) -> &'static [&'static str] {
        &["front", "back"]
    }
    fn size(&self, _node: &Node, inputs: &[Option<Size>]) -> Option<Size> {
        union_size(inputs)
    }
    fn render(&self, ctx: &Ctx, node: &Node, req: &Request) -> anyhow::Result<Tile> {
        let front = ctx.render_input(node, 0, &input_request(ctx, node, 0, req))?;
        let back = ctx.render_input(node, 1, &input_request(ctx, node, 1, req))?;
        if front.is_clear() {
            return Ok(back);
        }
        if back.is_clear() {
            return Ok(front);
        }
        ctx.kernel(&kernels::OVER, vec![KernelInput::aligned(front), KernelInput::aligned(back)], req, &[])
    }
    fn combines_inputs(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------

pub struct Disable;

impl NodeKind for Disable {
    fn id(&self) -> &'static str {
        "disable"
    }
    fn name(&self) -> &'static str {
        "Disable"
    }
    fn category(&self) -> Category {
        Category::Filter
    }
    fn description(&self) -> &'static str {
        "Makes every pixel transparent (used to hide a layer without removing it)."
    }
    fn inputs(&self) -> &'static [&'static str] {
        &["image"]
    }
    fn size(&self, _node: &Node, inputs: &[Option<Size>]) -> Option<Size> {
        inputs[0]
    }
    fn render(&self, _ctx: &Ctx, _node: &Node, req: &Request) -> anyhow::Result<Tile> {
        Ok(Tile::Clear { w: req.width, h: req.height })
    }
}

// ---------------------------------------------------------------------------

pub struct Invert;

impl NodeKind for Invert {
    fn id(&self) -> &'static str {
        "invert"
    }
    fn name(&self) -> &'static str {
        "Invert"
    }
    fn category(&self) -> Category {
        Category::Filter
    }
    fn description(&self) -> &'static str {
        "Inverts colors: (R, G, B) → (1−R, 1−G, 1−B)."
    }
    fn inputs(&self) -> &'static [&'static str] {
        &["image"]
    }
    fn size(&self, _node: &Node, inputs: &[Option<Size>]) -> Option<Size> {
        inputs[0]
    }
    fn render(&self, ctx: &Ctx, node: &Node, req: &Request) -> anyhow::Result<Tile> {
        let t = ctx.render_input(node, 0, &input_request(ctx, node, 0, req))?;
        if t.is_clear() {
            return Ok(t);
        }
        ctx.kernel(&kernels::INVERT, vec![KernelInput::aligned(t)], req, &[])
    }
}

// ---------------------------------------------------------------------------

pub struct Blend;

impl NodeKind for Blend {
    fn id(&self) -> &'static str {
        "blend"
    }
    fn name(&self) -> &'static str {
        "Blend"
    }
    fn category(&self) -> Category {
        Category::Combine
    }
    fn description(&self) -> &'static str {
        "Combines a layer with a base image using a blend mode (including logic gates)."
    }
    fn inputs(&self) -> &'static [&'static str] {
        &["base", "layer"]
    }
    fn params(&self) -> Vec<ParamDef> {
        vec![ParamDef::choice("mode", "Mode", BLEND_MODES, "multiply"), ParamDef::float("opacity", "Opacity", 0.0, 1.0, 0.01, 1.0)]
    }
    fn size(&self, _node: &Node, inputs: &[Option<Size>]) -> Option<Size> {
        inputs[0].or(inputs[1])
    }
    fn render(&self, ctx: &Ctx, node: &Node, req: &Request) -> anyhow::Result<Tile> {
        let base = ctx.render_input(node, 0, &input_request(ctx, node, 0, req))?;
        let layer = ctx.render_input(node, 1, &input_request(ctx, node, 1, req))?;
        if layer.is_clear() {
            return Ok(base);
        }
        let mode = node.params.choice_index("mode", BLEND_MODES) as f32;
        let opacity = node.params.f("opacity") as f32;
        ctx.kernel(&kernels::BLEND, vec![KernelInput::aligned(base), KernelInput::aligned(layer)], req, &[mode, opacity])
    }
    fn combines_inputs(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------

pub struct Reflect;

const ORIENTATIONS: &[(&str, &str)] = &[("vertical", "Vertical axis (mirror left/right)"), ("horizontal", "Horizontal axis (mirror top/bottom)")];
const SIDES: &[(&str, &str)] = &[("low_to_high", "Copy left/top onto right/bottom"), ("high_to_low", "Copy right/bottom onto left/top")];
const REFLECT_MODES: &[(&str, &str)] = &[("replace", "Replace"), ("additive", "Additive"), ("subtractive", "Subtractive")];

impl NodeKind for Reflect {
    fn id(&self) -> &'static str {
        "reflect"
    }
    fn name(&self) -> &'static str {
        "Reflection"
    }
    fn category(&self) -> Category {
        Category::Filter
    }
    fn description(&self) -> &'static str {
        "Mirrors one side of an axis onto the other side. Drag the axis in the image view."
    }
    fn inputs(&self) -> &'static [&'static str] {
        &["image"]
    }
    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::choice("orientation", "Axis", ORIENTATIONS, "vertical"),
            ParamDef::fraction("position", "Axis position", Axis::X, 0.5),
            ParamDef::choice("side", "Direction", SIDES, "low_to_high"),
            ParamDef::choice("mode", "Mode", REFLECT_MODES, "replace"),
        ]
    }
    fn size(&self, _node: &Node, inputs: &[Option<Size>]) -> Option<Size> {
        inputs[0]
    }
    fn overlays(&self, _node: &Node) -> Vec<Overlay> {
        vec![Overlay::Axis { orientation: "orientation", position: "position" }]
    }
    fn render(&self, ctx: &Ctx, node: &Node, req: &Request) -> anyhow::Result<Tile> {
        let ireq = input_request(ctx, node, 0, req);
        let direct = ctx.render_input(node, 0, &ireq)?;
        let p = &node.params;
        let horizontal = p.choice("orientation") == "horizontal";
        let lim = if horizontal { req.extent.h } else { req.extent.w };
        let axis = p.f("position") * lim;
        let high_to_low = p.choice("side") == "high_to_low";
        // Destination interval along the axis direction (where mirrored
        // pixels land and have a valid source inside the canvas).
        let (d0, d1) = if high_to_low { ((2.0 * axis - lim).max(0.0), axis) } else { (axis, (2.0 * axis).min(lim)) };
        let (r0, r1) = if horizontal { (req.region.y, req.region.bottom()) } else { (req.region.x, req.region.right()) };
        if d1 <= d0 || r1 <= d0 || r0 >= d1 {
            return Ok(direct);
        }
        let mirror = if horizontal { Map2::new(1.0, 0.0, -1.0, 2.0 * axis) } else { Map2::new(-1.0, 2.0 * axis, 1.0, 0.0) };
        let mreq = ireq.mapped(&mirror, ireq.extent);
        let mirrored = ctx.render_input(node, 0, &mreq)?;
        let map = req.texel_map_to(&mirror, &mreq);
        let mode = p.choice_index("mode", REFLECT_MODES) as f32;
        ctx.kernel(
            &kernels::REFLECT,
            vec![KernelInput::aligned(direct), KernelInput { tile: mirrored, map, mode: crate::engine::tile::SampleMode::Nearest }],
            req,
            &[horizontal as u8 as f32, axis as f32, high_to_low as u8 as f32, mode],
        )
    }
}

// ---------------------------------------------------------------------------

pub struct Clip;

impl NodeKind for Clip {
    fn id(&self) -> &'static str {
        "clip"
    }
    fn name(&self) -> &'static str {
        "Clip"
    }
    fn category(&self) -> Category {
        Category::Filter
    }
    fn description(&self) -> &'static str {
        "Keeps a rectangular part of the image. Drag the rectangle in the image view."
    }
    fn inputs(&self) -> &'static [&'static str] {
        &["image"]
    }
    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::fraction("x", "Left", Axis::X, 0.1),
            ParamDef::fraction("y", "Top", Axis::Y, 0.1),
            ParamDef::fraction("w", "Width", Axis::X, 0.8),
            ParamDef::fraction("h", "Height", Axis::Y, 0.8),
        ]
    }
    fn size(&self, node: &Node, inputs: &[Option<Size>]) -> Option<Size> {
        let s = inputs[0]?;
        let p = &node.params;
        Some(Size::new((p.f("w") * s.w).round().max(1.0), (p.f("h") * s.h).round().max(1.0)))
    }
    fn overlays(&self, _node: &Node) -> Vec<Overlay> {
        vec![Overlay::Rect { x: "x", y: "y", w: "w", h: "h" }]
    }
    fn edit_view(&self) -> EditView {
        EditView::Input(0)
    }
    fn input_map(&self, node: &Node, _port: usize, extent: Size, input: Option<Size>) -> (Map2, Size) {
        let p = &node.params;
        let e = input.unwrap_or_else(|| Size::new(extent.w / p.f("w").max(1e-6), extent.h / p.f("h").max(1e-6)));
        let (ox, oy) = ((p.f("x") * e.w).round(), (p.f("y") * e.h).round());
        (Map2::new(1.0, ox, 1.0, oy), e)
    }
    fn render(&self, ctx: &Ctx, node: &Node, req: &Request) -> anyhow::Result<Tile> {
        let (m, e) = self.input_map(node, 0, req.extent, ctx.input_size(node, 0));
        ctx.render_input(node, 0, &req.mapped(&m, e))
    }
}
