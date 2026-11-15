use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

static PINNED_OVERLAYS: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Capture overlays stay off-screen for the whole scroll, including the result preview.
static CAPTURE_PARKED: AtomicBool = AtomicBool::new(false);
/// AppKit frames saved before a scroll parks the capture windows off-screen.
#[cfg(target_os = "macos")]
static PARKED_FRAMES: Mutex<Vec<(String, f64, f64, f64, f64)>> = Mutex::new(Vec::new());
/// Desktop frame stashed while the overlay paints off-screen.
#[cfg(target_os = "macos")]
static PENDING_FRAMES: Mutex<Vec<(String, f64, f64, f64, f64)>> = Mutex::new(Vec::new());

/// Far enough left that a full-size overlay is not on any display.
#[cfg(target_os = "macos")]
const OFFSCREEN_X: f64 = -20_000.0;

/// The frame covering the desktop. An off-screen or tiny window is only a
/// parking spot, not a place to put the overlay back.
fn capture_home_frame(x: f64, y: f64, width: f64, height: f64) -> Option<(f64, f64, f64, f64)> {
    if width > 32.0 && height > 32.0 && x > -8_000.0 {
        Some((x, y, width, height))
    } else {
        None
    }
}

fn is_pinned(label: &str) -> bool {
    PINNED_OVERLAYS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .any(|item| item == label)
}

fn remember_pin(label: &str) {
    let mut pinned = PINNED_OVERLAYS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if pinned.iter().any(|item| item == label) {
        return;
    }
    pinned.push(label.to_string());
}

fn forget_pin(label: &str) {
    PINNED_OVERLAYS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .retain(|item| item != label);
}

fn logical_span(pixels: u32, scale: f64) -> f64 {
    f64::from(pixels) / scale.max(1.0)
}

fn logical_x(pixels: i32, scale: f64) -> f64 {
    f64::from(pixels) / scale.max(1.0)
}

fn logical_y(pixels: i32, scale: f64) -> f64 {
    f64::from(pixels) / scale.max(1.0)
}

/// AppKit frames use a bottom-left origin on the main display. `top` is the
/// top-left coordinate shared with the captured image.
#[cfg(any(target_os = "macos", test))]
fn appkit_origin_y(top: f64, height: f64, main_height: f64) -> f64 {
    main_height - top - height
}

use super::error::ScreenshotError;

pub fn warm_overlay(
    app: &AppHandle,
    slot: usize,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale: f64,
) -> Result<(), ScreenshotError> {
    let label = overlay_label(slot);
    if app.get_webview_window(&label).is_some() {
        return Ok(());
    }
    create_overlay(app, &label, x, y, width, height, scale, None)
}

pub fn open_overlay(
    app: &AppHandle,
    slot: usize,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale: f64,
) -> Result<String, ScreenshotError> {
    let preferred = overlay_label(slot);
    if !is_pinned(&preferred) {
        return open_named_overlay(app, &preferred, slot, x, y, width, height, scale, false);
    }
    // The monitor's overlay is already a pin. A spare window takes the next shot.
    let spare = spare_overlay_label(app, slot);
    open_named_overlay(app, &spare, slot, x, y, width, height, scale, true)
}

fn open_named_overlay(
    app: &AppHandle,
    label: &str,
    slot: usize,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale: f64,
    monitor_query: bool,
) -> Result<String, ScreenshotError> {
    let left = logical_x(x, scale);
    let top = logical_y(y, scale);
    let frame_width = logical_span(width, scale);
    let frame_height = logical_span(height, scale);
    if let Some(existing) = app.get_webview_window(label) {
        if !is_pinned(label) {
            position_overlay(&existing, left, top, frame_width, frame_height);
            conceal(&existing);
            return Ok(label.to_string());
        }
    }
    create_overlay(
        app,
        label,
        x,
        y,
        width,
        height,
        scale,
        monitor_query.then_some(slot),
    )?;
    Ok(label.to_string())
}

fn spare_overlay_label(app: &AppHandle, slot: usize) -> String {
    for generation in 1..32 {
        let label = format!("screenshot-overlay-m{slot}-{generation}");
        if app.get_webview_window(&label).is_none() || !is_pinned(&label) {
            return label;
        }
    }
    format!("screenshot-overlay-m{slot}-overflow")
}

fn overlay_label(slot: usize) -> String {
    format!("screenshot-overlay-{}", slot + 1)
}

fn create_overlay(
    app: &AppHandle,
    label: &str,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale: f64,
    monitor: Option<usize>,
) -> Result<(), ScreenshotError> {
    let _policy = OverlayPolicy::acquire();
    let left = logical_x(x, scale);
    let top = logical_y(y, scale);
    let frame_width = logical_span(width, scale);
    let frame_height = logical_span(height, scale);
    let url = match monitor {
        Some(index) => format!("index.html?view=shot&monitor={index}"),
        None => "index.html?view=shot".to_string(),
    };
    let window = WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
        .title("Pixo")
        .decorations(false)
        .transparent(false)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible(false)
        .focused(false)
        .position(left, top)
        .inner_size(frame_width, frame_height)
        .background_color(tauri::window::Color(0, 0, 0, 255))
        .build()
        .map_err(|error| {
            eprintln!("screenshot overlay: {error}");
            ScreenshotError::new("capture_failed")
        })?;
    position_overlay(&window, left, top, frame_width, frame_height);
    conceal(&window);
    Ok(())
}

fn position_overlay(window: &tauri::WebviewWindow, left: f64, top: f64, width: f64, height: f64) {
    forget_parked_frame(&window.label());
    raise_overlay(window, left, top, width, height);
    #[cfg(not(target_os = "macos"))]
    {
        use tauri::{LogicalPosition, LogicalSize};
        let _ = window.set_position(LogicalPosition::new(left, top));
        let _ = window.set_size(LogicalSize::new(width, height));
    }
}

pub fn arm_labels(app: &AppHandle, labels: &[String]) {
    for label in labels {
        if is_pinned(label) {
            continue;
        }
        if let Some(window) = app.get_webview_window(label) {
            arm(&window);
        }
    }
}

pub fn conceal_overlays(app: &AppHandle) {
    CAPTURE_PARKED.store(false, Ordering::Release);
    clear_parked_frames();
    for (label, window) in app.webview_windows() {
        if label.starts_with("screenshot-overlay-") && !is_pinned(&label) {
            conceal(&window);
        }
        if label.starts_with("screenshot-scroll") {
            #[cfg(target_os = "macos")]
            dismiss_overlay(&window);
            #[cfg(not(target_os = "macos"))]
            conceal(&window);
        }
    }
}

fn hide_capture_overlays(app: &AppHandle) {
    CAPTURE_PARKED.store(true, Ordering::Release);
    for (label, window) in app.webview_windows() {
        if label.starts_with("screenshot-overlay-") && !is_pinned(&label) {
            #[cfg(target_os = "macos")]
            park_capture_window(&window);
            #[cfg(not(target_os = "macos"))]
            conceal(&window);
        }
    }
}

fn forget_parked_frame(label: &str) {
    #[cfg(target_os = "macos")]
    {
        PARKED_FRAMES
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .retain(|(name, _, _, _, _)| name != label);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = label;
    }
}

fn clear_parked_frames() {
    #[cfg(target_os = "macos")]
    PARKED_FRAMES
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clear();
}

#[cfg(target_os = "macos")]
fn remember_pending_frame(label: &str, frame: (f64, f64, f64, f64)) {
    let mut frames = PENDING_FRAMES
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Some(existing) = frames.iter_mut().find(|(name, _, _, _, _)| name == label) {
        *existing = (label.to_string(), frame.0, frame.1, frame.2, frame.3);
        return;
    }
    frames.push((label.to_string(), frame.0, frame.1, frame.2, frame.3));
}

#[cfg(target_os = "macos")]
fn take_pending_frame(label: &str) -> Option<(f64, f64, f64, f64)> {
    let mut frames = PENDING_FRAMES
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let index = frames.iter().position(|(name, _, _, _, _)| name == label)?;
    let (_, x, y, width, height) = frames.remove(index);
    Some((x, y, width, height))
}

fn forget_pending_frame(label: &str) {
    #[cfg(target_os = "macos")]
    {
        PENDING_FRAMES
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .retain(|(name, _, _, _, _)| name != label);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = label;
    }
}

/// Move the opaque capture window off the desktop. orderOut alone can lose
/// to a later show, and alpha 0 on this window is a black sheet.
#[cfg(target_os = "macos")]
fn park_capture_window(window: &tauri::WebviewWindow) {
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    let Some(ns_window) = retained(window) else {
        return;
    };
    let frame = ns_window.frame();
    let label = window.label();
    let home = capture_home_frame(
        frame.origin.x,
        frame.origin.y,
        frame.size.width,
        frame.size.height,
    )
    .or_else(|| take_pending_frame(&label));
    if let Some(home) = home
        && let Some((x, y, width, height)) = capture_home_frame(home.0, home.1, home.2, home.3)
    {
        let mut frames = PARKED_FRAMES
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(existing) = frames.iter_mut().find(|(name, _, _, _, _)| name == &label) {
            *existing = (label.to_string(), x, y, width, height);
        } else {
            frames.push((label.to_string(), x, y, width, height));
        }
    }
    ns_window.setAlphaValue(1.0);
    ns_window.setSharingType(objc2_app_kit::NSWindowSharingType::None);
    ns_window.setIgnoresMouseEvents(true);
    ns_window.setContentMinSize(NSSize::new(1.0, 1.0));
    ns_window.setContentMaxSize(NSSize::new(16_000.0, 16_000.0));
    ns_window.setFrame_display(
        NSRect::new(NSPoint::new(-12_000.0, -12_000.0), NSSize::new(1.0, 1.0)),
        false,
    );
    ns_window.orderOut(None);
}

#[cfg(target_os = "macos")]
fn restore_parked_frames(app: &AppHandle) {
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    let frames = std::mem::take(
        &mut *PARKED_FRAMES
            .lock()
            .unwrap_or_else(|error| error.into_inner()),
    );
    for (label, x, y, width, height) in frames {
        let Some(window) = app.get_webview_window(&label) else {
            continue;
        };
        let Some(ns_window) = retained(&window) else {
            continue;
        };
        ns_window.setContentMinSize(NSSize::new(1.0, 1.0));
        ns_window.setContentMaxSize(NSSize::new(width.max(1.0), height.max(1.0)));
        ns_window.setFrame_display(
            NSRect::new(
                NSPoint::new(x, y),
                NSSize::new(width.max(1.0), height.max(1.0)),
            ),
            false,
        );
        ns_window.setContentMaxSize(NSSize::new(16_000.0, 16_000.0));
        ns_window.setAlphaValue(1.0);
    }
}

pub fn reveal_capture_overlays(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let app_for_work = app.clone();
        on_main(app, move || {
            CAPTURE_PARKED.store(false, Ordering::Release);
            restore_parked_frames(&app_for_work);
            for (label, window) in app_for_work.webview_windows() {
                if label.starts_with("screenshot-overlay-") && !is_pinned(&label) {
                    include_in_capture(&window);
                    show_content(&window);
                    order_front(&window);
                    if label == "screenshot-overlay-1" {
                        focus_overlay(&window);
                    }
                }
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    {
        CAPTURE_PARKED.store(false, Ordering::Release);
        for (label, window) in app.webview_windows() {
            if label.starts_with("screenshot-overlay-") && !is_pinned(&label) {
                reveal(&window);
            }
        }
    }
}

const RECORDER_LABEL: &str = "screenshot-recorder";
const SCROLL_LABEL: &str = "screenshot-scroll";
const SCROLL_FRAME_LABELS: [&str; 4] = [
    "screenshot-scroll-frame-top",
    "screenshot-scroll-frame-right",
    "screenshot-scroll-frame-bottom",
    "screenshot-scroll-frame-left",
];

fn recorder_window(
    app: &AppHandle,
    left: f64,
    top: f64,
    width: f64,
    height: f64,
) -> Result<tauri::WebviewWindow, ScreenshotError> {
    if let Some(window) = app.get_webview_window(RECORDER_LABEL) {
        return Ok(window);
    }
    WebviewWindowBuilder::new(
        app,
        RECORDER_LABEL,
        WebviewUrl::App("index.html?view=recorder".into()),
    )
    .title("Pixo")
    .decorations(false)
    .transparent(true)
    .shadow(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .visible(false)
    .focused(false)
    .position(left, top)
    .inner_size(width, height)
    .background_color(tauri::window::Color(0, 0, 0, 0))
    .build()
    .map_err(|error| {
        eprintln!("screenshot recorder: {error}");
        ScreenshotError::new("capture_failed")
    })
}

pub fn open_recorder(
    app: &AppHandle,
    left: f64,
    top: f64,
    width: f64,
    height: f64,
) -> Result<(), ScreenshotError> {
    let window = recorder_window(app, left, top, width, height)?;
    raise_overlay(&window, left, top, width, height);
    #[cfg(not(target_os = "macos"))]
    {
        use tauri::{LogicalPosition, LogicalSize};
        let _ = window.set_position(LogicalPosition::new(left, top));
        let _ = window.set_size(LogicalSize::new(width, height));
    }
    show_content(&window);
    let _ = window.show();
    order_front(&window);
    let _ = window.set_focus();
    Ok(())
}

pub fn close_recorder(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(RECORDER_LABEL) {
        conceal(&window);
    }
}

fn scroll_hud_window(
    app: &AppHandle,
    left: f64,
    top: f64,
    width: f64,
    height: f64,
) -> Result<tauri::WebviewWindow, ScreenshotError> {
    if let Some(window) = app.get_webview_window(SCROLL_LABEL) {
        return Ok(window);
    }
    // Build off the main thread. Tauri creates the window by waiting on the
    // event loop, so doing this inside on_main deadlocks.
    WebviewWindowBuilder::new(
        app,
        SCROLL_LABEL,
        WebviewUrl::App("index.html?view=scroll".into()),
    )
    .title("Pixo")
    .decorations(false)
    .transparent(false)
    .shadow(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .visible(false)
    .focused(false)
    .accept_first_mouse(true)
    .position(left, top)
    .inner_size(width, height)
    .min_inner_size(1.0, 1.0)
    .background_color(tauri::window::Color(28, 28, 28, 255))
    .build()
    .map_err(|error| {
        eprintln!("screenshot scroll hud: {error}");
        ScreenshotError::new("capture_failed")
    })
}

fn scroll_edge_window(
    app: &AppHandle,
    label: &str,
    left: f64,
    top: f64,
    width: f64,
    height: f64,
) -> Result<tauri::WebviewWindow, ScreenshotError> {
    if let Some(window) = app.get_webview_window(label) {
        return Ok(window);
    }
    WebviewWindowBuilder::new(app, label, WebviewUrl::App("record-frame.html".into()))
        .title("Pixo")
        .decorations(false)
        .transparent(false)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible(false)
        .focused(false)
        .position(left, top)
        .inner_size(width, height)
        .min_inner_size(1.0, 1.0)
        .background_color(tauri::window::Color(59, 130, 246, 255))
        .build()
        .map_err(|error| {
            eprintln!("screenshot scroll frame: {error}");
            ScreenshotError::new("capture_failed")
        })
}

fn prepare_scroll_edges(
    app: &AppHandle,
    edges: [Option<(f64, f64, f64, f64)>; 4],
) -> Result<(), ScreenshotError> {
    for (label, edge) in SCROLL_FRAME_LABELS.iter().zip(edges) {
        if let Some((left, top, width, height)) = edge {
            scroll_edge_window(app, label, left, top, width, height)?;
        }
    }
    Ok(())
}

fn place_scroll_edges(app: &AppHandle, edges: [Option<(f64, f64, f64, f64)>; 4]) {
    for (label, edge) in SCROLL_FRAME_LABELS.iter().zip(edges) {
        let Some(window) = app.get_webview_window(label) else {
            continue;
        };
        match edge {
            Some((left, top, width, height)) => {
                raise_overlay(&window, left, top, width, height);
                #[cfg(target_os = "macos")]
                {
                    exclude_from_capture(&window);
                    show_passthrough(&window);
                }
                #[cfg(not(target_os = "macos"))]
                {
                    use tauri::{LogicalPosition, LogicalSize};
                    let _ = window.set_position(LogicalPosition::new(left, top));
                    let _ = window.set_size(LogicalSize::new(width, height));
                    let _ = window.set_ignore_cursor_events(true);
                    let _ = window.show();
                }
                order_front(&window);
            }
            None => {
                #[cfg(target_os = "macos")]
                dismiss_overlay(&window);
                #[cfg(not(target_os = "macos"))]
                conceal(&window);
            }
        }
    }
}

/// Hide the full-screen capture layer, outline the selection, and show the live preview.
/// The capture window is opaque black; leaving it up covers the page being scrolled.
pub fn open_scroll_hud(
    app: &AppHandle,
    left: f64,
    top: f64,
    width: f64,
    height: f64,
    edges: [Option<(f64, f64, f64, f64)>; 4],
) -> Result<(), ScreenshotError> {
    let _window = scroll_hud_window(app, left, top, width, height)?;
    prepare_scroll_edges(app, edges)?;
    #[cfg(target_os = "macos")]
    {
        let app_for_work = app.clone();
        let shown = on_main(app, move || {
            hide_capture_overlays(&app_for_work);
            place_scroll_edges(&app_for_work, edges);
            let Some(window) = app_for_work.get_webview_window(SCROLL_LABEL) else {
                return false;
            };
            raise_overlay(&window, left, top, width, height);
            exclude_from_capture(&window);
            show_content(&window);
            order_front(&window);
            // The previous result grows this window. setFrame can be ignored
            // until the window is on screen, which leaves a near-fullscreen
            // black sheet over the page on the next scroll.
            raise_overlay(&window, left, top, width, height);
            restore_page(&window);
            focus_overlay(&window);
            true
        });
        if shown != Some(true) {
            close_scroll_hud(app);
            return Err(ScreenshotError::new("capture_failed"));
        }
        return Ok(());
    }
    #[cfg(not(target_os = "macos"))]
    {
        hide_capture_overlays(app);
        let Some(window) = app.get_webview_window(SCROLL_LABEL) else {
            reveal_capture_overlays(app);
            return Err(ScreenshotError::new("capture_failed"));
        };
        place_scroll_edges(app, edges);
        raise_overlay(&window, left, top, width, height);
        use tauri::{LogicalPosition, LogicalSize};
        let _ = window.set_position(LogicalPosition::new(left, top));
        let _ = window.set_size(LogicalSize::new(width, height));
        show_content(&window);
        let _ = window.show();
        order_front(&window);
        let _ = window.set_focus();
        Ok(())
    }
}

pub fn close_scroll_hud(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let app_for_work = app.clone();
        on_main(app, move || {
            for label in SCROLL_FRAME_LABELS {
                if let Some(window) = app_for_work.get_webview_window(label) {
                    dismiss_overlay(&window);
                }
            }
            if let Some(window) = app_for_work.get_webview_window(SCROLL_LABEL) {
                dismiss_overlay(&window);
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    {
        for label in SCROLL_FRAME_LABELS {
            if let Some(window) = app.get_webview_window(label) {
                conceal(&window);
            }
        }
        if let Some(window) = app.get_webview_window(SCROLL_LABEL) {
            conceal(&window);
        }
    }
    reveal_capture_overlays(app);
}

/// Keep the scroll window up, drop the selection outline, and grow it into the result preview.
pub fn present_scroll_result(
    app: &AppHandle,
    left: f64,
    top: f64,
    width: f64,
    height: f64,
) -> bool {
    #[cfg(target_os = "macos")]
    {
        let app_for_work = app.clone();
        return on_main(app, move || {
            for label in SCROLL_FRAME_LABELS {
                if let Some(window) = app_for_work.get_webview_window(label) {
                    dismiss_overlay(&window);
                }
            }
            let Some(window) = app_for_work.get_webview_window(SCROLL_LABEL) else {
                return false;
            };
            raise_overlay(&window, left, top, width, height);
            exclude_from_capture(&window);
            show_content(&window);
            order_front(&window);
            raise_overlay(&window, left, top, width, height);
            restore_page(&window);
            focus_overlay(&window);
            true
        })
        .unwrap_or(false);
    }
    #[cfg(not(target_os = "macos"))]
    {
        for label in SCROLL_FRAME_LABELS {
            if let Some(window) = app.get_webview_window(label) {
                conceal(&window);
            }
        }
        let Some(window) = app.get_webview_window(SCROLL_LABEL) else {
            return false;
        };
        raise_overlay(&window, left, top, width, height);
        use tauri::{LogicalPosition, LogicalSize};
        let _ = window.set_position(LogicalPosition::new(left, top));
        let _ = window.set_size(LogicalSize::new(width, height));
        show_content(&window);
        let _ = window.show();
        order_front(&window);
        let _ = window.set_focus();
        true
    }
}

pub fn show_record_preview(
    app: &AppHandle,
    left: f64,
    top: f64,
    width: f64,
    height: f64,
) -> Result<(), ScreenshotError> {
    if app.get_webview_window(RECORDER_LABEL).is_none() {
        let _ = recorder_window(app, left, top, width, height)?;
    }
    #[cfg(target_os = "macos")]
    {
        let app_for_work = app.clone();
        // Same path as a pin: move the window with AppKit on the main thread.
        // tao's show() inside this waited callback deadlocks, and the preview never appears.
        let placed = on_main(app, move || {
            let Some(window) = app_for_work.get_webview_window(RECORDER_LABEL) else {
                return false;
            };
            raise_overlay(&window, left, top, width, height);
            restore_page(&window);
            show_content(&window);
            order_front(&window);
            true
        });
        if placed != Some(true) {
            return Err(ScreenshotError::new("capture_failed"));
        }
        return Ok(());
    }
    #[cfg(not(target_os = "macos"))]
    {
        let Some(window) = app.get_webview_window(RECORDER_LABEL) else {
            return Err(ScreenshotError::new("capture_failed"));
        };
        raise_overlay(&window, left, top, width, height);
        use tauri::{LogicalPosition, LogicalSize};
        let _ = window.set_position(LogicalPosition::new(left, top));
        let _ = window.set_size(LogicalSize::new(width, height));
        show_content(&window);
        let _ = window.set_ignore_cursor_events(false);
        let _ = window.show();
        order_front(&window);
        Ok(())
    }
}

const FRAME_LABELS: [&str; 4] = [
    "screenshot-record-frame-top",
    "screenshot-record-frame-right",
    "screenshot-record-frame-bottom",
    "screenshot-record-frame-left",
];

pub fn open_record_frame(
    app: &AppHandle,
    edges: [Option<(f64, f64, f64, f64)>; 4],
) -> Result<(), ScreenshotError> {
    for (label, edge) in FRAME_LABELS.iter().zip(edges) {
        match edge {
            Some((left, top, width, height)) => {
                show_frame_edge(app, label, left, top, width, height)?
            }
            None => {
                if let Some(window) = app.get_webview_window(label) {
                    conceal(&window);
                }
            }
        }
    }
    Ok(())
}

pub fn close_record_frame(app: &AppHandle) {
    for label in FRAME_LABELS {
        if let Some(window) = app.get_webview_window(label) {
            conceal(&window);
        }
    }
}

fn show_frame_edge(
    app: &AppHandle,
    label: &str,
    left: f64,
    top: f64,
    width: f64,
    height: f64,
) -> Result<(), ScreenshotError> {
    let window = if let Some(window) = app.get_webview_window(label) {
        window
    } else {
        WebviewWindowBuilder::new(app, label, WebviewUrl::App("record-frame.html".into()))
            .title("Pixo")
            .decorations(false)
            .transparent(false)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .visible(false)
            .focused(false)
            .position(left, top)
            .inner_size(width, height)
            .min_inner_size(1.0, 1.0)
            .background_color(tauri::window::Color(59, 130, 246, 255))
            .build()
            .map_err(|error| {
                eprintln!("screenshot record frame: {error}");
                ScreenshotError::new("capture_failed")
            })?
    };
    raise_overlay(&window, left, top, width, height);
    #[cfg(not(target_os = "macos"))]
    {
        use tauri::{LogicalPosition, LogicalSize};
        let _ = window.set_position(LogicalPosition::new(left, top));
        let _ = window.set_size(LogicalSize::new(width, height));
    }
    show_content(&window);
    let _ = window.set_ignore_cursor_events(true);
    let _ = window.show();
    order_front(&window);
    Ok(())
}

/// The bundle id of the app in front, when it is not Pixo.
/// Captured before the overlay activates this process.
pub fn other_front_bundle(app: &AppHandle) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        return on_main(app, read_other_front_bundle).flatten();
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        None
    }
}

/// Hide the screenshot layer, put Pixo's own windows back where they were,
/// and return to the app that was in front when the shot started.
pub fn settle_after_shot(app: &AppHandle, windows: &[(String, bool)], previous: Option<&str>) {
    let windows: Vec<(String, bool)> = windows.to_vec();
    let previous = previous.map(str::to_string);
    let app = app.clone();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    if app
        .clone()
        .run_on_main_thread(move || {
            conceal_overlays(&app);
            restore_own_windows(&app, &windows, previous.as_deref());
            #[cfg(target_os = "macos")]
            if let Some(bundle) = previous.as_deref() {
                return_to_app(bundle);
            }
            let _ = sender.send(());
        })
        .is_err()
    {
        return;
    }
    let _ = receiver.recv();
}

fn restore_own_windows(app: &AppHandle, windows: &[(String, bool)], previous: Option<&str>) {
    for (label, was_visible) in windows {
        let Some(window) = app.get_webview_window(label) else {
            continue;
        };
        if *was_visible {
            if previous.is_some() {
                show_without_activating(&window);
            } else {
                let _ = window.show();
            }
        } else if window.is_visible().unwrap_or(false) {
            // Focusing the overlay activates the process and can surface the main window.
            let _ = window.hide();
        }
    }
}

fn show_without_activating(window: &tauri::WebviewWindow) {
    #[cfg(target_os = "macos")]
    if let Some(ns_window) = retained(window) {
        ns_window.orderFront(None);
        return;
    }
    let _ = window.show();
}

fn on_main<T: Send + 'static>(
    app: &AppHandle,
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    // Already on the main thread: waiting for a queued callback never runs.
    #[cfg(target_os = "macos")]
    if objc2::MainThreadMarker::new().is_some() {
        return Some(work());
    }
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    app.run_on_main_thread(move || {
        let _ = sender.send(work());
    })
    .ok()?;
    receiver.recv().ok()
}

#[cfg(target_os = "macos")]
fn dismiss_overlay(window: &tauri::WebviewWindow) {
    if let Some(ns_window) = retained(window) {
        ns_window.orderOut(None);
    }
}

#[cfg(target_os = "macos")]
fn focus_overlay(window: &tauri::WebviewWindow) {
    if let Some(ns_window) = retained(window) {
        ns_window.makeKeyAndOrderFront(None);
    }
}

#[cfg(target_os = "macos")]
fn show_passthrough(window: &tauri::WebviewWindow) {
    if let Some(ns_window) = retained(window) {
        ns_window.setAlphaValue(1.0);
        ns_window.setIgnoresMouseEvents(true);
    }
}

#[cfg(target_os = "macos")]
fn exclude_from_capture(window: &tauri::WebviewWindow) {
    if let Some(ns_window) = retained(window) {
        ns_window.setSharingType(objc2_app_kit::NSWindowSharingType::None);
    }
}

#[cfg(target_os = "macos")]
fn include_in_capture(window: &tauri::WebviewWindow) {
    if let Some(ns_window) = retained(window) {
        ns_window.setSharingType(objc2_app_kit::NSWindowSharingType::ReadOnly);
    }
}

pub fn reveal(window: &tauri::WebviewWindow) {
    // Scroll keeps the capture layer parked. A late ready event must not
    // bring that full-screen window back over the page.
    if CAPTURE_PARKED.load(Ordering::Acquire) {
        return;
    }
    #[cfg(target_os = "macos")]
    {
        restore_pending_frame(window);
        include_in_capture(window);
    }
    show_content(window);
    let _ = window.show();
    order_front(window);
    if window.label() == "screenshot-overlay-1" {
        let _ = window.set_focus();
    }
}

fn arm(window: &tauri::WebviewWindow) {
    if CAPTURE_PARKED.load(Ordering::Acquire) {
        return;
    }
    // The page background is black until the frozen frame is painted.
    // Keep the full-size window off the desktop so that wait is invisible.
    // Alpha 0 is not a substitute: on this opaque window it is a black sheet.
    #[cfg(target_os = "macos")]
    {
        include_in_capture(window);
        show_content(window);
        hold_overlay_offscreen(window);
        return;
    }
    #[cfg(not(target_os = "macos"))]
    let _ = window.hide();
}

fn conceal(window: &tauri::WebviewWindow) {
    forget_pending_frame(&window.label());
    // Recording finishes on a background thread. AppKit throws if these run
    // there, and that exception aborts the process.
    #[cfg(target_os = "macos")]
    order_out_on_main(window);
    let _ = window.hide();
}

#[cfg(target_os = "macos")]
fn order_out_window(window: &tauri::WebviewWindow) {
    if let Some(ns_window) = retained(window) {
        ns_window.setAlphaValue(1.0);
        ns_window.orderOut(None);
    }
}

#[cfg(target_os = "macos")]
fn order_out_on_main(window: &tauri::WebviewWindow) {
    if objc2::MainThreadMarker::new().is_some() {
        order_out_window(window);
        return;
    }
    let app = window.app_handle().clone();
    let label = window.label().to_string();
    let queued = app.clone();
    let _ = on_main(&app, move || {
        let Some(window) = queued.get_webview_window(&label) else {
            return;
        };
        order_out_window(&window);
    });
}

/// Order the overlay in at its real size, but off to the left, so the webview
/// can paint the screenshot before the window covers the desktop.
#[cfg(target_os = "macos")]
fn hold_overlay_offscreen(window: &tauri::WebviewWindow) {
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    let Some(ns_window) = retained(window) else {
        return;
    };
    let frame = ns_window.frame();
    if let Some(home) = capture_home_frame(
        frame.origin.x,
        frame.origin.y,
        frame.size.width,
        frame.size.height,
    ) {
        remember_pending_frame(&window.label(), home);
    }
    let width = frame.size.width.max(1.0);
    let height = frame.size.height.max(1.0);
    ns_window.setIgnoresMouseEvents(true);
    ns_window.setFrame_display(
        NSRect::new(
            NSPoint::new(OFFSCREEN_X, frame.origin.y),
            NSSize::new(width, height),
        ),
        false,
    );
    ns_window.orderFrontRegardless();
}

#[cfg(target_os = "macos")]
fn restore_pending_frame(window: &tauri::WebviewWindow) {
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    let Some((x, y, width, height)) = take_pending_frame(&window.label()) else {
        return;
    };
    let Some(ns_window) = retained(window) else {
        return;
    };
    ns_window.setContentMinSize(NSSize::new(1.0, 1.0));
    ns_window.setContentMaxSize(NSSize::new(width.max(1.0), height.max(1.0)));
    ns_window.setFrame_display(
        NSRect::new(
            NSPoint::new(x, y),
            NSSize::new(width.max(1.0), height.max(1.0)),
        ),
        false,
    );
    ns_window.setContentMaxSize(NSSize::new(16_000.0, 16_000.0));
}

#[cfg(target_os = "macos")]
fn order_front(window: &tauri::WebviewWindow) {
    if let Some(ns_window) = retained(window) {
        ns_window.orderFrontRegardless();
    }
}

#[cfg(not(target_os = "macos"))]
fn order_front(window: &tauri::WebviewWindow) {
    let _ = window.set_always_on_top(true);
}

/// Shrink the live overlay to the selection. The page is already painted, so this
/// does not encode a PNG or open another window. Returns whether the page was cropped
/// in place; other platforms only move the window.
pub fn adopt_overlay(
    app: &AppHandle,
    label: &str,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale: f64,
    crop_x: f64,
    crop_y: f64,
    page_w: f64,
    page_h: f64,
) -> Result<bool, ScreenshotError> {
    if !label.starts_with("screenshot-overlay-") || app.get_webview_window(label).is_none() {
        return Err(ScreenshotError::new("not_found"));
    }
    let left = logical_x(x, scale);
    let top = logical_y(y, scale);
    let frame_width = logical_span(width, scale);
    let frame_height = logical_span(height, scale);
    #[cfg(target_os = "macos")]
    {
        let app_for_work = app.clone();
        let owned = label.to_string();
        let cropped = on_main(&app.clone(), {
            let label = owned.clone();
            move || {
                let Some(window) = app_for_work.get_webview_window(&label) else {
                    return false;
                };
                // Keep the webview filling the window. The page shifts the selection
                // to the origin itself, so the window shows that rectangle.
                raise_pin(&window, left, top, frame_width, frame_height);
                loosen_pin(&window);
                restore_page(&window);
                show_content(&window);
                order_front(&window);
                let _ = (crop_x, crop_y, page_w, page_h);
                true
            }
        });
        if cropped != Some(true) {
            return Err(ScreenshotError::new("capture_failed"));
        }
        remember_pin(&owned);
        return Ok(true);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let Some(window) = app.get_webview_window(label) else {
            return Err(ScreenshotError::new("not_found"));
        };
        raise_pin(&window, left, top, frame_width, frame_height);
        use tauri::{LogicalPosition, LogicalSize};
        let _ = window.set_position(LogicalPosition::new(left, top));
        let _ = window.set_size(LogicalSize::new(frame_width, frame_height));
        let _ = window.show();
        order_front(&window);
        remember_pin(label);
        let _ = (crop_x, crop_y, page_w, page_h);
        Ok(false)
    }
}

pub fn release_overlay(app: &AppHandle, label: &str) -> Result<(), ScreenshotError> {
    if !label.starts_with("screenshot-overlay-") {
        return Err(ScreenshotError::new("not_found"));
    }
    forget_pin(label);
    let Some(window) = app.get_webview_window(label) else {
        return Ok(());
    };
    #[cfg(target_os = "macos")]
    {
        let app_for_work = app.clone();
        let owned = label.to_string();
        let _ = on_main(&app.clone(), move || {
            let Some(window) = app_for_work.get_webview_window(&owned) else {
                return;
            };
            restore_page(&window);
        });
    }
    conceal(&window);
    Ok(())
}

#[cfg(target_os = "macos")]
fn largest_subview(
    content: &objc2_app_kit::NSView,
) -> Option<objc2::rc::Retained<objc2_app_kit::NSView>> {
    let subviews = content.subviews();
    if subviews.count() == 0 {
        return None;
    }
    let mut chosen = subviews.objectAtIndex(0);
    let mut best = {
        let size = chosen.frame().size;
        size.width * size.height
    };
    let mut index = 1;
    while index < subviews.count() {
        let view = subviews.objectAtIndex(index);
        let size = view.frame().size;
        let area = size.width * size.height;
        if area > best {
            best = area;
            chosen = view;
        }
        index += 1;
    }
    Some(chosen)
}

#[cfg(target_os = "macos")]
fn loosen_pin(window: &tauri::WebviewWindow) {
    use objc2_app_kit::NSWindowCollectionBehavior;
    let Some(ns_window) = retained(window) else {
        return;
    };
    ns_window.setHasShadow(true);
    ns_window.setHidesOnDeactivate(false);
    ns_window.setMovable(true);
    ns_window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );
}

#[cfg(target_os = "macos")]
fn restore_page(window: &tauri::WebviewWindow) {
    use objc2_app_kit::NSAutoresizingMaskOptions;
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    let Some(ns_window) = retained(window) else {
        return;
    };
    let Some(content) = ns_window.contentView() else {
        return;
    };
    let Some(page) = largest_subview(&content) else {
        return;
    };
    let size = content.frame().size;
    page.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    page.setFrame(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(size.width.max(1.0), size.height.max(1.0)),
    ));
}

pub fn show_pin(
    app: &AppHandle,
    slot: u8,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale: f64,
) -> Result<(), ScreenshotError> {
    let label = format!("screenshot-pin-{slot}");
    let left = logical_x(x, scale);
    let top = logical_y(y, scale);
    let frame_width = logical_span(width, scale);
    let frame_height = logical_span(height, scale);
    if app.get_webview_window(&label).is_none() {
        create_pin_window(app, &label, left, top, frame_width, frame_height)?;
    }
    #[cfg(target_os = "macos")]
    {
        let app_for_work = app.clone();
        let label = label.clone();
        let shown = on_main(&app.clone(), move || {
            let Some(window) = app_for_work.get_webview_window(&label) else {
                return false;
            };
            // Floating level stays under the screensaver overlay. The page can
            // paint the image there; direct AppKit calls avoid tao's show().
            raise_pin(&window, left, top, frame_width, frame_height);
            show_content(&window);
            order_front(&window);
            hold_overlays_above(&app_for_work);
            true
        });
        if shown != Some(true) {
            return Err(ScreenshotError::new("capture_failed"));
        }
        return Ok(());
    }
    #[cfg(not(target_os = "macos"))]
    {
        use tauri::{LogicalPosition, LogicalSize};
        let Some(window) = app.get_webview_window(&label) else {
            return Err(ScreenshotError::new("capture_failed"));
        };
        raise_pin(&window, left, top, frame_width, frame_height);
        let _ = window.set_position(LogicalPosition::new(left, top));
        let _ = window.set_size(LogicalSize::new(frame_width, frame_height));
        let _ = window.show();
        order_front(&window);
        Ok(())
    }
}

pub fn conceal_pin(app: &AppHandle, slot: u8) {
    if let Some(window) = app.get_webview_window(&format!("screenshot-pin-{slot}")) {
        conceal(&window);
    }
}

fn create_pin_window(
    app: &AppHandle,
    label: &str,
    left: f64,
    top: f64,
    frame_width: f64,
    frame_height: f64,
) -> Result<tauri::WebviewWindow, ScreenshotError> {
    WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html?view=shot".into()))
        .title("Pixo")
        .decorations(false)
        .shadow(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible(false)
        .focused(false)
        .position(left, top)
        .inner_size(frame_width, frame_height)
        .background_color(tauri::window::Color(0, 0, 0, 255))
        .build()
        .map_err(|error| {
            eprintln!("screenshot pin: {error}");
            ScreenshotError::new("capture_failed")
        })
}

#[cfg(target_os = "macos")]
fn hold_overlays_above(app: &AppHandle) {
    use objc2_app_kit::NSScreenSaverWindowLevel;
    for slot in 1..=8 {
        let Some(overlay) = app.get_webview_window(&format!("screenshot-overlay-{slot}")) else {
            continue;
        };
        let Some(ns_window) = retained(&overlay) else {
            continue;
        };
        if ns_window.alphaValue() < 0.5 {
            continue;
        }
        ns_window.setLevel(NSScreenSaverWindowLevel);
        ns_window.orderFrontRegardless();
    }
}

#[cfg(target_os = "macos")]
fn raise_overlay(window: &tauri::WebviewWindow, x: f64, y: f64, width: f64, height: f64) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSScreenSaverWindowLevel, NSWindowCollectionBehavior};
    let Some(ns_window) = retained(window) else {
        return;
    };
    let _ = MainThreadMarker::new();
    ns_window.setLevel(NSScreenSaverWindowLevel);
    ns_window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Stationary,
    );
    ns_window.setHidesOnDeactivate(false);
    ns_window.setMovable(false);
    place_frame(&ns_window, x, y, width, height);
}

#[cfg(not(target_os = "macos"))]
fn raise_overlay(window: &tauri::WebviewWindow, _x: f64, _y: f64, _width: f64, _height: f64) {
    let _ = window.set_always_on_top(true);
}

#[cfg(target_os = "macos")]
fn raise_pin(window: &tauri::WebviewWindow, x: f64, y: f64, width: f64, height: f64) {
    use objc2_app_kit::NSFloatingWindowLevel;
    let Some(ns_window) = retained(window) else {
        return;
    };
    ns_window.setLevel(NSFloatingWindowLevel);
    ns_window.setHidesOnDeactivate(false);
    ns_window.setMovable(true);
    place_frame(&ns_window, x, y, width, height);
}

#[cfg(not(target_os = "macos"))]
fn raise_pin(window: &tauri::WebviewWindow, _x: f64, _y: f64, _width: f64, _height: f64) {
    let _ = window.set_always_on_top(true);
}

#[cfg(target_os = "macos")]
fn place_frame(window: &objc2_app_kit::NSWindow, x: f64, y: f64, width: f64, height: f64) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSScreen;
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    let Some(main_height) = primary_height() else {
        return;
    };
    let origin_y = appkit_origin_y(y, height, main_height);
    let mut rect = NSRect::new(NSPoint::new(x, origin_y), NSSize::new(width, height));
    if let Some(marker) = MainThreadMarker::new() {
        let screens = NSScreen::screens(marker);
        let mut index = 0;
        while index < screens.count() {
            let screen = screens.objectAtIndex(index);
            index += 1;
            let frame = screen.frame();
            let same_screen = (frame.origin.x - x).abs() < 1.0
                && (frame.origin.y - origin_y).abs() < 1.0
                && (frame.size.width - width).abs() < 1.0
                && (frame.size.height - height).abs() < 1.0;
            if same_screen {
                rect = frame;
                break;
            }
        }
        // A reused scroll window keeps the previous result size unless the
        // content limit is lowered before setFrame.
        window.setContentMinSize(NSSize::new(1.0, 1.0));
        window.setContentMaxSize(NSSize::new(
            rect.size.width.max(1.0),
            rect.size.height.max(1.0),
        ));
        window.setFrame_display(rect, true);
        window.setContentMaxSize(NSSize::new(16_000.0, 16_000.0));
        return;
    }
    window.setFrame_display(rect, false);
}

#[cfg(target_os = "macos")]
fn primary_height() -> Option<f64> {
    let bounds = unsafe { CGDisplayBounds(CGMainDisplayID()) };
    if bounds.size.height > 1.0 {
        Some(bounds.size.height)
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct CGSize {
    width: f64,
    height: f64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGMainDisplayID() -> u32;
    fn CGDisplayBounds(display: u32) -> CGRect;
}

#[cfg(target_os = "macos")]
fn show_content(window: &tauri::WebviewWindow) {
    let Some(ns_window) = retained(window) else {
        return;
    };
    ns_window.setIgnoresMouseEvents(false);
    ns_window.setAlphaValue(1.0);
}

#[cfg(not(target_os = "macos"))]
fn show_content(_window: &tauri::WebviewWindow) {}

#[cfg(target_os = "macos")]
fn retained(window: &tauri::WebviewWindow) -> Option<objc2::rc::Retained<objc2_app_kit::NSWindow>> {
    let raw = window.ns_window().ok()?;
    unsafe { objc2::rc::Retained::retain(raw.cast::<objc2_app_kit::NSWindow>()) }
}

#[cfg(target_os = "macos")]
struct OverlayPolicy {
    app: objc2::rc::Retained<objc2_app_kit::NSApplication>,
    previous: isize,
}

#[cfg(target_os = "macos")]
impl OverlayPolicy {
    fn acquire() -> Option<Self> {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
        let marker = MainThreadMarker::new()?;
        let app = NSApplication::sharedApplication(marker);
        let previous = app.activationPolicy().0;
        if previous != NSApplicationActivationPolicy::Accessory.0 {
            let _ = app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        }
        Some(Self { app, previous })
    }
}

#[cfg(target_os = "macos")]
impl Drop for OverlayPolicy {
    fn drop(&mut self) {
        use objc2_app_kit::NSApplicationActivationPolicy;
        if self.previous != NSApplicationActivationPolicy::Accessory.0 {
            let _ = self
                .app
                .setActivationPolicy(NSApplicationActivationPolicy(self.previous));
        }
    }
}

#[cfg(not(target_os = "macos"))]
struct OverlayPolicy;

#[cfg(not(target_os = "macos"))]
impl OverlayPolicy {
    fn acquire() -> Option<Self> {
        Some(Self)
    }
}

#[cfg(target_os = "macos")]
fn read_other_front_bundle() -> Option<String> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSRunningApplication, NSWorkspace};
    let _marker = MainThreadMarker::new()?;
    let front = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    let front_id = front.bundleIdentifier()?.to_string();
    let own_id = NSRunningApplication::currentApplication()
        .bundleIdentifier()?
        .to_string();
    (front_id != own_id).then_some(front_id)
}

#[cfg(target_os = "macos")]
fn return_to_app(bundle_id: &str) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationActivationOptions, NSRunningApplication};
    use objc2_foundation::NSString;
    let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(
        bundle_id,
    ));
    if apps.count() > 0 {
        let _ = apps
            .objectAtIndex(0)
            .activateWithOptions(NSApplicationActivationOptions::empty());
    }
    let Some(marker) = MainThreadMarker::new() else {
        return;
    };
    if front_is_ours() {
        NSApplication::sharedApplication(marker).deactivate();
    }
}

#[cfg(target_os = "macos")]
fn front_is_ours() -> bool {
    use objc2_app_kit::{NSRunningApplication, NSWorkspace};
    let Some(front) = NSWorkspace::sharedWorkspace().frontmostApplication() else {
        return false;
    };
    let Some(front_id) = front.bundleIdentifier() else {
        return false;
    };
    NSRunningApplication::currentApplication()
        .bundleIdentifier()
        .is_some_and(|own| own.to_string() == front_id.to_string())
}

#[cfg(test)]
mod tests {
    use super::appkit_origin_y;

    #[test]
    fn overlay_frame_covers_each_display_including_the_menu_bar() {
        assert_eq!(appkit_origin_y(0.0, 982.0, 982.0), 0.0);
        assert_eq!(appkit_origin_y(-56.0, 1080.0, 982.0), -42.0);
    }

    #[test]
    fn offscreen_overlay_is_not_remembered_as_the_desktop_frame() {
        assert_eq!(
            super::capture_home_frame(0.0, 0.0, 1440.0, 900.0),
            Some((0.0, 0.0, 1440.0, 900.0))
        );
        assert_eq!(
            super::capture_home_frame(-20_000.0, 40.0, 1440.0, 900.0),
            None
        );
        assert_eq!(super::capture_home_frame(0.0, 0.0, 1.0, 1.0), None);
    }
}
