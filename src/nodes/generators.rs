//! Procedural generators. They take no inputs and have no size: they render
//! at whatever extent and resolution they are requested at.

use super::{Axis, Category, NodeKind, Overlay, ParamDef, Params};
use crate::engine::geom::{Request, Size};
use crate::engine::kernels::{self, Kernel};
use crate::engine::tile::Tile;
use crate::engine::Ctx;
use crate::graph::Node;

pub struct Generator {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    kernel: &'static Kernel,
    params: fn() -> Vec<ParamDef>,
    pack: fn(&Params) -> Vec<f32>,
    overlays: fn() -> Vec<Overlay>,
}

impl NodeKind for Generator {
    fn id(&self) -> &'static str {
        self.id
    }
    fn name(&self) -> &'static str {
        self.name
    }
    fn category(&self) -> Category {
        Category::Generator
    }
    fn description(&self) -> &'static str {
        self.description
    }
    fn params(&self) -> Vec<ParamDef> {
        (self.params)()
    }
    fn size(&self, _node: &Node, _inputs: &[Option<Size>]) -> Option<Size> {
        None
    }
    fn overlays(&self, _node: &Node) -> Vec<Overlay> {
        (self.overlays)()
    }
    fn render(&self, ctx: &Ctx, node: &Node, req: &Request) -> anyhow::Result<Tile> {
        ctx.kernel(self.kernel, vec![], req, &(self.pack)(&node.params))
    }
}

fn colors(p: &Params) -> Vec<f32> {
    let mut v = p.color("color_a").to_vec();
    v.extend(p.color("color_b"));
    v
}

const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

fn color_pair(a: [f32; 4], b: [f32; 4]) -> Vec<ParamDef> {
    vec![ParamDef::color("color_a", "Color A", a), ParamDef::color("color_b", "Color B", b)]
}

const NOISE_TYPES: &[(&str, &str)] = &[("fbm", "fBm"), ("turbulence", "Turbulence"), ("ridged", "Ridged")];
const VORONOI_MODES: &[(&str, &str)] = &[("distance", "Distance"), ("cells", "Flat cells"), ("edges", "Edges")];

static SOLID: Generator = Generator {
    id: "solid",
    name: "Solid Color",
    description: "Fills the canvas with a single color.",
    kernel: &kernels::SOLID,
    params: || vec![ParamDef::color("color", "Color", [0.18, 0.45, 0.85, 1.0])],
    pack: |p| p.color("color").to_vec(),
    overlays: Vec::new,
};

static LINEAR: Generator = Generator {
    id: "linear_gradient",
    name: "Linear Gradient",
    description: "A gradient between two colors along a line. Drag the end points in the image view.",
    kernel: &kernels::LINEAR_GRADIENT,
    params: || {
        let mut v = color_pair(BLACK, WHITE);
        v.extend([
            ParamDef::fraction("x0", "Start X", Axis::X, 0.0),
            ParamDef::fraction("y0", "Start Y", Axis::Y, 0.5),
            ParamDef::fraction("x1", "End X", Axis::X, 1.0),
            ParamDef::fraction("y1", "End Y", Axis::Y, 0.5),
        ]);
        v
    },
    pack: |p| {
        let mut v = colors(p);
        v.extend([p.f("x0") as f32, p.f("y0") as f32, p.f("x1") as f32, p.f("y1") as f32]);
        v
    },
    overlays: || vec![Overlay::Point { x: "x0", y: "y0", label: "A" }, Overlay::Point { x: "x1", y: "y1", label: "B" }],
};

static RADIAL: Generator = Generator {
    id: "radial_gradient",
    name: "Radial Gradient",
    description: "A circular gradient. Drag the center in the image view.",
    kernel: &kernels::RADIAL_GRADIENT,
    params: || {
        let mut v = color_pair(WHITE, [0.0, 0.0, 0.0, 0.0]);
        v.extend([
            ParamDef::fraction("cx", "Center X", Axis::X, 0.5),
            ParamDef::fraction("cy", "Center Y", Axis::Y, 0.5),
            ParamDef::fraction("radius", "Radius", Axis::Min, 0.5),
        ]);
        v
    },
    pack: |p| {
        let mut v = colors(p);
        v.extend([p.f("cx") as f32, p.f("cy") as f32, p.f("radius") as f32]);
        v
    },
    overlays: || vec![Overlay::Point { x: "cx", y: "cy", label: "C" }],
};

static CHECKER: Generator = Generator {
    id: "checker",
    name: "Checkerboard",
    description: "Alternating squares.",
    kernel: &kernels::CHECKER,
    params: || {
        let mut v = color_pair(WHITE, BLACK);
        v.push(ParamDef::fraction("cell", "Cell size", Axis::Min, 0.1));
        v
    },
    pack: |p| {
        let mut v = colors(p);
        v.push(p.f("cell") as f32);
        v
    },
    overlays: Vec::new,
};

static NOISE: Generator = Generator {
    id: "noise",
    name: "Noise",
    description: "Fractal Perlin noise (fBm, turbulence or ridged).",
    kernel: &kernels::NOISE,
    params: || {
        let mut v = color_pair(BLACK, WHITE);
        v.extend([
            ParamDef::choice("type", "Type", NOISE_TYPES, "fbm"),
            ParamDef::fraction("scale", "Feature size", Axis::Min, 0.25),
            ParamDef::int("octaves", "Octaves", 1, 12, 6),
            ParamDef::float("persistence", "Persistence", 0.0, 1.0, 0.01, 0.5),
            ParamDef::float("lacunarity", "Lacunarity", 1.0, 4.0, 0.01, 2.0),
            ParamDef::float("contrast", "Contrast", 0.1, 5.0, 0.01, 1.2),
            ParamDef::int("seed", "Seed", 0, 1_000_000, 1),
        ]);
        v
    },
    pack: |p| {
        let mut v = colors(p);
        let ty = p.choice_index("type", NOISE_TYPES) as f32;
        v.extend([
            p.f("scale") as f32,
            p.f("octaves") as f32,
            p.f("persistence") as f32,
            p.f("lacunarity") as f32,
            p.f("seed") as f32,
            ty,
            p.f("contrast") as f32,
        ]);
        v
    },
    overlays: Vec::new,
};

static VORONOI: Generator = Generator {
    id: "voronoi",
    name: "Voronoi",
    description: "Worley / Voronoi cell patterns.",
    kernel: &kernels::VORONOI,
    params: || {
        let mut v = color_pair(BLACK, WHITE);
        v.extend([
            ParamDef::choice("mode", "Mode", VORONOI_MODES, "distance"),
            ParamDef::fraction("cell", "Cell size", Axis::Min, 0.1),
            ParamDef::float("jitter", "Jitter", 0.0, 1.0, 0.01, 1.0),
            ParamDef::int("seed", "Seed", 0, 1_000_000, 1),
        ]);
        v
    },
    pack: |p| {
        let mut v = colors(p);
        v.extend([p.f("cell") as f32, p.f("jitter") as f32, p.f("seed") as f32, p.choice_index("mode", VORONOI_MODES) as f32]);
        v
    },
    overlays: Vec::new,
};

static MANDELBROT: Generator = Generator {
    id: "mandelbrot",
    name: "Mandelbrot",
    description: "The Mandelbrot fractal with a cosine palette.",
    kernel: &kernels::MANDELBROT,
    params: || {
        vec![
            ParamDef::float("cx", "Center (re)", -2.5, 1.5, 0.001, -0.6),
            ParamDef::float("cy", "Center (im)", -2.0, 2.0, 0.001, 0.0),
            ParamDef::float("zoom", "Zoom", 0.1, 100000.0, 0.1, 1.0),
            ParamDef::int("iterations", "Iterations", 10, 5000, 256),
            ParamDef::float("phase_r", "Palette R", 0.0, 1.0, 0.01, 0.0),
            ParamDef::float("phase_g", "Palette G", 0.0, 1.0, 0.01, 0.1),
            ParamDef::float("phase_b", "Palette B", 0.0, 1.0, 0.01, 0.2),
            ParamDef::float("interior", "Interior", 0.0, 1.0, 0.01, 0.0),
        ]
    },
    pack: |p| {
        ["cx", "cy", "zoom", "iterations", "phase_r", "phase_g", "phase_b", "interior"].iter().map(|k| p.f(k) as f32).collect()
    },
    overlays: Vec::new,
};

pub fn kinds() -> Vec<&'static dyn NodeKind> {
    vec![&SOLID, &LINEAR, &RADIAL, &CHECKER, &NOISE, &VORONOI, &MANDELBROT]
}
