//! Reuse of GPU textures and readback buffers. Creating one costs far more
//! on the GPU than the kernel that fills it (the memory has to be mapped and
//! zeroed), while previews and export bands ask for the same sizes over and
//! over. Textures return here when the last tile using them is dropped, and
//! readback buffers once read.
//!
//! Only idle resources count against the cap; the least recently returned
//! are released first. Reused resources hold stale data, which kernels
//! overwrite: every kernel writes every texel it dispatches.
//!
//! The queue runs submissions in order, so a texture returned while earlier
//! work still reads it is only overwritten after that work.

use std::sync::Mutex;

/// Idle memory kept for reuse.
#[cfg(not(feature = "mobile"))]
pub const DEFAULT_CAP: usize = 512 << 20;
#[cfg(feature = "mobile")]
pub const DEFAULT_CAP: usize = 128 << 20;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Key {
    Texture(u32, u32),
    Buffer(u64),
}

enum Resource {
    Texture(wgpu::Texture, wgpu::TextureView),
    Buffer(wgpu::Buffer),
}

struct Idle {
    key: Key,
    bytes: usize,
    res: Resource,
}

#[derive(Default)]
struct State {
    /// Least recently returned first.
    idle: Vec<Idle>,
    bytes: usize,
    created: u64,
}

pub struct Pool {
    state: Mutex<State>,
    cap: Mutex<usize>,
}

/// Bytes of a pooled (RGBA `f32`) texture.
pub fn texture_bytes(w: u32, h: u32) -> usize {
    w as usize * h as usize * 16
}

impl Pool {
    pub fn new() -> Self {
        Self { state: Mutex::default(), cap: Mutex::new(DEFAULT_CAP) }
    }

    /// A texture of this size, reused when one is idle, otherwise made by
    /// `make`.
    pub fn texture(&self, w: u32, h: u32, make: impl FnOnce() -> wgpu::Texture) -> (wgpu::Texture, wgpu::TextureView) {
        match self.take(Key::Texture(w, h)) {
            Some(Resource::Texture(t, v)) => (t, v),
            _ => {
                let t = make();
                let v = t.create_view(&Default::default());
                (t, v)
            }
        }
    }

    /// A buffer of this size, reused when one is idle, otherwise made by
    /// `make`.
    pub fn buffer(&self, size: u64, make: impl FnOnce() -> wgpu::Buffer) -> wgpu::Buffer {
        match self.take(Key::Buffer(size)) {
            Some(Resource::Buffer(b)) => b,
            _ => make(),
        }
    }

    pub fn put_texture(&self, tex: wgpu::Texture, view: wgpu::TextureView) {
        let (w, h) = (tex.width(), tex.height());
        self.put(Idle { key: Key::Texture(w, h), bytes: texture_bytes(w, h), res: Resource::Texture(tex, view) });
    }

    /// Returns an unmapped buffer.
    pub fn put_buffer(&self, buf: wgpu::Buffer) {
        self.put(Idle { key: Key::Buffer(buf.size()), bytes: buf.size() as usize, res: Resource::Buffer(buf) });
    }

    fn take(&self, key: Key) -> Option<Resource> {
        let mut s = self.state.lock().unwrap();
        // The most recently returned is the likeliest to still be cached.
        match s.idle.iter().rposition(|i| i.key == key) {
            Some(n) => {
                let i = s.idle.remove(n);
                s.bytes -= i.bytes;
                Some(i.res)
            }
            None => {
                s.created += 1;
                None
            }
        }
    }

    fn put(&self, item: Idle) {
        let cap = self.cap();
        if item.bytes > cap {
            return;
        }
        let mut s = self.state.lock().unwrap();
        s.bytes += item.bytes;
        s.idle.push(item);
        Self::trim(&mut s, cap);
    }

    fn trim(s: &mut State, cap: usize) {
        let mut n = 0;
        while s.bytes > cap {
            s.bytes -= s.idle[n].bytes;
            n += 1;
        }
        s.idle.drain(..n);
    }

    pub fn cap(&self) -> usize {
        *self.cap.lock().unwrap()
    }

    /// Changes the cap, releasing idle resources beyond it.
    pub fn set_cap(&self, cap: usize) {
        *self.cap.lock().unwrap() = cap;
        Self::trim(&mut self.state.lock().unwrap(), cap);
    }

    /// Memory held by idle resources.
    pub fn idle_bytes(&self) -> usize {
        self.state.lock().unwrap().bytes
    }

    /// How many textures and buffers had to be created because none of the
    /// right size was idle.
    pub fn created(&self) -> u64 {
        self.state.lock().unwrap().created
    }
}

impl Default for Pool {
    fn default() -> Self {
        Self::new()
    }
}
