//! Export: the encoder consumes bands rendered ahead on another thread, so
//! output, progress, errors and cancellation must behave as if sequential.

use delos::engine::{is_cancelled, Engine};
use delos::formats::{self, BandSource, GraphBands};
use delos::graph::Graph;
use delos::nodes::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

fn no_cancel() -> bool {
    false
}
fn no_status(_: Option<&str>) {}

/// Noise on a 4096×2600 canvas: three bands (1024 + 1024 + 552 rows).
fn noise_graph() -> Graph {
    let mut g = Graph::new();
    let noise = g.add_node("noise", (0.0, 0.0));
    g.connect(noise, g.output, 0).unwrap();
    g.set_param(g.output, "canvas_w", Value::Int(4096));
    g.set_param(g.output, "canvas_h", Value::Int(2600));
    g
}

fn sequential_rgba(engine: &Engine, g: &Graph) -> Vec<u8> {
    let progress = |_: f32| {};
    let mut bands = GraphBands::new(engine, g, g.output.into(), &no_cancel, &progress, &no_status);
    let mut all = vec![];
    let mut count = 0;
    while let Some((_, band)) = bands.next_band().unwrap() {
        all.extend_from_slice(&band);
        count += 1;
    }
    assert_eq!(count, 3);
    all
}

#[test]
fn export_matches_sequential_render() {
    let engine = Engine::new(false);
    let g = noise_graph();
    let expected = sequential_rgba(&engine, &g);
    for name in ["PNG", "BMP"] {
        let codec = formats::codec_by_name(name).unwrap();
        let fractions = Mutex::new(vec![]);
        let progress = |f: f32| fractions.lock().unwrap().push(f);
        let mut out = vec![];
        formats::export(&engine, &g, g.output.into(), codec, &Default::default(), &mut out, &no_cancel, &progress, &no_status).unwrap();
        let d = formats::decode(&out).unwrap();
        assert_eq!((d.width, d.height), (4096, 2600), "{name}");
        assert!(d.rgba == expected, "{name}: exported pixels differ from a sequential render");
        let f = fractions.into_inner().unwrap();
        assert!(f.windows(2).all(|w| w[0] < w[1]), "{name}: progress not increasing: {f:?}");
        assert_eq!(f.last(), Some(&1.0), "{name}");
    }
}

/// Fails every write after the first `budget` bytes.
struct FailingWriter {
    budget: usize,
}

impl std::io::Write for FailingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.budget == 0 {
            return Err(std::io::Error::other("disk full"));
        }
        let n = buf.len().min(self.budget);
        self.budget -= n;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn write_errors_stop_the_export() {
    let engine = Engine::new(false);
    let g = noise_graph();
    let codec = formats::codec_by_name("PNG").unwrap();
    let progress = |_: f32| {};
    let mut out = FailingWriter { budget: 1 << 20 };
    let e = formats::export(&engine, &g, g.output.into(), codec, &Default::default(), &mut out, &no_cancel, &progress, &no_status).unwrap_err();
    assert!(format!("{e:#}").contains("disk full"), "{e:#}");
}

#[test]
fn cancelling_stops_the_export() {
    let engine = Engine::new(false);
    // Ten bands: rendering can only run two bands ahead of encoding, so most
    // of them are still to be rendered when the export is cancelled.
    let mut g = Graph::new();
    let solid = g.add_node("solid", (0.0, 0.0));
    g.connect(solid, g.output, 0).unwrap();
    g.set_param(g.output, "canvas_w", Value::Int(4096));
    g.set_param(g.output, "canvas_h", Value::Int(10240));
    let codec = formats::codec_by_name("PNG").unwrap();
    let cancelled = AtomicBool::new(false);
    let cancel = || cancelled.load(Ordering::Relaxed);
    // Cancel as soon as the first band has been encoded.
    let progress = |_: f32| cancelled.store(true, Ordering::Relaxed);
    let mut out = vec![];
    let e = formats::export(&engine, &g, g.output.into(), codec, &Default::default(), &mut out, &cancel, &progress, &no_status).unwrap_err();
    assert!(is_cancelled(&e), "{e:#}");
}
