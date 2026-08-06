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

/// Forwards browser shortcuts pressed while the page has keyboard focus.
/// The capture phase listener beats page handlers; only our exact combos
/// are intercepted.
const SHORTCUT_FORWARDER: &str = r#"
document.addEventListener('keydown', (e) => {
  const k = e.key;
  let action = null;
  if (e.altKey && !e.ctrlKey && k === 'ArrowLeft') action = 'back';
  else if (e.altKey && !e.ctrlKey && k === 'ArrowRight') action = 'forward';
  else if (e.ctrlKey && !e.altKey && !e.shiftKey) {
    if (k === 'l' || k === 'L') action = 'focus-url';
    else if (k === 'r' || k === 'R') action = 'reload';
    else if (k === 'h' || k === 'H') action = 'history';
    else if (k === 'd' || k === 'D') action = 'bookmark';
    else if (k === 'f' || k === 'F') action = 'find';
    else if (k === 't' || k === 'T') action = 'tab-new';
    else if (k === 'w' || k === 'W') action = 'tab-close';
    else if (k === 'PageDown') action = 'tab-next';
    else if (k === 'PageUp') action = 'tab-prev';
    else if (k === '=' || k === '+') action = 'zoom-in';
    else if (k === '-') action = 'zoom-out';
    else if (k === '0') action = 'zoom-reset';
  }
  if (action) {
    e.preventDefault();
    e.stopPropagation();
    window.ipc.postMessage(JSON.stringify({cmd: 'shortcut', action: action}));
  }
}, true);
"#;
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
    /// Navigation events `(pane_id, url)` reported by webviews' navigation
    /// handlers, queued for the UI thread to drain on its tick. Webviews and
    /// the UI loop share the main thread, so a thread-local queue is sufficient
    /// (no cross-thread channel needed).
    static NAV_EVENTS: RefCell<Vec<(u64, String)>> = const { RefCell::new(Vec::new()) };
    /// Title-change events `(pane_id, title)`.
    static TITLE_EVENTS: RefCell<Vec<(u64, String)>> = const { RefCell::new(Vec::new()) };
    /// Load-state events `(pane_id, started)`; `false` = finished.
    static LOAD_EVENTS: RefCell<Vec<(u64, bool)>> = const { RefCell::new(Vec::new()) };
    /// IPC messages `(pane_id, origin_uri, json_body)` posted by pages via window.ipc.
    static IPC_EVENTS: RefCell<Vec<(u64, String, String)>> = const { RefCell::new(Vec::new()) };
    /// Find-in-page match counts `(pane_id, count)`.
    static FIND_EVENTS: RefCell<Vec<(u64, u32)>> = const { RefCell::new(Vec::new()) };
    /// New-tab requests `(opener_pane_id, url)` queued when a clicked
    /// `target="_blank"` link asks for a new window.
    static NEW_TAB_EVENTS: RefCell<Vec<(u64, String)>> = const { RefCell::new(Vec::new()) };
    /// Shared WebContext for all webviews. On Linux, wry registers custom URI
    /// schemes at the WebContext level. Sharing one context means the "alterm"
    /// scheme is only registered once; requests are routed to the right webview
    /// by the WebViewId embedded in each request. Using a separate context per
    /// webview would trigger `ContextDuplicateCustomProtocol` on the second
    /// `create_webview` call.
    #[cfg(target_os = "linux")]
    static WEB_CONTEXT: RefCell<Option<wry::WebContext>> = const { RefCell::new(None) };
    /// Directory for persistent webview profile data (cookies, local
    /// storage). Consumed when the shared WebContext is first created.
    #[cfg(target_os = "linux")]
    static WEBVIEW_DATA_DIR: RefCell<Option<std::path::PathBuf>> = const { RefCell::new(None) };
    #[cfg(target_os = "linux")]
    static GTK_INITIALIZED: RefCell<bool> = const { RefCell::new(false) };
    /// Open popup windows `(opener_pane_id, window)` so a pane's popups can
    /// be closed when the pane is destroyed.
    #[cfg(target_os = "linux")]
    static POPUPS: RefCell<Vec<(u64, gtk::Window)>> = const { RefCell::new(Vec::new()) };
}

/// Set the directory used for persistent webview profile data (cookies,
/// local storage). Call once at startup, before any webview is created —
/// once the shared WebContext exists this has no effect.
pub fn set_data_dir(path: &std::path::Path) {
    #[cfg(target_os = "linux")]
    WEBVIEW_DATA_DIR.with(|d| {
        *d.borrow_mut() = Some(path.to_path_buf());
    });
    #[cfg(not(target_os = "linux"))]
    let _ = path;
}

/// Drain queued navigation events. Each is `(pane_id, url)` for a navigation
/// that occurred in a webview (URL-bar submit, link click, redirect, or a
/// confirmed back/forward move). The caller updates the matching pane state.
pub fn drain_nav_events() -> Vec<(u64, String)> {
    NAV_EVENTS.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

/// Drain queued title-change events.
pub fn drain_title_events() -> Vec<(u64, String)> {
    TITLE_EVENTS.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

/// Drain queued load-state events (`true` = started, `false` = finished).
pub fn drain_load_events() -> Vec<(u64, bool)> {
    LOAD_EVENTS.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

/// Drain queued IPC messages posted by pages via window.ipc.
/// Each element is `(pane_id, origin_uri, json_body)`. The `origin_uri` is
/// the URL of the page that posted the message (from wry's Request URI), used
/// by callers to gate destructive commands to trusted internal origins.
pub fn drain_ipc_events() -> Vec<(u64, String, String)> {
    IPC_EVENTS.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

/// Drain queued find-in-page match counts.
pub fn drain_find_events() -> Vec<(u64, u32)> {
    FIND_EVENTS.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

/// Drain queued open-in-new-tab requests from `target="_blank"` links.
pub fn drain_new_tab_events() -> Vec<(u64, String)> {
    NEW_TAB_EVENTS.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

/// Where a WebKit `create` (new window) request should be routed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateDisposition {
    /// Scripted `window.open` — needs a floating popup window backed by a
    /// process-related webview (OAuth relies on window.opener/postMessage).
    Popup,
    /// Clicked link targeting a new window — open the URL as a new tab.
    NewTab,
}

/// Classify a `create` request. `is_link_clicked` = navigation type was
/// `LinkClicked`; `url` may be empty for scripted about:blank popups.
/// Unexpected combinations fall back to `Popup`: a floating window that
/// works beats a dead click.
pub fn classify_create(is_link_clicked: bool, url: &str) -> CreateDisposition {
    if is_link_clicked && !url.is_empty() {
        CreateDisposition::NewTab
    } else {
        CreateDisposition::Popup
    }
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

    // On Linux we share one WebContext across all webviews so that the
    // "alterm" custom URI scheme is registered exactly once.  A second
    // `register_uri_scheme` call on the same context returns
    // `ContextDuplicateCustomProtocol`; wry dispatches requests to the
    // correct webview via the WebViewId embedded in each request.
    #[cfg(target_os = "linux")]
    let webview = {
        // Initialise the shared WebContext the first time a webview is created.
        // A data dir set via `set_data_dir` makes cookies/local storage
        // persist across launches.
        WEB_CONTEXT.with(|ctx| {
            let mut ctx = ctx.borrow_mut();
            if ctx.is_none() {
                let data_dir = WEBVIEW_DATA_DIR.with(|d| d.borrow().clone());
                *ctx = Some(wry::WebContext::new(data_dir));
            }
        });

        // Build with the shared context.  We borrow it mutably inside the
        // closure so the borrow ends before we drop the builder.
        WEB_CONTEXT.with(|ctx| {
            let mut ctx = ctx.borrow_mut();
            let web_ctx = ctx.as_mut().expect("WEB_CONTEXT initialised above");

            // The "alterm" scheme is registered once on the shared WebContext.
            // Calling with_custom_protocol again on the same context would
            // return Err(DuplicateCustomProtocol), so we skip it when the
            // scheme is already registered. Wry dispatches all alterm:// requests
            // through the single registered handler regardless of which webview
            // originated the request.
            let scheme_registered = web_ctx.is_custom_protocol_registered("alterm");

            let builder = WebViewBuilder::new_with_web_context(web_ctx)
                .with_url(url)
                .with_visible(true)
                .with_bounds(Rect {
                    position: LogicalPosition::new(bounds.0, bounds.1).into(),
                    size: LogicalSize::new(bounds.2, bounds.3).into(),
                })
                // Record every navigation so the UI keeps its history accurate.
                .with_navigation_handler(move |url| {
                    log::debug!("[nav-diag] navigation_handler fired: pane={pane_id} url={url}");
                    NAV_EVENTS.with(|q| q.borrow_mut().push((pane_id, url)));
                    true
                })
                .with_document_title_changed_handler(move |title| {
                    TITLE_EVENTS.with(|q| q.borrow_mut().push((pane_id, title)));
                })
                .with_on_page_load_handler(move |event, _url| {
                    let started = matches!(event, wry::PageLoadEvent::Started);
                    LOAD_EVENTS.with(|q| q.borrow_mut().push((pane_id, started)));
                })
                .with_ipc_handler(move |req: wry::http::Request<String>| {
                    IPC_EVENTS.with(|q| q.borrow_mut().push((pane_id, req.uri().to_string(), req.body().clone())));
                })
                .with_initialization_script(SHORTCUT_FORWARDER);

            let builder = if !scheme_registered {
                builder.with_custom_protocol("alterm".into(), |_webview_id, request| {
                    let uri = request.uri().to_string();
                    let (mime, body) = crate::internal_pages::respond(&uri);
                    wry::http::Response::builder()
                        .header("Content-Type", mime)
                        .body(std::borrow::Cow::<'static, [u8]>::Owned(body.into_bytes()))
                        .unwrap_or_else(|_| {
                            wry::http::Response::new(std::borrow::Cow::Borrowed(&b""[..]))
                        })
                })
            } else {
                builder
            };

            builder
                .build_as_child(&wrapper)
                .map_err(|e| format!("Failed to create webview: {e}"))
        })?
    };

    #[cfg(not(target_os = "linux"))]
    let webview = {
        WebViewBuilder::new()
            .with_url(url)
            .with_visible(true)
            .with_bounds(Rect {
                position: LogicalPosition::new(bounds.0, bounds.1).into(),
                size: LogicalSize::new(bounds.2, bounds.3).into(),
            })
            // Record every navigation so the UI keeps its history accurate.
            .with_navigation_handler(move |url| {
                log::debug!("[nav-diag] navigation_handler fired: pane={pane_id} url={url}");
                NAV_EVENTS.with(|q| q.borrow_mut().push((pane_id, url)));
                true
            })
            .with_document_title_changed_handler(move |title| {
                TITLE_EVENTS.with(|q| q.borrow_mut().push((pane_id, title)));
            })
            .with_on_page_load_handler(move |event, _url| {
                let started = matches!(event, wry::PageLoadEvent::Started);
                LOAD_EVENTS.with(|q| q.borrow_mut().push((pane_id, started)));
            })
            .with_ipc_handler(move |req: wry::http::Request<String>| {
                IPC_EVENTS.with(|q| q.borrow_mut().push((pane_id, req.uri().to_string(), req.body().clone())));
            })
            .with_initialization_script(SHORTCUT_FORWARDER)
            .build_as_child(&wrapper)
            .map_err(|e| format!("Failed to create webview: {e}"))?
    };

    // Connect find-in-page match-count signal so drain_find_events is populated.
    #[cfg(target_os = "linux")]
    {
        use webkit2gtk::{FindControllerExt, WebViewExt};
        use wry::WebViewExtUnix;
        if let Some(fc) = webview.webview().find_controller() {
            fc.connect_counted_matches(move |_, count| {
                FIND_EVENTS.with(|q| q.borrow_mut().push((pane_id, count)));
            });
        }
        // Handle window.open popups and target="_blank" links (Google
        // sign-in and friends). See connect_create_handler.
        connect_create_handler(&webview.webview(), pane_id);
    }

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
            match wv.set_bounds(Rect {
                position: LogicalPosition::new(x, y).into(),
                size: LogicalSize::new(w, h).into(),
            }) {
                Ok(()) => log::debug!(
                    "[wv-diag] set_bounds ok: pane {pane_id} -> ({x:.0},{y:.0},{w:.0},{h:.0})"
                ),
                Err(e) => log::warn!("WebView set_bounds failed for pane {pane_id}: {e}"),
            }
        } else {
            log::debug!("[wv-diag] set_bounds: no webview for pane {pane_id}");
        }
    });
}

pub fn set_visible(pane_id: u64, visible: bool) {
    WEBVIEWS.with(|wvs| {
        if let Some(wv) = wvs.borrow().get(&pane_id) {
            match wv.set_visible(visible) {
                Ok(()) => log::debug!("[wv-diag] set_visible({visible}) ok: pane {pane_id}"),
                Err(e) => log::warn!("WebView set_visible({visible}) failed for pane {pane_id}: {e}"),
            }
        } else {
            log::debug!("[wv-diag] set_visible({visible}): no webview for pane {pane_id}");
        }
    });
}

pub fn destroy(pane_id: u64) {
    WEBVIEWS.with(|wvs| {
        if let Some(wv) = wvs.borrow().get(&pane_id) {
            let _ = wv.set_visible(false);
        }
    });

    // Close any popup windows this pane opened; each window's destroy
    // handler removes its own POPUPS entry. Collect first, then close:
    // close() re-enters POPUPS via that handler, so no borrow may be held.
    #[cfg(target_os = "linux")]
    {
        use gtk::prelude::GtkWindowExt;
        let orphans: Vec<gtk::Window> = POPUPS.with(|p| {
            p.borrow()
                .iter()
                .filter(|(owner, _)| *owner == pane_id)
                .map(|(_, w)| w.clone())
                .collect()
        });
        for w in orphans {
            w.close();
        }
    }

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

/// Stop the current page load (Linux: webkit stop_loading; no-op elsewhere).
pub fn stop(pane_id: u64) {
    #[cfg(target_os = "linux")]
    WEBVIEWS.with(|wvs| {
        if let Some(wv) = wvs.borrow().get(&pane_id) {
            use webkit2gtk::WebViewExt;
            use wry::WebViewExtUnix;
            wv.webview().stop_loading();
        }
    });
    #[cfg(not(target_os = "linux"))]
    let _ = pane_id;
}

/// Set the page zoom factor (1.0 = 100%).
pub fn set_zoom(pane_id: u64, level: f64) {
    WEBVIEWS.with(|wvs| {
        if let Some(wv) = wvs.borrow().get(&pane_id) {
            if let Err(e) = wv.zoom(level) {
                log::warn!("WebView zoom failed for pane {pane_id}: {e}");
            }
        }
    });
}

/// Begin (or update) a find-in-page search. Emits match counts via
/// `drain_find_events`.
pub fn find_start(pane_id: u64, text: &str) {
    #[cfg(target_os = "linux")]
    WEBVIEWS.with(|wvs| {
        if let Some(wv) = wvs.borrow().get(&pane_id) {
            use webkit2gtk::{FindControllerExt, FindOptions, WebViewExt};
            use wry::WebViewExtUnix;
            if let Some(fc) = wv.webview().find_controller() {
                let flags = (FindOptions::CASE_INSENSITIVE | FindOptions::WRAP_AROUND).bits();
                fc.count_matches(text, flags, u32::MAX);
                fc.search(text, flags, u32::MAX);
            }
        }
    });
    #[cfg(not(target_os = "linux"))]
    let _ = (pane_id, text);
}

/// Jump to the next find match.
pub fn find_next(pane_id: u64) {
    #[cfg(target_os = "linux")]
    WEBVIEWS.with(|wvs| {
        if let Some(wv) = wvs.borrow().get(&pane_id) {
            use webkit2gtk::{FindControllerExt, WebViewExt};
            use wry::WebViewExtUnix;
            if let Some(fc) = wv.webview().find_controller() {
                fc.search_next();
            }
        }
    });
    #[cfg(not(target_os = "linux"))]
    let _ = pane_id;
}

/// Jump to the previous find match.
pub fn find_prev(pane_id: u64) {
    #[cfg(target_os = "linux")]
    WEBVIEWS.with(|wvs| {
        if let Some(wv) = wvs.borrow().get(&pane_id) {
            use webkit2gtk::{FindControllerExt, WebViewExt};
            use wry::WebViewExtUnix;
            if let Some(fc) = wv.webview().find_controller() {
                fc.search_previous();
            }
        }
    });
    #[cfg(not(target_os = "linux"))]
    let _ = pane_id;
}

/// End the find session and clear highlights.
pub fn find_finish(pane_id: u64) {
    #[cfg(target_os = "linux")]
    WEBVIEWS.with(|wvs| {
        if let Some(wv) = wvs.borrow().get(&pane_id) {
            use webkit2gtk::{FindControllerExt, WebViewExt};
            use wry::WebViewExtUnix;
            if let Some(fc) = wv.webview().find_controller() {
                fc.search_finish();
            }
        }
    });
    #[cfg(not(target_os = "linux"))]
    let _ = pane_id;
}

/// Route WebKit "create" (new window) requests for the webview belonging to
/// `pane_id`: scripted popups get a floating related-webview window; clicked
/// `_blank` links are queued for the app layer to open as a new tab.
///
/// wry only connects this signal itself when a `new_window_req_handler` is
/// set (we never set one), and wry's decide-policy handler ignores
/// new-window decisions, so this handler has the signal to itself.
#[cfg(target_os = "linux")]
fn connect_create_handler(webview: &webkit2gtk::WebView, pane_id: u64) {
    use webkit2gtk::{URIRequestExt, WebViewExt};
    webview.connect_create(move |view, action| {
        let url = action
            .request()
            .and_then(|r| r.uri())
            .map(|u| u.to_string())
            .unwrap_or_default();
        let is_link =
            action.navigation_type() == webkit2gtk::NavigationType::LinkClicked;
        match classify_create(is_link, &url) {
            CreateDisposition::NewTab => {
                log::info!("[popup] _blank link -> new tab: pane={pane_id} url={url}");
                NEW_TAB_EVENTS.with(|q| q.borrow_mut().push((pane_id, url)));
                None
            }
            CreateDisposition::Popup => {
                use gtk::prelude::Cast;
                log::info!("[popup] window.open -> popup: pane={pane_id} url={url:?}");
                Some(build_popup(view, pane_id).upcast::<gtk::Widget>())
            }
        }
    });
}

/// Build a popup webview **related** to `opener` (same web process and
/// session — required so window.opener/postMessage reach the opener page)
/// inside its own floating window. Returns the webview; WebKit loads the
/// popup content into it. Deliberately gets NO wry IPC handler.
#[cfg(target_os = "linux")]
fn build_popup(opener: &webkit2gtk::WebView, opener_pane: u64) -> webkit2gtk::WebView {
    use gtk::prelude::{ContainerExt, GtkWindowExt, WidgetExt};
    use webkit2gtk::WebViewExt;

    let popup = webkit2gtk::WebView::builder().related_view(opener).build();
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("alterm");
    window.add(&popup);

    // Show only once WebKit has applied the site's window features; size
    // from the requested geometry when given, else a sensible OAuth default.
    let win = window.clone();
    popup.connect_ready_to_show(move |wv| {
        let (mut w, mut h) = (500, 640);
        if let Some(props) = wv.window_properties() {
            if let Ok(geo) = props.property_value("geometry").get::<gtk::gdk::Rectangle>() {
                if geo.width() > 0 && geo.height() > 0 {
                    w = geo.width().clamp(200, 1600);
                    h = geo.height().clamp(200, 1200);
                }
            }
        }
        win.set_default_size(w, h);
        win.set_position(gtk::WindowPosition::Center);
        win.show_all();
    });

    // Keep the window title in sync with the page.
    let win = window.clone();
    popup.connect_title_notify(move |wv| {
        if let Some(t) = wv.title() {
            win.set_title(&t);
        }
    });

    // The page called window.close() (OAuth popups do this when done).
    let win = window.clone();
    popup.connect_close(move |_| win.close());

    // Popups can themselves open popups or _blank links; attribute them to
    // the original opener pane.
    connect_create_handler(&popup, opener_pane);

    // Track for cleanup; self-remove when the window is destroyed (either
    // via window.close() above or the user closing it).
    POPUPS.with(|p| p.borrow_mut().push((opener_pane, window.clone())));
    window.connect_destroy(|w| {
        POPUPS.with(|p| p.borrow_mut().retain(|(_, win)| win != w));
    });

    popup
}

#[cfg(test)]
mod tests {
    use super::{classify_create, drain_new_tab_events, CreateDisposition, NEW_TAB_EVENTS};

    #[test]
    fn classify_link_clicks_open_new_tab() {
        assert_eq!(
            classify_create(true, "https://example.com/"),
            CreateDisposition::NewTab
        );
    }

    #[test]
    fn classify_scripted_opens_are_popups() {
        // Google's account chooser does window.open() — must be a popup.
        assert_eq!(
            classify_create(false, "https://accounts.google.com/o/oauth2/auth"),
            CreateDisposition::Popup
        );
        // Scripted about:blank popups have no URL yet.
        assert_eq!(classify_create(false, ""), CreateDisposition::Popup);
    }

    #[test]
    fn classify_link_without_url_falls_back_to_popup() {
        assert_eq!(classify_create(true, ""), CreateDisposition::Popup);
    }

    #[test]
    fn new_tab_events_drain_and_clear() {
        NEW_TAB_EVENTS.with(|q| q.borrow_mut().push((7, "https://a.com/".into())));
        assert_eq!(drain_new_tab_events(), vec![(7, "https://a.com/".to_string())]);
        assert!(drain_new_tab_events().is_empty());
    }
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
