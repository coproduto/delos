//! Delos projects. A project is a directory holding the graph as a Delos
//! expression and a copy of every image it uses:
//!
//! ```text
//! my-project/
//!   project.delos     the node graph (see `expr`)
//!   inputs/           imported images, referenced as "inputs/<file>"
//! ```
//!
//! `inputs/` holds exactly the images the graph uses: every save copies in
//! images that are new to it and moves out those no longer used. Undo
//! history is not part of a project; images it still needs are kept in a
//! per-session stash outside the project and copied back if an undo
//! restores them.
//!
//! Projects are shared as `.delosproject` files: zip archives containing the
//! project directory.

use crate::engine::source::SourceImage;
use crate::expr::{self, Sexp};
use crate::formats;
use crate::graph::Graph;
use anyhow::{anyhow, bail, Context, Result};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

pub const PROJECT_FILE: &str = "project.delos";
pub const INPUTS_DIR: &str = "inputs";
pub const ARCHIVE_EXT: &str = "delosproject";

#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    pub dir: PathBuf,
}

/// Where new projects go: `$DELOS_PROJECTS_DIR`, else `~/.delos/projects`.
pub fn default_root() -> PathBuf {
    if let Some(dir) = std::env::var_os("DELOS_PROJECTS_DIR") {
        return PathBuf::from(dir);
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    home.join(".delos").join("projects")
}

/// A file-system friendly project name.
pub fn sanitize_name(name: &str) -> String {
    let s: String = name.trim().chars().map(|c| if c.is_control() || "/\\:*?\"<>|".contains(c) { '-' } else { c }).collect();
    let s = s.trim_matches(|c: char| c == '.' || c.is_whitespace()).to_string();
    if s.is_empty() { "Untitled".into() } else { s }
}

/// `parent/name`, or `parent/name-2`, `-3`… if that already exists.
pub fn unique_dir(parent: &Path, name: &str) -> PathBuf {
    let name = sanitize_name(name);
    let mut dir = parent.join(&name);
    let mut k = 2;
    while dir.exists() {
        dir = parent.join(format!("{name}-{k}"));
        k += 1;
    }
    dir
}

impl Project {
    /// Creates a new project directory (which must not exist or be empty)
    /// holding `graph`. Images in the graph must already have files.
    pub fn create(dir: &Path) -> Result<Project> {
        if dir.exists() && std::fs::read_dir(dir)?.next().is_some() {
            bail!("{} already exists and is not empty", dir.display());
        }
        std::fs::create_dir_all(dir.join(INPUTS_DIR)).with_context(|| format!("could not create {}", dir.display()))?;
        let p = Project { dir: dir.to_path_buf() };
        p.save(&Graph::new())?;
        Ok(p)
    }

    pub fn name(&self) -> String {
        self.dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Project".into())
    }

    pub fn file(&self) -> PathBuf {
        self.dir.join(PROJECT_FILE)
    }

    /// Copies an image into `inputs/` and returns its project path
    /// (`inputs/<file>`). Re-importing identical bytes under the same name
    /// reuses the existing copy.
    pub fn add_input(&self, name: &str, bytes: &[u8]) -> Result<String> {
        let inputs = self.dir.join(INPUTS_DIR);
        std::fs::create_dir_all(&inputs)?;
        let clean = sanitize_name(Path::new(name).file_name().and_then(|n| n.to_str()).unwrap_or("image"));
        let (stem, ext) = match clean.rsplit_once('.') {
            Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
            _ => (clean.clone(), String::new()),
        };
        let mut file = clean.clone();
        let mut k = 2;
        loop {
            let path = inputs.join(&file);
            if !path.exists() {
                write_atomic(&path, bytes)?;
                break;
            }
            if std::fs::metadata(&path)?.len() == bytes.len() as u64 && std::fs::read(&path)? == bytes {
                break;
            }
            file = format!("{stem}-{k}{ext}");
            k += 1;
        }
        Ok(format!("{INPUTS_DIR}/{file}"))
    }

    /// Copies an image that is already loaded into the project: from the
    /// file it came from when that is known, otherwise re-encoded as PNG.
    pub fn adopt_image(&self, img: &SourceImage) -> Result<()> {
        let origin = img.file().origin;
        let rel = match origin.as_deref().map(std::fs::read) {
            Some(Ok(bytes)) => self.add_input(&img.name, &bytes)?,
            _ => {
                let mut png = std::io::Cursor::new(Vec::new());
                image::RgbaImage::from_raw(img.width, img.height, img.to_rgba8())
                    .ok_or_else(|| anyhow!("invalid image data"))?
                    .write_to(&mut png, image::ImageFormat::Png)?;
                let stem = img.name.rsplit_once('.').map_or(img.name.as_str(), |(s, _)| s);
                self.add_input(&format!("{stem}.png"), png.get_ref())?
            }
        };
        img.set_origin(self.dir.join(&rel));
        img.set_project_file(rel);
        Ok(())
    }

    /// Makes `inputs/` match the graph: images the graph uses but the
    /// project lacks are copied in, and files it no longer uses are moved to
    /// `stash` (a session folder outside the project). `known` lists every
    /// image the session still holds (e.g. in undo history): those whose
    /// file is moved out now point at the stashed copy, so they can be
    /// copied back unchanged.
    pub fn sync_inputs(&self, g: &Graph, known: &[Arc<SourceImage>], stash: &Path) -> Result<()> {
        let used: Vec<&Arc<SourceImage>> = g
            .nodes
            .values()
            .filter(|n| n.kind == crate::nodes::ids::IMAGE)
            .filter_map(|n| g.images.get(&(n.params.i(crate::nodes::IMAGE_PARAM) as u64)))
            .collect();
        for img in &used {
            let present = img.file().project.is_some_and(|rel| self.resolve(&rel).is_ok_and(|p| p.is_file()));
            if !present {
                self.adopt_image(img)?;
            }
        }
        let referenced: std::collections::HashSet<String> = used.iter().filter_map(|i| i.file().project).collect();
        let inputs = self.dir.join(INPUTS_DIR);
        for entry in std::fs::read_dir(&inputs)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let rel = format!("{INPUTS_DIR}/{name}");
            if !entry.file_type()?.is_file() || name.starts_with('.') || name.ends_with(".tmp") || referenced.contains(&rel) {
                continue;
            }
            std::fs::create_dir_all(stash)?;
            let mut k = 0;
            let dest = loop {
                let d = stash.join(if k == 0 { name.clone() } else { format!("{k}-{name}") });
                if !d.exists() {
                    break d;
                }
                k += 1;
            };
            if std::fs::rename(entry.path(), &dest).is_err() {
                std::fs::copy(entry.path(), &dest)?;
                std::fs::remove_file(entry.path())?;
            }
            for img in known.iter().filter(|i| i.file().project.as_deref() == Some(rel.as_str())) {
                img.clear_project_file();
                img.set_origin(dest.clone());
            }
        }
        Ok(())
    }

    /// Writes the graph to `project.delos` (atomically). Its images must
    /// already be in the project (see [`Project::sync_inputs`]).
    pub fn save(&self, g: &Graph) -> Result<()> {
        let text = expr::to_text(g).map_err(|e| anyhow!(e))?;
        self.save_text(&text)
    }

    pub fn save_text(&self, text: &str) -> Result<()> {
        write_atomic(&self.file(), text.as_bytes()).with_context(|| format!("could not save {}", self.file().display()))
    }

    /// Absolute path of a project path, which must stay inside `inputs/`.
    pub fn resolve(&self, rel: &str) -> Result<PathBuf> {
        let p = Path::new(rel);
        let mut parts = p.components();
        let ok = parts.next() == Some(Component::Normal(INPUTS_DIR.as_ref())) && parts.clone().next().is_some() && parts.all(|c| matches!(c, Component::Normal(_)));
        if !ok {
            bail!("“{rel}” is not inside the project's {INPUTS_DIR}/ folder");
        }
        Ok(self.dir.join(p))
    }

    /// Checks the project's structure: a parseable `project.delos`, an
    /// `inputs/` folder, and every referenced image present inside it.
    pub fn verify(&self) -> Result<Vec<Sexp>> {
        let file = self.file();
        if !file.is_file() {
            bail!("{} is not a Delos project: {PROJECT_FILE} is missing", self.dir.display());
        }
        if !self.dir.join(INPUTS_DIR).is_dir() {
            bail!("{} is not a Delos project: the {INPUTS_DIR}/ folder is missing", self.dir.display());
        }
        let text = std::fs::read_to_string(&file).with_context(|| format!("could not read {}", file.display()))?;
        let forms = expr::parse(&text).map_err(|e| anyhow!("{PROJECT_FILE}, {e}"))?;
        for (rel, pos) in expr::image_files(&forms) {
            let path = self.resolve(&rel).map_err(|e| anyhow!("{PROJECT_FILE}, line {}: {e}", pos.line))?;
            if !path.is_file() {
                bail!("{PROJECT_FILE}, line {}: the image “{rel}” is missing", pos.line);
            }
        }
        Ok(forms)
    }

    /// Verifies the project, decodes its images (in parallel) and builds the
    /// graph.
    pub fn load(&self) -> Result<Graph> {
        use rayon::prelude::*;
        let forms = self.verify()?;
        let mut files: Vec<String> = expr::image_files(&forms).into_iter().map(|(f, _)| f).collect();
        files.sort();
        files.dedup();
        let decoded: Vec<(String, Result<Arc<SourceImage>, String>)> = files
            .into_par_iter()
            .map(|rel| {
                let r = (|| -> Result<Arc<SourceImage>> {
                    let bytes = std::fs::read(self.resolve(&rel)?)?;
                    let d = formats::decode(&bytes)?;
                    let name = Path::new(&rel).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    let img = SourceImage::from_rgba8(name, d.width, d.height, d.rgba);
                    img.set_origin(self.resolve(&rel)?);
                    Ok(Arc::new(img))
                })();
                (rel.clone(), r.map_err(|e| format!("could not open “{rel}”: {e:#}")))
            })
            .collect();
        let images: HashMap<String, Result<Arc<SourceImage>, String>> = decoded.into_iter().collect();
        expr::build(&forms, &|rel| images.get(rel).cloned().unwrap_or_else(|| Err(format!("“{rel}” was not loaded")))).map_err(|e| anyhow!("{PROJECT_FILE}, {e}"))
    }

    /// Writes the project as a `.delosproject` archive (a zip of the project
    /// directory).
    pub fn share(&self, dest: &Path) -> Result<()> {
        use zip::write::SimpleFileOptions;
        use zip::CompressionMethod;
        let tmp = tmp_path(dest);
        let result = (|| -> Result<()> {
            let mut zip = zip::ZipWriter::new(std::io::BufWriter::new(std::fs::File::create(&tmp)?));
            let root = sanitize_name(&self.name());
            let mut files = Vec::new();
            collect_files(&self.dir, &mut files)?;
            files.sort();
            zip.add_directory(format!("{root}/"), SimpleFileOptions::default())?;
            zip.add_directory(format!("{root}/{INPUTS_DIR}/"), SimpleFileOptions::default())?;
            for path in files {
                let rel = path.strip_prefix(&self.dir)?.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/");
                // Images are already compressed.
                let method = if rel == PROJECT_FILE { CompressionMethod::Deflated } else { CompressionMethod::Stored };
                zip.start_file(format!("{root}/{rel}"), SimpleFileOptions::default().compression_method(method).large_file(true))?;
                std::io::copy(&mut std::fs::File::open(&path)?, &mut zip)?;
            }
            zip.finish()?.flush()?;
            Ok(())
        })();
        match result {
            Ok(()) => std::fs::rename(&tmp, dest).with_context(|| format!("could not write {}", dest.display())),
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(e.context(format!("could not write {}", dest.display())))
            }
        }
    }
}

/// Opens a project from a project directory, its `project.delos` file, or a
/// `.delosproject` archive (unpacked into a new directory under `root`).
pub fn open(path: &Path, root: &Path) -> Result<(Project, Graph)> {
    let project = if path.extension().is_some_and(|e| e == ARCHIVE_EXT) {
        unpack(path, root)?
    } else if path.file_name().is_some_and(|n| n == PROJECT_FILE) {
        Project { dir: path.parent().unwrap_or(Path::new(".")).to_path_buf() }
    } else if path.is_dir() {
        Project { dir: path.to_path_buf() }
    } else {
        bail!("{} is not a Delos project (expected a folder, a {PROJECT_FILE} file or a .{ARCHIVE_EXT} file)", path.display());
    };
    let graph = project.load()?;
    Ok((project, graph))
}

/// Unpacks a `.delosproject` archive into a new directory under `root` and
/// verifies it. Nothing is left behind if it is not a valid project.
pub fn unpack(archive: &Path, root: &Path) -> Result<Project> {
    let file = std::fs::File::open(archive).with_context(|| format!("could not open {}", archive.display()))?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file)).with_context(|| format!("{} is not a .{ARCHIVE_EXT} file", archive.display()))?;
    // Entries are either inside one top-level folder or at the root.
    let mut names = Vec::new();
    for i in 0..zip.len() {
        let entry = zip.by_index(i)?;
        let Some(name) = entry.enclosed_name() else {
            bail!("{} contains an unsafe path ({})", archive.display(), String::from_utf8_lossy(entry.name_raw()));
        };
        names.push((name, entry.is_dir()));
    }
    let prefix: Option<PathBuf> = if names.iter().any(|(n, _)| n == Path::new(PROJECT_FILE)) {
        None
    } else {
        let top = names.iter().find_map(|(n, _)| n.components().next().map(|c| PathBuf::from(c.as_os_str())));
        match top {
            Some(top) if names.iter().any(|(n, _)| *n == top.join(PROJECT_FILE)) => Some(top),
            _ => bail!("{} is not a Delos project: {PROJECT_FILE} is missing", archive.display()),
        }
    };
    let stem = archive.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let name = prefix.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or(stem);
    std::fs::create_dir_all(root)?;
    let dest = unique_dir(root, &name);
    let result = (|| -> Result<Project> {
        std::fs::create_dir_all(&dest)?;
        for (i, (name, is_dir)) in names.iter().enumerate() {
            let rel = match &prefix {
                Some(p) => match name.strip_prefix(p) {
                    Ok(r) => r.to_path_buf(),
                    Err(_) => continue, // outside the project folder
                },
                None => name.clone(),
            };
            if rel.as_os_str().is_empty() {
                continue;
            }
            let out = dest.join(&rel);
            if *is_dir {
                std::fs::create_dir_all(&out)?;
                continue;
            }
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut entry = zip.by_index(i)?;
            let mut w = std::io::BufWriter::new(std::fs::File::create(&out)?);
            std::io::copy(&mut (&mut entry).take(u64::MAX), &mut w)?;
            w.flush()?;
        }
        let project = Project { dir: dest.clone() };
        project.verify()?;
        Ok(project)
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&dest);
    }
    result
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name.ends_with(".tmp") {
            continue;
        }
        let ty = entry.file_type()?;
        if ty.is_dir() {
            collect_files(&entry.path(), out)?;
        } else if ty.is_file() {
            out.push(entry.path());
        }
    }
    Ok(())
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(format!(".{}.tmp", std::process::id()));
    path.with_file_name(name)
}

/// Writes through a temporary file and renames it into place, so readers
/// never see a partial file.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = tmp_path(path);
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })?;
    Ok(())
}
