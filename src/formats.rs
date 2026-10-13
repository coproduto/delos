//! Image formats. Each format is a [`Codec`] in [`codecs`]; adding a format
//! means adding one entry. Export renders the image in horizontal bands so
//! the full-resolution floating-point image never exists in memory; the PNG
//! codec streams bands straight to the output.

use crate::engine::geom::{Rect, Request};
use crate::engine::tile::{unpremul, CpuTile};
use crate::engine::{render_to_cpu, Engine};
use crate::graph::{Graph, Link};
use anyhow::Context;
use std::io::{Cursor, Write};

/// Decoded straight-alpha RGBA8 pixels.
pub struct Decoded {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Copy, Debug)]
pub struct EncodeOptions {
    /// 1..=100, for lossy formats.
    pub quality: u8,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        Self { quality: 90 }
    }
}

/// Produces the image to encode, band by band, as straight RGBA8 rows.
pub trait BandSource {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    /// Next band `(rows, rgba)`, or `None` when done.
    fn next_band(&mut self) -> anyhow::Result<Option<(u32, Vec<u8>)>>;
}

pub trait Codec: Send + Sync {
    fn name(&self) -> &'static str;
    fn extensions(&self) -> &'static [&'static str];
    fn mime(&self) -> &'static str;
    fn can_decode(&self, bytes: &[u8]) -> bool;
    fn decode(&self, bytes: &[u8]) -> anyhow::Result<Decoded>;
    fn can_encode(&self) -> bool;
    fn encode(&self, src: &mut dyn BandSource, out: &mut dyn Write, opts: &EncodeOptions) -> anyhow::Result<()>;
}

/// Codec backed by the `image` crate.
struct ImageCrate {
    format: image::ImageFormat,
    name: &'static str,
    exts: &'static [&'static str],
    mime: &'static str,
    encode: bool,
}

impl Codec for ImageCrate {
    fn name(&self) -> &'static str {
        self.name
    }
    fn extensions(&self) -> &'static [&'static str] {
        self.exts
    }
    fn mime(&self) -> &'static str {
        self.mime
    }
    fn can_decode(&self, bytes: &[u8]) -> bool {
        image::guess_format(bytes).ok() == Some(self.format)
    }
    fn decode(&self, bytes: &[u8]) -> anyhow::Result<Decoded> {
        let mut reader = image::ImageReader::with_format(Cursor::new(bytes), self.format);
        // Default limits refuse images above 512 MiB; 16K images are normal here.
        reader.no_limits();
        let img = reader.decode()?.into_rgba8();
        let (width, height) = img.dimensions();
        Ok(Decoded { width, height, rgba: img.into_raw() })
    }
    fn can_encode(&self) -> bool {
        self.encode
    }
    fn encode(&self, src: &mut dyn BandSource, out: &mut dyn Write, opts: &EncodeOptions) -> anyhow::Result<()> {
        let (w, h) = (src.width(), src.height());
        let mut all = Vec::with_capacity(w as usize * h as usize * 4);
        while let Some((_, band)) = src.next_band()? {
            all.extend_from_slice(&band);
        }
        let mut buf = Cursor::new(Vec::new());
        match self.format {
            image::ImageFormat::Jpeg => {
                // No alpha in JPEG: composite over white.
                let rgb: Vec<u8> = all
                    .chunks_exact(4)
                    .flat_map(|p| {
                        let a = p[3] as u32;
                        [0, 1, 2].map(|i| ((p[i] as u32 * a + 255 * (255 - a)) / 255) as u8)
                    })
                    .collect();
                let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, opts.quality.clamp(1, 100));
                image::ImageEncoder::write_image(enc, &rgb, w, h, image::ExtendedColorType::Rgb8)?;
            }
            fmt => {
                let img = image::RgbaImage::from_raw(w, h, all).context("bad buffer")?;
                image::DynamicImage::ImageRgba8(img).write_to(&mut buf, fmt)?;
            }
        }
        out.write_all(buf.get_ref())?;
        Ok(())
    }
}

/// Streaming PNG encoder (constant memory regardless of image size).
struct Png;

impl Codec for Png {
    fn name(&self) -> &'static str {
        "PNG"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["png"]
    }
    fn mime(&self) -> &'static str {
        "image/png"
    }
    fn can_decode(&self, bytes: &[u8]) -> bool {
        bytes.starts_with(b"\x89PNG")
    }
    fn decode(&self, bytes: &[u8]) -> anyhow::Result<Decoded> {
        ImageCrate { format: image::ImageFormat::Png, name: "PNG", exts: &[], mime: "", encode: false }.decode(bytes)
    }
    fn can_encode(&self) -> bool {
        true
    }
    fn encode(&self, src: &mut dyn BandSource, out: &mut dyn Write, _opts: &EncodeOptions) -> anyhow::Result<()> {
        let mut enc = png::Encoder::new(out, src.width(), src.height());
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut writer = enc.write_header()?;
        {
            let mut stream = writer.stream_writer()?;
            while let Some((_, band)) = src.next_band()? {
                stream.write_all(&band)?;
            }
            stream.finish()?;
        }
        Ok(())
    }
}

static CODECS: std::sync::LazyLock<Vec<Box<dyn Codec>>> = std::sync::LazyLock::new(|| {
    use image::ImageFormat as F;
    let c = |format, name, exts, mime, encode| -> Box<dyn Codec> { Box::new(ImageCrate { format, name, exts, mime, encode }) };
    vec![
        Box::new(Png),
        c(F::Jpeg, "JPEG", &["jpg", "jpeg"], "image/jpeg", true),
        c(F::WebP, "WebP (lossless)", &["webp"], "image/webp", true),
        c(F::Gif, "GIF", &["gif"], "image/gif", true),
        c(F::Bmp, "BMP", &["bmp"], "image/bmp", true),
        c(F::Tiff, "TIFF", &["tif", "tiff"], "image/tiff", true),
        c(F::Ico, "ICO", &["ico"], "image/x-icon", false),
    ]
});

pub fn codecs() -> &'static [Box<dyn Codec>] {
    &CODECS
}

pub fn encoders() -> impl Iterator<Item = &'static dyn Codec> {
    CODECS.iter().map(|c| c.as_ref()).filter(|c| c.can_encode())
}

pub fn codec_by_name(name: &str) -> Option<&'static dyn Codec> {
    CODECS.iter().map(|c| c.as_ref()).find(|c| c.name() == name)
}

/// Accept string for file pickers.
pub fn accept_attr() -> String {
    CODECS.iter().flat_map(|c| c.extensions().iter().map(|e| format!(".{e}"))).collect::<Vec<_>>().join(",")
}

pub fn decode(bytes: &[u8]) -> anyhow::Result<Decoded> {
    let codec = CODECS.iter().find(|c| c.can_decode(bytes)).context("unsupported or unrecognised image format")?;
    codec.decode(bytes)
}

/// Converts a premultiplied float tile to straight RGBA8.
pub fn tile_to_rgba8(t: &CpuTile) -> Vec<u8> {
    use rayon::prelude::*;
    const CHUNK: usize = 4096;
    let mut out = vec![0u8; t.data.len() * 4];
    out.par_chunks_mut(CHUNK * 4).zip(t.data.par_chunks(CHUNK)).for_each(|(out, px)| {
        for (o, p) in out.chunks_exact_mut(4).zip(px) {
            let c = unpremul(*p);
            o.copy_from_slice(&[to8(c[0]), to8(c[1]), to8(c[2]), to8(p[3])]);
        }
    });
    out
}

#[inline]
fn to8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// Renders a node at full resolution, band by band.
pub struct GraphBands<'a> {
    pub engine: &'a Engine,
    pub graph: &'a Graph,
    pub target: Link,
    pub width: u32,
    pub height: u32,
    pub band_rows: u32,
    pub y: u32,
    pub cancel: &'a (dyn Fn() -> bool + Sync),
    pub progress: &'a dyn Fn(f32),
    pub status: &'a (dyn Fn(Option<&str>) + Sync),
}

impl<'a> GraphBands<'a> {
    pub fn new(
        engine: &'a Engine,
        graph: &'a Graph,
        target: Link,
        cancel: &'a (dyn Fn() -> bool + Sync),
        progress: &'a dyn Fn(f32),
        status: &'a (dyn Fn(Option<&str>) + Sync),
    ) -> Self {
        let (width, height) = graph.extent_of(target.node).pixels();
        // ~4 Mpx per band keeps the float working set around 64 MiB.
        let band_rows = ((4u64 << 20) / width as u64).clamp(1, height as u64) as u32;
        Self { engine, graph, target, width, height, band_rows, y: 0, cancel, progress, status }
    }
}

impl BandSource for GraphBands<'_> {
    fn width(&self) -> u32 {
        self.width
    }
    fn height(&self) -> u32 {
        self.height
    }
    fn next_band(&mut self) -> anyhow::Result<Option<(u32, Vec<u8>)>> {
        if self.y >= self.height {
            return Ok(None);
        }
        let rows = self.band_rows.min(self.height - self.y);
        let extent = self.graph.extent_of(self.target.node);
        let mut req = Request::new(extent, Rect::new(0.0, self.y as f64, self.width as f64, rows as f64), self.width, rows);
        req.exact = true;
        let tile = render_to_cpu(self.engine, self.graph, self.target, &req, self.cancel, self.status)?;
        self.y += rows;
        (self.progress)(self.y as f32 / self.height as f32);
        Ok(Some((rows, tile_to_rgba8(&tile))))
    }
}

pub fn export(
    engine: &Engine,
    graph: &Graph,
    target: Link,
    codec: &dyn Codec,
    opts: &EncodeOptions,
    out: &mut dyn Write,
    cancel: &(dyn Fn() -> bool + Sync),
    progress: &dyn Fn(f32),
    status: &(dyn Fn(Option<&str>) + Sync),
) -> anyhow::Result<()> {
    #[cfg(feature = "native")]
    {
        // Render on another thread so encoding a band overlaps rendering the
        // next one; the channel keeps at most one finished band waiting.
        let (width, height) = graph.extent_of(target.node).pixels();
        std::thread::scope(|s| {
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            s.spawn(move || {
                let no_progress = |_: f32| {};
                let mut bands = GraphBands::new(engine, graph, target, cancel, &no_progress, status);
                while let Some(band) = bands.next_band().transpose() {
                    let failed = band.is_err();
                    // A failed send means the encoder stopped early.
                    if tx.send(band).is_err() || failed {
                        break;
                    }
                }
            });
            let mut src = Prefetched { width, height, done: 0, rx, progress };
            let r = codec.encode(&mut src, out, opts);
            // Unblocks the render thread if encoding stopped early.
            drop(src);
            r
        })
    }
    #[cfg(not(feature = "native"))]
    {
        let mut src = GraphBands::new(engine, graph, target, cancel, progress, status);
        codec.encode(&mut src, out, opts)
    }
}

/// Bands rendered ahead on another thread (see [`export`]).
#[cfg(feature = "native")]
struct Prefetched<'a> {
    width: u32,
    height: u32,
    done: u32,
    rx: std::sync::mpsc::Receiver<anyhow::Result<(u32, Vec<u8>)>>,
    progress: &'a dyn Fn(f32),
}

#[cfg(feature = "native")]
impl BandSource for Prefetched<'_> {
    fn width(&self) -> u32 {
        self.width
    }
    fn height(&self) -> u32 {
        self.height
    }
    fn next_band(&mut self) -> anyhow::Result<Option<(u32, Vec<u8>)>> {
        // The render thread hangs up after the last band.
        let Ok(band) = self.rx.recv() else { return Ok(None) };
        let (rows, rgba) = band?;
        self.done += rows;
        (self.progress)(self.done as f32 / self.height as f32);
        Ok(Some((rows, rgba)))
    }
}
