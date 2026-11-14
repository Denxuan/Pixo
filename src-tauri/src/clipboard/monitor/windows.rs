use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};

use windows::Win32::Foundation::{
    CloseHandle, HANDLE, HGLOBAL, HWND, LPARAM, LRESULT, POINT, WPARAM,
};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, EmptyClipboard, GetClipboardData,
    GetClipboardOwner, IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW,
    RemoveClipboardFormatListener, SetClipboardData,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, GetWindowThreadProcessId,
    HWND_MESSAGE, MSG, PostQuitMessage, RegisterClassW, TranslateMessage, WM_CLIPBOARDUPDATE,
    WM_QUIT, WNDCLASSW,
};
use windows::core::w;

use super::{KindGate, MonitorEvent, Platform};
use crate::clipboard::error::ClipboardError;
use crate::clipboard::io;
use crate::clipboard::model::{Capture, Payload, PlatformCaps, RichText};
use crate::clipboard::rich::html_from_windows;
use crate::clipboard::util::{decode_dib, windows_history_excluded};

const CF_DIB: u32 = 8;
const CF_UNICODETEXT: u32 = 13;
const CF_HDROP: u32 = 15;
const CF_DIBV5: u32 = 17;

struct ListenerState {
    events: Mutex<Option<Sender<MonitorEvent>>>,
    recording: AtomicBool,
    hwnd: Mutex<Option<usize>>,
    kinds: KindGate,
}

fn state() -> &'static ListenerState {
    static STATE: OnceLock<ListenerState> = OnceLock::new();
    STATE.get_or_init(|| ListenerState {
        events: Mutex::new(None),
        recording: AtomicBool::new(false),
        hwnd: Mutex::new(None),
        kinds: KindGate::new(),
    })
}

fn clipboard_lock() -> &'static Mutex<()> {
    static LOCK: Mutex<()> = Mutex::new(());
    &LOCK
}

pub struct WindowsPlatform {
    _thread: JoinHandle<()>,
}

impl WindowsPlatform {
    pub fn start(
        events: Sender<MonitorEvent>,
        recording: Arc<AtomicBool>,
    ) -> Result<Self, ClipboardError> {
        state()
            .recording
            .store(recording.load(Ordering::SeqCst), Ordering::SeqCst);
        // The service owns `recording`; mirror it into the listener so the window procedure can see it.
        let _recording = recording;
        *lock(&state().events) = Some(events);
        let thread = thread::Builder::new()
            .name("pixo-clipboard".into())
            .spawn(move || {
                if let Err(error) = listen() {
                    send_event(MonitorEvent::Error(error.code));
                }
            })
            .map_err(|_| ClipboardError::new("unsupported"))?;
        Ok(Self { _thread: thread })
    }
}

impl Platform for WindowsPlatform {
    fn capabilities(&self) -> PlatformCaps {
        PlatformCaps {
            platform: "windows",
            can_monitor: true,
            can_read_source_app: true,
            can_global_shortcut: true,
            reason: None,
        }
    }

    fn set_recording(&self, recording: bool) {
        state().recording.store(recording, Ordering::SeqCst);
    }

    fn set_kinds(&self, text: bool, image: bool, files: bool) {
        state().kinds.set(text, image, files);
    }

    fn read_now(&self) -> Result<Capture, ClipboardError> {
        with_clipboard(read_open_clipboard)
    }

    fn write_text(&self, text: &RichText) -> Result<(), ClipboardError> {
        if text.html.is_none() && text.rtf.is_none() {
            return io::write_text(&text.plain);
        }
        let text = text.clone();
        with_clipboard(move || write_rich_open(&text))
    }

    fn write_image(&self, width: u32, height: u32, rgba: &[u8]) -> Result<(), ClipboardError> {
        io::write_image(width, height, rgba)
    }

    fn write_files(&self, paths: &[String]) -> Result<(), ClipboardError> {
        with_clipboard(|| write_files_open(paths))
    }
}

fn listen() -> Result<(), ClipboardError> {
    unsafe {
        let class_name = w!("PixoClipboardListener");
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: windows::Win32::Foundation::HINSTANCE(
                GetModuleHandleW(None)
                    .map_err(|_| ClipboardError::new("unsupported"))?
                    .0,
            ),
            lpszClassName: class_name,
            ..WNDCLASSW::default()
        };
        if RegisterClassW(&class) == 0 {
            return Err(ClipboardError::new("unsupported"));
        }
        let hwnd = CreateWindowExW(
            Default::default(),
            class_name,
            w!("PixoClipboardListener"),
            Default::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            None,
            None,
        )
        .map_err(|_| ClipboardError::new("unsupported"))?;
        *lock(&state().hwnd) = Some(hwnd.0 as usize);
        AddClipboardFormatListener(hwnd).map_err(|_| ClipboardError::new("unsupported"))?;
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        let _ = RemoveClipboardFormatListener(hwnd);
        Ok(())
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_CLIPBOARDUPDATE {
        if state().recording.load(Ordering::SeqCst) {
            let event = match with_clipboard(read_open_clipboard) {
                Ok(capture) => MonitorEvent::Capture(capture),
                Err(error) => MonitorEvent::Error(error.code),
            };
            send_event(event);
        }
        return LRESULT(0);
    }
    if message == WM_QUIT {
        unsafe { PostQuitMessage(0) };
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

fn send_event(event: MonitorEvent) {
    if let Some(sender) = lock(&state().events).as_ref() {
        let _ = sender.send(event);
    }
}

fn with_clipboard<T>(
    read: impl FnOnce() -> Result<T, ClipboardError>,
) -> Result<T, ClipboardError> {
    let _guard = lock(clipboard_lock());
    let mut delay = 50u64;
    for attempt in 0..3 {
        match unsafe { OpenClipboard(None) } {
            Ok(()) => {
                let result = read();
                unsafe {
                    let _ = CloseClipboard();
                }
                return result;
            }
            Err(_) if attempt < 2 => {
                std::thread::sleep(std::time::Duration::from_millis(delay));
                delay *= 2;
            }
            Err(_) => return Err(ClipboardError::new("busy")),
        }
    }
    Err(ClipboardError::new("busy"))
}

fn read_open_clipboard() -> Result<Capture, ClipboardError> {
    unsafe {
        let exclude = RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing"));
        let can_include = RegisterClipboardFormatW(w!("CanIncludeInClipboardHistory"));
        let exclude_present = exclude != 0 && IsClipboardFormatAvailable(exclude).is_ok();
        let include_value = if can_include != 0 && IsClipboardFormatAvailable(can_include).is_ok() {
            read_dword(can_include)
        } else {
            None
        };
        let source_app = clipboard_owner();
        if windows_history_excluded(exclude_present, include_value) {
            return Ok(Capture {
                payload: None,
                sensitive: true,
                source_app,
                types: Vec::new(),
            });
        }
        let allow_files = state().kinds.files();
        let allow_image = state().kinds.image();
        let allow_text = state().kinds.text();
        if allow_files && IsClipboardFormatAvailable(CF_HDROP).is_ok() {
            let files = read_hdrop()?;
            if !files.is_empty() {
                return Ok(Capture {
                    payload: Some(Payload::Files(files)),
                    sensitive: false,
                    source_app,
                    types: Vec::new(),
                });
            }
        }
        if allow_image {
            if let Some(image) = read_image()? {
                return Ok(Capture {
                    payload: Some(image),
                    sensitive: false,
                    source_app,
                    types: Vec::new(),
                });
            }
        }
        if allow_text {
            let plain = read_text()?.unwrap_or_default();
            let html =
                read_registered(w!("HTML Format"))?.and_then(|bytes| html_from_windows(&bytes));
            let rtf = read_registered(w!("Rich Text Format"))?;
            let text = RichText { plain, html, rtf };
            if !text.plain.is_empty() || text.formatted() {
                return Ok(Capture {
                    payload: Some(Payload::Text(text)),
                    sensitive: false,
                    source_app,
                    types: Vec::new(),
                });
            }
        }
        Ok(Capture {
            payload: None,
            sensitive: false,
            source_app,
            types: Vec::new(),
        })
    }
}

unsafe fn read_dword(format: u32) -> Option<u32> {
    let handle = GetClipboardData(format).ok()?;
    let pointer = GlobalLock(HGLOBAL(handle.0));
    if pointer.is_null() {
        return None;
    }
    let value = *(pointer as *const u32);
    let _ = GlobalUnlock(HGLOBAL(handle.0));
    Some(value)
}

unsafe fn read_text() -> Result<Option<String>, ClipboardError> {
    let Ok(handle) = GetClipboardData(CF_UNICODETEXT) else {
        return Ok(None);
    };
    let pointer = GlobalLock(HGLOBAL(handle.0)) as *const u16;
    if pointer.is_null() {
        return Ok(None);
    }
    let mut length = 0usize;
    while *pointer.add(length) != 0 {
        length += 1;
        if length > 16 * 1024 * 1024 {
            break;
        }
    }
    let text = String::from_utf16_lossy(std::slice::from_raw_parts(pointer, length));
    let _ = GlobalUnlock(HGLOBAL(handle.0));
    Ok(Some(text))
}

unsafe fn read_hdrop() -> Result<Vec<String>, ClipboardError> {
    let Ok(handle) = GetClipboardData(CF_HDROP) else {
        return Ok(Vec::new());
    };
    let drop = HDROP(handle.0);
    let count = DragQueryFileW(drop, 0xFFFF_FFFF, None);
    let mut paths = Vec::new();
    for index in 0..count {
        let mut buffer = vec![0u16; 32_768];
        let copied = DragQueryFileW(drop, index, Some(&mut buffer));
        if copied == 0 {
            continue;
        }
        let end = usize::try_from(copied).unwrap_or(0).min(buffer.len());
        let path = String::from_utf16_lossy(&buffer[..end]);
        if !path.is_empty() {
            paths.push(path);
        }
    }
    Ok(paths)
}

unsafe fn read_image() -> Result<Option<Payload>, ClipboardError> {
    let png_format = RegisterClipboardFormatW(w!("PNG"));
    if png_format != 0 && IsClipboardFormatAvailable(png_format).is_ok() {
        if let Some(bytes) = read_bytes(png_format)? {
            if let Ok((width, height, rgba)) = crate::clipboard::util::decode_png(&bytes) {
                return Ok(Some(Payload::Image {
                    width,
                    height,
                    rgba,
                }));
            }
        }
    }
    for format in [CF_DIBV5, CF_DIB] {
        if IsClipboardFormatAvailable(format).is_ok() {
            if let Some(bytes) = read_bytes(format)? {
                if let Some((width, height, rgba)) = decode_dib(&bytes) {
                    return Ok(Some(Payload::Image {
                        width,
                        height,
                        rgba,
                    }));
                }
            }
        }
    }
    Ok(None)
}

unsafe fn read_bytes(format: u32) -> Result<Option<Vec<u8>>, ClipboardError> {
    let Ok(handle) = GetClipboardData(format) else {
        return Ok(None);
    };
    let pointer = GlobalLock(HGLOBAL(handle.0));
    if pointer.is_null() {
        return Ok(None);
    }
    let size = windows::Win32::System::Memory::GlobalSize(HGLOBAL(handle.0));
    let bytes = std::slice::from_raw_parts(pointer as *const u8, size).to_vec();
    let _ = GlobalUnlock(HGLOBAL(handle.0));
    Ok(Some(bytes))
}

unsafe fn clipboard_owner() -> Option<String> {
    let Ok(owner) = GetClipboardOwner() else {
        return None;
    };
    let mut process_id = 0u32;
    GetWindowThreadProcessId(owner, Some(&mut process_id));
    if process_id == 0 {
        return None;
    }
    let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) else {
        return None;
    };
    let mut buffer = [0u16; 1024];
    let mut size = buffer.len() as u32;
    let name = QueryFullProcessImageNameW(
        process,
        PROCESS_NAME_WIN32,
        windows::core::PWSTR(buffer.as_mut_ptr()),
        &mut size,
    )
    .ok()
    .map(|_| String::from_utf16_lossy(&buffer[..size as usize]));
    let _ = CloseHandle(process);
    name.filter(|path| !path.is_empty())
}

fn write_rich_open(text: &RichText) -> Result<(), ClipboardError> {
    unsafe {
        EmptyClipboard().map_err(|_| ClipboardError::new("busy"))?;
        set_unicode_text(&text.plain)?;
        if let Some(html) = &text.html {
            let format = RegisterClipboardFormatW(w!("HTML Format"));
            if format != 0 {
                set_bytes(format, &crate::clipboard::rich::windows_html(html))?;
            }
        }
        if let Some(rtf) = &text.rtf {
            let format = RegisterClipboardFormatW(w!("Rich Text Format"));
            if format != 0 {
                set_bytes(format, rtf)?;
            }
        }
        Ok(())
    }
}

unsafe fn read_registered(name: windows::core::PCWSTR) -> Result<Option<Vec<u8>>, ClipboardError> {
    let format = RegisterClipboardFormatW(name);
    if format == 0 || IsClipboardFormatAvailable(format).is_err() {
        return Ok(None);
    }
    read_bytes(format)
}

fn write_files_open(paths: &[String]) -> Result<(), ClipboardError> {
    unsafe {
        EmptyClipboard().map_err(|_| ClipboardError::new("busy"))?;
        set_unicode_text(&paths.join("\n"))?;
        set_hdrop(paths)?;
        Ok(())
    }
}

unsafe fn set_unicode_text(text: &str) -> Result<(), ClipboardError> {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = std::slice::from_raw_parts(wide.as_ptr().cast::<u8>(), wide.len() * 2);
    set_bytes(CF_UNICODETEXT, bytes)
}

unsafe fn set_bytes(format: u32, bytes: &[u8]) -> Result<(), ClipboardError> {
    let memory =
        GlobalAlloc(GMEM_MOVEABLE, bytes.len()).map_err(|_| ClipboardError::new("storage"))?;
    let pointer = GlobalLock(memory);
    if pointer.is_null() {
        return Err(ClipboardError::new("storage"));
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer.cast(), bytes.len());
    let _ = GlobalUnlock(memory);
    SetClipboardData(format, Some(HANDLE(memory.0))).map_err(|_| ClipboardError::new("busy"))?;
    Ok(())
}

unsafe fn set_hdrop(paths: &[String]) -> Result<(), ClipboardError> {
    let mut wide = Vec::<u16>::new();
    for path in paths {
        wide.extend(path.encode_utf16());
        wide.push(0);
    }
    wide.push(0);
    #[repr(C)]
    struct Header {
        files: u32,
        x: i32,
        y: i32,
        non_client: i32,
        wide: i32,
    }
    let header = Header {
        files: 20,
        x: 0,
        y: 0,
        non_client: 0,
        wide: 1,
    };
    let bytes = std::mem::size_of::<Header>() + wide.len() * 2;
    let memory = GlobalAlloc(GMEM_MOVEABLE, bytes).map_err(|_| ClipboardError::new("storage"))?;
    let pointer = GlobalLock(memory) as *mut u8;
    if pointer.is_null() {
        return Err(ClipboardError::new("storage"));
    }
    std::ptr::copy_nonoverlapping(
        &header as *const Header as *const u8,
        pointer,
        std::mem::size_of::<Header>(),
    );
    std::ptr::copy_nonoverlapping(
        wide.as_ptr() as *const u8,
        pointer.add(std::mem::size_of::<Header>()),
        wide.len() * 2,
    );
    let _ = GlobalUnlock(memory);
    SetClipboardData(CF_HDROP, Some(HANDLE(memory.0))).map_err(|_| ClipboardError::new("busy"))?;
    Ok(())
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

// The listener thread keeps the window alive for the process lifetime.
impl Drop for WindowsPlatform {
    fn drop(&mut self) {
        if let Some(hwnd) = lock(&state().hwnd).take() {
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    Some(HWND(hwnd as *mut _)),
                    WM_QUIT,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        }
    }
}
