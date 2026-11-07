use std::io::Read;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread::{self, JoinHandle};

use wl_clipboard_rs::copy::{self, MimeSource, MimeType as CopyMime, Options, Source};
use wl_clipboard_rs::paste::{self, ClipboardType, MimeType, Seat, get_contents, get_mime_types};
use wl_clipboard_rs::utils::PASSWORD_MANAGER_HINT_MIME_TYPE;
use wl_clipboard_rs::watch::{ClipboardEvent, ClipboardType as WatchClipboard, Watcher};

use super::{KindGate, MonitorEvent};
use crate::clipboard::error::ClipboardError;
use crate::clipboard::model::{Capture, Payload, PlatformCaps};
use crate::clipboard::util::{
    decode_png, kde_secret, parse_gnome_copied, parse_uri_list, uri_list,
};

pub struct WaylandBackend {
    recording: Arc<AtomicBool>,
    kinds: Arc<KindGate>,
    _thread: JoinHandle<()>,
}

impl WaylandBackend {
    pub fn start(events: Sender<MonitorEvent>, recording: Arc<AtomicBool>) -> Result<Self, String> {
        Watcher::new(WatchClipboard::Regular, Seat::Unspecified).map_err(watch_reason)?;
        let flag = Arc::clone(&recording);
        let kinds = Arc::new(KindGate::new());
        let kinds_thread = Arc::clone(&kinds);
        let thread = thread::Builder::new()
            .name("pixo-clipboard".into())
            .spawn(move || watch_loop(events, flag, kinds_thread))
            .map_err(|_| "wayland_no_data_control".to_string())?;
        Ok(Self {
            recording,
            kinds,
            _thread: thread,
        })
    }

    pub fn capabilities(&self) -> PlatformCaps {
        PlatformCaps {
            platform: "linux-wayland",
            can_monitor: true,
            can_read_source_app: false,
            can_global_shortcut: false,
            reason: None,
        }
    }

    pub fn set_recording(&self, recording: bool) {
        self.recording.store(recording, Ordering::SeqCst);
    }

    pub fn set_kinds(&self, text: bool, image: bool, files: bool) {
        self.kinds.set(text, image, files);
    }

    pub fn read_now(&self) -> Result<Capture, ClipboardError> {
        let mimes = get_mime_types(ClipboardType::Regular, Seat::Unspecified).map_err(map_paste)?;
        let kinds = Arc::clone(&self.kinds);
        read_selected(&mimes.into_iter().collect::<Vec<_>>(), &kinds, |mime| {
            read_mime(mime)
        })
    }

    pub fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
        Options::new()
            .copy(Source::Bytes(text.as_bytes().into()), CopyMime::Text)
            .map_err(map_copy)
    }

    pub fn write_image(&self, width: u32, height: u32, rgba: &[u8]) -> Result<(), ClipboardError> {
        let png = crate::clipboard::util::encode_png(width, height, rgba)
            .map_err(|_| ClipboardError::new("storage"))?;
        Options::new()
            .copy(
                Source::Bytes(png.into_boxed_slice()),
                CopyMime::Specific("image/png".into()),
            )
            .map_err(map_copy)
    }

    pub fn write_files(&self, paths: &[String]) -> Result<(), ClipboardError> {
        let list = uri_list(paths);
        let text = paths.join("\n");
        Options::new()
            .copy_multi(vec![
                MimeSource {
                    source: Source::Bytes(list.into_bytes().into_boxed_slice()),
                    mime_type: CopyMime::Specific("text/uri-list".into()),
                },
                MimeSource {
                    source: Source::Bytes(text.into_bytes().into_boxed_slice()),
                    mime_type: CopyMime::Text,
                },
            ])
            .map_err(map_copy)
    }
}

fn watch_loop(events: Sender<MonitorEvent>, recording: Arc<AtomicBool>, kinds: Arc<KindGate>) {
    let mut watcher = match Watcher::new(WatchClipboard::Regular, Seat::Unspecified) {
        Ok(watcher) => watcher,
        Err(_) => {
            let _ = events.send(MonitorEvent::Error("unsupported"));
            return;
        }
    };
    let mut initial = true;
    loop {
        match watcher.next_event() {
            Ok(None) => break,
            Ok(Some(ClipboardEvent::Cleared { .. })) => {
                initial = false;
            }
            Ok(Some(ClipboardEvent::Changed {
                mime_types,
                mut offer,
                ..
            })) => {
                if initial {
                    initial = false;
                    continue;
                }
                if !recording.load(Ordering::SeqCst) {
                    continue;
                }
                let capture = read_selected(&mime_types, &kinds, |mime| {
                    let mut pipe = offer.receive(mime).map_err(map_paste)?;
                    let mut data = Vec::new();
                    pipe.read_to_end(&mut data)
                        .map_err(|_| ClipboardError::new("busy"))?;
                    Ok(data)
                });
                let event = match capture {
                    Ok(capture) => MonitorEvent::Capture(capture),
                    Err(error) => MonitorEvent::Error(error.code),
                };
                if events.send(event).is_err() {
                    break;
                }
            }
            Err(_) => {
                let _ = events.send(MonitorEvent::Error("unsupported"));
                break;
            }
        }
    }
}

fn read_selected(
    mimes: &[String],
    kinds: &KindGate,
    mut read: impl FnMut(&str) -> Result<Vec<u8>, ClipboardError>,
) -> Result<Capture, ClipboardError> {
    if mimes
        .iter()
        .any(|mime| mime.eq_ignore_ascii_case(PASSWORD_MANAGER_HINT_MIME_TYPE))
    {
        let hint = read(PASSWORD_MANAGER_HINT_MIME_TYPE).unwrap_or_default();
        if kde_secret(&String::from_utf8_lossy(&hint)) {
            return Ok(Capture {
                payload: None,
                sensitive: true,
                source_app: None,
            });
        }
    }
    let Some(mime) = choose_mime(mimes, kinds) else {
        return Ok(Capture {
            payload: None,
            sensitive: false,
            source_app: None,
        });
    };
    let data = read(mime)?;
    Ok(Capture {
        payload: payload_for_mime(mime, &data),
        sensitive: false,
        source_app: None,
    })
}

fn choose_mime(mimes: &[String], kinds: &KindGate) -> Option<&'static str> {
    let mut preferred = Vec::new();
    if kinds.files() {
        preferred.extend(["x-special/gnome-copied-files", "text/uri-list"]);
    }
    if kinds.image() {
        preferred.push("image/png");
    }
    if kinds.text() {
        preferred.extend(["text/plain;charset=utf-8", "text/plain", "UTF8_STRING"]);
    }
    preferred.into_iter().find(|candidate| {
        mimes
            .iter()
            .any(|mime| mime.eq_ignore_ascii_case(candidate))
    })
}

fn payload_for_mime(mime: &str, data: &[u8]) -> Option<Payload> {
    if mime.eq_ignore_ascii_case("image/png") {
        return decode_png(data)
            .ok()
            .map(|(width, height, rgba)| Payload::Image {
                width,
                height,
                rgba,
            });
    }
    let text = String::from_utf8_lossy(data);
    if mime.eq_ignore_ascii_case("x-special/gnome-copied-files") {
        let files = parse_gnome_copied(&text);
        return (!files.is_empty()).then_some(Payload::Files(files));
    }
    if mime.eq_ignore_ascii_case("text/uri-list") {
        let files = parse_uri_list(&text);
        return (!files.is_empty()).then_some(Payload::Files(files));
    }
    let text = text.trim_end_matches('\0').to_string();
    (!text.is_empty()).then_some(Payload::Text(text))
}

fn read_mime(mime: &str) -> Result<Vec<u8>, ClipboardError> {
    let (mut pipe, _) = get_contents(
        ClipboardType::Regular,
        Seat::Unspecified,
        MimeType::Specific(mime),
    )
    .map_err(map_paste)?;
    let mut data = Vec::new();
    pipe.read_to_end(&mut data)
        .map_err(|_| ClipboardError::new("busy"))?;
    Ok(data)
}

fn watch_reason(error: paste::Error) -> String {
    match error {
        paste::Error::MissingProtocol { .. } => "wayland_no_data_control".into(),
        _ => "wayland_no_data_control".into(),
    }
}

fn map_paste(error: paste::Error) -> ClipboardError {
    match error {
        paste::Error::MissingProtocol { .. } => ClipboardError::with_params(
            "unsupported",
            serde_json::json!({ "reason": "wayland_no_data_control" }),
        ),
        paste::Error::ClipboardEmpty | paste::Error::NoMimeType | paste::Error::NoSeats => {
            ClipboardError::new("not_found")
        }
        other => {
            eprintln!("clipboard wayland: {other}");
            ClipboardError::new("busy")
        }
    }
}

fn map_copy(error: copy::Error) -> ClipboardError {
    match error {
        copy::Error::MissingProtocol { .. } => ClipboardError::with_params(
            "unsupported",
            serde_json::json!({ "reason": "wayland_no_data_control" }),
        ),
        other => {
            eprintln!("clipboard wayland write: {other}");
            ClipboardError::new("busy")
        }
    }
}
