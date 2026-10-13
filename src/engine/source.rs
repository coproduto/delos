//! Decoded source images with a mip pyramid, so any region can be rendered
//! at any scale in time proportional to the *output* size.

use super::geom::{Request, Sampling};
use super::tile::CpuTile;
use rayon::prelude::*;

/// One pyramid level: premultiplied RGBA8, row-major.
struct Level {
    w: u32,
    h: u32,
    data: Vec<u8>,
}

pub struct SourceImage {
    /// Unique for the life of the process (identifies the image's GPU copy).
    pub id: u64,
    pub name: String,
    pub width: u32,
    pub height: u32,
    levels: Vec<Level>,
    /// Where the image is stored. Shared by every graph snapshot (undo
    /// history) holding the image, so all of them stay saveable.
    file: std::sync::Mutex<ImageFile>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImageFile {
    /// Path inside the project (`inputs/…`) when the image belongs to one.
    pub project: Option<String>,
    /// The file it was imported from, if known (used to copy it into a
    /// project later).
    pub origin: Option<std::path::PathBuf>,
}

static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

static U8_TO_F32: std::sync::LazyLock<[f32; 256]> = std::sync::LazyLock::new(|| std::array::from_fn(|i| i as f32 / 255.0));

impl SourceImage {
    /// Takes ownership of straight-alpha RGBA8 pixels (reusing the buffer
    /// for level 0) and builds the pyramid.
    pub fn from_rgba8(name: impl Into<String>, width: u32, height: u32, mut data: Vec<u8>) -> Self {
        assert_eq!(data.len(), width as usize * height as usize * 4);
        data.par_chunks_mut(4 * 4096).for_each(|chunk| {
            for px in chunk.chunks_exact_mut(4) {
                let a = px[3] as u32;
                if a != 255 {
                    px[0] = ((px[0] as u32 * a + 127) / 255) as u8;
                    px[1] = ((px[1] as u32 * a + 127) / 255) as u8;
                    px[2] = ((px[2] as u32 * a + 127) / 255) as u8;
                }
            }
        });
        let mut levels = vec![Level { w: width, h: height, data }];
        while {
            let l = levels.last().unwrap();
            l.w > 1 || l.h > 1
        } {
            let next = downsample(levels.last().unwrap());
            levels.push(next);
        }
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self { id, name: name.into(), width, height, levels, file: Default::default() }
    }

    pub fn file(&self) -> ImageFile {
        self.file.lock().unwrap().clone()
    }

    pub fn set_project_file(&self, path: impl Into<String>) {
        self.file.lock().unwrap().project = Some(path.into());
    }

    pub fn clear_project_file(&self) {
        self.file.lock().unwrap().project = None;
    }

    pub fn set_origin(&self, path: std::path::PathBuf) {
        self.file.lock().unwrap().origin = Some(path);
    }

    pub fn level_count(&self) -> usize {
        self.levels.len()
    }

    /// Pyramid level `i` (0 is full size): width, height and premultiplied
    /// RGBA8 pixels, row-major.
    pub fn level(&self, i: usize) -> (u32, u32, &[u8]) {
        let l = &self.levels[i];
        (l.w, l.h, &l.data)
    }

    pub fn bytes(&self) -> usize {
        self.levels.iter().map(|l| l.data.len()).sum()
    }

    /// Straight-alpha RGBA8 copy of the full-resolution image.
    pub fn to_rgba8(&self) -> Vec<u8> {
        let mut out = self.levels[0].data.clone();
        for px in out.chunks_exact_mut(4) {
            let a = px[3] as u32;
            if a != 0 && a != 255 {
                for c in &mut px[..3] {
                    *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
                }
            }
        }
        out
    }

    pub fn render(&self, req: &Request) -> CpuTile {
        let sx = req.step_x();
        let sy = req.step_y();
        let step = sx.min(sy);
        let level = if req.sampling == Sampling::Point || step <= 1.0 {
            0
        } else {
            (step.log2().floor() as usize).min(self.levels.len() - 1)
        };
        let lv = &self.levels[level];
        let lut = &*U8_TO_F32;
        let (iw, ih) = (self.width as f64, self.height as f64);
        // Scale from node space to this level's texel space.
        let kx = lv.w as f64 / iw;
        let ky = lv.h as f64 / ih;
        let r = req.region;
        let bilinear = level > 0;
        // The grid is separable: work out where each column and each row
        // samples the level once, rather than per pixel.
        let cols: Vec<Tap> = (0..req.width).map(|i| Tap::new(r.x + (i as f64 + 0.5) * sx, iw, kx, lv.w, bilinear)).collect();
        let mut t = CpuTile::new(req.width, req.height);
        let lw = lv.w as usize * 4;
        if req.width == 0 {
            return t;
        }
        let px = |row: &[u8], i: usize| -> [f32; 4] {
            let d = &row[i * 4..i * 4 + 4];
            [lut[d[0] as usize], lut[d[1] as usize], lut[d[2] as usize], lut[d[3] as usize]]
        };
        t.data.par_chunks_mut(req.width as usize).enumerate().for_each(|(j, out)| {
            let row = Tap::new(r.y + (j as f64 + 0.5) * sy, ih, ky, lv.h, bilinear);
            if row.outside {
                return; // tiles start transparent
            }
            let r0 = &lv.data[row.i0 * lw..][..lw];
            let r1 = &lv.data[row.i1 * lw..][..lw];
            for (o, c) in out.iter_mut().zip(&cols) {
                if c.outside {
                    continue;
                }
                if !bilinear {
                    *o = px(r0, c.i0);
                    continue;
                }
                let (a, b, cc, d) = (px(r0, c.i0), px(r0, c.i1), px(r1, c.i0), px(r1, c.i1));
                for k in 0..4 {
                    let t = a[k] + (b[k] - a[k]) * c.f;
                    let bt = cc[k] + (d[k] - cc[k]) * c.f;
                    o[k] = t + (bt - t) * row.f;
                }
            }
        });
        t
    }
}

/// Where one output column (or row) samples a pyramid level: the texels to
/// interpolate between and the weight of the second.
struct Tap {
    outside: bool,
    i0: usize,
    i1: usize,
    f: f32,
}

impl Tap {
    /// `n` is the pixel center in node space, `size` the image's size and `k`
    /// the scale to the level's `len` texels.
    fn new(n: f64, size: f64, k: f64, len: u32, bilinear: bool) -> Self {
        let outside = n < 0.0 || n >= size;
        let clamp = |i: i64| i.clamp(0, len as i64 - 1) as usize;
        let u = n * k;
        if !bilinear {
            let i = clamp(u as i64);
            return Tap { outside, i0: i, i1: i, f: 0.0 };
        }
        let x = u - 0.5;
        let x0 = x.floor();
        let f = (x - x0) as f32;
        let x0 = x0 as i64;
        Tap { outside, i0: clamp(x0), i1: clamp(x0 + 1), f }
    }
}

fn downsample(src: &Level) -> Level {
    let w = (src.w / 2).max(1);
    let h = (src.h / 2).max(1);
    let mut data = vec![0u8; w as usize * h as usize * 4];
    let sw = src.w as usize;
    data.par_chunks_mut(w as usize * 4).enumerate().for_each(|(y, row)| {
        let y0 = (y * 2).min(src.h as usize - 1);
        let y1 = (y * 2 + 1).min(src.h as usize - 1);
        for x in 0..w as usize {
            let x0 = (x * 2).min(sw - 1);
            let x1 = (x * 2 + 1).min(sw - 1);
            for c in 0..4 {
                let s = src.data[(y0 * sw + x0) * 4 + c] as u32
                    + src.data[(y0 * sw + x1) * 4 + c] as u32
                    + src.data[(y1 * sw + x0) * 4 + c] as u32
                    + src.data[(y1 * sw + x1) * 4 + c] as u32;
                row[x * 4 + c] = ((s + 2) / 4) as u8;
            }
        }
    });
    Level { w, h, data }
}
