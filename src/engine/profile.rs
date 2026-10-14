//! Opt-in timing of every engine operation: each kernel on either backend,
//! CPU sampling of source images, and transfers to and from the GPU.
//!
//! Disabled by default; [`Profile::enable`] starts recording. CPU work is
//! timed with wall-clock time. GPU kernels are timed on the GPU with
//! timestamp queries, which measure execution alone (not the queueing before
//! it); their samples arrive once the GPU has run them, so call
//! [`super::Engine::profile_samples`] to wait for outstanding ones.
//!
//! Copies to and from the GPU can't carry GPU timestamps on every device
//! (Apple GPUs only time whole passes), so they are timed on the CPU:
//! `upload` is the time to queue the data, `readback wait` the time until
//! the GPU has finished everything queued before it, and `readback copy`
//! the copy out of the mapped buffer.

use std::fmt::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Backend {
    Cpu,
    Gpu,
}

#[derive(Debug, Clone)]
pub struct Sample {
    /// Kernel name, `source` (sampling a source image), `upload`,
    /// `readback wait` or `readback copy`.
    pub op: &'static str,
    pub backend: Backend,
    /// Output pixels (transferred pixels for transfers).
    pub pixels: u64,
    pub time: Duration,
}

#[derive(Clone, Default)]
pub struct Profile {
    enabled: Arc<AtomicBool>,
    samples: Arc<Mutex<Vec<Sample>>>,
}

impl Profile {
    pub fn enable(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn record(&self, op: &'static str, backend: Backend, pixels: u64, time: Duration) {
        self.samples.lock().unwrap().push(Sample { op, backend, pixels, time });
    }

    /// Runs `f`, recording its wall-clock time when enabled.
    pub fn time<T>(&self, op: &'static str, backend: Backend, pixels: u64, f: impl FnOnce() -> T) -> T {
        if !self.enabled() {
            return f();
        }
        let t = Instant::now();
        let r = f();
        self.record(op, backend, pixels, t.elapsed());
        r
    }

    /// Removes and returns the samples recorded so far.
    pub(super) fn take(&self) -> Vec<Sample> {
        std::mem::take(&mut self.samples.lock().unwrap())
    }
}

/// Totals for one operation on one backend.
#[derive(Debug, Clone)]
pub struct Summary {
    pub op: &'static str,
    pub backend: Backend,
    pub count: usize,
    pub pixels: u64,
    pub total: Duration,
    pub median: Duration,
}

impl Summary {
    /// Throughput in megapixels per second.
    pub fn mpx_per_s(&self) -> f64 {
        self.pixels as f64 / 1e6 / self.total.as_secs_f64().max(1e-12)
    }
}

/// Groups samples by operation and backend, largest total first.
pub fn summarize(samples: &[Sample]) -> Vec<Summary> {
    let mut groups: std::collections::BTreeMap<(&'static str, Backend), Vec<&Sample>> = Default::default();
    for s in samples {
        groups.entry((s.op, s.backend)).or_default().push(s);
    }
    let mut out: Vec<Summary> = groups
        .into_iter()
        .map(|((op, backend), v)| {
            let mut times: Vec<Duration> = v.iter().map(|s| s.time).collect();
            times.sort();
            Summary {
                op,
                backend,
                count: v.len(),
                pixels: v.iter().map(|s| s.pixels).sum(),
                total: times.iter().sum(),
                median: times[times.len() / 2],
            }
        })
        .collect();
    out.sort_by_key(|s| std::cmp::Reverse(s.total));
    out
}

/// A plain-text table of [`summarize`]d samples.
pub fn report(samples: &[Sample]) -> String {
    let mut s = format!("{:<16} {:>4} {:>6} {:>10} {:>10} {:>9}\n", "op", "on", "count", "total ms", "median ms", "Mpx/s");
    for r in summarize(samples) {
        let on = if r.backend == Backend::Cpu { "CPU" } else { "GPU" };
        let _ = writeln!(
            s,
            "{:<16} {:>4} {:>6} {:>10.3} {:>10.3} {:>9.0}",
            r.op,
            on,
            r.count,
            r.total.as_secs_f64() * 1e3,
            r.median.as_secs_f64() * 1e3,
            r.mpx_per_s()
        );
    }
    s
}
