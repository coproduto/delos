//! The evaluation engine: pull-based, region-of-interest rendering of the
//! node graph with a GPU (wgpu) backend and a CPU (rayon) fallback.

pub mod cache;
pub mod geom;
#[cfg(feature = "gpu")]
pub mod gpu;
pub mod kernels;
#[cfg(feature = "gpu")]
pub mod pool;
pub mod profile;
#[cfg(feature = "gpu")]
pub mod resident;
pub mod source;
pub mod tile;

use crate::graph::{Graph, Link, NodeId};
use cache::Cache;
use geom::{Map2, Request};
use kernels::{Env, EnvInput, Kernel, MAX_PARAMS};
use profile::{Backend, Profile};
use source::SourceImage;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
#[cfg(feature = "gpu")]
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tile::{CpuTile, SampleMode, Tile};

/// Error raised by a specific node (shown on that node in the UI).
#[derive(Debug, Clone)]
pub struct NodeError {
    pub node: NodeId,
    pub message: String,
}

impl std::fmt::Display for NodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for NodeError {}

#[derive(Debug)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("cancelled")
    }
}

impl std::error::Error for Cancelled {}

/// Long-lived, shared engine state (GPU device, caches).
pub struct Engine {
    #[cfg(feature = "gpu")]
    pub gpu: Option<Arc<gpu::Gpu>>,
    pub use_gpu: AtomicBool,
    pub cache: Cache,
    /// Per-operation timings, when enabled.
    pub profile: Profile,
}

impl Engine {
    pub fn new(try_gpu: bool) -> Arc<Engine> {
        let profile = Profile::default();
        #[cfg(feature = "gpu")]
        let gpu = if try_gpu { gpu::Gpu::new(profile.clone()) } else { None };
        #[cfg(not(feature = "gpu"))]
        let _ = try_gpu;
        Arc::new(Engine {
            #[cfg(feature = "gpu")]
            use_gpu: AtomicBool::new(gpu.is_some()),
            #[cfg(not(feature = "gpu"))]
            use_gpu: AtomicBool::new(false),
            #[cfg(feature = "gpu")]
            gpu,
            cache: Cache::new(256 << 20),
            profile,
        })
    }

    /// Takes the timings recorded so far, first waiting for the GPU to run
    /// everything submitted so its kernels' timings are included.
    pub fn profile_samples(&self) -> Vec<profile::Sample> {
        #[cfg(feature = "gpu")]
        if let Some(g) = &self.gpu {
            g.finish();
        }
        self.profile.take()
    }

    pub fn backend_name(&self) -> String {
        #[cfg(feature = "gpu")]
        if let Some(g) = &self.gpu {
            if self.use_gpu.load(Ordering::Relaxed) && g.healthy() {
                return format!("GPU · {}", g.name);
            }
        }
        format!("CPU · {} threads", rayon::current_num_threads())
    }

    /// Compiles every GPU kernel up front so the first interactive render
    /// does not pay for shader compilation.
    pub fn warm_up(&self) {
        #[cfg(feature = "gpu")]
        if let Some(g) = &self.gpu {
            for k in kernels::ALL {
                g.pipeline(k);
            }
        }
    }

    /// Starts keeping `img` on the GPU (in the background), so renders that
    /// use it don't have to upload it.
    pub fn make_resident(&self, img: &Arc<SourceImage>) {
        #[cfg(feature = "gpu")]
        if let Some(g) = &self.gpu {
            g.resident.prefetch(img);
        }
        #[cfg(not(feature = "gpu"))]
        let _ = img;
    }

    /// Source images kept on the GPU, when there is one.
    #[cfg(feature = "gpu")]
    pub fn resident(&self) -> Option<&resident::Resident> {
        self.gpu.as_deref().map(|g| &g.resident)
    }

    pub fn has_gpu(&self) -> bool {
        #[cfg(feature = "gpu")]
        return self.gpu.is_some();
        #[cfg(not(feature = "gpu"))]
        false
    }

    #[cfg(feature = "gpu")]
    fn active_gpu(&self) -> Option<&gpu::Gpu> {
        if !self.use_gpu.load(Ordering::Relaxed) {
            return None;
        }
        self.gpu.as_deref().filter(|g| g.healthy())
    }
}

/// An input to a kernel: a rendered tile, the map from output pixels to the
/// tile's texels, and how to sample it.
pub struct KernelInput {
    pub tile: Tile,
    pub map: Map2,
    pub mode: SampleMode,
}

impl KernelInput {
    /// A tile rendered on exactly the same grid as the output.
    pub fn aligned(tile: Tile) -> Self {
        Self { tile, map: Map2::IDENTITY, mode: SampleMode::Nearest }
    }
}

/// Per-render evaluation context.
pub struct Ctx<'a> {
    pub engine: &'a Engine,
    pub graph: &'a Graph,
    cancel: &'a (dyn Fn() -> bool + Sync),
    status: &'a (dyn Fn(Option<&str>) + Sync),
    hashes: RefCell<HashMap<NodeId, u64>>,
}

impl<'a> Ctx<'a> {
    pub fn new(engine: &'a Engine, graph: &'a Graph, cancel: &'a (dyn Fn() -> bool + Sync), status: &'a (dyn Fn(Option<&str>) + Sync)) -> Self {
        Self { engine, graph, cancel, status, hashes: RefCell::new(HashMap::new()) }
    }

    pub fn cancelled(&self) -> bool {
        (self.cancel)()
    }

    /// Reports a long-running step to the UI (`None` clears it).
    pub fn status(&self, s: Option<&str>) {
        (self.status)(s)
    }

    pub fn node_hash(&self, id: NodeId) -> u64 {
        self.graph.hash_of(id, &mut self.hashes.borrow_mut())
    }

    /// Structural hash of one output's image (a cache key).
    pub fn link_hash(&self, link: Link) -> u64 {
        self.graph.link_hash(link, &mut self.hashes.borrow_mut())
    }

    /// Renders one output of a node (a node id means its first output).
    pub fn render(&self, link: impl Into<Link>, req: &Request) -> anyhow::Result<Tile> {
        let link = link.into();
        let id = link.node;
        if self.cancelled() {
            return Err(Cancelled.into());
        }
        let node = self.graph.node(id).ok_or_else(|| anyhow::anyhow!("missing node {id}"))?;
        if req.width == 0 || req.height == 0 || req.region.is_empty() {
            return Ok(Tile::Clear { w: req.width, h: req.height });
        }
        node.kind().render_output(self, node, link.output, req).map_err(|e| {
            if e.is::<NodeError>() || e.is::<Cancelled>() {
                e
            } else {
                NodeError { node: id, message: format!("{e:#}") }.into()
            }
        })
    }

    /// Renders whatever is connected to `port` of `node` (transparent when
    /// nothing is connected).
    pub fn render_input(&self, node: &crate::graph::Node, port: usize, req: &Request) -> anyhow::Result<Tile> {
        match node.inputs.get(port).copied().flatten() {
            Some(i) => self.render(i, req),
            None => Ok(Tile::Clear { w: req.width, h: req.height }),
        }
    }

    pub fn input_size(&self, node: &crate::graph::Node, port: usize) -> Option<geom::Size> {
        node.input_node(port).and_then(|i| self.graph.size_of(i))
    }

    pub fn has_input(&self, node: &crate::graph::Node, port: usize) -> bool {
        node.inputs.get(port).copied().flatten().is_some()
    }

    pub fn to_cpu(&self, t: Tile) -> anyhow::Result<CpuTile> {
        match t {
            Tile::Clear { w, h } => Ok(CpuTile::new(w, h)),
            Tile::Cpu(c) => Ok(c),
            #[cfg(feature = "gpu")]
            Tile::Gpu(g) => self.engine.gpu.as_ref().expect("gpu tile without gpu").download(&g),
        }
    }

    /// Samples a source image for `req`: on the GPU from its resident
    /// pyramid level when possible, otherwise on the CPU.
    pub fn source(&self, img: &Arc<SourceImage>, req: &Request) -> Tile {
        #[cfg(feature = "gpu")]
        if let Some(t) = self.source_on_gpu(img, req) {
            return t;
        }
        let pixels = req.width as u64 * req.height as u64;
        Tile::Cpu(self.engine.profile.time("source", Backend::Cpu, pixels, || img.render(req)))
    }

    /// `None` unless the GPU is in use and the level is resident (asking
    /// queues its upload). The level is RGBA8, unlike other GPU tiles, so it
    /// is only ever a kernel input here, never returned.
    #[cfg(feature = "gpu")]
    fn source_on_gpu(&self, img: &Arc<SourceImage>, req: &Request) -> Option<Tile> {
        let g = self.engine.active_gpu()?;
        if !g.fits(req.width, req.height) {
            return None;
        }
        let pipeline = g.pipeline(&kernels::RESAMPLE)?;
        let level = img.level_for(req);
        let tile = gpu::GpuTile::from(g.resident.get(img, level)?);
        let (map, mode) = img.texel_map(level, req);
        let input = gpu::GpuInput { tile: Some(&tile), map: map.as_f32(), mode };
        let extent = [req.extent.w as f32, req.extent.h as f32];
        let out = g.run("source", &pipeline, &[input], req.pixel_to_node().as_f32(), extent, &[0.0; MAX_PARAMS], req.width, req.height);
        Some(Tile::Gpu(out))
    }

    /// Runs a kernel over the request's grid, on the GPU when possible.
    pub fn kernel(&self, k: &Kernel, inputs: Vec<KernelInput>, req: &Request, params: &[f32]) -> anyhow::Result<Tile> {
        if self.cancelled() {
            return Err(Cancelled.into());
        }
        let mut p = [0.0f32; MAX_PARAMS];
        p[..params.len()].copy_from_slice(params);
        let node_map = req.pixel_to_node().as_f32();
        let extent = [req.extent.w as f32, req.extent.h as f32];

        #[cfg(feature = "gpu")]
        if let Some(g) = self.engine.active_gpu() {
            let fits = g.fits(req.width, req.height) && inputs.iter().all(|i| {
                let (w, h) = i.tile.dims();
                g.fits(w, h)
            });
            if fits {
                if let Some(pipeline) = g.pipeline(k) {
                    let mut owned = Vec::with_capacity(inputs.len());
                    let mut uploaded: Vec<Option<gpu::GpuTile>> = Vec::with_capacity(inputs.len());
                    for i in inputs {
                        owned.push((i.map, i.mode));
                        uploaded.push(match i.tile {
                            Tile::Clear { .. } => None,
                            Tile::Cpu(c) => Some(g.upload(&c)),
                            Tile::Gpu(t) => Some(t),
                        });
                    }
                    let gin: Vec<gpu::GpuInput> = uploaded
                        .iter()
                        .zip(&owned)
                        .map(|(t, (map, mode))| gpu::GpuInput { tile: t.as_ref(), map: map.as_f32(), mode: *mode })
                        .collect();
                    return Ok(Tile::Gpu(g.run(k.name, &pipeline, &gin, node_map, extent, &p, req.width, req.height)));
                }
            }
        }

        let mut cpu_inputs = Vec::with_capacity(inputs.len());
        for i in inputs {
            let clear = i.tile.is_clear();
            cpu_inputs.push((if clear { None } else { Some(self.to_cpu(i.tile)?) }, i.map, i.mode));
        }
        let env = Env {
            inputs: cpu_inputs.iter().map(|(t, map, mode)| EnvInput { tile: t.as_ref(), map: map.as_f32(), mode: *mode }).collect(),
            node_map,
            extent,
            p,
        };
        let pixels = req.width as u64 * req.height as u64;
        Ok(Tile::Cpu(self.engine.profile.time(k.name, Backend::Cpu, pixels, || kernels::run_cpu(k, &env, req.width, req.height))))
    }
}

/// Renders `target` for `req` and returns the result on the CPU.
pub fn render_to_cpu(
    engine: &Engine,
    graph: &Graph,
    target: impl Into<Link>,
    req: &Request,
    cancel: &(dyn Fn() -> bool + Sync),
    status: &(dyn Fn(Option<&str>) + Sync),
) -> anyhow::Result<CpuTile> {
    let ctx = Ctx::new(engine, graph, cancel, status);
    let t = ctx.render(target, req)?;
    ctx.to_cpu(t)
}

pub fn is_cancelled(e: &anyhow::Error) -> bool {
    e.is::<Cancelled>()
}
