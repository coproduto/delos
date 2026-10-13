//! Source images kept resident on the GPU. Each pyramid level is uploaded
//! once (as RGBA8) on a background thread and then shared by every render
//! that needs it, instead of being sampled on the CPU and uploaded every
//! frame.
//!
//! Rendering never waits for an upload: a level that isn't resident (still
//! uploading, evicted, or too large for a texture or for the budget) is
//! rendered the CPU way, and asking for it queues its upload. Levels beyond
//! the memory budget are evicted least recently used first.

use super::source::SourceImage;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, Weak};

/// Default GPU memory for resident source levels.
#[cfg(not(feature = "mobile"))]
pub const DEFAULT_BUDGET: usize = 4 << 30;
#[cfg(feature = "mobile")]
pub const DEFAULT_BUDGET: usize = 1 << 30;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Large levels are written in chunks of about this many bytes, so the
/// staging memory wgpu needs stays bounded.
const UPLOAD_CHUNK: usize = 64 << 20;

/// A resident level, ready to bind as a kernel input.
#[derive(Clone)]
pub struct Level {
    pub w: u32,
    pub h: u32,
    pub tex: wgpu::Texture,
    pub view: wgpu::TextureView,
}

enum State {
    Queued,
    Resident { level: Level, bytes: usize, used: u64 },
    /// Can't be made resident (larger than a texture or the whole budget,
    /// or the upload failed); rendered the CPU way.
    Skipped,
}

struct Slot {
    image: Weak<SourceImage>,
    state: State,
}

/// (image id, level)
type Key = (u64, usize);

struct Registry {
    slots: HashMap<Key, Slot>,
    budget: usize,
    bytes: usize,
    tick: u64,
}

impl Registry {
    /// Forgets images that no graph (or undo snapshot) holds any more.
    fn drop_dead(&mut self) {
        let mut freed = 0;
        self.slots.retain(|_, s| {
            let alive = s.image.strong_count() > 0;
            if let (false, State::Resident { bytes, .. }) = (alive, &s.state) {
                freed += bytes;
            }
            alive
        });
        self.bytes -= freed;
    }

    /// Evicts least recently used levels of other images until `need` more
    /// bytes fit in the budget. Returns whether they do.
    fn make_room(&mut self, need: usize, keep: u64) -> bool {
        while self.bytes + need > self.budget {
            let victim = self
                .slots
                .iter()
                .filter(|(k, _)| k.0 != keep)
                .filter_map(|(k, s)| match s.state {
                    State::Resident { used, .. } => Some((used, *k)),
                    _ => None,
                })
                .min();
            let Some((_, k)) = victim else { return false };
            self.evict(k);
        }
        true
    }

    fn evict(&mut self, k: Key) {
        // Renders already using the texture keep it alive until they finish.
        let slot = self.slots.remove(&k).unwrap();
        if let State::Resident { bytes, .. } = slot.state {
            self.bytes -= bytes;
        }
    }
}

pub struct Resident {
    registry: Arc<Mutex<Registry>>,
    jobs: Sender<Key>,
    max_dim: u32,
}

impl Resident {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, max_dim: u32) -> Self {
        let registry = Arc::new(Mutex::new(Registry { slots: HashMap::new(), budget: DEFAULT_BUDGET, bytes: 0, tick: 0 }));
        let (jobs, rx) = channel();
        let r = registry.clone();
        std::thread::Builder::new()
            .name("delos-upload".into())
            .spawn(move || upload_loop(device, queue, r, rx))
            .expect("spawn upload thread");
        Self { registry, jobs, max_dim }
    }

    /// Queues every level of `img` for upload, smallest first, so fitted
    /// previews become fast first.
    pub fn prefetch(&self, img: &Arc<SourceImage>) {
        for level in (0..img.level_count()).rev() {
            self.queue(img, level);
        }
    }

    /// The level if it is resident (marking it recently used); otherwise
    /// queues its upload and returns `None`.
    pub fn get(&self, img: &Arc<SourceImage>, level: usize) -> Option<Level> {
        {
            let mut r = self.registry.lock().unwrap();
            r.tick += 1;
            let tick = r.tick;
            if let Some(Slot { state: State::Resident { level: lv, used, .. }, .. }) = r.slots.get_mut(&(img.id, level)) {
                *used = tick;
                return Some(lv.clone());
            }
        }
        self.queue(img, level);
        None
    }

    pub fn is_resident(&self, img: &SourceImage, level: usize) -> bool {
        matches!(self.registry.lock().unwrap().slots.get(&(img.id, level)), Some(Slot { state: State::Resident { .. }, .. }))
    }

    /// GPU memory held by resident levels.
    pub fn bytes(&self) -> usize {
        let mut r = self.registry.lock().unwrap();
        r.drop_dead();
        r.bytes
    }

    pub fn budget(&self) -> usize {
        self.registry.lock().unwrap().budget
    }

    /// Changes the memory budget, evicting least recently used levels if the
    /// resident ones no longer fit. Skipped levels are reconsidered the next
    /// time they're needed.
    pub fn set_budget(&self, budget: usize) {
        let mut r = self.registry.lock().unwrap();
        r.budget = budget;
        r.drop_dead();
        r.make_room(0, 0);
        r.slots.retain(|_, s| !matches!(s.state, State::Skipped));
    }

    fn queue(&self, img: &Arc<SourceImage>, level: usize) {
        let (w, h, _) = img.level(level);
        let key = (img.id, level);
        let mut r = self.registry.lock().unwrap();
        r.drop_dead();
        if r.slots.contains_key(&key) {
            return;
        }
        let fits = w <= self.max_dim && h <= self.max_dim && level_bytes(w, h) <= r.budget;
        let state = if fits { State::Queued } else { State::Skipped };
        r.slots.insert(key, Slot { image: Arc::downgrade(img), state });
        if fits {
            let _ = self.jobs.send(key);
        }
    }
}

fn level_bytes(w: u32, h: u32) -> usize {
    w as usize * h as usize * 4
}

fn upload_loop(device: wgpu::Device, queue: wgpu::Queue, registry: Arc<Mutex<Registry>>, jobs: Receiver<Key>) {
    // Ends when the `Resident` (and with it the sender) is dropped.
    for key in jobs {
        let img = {
            let mut r = registry.lock().unwrap();
            r.drop_dead();
            let Some(slot) = r.slots.get(&key) else { continue };
            if !matches!(slot.state, State::Queued) {
                continue;
            }
            let Some(img) = slot.image.upgrade() else { continue };
            let (w, h, _) = img.level(key.1);
            if !r.make_room(level_bytes(w, h), key.0) {
                // Only the image's own levels could make room; retried the
                // next time the level is needed.
                r.slots.remove(&key);
                continue;
            }
            img
        };
        let uploaded = upload(&device, &queue, &img, key.1);
        let mut r = registry.lock().unwrap();
        // Dropped or re-budgeted while uploading: nothing to record.
        if !r.slots.get(&key).is_some_and(|s| matches!(s.state, State::Queued)) {
            continue;
        }
        r.tick += 1;
        let used = r.tick;
        let state = match uploaded {
            Ok(level) => {
                let bytes = level_bytes(level.w, level.h);
                r.bytes += bytes;
                State::Resident { level, bytes, used }
            }
            Err(e) => {
                eprintln!("delos: couldn't keep '{}' level {} on the GPU: {e}", img.name, key.1);
                State::Skipped
            }
        };
        r.slots.get_mut(&key).unwrap().state = state;
    }
}

/// Uploads one level. Errors (such as running out of memory) are caught here
/// rather than reported as device failures, which would disable the GPU.
fn upload(device: &wgpu::Device, queue: &wgpu::Queue, img: &SourceImage, level: usize) -> anyhow::Result<Level> {
    let (w, h, data) = img.level(level);
    let oom = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let invalid = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("source level"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let row = w as usize * 4;
    let rows = (UPLOAD_CHUNK / row).max(1);
    for (i, chunk) in data.chunks(rows * row).enumerate() {
        let n = (chunk.len() / row) as u32;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: (i * rows) as u32, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            chunk,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row as u32), rows_per_image: Some(n) },
            wgpu::Extent3d { width: w, height: n, depth_or_array_layers: 1 },
        );
        // Flush this chunk's staging copy before writing the next.
        queue.submit([]);
        device.poll(wgpu::PollType::wait_indefinitely())?;
    }
    let invalid = pollster::block_on(invalid.pop());
    let oom = pollster::block_on(oom.pop());
    if let Some(e) = invalid.or(oom) {
        anyhow::bail!("{e}");
    }
    let view = tex.create_view(&Default::default());
    Ok(Level { w, h, tex, view })
}
