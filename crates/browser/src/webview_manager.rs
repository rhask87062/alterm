/// WebView manager — manages wry WebView instances on the main thread.
///
/// `wry::WebView` is `!Send`, so we store all instances in a `thread_local!`.
/// This is safe because iced's `update()` and `view()` run on the main thread.

use std::cell::RefCell;
use std::collections::HashMap;

use gtk::prelude::ObjectExt;
use raw_window_handle::{
    HandleError, HasWindowHandle, RawWindowHandle, WindowHandle, XlibWindowHandle,
};
use wry::dpi::{LogicalPosition, LogicalSize};
use wry::{Rect, WebView, WebViewBuilder};

thread_local! {
    static WEBVIEWS: RefCell<HashMap<u64, WebView>> = RefCell::new(HashMap::new());
    static GTK_INITIALIZED: RefCell<bool> = RefCell::new(false);
}

/// Ensure GTK is initialized (required by webkit2gtk before any webview creation).
pub fn init_gtk() {
    GTK_INITIALIZED.with(|init| {
        if !*init.borrow() {
            // Force WebKit to use software rendering — avoids GBM/DRM permission
            // errors on systems where GPU buffer allocation is restricted (common
            // with NVIDIA proprietary drivers).
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
            std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");

            // Force dark theme BEFORE GTK init so WebKit picks it up immediately.
            // This makes prefers-color-scheme: dark work on the first page load.
            std::env::set_var("GTK_THEME", "Adwaita:dark");

            gtk::init().expect("Failed to init GTK");

            // Also set the GTK property for any runtime checks
            if let Some(settings) = gtk::Settings::default() {
                settings.set_property("gtk-application-prefer-dark-theme", true);
            }

            *init.borrow_mut() = true;
        }
    });
}

/// Pump pending GTK events so webkit2gtk can process network/render tasks.
/// Call this every tick from the iced main loop.
pub fn pump_gtk_events() {
    GTK_INITIALIZED.with(|init| {
        if *init.borrow() {
            // Process up to a limited number of events per tick to avoid blocking.
            let mut count = 0;
            while gtk::events_pending() && count < 50 {
                gtk::main_iteration_do(false);
                count += 1;
            }
        }
    });
}

/// Create a webview as a child of the given X11 window.
///
/// - `pane_id`: unique identifier for tracking (derived from pane index).
/// - `parent_xid`: the X11 window ID of the iced window.
/// - `url`: initial URL to load.
/// - `bounds`: (x, y, width, height) in logical pixels, relative to the parent window.
pub fn create_webview(
    pane_id: u64,
    parent_xid: u64,
    url: &str,
    bounds: (f64, f64, f64, f64),
) -> Result<(), String> {
    init_gtk();

    let wrapper = X11Parent(parent_xid);

    // Start with a dark blank page so WebKit's web process initializes with
    // dark mode active. We then navigate to the real URL immediately after.
    let dark_blank = "data:text/html,<html style='color-scheme:dark;background:%23121214'><head><meta name='color-scheme' content='dark'></head><body></body></html>";

    let real_url = url.to_string();
    let webview = WebViewBuilder::new()
        .with_url(dark_blank)
        .with_visible(true)
        .with_bounds(Rect {
            position: LogicalPosition::new(bounds.0, bounds.1).into(),
            size: LogicalSize::new(bounds.2, bounds.3).into(),
        })
        .build_as_child(&wrapper)
        .map_err(|e| format!("Failed to create webview: {e}"))?;

    // Navigate to the real URL now that dark mode is established
    let _ = webview.load_url(&real_url);

    WEBVIEWS.with(|wvs| {
        wvs.borrow_mut().insert(pane_id, webview);
    });

    log::info!(
        "WebView created: pane_id={pane_id} url={url} bounds=({}, {}, {}, {})",
        bounds.0,
        bounds.1,
        bounds.2,
        bounds.3
    );

    Ok(())
}

/// Navigate an existing webview to a new URL.
pub fn navigate(pane_id: u64, url: &str) {
    WEBVIEWS.with(|wvs| {
        if let Some(wv) = wvs.borrow().get(&pane_id) {
            if let Err(e) = wv.load_url(url) {
                log::warn!("WebView navigate failed for pane {pane_id}: {e}");
            }
        }
    });
}

/// Update the position and size of an existing webview.
pub fn set_bounds(pane_id: u64, x: f64, y: f64, w: f64, h: f64) {
    WEBVIEWS.with(|wvs| {
        if let Some(wv) = wvs.borrow().get(&pane_id) {
            if let Err(e) = wv.set_bounds(Rect {
                position: LogicalPosition::new(x, y).into(),
                size: LogicalSize::new(w, h).into(),
            }) {
                log::warn!("WebView set_bounds failed for pane {pane_id}: {e}");
            }
        }
    });
}

/// Show or hide a webview.
pub fn set_visible(pane_id: u64, visible: bool) {
    WEBVIEWS.with(|wvs| {
        if let Some(wv) = wvs.borrow().get(&pane_id) {
            if let Err(e) = wv.set_visible(visible) {
                log::warn!("WebView set_visible({visible}) failed for pane {pane_id}: {e}");
            }
        }
    });
}

/// Destroy a webview, removing it from tracking.
pub fn destroy(pane_id: u64) {
    WEBVIEWS.with(|wvs| {
        if wvs.borrow_mut().remove(&pane_id).is_some() {
            log::info!("WebView destroyed: pane_id={pane_id}");
        }
    });
}

/// Check whether a webview exists for the given pane.
pub fn exists(pane_id: u64) -> bool {
    WEBVIEWS.with(|wvs| wvs.borrow().contains_key(&pane_id))
}

/// Reload the current page in the webview.
pub fn reload(pane_id: u64) {
    WEBVIEWS.with(|wvs| {
        if let Some(wv) = wvs.borrow().get(&pane_id) {
            // wry doesn't have a direct reload method; re-load the current URL
            // via JavaScript.
            if let Err(e) = wv.evaluate_script("location.reload()") {
                log::warn!("WebView reload failed for pane {pane_id}: {e}");
            }
        }
    });
}

/// Go back in the webview's navigation history.
pub fn go_back(pane_id: u64) {
    WEBVIEWS.with(|wvs| {
        if let Some(wv) = wvs.borrow().get(&pane_id) {
            if let Err(e) = wv.evaluate_script("history.back()") {
                log::warn!("WebView go_back failed for pane {pane_id}: {e}");
            }
        }
    });
}

/// Go forward in the webview's navigation history.
pub fn go_forward(pane_id: u64) {
    WEBVIEWS.with(|wvs| {
        if let Some(wv) = wvs.borrow().get(&pane_id) {
            if let Err(e) = wv.evaluate_script("history.forward()") {
                log::warn!("WebView go_forward failed for pane {pane_id}: {e}");
            }
        }
    });
}

// ---------------------------------------------------------------------------
// X11 parent window handle wrapper
// ---------------------------------------------------------------------------

/// Wrapper that implements `HasWindowHandle` for an X11 window ID (XID).
struct X11Parent(u64);

impl HasWindowHandle for X11Parent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let handle = XlibWindowHandle::new(self.0 as std::ffi::c_ulong);
        // SAFETY: the handle is valid for the lifetime of this borrow.
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Xlib(handle)) })
    }
}
