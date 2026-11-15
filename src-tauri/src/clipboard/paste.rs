use std::thread;
use std::time::Duration;

use tauri::AppHandle;

#[cfg(target_os = "windows")]
use std::sync::Mutex;

#[cfg(target_os = "windows")]
static PREVIOUS_WINDOW: Mutex<isize> = Mutex::new(0);

pub fn remember_target() {
    #[cfg(target_os = "windows")]
    {
        let hwnd = unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
        let value = hwnd.0 as isize;
        if value == 0 {
            return;
        }
        if let Ok(mut slot) = PREVIOUS_WINDOW.lock() {
            *slot = value;
        }
    }
}

pub fn schedule(app: &AppHandle) -> Result<(), &'static str> {
    available()?;
    let app = app.clone();
    thread::Builder::new()
        .name("pixo-paste".into())
        .spawn(move || {
            let (sender, receiver) = std::sync::mpsc::channel();
            let dismissed = app.clone();
            let _ = app.run_on_main_thread(move || {
                crate::shortcut::dismiss_panel(&dismissed);
                let _ = sender.send(());
            });
            let _ = receiver.recv_timeout(Duration::from_millis(500));
            thread::sleep(Duration::from_millis(80));
            send_paste();
        })
        .map_err(|_| "unsupported")?;
    Ok(())
}

fn available() -> Result<(), &'static str> {
    #[cfg(target_os = "macos")]
    {
        return if unsafe { AXIsProcessTrusted() } != 0 {
            Ok(())
        } else {
            Err("permission_denied")
        };
    }
    #[cfg(target_os = "windows")]
    {
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        return linux_available();
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Err("unsupported")
    }
}

fn send_paste() {
    #[cfg(target_os = "macos")]
    send_command_v();
    #[cfg(target_os = "windows")]
    send_ctrl_v();
    #[cfg(target_os = "linux")]
    send_linux_paste();
}

#[cfg(target_os = "macos")]
fn send_command_v() {
    unsafe {
        let source = CGEventSourceCreate(0);
        let down = CGEventCreateKeyboardEvent(source, 9, true);
        let up = CGEventCreateKeyboardEvent(source, 9, false);
        if !down.is_null() && !up.is_null() {
            CGEventSetFlags(down, 0x0010_0000);
            CGEventSetFlags(up, 0x0010_0000);
            CGEventPost(0, down);
            CGEventPost(0, up);
        }
        if !down.is_null() {
            CFRelease(down);
        }
        if !up.is_null() {
            CFRelease(up);
        }
        if !source.is_null() {
            CFRelease(source);
        }
    }
}

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> u8;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventSourceCreate(state: u32) -> *mut std::ffi::c_void;
    fn CGEventCreateKeyboardEvent(
        source: *mut std::ffi::c_void,
        keycode: u16,
        key_down: bool,
    ) -> *mut std::ffi::c_void;
    fn CGEventSetFlags(event: *mut std::ffi::c_void, flags: u64);
    fn CGEventPost(tap: u32, event: *mut std::ffi::c_void);
}

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: *const std::ffi::c_void);
}

#[cfg(target_os = "windows")]
fn send_ctrl_v() {
    use std::ffi::c_void;

    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
        VIRTUAL_KEY, VK_CONTROL, VK_V,
    };
    use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

    let value = PREVIOUS_WINDOW.lock().map(|slot| *slot).unwrap_or(0);
    if value != 0 {
        unsafe {
            let _ = SetForegroundWindow(HWND(value as *mut c_void));
        }
        thread::sleep(Duration::from_millis(40));
    }
    let inputs = [
        key_input(VK_CONTROL, false),
        key_input(VK_V, false),
        key_input(VK_V, true),
        key_input(VK_CONTROL, true),
    ];
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }

    fn key_input(vk: VIRTUAL_KEY, up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: if up {
                        KEYEVENTF_KEYUP
                    } else {
                        KEYBD_EVENT_FLAGS(0)
                    },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy)]
enum PasteTool {
    Xdotool,
    Wtype,
    Ydotool,
}

#[cfg(target_os = "linux")]
fn linux_available() -> Result<(), &'static str> {
    if paste_tool().is_some() {
        Ok(())
    } else {
        Err("unsupported")
    }
}

#[cfg(target_os = "linux")]
fn send_linux_paste() {
    let Some(tool) = paste_tool() else {
        return;
    };
    let mut command = std::process::Command::new(match tool {
        PasteTool::Xdotool => "xdotool",
        PasteTool::Wtype => "wtype",
        PasteTool::Ydotool => "ydotool",
    });
    match tool {
        PasteTool::Xdotool => {
            command.args(["key", "--clearmodifiers", "ctrl+v"]);
        }
        PasteTool::Wtype => {
            command.args(["-M", "ctrl", "v", "-m", "ctrl"]);
        }
        PasteTool::Ydotool => {
            command.args(["key", "29:1", "47:1", "47:0", "29:0"]);
        }
    }
    let _ = command.status();
}

#[cfg(target_os = "linux")]
fn paste_tool() -> Option<PasteTool> {
    static TOOL: std::sync::OnceLock<Option<PasteTool>> = std::sync::OnceLock::new();
    *TOOL.get_or_init(|| {
        if wayland_without_x() {
            if tool_exists("wtype") {
                return Some(PasteTool::Wtype);
            }
            if tool_exists("ydotool") {
                return Some(PasteTool::Ydotool);
            }
            return None;
        }
        tool_exists("xdotool").then_some(PasteTool::Xdotool)
    })
}

#[cfg(target_os = "linux")]
fn wayland_without_x() -> bool {
    std::env::var_os("DISPLAY").is_none()
        && (std::env::var_os("WAYLAND_DISPLAY").is_some()
            || std::env::var("XDG_SESSION_TYPE").ok().as_deref() == Some("wayland"))
}

#[cfg(target_os = "linux")]
fn tool_exists(name: &str) -> bool {
    std::process::Command::new(name)
        .arg("--help")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}
