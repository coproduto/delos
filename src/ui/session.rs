//! Sessions: quick mode (one-off edits, nothing saved) and projects (a
//! folder with the graph and copies of its images, saved automatically).
//!
//! Every action behind the Project menu, the "start a project?" prompt and
//! autosave lives here as a plain function of [`App`], so the UI and the
//! end-to-end tests drive exactly the same code.

use super::state::{App, ImportTarget};
use super::WorkerHandle;
use crate::worker::ImportOptions;
use dioxus::prelude::*;
use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Session {
    /// Nothing imported yet: the next import asks whether to start a project.
    #[default]
    Fresh,
    /// One-off editing; imported images are used in place, nothing is saved.
    Quick,
    /// Work is saved to a project folder after every change.
    #[cfg(feature = "native")]
    Project(crate::project::Project),
}

impl Session {
    #[cfg(feature = "native")]
    pub fn project(&self) -> Option<&crate::project::Project> {
        match self {
            Session::Project(p) => Some(p),
            _ => None,
        }
    }
    #[cfg(not(feature = "native"))]
    pub fn project(&self) -> Option<&std::convert::Infallible> {
        None
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum SaveState {
    #[default]
    Idle,
    Saving,
    Saved,
    Failed(String),
}

/// An image to import.
#[derive(Clone)]
pub enum ImportSource {
    /// From a file input or a drop.
    File(dioxus::html::FileData),
    Bytes { name: String, bytes: Vec<u8> },
    /// A file on disk, read in the background.
    Path(PathBuf),
}

impl ImportSource {
    pub fn name(&self) -> String {
        match self {
            ImportSource::File(f) => f.name(),
            ImportSource::Bytes { name, .. } => name.clone(),
            ImportSource::Path(p) => p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        }
    }
}

/// The "start a project?" / "new project" dialog.
#[derive(Clone)]
pub struct ProjectPrompt {
    /// Imports waiting for the answer (empty for Project ▸ New project).
    pub pending: Vec<(ImportSource, ImportTarget)>,
    pub name: String,
    /// Folder the project directory is created in.
    pub parent: PathBuf,
    pub error: Option<String>,
}

impl PartialEq for ProjectPrompt {
    fn eq(&self, o: &Self) -> bool {
        self.pending.len() == o.pending.len() && self.name == o.name && self.parent == o.parent && self.error == o.error
    }
}

/// Projects need a file system (desktop and mobile).
pub const PROJECTS_SUPPORTED: bool = cfg!(feature = "native");

pub fn default_projects_root() -> PathBuf {
    #[cfg(feature = "native")]
    {
        crate::project::default_root()
    }
    #[cfg(not(feature = "native"))]
    PathBuf::new()
}

/// Entry point of every image import (toolbar, drop, layer stack). In a
/// fresh session it first asks whether to start a project.
pub fn import(mut app: App, worker: WorkerHandle, files: Vec<ImportSource>, target: ImportTarget) {
    if files.is_empty() {
        return;
    }
    let session = app.session.peek().clone();
    if session == Session::Fresh {
        if PROJECTS_SUPPORTED {
            let name = files[0].name();
            let stem = name.rsplit_once('.').map_or(name.as_str(), |(s, _)| s).to_string();
            app.project_prompt.set(Some(ProjectPrompt {
                pending: files.into_iter().map(|f| (f, target)).collect(),
                name: stem,
                parent: app.projects_root.peek().clone(),
                error: None,
            }));
            return;
        }
        app.session.set(Session::Quick);
    }
    start_imports(app, worker, files, target);
}

/// Prompt answer: continue without a project.
pub fn choose_quick(mut app: App, worker: WorkerHandle) {
    let Some(prompt) = app.project_prompt.take() else { return };
    if *app.session.peek() == Session::Fresh {
        app.session.set(Session::Quick);
    }
    for (file, target) in prompt.pending {
        start_imports(app, worker.clone(), vec![file], target);
    }
}

/// Project ▸ New project: creates a project from the current work.
pub fn prompt_new_project(mut app: App) {
    let name = app.graph.peek().images.values().next().map(|i| i.name.rsplit_once('.').map_or(i.name.clone(), |(s, _)| s.to_string()));
    app.project_prompt.set(Some(ProjectPrompt { pending: Vec::new(), name: name.unwrap_or_else(|| "Untitled".into()), parent: app.projects_root.peek().clone(), error: None }));
}

/// The prompt's name field.
pub fn set_prompt_name(mut app: App, name: &str) {
    if let Some(p) = app.project_prompt.write().as_mut() {
        p.name = name.to_string();
        p.error = None;
    }
}

pub fn cancel_prompt(mut app: App) {
    app.project_prompt.set(None);
}

/// Prompt answer: create the project (in the background), then run the
/// pending imports. The first save copies in the images the graph uses.
#[cfg(feature = "native")]
pub fn create_project(mut app: App, worker: WorkerHandle) {
    let Some(prompt) = app.project_prompt.peek().clone() else { return };
    let dir = prompt.parent.join(crate::project::sanitize_name(&prompt.name));
    if dir.exists() && std::fs::read_dir(&dir).map(|mut d| d.next().is_some()).unwrap_or(true) {
        if let Some(p) = app.project_prompt.write().as_mut() {
            p.error = Some(format!("{} already exists. Choose another name.", dir.display()));
        }
        return;
    }
    app.project_busy.set(Some(format!("Creating project “{}”…", prompt.name.trim())));
    worker.0.create_project(dir);
}

#[cfg(feature = "native")]
pub fn on_project_created(mut app: App, worker: WorkerHandle, result: Result<crate::project::Project, String>) {
    app.project_busy.set(None);
    match result {
        Ok(project) => {
            let pending = app.project_prompt.take().map(|p| p.pending).unwrap_or_default();
            let name = project.name();
            app.session.set(Session::Project(project));
            app.save_state.set(SaveState::Idle);
            app.toast(format!("Created project “{name}”"), false);
            for (file, target) in pending {
                start_imports(app, worker.clone(), vec![file], target);
            }
        }
        Err(e) => {
            if let Some(p) = app.project_prompt.write().as_mut() {
                p.error = Some(e.clone());
            }
            if app.project_prompt.peek().is_none() {
                app.toast(format!("Could not create the project: {e}"), true);
            }
        }
    }
}

/// Opens a project folder, `project.delos` file or `.delosproject` archive.
#[cfg(feature = "native")]
pub fn open_project(mut app: App, worker: WorkerHandle, path: PathBuf) {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    app.project_busy.set(Some(format!("Opening “{name}”…")));
    let root = app.projects_root.peek().clone();
    worker.0.open_project(path, root);
}

#[cfg(feature = "native")]
pub fn on_project_opened(mut app: App, result: Result<(crate::project::Project, crate::graph::Graph), String>) {
    app.project_busy.set(None);
    match result {
        Ok((project, graph)) => {
            let name = project.name();
            app.replace_document(graph);
            app.project_prompt.set(None);
            app.session.set(Session::Project(project));
            // Freshly loaded: nothing to write until something changes.
            app.save_state.set(SaveState::Saved);
            app.toast(format!("Opened project “{name}”"), false);
        }
        Err(e) => app.toast(format!("Could not open the project: {e}"), true),
    }
}

/// Leaves the current project or quick session and starts over.
pub fn close_session(mut app: App) {
    app.replace_document(crate::graph::Graph::new());
    app.session.set(Session::Fresh);
    app.save_state.set(SaveState::Idle);
}

#[cfg(feature = "native")]
pub fn share_project(mut app: App, worker: WorkerHandle, dest: PathBuf) {
    let Some(project) = app.session.peek().project().cloned() else { return };
    app.project_busy.set(Some("Packing project…".into()));
    worker.0.share_project(project, dest);
}

/// Default file name for Share project.
pub fn share_file_name(app: &App) -> String {
    #[cfg(feature = "native")]
    if let Some(p) = app.session.peek().project() {
        return format!("{}.{}", p.name(), crate::project::ARCHIVE_EXT);
    }
    let _ = app;
    String::new()
}

/// Whether leaving the session would lose work that is not in a project.
pub fn has_unsaved_work(app: &App) -> bool {
    *app.session.peek() != Session::Fresh && app.session.peek().project().is_none() && app.graph.peek().nodes.len() > 1
}

/// Saves the project (in the background) after every change to the graph.
pub fn use_autosave(app: App, worker: WorkerHandle) {
    let seq = use_hook(|| std::rc::Rc::new(std::cell::Cell::new(0u64)));
    let last_dir = use_hook(|| std::rc::Rc::new(std::cell::RefCell::new(None::<PathBuf>)));
    use_effect(move || {
        let session = app.session.read().clone();
        let graph = app.graph.read().clone();
        #[cfg(feature = "native")]
        if let Session::Project(project) = session {
            let mut app = app;
            // A project that was just opened is already saved: leave its
            // files alone until something changes.
            let switched = last_dir.replace(Some(project.dir.clone())).as_ref() != Some(&project.dir);
            if switched && *app.save_state.peek() == SaveState::Saved {
                return;
            }
            seq.set(seq.get() + 1);
            app.save_state.set(SaveState::Saving);
            worker.0.save_project(project, graph, app.all_images(), seq.get());
        }
        #[cfg(not(feature = "native"))]
        let _ = (session, graph, &worker, &seq, &last_dir);
    });
}

static IMPORT_TOKEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Reads and decodes images in the background (copying them into the
/// project first when one is open); results arrive as `Msg::Imported`.
pub fn start_imports(mut app: App, worker: WorkerHandle, files: Vec<ImportSource>, target: ImportTarget) {
    let keep_bytes = app.session.peek().project().is_some();
    for f in files {
        let worker = worker.clone();
        *app.importing.write() += 1;
        let token = IMPORT_TOKEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        app.imports.write().insert(token, target);
        let name = f.name();
        let mut opts = ImportOptions { origin: None, keep_bytes };
        match f {
            ImportSource::Bytes { bytes, .. } => worker.0.import(name, Some(bytes), token, opts),
            ImportSource::Path(path) => {
                opts.origin = Some(path);
                worker.0.import(name, None, token, opts);
            }
            ImportSource::File(file) => {
                spawn(async move {
                    #[cfg(feature = "desktop")]
                    {
                        let path = file.path();
                        if path.is_file() {
                            opts.origin = Some(path);
                        }
                    }
                    match file.read_bytes().await {
                        Ok(bytes) => worker.0.import(name, Some(bytes.to_vec()), token, opts),
                        Err(e) => {
                            *app.importing.write() -= 1;
                            app.imports.write().remove(&token);
                            app.toast(format!("Could not read {name}: {e}"), true);
                        }
                    }
                });
            }
        }
    }
}
