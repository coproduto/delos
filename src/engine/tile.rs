//! Pixel tiles: the unit of data that flows between nodes.
//!
//! Pixels are stored as premultiplied RGBA `f32` (sRGB-encoded values). A
//! tile only ever covers what a request asked for, so tiles are viewport- or
//! band-sized even when the source images are huge.

use rayon::prelude::*;

pub type Px = [f32; 4];

pub const CLEAR: Px = [0.0; 4];

#[derive(Clone)]
pub struct CpuTile {
    pub w: u32,
    pub h: u32,
    pub data: Vec<Px>,
}

impl CpuTile {
    pub fn new(w: u32, h: u32) -> Self {
        Self { w, h, data: vec![CLEAR; w as usize * h as usize] }
    }

    pub fn from_fn_par(w: u32, h: u32, f: impl Fn(u32, u32) -> Px + Sync) -> Self {
        let mut t = Self::new(w, h);
        t.data.par_chunks_mut(w.max(1) as usize).enumerate().for_each(|(y, row)| {
            for (x, px) in row.iter_mut().enumerate() {
                *px = f(x as u32, y as u32);
            }
        });
        t
    }

    #[inline]
    pub fn get(&self, x: i64, y: i64) -> Px {
        if x < 0 || y < 0 || x >= self.w as i64 || y >= self.h as i64 {
            CLEAR
        } else {
            self.data[y as usize * self.w as usize + x as usize]
        }
    }

    #[inline]
    fn get_clamped(&self, x: i64, y: i64) -> Px {
        let x = x.clamp(0, self.w as i64 - 1);
        let y = y.clamp(0, self.h as i64 - 1);
        self.data[y as usize * self.w as usize + x as usize]
    }

    pub fn bytes(&self) -> usize {
        self.data.len() * std::mem::size_of::<Px>()
    }

    /// Samples at continuous texel coordinates (texel `i` is centered at
    /// `i + 0.5`). Points outside the tile are transparent; filter taps
    /// that fall outside are clamped to the edge.
    #[inline]
    pub fn sample(&self, mode: SampleMode, u: f32, v: f32) -> Px {
        if !(u >= 0.0 && v >= 0.0 && u < self.w as f32 && v < self.h as f32) {
            return CLEAR;
        }
        match mode {
            SampleMode::Nearest => self.get(u as i64, v as i64),
            SampleMode::Bilinear => {
                let x = u - 0.5;
                let y = v - 0.5;
                let x0 = x.floor();
                let y0 = y.floor();
                let fx = x - x0;
                let fy = y - y0;
                let (x0, y0) = (x0 as i64, y0 as i64);
                let a = self.get_clamped(x0, y0);
                let b = self.get_clamped(x0 + 1, y0);
                let c = self.get_clamped(x0, y0 + 1);
                let d = self.get_clamped(x0 + 1, y0 + 1);
                let mut o = CLEAR;
                for i in 0..4 {
                    let top = a[i] + (b[i] - a[i]) * fx;
                    let bot = c[i] + (d[i] - c[i]) * fx;
                    o[i] = top + (bot - top) * fy;
                }
                o
            }
            SampleMode::Bicubic => {
                let x = u - 0.5;
                let y = v - 0.5;
                let x0 = x.floor();
                let y0 = y.floor();
                let wx = catmull_rom(x - x0);
                let wy = catmull_rom(y - y0);
                let (x0, y0) = (x0 as i64, y0 as i64);
                let mut o = CLEAR;
                for (j, wyj) in wy.iter().enumerate() {
                    for (i, wxi) in wx.iter().enumerate() {
                        let p = self.get_clamped(x0 - 1 + i as i64, y0 - 1 + j as i64);
                        let w = wxi * wyj;
                        for c in 0..4 {
                            o[c] += p[c] * w;
                        }
                    }
                }
                clamp_premul(o)
            }
        }
    }
}

/// Catmull-Rom weights for the 4 taps around a fractional position.
#[inline]
pub fn catmull_rom(t: f32) -> [f32; 4] {
    let t2 = t * t;
    let t3 = t2 * t;
    [
        0.5 * (-t3 + 2.0 * t2 - t),
        0.5 * (3.0 * t3 - 5.0 * t2 + 2.0),
        0.5 * (-3.0 * t3 + 4.0 * t2 + t),
        0.5 * (t3 - t2),
    ]
}

#[inline]
pub fn clamp_premul(p: Px) -> Px {
    let a = p[3].clamp(0.0, 1.0);
    [p[0].clamp(0.0, a), p[1].clamp(0.0, a), p[2].clamp(0.0, a), a]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleMode {
    Nearest = 0,
    Bilinear = 1,
    Bicubic = 2,
}

/// Straight (non-premultiplied) color from a premultiplied pixel.
#[inline]
pub fn unpremul(p: Px) -> [f32; 3] {
    if p[3] <= 1e-6 { [0.0; 3] } else { [p[0] / p[3], p[1] / p[3], p[2] / p[3]] }
}

#[inline]
pub fn premul(c: [f32; 3], a: f32) -> Px {
    [c[0] * a, c[1] * a, c[2] * a, a]
}

/// A tile as produced by the engine: on the CPU, on the GPU, or a fully
/// transparent tile that has not been materialised at all.
pub enum Tile {
    Clear { w: u32, h: u32 },
    Cpu(CpuTile),
    #[cfg(feature = "gpu")]
    Gpu(super::gpu::GpuTile),
}

impl Tile {
    pub fn dims(&self) -> (u32, u32) {
        match self {
            Tile::Clear { w, h } => (*w, *h),
            Tile::Cpu(t) => (t.w, t.h),
            #[cfg(feature = "gpu")]
            Tile::Gpu(t) => (t.w, t.h),
        }
    }

    pub fn is_clear(&self) -> bool {
        matches!(self, Tile::Clear { .. })
    }
}
