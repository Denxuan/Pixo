#[cfg(target_os = "macos")]
use std::ffi::c_void;
#[cfg(target_os = "macos")]
use std::sync::Mutex;
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "macos")]
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState, ShortcutWrapper};

#[cfg(target_os = "macos")]
use block2::RcBlock;
#[cfg(target_os = "macos")]
use dispatch2::{DispatchQueue, DispatchTime};
#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2::runtime::{AnyClass, AnyObject};
#[cfg(target_os = "macos")]
use objc2::{MainThreadMarker, Message, msg_send, sel};
#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSEvent, NSEventMask, NSFloatingWindowLevel,
    NSResponder, NSView, NSWindow, NSWindowAnimationBehavior, NSWindowCollectionBehavior,
    NSWindowStyleMask,
};
#[cfg(target_os = "macos")]
use objc2_foundation::{NSPoint, NSRect, NSSize};
#[cfg(target_os = "macos")]
use std::ptr::NonNull;

use crate::clipboard::ClipboardError;

const PANEL_LABEL: &str = "clipboard-panel";

enum Placement {
    Input,
    Status(tauri::Rect),
}

pub fn show_panel(app: &AppHandle) {
    let app = app.clone();
    let _ = app
        .clone()
        .run_on_main_thread(move || present(&app, false, Placement::Input));
}

pub fn toggle_panel(app: &AppHandle) {
    let app = app.clone();
    let _ = app
        .clone()
        .run_on_main_thread(move || present(&app, true, Placement::Input));
}

/// Opens the panel under the status item, or closes it when that panel is already showing.
pub fn toggle_from_status(app: &AppHandle, rect: tauri::Rect) {
    let app = app.clone();
    let _ = app
        .clone()
        .run_on_main_thread(move || present(&app, true, Placement::Status(rect)));
}

pub fn present_tool(app: &AppHandle, path: &str, host_id: Option<i64>) {
    let app = app.clone();
    let path = path.to_string();
    let _ = app.clone().run_on_main_thread(move || {
        hide_panel(&app);
        if crate::ensure_main_window(&app).is_err() {
            return;
        }
        let Some(window) = app.get_webview_window("main") else {
            return;
        };
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.emit(
            "clipboard://open-main",
            serde_json::json!({ "path": path, "enable": false, "hostId": host_id }),
        );
    });
}

#[tauri::command]
pub fn launcher_open(app: AppHandle, path: String, host_id: Option<i64>) {
    let allowed = matches!(
        path.as_str(),
        "/clipboard" | "/screenshot" | "/ssh" | "/docker"
    );
    if !allowed || (host_id.is_some() && path != "/ssh" && path != "/docker") {
        return;
    }
    present_tool(&app, &path, host_id);
}

pub fn present_main(app: &AppHandle, path: &str, enable: bool) {
    let app = app.clone();
    let path = path.to_string();
    let _ = app.clone().run_on_main_thread(move || {
        hide_panel(&app);
        if crate::ensure_main_window(&app).is_err() {
            return;
        }
        let Some(window) = app.get_webview_window("main") else {
            return;
        };
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.emit(
            "clipboard://open-main",
            serde_json::json!({ "path": path, "enable": enable }),
        );
    });
}

pub fn parse_shortcut(shortcut: &str) -> Result<(), ClipboardError> {
    ShortcutWrapper::try_from(shortcut)
        .map(|_| ())
        .map_err(|_| ClipboardError::new("invalid_input"))
}

pub fn sync_shortcut(
    app: &AppHandle,
    desired: &str,
    active: bool,
    registered: &mut Option<String>,
) -> Result<(), ClipboardError> {
    if !active {
        if let Some(current) = registered.take() {
            let _ = app.global_shortcut().unregister(current.as_str());
        }
        return Ok(());
    }
    if registered.as_deref() == Some(desired) {
        return Ok(());
    }
    parse_shortcut(desired)?;
    let previous = registered.clone();
    if let Some(previous) = previous.as_deref() {
        let _ = app.global_shortcut().unregister(previous);
    }
    match register(app, desired) {
        Ok(()) => {
            *registered = Some(desired.to_string());
            Ok(())
        }
        Err(error) => {
            eprintln!("clipboard shortcut: {error}");
            *registered = None;
            if let Some(previous) = previous {
                if register(app, &previous).is_ok() {
                    *registered = Some(previous);
                }
            }
            Err(ClipboardError::new("shortcut_conflict"))
        }
    }
}

fn register(app: &AppHandle, shortcut: &str) -> Result<(), tauri_plugin_global_shortcut::Error> {
    let app_handle = app.clone();
    app.global_shortcut()
        .on_shortcut(shortcut, move |_app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                toggle_panel(&app_handle);
            }
        })
}

fn present(app: &AppHandle, toggle: bool, placement: Placement) {
    if let Some(window) = app.get_webview_window(PANEL_LABEL) {
        if toggle && panel_is_showing(&window) {
            dismiss(app);
        } else {
            reveal(app, &window, placement);
        }
        return;
    }
    if let Err(error) = open_panel(app, placement) {
        eprintln!("clipboard panel: {error}");
    }
}

static LAUNCHER_SHORTCUT: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

#[tauri::command]
pub fn launcher_set_shortcut(app: AppHandle, shortcut: String) -> Result<(), ClipboardError> {
    let shortcut = shortcut.trim().to_string();
    let mut registered = LAUNCHER_SHORTCUT
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if shortcut.is_empty() {
        if let Some(current) = registered.take() {
            let _ = app.global_shortcut().unregister(current.as_str());
        }
        return Ok(());
    }
    if registered.as_deref() == Some(shortcut.as_str()) {
        return Ok(());
    }
    parse_shortcut(&shortcut)?;
    let previous = registered.clone();
    if let Some(previous) = previous.as_deref() {
        let _ = app.global_shortcut().unregister(previous);
    }
    let app_handle = app.clone();
    match app
        .global_shortcut()
        .on_shortcut(shortcut.as_str(), move |_app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                open_launcher(&app_handle);
            }
        }) {
        Ok(()) => {
            *registered = Some(shortcut);
            Ok(())
        }
        Err(error) => {
            eprintln!("launcher shortcut: {error}");
            *registered = None;
            if let Some(previous) = previous {
                let app_handle = app.clone();
                if app
                    .global_shortcut()
                    .on_shortcut(previous.as_str(), move |_app, _shortcut, event| {
                        if event.state == ShortcutState::Pressed {
                            open_launcher(&app_handle);
                        }
                    })
                    .is_ok()
                {
                    *registered = Some(previous);
                }
            }
            Err(ClipboardError::new("shortcut_conflict"))
        }
    }
}

fn open_launcher(app: &AppHandle) {
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        dismiss(&app);
        if crate::ensure_main_window(&app).is_err() {
            return;
        }
        let Some(window) = app.get_webview_window("main") else {
            return;
        };
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.emit("launcher://toggle", ());
    });
}

#[tauri::command]
pub fn clipboard_dismiss_panel(app: AppHandle) {
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || dismiss(&app));
}

pub(crate) fn dismiss_panel(app: &AppHandle) {
    dismiss(app);
}

fn dismiss(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    resign_keyboard();
    hide_panel(app);
}

fn hide_panel(app: &AppHandle) {
    stop_outside_click();
    #[cfg(target_os = "macos")]
    stop_key_capture();
    if let Some(window) = app.get_webview_window(PANEL_LABEL) {
        let _ = window.hide();
    }
}

fn start_outside_click(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    watch_outside_clicks(app);
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

fn stop_outside_click() {
    #[cfg(target_os = "macos")]
    clear_outside_clicks();
}

#[derive(Clone, Copy)]
struct ScreenBox {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

fn contains_point(area: ScreenBox, x: f64, y: f64) -> bool {
    x >= area.x && y >= area.y && x <= area.x + area.width && y <= area.y + area.height
}

/// A press closes the panel when it lands outside the card. The status item is
/// excluded so that same click can still toggle the panel shut on mouse up.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn outside_click_dismisses(panel: ScreenBox, status: Option<ScreenBox>, x: f64, y: f64) -> bool {
    if contains_point(panel, x, y) {
        return false;
    }
    if status.is_some_and(|item| contains_point(item, x, y)) {
        return false;
    }
    true
}

#[cfg(target_os = "macos")]
struct ClickWatch {
    global_monitor: *mut AnyObject,
    local_monitor: *mut AnyObject,
    global_block: *mut block2::Block<dyn Fn(NonNull<NSEvent>)>,
    local_block: *mut block2::Block<dyn Fn(NonNull<NSEvent>) -> *mut NSEvent>,
}

#[cfg(target_os = "macos")]
unsafe impl Send for ClickWatch {}

#[cfg(target_os = "macos")]
static CLICK_WATCH: Mutex<Option<ClickWatch>> = Mutex::new(None);

#[cfg(target_os = "macos")]
fn watch_outside_clicks(app: &AppHandle) {
    let mut slot = match CLICK_WATCH.lock() {
        Ok(slot) => slot,
        Err(poisoned) => poisoned.into_inner(),
    };
    if slot.is_some() {
        return;
    }
    let mask =
        NSEventMask::LeftMouseDown | NSEventMask::RightMouseDown | NSEventMask::OtherMouseDown;
    let global_app = app.clone();
    let global_block: RcBlock<dyn Fn(NonNull<NSEvent>)> =
        RcBlock::new(move |_event: NonNull<NSEvent>| note_outside_click(&global_app));
    let local_app = app.clone();
    let local_block: RcBlock<dyn Fn(NonNull<NSEvent>) -> *mut NSEvent> =
        RcBlock::new(move |event: NonNull<NSEvent>| {
            note_outside_click(&local_app);
            event.as_ptr()
        });
    let global_monitor =
        NSEvent::addGlobalMonitorForEventsMatchingMask_handler(mask, &global_block);
    let local_monitor =
        unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &local_block) };
    if global_monitor.is_none() && local_monitor.is_none() {
        eprintln!("clipboard panel: outside clicks cannot close the panel");
        return;
    }
    *slot = Some(ClickWatch {
        global_monitor: global_monitor
            .map(Retained::into_raw)
            .unwrap_or(std::ptr::null_mut()),
        local_monitor: local_monitor
            .map(Retained::into_raw)
            .unwrap_or(std::ptr::null_mut()),
        global_block: RcBlock::into_raw(global_block),
        local_block: RcBlock::into_raw(local_block),
    });
}

#[cfg(target_os = "macos")]
fn clear_outside_clicks() {
    let watch = {
        let mut slot = match CLICK_WATCH.lock() {
            Ok(slot) => slot,
            Err(poisoned) => poisoned.into_inner(),
        };
        slot.take()
    };
    let Some(watch) = watch else {
        return;
    };
    unsafe {
        if !watch.global_monitor.is_null() {
            let monitor = Retained::from_raw(watch.global_monitor).unwrap();
            NSEvent::removeMonitor(&monitor);
        }
        if !watch.local_monitor.is_null() {
            let monitor = Retained::from_raw(watch.local_monitor).unwrap();
            NSEvent::removeMonitor(&monitor);
        }
        if !watch.global_block.is_null() {
            drop(RcBlock::from_raw(watch.global_block));
        }
        if !watch.local_block.is_null() {
            drop(RcBlock::from_raw(watch.local_block));
        }
    }
}

#[cfg(target_os = "macos")]
fn note_outside_click(app: &AppHandle) {
    let point = NSEvent::mouseLocation();
    let x = point.x;
    let y = point.y;
    let app = app.clone();
    // The monitor can be running inside the mouse-down that it may remove, so the
    // hit test and hide happen on the next turn of the main queue.
    let _ = DispatchQueue::main().exec_async(move || {
        let Some(window) = app.get_webview_window(PANEL_LABEL) else {
            return;
        };
        if !window.is_visible().unwrap_or(false) {
            return;
        }
        let Some(ns_window) = retained_window(&window) else {
            return;
        };
        let frame = ns_window.frame();
        let panel = ScreenBox {
            x: frame.origin.x,
            y: frame.origin.y,
            width: frame.size.width,
            height: frame.size.height,
        };
        let status = crate::tray::status_button_frame().map(|(x, y, width, height)| ScreenBox {
            x,
            y,
            width,
            height,
        });
        if let Some(ns_window) = retained_window(&window) {
            // A click that focuses the pin title otherwise fills this transparent
            // webview with black and hides the panel.
            clear_panel_surface(&ns_window);
        }
        if outside_click_dismisses(panel, status, x, y) {
            dismiss(&app);
            return;
        }
        for delay in [50_u64, 200] {
            let later = app.clone();
            let when =
                DispatchTime::try_from(Duration::from_millis(delay)).unwrap_or(DispatchTime::NOW);
            let _ = DispatchQueue::main().after(when, move || {
                let Some(window) = later.get_webview_window(PANEL_LABEL) else {
                    return;
                };
                let Some(ns_window) = retained_window(&window) else {
                    return;
                };
                clear_panel_surface(&ns_window);
            });
        }
    });
}

#[cfg(target_os = "macos")]
pub fn prepare_panel(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window(PANEL_LABEL).is_some() {
        return Ok(());
    }
    let window = {
        // This panel has to be created before any other window, while the process is an
        // accessory app. A normal window created afterwards cannot enter another app's
        // full-screen space.
        let _overlay = OverlayPolicy::acquire(false);
        build_panel(app)?
    };
    raise_overlay(&window);
    Ok(())
}

fn open_panel(app: &AppHandle, placement: Placement) -> tauri::Result<()> {
    let window = {
        #[cfg(target_os = "macos")]
        let _overlay = OverlayPolicy::acquire(true);
        build_panel(app)?
    };
    reveal(app, &window, placement);
    Ok(())
}

fn build_panel(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    WebviewWindowBuilder::new(
        app,
        PANEL_LABEL,
        WebviewUrl::App("index.html?view=panel".into()),
    )
    .title("Pixo")
    .inner_size(440.0, 560.0)
    .min_inner_size(320.0, 420.0)
    .decorations(false)
    .transparent(true)
    // The card paints its own hard offset shadow in the transparent margin.
    .shadow(false)
    .always_on_top(cfg!(not(target_os = "macos")))
    .skip_taskbar(true)
    .visible_on_all_workspaces(cfg!(not(target_os = "macos")))
    .resizable(true)
    .visible(false)
    .focused(false)
    .center()
    .build()
    .map(|window| {
        #[cfg(target_os = "macos")]
        mark_nonactivating(&window);
        window
    })
}

fn reveal(app: &AppHandle, window: &WebviewWindow, placement: Placement) {
    crate::clipboard::paste::remember_target();
    #[cfg(target_os = "macos")]
    if place_panel(app, window, &placement) {
        focus_search(window, true);
        start_outside_click(app);
        return;
    }
    #[cfg(not(target_os = "macos"))]
    let _ = app;
    match placement {
        Placement::Input => place_near_cursor(window),
        Placement::Status(rect) => place_under_tray(window, rect),
    }
    let _ = window.show();
    let _ = window.set_focus();
    focus_search(window, true);
    start_outside_click(app);
}

fn focus_search(window: &WebviewWindow, reset: bool) {
    let script = if reset {
        "(() => { window.dispatchEvent(new Event('clipboard-panel-shown')); window.focus(); const field = document.getElementById('clipboard-panel-search'); const input = field instanceof HTMLInputElement ? field : field && field.querySelector('input'); if (!(input instanceof HTMLInputElement)) return; input.focus(); })()"
    } else {
        "(() => { window.focus(); const field = document.getElementById('clipboard-panel-search'); const input = field instanceof HTMLInputElement ? field : field && field.querySelector('input'); if (!(input instanceof HTMLInputElement)) return; input.focus(); })()"
    };
    let _ = window.eval(script);
}

fn panel_is_showing(window: &WebviewWindow) -> bool {
    if !window.is_visible().unwrap_or(false) {
        return false;
    }
    #[cfg(target_os = "macos")]
    {
        return retained_window(window).is_some_and(|ns_window| ns_window.isOnActiveSpace());
    }
    #[cfg(not(target_os = "macos"))]
    true
}

#[cfg(target_os = "macos")]
fn place_panel(app: &AppHandle, window: &WebviewWindow, placement: &Placement) -> bool {
    let Some(ns_window) = retained_window(window) else {
        return false;
    };
    let frame = ns_window.frame();
    let width = if frame.size.width < 50.0 {
        440.0
    } else {
        frame.size.width
    };
    let height = if frame.size.height < 50.0 {
        560.0
    } else {
        frame.size.height
    };
    let (x, y) = match placement {
        Placement::Input => {
            let anchor = crate::panel_anchor::focused_input();
            let screen = crate::panel_anchor::screen_containing(anchor);
            crate::panel_anchor::place_beside(anchor, width, height, screen)
        }
        Placement::Status(rect) => {
            let _ = rect;
            let pointer = crate::panel_anchor::pointer();
            let area = crate::panel_anchor::work_area(pointer);
            crate::panel_anchor::place_under_bar(
                pointer.x + pointer.width / 2.0,
                width,
                height,
                area,
            )
        }
    };
    raise_overlay(window);
    ns_window.setFrame_display(
        NSRect::new(NSPoint::new(x, y), NSSize::new(width, height)),
        false,
    );
    keep_on_top(&ns_window);
    take_keyboard(app, window, &ns_window);
    true
}

#[cfg(target_os = "macos")]
fn raise_overlay(window: &WebviewWindow) {
    let Some(ns_window) = retained_window(window) else {
        return;
    };
    // CanJoinAllSpaces keeps the panel on the user's current space. MoveToActiveSpace
    // instead pulls that space over to the window and leaves fullscreen.
    ns_window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Transient
            | NSWindowCollectionBehavior::IgnoresCycle
            | NSWindowCollectionBehavior::FullScreenDisallowsTiling,
    );
    // Floating sits above the app in front, and stays under screenshot UI and the menu bar.
    ns_window.setLevel(NSFloatingWindowLevel);
    ns_window.setAnimationBehavior(NSWindowAnimationBehavior::None);
    ns_window.setHidesOnDeactivate(false);
}

#[cfg(target_os = "macos")]
static MADE_ACTIVE: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "macos")]
fn keep_on_top(ns_window: &NSWindow) {
    ns_window.setLevel(NSFloatingWindowLevel);
    ns_window.orderFrontRegardless();
}

#[cfg(target_os = "macos")]
fn take_keyboard(app: &AppHandle, window: &WebviewWindow, ns_window: &NSWindow) {
    let Some(marker) = MainThreadMarker::new() else {
        return;
    };
    let application = NSApplication::sharedApplication(marker);
    // Making Pixo the frontmost app switches the menu bar away from the app the
    // user is in. Marking this process active is enough for the panel to become key.
    if !application.isActive() && responds(&application, sel!(setIsActive:)) {
        let _: () = unsafe { msg_send![&*application, setIsActive: true] };
        MADE_ACTIVE.store(true, Ordering::SeqCst);
    }
    keep_on_top(ns_window);
    clear_panel_surface(ns_window);
    ns_window.makeKeyWindow();
    focus_webview(ns_window);
    focus_search(window, true);
    start_key_capture(app);
    let later = window.clone();
    let when = DispatchTime::try_from(Duration::from_millis(50)).unwrap_or(DispatchTime::NOW);
    let _ = DispatchQueue::main().after(when, move || refocus(&later));
}

#[cfg(target_os = "macos")]
pub fn click_modifiers() -> (bool, bool) {
    let Some(class) = AnyClass::get(c"NSEvent") else {
        return (false, false);
    };
    let flags: usize = unsafe { msg_send![class, modifierFlags] };
    (flags & (1 << 19) != 0, flags & (1 << 17) != 0)
}

#[cfg(target_os = "macos")]
fn resign_keyboard() {
    if !MADE_ACTIVE.swap(false, Ordering::SeqCst) {
        return;
    }
    let Some(marker) = MainThreadMarker::new() else {
        return;
    };
    let application = NSApplication::sharedApplication(marker);
    if responds(&application, sel!(setIsActive:)) {
        let _: () = unsafe { msg_send![&*application, setIsActive: false] };
    }
}

#[cfg(target_os = "macos")]
fn responds(application: &NSApplication, selector: objc2::runtime::Sel) -> bool {
    unsafe { msg_send![application, respondsToSelector: selector] }
}

#[cfg(target_os = "macos")]
fn refocus(window: &WebviewWindow) {
    if let Some(ns_window) = retained_window(window) {
        keep_on_top(&ns_window);
        clear_panel_surface(&ns_window);
        ns_window.makeKeyWindow();
        focus_webview(&ns_window);
    }
    focus_search(window, false);
}

#[cfg(target_os = "macos")]
fn clear_panel_surface(ns_window: &NSWindow) {
    if let Some(color) = clear_color() {
        let _: () = unsafe { msg_send![ns_window, setBackgroundColor: &*color] };
    }
    let _: () = unsafe { msg_send![ns_window, setOpaque: false] };
    if let Some(content) = ns_window.contentView() {
        clear_hosted_page(&content);
    }
}

#[cfg(target_os = "macos")]
fn clear_color() -> Option<Retained<AnyObject>> {
    let class = AnyClass::get(c"NSColor")?;
    let color: *mut AnyObject = unsafe { msg_send![class, clearColor] };
    if color.is_null() {
        None
    } else {
        unsafe { Retained::retain(color) }
    }
}

#[cfg(target_os = "macos")]
fn clear_hosted_page(view: &NSView) {
    let under_page = sel!(setUnderPageBackgroundColor:);
    if responds_view(view, under_page) {
        if let Some(color) = clear_color() {
            let _: () = unsafe { msg_send![view, setUnderPageBackgroundColor: &*color] };
        }
        let opaque = sel!(setOpaque:);
        if responds_view(view, opaque) {
            let _: () = unsafe { msg_send![view, setOpaque: false] };
        }
    }
    if responds_view(view, sel!(setDrawsBackground:)) {
        let _: () = unsafe { msg_send![view, setDrawsBackground: false] };
    }
    let subviews = view.subviews();
    for index in 0..subviews.count() {
        clear_hosted_page(&subviews.objectAtIndex(index));
    }
}

#[cfg(target_os = "macos")]
fn responds_view(view: &NSView, selector: objc2::runtime::Sel) -> bool {
    unsafe { msg_send![view, respondsToSelector: selector] }
}

#[cfg(target_os = "macos")]
struct KeyTap {
    port: *mut c_void,
    source: *mut c_void,
    app: AppHandle,
}

#[cfg(target_os = "macos")]
unsafe impl Send for KeyTap {}

#[cfg(target_os = "macos")]
struct CopiedEvent(*mut c_void);

#[cfg(target_os = "macos")]
unsafe impl Send for CopiedEvent {}

#[cfg(target_os = "macos")]
static KEY_TAP: Mutex<Option<KeyTap>> = Mutex::new(None);

#[cfg(target_os = "macos")]
fn start_key_capture(app: &AppHandle) {
    remember_panel_hotkey(app);
    let mut tap = match KEY_TAP.lock() {
        Ok(tap) => tap,
        Err(poisoned) => poisoned.into_inner(),
    };
    if tap.is_some() {
        return;
    }
    let port = unsafe {
        CGEventTapCreate(
            1,
            0,
            0,
            (1 << 10) | (1 << 11) | (1 << 12),
            key_tap_callback,
            std::ptr::null_mut(),
        )
    };
    if port.is_null() {
        eprintln!(
            "clipboard panel: keyboard capture is unavailable until accessibility access is allowed"
        );
        return;
    }
    let source = unsafe { CFMachPortCreateRunLoopSource(std::ptr::null_mut(), port, 0) };
    if source.is_null() {
        unsafe { CFRelease(port) };
        return;
    }
    unsafe {
        CFRunLoopAddSource(CFRunLoopGetMain(), source, kCFRunLoopCommonModes);
        CGEventTapEnable(port, true);
    }
    *tap = Some(KeyTap {
        port,
        source,
        app: app.clone(),
    });
}

#[cfg(target_os = "macos")]
fn stop_key_capture() {
    let tap = {
        let mut guard = match KEY_TAP.lock() {
            Ok(tap) => tap,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.take()
    };
    let Some(tap) = tap else {
        return;
    };
    unsafe {
        CGEventTapEnable(tap.port, false);
        CFRunLoopRemoveSource(CFRunLoopGetMain(), tap.source, kCFRunLoopCommonModes);
        CFRelease(tap.source);
        CFRelease(tap.port);
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn key_tap_callback(
    _proxy: *mut c_void,
    event_type: u32,
    event: *mut c_void,
    _user: *mut c_void,
) -> *mut c_void {
    if event_type == 0xFFFF_FFFE || event_type == 0xFFFF_FFFF {
        if let Some(port) = key_tap_port() {
            unsafe { CGEventTapEnable(port, true) };
        }
        return event;
    }
    if event.is_null() {
        return event;
    }
    let keycode = unsafe { CGEventGetIntegerValueField(event, 9) };
    let flags = unsafe { CGEventGetFlags(event) };
    let command = flags & 0x0010_0000 != 0;
    let shift = flags & 0x0002_0000 != 0;
    let option = flags & 0x0008_0000 != 0;
    let control = flags & 0x0004_0000 != 0;
    // The panel owns its shortcut while it is open. Carbon does not receive the
    // chord once another app is key, so the tap closes the panel itself.
    if matches_panel_hotkey(keycode, command, shift, option, control)
        && matches!(event_type, 10 | 11)
    {
        if event_type == 10
            && let Some(app) = key_tap_app()
        {
            let _ = DispatchQueue::main().exec_async(move || dismiss(&app));
        }
        return std::ptr::null_mut();
    }
    // Leave system screenshot shortcuts for macOS.
    if releases_to_system(event_type, keycode, command, shift) {
        return event;
    }
    if event_type == 10 && keycode == 53 {
        if let Some(app) = key_tap_app() {
            let _ = DispatchQueue::main().exec_async(move || dismiss(&app));
        }
        return std::ptr::null_mut();
    }
    let copied = unsafe { CGEventCreateCopy(event) };
    if copied.is_null() {
        return event;
    }
    let copied = CopiedEvent(copied);
    let _ = DispatchQueue::main().exec_async(move || deliver_key(copied));
    std::ptr::null_mut()
}

#[cfg(target_os = "macos")]
fn key_tap_port() -> Option<*mut c_void> {
    let guard = KEY_TAP
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.as_ref().map(|tap| tap.port)
}

#[cfg(target_os = "macos")]
fn key_tap_app() -> Option<AppHandle> {
    let guard = KEY_TAP
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.as_ref().map(|tap| tap.app.clone())
}

#[cfg(target_os = "macos")]
fn deliver_key(event: CopiedEvent) {
    let cg_event = event.0;
    if cg_event.is_null() {
        return;
    }
    let delivered = key_tap_app().and_then(|app| {
        let window = app.get_webview_window(PANEL_LABEL)?;
        let ns_window = retained_window(&window)?;
        let class = AnyClass::get(c"NSEvent")?;
        let cg_event = cg_event.cast::<objc2::runtime::AnyObject>();
        let ns_event: *mut objc2::runtime::AnyObject =
            unsafe { msg_send![class, eventWithCGEvent: &*cg_event] };
        if ns_event.is_null() {
            return None;
        }
        keep_on_top(&ns_window);
        if !ns_window.isKeyWindow() {
            ns_window.makeKeyWindow();
            focus_webview(&ns_window);
        }
        let _: () = unsafe { msg_send![&*ns_window, sendEvent: &*ns_event] };
        Some(())
    });
    unsafe { CFRelease(cg_event) };
    let _ = delivered;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: unsafe extern "C" fn(*mut c_void, u32, *mut c_void, *mut c_void) -> *mut c_void,
        user_info: *mut c_void,
    ) -> *mut c_void;
    fn CGEventTapEnable(tap: *mut c_void, enable: bool);
    fn CGEventGetIntegerValueField(event: *mut c_void, field: u32) -> i64;
    fn CGEventGetFlags(event: *mut c_void) -> u64;
    fn CGEventCreateCopy(event: *mut c_void) -> *mut c_void;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFRunLoopCommonModes: *const c_void;
    fn CFRunLoopGetMain() -> *mut c_void;
    fn CFMachPortCreateRunLoopSource(
        allocator: *mut c_void,
        port: *mut c_void,
        order: isize,
    ) -> *mut c_void;
    fn CFRunLoopAddSource(run_loop: *mut c_void, source: *mut c_void, mode: *const c_void);
    fn CFRunLoopRemoveSource(run_loop: *mut c_void, source: *mut c_void, mode: *const c_void);
    fn CFRelease(value: *const c_void);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PanelHotkey {
    keycode: i64,
    command: bool,
    shift: bool,
    option: bool,
    control: bool,
}

#[cfg(target_os = "macos")]
static PANEL_HOTKEY: Mutex<Option<PanelHotkey>> = Mutex::new(None);

#[cfg(target_os = "macos")]
fn remember_panel_hotkey(app: &AppHandle) {
    let shortcut = app
        .try_state::<crate::clipboard::ClipboardState>()
        .map(|state| state.shortcut_bindings()[0].clone())
        .unwrap_or_else(|| "CommandOrControl+Shift+V".to_string());
    let hotkey =
        parse_panel_hotkey(&shortcut).or_else(|| parse_panel_hotkey("CommandOrControl+Shift+V"));
    let mut guard = match PANEL_HOTKEY.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    *guard = hotkey;
}

#[cfg(target_os = "macos")]
fn matches_panel_hotkey(
    keycode: i64,
    command: bool,
    shift: bool,
    option: bool,
    control: bool,
) -> bool {
    let guard = match PANEL_HOTKEY.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard
        .as_ref()
        .is_some_and(|hotkey| hotkey_matches(*hotkey, keycode, command, shift, option, control))
}

fn hotkey_matches(
    hotkey: PanelHotkey,
    keycode: i64,
    command: bool,
    shift: bool,
    option: bool,
    control: bool,
) -> bool {
    hotkey.keycode == keycode
        && hotkey.command == command
        && hotkey.shift == shift
        && hotkey.option == option
        && hotkey.control == control
}

fn parse_panel_hotkey(shortcut: &str) -> Option<PanelHotkey> {
    let mut command = false;
    let mut shift = false;
    let mut option = false;
    let mut control = false;
    let mut keycode = None;
    for part in shortcut
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        match part.to_ascii_lowercase().as_str() {
            "commandorcontrol" | "commandorctrl" | "cmdorctrl" | "cmdorcontrol" | "command"
            | "cmd" => {
                command = true;
            }
            "control" | "ctrl" => control = true,
            "alt" | "option" => option = true,
            "shift" => shift = true,
            key => keycode = mac_keycode(key),
        }
    }
    Some(PanelHotkey {
        keycode: keycode?,
        command,
        shift,
        option,
        control,
    })
}

fn mac_keycode(key: &str) -> Option<i64> {
    const LETTERS: &[(char, i64)] = &[
        ('a', 0),
        ('s', 1),
        ('d', 2),
        ('f', 3),
        ('h', 4),
        ('g', 5),
        ('z', 6),
        ('x', 7),
        ('c', 8),
        ('v', 9),
        ('b', 11),
        ('q', 12),
        ('w', 13),
        ('e', 14),
        ('r', 15),
        ('y', 16),
        ('t', 17),
        ('o', 31),
        ('u', 32),
        ('i', 34),
        ('p', 35),
        ('l', 37),
        ('j', 38),
        ('k', 40),
        ('n', 45),
        ('m', 46),
    ];
    const DIGITS: &[(char, i64)] = &[
        ('1', 18),
        ('2', 19),
        ('3', 20),
        ('4', 21),
        ('5', 23),
        ('6', 22),
        ('7', 26),
        ('8', 28),
        ('9', 25),
        ('0', 29),
    ];
    let key = key.to_ascii_lowercase();
    if key.len() == 1 {
        let key = key.chars().next()?;
        return LETTERS
            .iter()
            .chain(DIGITS.iter())
            .find(|(symbol, _)| *symbol == key)
            .map(|(_, code)| *code);
    }
    Some(match key.as_str() {
        "minus" => 27,
        "equal" => 24,
        "bracketleft" => 33,
        "bracketright" => 30,
        "backslash" => 42,
        "semicolon" => 41,
        "quote" => 39,
        "comma" => 43,
        "period" => 47,
        "slash" => 44,
        "space" => 49,
        "tab" => 48,
        "enter" | "return" => 36,
        "backspace" | "delete" => 51,
        "escape" | "esc" => 53,
        "left" | "arrowleft" => 123,
        "right" | "arrowright" => 124,
        "down" | "arrowdown" => 125,
        "up" | "arrowup" => 126,
        "f1" => 122,
        "f2" => 120,
        "f3" => 99,
        "f4" => 118,
        "f5" => 96,
        "f6" => 97,
        "f7" => 98,
        "f8" => 100,
        "f9" => 101,
        "f10" => 109,
        "f11" => 103,
        "f12" => 111,
        _ => return None,
    })
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn releases_to_system(event_type: u32, keycode: i64, command: bool, shift: bool) -> bool {
    if !command || !shift {
        return false;
    }
    // Modifier changes stay with the system so the other app does not see a stuck Shift.
    if event_type == 12 {
        return true;
    }
    // Command-Shift-3, 4, and 5 start a screenshot. Control may also be held.
    matches!(keycode, 20 | 21 | 23)
}

#[cfg(target_os = "macos")]
fn focus_webview(window: &NSWindow) {
    let Some(content) = window.contentView() else {
        return;
    };
    let target = best_responder(&content);
    let responder: &NSResponder = &target;
    let _ = window.makeFirstResponder(Some(responder));
}

#[cfg(target_os = "macos")]
fn best_responder(view: &NSView) -> Retained<NSView> {
    let mut best = view.retain();
    let mut best_score = responder_score(view);
    let subviews = view.subviews();
    for index in 0..subviews.count() {
        let child = best_responder(&subviews.objectAtIndex(index));
        let child_score = responder_score(&child);
        if child_score > best_score {
            best_score = child_score;
            best = child;
        }
    }
    best
}

#[cfg(target_os = "macos")]
fn responder_score(view: &NSView) -> i32 {
    let name = view.class().name().to_bytes();
    let mut score = 0;
    if contains_bytes(name, b"WKContentView") {
        score += 100;
    } else if contains_bytes(name, b"WebView") {
        score += 40;
    }
    if view.acceptsFirstResponder() {
        score += 10;
    }
    score
}

#[cfg(target_os = "macos")]
fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|part| part == needle)
}

#[cfg(target_os = "macos")]
fn mark_nonactivating(window: &WebviewWindow) {
    let Some(ns_window) = retained_window(window) else {
        return;
    };
    let mask = ns_window.styleMask();
    if !mask.contains(NSWindowStyleMask::NonactivatingPanel) {
        ns_window.setStyleMask(mask | NSWindowStyleMask::NonactivatingPanel);
    }
}

#[cfg(target_os = "macos")]
fn retained_window(window: &WebviewWindow) -> Option<Retained<NSWindow>> {
    let raw = window.ns_window().ok()?;
    unsafe { Retained::retain(raw.cast::<NSWindow>()) }
}

#[cfg(target_os = "macos")]
struct OverlayPolicy {
    app: Retained<NSApplication>,
    previous: isize,
    was_active: bool,
    restore_focus: bool,
}

#[cfg(target_os = "macos")]
impl OverlayPolicy {
    fn acquire(restore_focus: bool) -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        let app = NSApplication::sharedApplication(mtm);
        let previous = app.activationPolicy().0;
        let was_active = app.isActive();
        if previous != NSApplicationActivationPolicy::Accessory.0
            && !app.setActivationPolicy(NSApplicationActivationPolicy::Accessory)
        {
            eprintln!("clipboard panel: accessory activation policy was rejected");
        }
        Some(Self {
            app,
            previous,
            was_active,
            restore_focus,
        })
    }
}

#[cfg(target_os = "macos")]
impl Drop for OverlayPolicy {
    fn drop(&mut self) {
        if self.previous != NSApplicationActivationPolicy::Accessory.0 {
            let _ = self
                .app
                .setActivationPolicy(NSApplicationActivationPolicy(self.previous));
        }
        if self.restore_focus && !self.was_active && self.app.isActive() {
            self.app.deactivate();
        }
    }
}

fn place_under_tray(window: &WebviewWindow, rect: tauri::Rect) {
    let fallback_scale = window.scale_factor().unwrap_or(1.0);
    let physical = rect.position.to_physical::<f64>(fallback_scale);
    let physical_size = rect.size.to_physical::<f64>(fallback_scale);
    if physical_size.width < 1.0 || physical_size.height < 1.0 {
        place_near_cursor(window);
        return;
    }
    let center_x = physical.x + physical_size.width / 2.0;
    let center_y = physical.y + physical_size.height / 2.0;
    let monitors = window.available_monitors().unwrap_or_default();
    let monitor = monitors.iter().find(|monitor| {
        let pos = monitor.position();
        let area = monitor.size();
        center_x >= f64::from(pos.x)
            && center_y >= f64::from(pos.y)
            && center_x < f64::from(pos.x) + f64::from(area.width)
            && center_y < f64::from(pos.y) + f64::from(area.height)
    });
    let scale = monitor
        .map(|monitor| monitor.scale_factor())
        .unwrap_or(fallback_scale);
    let screen = monitor.map(|monitor| {
        let pos = monitor.position();
        let area = monitor.size();
        crate::panel_anchor::Rect {
            x: f64::from(pos.x) / scale,
            y: f64::from(pos.y) / scale,
            width: f64::from(area.width) / scale,
            height: f64::from(area.height) / scale,
        }
    });
    let Some(screen) = screen else {
        place_near_cursor(window);
        return;
    };
    let Ok(size) = window.outer_size() else {
        return;
    };
    let (x, y) = crate::panel_anchor::place_under_icon(
        crate::panel_anchor::Rect {
            x: physical.x / scale,
            y: physical.y / scale,
            width: physical_size.width / scale,
            height: physical_size.height / scale,
        },
        f64::from(size.width) / scale,
        f64::from(size.height) / scale,
        screen,
    );
    let _ = window.set_position(tauri::LogicalPosition::new(x, y));
}

fn place_near_cursor(window: &WebviewWindow) {
    #[cfg(target_os = "windows")]
    if let Some(anchor) = crate::panel_anchor::input_anchor() {
        place_near_point(
            window,
            anchor.x + anchor.width,
            anchor.y + anchor.height / 2.0,
        );
        return;
    }
    let Ok(cursor) = window.cursor_position() else {
        return;
    };
    place_near_point(window, cursor.x, cursor.y);
}

fn place_near_point(window: &WebviewWindow, origin_x: f64, origin_y: f64) {
    let Ok(size) = window.outer_size() else {
        return;
    };
    let mut x = origin_x + 16.0;
    let mut y = origin_y + 16.0;
    let Ok(monitors) = window.available_monitors() else {
        let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
        return;
    };
    if let Some(monitor) = monitors.iter().find(|monitor| {
        let pos = monitor.position();
        let area = monitor.size();
        let left = f64::from(pos.x);
        let top = f64::from(pos.y);
        origin_x >= left
            && origin_y >= top
            && origin_x < left + f64::from(area.width)
            && origin_y < top + f64::from(area.height)
    }) {
        let pos = monitor.position();
        let area = monitor.size();
        let left = f64::from(pos.x);
        let top = f64::from(pos.y);
        let max_x = left + f64::from(area.width) - f64::from(size.width);
        let max_y = top + f64::from(area.height) - f64::from(size.height);
        x = x.clamp(left, max_x.max(left));
        y = y.clamp(top, max_y.max(top));
    }
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
}

#[cfg(test)]
mod tests {
    #[test]
    fn screenshot_shortcuts_reach_the_system() {
        assert!(super::releases_to_system(10, 20, true, true));
        assert!(super::releases_to_system(10, 21, true, true));
        assert!(super::releases_to_system(11, 23, true, true));
        assert!(super::releases_to_system(12, 0, true, true));
        assert!(!super::releases_to_system(10, 9, true, true));
        assert!(!super::releases_to_system(10, 20, true, false));
        assert!(!super::releases_to_system(10, 0, true, true));
        assert!(!super::releases_to_system(10, 53, false, false));
    }

    #[test]
    fn outside_click_closes_except_on_the_panel_and_status_item() {
        let panel = super::ScreenBox {
            x: 100.0,
            y: 200.0,
            width: 440.0,
            height: 560.0,
        };
        let status = super::ScreenBox {
            x: 800.0,
            y: 900.0,
            width: 28.0,
            height: 24.0,
        };
        assert!(!super::outside_click_dismisses(
            panel,
            Some(status),
            100.0,
            200.0
        ));
        assert!(!super::outside_click_dismisses(
            panel,
            Some(status),
            320.0,
            480.0
        ));
        assert!(!super::outside_click_dismisses(
            panel,
            Some(status),
            540.0,
            760.0
        ));
        assert!(!super::outside_click_dismisses(
            panel,
            Some(status),
            810.0,
            910.0
        ));
        assert!(super::outside_click_dismisses(
            panel,
            Some(status),
            99.0,
            400.0
        ));
        assert!(super::outside_click_dismisses(
            panel,
            Some(status),
            10.0,
            10.0
        ));
        assert!(super::outside_click_dismisses(panel, None, 810.0, 910.0));
    }

    #[test]
    fn open_panel_shortcut_closes_it_from_the_keyboard_tap() {
        let hotkey = super::parse_panel_hotkey("CommandOrControl+Shift+V").unwrap();
        assert!(super::hotkey_matches(hotkey, 9, true, true, false, false));
        assert!(!super::hotkey_matches(hotkey, 9, true, true, true, false));
        assert!(!super::hotkey_matches(hotkey, 0, true, true, false, false));
        let custom = super::parse_panel_hotkey("Alt+Shift+P").unwrap();
        assert!(super::hotkey_matches(custom, 35, false, true, true, false));
    }

    #[test]
    fn selection_shortcuts_parse_without_registration() {
        super::parse_shortcut("CommandOrControl+P").unwrap();
        super::parse_shortcut("CommandOrControl+Backspace").unwrap();
    }
}
