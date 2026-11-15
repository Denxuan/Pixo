#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct RawWindow {
    pub bounds: Bounds,
    pub pid: u32,
    pub minimized: bool,
}

pub fn snap_windows(windows: &[RawWindow], monitor: Bounds, own_pid: u32) -> Vec<Bounds> {
    windows
        .iter()
        .filter(|window| {
            !window.minimized
                && window.pid != own_pid
                && window.bounds.width >= 64
                && window.bounds.height >= 64
                && intersects(window.bounds, monitor)
                && !covers_monitor(window.bounds, monitor, 8)
        })
        .map(|window| window.bounds)
        .collect()
}

fn intersects(window: Bounds, monitor: Bounds) -> bool {
    let right = window.x.saturating_add(window.width as i32);
    let bottom = window.y.saturating_add(window.height as i32);
    let monitor_right = monitor.x.saturating_add(monitor.width as i32);
    let monitor_bottom = monitor.y.saturating_add(monitor.height as i32);
    window.x < monitor_right && right > monitor.x && window.y < monitor_bottom && bottom > monitor.y
}

fn covers_monitor(window: Bounds, monitor: Bounds, slack: i32) -> bool {
    let window_right = window.x.saturating_add(window.width as i32);
    let window_bottom = window.y.saturating_add(window.height as i32);
    let monitor_right = monitor.x.saturating_add(monitor.width as i32);
    let monitor_bottom = monitor.y.saturating_add(monitor.height as i32);
    (window.x - monitor.x).abs() <= slack
        && (window.y - monitor.y).abs() <= slack
        && (window_right - monitor_right).abs() <= slack
        && (window_bottom - monitor_bottom).abs() <= slack
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(x: i32, y: i32, width: u32, height: u32) -> Bounds {
        Bounds {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn snap_skips_the_fullscreen_root_when_a_smaller_window_exists() {
        let monitor = bounds(0, 0, 1000, 800);
        let windows = [
            RawWindow {
                bounds: bounds(-2, -2, 1004, 804),
                pid: 1,
                minimized: false,
            },
            RawWindow {
                bounds: bounds(40, 40, 300, 200),
                pid: 2,
                minimized: false,
            },
        ];
        assert_eq!(
            snap_windows(&windows, monitor, 9),
            vec![bounds(40, 40, 300, 200)]
        );
    }

    #[test]
    fn snap_drops_this_process_and_tiny_windows() {
        let monitor = bounds(0, 0, 1000, 800);
        let windows = [
            RawWindow {
                bounds: bounds(10, 10, 200, 200),
                pid: 7,
                minimized: false,
            },
            RawWindow {
                bounds: bounds(20, 20, 20, 20),
                pid: 3,
                minimized: false,
            },
            RawWindow {
                bounds: bounds(30, 30, 200, 200),
                pid: 3,
                minimized: true,
            },
        ];
        assert!(snap_windows(&windows, monitor, 7).is_empty());
    }
}
