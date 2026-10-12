//! Projects: directory layout, input copies, save/load round trips, structure
//! verification and `.delosproject` archives.

use delos::engine::geom::Request;
use delos::engine::source::SourceImage;
use delos::engine::{render_to_cpu, Engine};
use delos::expr;
use delos::graph::{Graph, NodeId};
use delos::nodes::{Value, IMAGE_PARAM};
use delos::project::{self, Project, ARCHIVE_EXT, INPUTS_DIR, PROJECT_FILE};
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

fn png(w: u32, h: u32, seed: u8) -> Vec<u8> {
    let img = image::RgbaImage::from_fn(w, h, |x, y| image::Rgba([(x * 7) as u8 ^ seed, (y * 5) as u8, seed, 200 + (x % 50) as u8]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// Adds an image node the way an import into a project does.
fn add_image(g: &mut Graph, p: &Project, name: &str, bytes: &[u8]) -> NodeId {
    let file = p.add_input(name, bytes).unwrap();
    let d = delos::formats::decode(bytes).unwrap();
    let (w, h) = (d.width, d.height);
    let img = Arc::new(SourceImage::from_rgba8(name, w, h, d.rgba));
    img.set_project_file(file);
    let iid = g.add_image(img);
    let n = g.add_node("image", (0.0, 0.0));
    g.set_param(n, IMAGE_PARAM, Value::Int(iid as i64));
    g.set_param(n, "width", Value::Int(w as i64));
    g.set_param(n, "height", Value::Int(h as i64));
    g.node_mut(n).unwrap().label = Some(name.into());
    n
}

/// photo → reflect ─┐
///          photo ──┴ blend(screen) ─ stack(·, noise) → output, plus a
/// detached checkerboard.
fn sample_graph(p: &Project) -> Graph {
    let mut g = Graph::new();
    let photo = add_image(&mut g, p, "photo.png", &png(64, 40, 3));
    let other = add_image(&mut g, p, "other.png", &png(32, 32, 9));
    let reflect = g.add_node("reflect", (0.0, 0.0));
    g.connect(photo, reflect, 0).unwrap();
    g.set_param(reflect, "position", Value::Float(0.37));
    let blend = g.add_node("blend", (0.0, 0.0));
    g.connect(photo, blend, 0).unwrap();
    g.connect(reflect, blend, 1).unwrap();
    g.set_param(blend, "mode", Value::Choice("screen".into()));
    let noise = g.add_node("noise", (0.0, 0.0));
    g.set_param(noise, "color_a", Value::Color([0.1, 0.2, 0.3, 0.25]));
    let stack = g.add_node("stack", (0.0, 0.0));
    g.connect(blend, stack, 0).unwrap();
    g.connect(noise, stack, 1).unwrap();
    let out = g.output;
    g.connect(stack, out, 0).unwrap();
    g.node_mut(stack).unwrap().label = Some("My \"layers\"".into());
    let checker = g.add_node("checker", (0.0, 0.0));
    let _ = (other, checker);
    g
}

fn render_output(g: &Graph) -> Vec<[f32; 4]> {
    let engine = Engine::new(false);
    let req = Request::native(g.extent_of(g.output));
    render_to_cpu(&engine, g, g.output, &req, &|| false, &|_| {}).unwrap().data
}

fn new_project(root: &Path, name: &str) -> Project {
    Project::create(&root.join(name)).unwrap()
}

#[test]
fn create_makes_the_project_layout() {
    let tmp = tempfile::tempdir().unwrap();
    let p = new_project(tmp.path(), "first");
    assert!(p.dir.join(PROJECT_FILE).is_file());
    assert!(p.dir.join(INPUTS_DIR).is_dir());
    assert_eq!(p.name(), "first");
    let g = p.load().unwrap();
    assert_eq!(g.nodes.len(), 1, "a new project holds just the output");
    // Refuses to overwrite a non-empty directory.
    assert!(Project::create(&p.dir).is_err());
}

#[test]
fn inputs_are_copied_and_deduplicated() {
    let tmp = tempfile::tempdir().unwrap();
    let p = new_project(tmp.path(), "p");
    let a = png(8, 8, 1);
    let b = png(8, 8, 2);
    assert_eq!(p.add_input("cat.png", &a).unwrap(), "inputs/cat.png");
    assert_eq!(p.add_input("cat.png", &a).unwrap(), "inputs/cat.png", "same bytes reuse the copy");
    assert_eq!(p.add_input("cat.png", &b).unwrap(), "inputs/cat-2.png", "different bytes get a new name");
    assert_eq!(p.add_input("/some/dir/we:ird?.png", &a).unwrap(), "inputs/we-ird-.png");
    assert_eq!(std::fs::read(p.dir.join("inputs/cat-2.png")).unwrap(), b);
}

#[test]
fn save_and_load_give_the_same_graph() {
    let tmp = tempfile::tempdir().unwrap();
    let p = new_project(tmp.path(), "roundtrip");
    let g = sample_graph(&p);
    p.save(&g).unwrap();
    let text = std::fs::read_to_string(p.file()).unwrap();
    assert!(text.contains("(def photo (image \"inputs/photo.png\""), "{text}");

    let loaded = p.load().unwrap();
    assert_eq!(loaded.nodes.len(), g.nodes.len());
    assert_eq!(loaded.images.len(), 2);
    // Same text after a second round trip, and the same pixels.
    assert_eq!(expr::to_text(&loaded).unwrap(), text);
    assert_eq!(render_output(&loaded), render_output(&g));
    // Titles, positions and parameters survive.
    let find = |g: &Graph, kind: &str| g.nodes.values().find(|n| n.kind == kind).unwrap().clone();
    assert_eq!(find(&loaded, "stack").label.as_deref(), Some("My \"layers\""));
    assert_eq!(find(&loaded, "noise").params.color("color_a"), [0.1, 0.2, 0.3, 0.25]);
    assert_eq!(find(&loaded, "reflect").pos, (find(&g, "reflect").pos.0.round(), find(&g, "reflect").pos.1.round()));
}

#[test]
fn open_accepts_folder_or_project_file() {
    let tmp = tempfile::tempdir().unwrap();
    let p = new_project(tmp.path(), "openme");
    p.save(&sample_graph(&p)).unwrap();
    let (a, ga) = project::open(&p.dir, tmp.path()).unwrap();
    let (b, gb) = project::open(&p.file(), tmp.path()).unwrap();
    assert_eq!(a, p);
    assert_eq!(b, p);
    assert_eq!(ga.nodes.len(), gb.nodes.len());
    assert!(project::open(&tmp.path().join("nope.txt"), tmp.path()).is_err());
}

#[test]
fn verify_rejects_broken_projects() {
    let tmp = tempfile::tempdir().unwrap();
    let msg = |dir: &Path| format!("{:#}", Project { dir: dir.to_path_buf() }.verify().unwrap_err());

    let empty = tmp.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    assert!(msg(&empty).contains("project.delos is missing"));

    let no_inputs = tmp.path().join("no-inputs");
    std::fs::create_dir_all(&no_inputs).unwrap();
    std::fs::write(no_inputs.join(PROJECT_FILE), "(output)").unwrap();
    assert!(msg(&no_inputs).contains("inputs/ folder is missing"));

    let p = new_project(tmp.path(), "broken");
    p.save_text("(output\n  (image \"inputs/gone.png\"))").unwrap();
    assert!(msg(&p.dir).contains("line 2: the image “inputs/gone.png” is missing"), "{}", msg(&p.dir));

    p.save_text("(output (image \"../secret.png\"))").unwrap();
    assert!(msg(&p.dir).contains("not inside the project's inputs/ folder"));
    p.save_text("(output (image \"/etc/passwd\"))").unwrap();
    assert!(msg(&p.dir).contains("not inside"));
    p.save_text("(output (image \"inputs/../../x.png\"))").unwrap();
    assert!(msg(&p.dir).contains("not inside"));

    p.save_text("(output (invert)").unwrap();
    assert!(msg(&p.dir).contains("line 1:1: unclosed"));

    // A corrupt image is reported by name when loading.
    std::fs::write(p.dir.join("inputs/bad.png"), b"not a png").unwrap();
    p.save_text("(output (image \"inputs/bad.png\"))").unwrap();
    assert!(format!("{:#}", p.load().unwrap_err()).contains("could not open “inputs/bad.png”"));
}

#[test]
fn share_and_import_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let p = new_project(tmp.path(), "Shared Work");
    let g = sample_graph(&p);
    p.save(&g).unwrap();
    let archive = tmp.path().join(format!("shared.{ARCHIVE_EXT}"));
    p.share(&archive).unwrap();

    // It is a zip containing the project folder.
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&archive).unwrap()).unwrap();
    let names: Vec<String> = (0..zip.len()).map(|i| zip.by_index(i).unwrap().name().unwrap().into_owned()).collect();
    for expected in ["Shared Work/project.delos", "Shared Work/inputs/photo.png", "Shared Work/inputs/other.png"] {
        assert!(names.iter().any(|n| n == expected), "{expected} missing from {names:?}");
    }

    let other_root = tmp.path().join("elsewhere");
    let (q, loaded) = project::open(&archive, &other_root).unwrap();
    assert_eq!(q.dir, other_root.join("Shared Work"));
    assert_eq!(render_output(&loaded), render_output(&g));
    assert_eq!(std::fs::read(q.dir.join("inputs/photo.png")).unwrap(), std::fs::read(p.dir.join("inputs/photo.png")).unwrap());
    // Importing again does not overwrite the first copy.
    let (q2, _) = project::open(&archive, &other_root).unwrap();
    assert_eq!(q2.dir, other_root.join("Shared Work-2"));
}

fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, bytes) in entries {
        zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}

#[test]
fn import_accepts_archives_without_a_top_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join(format!("flat.{ARCHIVE_EXT}"));
    let img = png(4, 4, 5);
    write_zip(&archive, &[(PROJECT_FILE, b"(output (invert (image \"inputs/a.png\")))"), ("inputs/a.png", &img)]);
    let (p, g) = project::open(&archive, &tmp.path().join("root")).unwrap();
    assert_eq!(p.name(), "flat");
    assert_eq!(g.nodes.len(), 3);
}

#[test]
fn import_rejects_invalid_archives_and_cleans_up() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("root");
    let err = |entries: &[(&str, &[u8])]| {
        let archive = tmp.path().join(format!("bad.{ARCHIVE_EXT}"));
        write_zip(&archive, entries);
        format!("{:#}", project::open(&archive, &root).unwrap_err())
    };
    assert!(err(&[("readme.txt", b"hi")]).contains("project.delos is missing"));
    assert!(err(&[("p/project.delos", b"(output (image \"inputs/x.png\"))"), ("p/inputs/", b"")]).contains("missing"));
    assert!(err(&[("../evil/project.delos", b"(output)")]).contains("unsafe path"));
    // Nothing is left behind by failed imports.
    let left: Vec<_> = std::fs::read_dir(&root).map(|d| d.flatten().map(|e| e.file_name()).collect()).unwrap_or_default();
    assert!(left.is_empty(), "left behind: {left:?}");

    let not_zip = tmp.path().join(format!("plain.{ARCHIVE_EXT}"));
    std::fs::write(&not_zip, b"definitely not a zip").unwrap();
    assert!(format!("{:#}", project::open(&not_zip, &root).unwrap_err()).contains("is not a .delosproject file"));
}

#[test]
fn names_and_locations() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(project::sanitize_name("  a/b:c  "), "a-b-c");
    assert_eq!(project::sanitize_name("..."), "Untitled");
    std::fs::create_dir_all(tmp.path().join("x")).unwrap();
    assert_eq!(project::unique_dir(tmp.path(), "x"), tmp.path().join("x-2"));
    assert!(project::default_root().ends_with(".delos/projects") || std::env::var_os("DELOS_PROJECTS_DIR").is_some());
}

#[test]
fn large_graphs_serialize_quickly() {
    // 3000 nodes: 100 chains of 30 sharing one source, layered into a
    // stack, plus one 300-deep chain.
    let mut g = Graph::new();
    let shared = g.add_node("noise", (0.0, 0.0));
    let mut layers = Vec::new();
    for c in 0..100 {
        let mut prev = shared;
        for i in 0..30 {
            let n = if i % 3 == 0 { g.add_node("blend", (i as f64, c as f64)) } else { g.add_node("invert", (i as f64, c as f64)) };
            g.connect(prev, n, 0).unwrap();
            if i % 3 == 0 {
                g.connect(shared, n, 1).unwrap();
            }
            prev = n;
        }
        layers.push(Some(prev));
    }
    let mut deep = g.add_node("checker", (0.0, 0.0));
    for _ in 0..300 {
        let n = g.add_node("invert", (0.0, 0.0));
        g.connect(deep, n, 0).unwrap();
        deep = n;
    }
    layers.push(Some(deep));
    let stack = g.add_node("stack", (0.0, 0.0));
    g.set_stack_layers(stack, layers);
    let out = g.output;
    g.connect(stack, out, 0).unwrap();
    let t0 = std::time::Instant::now();
    let text = expr::to_text(&g).unwrap();
    let t1 = std::time::Instant::now();
    let g2 = expr::from_text(&text, &|_| Err("no images".into())).unwrap();
    let t2 = std::time::Instant::now();
    println!("{} nodes: {} KB, serialize {:?}, parse+build {:?}", g.nodes.len(), text.len() / 1024, t1 - t0, t2 - t1);
    assert_eq!(g2.nodes.len(), g.nodes.len());
    assert!((t2 - t0).as_millis() < 1000, "too slow: {:?}", t2 - t0);
}

#[test]
fn sync_keeps_inputs_equal_to_the_images_in_use() {
    let tmp = tempfile::tempdir().unwrap();
    let stash = tmp.path().join("stash");
    let p = new_project(tmp.path(), "sync");
    let mut g = Graph::new();
    let a = add_image(&mut g, &p, "a.png", &png(6, 6, 1));
    let b_bytes = png(6, 6, 2);
    let b = add_image(&mut g, &p, "b.png", &b_bytes);
    let known: Vec<Arc<SourceImage>> = g.images.values().cloned().collect();
    let b_img = known.iter().find(|i| i.name == "b.png").unwrap().clone();
    p.sync_inputs(&g, &known, &stash).unwrap();
    assert_eq!(std::fs::read_dir(p.dir.join(INPUTS_DIR)).unwrap().count(), 2);

    // b is no longer used: its file moves to the stash and the image points there.
    g.remove_node(b);
    p.sync_inputs(&g, &known, &stash).unwrap();
    assert!(!p.dir.join("inputs/b.png").exists());
    assert!(p.dir.join("inputs/a.png").exists());
    assert_eq!(b_img.file().project, None);
    let stashed = b_img.file().origin.unwrap();
    assert!(stashed.starts_with(&stash));
    assert_eq!(std::fs::read(&stashed).unwrap(), b_bytes);

    // It comes back unchanged when used again (as after an undo).
    let iid = g.add_image(b_img.clone());
    let n = g.add_node("image", (0.0, 0.0));
    g.set_param(n, IMAGE_PARAM, Value::Int(iid as i64));
    p.sync_inputs(&g, &known, &stash).unwrap();
    assert_eq!(b_img.file().project.as_deref(), Some("inputs/b.png"));
    assert_eq!(std::fs::read(p.dir.join("inputs/b.png")).unwrap(), b_bytes);
    p.save(&g).unwrap();
    assert_eq!(p.load().unwrap().images.len(), 2);
    let _ = a;
}

#[test]
fn sync_copies_images_without_a_file_and_moves_out_strays() {
    let tmp = tempfile::tempdir().unwrap();
    let stash = tmp.path().join("stash");
    let p = new_project(tmp.path(), "adopt");
    std::fs::write(p.dir.join("inputs/stray.txt"), "x").unwrap();
    // An image known only by its pixels is written as PNG.
    let mut g = Graph::new();
    let img = Arc::new(SourceImage::from_rgba8("pixels.jpg", 2, 1, vec![10, 20, 30, 255, 40, 50, 60, 128]));
    let iid = g.add_image(img.clone());
    let n = g.add_node("image", (0.0, 0.0));
    g.set_param(n, IMAGE_PARAM, Value::Int(iid as i64));
    p.sync_inputs(&g, &[img.clone()], &stash).unwrap();
    assert_eq!(img.file().project.as_deref(), Some("inputs/pixels.png"));
    let decoded = delos::formats::decode(&std::fs::read(p.dir.join("inputs/pixels.png")).unwrap()).unwrap();
    assert_eq!(decoded.rgba, img.to_rgba8());
    assert!(!p.dir.join("inputs/stray.txt").exists());
    assert!(stash.join("stray.txt").exists());
}
