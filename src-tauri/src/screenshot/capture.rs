use super::error::ScreenshotError;
use super::snap::{Bounds, RawWindow};

#[derive(Debug, Clone)]
pub struct CapturedMonitor {
    pub origin_x: i32,
    pub origin_y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
    pub rgba: Vec<u8>,
}

pub fn session_kind() -> &'static str {
    if cfg!(target_os = "macos") {
        return "macos";
    }
    if cfg!(target_os = "windows") {
        return "windows";
    }
    if is_wayland() {
        "linux-wayland"
    } else {
        "linux-x11"
    }
}

pub fn is_wayland() -> bool {
    if cfg!(not(target_os = "linux")) {
        return false;
    }
    matches!(
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        Some("wayland")
    ) || (std::env::var_os("WAYLAND_DISPLAY").is_some() && std::env::var_os("DISPLAY").is_none())
}

#[derive(Clone, Copy)]
pub struct MonitorLayout {
    pub origin_x: i32,
    pub origin_y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
}

pub fn monitor_layout() -> Vec<MonitorLayout> {
    let Ok(monitors) = xcap::Monitor::all() else {
        return Vec::new();
    };
    monitors
        .into_iter()
        .take(8)
        .map(|monitor| {
            let scale = f64::from(monitor.scale_factor().unwrap_or(1.0));
            let (origin_x, origin_y) =
                to_physical_origin(monitor.x().unwrap_or(0), monitor.y().unwrap_or(0), scale);
            let (width, height) = to_physical_size(
                monitor.width().unwrap_or(0),
                monitor.height().unwrap_or(0),
                scale,
            );
            MonitorLayout {
                origin_x,
                origin_y,
                width,
                height,
                scale_factor: scale,
            }
        })
        .collect()
}

/// The display under the pointer, in the same physical space as [`monitor_layout`].
/// Returns nothing when the pointer cannot be read, including Wayland.
pub fn monitor_under_cursor() -> Option<MonitorLayout> {
    let (x, y) = cursor_desktop()?;
    let screens = desktop_screens();
    let index = screen_index(
        &screens
            .iter()
            .map(|screen| (screen.left, screen.top, screen.width, screen.height))
            .collect::<Vec<_>>(),
        x,
        y,
    )?;
    screens.get(index).map(|screen| screen.layout)
}

struct DesktopScreen {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
    layout: MonitorLayout,
}

fn desktop_screens() -> Vec<DesktopScreen> {
    let Ok(monitors) = xcap::Monitor::all() else {
        return Vec::new();
    };
    monitors
        .into_iter()
        .take(8)
        .map(|monitor| {
            let scale = f64::from(monitor.scale_factor().unwrap_or(1.0));
            let raw_x = monitor.x().unwrap_or(0);
            let raw_y = monitor.y().unwrap_or(0);
            let raw_w = monitor.width().unwrap_or(0);
            let raw_h = monitor.height().unwrap_or(0);
            let (origin_x, origin_y) = to_physical_origin(raw_x, raw_y, scale);
            let (width, height) = to_physical_size(raw_w, raw_h, scale);
            let (left, top, hit_w, hit_h) = cursor_hit_rect(
                raw_x, raw_y, raw_w, raw_h, origin_x, origin_y, width, height,
            );
            DesktopScreen {
                left,
                top,
                width: hit_w,
                height: hit_h,
                layout: MonitorLayout {
                    origin_x,
                    origin_y,
                    width,
                    height,
                    scale_factor: scale,
                },
            }
        })
        .collect()
}

/// macOS Quartz and Windows report the pointer in xcap's public origin space.
/// X11's pointer is in root-window pixels, which matches the physical layout.
fn cursor_hit_rect(
    raw_x: i32,
    raw_y: i32,
    raw_w: u32,
    raw_h: u32,
    origin_x: i32,
    origin_y: i32,
    width: u32,
    height: u32,
) -> (f64, f64, f64, f64) {
    if cfg!(target_os = "linux") {
        (
            f64::from(origin_x),
            f64::from(origin_y),
            f64::from(width),
            f64::from(height),
        )
    } else {
        (
            f64::from(raw_x),
            f64::from(raw_y),
            f64::from(raw_w),
            f64::from(raw_h),
        )
    }
}

fn screen_index(screens: &[(f64, f64, f64, f64)], x: f64, y: f64) -> Option<usize> {
    if screens.is_empty() || !x.is_finite() || !y.is_finite() {
        return None;
    }
    screens
        .iter()
        .position(|screen| contains_screen(*screen, x, y))
        .or_else(|| {
            screens
                .iter()
                .enumerate()
                .min_by(|(_, left), (_, right)| {
                    screen_distance(**left, x, y).total_cmp(&screen_distance(**right, x, y))
                })
                .map(|(index, _)| index)
        })
}

fn contains_screen(screen: (f64, f64, f64, f64), x: f64, y: f64) -> bool {
    let (left, top, width, height) = screen;
    width > 0.0 && height > 0.0 && x >= left && y >= top && x < left + width && y < top + height
}

fn screen_distance(screen: (f64, f64, f64, f64), x: f64, y: f64) -> f64 {
    let (left, top, width, height) = screen;
    let dx = x - (left + width / 2.0);
    let dy = y - (top + height / 2.0);
    dx * dx + dy * dy
}

pub fn capture_monitors() -> Result<(Vec<CapturedMonitor>, bool), ScreenshotError> {
    if is_wayland() {
        return Err(ScreenshotError::with_params(
            "unsupported",
            serde_json::json!({ "reason": "wayland" }),
        ));
    }
    let monitors = xcap::Monitor::all().map_err(|error| {
        eprintln!("screenshot monitors: {error}");
        ScreenshotError::new("capture_failed")
    })?;
    if monitors.is_empty() {
        return Err(ScreenshotError::new("capture_failed"));
    }
    let truncated = monitors.len() > 8;
    let mut captured = Vec::new();
    for monitor in monitors.into_iter().take(8) {
        let image = monitor.capture_image().map_err(|error| {
            eprintln!("screenshot capture: {error}");
            ScreenshotError::new("capture_failed")
        })?;
        let width = image.width();
        let height = image.height();
        let rgba = image.into_raw();
        let scale = f64::from(monitor.scale_factor().unwrap_or(1.0));
        let (origin_x, origin_y) =
            to_physical_origin(monitor.x().unwrap_or(0), monitor.y().unwrap_or(0), scale);
        captured.push(CapturedMonitor {
            origin_x,
            origin_y,
            width,
            height,
            scale_factor: scale,
            rgba,
        });
    }
    Ok((captured, truncated))
}

pub fn monitor_signature() -> Option<Vec<(i32, i32, u32, u32)>> {
    let monitors = xcap::Monitor::all().ok()?;
    Some(
        monitors
            .into_iter()
            .take(8)
            .map(|monitor| {
                let scale = f64::from(monitor.scale_factor().unwrap_or(1.0));
                let (x, y) =
                    to_physical_origin(monitor.x().unwrap_or(0), monitor.y().unwrap_or(0), scale);
                let (width, height) = to_physical_size(
                    monitor.width().unwrap_or(0),
                    monitor.height().unwrap_or(0),
                    scale,
                );
                (x, y, width, height)
            })
            .collect(),
    )
}

/// xcap reports monitor origins in points on macOS and Linux, then capture
/// returns physical pixels. Windows already reports physical pixels.
pub fn scale_coord(value: i32, scale: f64) -> i32 {
    (f64::from(value) * scale.max(1.0)).round() as i32
}

pub fn scale_len(value: u32, scale: f64) -> u32 {
    (f64::from(value) * scale.max(1.0)).round() as u32
}

pub fn to_physical_origin(x: i32, y: i32, scale: f64) -> (i32, i32) {
    if cfg!(target_os = "windows") {
        (x, y)
    } else {
        (scale_coord(x, scale), scale_coord(y, scale))
    }
}

pub fn to_physical_size(width: u32, height: u32, scale: f64) -> (u32, u32) {
    if cfg!(target_os = "windows") {
        (width, height)
    } else {
        (scale_len(width, scale), scale_len(height, scale))
    }
}

/// macOS window frames from xcap are in points. Linux and Windows are pixels.
pub fn window_in_pixels(window: &RawWindow, scale: f64) -> RawWindow {
    if cfg!(not(target_os = "macos")) {
        return RawWindow {
            bounds: window.bounds,
            pid: window.pid,
            minimized: window.minimized,
        };
    }
    RawWindow {
        bounds: Bounds {
            x: scale_coord(window.bounds.x, scale),
            y: scale_coord(window.bounds.y, scale),
            width: scale_len(window.bounds.width, scale),
            height: scale_len(window.bounds.height, scale),
        },
        pid: window.pid,
        minimized: window.minimized,
    }
}

pub fn list_windows() -> Vec<RawWindow> {
    let Ok(windows) = xcap::Window::all() else {
        return Vec::new();
    };
    windows
        .into_iter()
        .filter_map(|window| {
            Some(RawWindow {
                bounds: Bounds {
                    x: window.x().ok()?,
                    y: window.y().ok()?,
                    width: window.width().ok()?,
                    height: window.height().ok()?,
                },
                pid: window.pid().ok()?,
                minimized: window.is_minimized().unwrap_or(false),
            })
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn cursor_desktop() -> Option<(f64, f64)> {
    unsafe {
        let event = CGEventCreate(std::ptr::null_mut());
        if event.is_null() {
            return None;
        }
        let point = CGEventGetLocation(event);
        CFRelease(event);
        (point.x.is_finite() && point.y.is_finite()).then_some((point.x, point.y))
    }
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct CgPoint {
    x: f64,
    y: f64,
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventCreate(source: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    fn CGEventGetLocation(event: *mut std::ffi::c_void) -> CgPoint;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: *const std::ffi::c_void);
}

#[cfg(target_os = "windows")]
fn cursor_desktop() -> Option<(f64, f64)> {
    use ::windows::Win32::Foundation::POINT;
    use ::windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut point = POINT::default();
    if unsafe { GetCursorPos(&mut point) }.is_err() {
        return None;
    }
    Some((f64::from(point.x), f64::from(point.y)))
}

#[cfg(target_os = "linux")]
fn cursor_desktop() -> Option<(f64, f64)> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::ConnectionExt;

    if is_wayland() {
        return None;
    }
    let (connection, screen_index) = x11rb::connect(None).ok()?;
    let root = connection.setup().roots.get(screen_index)?.root;
    let reply = connection.query_pointer(root).ok()?.reply().ok()?;
    Some((f64::from(reply.root_x), f64::from(reply.root_y)))
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn cursor_desktop() -> Option<(f64, f64)> {
    None
}

#[cfg(test)]
mod tests {
    use super::{scale_coord, scale_len, screen_index, to_physical_origin, window_in_pixels};
    use crate::screenshot::snap::{Bounds, RawWindow};

    #[test]
    fn retina_points_become_physical_pixels() {
        assert_eq!(scale_coord(0, 2.0), 0);
        assert_eq!(scale_coord(-1920, 2.0), -3840);
        assert_eq!(scale_coord(-56, 2.0), -112);
        assert_eq!(scale_len(1512, 2.0), 3024);
        assert_eq!(scale_len(1080, 2.0), 2160);
        let (x, y) = to_physical_origin(-1920, -56, 2.0);
        if cfg!(target_os = "windows") {
            assert_eq!((x, y), (-1920, -56));
        } else {
            assert_eq!((x, y), (-3840, -112));
            assert_eq!((f64::from(x) / 2.0, f64::from(y) / 2.0), (-1920.0, -56.0));
        }
    }

    #[test]
    fn macos_window_frames_use_the_same_scale_as_the_image() {
        let window = RawWindow {
            bounds: Bounds {
                x: 100,
                y: 40,
                width: 400,
                height: 300,
            },
            pid: 4,
            minimized: false,
        };
        let pixels = window_in_pixels(&window, 2.0);
        if cfg!(target_os = "macos") {
            assert_eq!(
                pixels.bounds,
                Bounds {
                    x: 200,
                    y: 80,
                    width: 800,
                    height: 600
                }
            );
        } else {
            assert_eq!(pixels.bounds, window.bounds);
        }
    }

    #[test]
    fn the_pointer_selects_the_screen_that_contains_it() {
        let screens = [
            (0.0, 0.0, 1920.0, 1080.0),
            (1920.0, 0.0, 1440.0, 900.0),
            (-1920.0, 0.0, 1920.0, 1080.0),
        ];
        assert_eq!(screen_index(&screens, 10.0, 10.0), Some(0));
        assert_eq!(screen_index(&screens, 2000.0, 100.0), Some(1));
        assert_eq!(screen_index(&screens, -100.0, 400.0), Some(2));
        assert_eq!(screen_index(&screens, 1919.0, 0.0), Some(0));
        assert_eq!(screen_index(&screens, 1920.0, 0.0), Some(1));
        assert_eq!(screen_index(&screens, 3400.0, 400.0), Some(1));
        assert_eq!(screen_index(&[], 0.0, 0.0), None);
        assert_eq!(screen_index(&screens, f64::NAN, 0.0), None);
    }
}
