//! Decoded source images with a mip pyramid, so any region can be rendered
//! at any scale in time proportional to the *output* size.

use super::geom::{Request, Sampling};
use super::tile::{CpuTile, CLEAR};
use rayon::prelude::*;

/// One pyramid level: premultiplied RGBA8, row-major.
struct Level {
    w: u32,
    h: u32,
    data: Vec<u8>,
}

pub struct SourceImage {
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
        Self { name: name.into(), width, height, levels, file: Default::default() }
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
        let fetch = |x: i64, y: i64| -> [f32; 4] {
            let x = x.clamp(0, lv.w as i64 - 1) as usize;
            let y = y.clamp(0, lv.h as i64 - 1) as usize;
            let o = (y * lv.w as usize + x) * 4;
            let d = &lv.data[o..o + 4];
            [lut[d[0] as usize], lut[d[1] as usize], lut[d[2] as usize], lut[d[3] as usize]]
        };
        let bilinear = level > 0;
        CpuTile::from_fn_par(req.width, req.height, |i, j| {
            let nx = r.x + (i as f64 + 0.5) * sx;
            let ny = r.y + (j as f64 + 0.5) * sy;
            if nx < 0.0 || ny < 0.0 || nx >= iw || ny >= ih {
                return CLEAR;
            }
            let u = nx * kx;
            let v = ny * ky;
            if !bilinear {
                return fetch(u as i64, v as i64);
            }
            let x = u - 0.5;
            let y = v - 0.5;
            let x0 = x.floor();
            let y0 = y.floor();
            let fx = (x - x0) as f32;
            let fy = (y - y0) as f32;
            let (x0, y0) = (x0 as i64, y0 as i64);
            let a = fetch(x0, y0);
            let b = fetch(x0 + 1, y0);
            let c = fetch(x0, y0 + 1);
            let d = fetch(x0 + 1, y0 + 1);
            let mut o = [0.0; 4];
            for k in 0..4 {
                let t = a[k] + (b[k] - a[k]) * fx;
                let bt = c[k] + (d[k] - c[k]) * fx;
                o[k] = t + (bt - t) * fy;
            }
            o
        })
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
