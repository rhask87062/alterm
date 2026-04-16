/// WebView manager — manages wry WebView instances on the main thread.
///
/// `wry::WebView` is `!Send`, so we store all instances in a `thread_local!`.

use std::cell::RefCell;
use std::collections::HashMap;

use raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle, WindowHandle};
use wry::dpi::{LogicalPosition, LogicalSize};
use wry::{Rect, WebView, WebViewBuilder};

// Platform-specific imports
#[cfg(target_os = "linux")]
use {
    gtk::prelude::ObjectExt,
    raw_window_handle::XlibWindowHandle,
};
#[cfg(target_os = "macos")]
use {
    raw_window_handle::AppKitWindowHandle,
    std::ffi::c_void,
    std::ptr::NonNull,
};
#[cfg(target_os = "windows")]
use {
    raw_window_handle::Win32WindowHandle,
    std::num::NonZeroIsize,
};

thread_local! {
    static WEBVIEWS: RefCell<HashMap<u64, WebView>> = RefCell::new(HashMap::new());
    #[cfg(target_os = "linux")]
    static GTK_INITIALIZED: RefCell<bool> = RefCell::new(false);
}

/// Ensure GTK is initialized. No-op on non-Linux platforms.
pub fn init_gtk() {
    #[cfg(target_os = "linux")]
    GTK_INITIALIZED.with(|init| {
        if !*init.borrow() {
            gtk::init().expect("Failed to init GTK");
            if let Some(settings) = gtk::Settings::default() {
                settings.set_property("gtk-application-prefer-dark-theme", true);
            }
            *init.borrow_mut() = true;
        }
    });
}

/// Pump pending GTK events. No-op on non-Linux platforms.
pub fn pump_gtk_events() {
    #[cfg(target_os = "linux")]
    {
        let has_webviews = WEBVIEWS.with(|wvs| !wvs.borrow().is_empty());
        if !has_webviews {
            return;
        }
        GTK_INITIALIZED.with(|init| {
            if *init.borrow() {
                let mut count = 0;
                while gtk::events_pending() && count < 50 {
                    gtk::main_iteration_do(false);
                    count += 1;
                }
            }
        });
    }
}

/// Create a webview as a child of the native window identified by `parent_id`.
///
/// - Linux: `parent_id` is an X11 XID.
/// - macOS: `parent_id` is an NSView pointer.
/// - Windows: `parent_id` is an HWND.
pub fn create_webview(
    pane_id: u64,
    parent_id: u64,
    url: &str,
    bounds: (f64, f64, f64, f64),
) -> Result<(), String> {
    init_gtk();

    let wrapper = NativeParent(parent_id);

    let webview = WebViewBuilder::new()
        .with_url(url)
        .with_visible(true)
        .with_bounds(Rect {
            position: LogicalPosition::new(bounds.0, bounds.1).into(),
            size: LogicalSize::new(bounds.2, bounds.3).into(),
        })
        .build_as_child(&wrapper)
        .map_err(|e| format!("Failed to create webview: {e}"))?;

    WEBVIEWS.with(|wvs| {
        wvs.borrow_mut().insert(pane_id, webview);
    });

    log::info!(
        "WebView created: pane_id={pane_id} url={url} bounds=({}, {}, {}, {})",
        bounds.0, bounds.1, bounds.2, bounds.3
    );

    Ok(())
}

pub fn navigate(pane_id: u64, url: &str) {
    WEBVIEWS.with(|wvs| {
        if let Some(wv) = wvs.borrow().get(&pane_id) {
            if let Err(e) = wv.load_url(url) {
                log::warn!("WebView navigate failed for pane {pane_id}: {e}");
            }
        }
    });
}

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

pub fn set_visible(pane_id: u64, visible: bool) {
    WEBVIEWS.with(|wvs| {
        if let Some(wv) = wvs.borrow().get(&pane_id) {
            if let Err(e) = wv.set_visible(visible) {
                log::warn!("WebView set_visible({visible}) failed for pane {pane_id}: {e}");
            }
        }
    });
}

pub fn destroy(pane_id: u64) {
    WEBVIEWS.with(|wvs| {
        if let Some(wv) = wvs.borrow().get(&pane_id) {
            let _ = wv.set_visible(false);
        }
    });

    #[cfg(target_os = "linux")]
    GTK_INITIALIZED.with(|init| {
        if *init.borrow() {
            let mut count = 0;
            while gtk::events_pending() && count < 20 {
                gtk::main_iteration_do(false);
                count += 1;
            }
        }
    });

    WEBVIEWS.with(|wvs| {
        if wvs.borrow_mut().remove(&pane_id).is_some() {
            log::info!("WebView destroyed: pane_id={pane_id}");
        }
    });
}

pub fn exists(pane_id: u64) -> bool {
    WEBVIEWS.with(|wvs| wvs.borrow().contains_key(&pane_id))
}

pub fn reload(pane_id: u64) {
    WEBVIEWS.with(|wvs| {
        if let Some(wv) = wvs.borrow().get(&pane_id) {
            if let Err(e) = wv.evaluate_script("location.reload()") {
                log::warn!("WebView reload failed for pane {pane_id}: {e}");
            }
        }
    });
}

pub fn go_back(pane_id: u64) {
    WEBVIEWS.with(|wvs| {
        if let Some(wv) = wvs.borrow().get(&pane_id) {
            if let Err(e) = wv.evaluate_script("history.back()") {
                log::warn!("WebView go_back failed for pane {pane_id}: {e}");
            }
        }
    });
}

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
// Platform-specific parent window handle wrappers
// ---------------------------------------------------------------------------

struct NativeParent(u64);

#[cfg(target_os = "linux")]
impl HasWindowHandle for NativeParent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let handle = XlibWindowHandle::new(self.0 as std::ffi::c_ulong);
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Xlib(handle)) })
    }
}

#[cfg(target_os = "macos")]
impl HasWindowHandle for NativeParent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let ptr = NonNull::new(self.0 as *mut c_void).ok_or(HandleError::Unavailable)?;
        let handle = AppKitWindowHandle::new(ptr);
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::AppKit(handle)) })
    }
}

#[cfg(target_os = "windows")]
impl HasWindowHandle for NativeParent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let hwnd = NonZeroIsize::new(self.0 as isize).ok_or(HandleError::Unavailable)?;
        let handle = Win32WindowHandle::new(hwnd);
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Win32(handle)) })
    }
}
