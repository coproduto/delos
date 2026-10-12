use super::{Category, NodeKind, ParamDef, Params, Value};
use crate::engine::geom::{Map2, Request, Sampling, Size};
use crate::engine::kernels;
use crate::engine::tile::{clamp_premul, CpuTile, SampleMode, Tile};
use crate::engine::{Ctx, KernelInput};
use crate::graph::Node;
use rustfft::num_complex::Complex32;
use rustfft::FftPlanner;

pub fn kinds() -> Vec<&'static dyn NodeKind> {
    vec![&Resize, &Downscale]
}

const MODES: &[(&str, &str)] = &[("scale", "Scale factor"), ("size", "Exact size")];
const FILTERS: &[(&str, &str)] = &[("nearest", "Nearest neighbour"), ("bilinear", "Bilinear"), ("bicubic", "Bicubic"), ("fourier", "Fourier")];

pub fn resize_to(p: &mut Params, size: Size) {
    p.set("mode", Value::Choice("size".into()));
    p.set("width", Value::Int(size.w.round() as i64));
    p.set("height", Value::Int(size.h.round() as i64));
}

pub struct Resize;

impl Resize {
    fn factors(node: &Node, input: Option<Size>, extent: Size) -> (f64, f64, Size) {
        let p = &node.params;
        if p.choice("mode") == "size" {
            let out = Size::new(p.f("width").max(1.0), p.f("height").max(1.0));
            let inp = input.unwrap_or(out);
            (out.w / inp.w, out.h / inp.h, inp)
        } else {
            let f = p.f("scale").max(1e-4);
            (f, f, input.unwrap_or(Size::new(extent.w / f, extent.h / f)))
        }
    }
}

impl NodeKind for Resize {
    fn id(&self) -> &'static str {
        "resize"
    }
    fn name(&self) -> &'static str {
        "Resize"
    }
    fn category(&self) -> Category {
        Category::Filter
    }
    fn description(&self) -> &'static str {
        "Upscales or downscales with nearest-neighbour, bilinear, bicubic or Fourier interpolation."
    }
    fn inputs(&self) -> &'static [&'static str] {
        &["image"]
    }
    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::choice("mode", "Mode", MODES, "scale"),
            ParamDef::float("scale", "Scale", 0.01, 16.0, 0.01, 2.0).when("mode", &["scale"]),
            ParamDef::int("width", "Width", 1, 65536, 1024).when("mode", &["size"]),
            ParamDef::int("height", "Height", 1, 65536, 1024).when("mode", &["size"]),
            ParamDef::choice("filter", "Interpolation", FILTERS, "bicubic"),
        ]
    }
    fn size(&self, node: &Node, inputs: &[Option<Size>]) -> Option<Size> {
        let p = &node.params;
        if p.choice("mode") == "size" {
            return Some(Size::new(p.f("width").max(1.0), p.f("height").max(1.0)));
        }
        let s = inputs[0]?;
        let f = p.f("scale").max(1e-4);
        Some(Size::new((s.w * f).round().max(1.0), (s.h * f).round().max(1.0)))
    }
    fn input_map(&self, node: &Node, _port: usize, extent: Size, input: Option<Size>) -> (Map2, Size) {
        let (fx, fy, in_extent) = Self::factors(node, input, extent);
        // Exact output sizes are rounded; use the effective factor.
        let (fx, fy) = match self.size(node, &[input]) {
            Some(out) if input.is_some() => (out.w / in_extent.w, out.h / in_extent.h),
            _ => (fx, fy),
        };
        (Map2::new(1.0 / fx, 0.0, 1.0 / fy, 0.0), in_extent)
    }
    fn render(&self, ctx: &Ctx, node: &Node, req: &Request) -> anyhow::Result<Tile> {
        let input = ctx.input_size(node, 0);
        let (to_input, in_extent) = self.input_map(node, 0, req.extent, input);
        let mut mapped = req.mapped(&to_input, in_extent);
        let filter = node.params.choice("filter");

        if filter == "nearest" {
            mapped.sampling = Sampling::Point;
            return ctx.render_input(node, 0, &mapped);
        }
        // Size-less inputs are resolution independent, and when the grid is
        // at least as coarse as the input's pixels the request is simply an
        // area-filtered downscale.
        let (sx, sy) = (mapped.step_x(), mapped.step_y());
        if input.is_none() || (sx >= 1.0 && sy >= 1.0) {
            return ctx.render_input(node, 0, &mapped);
        }
        // Upsampling: fetch the native pixels under the region and
        // interpolate them.
        let Some(native) = Request::native_cover(&mapped.region, 3.0, &in_extent.rect(), in_extent, req.exact) else {
            return Ok(Tile::Clear { w: req.width, h: req.height });
        };
        if native.pixel_count() > 4 * req.pixel_count().max(1 << 16) {
            return ctx.render_input(node, 0, &mapped);
        }
        let tile = ctx.render_input(node, 0, &native)?;
        let map = req.texel_map_to(&to_input, &native);
        if filter == "fourier" {
            ctx.status(Some("Fourier resampling…"));
            let cpu = ctx.to_cpu(tile)?;
            let (nw, nh) = (((cpu.w as f64) / sx.min(1.0)).ceil() as u32, ((cpu.h as f64) / sy.min(1.0)).ceil() as u32);
            let up = fourier_resample(&cpu, nw.max(1), nh.max(1));
            ctx.status(None);
            let k = Map2::new(nw as f64 / cpu.w as f64, 0.0, nh as f64 / cpu.h as f64, 0.0);
            let map = k.after(&map);
            return ctx.kernel(&kernels::RESAMPLE, vec![KernelInput { tile: Tile::Cpu(up), map, mode: SampleMode::Bilinear }], req, &[]);
        }
        let mode = if filter == "bilinear" { SampleMode::Bilinear } else { SampleMode::Bicubic };
        ctx.kernel(&kernels::RESAMPLE, vec![KernelInput { tile, map, mode }], req, &[])
    }
}

/// Band-limited (sinc) resampling by zero-padding / truncating the spectrum,
/// separably along rows then columns.
pub fn fourier_resample(src: &CpuTile, nw: u32, nh: u32) -> CpuTile {
    use rayon::prelude::*;
    let (w, h) = (src.w as usize, src.h as usize);
    let (nw, nh) = (nw as usize, nh as usize);
    let mut planner = FftPlanner::<f32>::new();
    let fw = planner.plan_fft_forward(w);
    let iw = planner.plan_fft_inverse(nw);
    let fh = planner.plan_fft_forward(h);
    let ih = planner.plan_fft_inverse(nh);

    // Rows: w -> nw
    let mut rows = vec![[0.0f32; 4]; nw * h];
    rows.par_chunks_mut(nw).enumerate().for_each(|(y, out)| {
        let mut line = vec![Complex32::default(); w];
        let mut spec = vec![Complex32::default(); nw];
        for c in 0..4 {
            for x in 0..w {
                line[x] = Complex32::new(src.data[y * w + x][c], 0.0);
            }
            fw.process(&mut line);
            resize_spectrum(&line, &mut spec);
            iw.process(&mut spec);
            let k = 1.0 / w as f32;
            for x in 0..nw {
                out[x][c] = spec[x].re * k;
            }
        }
    });
    // Columns: h -> nh
    let mut out = CpuTile::new(nw as u32, nh as u32);
    let cols: Vec<Vec<[f32; 4]>> = (0..nw)
        .into_par_iter()
        .map(|x| {
            let mut line = vec![Complex32::default(); h];
            let mut spec = vec![Complex32::default(); nh];
            let mut col = vec![[0.0f32; 4]; nh];
            for c in 0..4 {
                for y in 0..h {
                    line[y] = Complex32::new(rows[y * nw + x][c], 0.0);
                }
                fh.process(&mut line);
                resize_spectrum(&line, &mut spec);
                ih.process(&mut spec);
                let k = 1.0 / h as f32;
                for y in 0..nh {
                    col[y][c] = spec[y].re * k;
                }
            }
            col
        })
        .collect();
    for (x, col) in cols.into_iter().enumerate() {
        for (y, px) in col.into_iter().enumerate() {
            out.data[y * nw + x] = clamp_premul(px);
        }
    }
    out
}

/// Copies the low frequencies of `src` into `dst` (zero-padding or
/// truncating), splitting the Nyquist bin when it exists.
fn resize_spectrum(src: &[Complex32], dst: &mut [Complex32]) {
    let n = src.len();
    let m = dst.len();
    dst.fill(Complex32::default());
    let half = n.min(m) / 2;
    for k in 0..half {
        dst[k] = src[k];
        if k > 0 {
            dst[m - k] = src[n - k];
        }
    }
    if n.min(m) % 2 == 1 {
        dst[half] = src[half];
        dst[m - half] = src[n - half];
    } else if half > 0 {
        let nyq = src[half] * 0.5;
        dst[half] += nyq;
        dst[m - half] += nyq;
    }
}

// ---------------------------------------------------------------------------

const DOWN_METHODS: &[(&str, &str)] = &[("box", "Box (average)"), ("nearest", "Nearest neighbour")];

pub struct Downscale;

impl NodeKind for Downscale {
    fn id(&self) -> &'static str {
        "downscale"
    }
    fn name(&self) -> &'static str {
        "Downscale"
    }
    fn category(&self) -> Category {
        Category::Filter
    }
    fn description(&self) -> &'static str {
        "Reduces resolution by a factor."
    }
    fn inputs(&self) -> &'static [&'static str] {
        &["image"]
    }
    fn params(&self) -> Vec<ParamDef> {
        vec![ParamDef::float("factor", "Factor", 1.0, 64.0, 0.5, 2.0), ParamDef::choice("method", "Method", DOWN_METHODS, "box")]
    }
    fn size(&self, node: &Node, inputs: &[Option<Size>]) -> Option<Size> {
        let s = inputs[0]?;
        let f = node.params.f("factor").max(1.0);
        Some(Size::new((s.w / f).floor().max(1.0), (s.h / f).floor().max(1.0)))
    }
    fn input_map(&self, node: &Node, _port: usize, extent: Size, input: Option<Size>) -> (Map2, Size) {
        let f = node.params.f("factor").max(1.0);
        let (in_extent, fx, fy) = match (input, self.size(node, &[input])) {
            (Some(i), Some(o)) => (i, i.w / o.w, i.h / o.h),
            _ => (Size::new(extent.w * f, extent.h * f), f, f),
        };
        (Map2::new(fx, 0.0, fy, 0.0), in_extent)
    }
    fn render(&self, ctx: &Ctx, node: &Node, req: &Request) -> anyhow::Result<Tile> {
        let (m, in_extent) = self.input_map(node, 0, req.extent, ctx.input_size(node, 0));
        let mut mapped = req.mapped(&m, in_extent);
        if node.params.choice("method") == "nearest" {
            mapped.sampling = Sampling::Point;
        }
        ctx.render_input(node, 0, &mapped)
    }
}
