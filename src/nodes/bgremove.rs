//! Background removal using the `rembg-rs` crate (U²-Net via ONNX Runtime).
//!
//! The network works at 320×320 internally, so the mask is computed once from
//! a bounded-resolution render of the input, cached by the input's structural
//! hash, and then applied (bilinearly upsampled) to any requested region.
//! Re-rendering, panning and zooming never re-run the network.

use super::basic::input_request;
use super::{Category, NodeKind, ParamDef};
use crate::engine::geom::{Map2, Request, Size};
use crate::engine::kernels;
use crate::engine::tile::{CpuTile, SampleMode, Tile};
use crate::engine::{Ctx, KernelInput};
use crate::graph::Node;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

pub fn kinds() -> Vec<&'static dyn NodeKind> {
    vec![&BackgroundRemoval]
}

const RESOLUTIONS: &[(&str, &str)] = &[("512", "512 px"), ("1024", "1024 px"), ("2048", "2048 px")];

pub struct BackgroundRemoval;

impl NodeKind for BackgroundRemoval {
    fn id(&self) -> &'static str {
        "remove_background"
    }
    fn name(&self) -> &'static str {
        "Background Removal"
    }
    fn category(&self) -> Category {
        Category::Filter
    }
    fn description(&self) -> &'static str {
        "Removes the background with a neural network (rembg-rs). Needs an ONNX model (e.g. u2net.onnx) and the ONNX Runtime library."
    }
    fn inputs(&self) -> &'static [&'static str] {
        &["image"]
    }
    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::int("threshold", "Threshold", 0, 255, 160),
            ParamDef::bool("binary", "Hard edges (binary)", false),
            ParamDef::bool("sticker", "Clean sticker border", false),
            ParamDef::bool("invert", "Keep background instead", false),
            ParamDef::choice("resolution", "Mask resolution", RESOLUTIONS, "1024"),
            ParamDef::text("model", "Model path (.onnx)", ""),
        ]
    }
    fn size(&self, _node: &Node, inputs: &[Option<Size>]) -> Option<Size> {
        inputs[0]
    }
    fn slow_hint(&self) -> Option<&'static str> {
        Some("Runs a neural network the first time its input changes.")
    }
    fn render(&self, ctx: &Ctx, node: &Node, req: &Request) -> anyhow::Result<Tile> {
        let Some(input) = node.inputs[0] else {
            return Ok(Tile::Clear { w: req.width, h: req.height });
        };
        let extent = ctx.input_size(node, 0).unwrap_or(req.extent);
        let p = &node.params;
        let res: f64 = p.choice("resolution").parse().unwrap_or(1024.0);

        let mut h = std::collections::hash_map::DefaultHasher::new();
        ("rembg", ctx.link_hash(input), p.i("threshold"), p.b("binary"), p.b("sticker"), p.choice("resolution"), p.text("model")).hash(&mut h);
        extent.w.to_bits().hash(&mut h);
        extent.h.to_bits().hash(&mut h);
        let key = h.finish();

        let mask = match ctx.engine.cache.get::<CpuTile>(key) {
            Some(m) => m,
            None => {
                let k = (res / extent.w.max(extent.h)).min(1.0);
                let (mw, mh) = (((extent.w * k).round() as u32).max(1), ((extent.h * k).round() as u32).max(1));
                let src_req = Request::new(extent, extent.rect(), mw, mh);
                let src = ctx.render(input, &src_req)?;
                let src = ctx.to_cpu(src)?;
                ctx.status(Some("Removing background (neural network)…"));
                let m = compute_mask(&src, p.i("threshold").clamp(0, 255) as u8, p.b("binary"), p.b("sticker"), p.text("model"));
                ctx.status(None);
                let m = Arc::new(m?);
                ctx.engine.cache.insert(key, m.clone(), m.bytes());
                m
            }
        };

        let image = ctx.render_input(node, 0, &input_request(ctx, node, 0, req))?;
        let node_to_mask = Map2::new(mask.w as f64 / extent.w, 0.0, mask.h as f64 / extent.h, 0.0);
        let map = node_to_mask.after(&req.pixel_to_node());
        ctx.kernel(
            &kernels::MASK,
            vec![KernelInput::aligned(image), KernelInput { tile: Tile::Cpu((*mask).clone()), map, mode: SampleMode::Bilinear }],
            req,
            &[p.b("invert") as u8 as f32],
        )
    }
}

#[cfg(feature = "rembg")]
fn compute_mask(src: &CpuTile, threshold: u8, binary: bool, sticker: bool, model: &str) -> anyhow::Result<CpuTile> {
    use rembg_rs::manager::ModelManager;
    use rembg_rs::options::RemovalOptions;
    use std::path::PathBuf;
    use std::sync::Mutex;

    static MANAGER: Mutex<Option<(PathBuf, ModelManager)>> = Mutex::new(None);

    let path = resolve_model(model)?;
    let mut rgba = image::RgbaImage::new(src.w, src.h);
    for (px, s) in rgba.pixels_mut().zip(&src.data) {
        let c = crate::engine::tile::unpremul(*s);
        *px = image::Rgba([to8(c[0]), to8(c[1]), to8(c[2]), to8(s[3])]);
    }
    let options = RemovalOptions { threshold, binary, sticker };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> anyhow::Result<image::RgbaImage> {
        let mut guard = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
        if guard.as_ref().is_none_or(|(p, _)| *p != path) {
            let m = ModelManager::from_file(&path).map_err(|e| anyhow::anyhow!("could not load model {}: {e}", path.display()))?;
            *guard = Some((path.clone(), m));
        }
        let (_, manager) = guard.as_mut().unwrap();
        let r = rembg_rs::rembg::rembg(manager, image::DynamicImage::ImageRgba8(rgba), &options).map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(r.image)
    }));
    let out = match result {
        Ok(r) => r?,
        Err(_) => anyhow::bail!(
            "ONNX Runtime could not be loaded. Install it (e.g. `brew install onnxruntime`) or set ORT_DYLIB_PATH to libonnxruntime."
        ),
    };
    let mut m = CpuTile::new(src.w, src.h);
    for (d, px) in m.data.iter_mut().zip(out.pixels()) {
        let a = px.0[3] as f32 / 255.0;
        *d = [a, a, a, 1.0];
    }
    Ok(m)
}

#[cfg(feature = "rembg")]
fn to8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(feature = "rembg")]
fn resolve_model(model: &str) -> anyhow::Result<std::path::PathBuf> {
    use std::path::PathBuf;
    let mut candidates: Vec<PathBuf> = Vec::new();
    if !model.trim().is_empty() {
        candidates.push(PathBuf::from(model.trim()));
    }
    if let Some(p) = std::env::var_os("DELOS_REMBG_MODEL") {
        candidates.push(PathBuf::from(p));
    }
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        let dir = PathBuf::from(home).join(".delos").join("models");
        for name in ["u2net.onnx", "u2netp.onnx", "isnet-general-use.onnx", "u2net_human_seg.onnx", "silueta.onnx"] {
            candidates.push(dir.join(name));
        }
    }
    candidates.into_iter().find(|p| p.is_file()).ok_or_else(|| {
        anyhow::anyhow!(
            "No background-removal model found. Download u2net.onnx (https://github.com/danielgatis/rembg/releases) to ~/.delos/models/, set DELOS_REMBG_MODEL, or enter a path in the node's settings."
        )
    })
}

#[cfg(not(feature = "rembg"))]
fn compute_mask(_src: &CpuTile, _threshold: u8, _binary: bool, _sticker: bool, _model: &str) -> anyhow::Result<CpuTile> {
    anyhow::bail!("Background removal is not available on this platform (it needs the native desktop build with the `rembg` feature).")
}
