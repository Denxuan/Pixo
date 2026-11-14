use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{AnyThread, ClassType, MainThreadMarker};
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSCompositingOperation, NSDeviceRGBColorSpace,
    NSGraphicsContext, NSImage, NSPasteboard, NSPasteboardAccessBehavior, NSPasteboardTypeFileURL,
    NSPasteboardTypeString, NSPasteboardWriting, NSWorkspace,
};
use objc2_foundation::{
    NSArray, NSData, NSDictionary, NSFileManager, NSPoint, NSRect, NSSize, NSString, NSURL,
};

use super::{KindGate, MonitorEvent, Platform};
use crate::clipboard::error::ClipboardError;
use crate::clipboard::io;
use crate::clipboard::model::{Capture, InstalledApp, Payload, PlatformCaps, RichText, SourceMark};
use crate::clipboard::util::{
    decode_image_bytes, macos_sensitive, opaque_cache_file, path_to_file_uri,
};

pub struct MacPlatform {
    recording: Arc<AtomicBool>,
    kinds: Arc<KindGate>,
    _thread: JoinHandle<()>,
}

impl MacPlatform {
    pub fn start(events: Sender<MonitorEvent>, recording: Arc<AtomicBool>) -> Self {
        let flag = Arc::clone(&recording);
        let kinds = Arc::new(KindGate::new());
        let kinds_thread = Arc::clone(&kinds);
        let thread = thread::Builder::new()
            .name("pixo-clipboard".into())
            .spawn(move || poll_loop(events, flag, kinds_thread))
            .expect("clipboard monitor thread");
        Self {
            recording,
            kinds,
            _thread: thread,
        }
    }
}

impl Platform for MacPlatform {
    fn capabilities(&self) -> PlatformCaps {
        PlatformCaps {
            platform: "macos",
            can_monitor: true,
            can_read_source_app: true,
            can_global_shortcut: true,
            reason: None,
        }
    }

    fn set_recording(&self, recording: bool) {
        self.recording.store(recording, Ordering::SeqCst);
    }

    fn set_kinds(&self, text: bool, image: bool, files: bool) {
        self.kinds.set(text, image, files);
    }

    fn read_now(&self) -> Result<Capture, ClipboardError> {
        let files = self.kinds.files();
        let image = self.kinds.image();
        let text = self.kinds.text();
        on_main(move || read_pasteboard(files, image, text))
    }

    fn write_text(&self, text: &RichText) -> Result<(), ClipboardError> {
        let text = text.clone();
        on_main(move || write_rich(&text))
    }

    fn write_image(&self, width: u32, height: u32, rgba: &[u8]) -> Result<(), ClipboardError> {
        let rgba = rgba.to_vec();
        on_main(move || io::write_image(width, height, &rgba))
    }

    fn write_files(&self, paths: &[String]) -> Result<(), ClipboardError> {
        let paths = paths.to_vec();
        on_main(move || write_files(&paths))
    }

    fn prepare_file_path(&self, path: &str) -> Option<String> {
        let path = path.to_string();
        on_main(move || filesystem_path(&path))
    }

    fn access(&self) -> Option<String> {
        on_main(access_behavior)
    }
}

fn poll_loop(events: Sender<MonitorEvent>, recording: Arc<AtomicBool>, kinds: Arc<KindGate>) {
    let mut last_change = on_main(change_count);
    loop {
        thread::sleep(Duration::from_millis(500));
        let change = on_main(change_count);
        if change == last_change {
            continue;
        }
        last_change = change;
        if !recording.load(Ordering::SeqCst) {
            continue;
        }
        let files = kinds.files();
        let image = kinds.image();
        let text = kinds.text();
        let event = match on_main(move || read_pasteboard(files, image, text)) {
            Ok(capture) => MonitorEvent::Capture(capture),
            Err(error) => MonitorEvent::Error(error.code),
        };
        if events.send(event).is_err() {
            break;
        }
    }
}

fn on_main<T: Send>(work: impl FnOnce() -> T + Send) -> T {
    if MainThreadMarker::new().is_some() {
        return work();
    }
    let (sender, receiver) = mpsc::sync_channel(1);
    DispatchQueue::main().exec_sync(move || {
        let _ = sender.send(work());
    });
    receiver
        .recv()
        .expect("main queue returned the clipboard result")
}

fn change_count() -> isize {
    NSPasteboard::generalPasteboard().changeCount()
}

fn access_behavior() -> Option<String> {
    let behavior = NSPasteboard::generalPasteboard().accessBehavior();
    Some(
        match behavior {
            NSPasteboardAccessBehavior::AlwaysDeny => "deny",
            NSPasteboardAccessBehavior::AlwaysAllow => "allow",
            NSPasteboardAccessBehavior::Ask => "ask",
            _ => "default",
        }
        .to_string(),
    )
}

fn read_pasteboard(
    files_enabled: bool,
    image_enabled: bool,
    text_enabled: bool,
) -> Result<Capture, ClipboardError> {
    if access_behavior().as_deref() == Some("deny") {
        return Err(ClipboardError::new("permission_denied"));
    }
    let pasteboard = NSPasteboard::generalPasteboard();
    let source_app = frontmost_bundle();
    let types = pasteboard_types(&pasteboard);
    if macos_sensitive(&types) {
        return Ok(Capture {
            payload: None,
            sensitive: true,
            source_app,
            types: types.clone(),
        });
    }
    if files_enabled {
        if let Some(image) = image_from_opaque_file(&pasteboard) {
            return Ok(Capture {
                payload: Some(image),
                sensitive: false,
                source_app,
                types: types.clone(),
            });
        }
        let files = file_paths(&pasteboard);
        if !files.is_empty() {
            return Ok(Capture {
                payload: Some(Payload::Files(files)),
                sensitive: false,
                source_app,
                types: types.clone(),
            });
        }
    }
    let mut payload = io::read_text_or_image_filtered(image_enabled, text_enabled)?;
    if text_enabled {
        if let Some(Payload::Text(text)) = &mut payload {
            text.html = pasteboard_string(&pasteboard, "public.html")
                .and_then(|html| crate::clipboard::rich::html_from_windows(html.as_bytes()));
            text.rtf = pasteboard_data(&pasteboard, "public.rtf");
        }
    }
    Ok(Capture {
        payload,
        sensitive: false,
        source_app,
        types,
    })
}

fn pasteboard_string(pasteboard: &NSPasteboard, name: &str) -> Option<String> {
    let kind = NSString::from_str(name);
    let value = pasteboard.stringForType(&kind)?.to_string();
    (!value.trim().is_empty()).then_some(value)
}

fn pasteboard_data(pasteboard: &NSPasteboard, name: &str) -> Option<Vec<u8>> {
    let kind = NSString::from_str(name);
    let bytes = pasteboard.dataForType(&kind)?.to_vec();
    (!bytes.is_empty()).then_some(bytes)
}

fn write_rich(text: &RichText) -> Result<(), ClipboardError> {
    if !text.formatted() {
        return io::write_text(&text.plain);
    }
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    let plain = NSString::from_str(&text.plain);
    let string_type = unsafe { &*NSPasteboardTypeString };
    if !pasteboard.setString_forType(&plain, string_type) {
        return Err(ClipboardError::new("busy"));
    }
    if let Some(html) = &text.html {
        let kind = NSString::from_str("public.html");
        let value = NSString::from_str(html);
        if !pasteboard.setString_forType(&value, &kind) {
            return Err(ClipboardError::new("busy"));
        }
    }
    if let Some(rtf) = &text.rtf {
        let kind = NSString::from_str("public.rtf");
        let data = unsafe { NSData::dataWithBytes_length(rtf.as_ptr().cast(), rtf.len()) };
        if !pasteboard.setData_forType(Some(&data), &kind) {
            return Err(ClipboardError::new("busy"));
        }
    }
    Ok(())
}

fn pasteboard_types(pasteboard: &NSPasteboard) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(types) = pasteboard.types() {
        for index in 0..types.count() {
            names.push(types.objectAtIndex(index).to_string());
        }
    }
    if let Some(items) = pasteboard.pasteboardItems() {
        for index in 0..items.count() {
            let item_types = items.objectAtIndex(index).types();
            for type_index in 0..item_types.count() {
                names.push(item_types.objectAtIndex(type_index).to_string());
            }
        }
    }
    names
}

const OPAQUE_IMAGE_LIMIT: u64 = 40 * 1024 * 1024;

fn image_from_opaque_file(pasteboard: &NSPasteboard) -> Option<Payload> {
    let urls = pasteboard_file_urls(pasteboard);
    let path = if urls.len() == 1 {
        urls[0].path()?.to_string()
    } else if urls.is_empty() {
        let paths = file_paths(pasteboard);
        if paths.len() != 1 {
            return None;
        }
        paths.into_iter().next()?
    } else {
        return None;
    };
    if !opaque_cache_file(&path) {
        return None;
    }
    if let Some(url) = urls.first() {
        if let Some(bytes) = read_url_bytes(url) {
            if let Some(payload) = image_payload(&bytes) {
                return Some(payload);
            }
        }
    } else if let Some(data) = NSData::dataWithContentsOfFile(&NSString::from_str(&path)) {
        if let Some(payload) = image_payload(&data.to_vec()) {
            return Some(payload);
        }
    }
    image_payload(&pasteboard_tiff(pasteboard)?)
}

fn image_payload(bytes: &[u8]) -> Option<Payload> {
    if bytes.is_empty() || bytes.len() as u64 > OPAQUE_IMAGE_LIMIT {
        return None;
    }
    let (width, height, rgba) = decode_image_bytes(bytes)?;
    Some(Payload::Image {
        width,
        height,
        rgba,
    })
}

fn pasteboard_file_urls(pasteboard: &NSPasteboard) -> Vec<Retained<NSURL>> {
    let classes = NSArray::from_slice(&[NSURL::class()]);
    let Some(objects) = (unsafe { pasteboard.readObjectsForClasses_options(&classes, None) })
    else {
        return Vec::new();
    };
    let mut urls = Vec::new();
    for index in 0..objects.count() {
        if let Ok(url) = objects.objectAtIndex(index).downcast::<NSURL>() {
            urls.push(url);
        }
    }
    urls
}

fn read_url_bytes(url: &NSURL) -> Option<Vec<u8>> {
    let started = unsafe { url.startAccessingSecurityScopedResource() };
    let too_big = url
        .path()
        .and_then(|path| std::fs::metadata(path.to_string()).ok())
        .is_some_and(|meta| meta.len() > OPAQUE_IMAGE_LIMIT);
    let data = if too_big {
        None
    } else {
        NSData::dataWithContentsOfURL(url).map(|data| data.to_vec())
    };
    if started {
        unsafe { url.stopAccessingSecurityScopedResource() };
    }
    data.filter(|bytes| !bytes.is_empty())
}

fn pasteboard_tiff(pasteboard: &NSPasteboard) -> Option<Vec<u8>> {
    if !NSImage::canInitWithPasteboard(pasteboard) {
        return None;
    }
    NSImage::initWithPasteboard(NSImage::alloc(), pasteboard)?
        .TIFFRepresentation()
        .map(|data| data.to_vec())
}

fn file_paths(pasteboard: &NSPasteboard) -> Vec<String> {
    let mut paths = Vec::new();
    let Some(items) = pasteboard.pasteboardItems() else {
        return paths;
    };
    let file_type = unsafe { &*NSPasteboardTypeFileURL };
    for index in 0..items.count() {
        let Some(value) = items.objectAtIndex(index).stringForType(file_type) else {
            continue;
        };
        if let Some(path) = filesystem_path(&value.to_string()) {
            paths.push(path);
        }
    }
    paths
}

pub fn source_mark(source: &str) -> Option<SourceMark> {
    let source = source.trim();
    if source.is_empty() || source.len() > 256 || source.chars().any(char::is_control) {
        return None;
    }
    let source = source.to_string();
    on_main(move || cached_source_mark(&source))
}

fn cached_source_mark(source: &str) -> Option<SourceMark> {
    static CACHE: Mutex<Option<HashMap<String, Option<SourceMark>>>> = Mutex::new(None);
    let mut guard = CACHE.lock().unwrap_or_else(|error| error.into_inner());
    let cache = guard.get_or_insert_with(HashMap::new);
    if let Some(mark) = cache.get(source) {
        return mark.clone();
    }
    let mark = lookup_source_mark(source);
    cache.insert(source.to_string(), mark.clone());
    mark
}

fn lookup_source_mark(source: &str) -> Option<SourceMark> {
    let workspace = NSWorkspace::sharedWorkspace();
    let url = workspace.URLForApplicationWithBundleIdentifier(&NSString::from_str(source))?;
    let path = url.path()?;
    let name = NSFileManager::defaultManager()
        .displayNameAtPath(&path)
        .to_string();
    let name = name.trim();
    let name = if name.is_empty() {
        source.to_string()
    } else {
        name.to_string()
    };
    let icon_png = icon_png(&workspace.iconForFile(&path)).unwrap_or_default();
    Some(SourceMark { name, icon_png })
}

fn icon_png(image: &NSImage) -> Option<Vec<u8>> {
    let rep = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            32,
            32,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            0,
            0,
        )
    }?;
    let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&rep)?;
    NSGraphicsContext::saveGraphicsState_class();
    NSGraphicsContext::setCurrentContext(Some(&context));
    image.drawInRect_fromRect_operation_fraction(
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(32.0, 32.0)),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0)),
        NSCompositingOperation::SourceOver,
        1.0,
    );
    NSGraphicsContext::restoreGraphicsState_class();
    let properties = NSDictionary::new();
    let png =
        unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &properties) }?;
    let bytes = png.to_vec();
    bytes.starts_with(b"\x89PNG").then_some(bytes)
}

fn frontmost_bundle() -> Option<String> {
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    let identifier = app.bundleIdentifier()?.to_string();
    if identifier.is_empty() {
        None
    } else {
        Some(identifier)
    }
}

fn filesystem_path(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let reference = if raw.len() >= 5 && raw[..5].eq_ignore_ascii_case("file:") {
        raw.to_string()
    } else if raw.starts_with("/.file/") {
        format!("file://{raw}")
    } else if raw.starts_with('/') {
        return Some(raw.to_string());
    } else {
        return None;
    };
    let url = NSURL::URLWithString(&NSString::from_str(&reference))?;
    let path = url
        .filePathURL()
        .and_then(|resolved| resolved.path())
        .or_else(|| url.path())?;
    let path = path.to_string();
    if path.is_empty() || path.starts_with("/.file/") {
        None
    } else {
        Some(path)
    }
}

fn file_url(path: &str) -> Option<Retained<NSURL>> {
    let path = filesystem_path(path)?;
    NSURL::URLWithString(&NSString::from_str(&path_to_file_uri(&path)))
}

fn write_files(paths: &[String]) -> Result<(), ClipboardError> {
    let urls: Vec<_> = paths.iter().filter_map(|path| file_url(path)).collect();
    if urls.is_empty() {
        return Err(ClipboardError::new("file_missing"));
    }
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    let writers: Vec<&ProtocolObject<dyn NSPasteboardWriting>> = urls
        .iter()
        .map(|url| ProtocolObject::from_ref::<NSURL>(&**url))
        .collect();
    let array = NSArray::from_slice(&writers);
    if !pasteboard.writeObjects(&array) {
        return Err(ClipboardError::new("busy"));
    }
    let text = NSString::from_str(&paths.join("\n"));
    let string_type = unsafe { &*NSPasteboardTypeString };
    if !pasteboard.setString_forType(&text, string_type) {
        return Err(ClipboardError::new("busy"));
    }
    Ok(())
}

pub fn installed_apps() -> Vec<InstalledApp> {
    let home = std::env::var("HOME").ok();
    let mut found = Vec::new();
    for root in application_roots(home.as_deref()) {
        collect_apps(&root, 0, &mut found);
    }
    let mut apps = Vec::new();
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    for (id, name, path) in found {
        if remember_app(&mut apps, &mut seen, id, name) {
            paths.push(path);
        }
    }
    for (app, icon) in apps.iter_mut().zip(app_icons(&paths)) {
        app.icon_png = icon;
    }
    apps.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then(left.id.cmp(&right.id))
    });
    apps
}

fn application_roots(home: Option<&str>) -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Library/CoreServices/Finder.app"),
    ];
    if let Some(home) = home.map(str::trim).filter(|home| !home.is_empty()) {
        roots.insert(1, PathBuf::from(home).join("Applications"));
    }
    roots
}

fn collect_apps(dir: &Path, depth: u8, found: &mut Vec<(String, String, String)>) {
    if found.len() > 2000 {
        return;
    }
    if is_app(dir) {
        if let Some(app) = read_installed_app(dir) {
            found.push(app);
        }
        return;
    }
    if depth > 2 {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        if found.len() > 2000 {
            return;
        }
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if file_name.starts_with('.') {
            continue;
        }
        if is_app(&path) {
            if let Some(app) = read_installed_app(&path) {
                found.push(app);
            }
            continue;
        }
        if path.is_dir() {
            collect_apps(&path, depth + 1, found);
        }
    }
}

fn is_app(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("app")
}

fn read_installed_app(path: &Path) -> Option<(String, String, String)> {
    let path_text = path.to_str()?;
    let id = bundle_identifier(&path.join("Contents/Info.plist"))?;
    let mut name = NSFileManager::defaultManager()
        .displayNameAtPath(&NSString::from_str(path_text))
        .to_string();
    if let Some(stripped) = name.strip_suffix(".app") {
        name = stripped.to_string();
    }
    let name = name.trim().to_string();
    let name = if name.is_empty() {
        path.file_stem()?.to_str()?.to_string()
    } else {
        name
    };
    Some((id, name, path_text.to_string()))
}

fn app_icons(paths: &[String]) -> Vec<Vec<u8>> {
    let workspace = NSWorkspace::sharedWorkspace();
    paths
        .iter()
        .map(|path| icon_png(&workspace.iconForFile(&NSString::from_str(path))).unwrap_or_default())
        .collect()
}

#[allow(deprecated)]
fn bundle_identifier(plist: &Path) -> Option<String> {
    let path = plist.to_str()?;
    let dict = unsafe {
        NSDictionary::<NSString, AnyObject>::dictionaryWithContentsOfFile(&NSString::from_str(
            path,
        ))?
    };
    let value = dict.objectForKey(&NSString::from_str("CFBundleIdentifier"))?;
    let text = value.downcast::<NSString>().ok()?;
    Some(text.to_string())
}

fn remember_app(
    apps: &mut Vec<InstalledApp>,
    seen: &mut HashSet<String>,
    id: String,
    name: String,
) -> bool {
    let id = id.trim().to_string();
    let name = name.trim().to_string();
    if id.is_empty()
        || name.is_empty()
        || id.chars().count() > 256
        || name.chars().count() > 256
        || id.chars().any(char::is_control)
        || name.chars().any(char::is_control)
        || !seen.insert(id.clone())
    {
        return false;
    }
    apps.push(InstalledApp {
        id,
        name,
        icon_png: Vec::new(),
    });
    true
}

#[cfg(test)]
mod tests {
    use objc2_foundation::{NSString, NSURL};

    use super::filesystem_path;

    #[test]
    fn resolves_file_urls_without_panicking() {
        assert_eq!(filesystem_path("/Users").as_deref(), Some("/Users"));
        assert_eq!(filesystem_path("file:///Users").as_deref(), Some("/Users"));
        assert_eq!(filesystem_path("file:///.file/id=missing").as_deref(), None);
        let url = NSURL::fileURLWithPath(&NSString::from_str("/Users"));
        let Some(reference) = url.fileReferenceURL() else {
            return;
        };
        let Some(absolute) = reference.absoluteString() else {
            return;
        };
        let resolved = filesystem_path(&absolute.to_string()).expect("file reference resolves");
        assert!(resolved.starts_with('/'));
        assert!(!resolved.starts_with("/.file/"));
    }

    #[test]
    fn application_roots_include_the_home_applications_folder() {
        let roots = super::application_roots(Some("/Users/example"));
        assert!(roots.iter().any(|path| path.ends_with("Applications")));
        assert!(
            roots
                .iter()
                .any(|path| path == std::path::Path::new("/Users/example/Applications"))
        );
    }

    #[test]
    fn remember_app_keeps_the_first_identifier_and_rejects_blank_names() {
        let mut apps = Vec::new();
        let mut seen = std::collections::HashSet::new();
        super::remember_app(
            &mut apps,
            &mut seen,
            "com.apple.Safari".into(),
            "Safari".into(),
        );
        super::remember_app(
            &mut apps,
            &mut seen,
            " com.apple.Safari ".into(),
            "Safari again".into(),
        );
        super::remember_app(
            &mut apps,
            &mut seen,
            "com.example.blank".into(),
            "  ".into(),
        );
        assert_eq!(
            apps,
            vec![super::InstalledApp {
                id: "com.apple.Safari".into(),
                name: "Safari".into(),
                icon_png: Vec::new(),
            }]
        );
    }

    #[test]
    fn installed_apps_include_safari_and_finder() {
        let apps = super::installed_apps();
        if std::path::Path::new("/Applications/Safari.app").exists() {
            let safari = apps
                .iter()
                .find(|app| app.id == "com.apple.Safari")
                .expect("Safari is installed");
            assert!(!safari.name.is_empty());
            assert_ne!(safari.name, safari.id);
            assert!(safari.icon_png.starts_with(b"\x89PNG"));
        }
        if std::path::Path::new("/System/Library/CoreServices/Finder.app").exists() {
            let finder = apps
                .iter()
                .find(|app| app.id == "com.apple.finder")
                .expect("Finder is installed");
            assert!(!finder.name.is_empty());
            assert!(finder.icon_png.starts_with(b"\x89PNG"));
        }
        if std::path::Path::new("/Applications").exists() {
            assert!(apps.len() > 5, "only found {}", apps.len());
        }
        assert!(apps.windows(2).all(|pair| {
            pair[0].name.to_lowercase() <= pair[1].name.to_lowercase()
                || pair[0].name.to_lowercase() == pair[1].name.to_lowercase()
                    && pair[0].id <= pair[1].id
        }));
    }

    #[test]
    fn source_mark_rejects_blank_identifiers() {
        assert!(super::source_mark("").is_none());
        assert!(super::source_mark("com.apple\n.finder").is_none());
    }
}
