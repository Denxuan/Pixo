#[cfg(target_os = "macos")]
use objc2::MainThreadMarker;
#[cfg(target_os = "macos")]
use objc2_app_kit::NSScreen;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub fn ax_to_appkit(rect: Rect, primary_height: f64) -> Rect {
    Rect {
        x: rect.x,
        y: primary_height - rect.y - rect.height,
        width: rect.width,
        height: rect.height,
    }
}

pub fn place_beside(anchor: Rect, panel_width: f64, panel_height: f64, screen: Rect) -> (f64, f64) {
    let gap = 8.0;
    let margin = 8.0;
    let screen_right = screen.x + screen.width;
    let screen_top = screen.y + screen.height;
    let anchor_top = anchor.y + anchor.height;
    let mut x = anchor.x + anchor.width + gap;
    let mut y = anchor_top - panel_height;
    if x + panel_width > screen_right - margin {
        let left = anchor.x - gap - panel_width;
        if left >= screen.x + margin {
            x = left;
        } else {
            x = anchor.x;
            y = anchor.y - gap - panel_height;
            if y < screen.y + margin {
                y = anchor_top + gap;
            }
        }
    }
    let min_x = screen.x + margin;
    let max_x = (screen_right - margin - panel_width).max(min_x);
    let min_y = screen.y + margin;
    let max_y = (screen_top - margin - panel_height).max(min_y);
    (x.clamp(min_x, max_x), y.clamp(min_y, max_y))
}

/// Place a panel just under the top of a work area. Coordinates are AppKit points:
/// origin at the bottom left, y growing upward. `anchor_x` is the horizontal center.
pub fn place_under_bar(
    anchor_x: f64,
    panel_width: f64,
    panel_height: f64,
    area: Rect,
) -> (f64, f64) {
    let gap = 8.0;
    let margin = 8.0;
    let min_x = area.x + margin;
    let max_x = (area.x + area.width - margin - panel_width).max(min_x);
    let min_y = area.y + margin;
    let max_y = (area.y + area.height - margin - panel_height).max(min_y);
    let x = (anchor_x - panel_width / 2.0).clamp(min_x, max_x);
    let y = (area.y + area.height - gap - panel_height).clamp(min_y, max_y);
    (x, y)
}

/// Place a panel under an icon, or above it when the icon sits on a bottom taskbar.
/// Coordinates use a top-left origin and y grows downward.
pub fn place_under_icon(
    icon: Rect,
    panel_width: f64,
    panel_height: f64,
    screen: Rect,
) -> (f64, f64) {
    let gap = 8.0;
    let margin = 8.0;
    let min_x = screen.x + margin;
    let max_x = (screen.x + screen.width - margin - panel_width).max(min_x);
    let min_y = screen.y + margin;
    let max_y = (screen.y + screen.height - margin - panel_height).max(min_y);
    let x = (icon.x + icon.width / 2.0 - panel_width / 2.0).clamp(min_x, max_x);
    let below = icon.y + icon.height + gap;
    let y = if below + panel_height <= screen.y + screen.height - margin {
        below
    } else {
        icon.y - gap - panel_height
    };
    (x, y.clamp(min_y, max_y))
}

#[cfg(target_os = "macos")]
pub fn focused_input() -> Rect {
    input_rect().unwrap_or_else(mouse_rect)
}

#[cfg(target_os = "macos")]
fn input_rect() -> Option<Rect> {
    if !trusted() {
        return None;
    }
    let system = unsafe { AXUIElementCreateSystemWide() };
    if system.is_null() {
        return None;
    }
    let focused = copy_attribute(system, "AXFocusedUIElement");
    release(system);
    let focused = focused?;
    let frame = element_frame(focused.cast_mut()).map(|rect| ax_to_appkit(rect, primary_height()));
    let caret = caret_rect(focused.cast_mut()).map(|rect| ax_to_appkit(rect, primary_height()));
    release(focused);
    choose_anchor(frame, caret)
}

#[cfg(target_os = "macos")]
fn choose_anchor(frame: Option<Rect>, caret: Option<Rect>) -> Option<Rect> {
    if let Some(frame) = frame {
        if frame.height >= 8.0
            && frame.height <= 160.0
            && frame.width >= 24.0
            && frame.width <= 1600.0
        {
            return Some(frame);
        }
    }
    if let Some(caret) =
        caret.filter(|rect| rect.height >= 1.0 && rect.height <= 240.0 && rect.width <= 1600.0)
    {
        return Some(caret);
    }
    frame.filter(|rect| rect.width <= 1200.0 && rect.height <= 800.0)
}

#[cfg(target_os = "macos")]
fn trusted() -> bool {
    if unsafe { AXIsProcessTrusted() } != 0 {
        return true;
    }
    crate::permissions::prompt_accessibility();
    false
}

#[cfg(target_os = "macos")]
fn element_frame(element: *mut std::ffi::c_void) -> Option<Rect> {
    let position = copy_attribute(element, "AXPosition")?;
    let size = copy_attribute(element, "AXSize")?;
    let mut point = CGPoint { x: 0.0, y: 0.0 };
    let mut cg_size = CGSize {
        width: 0.0,
        height: 0.0,
    };
    let point_ok = unsafe { AXValueGetValue(position, 1, (&raw mut point).cast()) } != 0;
    let size_ok = unsafe { AXValueGetValue(size, 2, (&raw mut cg_size).cast()) } != 0;
    release(position);
    release(size);
    if !point_ok || !size_ok || cg_size.width < 1.0 || cg_size.height < 1.0 {
        return None;
    }
    Some(Rect {
        x: point.x,
        y: point.y,
        width: cg_size.width,
        height: cg_size.height,
    })
}

#[cfg(target_os = "macos")]
fn caret_rect(element: *mut std::ffi::c_void) -> Option<Rect> {
    let range = copy_attribute(element, "AXSelectedTextRange")?;
    let bounds_name = cf_string("AXBoundsForRange")?;
    let mut bounds = std::ptr::null();
    let status = unsafe {
        AXUIElementCopyParameterizedAttributeValue(element, bounds_name, range, &raw mut bounds)
    };
    release(bounds_name);
    release(range);
    if status != 0 || bounds.is_null() {
        release(bounds);
        return None;
    }
    let mut rect = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize {
            width: 0.0,
            height: 0.0,
        },
    };
    let ok = unsafe { AXValueGetValue(bounds, 3, (&raw mut rect).cast()) } != 0;
    release(bounds);
    if !ok || rect.size.height < 1.0 {
        return None;
    }
    Some(Rect {
        x: rect.origin.x,
        y: rect.origin.y,
        width: rect.size.width.max(1.0),
        height: rect.size.height,
    })
}

#[cfg(target_os = "macos")]
fn copy_attribute(element: *mut std::ffi::c_void, name: &str) -> Option<*const std::ffi::c_void> {
    let attribute = cf_string(name)?;
    let mut value = std::ptr::null();
    let status = unsafe { AXUIElementCopyAttributeValue(element, attribute, &raw mut value) };
    release(attribute);
    if status != 0 || value.is_null() {
        release(value);
        None
    } else {
        Some(value)
    }
}

#[cfg(target_os = "macos")]
fn cf_string(text: &str) -> Option<*const std::ffi::c_void> {
    let Ok(c_text) = std::ffi::CString::new(text) else {
        return None;
    };
    let string =
        unsafe { CFStringCreateWithCString(std::ptr::null(), c_text.as_ptr(), 0x0800_0100) };
    if string.is_null() { None } else { Some(string) }
}

#[cfg(target_os = "macos")]
fn release(value: *const std::ffi::c_void) {
    if !value.is_null() {
        unsafe { CFRelease(value) };
    }
}

#[cfg(target_os = "macos")]
fn primary_height() -> f64 {
    let bounds = unsafe { CGDisplayBounds(CGMainDisplayID()) };
    if bounds.size.height > 1.0 {
        bounds.size.height
    } else {
        900.0
    }
}

#[cfg(target_os = "macos")]
pub fn pointer() -> Rect {
    mouse_rect()
}

/// Usable area under the menu bar on the display that contains `anchor`.
#[cfg(target_os = "macos")]
pub fn work_area(anchor: Rect) -> Rect {
    if let Some(area) = visible_area(anchor) {
        if area.width > 1.0 && area.height > 1.0 {
            return area;
        }
    }
    let screen = screen_containing(anchor);
    Rect {
        x: screen.x,
        y: screen.y,
        width: screen.width,
        height: (screen.height - 24.0).max(1.0),
    }
}

#[cfg(target_os = "macos")]
fn visible_area(anchor: Rect) -> Option<Rect> {
    let mtm = MainThreadMarker::new()?;
    let screens = NSScreen::screens(mtm);
    let center_x = anchor.x + anchor.width / 2.0;
    let center_y = anchor.y + anchor.height / 2.0;
    let count = screens.count();
    let mut index = 0;
    while index < count {
        let screen = screens.objectAtIndex(index);
        index += 1;
        let frame = screen.frame();
        if center_x < frame.origin.x
            || center_y < frame.origin.y
            || center_x >= frame.origin.x + frame.size.width
            || center_y >= frame.origin.y + frame.size.height
        {
            continue;
        }
        let visible = screen.visibleFrame();
        return Some(Rect {
            x: visible.origin.x,
            y: visible.origin.y,
            width: visible.size.width,
            height: visible.size.height,
        });
    }
    None
}

#[cfg(target_os = "macos")]
fn mouse_rect() -> Rect {
    let event = unsafe { CGEventCreate(std::ptr::null_mut()) };
    let point = if event.is_null() {
        CGPoint { x: 200.0, y: 200.0 }
    } else {
        let point = unsafe { CGEventGetLocation(event) };
        release(event);
        point
    };
    Rect {
        x: point.x,
        y: point.y,
        width: 1.0,
        height: 16.0,
    }
}

#[cfg(target_os = "macos")]
pub fn screen_containing(anchor: Rect) -> Rect {
    let mut ids = [0u32; 16];
    let mut count = 0u32;
    let status = unsafe { CGGetActiveDisplayList(16, ids.as_mut_ptr(), &raw mut count) };
    let screens = if status == 0 {
        ids.iter()
            .take(count as usize)
            .map(|id| unsafe { CGDisplayBounds(*id) })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let center_x = anchor.x + anchor.width / 2.0;
    let center_y = anchor.y + anchor.height / 2.0;
    let found = screens.iter().find(|screen| {
        center_x >= screen.origin.x
            && center_y >= screen.origin.y
            && center_x < screen.origin.x + screen.size.width
            && center_y < screen.origin.y + screen.size.height
    });
    let screen = found.copied().unwrap_or(CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize {
            width: 1440.0,
            height: primary_height(),
        },
    });
    Rect {
        x: screen.origin.x,
        y: screen.origin.y,
        width: screen.size.width,
        height: screen.size.height,
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
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> u8;
    fn AXUIElementCreateSystemWide() -> *mut std::ffi::c_void;
    fn AXUIElementCopyAttributeValue(
        element: *mut std::ffi::c_void,
        attribute: *const std::ffi::c_void,
        value: *mut *const std::ffi::c_void,
    ) -> i32;
    fn AXUIElementCopyParameterizedAttributeValue(
        element: *mut std::ffi::c_void,
        attribute: *const std::ffi::c_void,
        parameter: *const std::ffi::c_void,
        value: *mut *const std::ffi::c_void,
    ) -> i32;
    fn AXValueGetValue(
        value: *const std::ffi::c_void,
        value_type: u32,
        out: *mut std::ffi::c_void,
    ) -> u8;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: *const std::ffi::c_void);
    fn CFStringCreateWithCString(
        allocator: *const std::ffi::c_void,
        text: *const i8,
        encoding: u32,
    ) -> *const std::ffi::c_void;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGMainDisplayID() -> u32;
    fn CGDisplayBounds(display: u32) -> CGRect;
    fn CGGetActiveDisplayList(max: u32, displays: *mut u32, count: *mut u32) -> i32;
    fn CGEventCreate(source: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    fn CGEventGetLocation(event: *mut std::ffi::c_void) -> CGPoint;
}

#[cfg(test)]
mod tests {
    use super::{Rect, ax_to_appkit, place_beside, place_under_bar, place_under_icon};

    #[cfg(target_os = "macos")]
    #[test]
    fn display_geometry_matches_the_pointer() {
        let height = super::primary_height();
        assert!((100.0..20_000.0).contains(&height), "{height}");
        let mouse = super::mouse_rect();
        assert!(mouse.x.is_finite() && mouse.y.is_finite(), "{mouse:?}");
        let screen = super::screen_containing(mouse);
        assert!(screen.width > 100.0 && screen.height > 100.0, "{screen:?}");
        let (x, y) = place_beside(mouse, 440.0, 560.0, screen);
        assert!(x >= screen.x && y >= screen.y, "{x} {y} {screen:?}");
        assert!(x + 440.0 <= screen.x + screen.width + 1.0);
        assert!(y + 560.0 <= screen.y + screen.height + 1.0);
    }

    #[test]
    fn flips_accessibility_coordinates_onto_the_appkit_screen() {
        let rect = ax_to_appkit(
            Rect {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 30.0,
            },
            900.0,
        );
        assert_eq!(
            rect,
            Rect {
                x: 10.0,
                y: 850.0,
                width: 100.0,
                height: 30.0
            }
        );
    }

    #[test]
    fn places_the_panel_beside_the_input_and_keeps_it_on_screen() {
        let screen = Rect {
            x: 0.0,
            y: 0.0,
            width: 1440.0,
            height: 900.0,
        };
        let (x, y) = place_beside(
            Rect {
                x: 100.0,
                y: 400.0,
                width: 240.0,
                height: 32.0,
            },
            440.0,
            560.0,
            screen,
        );
        assert_eq!((x, y), (348.0, 8.0));
        let (x, y) = place_beside(
            Rect {
                x: 100.0,
                y: 500.0,
                width: 240.0,
                height: 32.0,
            },
            280.0,
            120.0,
            screen,
        );
        assert_eq!((x, y), (348.0, 412.0));
        let (x, y) = place_beside(
            Rect {
                x: 1300.0,
                y: 400.0,
                width: 80.0,
                height: 32.0,
            },
            440.0,
            560.0,
            screen,
        );
        assert_eq!(x, 852.0);
        assert!(y >= 8.0);
        let (x, y) = place_beside(
            Rect {
                x: 10.0,
                y: 10.0,
                width: 40.0,
                height: 20.0,
            },
            440.0,
            560.0,
            screen,
        );
        assert!(x >= 8.0 && y >= 8.0);
        assert!(x + 440.0 <= 1432.0 && y + 560.0 <= 892.0);
    }

    #[test]
    fn places_the_panel_under_the_menu_bar() {
        let area = Rect {
            x: 0.0,
            y: 0.0,
            width: 1440.0,
            height: 876.0,
        };
        assert_eq!(place_under_bar(1400.0, 440.0, 560.0, area), (992.0, 308.0));
        assert_eq!(place_under_bar(720.0, 440.0, 560.0, area), (500.0, 308.0));
    }

    #[test]
    fn places_the_panel_under_a_tray_icon_or_above_a_bottom_taskbar() {
        let screen = Rect {
            x: 0.0,
            y: 0.0,
            width: 1440.0,
            height: 900.0,
        };
        assert_eq!(
            place_under_icon(
                Rect {
                    x: 1200.0,
                    y: 0.0,
                    width: 24.0,
                    height: 24.0,
                },
                440.0,
                560.0,
                screen,
            ),
            (992.0, 32.0)
        );
        assert_eq!(
            place_under_icon(
                Rect {
                    x: 100.0,
                    y: 860.0,
                    width: 40.0,
                    height: 32.0,
                },
                440.0,
                200.0,
                screen,
            ),
            (8.0, 652.0)
        );
    }
}
