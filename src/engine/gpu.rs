//! wgpu compute backend. Kernels from [`super::kernels`] are compiled lazily
//! into compute pipelines; intermediate tiles stay on the GPU between
//! consecutive GPU kernels and are only read back when a CPU-only operation
//! or the final display needs them.

use super::kernels::{wgsl_source, Kernel, MAX_INPUTS, MAX_PARAMS};
use super::pool::Pool;
use super::profile::{Backend, Profile};
use super::resident::Resident;
use super::tile::{CpuTile, SampleMode};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;

/// A tile on the GPU. Clones share the texture, which goes back to the
/// pool when the last one is dropped.
#[derive(Clone)]
pub struct GpuTile {
    pub w: u32,
    pub h: u32,
    tex: Arc<Texture>,
}

struct Texture {
    tex: wgpu::Texture,
    view: wgpu::TextureView,
    /// `None` for textures the pool doesn't own (resident source levels).
    pool: Option<Weak<Pool>>,
}

impl Drop for Texture {
    fn drop(&mut self) {
        if let Some(pool) = self.pool.as_ref().and_then(Weak::upgrade) {
            pool.put_texture(self.tex.clone(), self.view.clone());
        }
    }
}

impl GpuTile {
    fn view(&self) -> &wgpu::TextureView {
        &self.tex.view
    }
}

impl From<super::resident::Level> for GpuTile {
    fn from(l: super::resident::Level) -> Self {
        GpuTile { w: l.w, h: l.h, tex: Arc::new(Texture { tex: l.tex, view: l.view, pool: None }) }
    }
}

pub struct GpuInput<'a> {
    pub tile: Option<&'a GpuTile>,
    pub map: [f32; 4],
    pub mode: SampleMode,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    out_size: [u32; 4],
    node_map: [f32; 4],
    extent: [f32; 4],
    in_map: [[f32; 4]; MAX_INPUTS],
    in_mode: [u32; 4],
    params: [f32; MAX_PARAMS],
}

pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    pipelines: Mutex<HashMap<&'static str, Option<Arc<wgpu::ComputePipeline>>>>,
    dummy: wgpu::TextureView,
    failed: Arc<AtomicBool>,
    pub name: String,
    pub max_dim: u32,
    /// Source image levels kept on the GPU.
    pub resident: Resident,
    /// Idle textures and readback buffers kept for reuse.
    pub pool: Arc<Pool>,
    profile: Profile,
    /// Nanoseconds per timestamp tick, when the device supports timestamps.
    timestamp_period: Option<f32>,
}

impl Gpu {
    /// Tries to acquire a GPU. Returns `None` (CPU fallback) when no adapter
    /// is available or `DELOS_NO_GPU` is set.
    pub fn new(profile: Profile) -> Option<Arc<Gpu>> {
        if std::env::var_os("DELOS_NO_GPU").is_some() {
            return None;
        }
        let r = std::panic::catch_unwind(|| pollster::block_on(Self::init(profile)));
        match r {
            Ok(Ok(g)) => Some(Arc::new(g)),
            Ok(Err(e)) => {
                eprintln!("delos: GPU unavailable, using CPU: {e}");
                None
            }
            Err(_) => {
                eprintln!("delos: GPU initialisation panicked, using CPU");
                None
            }
        }
    }

    async fn init(profile: Profile) -> anyhow::Result<Gpu> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() })
            .await?;
        let info = adapter.get_info();
        if info.device_type == wgpu::DeviceType::Cpu {
            anyhow::bail!("only a software adapter ({}) is available", info.name);
        }
        let limits = adapter.limits();
        // Only used while profiling, but requesting it costs nothing.
        let features = adapter.features() & wgpu::Features::TIMESTAMP_QUERY;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("delos"),
                required_features: features,
                required_limits: limits.clone(),
                ..Default::default()
            })
            .await?;
        let timestamp_period = features.contains(wgpu::Features::TIMESTAMP_QUERY).then(|| queue.get_timestamp_period());
        let failed = Arc::new(AtomicBool::new(false));
        let f2 = failed.clone();
        device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
            eprintln!("delos: GPU error: {e}");
            f2.store(true, Ordering::Relaxed);
        }));

        let tex_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("kernel"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                tex_entry(1),
                tex_entry(2),
                tex_entry(3),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: FORMAT,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("kernel"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let dummy = Self::make_texture(&device, 1, 1).create_view(&Default::default());
        let resident = Resident::new(device.clone(), queue.clone(), limits.max_texture_dimension_2d);
        Ok(Gpu {
            device,
            queue,
            layout,
            pipeline_layout,
            pipelines: Mutex::new(HashMap::new()),
            dummy,
            failed,
            name: format!("{} ({:?})", info.name, info.backend),
            max_dim: limits.max_texture_dimension_2d,
            resident,
            pool: Arc::new(Pool::new()),
            profile,
            timestamp_period,
        })
    }

    fn make_texture(device: &wgpu::Device, w: u32, h: u32) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
    }

    pub fn healthy(&self) -> bool {
        !self.failed.load(Ordering::Relaxed)
    }

    pub fn fits(&self, w: u32, h: u32) -> bool {
        w <= self.max_dim && h <= self.max_dim
    }

    /// Compiled pipeline for a kernel, or `None` if it failed to compile (in
    /// which case the CPU implementation is used).
    pub fn pipeline(&self, k: &Kernel) -> Option<Arc<wgpu::ComputePipeline>> {
        let mut map = self.pipelines.lock().unwrap();
        if let Some(p) = map.get(k.name) {
            return p.clone();
        }
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(k.name),
            source: wgpu::ShaderSource::Wgsl(wgsl_source(k).into()),
        });
        let pipeline = self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(k.name),
            layout: Some(&self.pipeline_layout),
            module: &module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let result = match pollster::block_on(scope.pop()) {
            None => Some(Arc::new(pipeline)),
            Some(e) => {
                eprintln!("delos: kernel '{}' failed to compile for GPU, using CPU: {e}", k.name);
                None
            }
        };
        map.insert(k.name, result.clone());
        result
    }

    /// A texture for a tile, from the pool when one of this size is idle.
    fn tile(&self, w: u32, h: u32) -> GpuTile {
        let (tex, view) = self.pool.texture(w, h, || Self::make_texture(&self.device, w, h));
        GpuTile { w, h, tex: Arc::new(Texture { tex, view, pool: Some(Arc::downgrade(&self.pool)) }) }
    }

    pub fn upload(&self, t: &CpuTile) -> GpuTile {
        let start = self.profile.enabled().then(Instant::now);
        let tile = self.tile(t.w, t.h);
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &tile.tex.tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            bytemuck::cast_slice(&t.data),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(t.w * 16), rows_per_image: Some(t.h) },
            wgpu::Extent3d { width: t.w, height: t.h, depth_or_array_layers: 1 },
        );
        if let Some(start) = start {
            self.profile.record("upload", Backend::Gpu, t.w as u64 * t.h as u64, start.elapsed());
        }
        tile
    }

    pub fn download(&self, t: &GpuTile) -> anyhow::Result<CpuTile> {
        let row = t.w as u64 * 16;
        let padded = row.div_ceil(256) * 256;
        let size = padded * t.h as u64;
        let buf = self.pool.buffer(size, || {
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("readback"),
                size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            })
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &t.tex.tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded as u32), rows_per_image: Some(t.h) },
            },
            wgpu::Extent3d { width: t.w, height: t.h, depth_or_array_layers: 1 },
        );
        let pixels = t.w as u64 * t.h as u64;
        let start = Instant::now();
        self.queue.submit([enc.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buf.map_async(wgpu::MapMode::Read, .., move |r| {
            let _ = tx.send(r);
        });
        self.device.poll(wgpu::PollType::wait_indefinitely())?;
        rx.recv()??;
        if self.profile.enabled() {
            self.profile.record("readback wait", Backend::Gpu, pixels, start.elapsed());
        }
        let start = Instant::now();
        let mut out = CpuTile::new(t.w, t.h);
        {
            let view = buf.get_mapped_range(..)?;
            let bytes: &[u8] = &view;
            for y in 0..t.h as usize {
                let src = &bytes[y * padded as usize..y * padded as usize + row as usize];
                let dst = &mut out.data[y * t.w as usize..(y + 1) * t.w as usize];
                bytemuck::cast_slice_mut::<_, u8>(dst).copy_from_slice(src);
            }
        }
        buf.unmap();
        self.pool.put_buffer(buf);
        if self.profile.enabled() {
            self.profile.record("readback copy", Backend::Gpu, pixels, start.elapsed());
        }
        if !self.healthy() {
            anyhow::bail!("GPU reported an error");
        }
        Ok(out)
    }

    /// Dispatches a kernel; `op` names it in profiles.
    #[allow(clippy::too_many_arguments)]
    pub fn run(
        &self,
        op: &'static str,
        pipeline: &wgpu::ComputePipeline,
        inputs: &[GpuInput],
        node_map: [f32; 4],
        extent: [f32; 2],
        params: &[f32; MAX_PARAMS],
        w: u32,
        h: u32,
    ) -> GpuTile {
        let mut u = Uniforms {
            out_size: [w, h, 0, 0],
            node_map,
            extent: [extent[0], extent[1], 0.0, 0.0],
            in_map: [[0.0; 4]; MAX_INPUTS],
            in_mode: [3; 4],
            params: *params,
        };
        for (i, inp) in inputs.iter().enumerate().take(MAX_INPUTS) {
            u.in_map[i] = inp.map;
            u.in_mode[i] = if inp.tile.is_some() { inp.mode as u32 } else { 3 };
        }
        let ubuf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&ubuf, 0, bytemuck::bytes_of(&u));
        let out = self.tile(w, h);
        let view_of = |i: usize| inputs.get(i).and_then(|x| x.tile).map(GpuTile::view).unwrap_or(&self.dummy);
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: ubuf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(view_of(0)) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(view_of(1)) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(view_of(2)) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(out.view()) },
            ],
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        let timer = self.timer();
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(op),
                timestamp_writes: timer.as_ref().map(|t| wgpu::ComputePassTimestampWrites {
                    query_set: &t.queries,
                    beginning_of_pass_write_index: Some(0),
                    end_of_pass_write_index: Some(1),
                }),
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(w.div_ceil(8), h.div_ceil(8), 1);
        }
        if let Some(t) = &timer {
            enc.resolve_query_set(&t.queries, 0..2, &t.resolved, 0);
            enc.copy_buffer_to_buffer(&t.resolved, 0, &t.readable, 0, 16);
        }
        self.queue.submit([enc.finish()]);
        if let Some(t) = timer {
            self.collect(t, op, w as u64 * h as u64);
        }
        out
    }

    /// Queries timing one pass, while profiling on a device with timestamps.
    fn timer(&self) -> Option<Timer> {
        let period = self.timestamp_period.filter(|_| self.profile.enabled())?;
        let buffer = |usage| {
            self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("timestamps"), size: 16, usage, mapped_at_creation: false })
        };
        Some(Timer {
            queries: self.device.create_query_set(&wgpu::QuerySetDescriptor { label: None, ty: wgpu::QueryType::Timestamp, count: 2 }),
            resolved: buffer(wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC),
            readable: buffer(wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST),
            period,
        })
    }

    /// Records a submitted pass's GPU time once the GPU has run it (seen by
    /// the next poll of the device).
    fn collect(&self, t: Timer, op: &'static str, pixels: u64) {
        let profile = self.profile.clone();
        let buf = t.readable.clone();
        t.readable.map_async(wgpu::MapMode::Read, .., move |r| {
            if r.is_err() {
                return;
            }
            let ticks = {
                let Ok(view) = buf.get_mapped_range(..) else { return };
                let ts: &[u64] = bytemuck::cast_slice(&view);
                ts[1].saturating_sub(ts[0])
            };
            buf.unmap();
            profile.record(op, Backend::Gpu, pixels, Duration::from_nanos((ticks as f64 * t.period as f64) as u64));
        });
    }

    /// Waits for the GPU to finish everything submitted, so that every
    /// kernel's timing has been recorded.
    pub fn finish(&self) {
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
    }
}

struct Timer {
    queries: wgpu::QuerySet,
    resolved: wgpu::Buffer,
    readable: wgpu::Buffer,
    period: f32,
}
