//! wgpu compute backend. Kernels from [`super::kernels`] are compiled lazily
//! into compute pipelines; intermediate tiles stay on the GPU between
//! consecutive GPU kernels and are only read back when a CPU-only operation
//! or the final display needs them.

use super::kernels::{wgsl_source, Kernel, MAX_INPUTS, MAX_PARAMS};
use super::tile::{CpuTile, SampleMode};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;

pub struct GpuTile {
    pub w: u32,
    pub h: u32,
    tex: wgpu::Texture,
    view: wgpu::TextureView,
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
}

impl Gpu {
    /// Tries to acquire a GPU. Returns `None` (CPU fallback) when no adapter
    /// is available or `DELOS_NO_GPU` is set.
    pub fn new() -> Option<Arc<Gpu>> {
        if std::env::var_os("DELOS_NO_GPU").is_some() {
            return None;
        }
        let r = std::panic::catch_unwind(|| pollster::block_on(Self::init()));
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

    async fn init() -> anyhow::Result<Gpu> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() })
            .await?;
        let info = adapter.get_info();
        if info.device_type == wgpu::DeviceType::Cpu {
            anyhow::bail!("only a software adapter ({}) is available", info.name);
        }
        let limits = adapter.limits();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor { label: Some("delos"), required_limits: limits.clone(), ..Default::default() })
            .await?;
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

    pub fn upload(&self, t: &CpuTile) -> GpuTile {
        let tex = Self::make_texture(&self.device, t.w, t.h);
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            bytemuck::cast_slice(&t.data),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(t.w * 16), rows_per_image: Some(t.h) },
            wgpu::Extent3d { width: t.w, height: t.h, depth_or_array_layers: 1 },
        );
        let view = tex.create_view(&Default::default());
        GpuTile { w: t.w, h: t.h, tex, view }
    }

    pub fn download(&self, t: &GpuTile) -> anyhow::Result<CpuTile> {
        let row = t.w as u64 * 16;
        let padded = row.div_ceil(256) * 256;
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: padded * t.h as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &t.tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded as u32), rows_per_image: Some(t.h) },
            },
            wgpu::Extent3d { width: t.w, height: t.h, depth_or_array_layers: 1 },
        );
        self.queue.submit([enc.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buf.map_async(wgpu::MapMode::Read, .., move |r| {
            let _ = tx.send(r);
        });
        self.device.poll(wgpu::PollType::wait_indefinitely())?;
        rx.recv()??;
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
        if !self.healthy() {
            anyhow::bail!("GPU reported an error");
        }
        Ok(out)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn run(
        &self,
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
        let out = Self::make_texture(&self.device, w, h);
        let out_view = out.create_view(&Default::default());
        let view_of = |i: usize| inputs.get(i).and_then(|x| x.tile).map(|t| &t.view).unwrap_or(&self.dummy);
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: ubuf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(view_of(0)) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(view_of(1)) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(view_of(2)) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&out_view) },
            ],
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_compute_pass(&Default::default());
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(w.div_ceil(8), h.div_ceil(8), 1);
        }
        self.queue.submit([enc.finish()]);
        GpuTile { w, h, tex: out, view: out_view }
    }
}
