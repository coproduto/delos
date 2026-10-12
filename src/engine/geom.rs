//! Geometry primitives and the render request model.
//!
//! Every node lives in its own "node space": a continuous coordinate system
//! measured in the node's native pixels. A [`Request`] asks a node for a
//! regular grid of samples covering a region of that space. Previews request
//! viewport-sized grids, exports request the native (1:1) grid in bands, so
//! the cost of a preview never depends on how large the underlying image is.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size {
    pub w: f64,
    pub h: f64,
}

impl Size {
    pub const fn new(w: f64, h: f64) -> Self {
        Self { w, h }
    }

    pub fn rect(self) -> Rect {
        Rect::new(0.0, 0.0, self.w, self.h)
    }

    pub fn min_dim(self) -> f64 {
        self.w.min(self.h).max(1.0)
    }

    pub fn pixels(self) -> (u32, u32) {
        (self.w.round().max(1.0) as u32, self.h.round().max(1.0) as u32)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> f64 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }

    pub fn is_empty(&self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }

    pub fn intersect(&self, o: &Rect) -> Rect {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = self.right().min(o.right());
        let y1 = self.bottom().min(o.bottom());
        Rect::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
    }

    pub fn translate(&self, dx: f64, dy: f64) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.w, self.h)
    }
}

/// One-dimensional affine map `a * v + b`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Map1 {
    pub a: f64,
    pub b: f64,
}

impl Map1 {
    pub const IDENTITY: Map1 = Map1 { a: 1.0, b: 0.0 };

    pub fn apply(&self, v: f64) -> f64 {
        self.a * v + self.b
    }

    /// `self ∘ inner`: first apply `inner`, then `self`.
    pub fn after(&self, inner: &Map1) -> Map1 {
        Map1 { a: self.a * inner.a, b: self.a * inner.b + self.b }
    }

    pub fn inverse(&self) -> Map1 {
        Map1 { a: 1.0 / self.a, b: -self.b / self.a }
    }
}

/// Separable 2D affine map (no rotation/shear), used for every coordinate
/// mapping in the engine: output pixel → node space, node → input node space,
/// output pixel → input tile texel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Map2 {
    pub x: Map1,
    pub y: Map1,
}

impl Map2 {
    pub const IDENTITY: Map2 = Map2 { x: Map1::IDENTITY, y: Map1::IDENTITY };

    pub fn new(ax: f64, bx: f64, ay: f64, by: f64) -> Self {
        Self { x: Map1 { a: ax, b: bx }, y: Map1 { a: ay, b: by } }
    }

    pub fn after(&self, inner: &Map2) -> Map2 {
        Map2 { x: self.x.after(&inner.x), y: self.y.after(&inner.y) }
    }

    pub fn inverse(&self) -> Map2 {
        Map2 { x: self.x.inverse(), y: self.y.inverse() }
    }

    /// Image of a rectangle (handles negative scales, i.e. mirroring).
    pub fn apply_rect(&self, r: &Rect) -> Rect {
        let x0 = self.x.apply(r.x);
        let x1 = self.x.apply(r.right());
        let y0 = self.y.apply(r.y);
        let y1 = self.y.apply(r.bottom());
        Rect::new(x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs())
    }

    pub fn as_f32(&self) -> [f32; 4] {
        [self.x.a as f32, self.x.b as f32, self.y.a as f32, self.y.b as f32]
    }
}

/// How a node should reduce detail when a request is coarser than its native
/// resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sampling {
    /// Each output pixel represents the average over its footprint (default).
    Area,
    /// Each output pixel is a point sample at its center (nearest-neighbour).
    Point,
}

#[derive(Clone, Copy, Debug)]
pub struct Request {
    /// Logical canvas size of the node being rendered. For nodes with a
    /// definite size this equals that size; size-less generators use it to
    /// normalise their coordinates.
    pub extent: Size,
    /// Region of node space covered by the grid.
    pub region: Rect,
    /// Grid resolution in pixels.
    pub width: u32,
    pub height: u32,
    pub sampling: Sampling,
    /// Export-quality render (as opposed to an interactive preview).
    pub exact: bool,
}

impl Request {
    pub fn new(extent: Size, region: Rect, width: u32, height: u32) -> Self {
        Self { extent, region, width: width.max(1), height: height.max(1), sampling: Sampling::Area, exact: false }
    }

    /// Native 1:1 render of a whole node.
    pub fn native(extent: Size) -> Self {
        let (w, h) = extent.pixels();
        Self::new(extent, extent.rect(), w, h)
    }

    pub fn step_x(&self) -> f64 {
        self.region.w / self.width as f64
    }

    pub fn step_y(&self) -> f64 {
        self.region.h / self.height as f64
    }

    /// Maps continuous output pixel coordinates (pixel `i` has its center at
    /// `i + 0.5`) to node space.
    pub fn pixel_to_node(&self) -> Map2 {
        Map2::new(self.step_x(), self.region.x, self.step_y(), self.region.y)
    }

    pub fn pixel_count(&self) -> u64 {
        self.width as u64 * self.height as u64
    }

    /// Same grid, transformed into another node space through `node_map`
    /// (which maps this node's space to the input's space).
    pub fn mapped(&self, node_map: &Map2, extent: Size) -> Request {
        Request { extent, region: node_map.apply_rect(&self.region), ..*self }
    }

    /// A grid aligned to the integer pixels of the native resolution that
    /// covers `region` (plus `margin` pixels), clipped to `bounds`.
    pub fn native_cover(region: &Rect, margin: f64, bounds: &Rect, extent: Size, exact: bool) -> Option<Request> {
        let x0 = (region.x - margin).floor().max(bounds.x.floor());
        let y0 = (region.y - margin).floor().max(bounds.y.floor());
        let x1 = (region.right() + margin).ceil().min(bounds.right().ceil());
        let y1 = (region.bottom() + margin).ceil().min(bounds.bottom().ceil());
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        let r = Rect::new(x0, y0, x1 - x0, y1 - y0);
        let mut req = Request::new(extent, r, r.w as u32, r.h as u32);
        req.exact = exact;
        Some(req)
    }

    /// Texel mapping from this request's pixels to the pixels of `input`,
    /// given the node-space transform from this node to the input's node.
    pub fn texel_map_to(&self, node_map: &Map2, input: &Request) -> Map2 {
        let in_px_to_node = input.pixel_to_node();
        in_px_to_node.inverse().after(&node_map.after(&self.pixel_to_node()))
    }
}
