//! Background work: previews, imports and exports. On native platforms
//! everything runs off the UI thread (previews on a dedicated thread with
//! "latest request wins" semantics, so stale renders are abandoned as soon
//! as a newer one arrives). On the web the same jobs run cooperatively.

use crate::engine::geom::{Rect, Request, Size};
use crate::engine::source::SourceImage;
use crate::engine::{is_cancelled, render_to_cpu, Engine, NodeError};
use crate::formats::{self, EncodeOptions};
use crate::graph::{Graph, Link, NodeId};
use futures_channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub struct PreviewJob {
    pub seq: u64,
    pub graph: Graph,
    pub target: Link,
    pub req: Request,
}

pub enum ExportDest {
    #[cfg(feature = "native")]
    File(std::path::PathBuf),
    Memory,
}

pub struct ExportJob {
    pub id: u64,
    pub graph: Graph,
    pub target: Link,
    pub codec: String,
    pub opts: EncodeOptions,
    pub dest: ExportDest,
    pub file_name: String,
}

pub struct Frame {
    pub seq: u64,
    pub region: Rect,
    pub extent: Size,
    pub width: u32,
    pub height: u32,
    /// Display-ready BMP (composited over a checkerboard).
    pub bmp: Arc<Vec<u8>>,
    pub millis: f64,
    pub provisional: bool,
}

pub enum Msg {
    Frame(Frame),
    RenderError { seq: u64, node: Option<NodeId>, message: String },
    Status { seq: u64, text: Option<String> },
    Imported { name: String, result: Result<Arc<SourceImage>, String>, token: u64 },
    ExportProgress { id: u64, fraction: f32, status: Option<String> },
    ExportDone { id: u64, result: Result<ExportOutput, String> },
    /// A project was created and the given images copied into it.
    #[cfg(feature = "native")]
    ProjectCreated { result: Result<crate::project::Project, String> },
    #[cfg(feature = "native")]
    ProjectOpened { result: Result<(crate::project::Project, Graph), String> },
    /// `project.delos` was written (`seq` orders autosaves).
    #[cfg(feature = "native")]
    ProjectSaved { seq: u64, result: Result<(), String> },
    #[cfg(feature = "native")]
    ProjectShared { result: Result<String, String> },
}

/// Where an imported image's bytes come from.
#[derive(Clone, Debug, Default)]
pub struct ImportOptions {
    /// The file the bytes were read from, if known.
    pub origin: Option<std::path::PathBuf>,
    /// Keep a copy of the bytes in the session stash when there is no
    /// origin, so a project can later copy the file unchanged.
    pub keep_bytes: bool,
}

pub enum ExportOutput {
    Saved(String),
    Bytes { name: String, mime: String, bytes: Vec<u8> },
}

pub struct Worker {
    pub engine: Arc<Engine>,
    tx: UnboundedSender<Msg>,
    latest: Arc<AtomicU64>,
    last_ms: Arc<Mutex<f64>>,
    exports: Mutex<std::collections::HashMap<u64, Arc<AtomicBool>>>,
    /// Highest autosave sequence written so far (older saves are skipped).
    /// Also serializes saves with sharing.
    #[cfg(feature = "native")]
    saved_seq: Arc<Mutex<u64>>,
    /// Session folder (outside any project) for image files the project no
    /// longer uses but the undo history may bring back.
    #[cfg(feature = "native")]
    stash: std::path::PathBuf,
    #[cfg(feature = "native")]
    slot: Arc<(Mutex<Option<PreviewJob>>, std::sync::Condvar)>,
}

pub fn now_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window().and_then(|w| w.performance()).map(|p| p.now()).unwrap_or(0.0)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0)
    }
}

impl Worker {
    pub fn new() -> (Arc<Worker>, UnboundedReceiver<Msg>) {
        let (tx, rx) = unbounded();
        let engine = Engine::new(true);
        let w = Arc::new(Worker {
            engine,
            tx,
            latest: Arc::new(AtomicU64::new(0)),
            last_ms: Arc::new(Mutex::new(0.0)),
            exports: Mutex::new(Default::default()),
            #[cfg(feature = "native")]
            saved_seq: Arc::new(Mutex::new(0)),
            #[cfg(feature = "native")]
            stash: new_stash_dir(),
            #[cfg(feature = "native")]
            slot: Arc::new((Mutex::new(None), std::sync::Condvar::new())),
        });
        #[cfg(feature = "native")]
        {
            let w2 = w.clone();
            std::thread::Builder::new()
                .name("delos-preview".into())
                .spawn(move || {
                    w2.engine.warm_up();
                    loop {
                        let job = {
                            let (lock, cv) = &*w2.slot;
                            let mut g = lock.lock().unwrap();
                            while g.is_none() {
                                g = cv.wait(g).unwrap();
                            }
                            g.take().unwrap()
                        };
                        w2.run_preview(job);
                    }
                })
                .expect("spawn preview thread");
        }
        (w, rx)
    }

    pub fn backend_name(&self) -> String {
        self.engine.backend_name()
    }

    pub fn preview(self: &Arc<Self>, job: PreviewJob) {
        self.latest.store(job.seq, Ordering::SeqCst);
        #[cfg(feature = "native")]
        {
            let (lock, cv) = &*self.slot;
            *lock.lock().unwrap() = Some(job);
            cv.notify_one();
        }
        #[cfg(not(feature = "native"))]
        {
            let me = self.clone();
            spawn_local(async move {
                yield_now().await;
                if me.latest.load(Ordering::SeqCst) == job.seq {
                    me.run_preview(job);
                }
            });
        }
    }

    fn run_preview(&self, job: PreviewJob) {
        let seq = job.seq;
        let latest = self.latest.clone();
        let cancel = move || latest.load(Ordering::Relaxed) != seq;
        let tx = self.tx.clone();
        let last_status = Mutex::new(None::<String>);
        let status = move |s: Option<&str>| {
            let s = s.map(str::to_string);
            let mut l = last_status.lock().unwrap();
            if *l != s {
                *l = s.clone();
                let _ = tx.unbounded_send(Msg::Status { seq, text: s });
            }
        };

        // Progressive refinement: when full renders are slow, show a quick
        // quarter-resolution frame first.
        let slow = *self.last_ms.lock().unwrap() > 80.0;
        if slow && job.req.pixel_count() > 200_000 {
            let mut quick = job.req;
            quick.width = (quick.width / 4).max(1);
            quick.height = (quick.height / 4).max(1);
            let t0 = now_ms();
            if let Ok(tile) = render_to_cpu(&self.engine, &job.graph, job.target, &quick, &cancel, &status) {
                if !cancel() {
                    self.send_frame(&job, &quick, &tile, now_ms() - t0, true);
                }
            }
        }
        if cancel() {
            return;
        }
        let t0 = now_ms();
        match render_to_cpu(&self.engine, &job.graph, job.target, &job.req, &cancel, &status) {
            Ok(tile) => {
                let ms = now_ms() - t0;
                *self.last_ms.lock().unwrap() = ms;
                self.send_frame(&job, &job.req, &tile, ms, false);
            }
            Err(e) if is_cancelled(&e) => {}
            Err(e) => {
                let node = e.downcast_ref::<NodeError>().map(|n| n.node);
                let _ = self.tx.unbounded_send(Msg::RenderError { seq, node, message: format!("{e:#}") });
            }
        }
        status(None);
    }

    fn send_frame(&self, job: &PreviewJob, req: &Request, tile: &crate::engine::tile::CpuTile, ms: f64, provisional: bool) {
        let bmp = encode_display_bmp(tile, req);
        let _ = self.tx.unbounded_send(Msg::Frame(Frame {
            seq: job.seq,
            region: req.region,
            extent: req.extent,
            width: tile.w,
            height: tile.h,
            bmp: Arc::new(bmp),
            millis: ms,
            provisional,
        }));
    }

    /// Decodes an image; without `bytes` it is read from `opts.origin`.
    pub fn import(self: &Arc<Self>, name: String, bytes: Option<Vec<u8>>, token: u64, opts: ImportOptions) {
        let tx = self.tx.clone();
        let engine = self.engine.clone();
        #[cfg(feature = "native")]
        let stash = self.stash.clone();
        let work = move || {
            let result = (|| -> anyhow::Result<Arc<SourceImage>> {
                #[cfg_attr(not(feature = "native"), allow(unused_mut))]
                let mut origin = opts.origin;
                let bytes = match (bytes, &origin) {
                    (Some(b), _) => b,
                    (None, Some(path)) => std::fs::read(path)?,
                    (None, None) => anyhow::bail!("nothing to read"),
                };
                let d = formats::decode(&bytes)?;
                #[cfg(feature = "native")]
                if origin.is_none() && opts.keep_bytes {
                    let dir = stash.join("imported");
                    std::fs::create_dir_all(&dir)?;
                    let path = dir.join(format!("{token}-{}", crate::project::sanitize_name(&name)));
                    std::fs::write(&path, &bytes)?;
                    origin = Some(path);
                }
                drop(bytes);
                let img = SourceImage::from_rgba8(name.clone(), d.width, d.height, d.rgba);
                if let Some(origin) = origin {
                    img.set_origin(origin);
                }
                let img = Arc::new(img);
                engine.make_resident(&img);
                Ok(img)
            })()
            .map_err(|e| format!("{e:#}"));
            let _ = tx.unbounded_send(Msg::Imported { name, result, token });
        };
        run_background(work);
    }

    pub fn export(self: &Arc<Self>, job: ExportJob) {
        let cancel_flag = Arc::new(AtomicBool::new(false));
        self.exports.lock().unwrap().insert(job.id, cancel_flag.clone());
        let tx = self.tx.clone();
        let engine = self.engine.clone();
        let work = move || {
            let id = job.id;
            let cancel = move || cancel_flag.load(Ordering::Relaxed);
            let tx2 = tx.clone();
            let progress = move |f: f32| {
                let _ = tx2.unbounded_send(Msg::ExportProgress { id, fraction: f, status: None });
            };
            let tx3 = tx.clone();
            let status = move |s: Option<&str>| {
                let _ = tx3.unbounded_send(Msg::ExportProgress { id, fraction: -1.0, status: s.map(str::to_string) });
            };
            let result = (|| -> anyhow::Result<ExportOutput> {
                let codec = formats::codec_by_name(&job.codec).ok_or_else(|| anyhow::anyhow!("unknown format"))?;
                match job.dest {
                    #[cfg(feature = "native")]
                    ExportDest::File(path) => {
                        let f = std::fs::File::create(&path)?;
                        let mut w = std::io::BufWriter::with_capacity(1 << 20, f);
                        let r = formats::export(&engine, &job.graph, job.target, codec, &job.opts, &mut w, &cancel, &progress, &status);
                        if r.is_err() {
                            drop(w);
                            let _ = std::fs::remove_file(&path);
                        }
                        r?;
                        Ok(ExportOutput::Saved(path.display().to_string()))
                    }
                    ExportDest::Memory => {
                        let mut buf = Vec::new();
                        formats::export(&engine, &job.graph, job.target, codec, &job.opts, &mut buf, &cancel, &progress, &status)?;
                        Ok(ExportOutput::Bytes { name: job.file_name.clone(), mime: codec.mime().to_string(), bytes: buf })
                    }
                }
            })()
            .map_err(|e| if is_cancelled(&e) { "Export cancelled".to_string() } else { format!("{e:#}") });
            let _ = tx.unbounded_send(Msg::ExportDone { id, result });
        };
        run_background(work);
    }

    /// Creates an empty project at `dir`; the first save copies in the
    /// images the graph uses.
    #[cfg(feature = "native")]
    pub fn create_project(&self, dir: std::path::PathBuf) {
        let tx = self.tx.clone();
        run_background(move || {
            let result = crate::project::Project::create(&dir).map_err(|e| format!("{e:#}"));
            let _ = tx.unbounded_send(Msg::ProjectCreated { result });
        });
    }

    /// Opens a project folder, `project.delos` file or `.delosproject`
    /// archive (unpacked under `root`).
    #[cfg(feature = "native")]
    pub fn open_project(&self, path: std::path::PathBuf, root: std::path::PathBuf) {
        let tx = self.tx.clone();
        let engine = self.engine.clone();
        run_background(move || {
            let result = crate::project::open(&path, &root).map_err(|e| format!("{e:#}"));
            if let Ok((_, graph)) = &result {
                for img in graph.images.values() {
                    engine.make_resident(img);
                }
            }
            let _ = tx.unbounded_send(Msg::ProjectOpened { result });
        });
    }

    /// Saves `graph` to the project unless a newer save already ran: makes
    /// `inputs/` match the graph (see `Project::sync_inputs`; `known` are all
    /// images the session holds), then writes `project.delos`.
    #[cfg(feature = "native")]
    pub fn save_project(&self, project: crate::project::Project, graph: Graph, known: Vec<Arc<SourceImage>>, seq: u64) {
        let tx = self.tx.clone();
        let saved = self.saved_seq.clone();
        let stash = self.stash.clone();
        run_background(move || {
            let mut last = saved.lock().unwrap();
            if *last > seq {
                return;
            }
            let result = (|| -> anyhow::Result<()> {
                project.sync_inputs(&graph, &known, &stash)?;
                let text = crate::expr::to_text(&graph).map_err(|e| anyhow::anyhow!(e))?;
                if std::fs::read_to_string(project.file()).ok().as_deref() != Some(text.as_str()) {
                    project.save_text(&text)?;
                }
                Ok(())
            })()
            .map_err(|e| format!("{e:#}"));
            *last = seq;
            drop(last);
            let _ = tx.unbounded_send(Msg::ProjectSaved { seq, result });
        });
    }

    #[cfg(feature = "native")]
    pub fn share_project(&self, project: crate::project::Project, dest: std::path::PathBuf) {
        let tx = self.tx.clone();
        let saved = self.saved_seq.clone();
        run_background(move || {
            // Waits for a save in progress, so the archive is consistent.
            let _lock = saved.lock().unwrap();
            let result = project.share(&dest).map(|_| dest.display().to_string()).map_err(|e| format!("{e:#}"));
            let _ = tx.unbounded_send(Msg::ProjectShared { result });
        });
    }

    pub fn cancel_export(&self, id: u64) {
        if let Some(f) = self.exports.lock().unwrap().get(&id) {
            f.store(true, Ordering::Relaxed);
        }
    }
}

/// A fresh per-session stash folder in the system temp directory. Stashes
/// left by earlier sessions more than a day old are removed.
#[cfg(feature = "native")]
fn new_stash_dir() -> std::path::PathBuf {
    let base = std::env::temp_dir().join("delos-stash");
    if let Ok(entries) = std::fs::read_dir(&base) {
        let day = std::time::Duration::from_secs(24 * 3600);
        for e in entries.flatten() {
            let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > day);
            if old {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    static N: AtomicU64 = AtomicU64::new(0);
    base.join(format!("{}-{}-{}", std::process::id(), now_ms() as u64, N.fetch_add(1, Ordering::Relaxed)))
}

fn run_background(work: impl FnOnce() + Send + 'static) {
    #[cfg(feature = "native")]
    std::thread::spawn(work);
    #[cfg(not(feature = "native"))]
    spawn_local(async move {
        yield_now().await;
        work();
    });
}

#[cfg(not(feature = "native"))]
fn spawn_local(f: impl std::future::Future<Output = ()> + 'static) {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(f);
    #[cfg(not(target_arch = "wasm32"))]
    futures_util::FutureExt::now_or_never(f).unwrap_or(());
}

/// Gives the browser a chance to paint and deliver newer events.
#[cfg(not(feature = "native"))]
async fn yield_now() {
    #[cfg(target_arch = "wasm32")]
    {
        let p = js_sys::Promise::new(&mut |resolve, _| {
            if let Some(w) = web_sys::window() {
                let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 0);
            }
        });
        let _ = wasm_bindgen_futures::JsFuture::from(p).await;
    }
}

/// Composites a premultiplied tile over a checkerboard (anchored to image
/// coordinates) and encodes it as an uncompressed top-down 24-bit BMP, which
/// every webview decodes essentially for free.
pub fn encode_display_bmp(t: &crate::engine::tile::CpuTile, req: &Request) -> Vec<u8> {
    use rayon::prelude::*;
    let (w, h) = (t.w as usize, t.h as usize);
    let row = (w * 3).div_ceil(4) * 4;
    let size = 54 + row * h;
    let mut out = vec![0u8; size];
    out[0..2].copy_from_slice(b"BM");
    out[2..6].copy_from_slice(&(size as u32).to_le_bytes());
    out[10..14].copy_from_slice(&54u32.to_le_bytes());
    out[14..18].copy_from_slice(&40u32.to_le_bytes());
    out[18..22].copy_from_slice(&(w as i32).to_le_bytes());
    out[22..26].copy_from_slice(&(-(h as i32)).to_le_bytes());
    out[26..28].copy_from_slice(&1u16.to_le_bytes());
    out[28..30].copy_from_slice(&24u16.to_le_bytes());
    out[34..38].copy_from_slice(&((row * h) as u32).to_le_bytes());
    let (sx, sy) = (req.step_x(), req.step_y());
    let cell_x = 10.0 * sx;
    let cell_y = 10.0 * sy;
    let (rx, ry) = (req.region.x, req.region.y);
    out[54..].par_chunks_mut(row).enumerate().for_each(|(y, line)| {
        let cy = ((ry + (y as f64 + 0.5) * sy) / cell_y).floor() as i64;
        for x in 0..w {
            let cx = ((rx + (x as f64 + 0.5) * sx) / cell_x).floor() as i64;
            let bg = if (cx + cy) & 1 == 0 { 0.36f32 } else { 0.28 };
            let p = t.data[y * w + x];
            let k = 1.0 - p[3];
            let px = |c: f32| ((c + bg * k).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            line[x * 3] = px(p[2]);
            line[x * 3 + 1] = px(p[1]);
            line[x * 3 + 2] = px(p[0]);
        }
    });
    out
}
