//! End-to-end tests of projects and quick mode: the application's real
//! services (state, background worker, message pump, autosave) run in a
//! headless Dioxus runtime, and each test performs what the UI's buttons do
//! and waits for the background work, checking state and files on disk.

use delos::expr;
use delos::nodes::IMAGE_PARAM;
use delos::project::{ARCHIVE_EXT, PROJECT_FILE};
use delos::ui::session::{self, ImportSource, SaveState, Session};
use delos::ui::state::{App, ImportTarget};
use delos::ui::{use_services, WorkerHandle};
use dioxus::dioxus_core::NoOpMutations;
use dioxus::prelude::*;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

thread_local! {
    static SERVICES: RefCell<Option<(App, WorkerHandle)>> = const { RefCell::new(None) };
}

#[allow(non_snake_case)]
fn Harness() -> Element {
    let services = use_services();
    use_hook(|| SERVICES.with(|s| *s.borrow_mut() = Some(services.clone())));
    rsx! {}
}

/// A running application without a window.
struct Delos {
    dom: VirtualDom,
    app: App,
    worker: WorkerHandle,
    _root: tempfile::TempDir,
    root: PathBuf,
}

impl Delos {
    fn start() -> Delos {
        let mut dom = VirtualDom::new(Harness);
        dom.rebuild_in_place();
        let (app, worker) = SERVICES.with(|s| s.borrow_mut().take()).expect("services");
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("projects");
        let mut d = Delos { dom, app, worker, _root: tmp, root: root.clone() };
        d.run(|app, _| app.projects_root.clone().set(root));
        d
    }

    /// Runs `f` inside the app (as an event handler would).
    fn run<T>(&mut self, f: impl FnOnce(App, WorkerHandle) -> T) -> T {
        let (app, worker) = (self.app, self.worker.clone());
        let out = self.dom.in_scope(ScopeId::ROOT, || f(app, worker));
        self.dom.render_immediate(&mut NoOpMutations);
        out
    }

    fn read<T>(&self, f: impl FnOnce(App) -> T) -> T {
        let app = self.app;
        self.dom.in_runtime(|| f(app))
    }

    /// Processes background results until `cond` holds.
    async fn until(&mut self, what: &str, cond: impl Fn(App) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            self.dom.render_immediate(&mut NoOpMutations);
            if self.read(&cond) {
                return;
            }
            if Instant::now() > deadline {
                let toast = self.read(|a| a.toast.peek().clone());
                panic!("timed out waiting for: {what} (last toast: {toast:?})");
            }
            let _ = tokio::time::timeout(Duration::from_millis(20), self.dom.wait_for_work()).await;
        }
    }

    async fn idle(&mut self) {
        self.until("imports to finish", |a| *a.importing.peek() == 0 && a.project_busy.peek().is_none()).await;
    }

    async fn saved(&mut self) {
        self.until("the project to be saved", |a| *a.save_state.peek() == SaveState::Saved).await;
    }

    fn session(&self) -> Session {
        self.read(|a| a.session.peek().clone())
    }

    fn project_dir(&self) -> PathBuf {
        self.session().project().expect("a project is open").dir.clone()
    }

    fn import(&mut self, files: Vec<ImportSource>) {
        self.run(|app, worker| session::import(app, worker, files, ImportTarget::Output));
    }

    fn graph_text(&self) -> String {
        self.read(|a| expr::to_text(&a.graph.peek()).unwrap())
    }

    fn image_names(&self) -> Vec<String> {
        self.read(|a| {
            let g = a.graph.peek();
            let mut v: Vec<String> = g.nodes.values().filter(|n| n.kind == "image").map(|n| n.title()).collect();
            v.sort();
            v
        })
    }
}

fn write_png(dir: &Path, name: &str, seed: u8) -> PathBuf {
    let img = image::RgbaImage::from_fn(24, 16, |x, y| image::Rgba([x as u8 * 9 ^ seed, y as u8 * 13, seed, 255]));
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    img.save(&path).unwrap();
    path
}

fn files_in(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir).map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    v.sort();
    v
}

/// `project.delos`, with line breaks and indentation collapsed.
fn saved_text(dir: &Path) -> String {
    std::fs::read_to_string(dir.join(PROJECT_FILE)).unwrap().split_whitespace().collect::<Vec<_>>().join(" ")
}

#[tokio::test(flavor = "current_thread")]
async fn first_import_asks_and_creates_a_project() {
    let mut d = Delos::start();
    let src = write_png(&d.root.parent().unwrap().join("photos"), "beach.png", 1);
    assert_eq!(d.session(), Session::Fresh);

    d.import(vec![ImportSource::Path(src.clone())]);
    // Nothing is imported until the user answers.
    let prompt = d.read(|a| a.project_prompt.peek().clone()).expect("asks whether to start a project");
    assert_eq!(prompt.name, "beach");
    assert_eq!(prompt.parent, d.root);
    assert_eq!(d.image_names(), Vec::<String>::new());

    d.run(|app, _| session::set_prompt_name(app, "Holiday"));
    d.run(session::create_project);
    d.idle().await;
    d.saved().await;

    let dir = d.project_dir();
    assert_eq!(dir, d.root.join("Holiday"));
    assert!(d.read(|a| a.project_prompt.peek().is_none()));
    assert_eq!(d.image_names(), vec!["beach.png"]);
    // The image was copied into the project unchanged and is what the file refers to.
    assert_eq!(files_in(&dir.join("inputs")), vec!["beach.png"]);
    assert_eq!(std::fs::read(dir.join("inputs/beach.png")).unwrap(), std::fs::read(&src).unwrap());
    assert!(saved_text(&dir).contains("(output (image \"inputs/beach.png\""), "{}", saved_text(&dir));

    // Later imports go straight into the project.
    let src2 = write_png(&d.root.parent().unwrap().join("photos"), "dunes.png", 2);
    d.import(vec![ImportSource::Path(src2)]);
    assert!(d.read(|a| a.project_prompt.peek().is_none()), "no second prompt");
    d.idle().await;
    d.saved().await;
    assert_eq!(files_in(&dir.join("inputs")), vec!["beach.png", "dunes.png"]);
    assert!(saved_text(&dir).contains("inputs/dunes.png"));
}

#[tokio::test(flavor = "current_thread")]
async fn edits_are_autosaved_and_reopen_identically() {
    let mut d = Delos::start();
    let src = write_png(&d.root.parent().unwrap().join("in"), "cat.png", 3);
    d.import(vec![ImportSource::Path(src)]);
    d.run(session::create_project);
    d.idle().await;
    d.saved().await;
    let dir = d.project_dir();

    // Select the image and add two nodes after it; each edit is saved.
    d.run(|app, _| {
        let img = app.graph.peek().nodes.values().find(|n| n.kind == "image").unwrap().id;
        app.select(Some(img));
        app.add_node("invert");
    });
    d.until("invert to be saved", |_| saved_text(&dir).contains("(invert")).await;
    d.run(|app, _| app.add_node("noise"));
    d.until("noise to be saved", |_| saved_text(&dir).contains("(noise")).await;
    d.saved().await;
    let before = d.graph_text();
    assert_eq!(saved_text(&dir), before.split_whitespace().collect::<Vec<_>>().join(" "));

    // Undo is saved too.
    d.run(|app, _| app.undo());
    d.until("undo to be saved", |_| !saved_text(&dir).contains("(noise")).await;
    d.run(|app, _| app.redo());
    d.until("redo to be saved", |_| saved_text(&dir).contains("(noise")).await;

    // Close and reopen.
    d.run(|app, _| session::close_session(app));
    assert_eq!(d.session(), Session::Fresh);
    assert_eq!(d.read(|a| a.graph.peek().nodes.len()), 1);
    d.run(|app, worker| session::open_project(app, worker, dir.clone()));
    d.idle().await;
    assert_eq!(d.project_dir(), dir);
    assert_eq!(d.graph_text(), before, "the reopened graph is the same");
    assert!(d.read(|a| !a.graph.peek().images.is_empty()));
    assert_eq!(d.read(|a| a.save_state.peek().clone()), SaveState::Saved);
    assert!(!d.read(|a| a.can_undo()), "history starts fresh");
}

#[tokio::test(flavor = "current_thread")]
async fn quick_mode_imports_without_copying() {
    let mut d = Delos::start();
    let src = write_png(&d.root.parent().unwrap().join("in"), "quick.png", 4);
    d.import(vec![ImportSource::Path(src.clone())]);
    d.run(session::choose_quick);
    d.idle().await;
    assert_eq!(d.session(), Session::Quick);
    assert_eq!(d.image_names(), vec!["quick.png"]);
    assert!(!d.root.exists(), "nothing is written in quick mode");

    // Later imports do not ask again.
    d.import(vec![ImportSource::Bytes { name: "dropped.png".into(), bytes: std::fs::read(&src).unwrap() }]);
    assert!(d.read(|a| a.project_prompt.peek().is_none()));
    d.idle().await;
    assert_eq!(d.image_names(), vec!["dropped.png", "quick.png"]);
    assert!(!d.root.exists());
    assert!(session::has_unsaved_work(&d.app));
}

#[tokio::test(flavor = "current_thread")]
async fn quick_session_becomes_a_project_with_the_images_it_uses() {
    let mut d = Delos::start();
    let dir_in = d.root.parent().unwrap().join("in");
    let a = write_png(&dir_in, "a.jpg.png", 5);
    let b = write_png(&dir_in, "b.png", 6);
    d.import(vec![ImportSource::Path(a.clone())]);
    d.run(session::choose_quick);
    d.idle().await;
    d.import(vec![ImportSource::Path(b.clone())]);
    d.idle().await;
    // An image without a source file (e.g. dropped in a browser).
    d.import(vec![ImportSource::Bytes { name: "pasted.png".into(), bytes: std::fs::read(&a).unwrap() }]);
    d.idle().await;
    // Delete b: it only lives in the undo history now.
    d.run(|app, _| {
        let id = app.graph.peek().nodes.values().find(|n| n.title() == "b.png").unwrap().id;
        app.delete_node(id);
    });
    assert_eq!(d.image_names(), vec!["a.jpg.png", "pasted.png"]);

    d.run(|app, _| session::prompt_new_project(app));
    let prompt = d.read(|a| a.project_prompt.peek().clone()).unwrap();
    assert!(prompt.pending.is_empty());
    d.run(|app, _| session::set_prompt_name(app, "From quick"));
    d.run(session::create_project);
    d.idle().await;
    d.saved().await;
    let dir = d.project_dir();
    // Only the images the graph uses are copied; the undo history is not
    // part of the project.
    assert_eq!(files_in(&dir.join("inputs")), vec!["a.jpg.png", "pasted.png"]);
    assert_eq!(std::fs::read(dir.join("inputs/a.jpg.png")).unwrap(), std::fs::read(&a).unwrap());

    // Undoing the deletion brings b back into the project, unchanged.
    d.run(|app, _| app.undo());
    d.until("b to be copied in", |_| dir.join("inputs/b.png").is_file()).await;
    d.saved().await;
    assert!(saved_text(&dir).contains("inputs/b.png"));
    assert_eq!(std::fs::read(dir.join("inputs/b.png")).unwrap(), std::fs::read(&b).unwrap());
}

#[tokio::test(flavor = "current_thread")]
async fn removed_images_leave_the_project_and_undo_restores_them() {
    let mut d = Delos::start();
    let dir_in = d.root.parent().unwrap().join("in");
    let keep = write_png(&dir_in, "keep.png", 9);
    let gone = write_png(&dir_in, "gone.png", 10);
    d.import(vec![ImportSource::Path(keep)]);
    d.run(session::create_project);
    d.idle().await;
    d.saved().await;
    let dir = d.project_dir();
    // Dropped bytes (no source file) are copied byte for byte, not re-encoded.
    let gone_bytes = std::fs::read(&gone).unwrap();
    std::fs::remove_file(&gone).unwrap();
    d.import(vec![ImportSource::Bytes { name: "gone.png".into(), bytes: gone_bytes.clone() }]);
    d.idle().await;
    d.saved().await;
    assert_eq!(files_in(&dir.join("inputs")), vec!["gone.png", "keep.png"]);
    assert_eq!(std::fs::read(dir.join("inputs/gone.png")).unwrap(), gone_bytes);

    // Deleting the node removes the file from the project.
    d.run(|app, _| {
        let id = app.graph.peek().nodes.values().find(|n| n.title() == "gone.png").unwrap().id;
        app.delete_node(id);
    });
    d.until("gone.png to leave inputs/", |_| !dir.join("inputs/gone.png").exists()).await;
    d.saved().await;
    assert_eq!(files_in(&dir.join("inputs")), vec!["keep.png"]);
    assert!(!saved_text(&dir).contains("gone.png"));

    // Undo brings the identical file back (its original source is deleted).
    d.run(|app, _| app.undo());
    d.until("gone.png to come back", |_| dir.join("inputs/gone.png").is_file()).await;
    d.saved().await;
    assert_eq!(std::fs::read(dir.join("inputs/gone.png")).unwrap(), gone_bytes);

    // Sharing after a removal packs only the images in use.
    d.run(|app, _| app.redo());
    d.until("gone.png to leave again", |_| !dir.join("inputs/gone.png").exists()).await;
    let archive = d.root.parent().unwrap().join(format!("x.{ARCHIVE_EXT}"));
    d.run(|app, worker| session::share_project(app, worker, archive.clone()));
    d.idle().await;
    let zip = zip::ZipArchive::new(std::fs::File::open(&archive).unwrap()).unwrap();
    let names: Vec<String> = zip.file_names().map(|n| n.unwrap().into_owned()).collect();
    assert!(names.iter().any(|n| n.ends_with("inputs/keep.png")));
    assert!(!names.iter().any(|n| n.contains("gone.png")), "{names:?}");
}

#[tokio::test(flavor = "current_thread")]
async fn opening_a_project_does_not_rewrite_it() {
    let mut d = Delos::start();
    let dir = d.root.join("handwritten");
    std::fs::create_dir_all(dir.join("inputs")).unwrap();
    write_png(&dir.join("inputs"), "pic.png", 11);
    std::fs::write(dir.join("inputs/extra.txt"), "notes").unwrap();
    let text = "; written by hand\n(output (invert (image \"inputs/pic.png\")))\n";
    std::fs::write(dir.join(PROJECT_FILE), text).unwrap();
    d.run(|app, worker| session::open_project(app, worker, dir.clone()));
    d.idle().await;
    assert_eq!(d.project_dir(), dir);
    for _ in 0..10 {
        let _ = tokio::time::timeout(Duration::from_millis(20), d.dom.wait_for_work()).await;
        d.dom.render_immediate(&mut NoOpMutations);
    }
    assert_eq!(std::fs::read_to_string(dir.join(PROJECT_FILE)).unwrap(), text, "untouched until edited");
    assert!(dir.join("inputs/extra.txt").exists());
    // The first edit saves in canonical form, with every parameter.
    d.run(|app, _| app.add_node("noise"));
    d.until("the edit to be saved", |_| saved_text(&dir).contains("(noise")).await;
    assert!(saved_text(&dir).contains(":octaves 6"));
}

#[tokio::test(flavor = "current_thread")]
async fn share_and_import_a_project() {
    let mut d = Delos::start();
    let src = write_png(&d.root.parent().unwrap().join("in"), "shared.png", 7);
    d.import(vec![ImportSource::Path(src)]);
    d.run(|app, _| session::set_prompt_name(app, "Team"));
    d.run(session::create_project);
    d.idle().await;
    d.saved().await;
    let original = d.graph_text();
    assert_eq!(session::share_file_name(&d.app), format!("Team.{ARCHIVE_EXT}"));

    let archive = d.root.parent().unwrap().join(format!("Team.{ARCHIVE_EXT}"));
    d.run(|app, worker| session::share_project(app, worker, archive.clone()));
    d.idle().await;
    assert!(archive.is_file());
    assert!(d.read(|a| a.toast.peek().clone()).unwrap().0.contains("Project shared"));

    // A colleague imports it: it is unpacked next to the original.
    let mut other = Delos::start();
    other.run(|app, worker| session::open_project(app, worker, archive.clone()));
    other.idle().await;
    let dir = other.project_dir();
    assert_eq!(dir, other.root.join("Team"));
    assert_eq!(other.graph_text(), original);
    assert_eq!(files_in(&dir.join("inputs")), vec!["shared.png"]);

    // Their edits save into their copy.
    other.run(|app, _| app.add_node("checker"));
    other.until("their edit to be saved", |_| saved_text(&dir).contains("(checker")).await;
    assert!(!saved_text(&d.project_dir()).contains("(checker"));
}

#[tokio::test(flavor = "current_thread")]
async fn invalid_projects_are_rejected_with_a_message() {
    let mut d = Delos::start();
    let bogus = d.root.parent().unwrap().join("not-a-project");
    std::fs::create_dir_all(&bogus).unwrap();
    d.run(|app, worker| session::open_project(app, worker, bogus.clone()));
    d.idle().await;
    let (msg, is_error) = d.read(|a| a.toast.peek().clone()).unwrap();
    assert!(is_error && msg.contains("project.delos is missing"), "{msg}");
    assert_eq!(d.session(), Session::Fresh);

    // A project whose file references a missing image.
    std::fs::create_dir_all(bogus.join("inputs")).unwrap();
    std::fs::write(bogus.join(PROJECT_FILE), "(output (image \"inputs/lost.png\"))").unwrap();
    d.run(|app, worker| session::open_project(app, worker, bogus.clone()));
    d.idle().await;
    assert!(d.read(|a| a.toast.peek().clone()).unwrap().0.contains("“inputs/lost.png” is missing"));
    assert_eq!(d.session(), Session::Fresh);
}

#[tokio::test(flavor = "current_thread")]
async fn project_name_collisions_and_save_failures_are_reported() {
    let mut d = Delos::start();
    std::fs::create_dir_all(d.root.join("Taken")).unwrap();
    std::fs::write(d.root.join("Taken/file.txt"), "x").unwrap();
    d.run(|app, _| session::prompt_new_project(app));
    d.run(|app, _| session::set_prompt_name(app, "Taken"));
    d.run(session::create_project);
    let err = d.read(|a| a.project_prompt.peek().clone()).unwrap().error.unwrap();
    assert!(err.contains("already exists"), "{err}");
    assert_eq!(d.session(), Session::Fresh);

    d.run(|app, _| session::set_prompt_name(app, "Fine"));
    d.run(session::create_project);
    d.idle().await;
    d.saved().await;
    let dir = d.project_dir();
    // The project folder disappears: the next save fails visibly.
    std::fs::remove_dir_all(&dir).unwrap();
    d.run(|app, _| app.add_node("noise"));
    d.until("the save to fail", |a| matches!(&*a.save_state.peek(), SaveState::Failed(_))).await;
}

#[tokio::test(flavor = "current_thread")]
async fn cancelling_the_prompt_imports_nothing() {
    let mut d = Delos::start();
    let src = write_png(&d.root.parent().unwrap().join("in"), "x.png", 8);
    d.import(vec![ImportSource::Path(src)]);
    d.run(|app, _| session::cancel_prompt(app));
    d.idle().await;
    assert_eq!(d.session(), Session::Fresh);
    assert!(d.image_names().is_empty());
    // Image nodes keep pointing at their decoded image.
    assert!(d.read(|a| a.graph.peek().nodes.values().all(|n| n.kind != "image" || n.params.i(IMAGE_PARAM) > 0)));
}
