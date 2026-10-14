//! Probes for spec clauses the traceability matrix marks as violated. Each
//! test asserts what the spec says; a FAILING test confirms the finding.
//! Run with `spectrace probe system-models/spec-traceability`, which copies
//! this file into `tests/` for the run and removes it afterwards.

use delos::project::{self, ARCHIVE_EXT, PROJECT_FILE};
use delos::ui::session::{self, SaveState};
use delos::ui::state::App;
use delos::ui::{use_services, WorkerHandle};
use dioxus::dioxus_core::NoOpMutations;
use dioxus::prelude::*;
use std::cell::RefCell;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_fn(w, h, |x, y| image::Rgba([x as u8 * 40, y as u8 * 40, 90, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, bytes) in entries {
        zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}

/// P15: "A failed import leaves nothing behind." The archive's structure is
/// valid (so `verify` passes inside `unpack`), but the graph names a node
/// kind this version does not know, so `load` fails after unpacking.
#[test]
fn failed_import_leaves_nothing_behind() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join(format!("future.{ARCHIVE_EXT}"));
    let img = png(4, 4);
    write_zip(&archive, &[(PROJECT_FILE, b"(output (sparkle (image \"inputs/a.png\")))"), ("inputs/a.png", &img)]);
    let root = tmp.path().join("projects");
    let err = project::open(&archive, &root).err().expect("the import fails");
    assert!(format!("{err:#}").contains("sparkle"), "{err:#}");
    let left: Vec<_> = std::fs::read_dir(&root).map(|d| d.flatten().map(|e| e.file_name()).collect()).unwrap_or_default();
    assert!(left.is_empty(), "a failed import left {left:?} in the projects folder");
}

// ---------------------------------------------------------------------------
// Headless application, as in tests/project_e2e.rs.

thread_local! {
    static SERVICES: RefCell<Option<(App, WorkerHandle)>> = const { RefCell::new(None) };
}

#[allow(non_snake_case)]
fn Harness() -> Element {
    let services = use_services();
    use_hook(|| SERVICES.with(|s| *s.borrow_mut() = Some(services.clone())));
    rsx! {}
}

struct Delos {
    dom: VirtualDom,
    app: App,
    worker: WorkerHandle,
}

impl Delos {
    fn start() -> Delos {
        let mut dom = VirtualDom::new(Harness);
        dom.rebuild_in_place();
        let (app, worker) = SERVICES.with(|s| s.borrow_mut().take()).expect("services");
        Delos { dom, app, worker }
    }

    fn run<T>(&mut self, f: impl FnOnce(App, WorkerHandle) -> T) -> T {
        let (app, worker) = (self.app, self.worker.clone());
        let out = self.dom.in_scope(ScopeId::ROOT, || f(app, worker));
        self.dom.render_immediate(&mut NoOpMutations);
        out
    }

    async fn pump_for(&mut self, d: Duration) {
        let end = Instant::now() + d;
        while Instant::now() < end {
            let _ = tokio::time::timeout(Duration::from_millis(20), self.dom.wait_for_work()).await;
            self.dom.render_immediate(&mut NoOpMutations);
        }
    }

    async fn until(&mut self, what: &str, cond: impl Fn(App) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            self.dom.render_immediate(&mut NoOpMutations);
            let app = self.app;
            if self.dom.in_runtime(|| cond(app)) {
                return;
            }
            assert!(Instant::now() < deadline, "timed out waiting for: {what}");
            let _ = tokio::time::timeout(Duration::from_millis(20), self.dom.wait_for_work()).await;
        }
    }
}

/// P6.2: "Opening a project does not change anything on disk until the user
/// edits it." The project holds only a generator, so the image view shows
/// size-less content and, once it knows its size, runs the same call as its
/// effect at image_view.rs:70-76: `fit_canvas_to_window(false)`.
#[tokio::test(flavor = "current_thread")]
async fn opening_a_generator_project_writes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("clouds");
    std::fs::create_dir_all(dir.join("inputs")).unwrap();
    let text = "(output (noise) :canvas_w 1920 :canvas_h 1080)\n";
    std::fs::write(dir.join(PROJECT_FILE), text).unwrap();

    let mut d = Delos::start();
    d.run(|app, worker| session::open_project(app, worker, dir.clone()));
    d.until("the project to open", |a| a.session.peek().project().is_some() && *a.save_state.peek() == SaveState::Saved).await;

    // The image view mounts in a 1000×800 pane and fits the size-less canvas.
    d.run(|app, _| {
        app.viewport.clone().set((1000.0, 800.0));
        app.fit_canvas_to_window(false);
    });
    d.pump_for(Duration::from_millis(1500)).await;

    assert_eq!(std::fs::read_to_string(dir.join(PROJECT_FILE)).unwrap(), text, "opening rewrote project.delos without an edit");
}
