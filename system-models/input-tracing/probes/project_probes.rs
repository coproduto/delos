//! Probe for finding 3 (see ../findings.md), using the same headless
//! services as `tests/project_e2e.rs`: the real state, worker, message pump
//! and autosave effect.
//!
//! The image view cannot be mounted from a test (`ui::image_view` is
//! private), so the probe replays the body of its canvas effect
//! (src/ui/image_view.rs:70-76): when the shown node is size-less and the
//! viewport is known, it calls `App::fit_canvas_to_window(false)`.
//!
//! FAILS on commit 197e30c. To run it:
//!
//! ```sh
//! cp system-models/input-tracing/probes/project_probes.rs tests/
//! cargo test --test project_probes
//! rm tests/project_probes.rs
//! ```

use delos::project::PROJECT_FILE;
use delos::ui::session;
use delos::ui::state::App;
use delos::ui::{use_services, WorkerHandle};
use dioxus::dioxus_core::NoOpMutations;
use dioxus::prelude::*;
use std::cell::RefCell;
use std::time::Duration;

thread_local! {
    static SERVICES: RefCell<Option<(App, WorkerHandle)>> = const { RefCell::new(None) };
}

#[allow(non_snake_case)]
fn Harness() -> Element {
    let services = use_services();
    use_hook(|| SERVICES.with(|s| *s.borrow_mut() = Some(services.clone())));
    rsx! {}
}

async fn settle(dom: &mut VirtualDom) {
    for _ in 0..25 {
        let _ = tokio::time::timeout(Duration::from_millis(20), dom.wait_for_work()).await;
        dom.render_immediate(&mut NoOpMutations);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn opening_a_project_with_a_sizeless_output_does_not_rewrite_it() {
    let mut dom = VirtualDom::new(Harness);
    dom.rebuild_in_place();
    let (app, worker) = SERVICES.with(|s| s.borrow_mut().take()).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("poster");
    std::fs::create_dir_all(dir.join("inputs")).unwrap();
    // A generator straight into the output, on a canvas the user chose.
    let text = "(output (noise) :canvas_w 1920 :canvas_h 1080)\n";
    std::fs::write(dir.join(PROJECT_FILE), text).unwrap();

    dom.in_scope(ScopeId::ROOT, || session::open_project(app, worker.clone(), dir.clone()));
    for _ in 0..200 {
        dom.render_immediate(&mut NoOpMutations);
        if dom.in_runtime(|| app.session.peek().project().is_some()) {
            break;
        }
        let _ = tokio::time::timeout(Duration::from_millis(20), dom.wait_for_work()).await;
    }
    assert!(dom.in_runtime(|| app.session.peek().project().is_some()), "project did not open");
    settle(&mut dom).await;
    assert_eq!(std::fs::read_to_string(dir.join(PROJECT_FILE)).unwrap(), text, "setup: untouched after opening");

    // The window lays out: the image view reports its size (onresize,
    // src/ui/image_view.rs:201-204), and its canvas effect runs because the
    // shown node is size-less.
    dom.in_scope(ScopeId::ROOT, || {
        let mut viewport = app.viewport;
        viewport.set((1000.0, 700.0));
        assert!(app.displayed().sizeless, "setup: the canvas effect's condition holds");
        app.fit_canvas_to_window(false);
    });
    dom.render_immediate(&mut NoOpMutations);
    settle(&mut dom).await;

    let on_disk = std::fs::read_to_string(dir.join(PROJECT_FILE)).unwrap();
    let canvas = dom.in_runtime(|| app.graph.peek().canvas());
    let undoable = dom.in_runtime(|| app.can_undo());
    eprintln!("canvas now {}x{}, undo available: {undoable}\nproject.delos now:\n{on_disk}", canvas.w, canvas.h);
    assert_eq!(on_disk, text, "project.delos was rewritten without an edit");
    assert_eq!((canvas.w, canvas.h), (1920.0, 1080.0), "the saved canvas size was replaced by the window size");
}
