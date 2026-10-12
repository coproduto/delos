//! Platform glue: getting rendered frames into the webview, and saving
//! exported files.
//!
//! * Desktop/mobile: frames are served from memory by a custom protocol
//!   handler (`/delosframe/<seq>.bmp`), avoiding any IPC serialisation.
//! * Web: frames become `blob:` URLs.

use dioxus::prelude::*;
use std::sync::Arc;

#[cfg(feature = "native")]
mod native {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    pub static FRAMES: Mutex<VecDeque<(String, Arc<Vec<u8>>)>> = Mutex::new(VecDeque::new());

    pub fn store(name: String, bytes: Arc<Vec<u8>>) {
        let mut f = FRAMES.lock().unwrap();
        f.push_back((name, bytes));
        while f.len() > 4 {
            f.pop_front();
        }
    }

    pub fn get(name: &str) -> Option<Arc<Vec<u8>>> {
        FRAMES.lock().unwrap().iter().rev().find(|(n, _)| n == name).map(|(_, b)| b.clone())
    }
}

/// Registers whatever the platform needs to serve frames. Call once from the
/// root component.
pub fn use_frame_server() {
    #[cfg(feature = "native")]
    {
        #[cfg(feature = "desktop")]
        use dioxus::desktop::{use_asset_handler, wry::http::Response};
        #[cfg(all(feature = "mobile", not(feature = "desktop")))]
        use dioxus::mobile::{use_asset_handler, wry::http::Response};
        use_asset_handler("delosframe", |req, responder| {
            let name = req.uri().path().trim_start_matches('/').trim_start_matches("delosframe/").to_string();
            match native::get(&name) {
                Some(bytes) => responder.respond(
                    Response::builder().header("Content-Type", "image/bmp").header("Cache-Control", "no-store").body(bytes.to_vec()).unwrap(),
                ),
                None => responder.respond(Response::builder().status(404).body(Vec::new()).unwrap()),
            }
        });
    }
}

/// Makes a frame available to `<img>` and returns its URL.
pub fn frame_url(seq: u64, provisional: bool, bytes: Arc<Vec<u8>>) -> String {
    let name = format!("{seq}{}.bmp", if provisional { "p" } else { "" });
    #[cfg(feature = "native")]
    {
        native::store(name.clone(), bytes);
        format!("/delosframe/{name}")
    }
    #[cfg(not(feature = "native"))]
    {
        let _ = name;
        web::blob_url(&bytes, "image/bmp", true).unwrap_or_default()
    }
}

#[cfg(feature = "web")]
pub mod web {
    use std::cell::RefCell;
    use wasm_bindgen::JsCast;

    thread_local! {
        static LIVE: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }

    /// Creates a blob URL. Frame URLs are recycled (only the last few stay
    /// alive) so memory does not grow while editing.
    pub fn blob_url(bytes: &[u8], mime: &str, recycle: bool) -> Option<String> {
        let arr = js_sys::Uint8Array::from(bytes);
        let parts = js_sys::Array::of1(&arr);
        let opts = web_sys::BlobPropertyBag::new();
        opts.set_type(mime);
        let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &opts).ok()?;
        let url = web_sys::Url::create_object_url_with_blob(&blob).ok()?;
        if recycle {
            LIVE.with(|l| {
                let mut l = l.borrow_mut();
                l.push(url.clone());
                while l.len() > 4 {
                    let old = l.remove(0);
                    let _ = web_sys::Url::revoke_object_url(&old);
                }
            });
        }
        Some(url)
    }

    pub fn download(name: &str, mime: &str, bytes: &[u8]) -> Result<(), String> {
        let url = blob_url(bytes, mime, false).ok_or("could not create blob")?;
        let doc = web_sys::window().and_then(|w| w.document()).ok_or("no document")?;
        let a = doc.create_element("a").map_err(|_| "no anchor")?.dyn_into::<web_sys::HtmlAnchorElement>().map_err(|_| "no anchor")?;
        a.set_href(&url);
        a.set_download(name);
        a.click();
        let _ = web_sys::Url::revoke_object_url(&url);
        Ok(())
    }
}

/// Where an export should go, chosen before rendering starts.
pub enum SaveTarget {
    #[cfg(feature = "native")]
    Path(std::path::PathBuf),
    /// Rendered to memory, then handed to [`deliver_bytes`].
    Memory,
    Cancelled,
}

/// Asks the user where to save (desktop) or prepares an in-memory export.
pub async fn choose_save_target(file_name: &str, ext: &[&str], format_name: &str) -> SaveTarget {
    #[cfg(feature = "desktop")]
    {
        let dialog = rfd::AsyncFileDialog::new().set_file_name(file_name).add_filter(format_name, ext);
        match dialog.save_file().await {
            Some(h) => SaveTarget::Path(h.path().to_path_buf()),
            None => SaveTarget::Cancelled,
        }
    }
    #[cfg(all(feature = "mobile", not(feature = "desktop")))]
    {
        let _ = (ext, format_name);
        let dir = std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join("Documents")).unwrap_or_else(std::env::temp_dir);
        let _ = std::fs::create_dir_all(&dir);
        SaveTarget::Path(dir.join(file_name))
    }
    #[cfg(not(feature = "native"))]
    {
        let _ = (file_name, ext, format_name);
        SaveTarget::Memory
    }
}

/// Hands in-memory export bytes to the user (browser download).
pub fn deliver_bytes(name: &str, mime: &str, bytes: &[u8]) -> Result<String, String> {
    #[cfg(feature = "web")]
    {
        web::download(name, mime, bytes)?;
        Ok(format!("Downloaded {name}"))
    }
    #[cfg(not(feature = "web"))]
    {
        let _ = mime;
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
        Ok(format!("Saved to {}", path.display()))
    }
}

/// Device pixel ratio of the webview.
pub async fn device_pixel_ratio() -> f64 {
    document::eval("return window.devicePixelRatio || 1;").join::<f64>().await.unwrap_or(1.0).clamp(0.5, 4.0)
}
