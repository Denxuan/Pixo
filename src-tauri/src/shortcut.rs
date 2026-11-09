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
use dispatch2::{DispatchQueue, DispatchTime};
#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2::runtime::AnyClass;
#[cfg(target_os = "macos")]
use objc2::{MainThreadMarker, Message, msg_send, sel};
#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSFloatingWindowLevel, NSResponder, NSView,
    NSWindow, NSWindowAnimationBehavior, NSWindowCollectionBehavior, NSWindowStyleMask,
};
#[cfg(target_os = "macos")]
use objc2_foundation::{NSPoint, NSRect, NSSize};

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

pub fn show_from_status(app: &AppHandle, rect: tauri::Rect) {
    let app = app.clone();
    let _ = app
        .clone()
        .run_on_main_thread(move || present(&app, false, Placement::Status(rect)));
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
        if registered.is_some() {
            let _ = app.global_shortcut().unregister_all();
            *registered = None;
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

#[tauri::command]
pub fn clipboard_dismiss_panel(app: AppHandle) {
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || dismiss(&app));
}

fn dismiss(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    resign_keyboard();
    hide_panel(app);
}

fn hide_panel(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    stop_key_capture();
    if let Some(window) = app.get_webview_window(PANEL_LABEL) {
        let _ = window.hide();
    }
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
    .shadow(true)
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
    #[cfg(target_os = "macos")]
    if place_panel(app, window, &placement) {
        focus_search(window);
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
    focus_search(window);
}

fn focus_search(window: &WebviewWindow) {
    let _ = window.eval(
        "(() => { window.focus(); const field = document.getElementById('clipboard-panel-search'); const input = field instanceof HTMLInputElement ? field : field && field.querySelector('input'); if (!(input instanceof HTMLInputElement)) return; input.focus(); const end = input.value.length; input.setSelectionRange(end, end); })()",
    );
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
    ns_window.makeKeyWindow();
    focus_webview(ns_window);
    focus_search(window);
    start_key_capture(app);
    let later = window.clone();
    let when = DispatchTime::try_from(Duration::from_millis(50)).unwrap_or(DispatchTime::NOW);
    let _ = DispatchQueue::main().after(when, move || refocus(&later));
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
        ns_window.makeKeyWindow();
        focus_webview(&ns_window);
    }
    focus_search(window);
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
    // Leave the panel toggle and system screenshot shortcuts for macOS.
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

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn releases_to_system(event_type: u32, keycode: i64, command: bool, shift: bool) -> bool {
    if !command || !shift {
        return false;
    }
    // Command-Shift-V toggles the panel. The modifier event has to pass through too.
    if event_type == 12 || keycode == 9 {
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
    let Ok(cursor) = window.cursor_position() else {
        return;
    };
    let Ok(size) = window.outer_size() else {
        return;
    };
    let mut x = cursor.x + 16.0;
    let mut y = cursor.y + 16.0;
    let Ok(monitors) = window.available_monitors() else {
        let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
        return;
    };
    if let Some(monitor) = monitors.iter().find(|monitor| {
        let pos = monitor.position();
        let area = monitor.size();
        let left = f64::from(pos.x);
        let top = f64::from(pos.y);
        cursor.x >= left
            && cursor.y >= top
            && cursor.x < left + f64::from(area.width)
            && cursor.y < top + f64::from(area.height)
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
        assert!(super::releases_to_system(10, 9, true, true));
        assert!(super::releases_to_system(12, 0, true, true));
        assert!(!super::releases_to_system(10, 20, true, false));
        assert!(!super::releases_to_system(10, 0, true, true));
        assert!(!super::releases_to_system(10, 53, false, false));
    }
}
