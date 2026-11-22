use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};

use super::error::{ClipboardError, log_clipboard};
use super::model::{
    Capture, ClipKind, CopyOutcome, Detail, ListPage, Payload, Settings, SettingsUpdate, Status,
    Summary,
};
use super::monitor::{self, MonitorEvent};
use super::store::{self, Store, UpsertRequest, UpsertResult};
use super::util::{
    PRODUCED_IMAGE_LIMIT, app_excluded, decode_png, encode_png, files_size_bytes, gif_still_png,
    hash_files, hash_image, image_rgba_len, is_gif, now_millis, png_dimensions, thumbnail_png,
    type_ignored,
};
use crate::shortcut;

struct SelfWrite {
    hash: String,
    at: Instant,
}

struct Inner {
    store: Mutex<Store>,
    settings: Mutex<Settings>,
    counters: Mutex<super::model::Counters>,
    self_write: Mutex<Option<SelfWrite>>,
    platform: Box<dyn monitor::Platform>,
    images_dir: PathBuf,
    app: AppHandle,
    storage_path: String,
    last_error: Mutex<Option<&'static str>>,
    registered_shortcut: Mutex<Option<String>>,
    ignore_next: AtomicBool,
}

pub struct ClipboardState {
    inner: Arc<Inner>,
}

pub fn initialize(app: AppHandle) -> Result<ClipboardState, ClipboardError> {
    let root = app.path().app_data_dir().map_err(|error| {
        eprintln!("clipboard path: {error}");
        ClipboardError::new("storage")
    })?;
    let (database, images_dir) = store::prepare_layout(&root)?;
    let mut store = Store::open(&database)?;
    let settings = store.load_settings()?;
    store.save_settings(&settings)?;
    let (sender, receiver) = mpsc::channel();
    let platform = monitor::start(sender, Arc::new(std::sync::atomic::AtomicBool::new(false)))?;
    platform.set_kinds(
        settings.record_text,
        settings.record_image,
        settings.record_files,
    );
    let caps = platform.capabilities();
    platform.set_recording(settings.enabled && !settings.paused && caps.can_monitor);
    let pruned = store.apply_prune(&settings, now_millis())?;
    remove_images(&images_dir, &pruned.image_hashes);
    if let Ok(hashes) = store.image_hashes() {
        remove_orphan_images(&images_dir, &hashes);
    }
    let storage_path = database
        .parent()
        .unwrap_or(root.as_path())
        .to_string_lossy()
        .into_owned();
    let inner = Arc::new(Inner {
        store: Mutex::new(store),
        settings: Mutex::new(settings),
        counters: Mutex::new(super::model::Counters::default()),
        self_write: Mutex::new(None),
        platform,
        images_dir,
        app,
        storage_path,
        last_error: Mutex::new(None),
        registered_shortcut: Mutex::new(None),
        ignore_next: AtomicBool::new(false),
    });
    if let Err(error) = inner.sync_shortcut() {
        eprintln!("clipboard shortcut: {}", error.code);
    }
    let thread_state = Arc::clone(&inner);
    thread::Builder::new()
        .name("pixo-clipboard-store".into())
        .spawn(move || ingest_loop(thread_state, receiver))
        .map_err(|error| {
            eprintln!("clipboard ingest: {error}");
            ClipboardError::new("storage")
        })?;
    Ok(ClipboardState { inner })
}

fn ingest_loop(inner: Arc<Inner>, receiver: Receiver<MonitorEvent>) {
    while let Ok(event) = receiver.recv() {
        match event {
            MonitorEvent::Error(code) => {
                *lock(&inner.last_error) = Some(code);
                inner.emit_status();
            }
            MonitorEvent::Capture(capture) => {
                *lock(&inner.last_error) = None;
                if let Err(error) = inner.record(capture, false) {
                    log_clipboard(error.code, "record", 0);
                }
            }
        }
    }
}

impl ClipboardState {
    pub fn list(
        &self,
        query: Option<&str>,
        kind: Option<ClipKind>,
        pinned_only: bool,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<ListPage, ClipboardError> {
        lock(&self.inner.store).list(query, kind, pinned_only, cursor, limit)
    }

    pub fn get(&self, id: i64) -> Result<Detail, ClipboardError> {
        lock(&self.inner.store).get(id)
    }

    pub fn image(&self, id: i64, thumb: bool) -> Result<Vec<u8>, ClipboardError> {
        let hash = lock(&self.inner.store).image_hash(id)?;
        let (png, thumbnail) = store::image_paths(&self.inner.images_dir, &hash);
        let path = if thumb { thumbnail } else { png };
        fs::read(&path).map_err(|_| ClipboardError::new("not_found"))
    }

    pub fn copy(&self, id: i64, plain: bool) -> Result<CopyOutcome, ClipboardError> {
        let detail = lock(&self.inner.store).get(id)?;
        let outcome = match detail.kind.as_str() {
            "text" => {
                let mut text = lock(&self.inner.store).rich_text(id)?;
                if plain {
                    text.html = None;
                    text.rtf = None;
                }
                self.arm(&crate::clipboard::util::hash_rich(&text));
                if let Err(error) = self.inner.platform.write_text(&text) {
                    self.disarm();
                    return Err(error);
                }
                Ok(CopyOutcome::copied(Vec::new()))
            }
            "files" => {
                let mut writable = Vec::new();
                let mut missing = Vec::new();
                for path in &detail.files {
                    match self.inner.platform.prepare_file_path(path) {
                        Some(real) => {
                            if !Path::new(&real).exists() {
                                missing.push(path.clone());
                            }
                            writable.push(real);
                        }
                        None => missing.push(path.clone()),
                    }
                }
                if writable.is_empty() {
                    return Err(ClipboardError::new("file_missing"));
                }
                let hash = hash_files(&writable);
                if writable != detail.files {
                    let files_json = store::files_json(&writable)?;
                    let size_bytes = files_size_bytes(&writable);
                    if lock(&self.inner.store).replace_files(id, &hash, &files_json, size_bytes)? {
                        self.inner.emit_change(id, "updated");
                    }
                }
                self.arm(&hash);
                if let Err(error) = self.inner.platform.write_files(&writable) {
                    self.disarm();
                    return Err(error);
                }
                Ok(CopyOutcome::copied(missing))
            }
            "image" => {
                let hash = lock(&self.inner.store).image_hash(id)?;
                let (png, _) = store::image_paths(&self.inner.images_dir, &hash);
                let bytes = fs::read(&png).map_err(|_| ClipboardError::new("file_missing"))?;
                let (width, height, rgba) = decode_png(&bytes).map_err(|error| {
                    eprintln!("clipboard image decode: {error}");
                    ClipboardError::new("storage")
                })?;
                self.arm(&hash_image(width, height, &rgba));
                if let Err(error) = self.inner.platform.write_image(width, height, &rgba) {
                    self.disarm();
                    return Err(error);
                }
                Ok(CopyOutcome::copied(Vec::new()))
            }
            _ => Err(ClipboardError::new("not_found")),
        }?;
        lock(&self.inner.store).touch_copied(id, now_millis())?;
        self.inner.emit_change(id, "updated");
        Ok(outcome)
    }

    pub fn copy_file_path(&self, path: &str) -> Result<(), ClipboardError> {
        let real = self
            .inner
            .platform
            .prepare_file_path(path)
            .ok_or_else(|| ClipboardError::new("file_missing"))?;
        if !Path::new(&real).exists() {
            return Err(ClipboardError::new("file_missing"));
        }
        let paths = vec![real];
        let hash = hash_files(&paths);
        let settings = lock(&self.inner.settings).clone();
        let record_history = settings.enabled && settings.allows(ClipKind::Files);
        self.arm(&hash);
        if let Err(error) = self.inner.platform.write_files(&paths) {
            self.disarm();
            return Err(error);
        }
        if !record_history {
            return Ok(());
        }
        // The monitor ignores a self-write until the file is already stored.
        self.disarm();
        let capture = Capture {
            payload: Some(Payload::Files(paths)),
            sensitive: false,
            source_app: Some("Pixo".into()),
            types: vec!["public.file-url".into()],
        };
        self.inner.record(capture, true)?;
        self.arm(&hash);
        Ok(())
    }

    pub fn set_pinned(&self, id: i64, pinned: bool) -> Result<(), ClipboardError> {
        lock(&self.inner.store).set_pinned(id, pinned)?;
        self.inner.emit_change(id, "updated");
        Ok(())
    }

    pub fn set_label(&self, id: i64, label: Option<&str>) -> Result<(), ClipboardError> {
        lock(&self.inner.store).set_label(id, label)?;
        self.inner.emit_change(id, "updated");
        Ok(())
    }

    pub fn note_text(&self, text: &str, source: &str) -> Result<(), ClipboardError> {
        let text = text.trim();
        if text.is_empty()
            || text.len() > 1024 * 1024
            || source.is_empty()
            || source.chars().count() > 64
            || !source
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':'))
        {
            return Err(ClipboardError::new("invalid_input"));
        }
        let settings = lock(&self.inner.settings).clone();
        if settings.enabled && !settings.paused && settings.allows(ClipKind::Text) {
            let rich = super::model::RichText::plain(text);
            let hash = super::util::hash_rich(&rich);
            let capture = Capture {
                payload: Some(Payload::Text(rich)),
                sensitive: false,
                source_app: Some(source.to_string()),
                types: vec!["text/plain".into()],
            };
            self.inner.record(capture, true)?;
            self.arm(&hash);
        }
        if let Err(error) = super::io::write_text(text) {
            self.disarm();
            return Err(error);
        }
        Ok(())
    }

    pub fn delete(&self, ids: Vec<i64>) -> Result<(), ClipboardError> {
        let removal = lock(&self.inner.store).delete_ids(&ids)?;
        remove_images(&self.inner.images_dir, &removal.image_hashes);
        self.inner
            .emit_change(ids.first().copied().unwrap_or(0), "removed");
        Ok(())
    }

    pub fn clear(&self, keep_pinned: bool) -> Result<(), ClipboardError> {
        let removal = lock(&self.inner.store).clear(keep_pinned)?;
        remove_images(&self.inner.images_dir, &removal.image_hashes);
        self.inner.emit_change(0, "removed");
        Ok(())
    }

    pub fn read_now(&self) -> Result<Summary, ClipboardError> {
        let capture = match self.inner.platform.read_now() {
            Ok(capture) => capture,
            Err(error) => {
                if error.code == "permission_denied" {
                    *lock(&self.inner.last_error) = Some("permission_denied");
                    self.inner.emit_status();
                }
                return Err(error);
            }
        };
        *lock(&self.inner.last_error) = None;
        self.inner
            .record(capture, true)?
            .ok_or_else(|| ClipboardError::new("not_found"))
    }

    pub fn status(&self) -> Status {
        self.inner.status()
    }

    pub fn set_paused(&self, paused: bool) -> Result<Status, ClipboardError> {
        let settings = {
            let mut settings = lock(&self.inner.settings);
            settings.paused = paused;
            lock(&self.inner.store).save_settings(&settings)?;
            settings.clone()
        };
        self.inner.apply_recording(&settings);
        self.inner.emit_status();
        Ok(self.inner.status())
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<Status, ClipboardError> {
        let current = lock(&self.inner.settings).clone();
        if current.enabled == enabled {
            return Ok(self.inner.status());
        }
        let mut proposed = current.clone();
        proposed.enabled = enabled;
        let caps = self.inner.platform.capabilities();
        {
            let mut registered = lock(&self.inner.registered_shortcut);
            if let Err(error) = shortcut::sync_shortcut(
                &self.inner.app,
                &proposed.shortcut,
                proposed.enabled && caps.can_global_shortcut,
                &mut registered,
            ) {
                return Err(error);
            }
        }
        if let Err(error) = lock(&self.inner.store).save_settings(&proposed) {
            let mut registered = lock(&self.inner.registered_shortcut);
            let _ = shortcut::sync_shortcut(
                &self.inner.app,
                &current.shortcut,
                current.enabled && caps.can_global_shortcut,
                &mut registered,
            );
            return Err(error);
        }
        *lock(&self.inner.settings) = proposed.clone();
        self.inner.apply_recording(&proposed);
        self.inner.emit_status();
        Ok(self.inner.status())
    }

    pub fn settings(&self) -> Settings {
        lock(&self.inner.settings).clone()
    }

    pub fn update_settings(&self, update: SettingsUpdate) -> Result<Settings, ClipboardError> {
        let confirm = update.confirm;
        let current = lock(&self.inner.settings).clone();
        let proposed = update.into_settings(current.paused)?;
        crate::shortcut::parse_shortcut(&proposed.shortcut)?;
        crate::shortcut::parse_shortcut(&proposed.pin_shortcut)?;
        crate::shortcut::parse_shortcut(&proposed.delete_shortcut)?;
        if crate::screenshot::conflicts_with(
            &self.inner.app,
            &[
                proposed.shortcut.as_str(),
                proposed.pin_shortcut.as_str(),
                proposed.delete_shortcut.as_str(),
            ],
        ) {
            return Err(ClipboardError::new("shortcut_conflict"));
        }
        let plan = lock(&self.inner.store).prune_plan(&proposed, now_millis())?;
        if plan.count > 0 && !confirm {
            return Err(ClipboardError::with_params(
                "needs_confirm",
                serde_json::json!({ "count": plan.count }),
            ));
        }
        {
            let mut registered = lock(&self.inner.registered_shortcut);
            let caps = self.inner.platform.capabilities();
            shortcut::sync_shortcut(
                &self.inner.app,
                &proposed.shortcut,
                proposed.enabled && caps.can_global_shortcut,
                &mut registered,
            )?;
        }
        if let Err(error) = lock(&self.inner.store).save_settings(&proposed) {
            let mut registered = lock(&self.inner.registered_shortcut);
            let caps = self.inner.platform.capabilities();
            let _ = shortcut::sync_shortcut(
                &self.inner.app,
                &current.shortcut,
                current.enabled && caps.can_global_shortcut,
                &mut registered,
            );
            return Err(error);
        }
        *lock(&self.inner.settings) = proposed.clone();
        self.inner.platform.set_kinds(
            proposed.record_text,
            proposed.record_image,
            proposed.record_files,
        );
        self.inner.apply_recording(&proposed);
        if plan.count > 0 {
            let removal = lock(&self.inner.store).apply_prune(&proposed, now_millis())?;
            remove_images(&self.inner.images_dir, &removal.image_hashes);
            self.inner.emit_change(0, "removed");
        }
        self.inner.emit_status();
        Ok(proposed)
    }

    pub fn arm_ignore_next(&self) {
        self.inner.ignore_next.store(true, Ordering::SeqCst);
        self.inner.emit_status();
    }

    pub fn ignore_next_armed(&self) -> bool {
        self.inner.ignore_next.load(Ordering::SeqCst)
    }

    pub fn shortcut_bindings(&self) -> [String; 3] {
        let settings = self.settings();
        [
            settings.shortcut,
            settings.pin_shortcut,
            settings.delete_shortcut,
        ]
    }

    pub fn record_produced_image(
        &self,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<crate::clipboard::util::ProducedHistory, ClipboardError> {
        use crate::clipboard::util::{
            PRODUCED_IMAGE_LIMIT, ProducedHistory, produced_image_history,
        };

        let expected =
            image_rgba_len(width, height).ok_or_else(|| ClipboardError::new("invalid_input"))?;
        if rgba.len() != expected {
            return Err(ClipboardError::new("invalid_input"));
        }
        let hash = hash_image(width, height, rgba);
        let settings = lock(&self.inner.settings).clone();
        let decision = produced_image_history(
            settings.enabled,
            settings.allows(ClipKind::Image),
            rgba.len() as u64,
        );
        self.arm(&hash);
        if let Err(error) = self.inner.platform.write_image(width, height, rgba) {
            self.disarm();
            return Err(error);
        }
        if decision != ProducedHistory::Added {
            return Ok(decision);
        }
        // A self-write of a hash that is not stored yet is ignored. Insert first,
        // then arm again so the clipboard monitor updates that row in place.
        self.disarm();
        let capture = Capture {
            payload: Some(Payload::Image {
                width,
                height,
                rgba: rgba.to_vec(),
            }),
            sensitive: false,
            source_app: Some("Pixo".into()),
            types: vec!["image/png".into()],
        };
        self.inner
            .record_with_image_limit(capture, true, Some(PRODUCED_IMAGE_LIMIT), None)?;
        self.arm(&hash);
        Ok(ProducedHistory::Added)
    }

    pub fn record_produced_png(
        &self,
        png: &[u8],
    ) -> Result<crate::clipboard::util::ProducedHistory, ClipboardError> {
        use crate::clipboard::util::{ProducedHistory, produced_image_history};

        if !self.inner.platform.accepts_png_bytes() {
            let (width, height, rgba) = decode_png(png).map_err(|error| {
                eprintln!("clipboard png: {error}");
                ClipboardError::new("invalid_input")
            })?;
            return self.record_produced_image(width, height, &rgba);
        }
        let (width, height) =
            png_dimensions(png).ok_or_else(|| ClipboardError::new("invalid_input"))?;
        let raw_len = image_rgba_len(width, height)
            .ok_or_else(|| ClipboardError::new("invalid_input"))? as u64;
        let settings = lock(&self.inner.settings).clone();
        let decision =
            produced_image_history(settings.enabled, settings.allows(ClipKind::Image), raw_len);
        // Paste the already compressed PNG. NSImage would rebuild a TIFF on the main thread.
        self.inner.platform.write_png(png)?;
        if decision != ProducedHistory::Added {
            return Ok(decision);
        }
        let png = png.to_vec();
        let inner = Arc::clone(&self.inner);
        thread::Builder::new()
            .name("pixo-screenshot-history".into())
            .spawn(move || {
                if let Err(error) = inner.persist_produced_png(&png) {
                    eprintln!("screenshot history: {}", error.code);
                }
            })
            .map_err(|error| {
                eprintln!("screenshot history thread: {error}");
                ClipboardError::new("storage")
            })?;
        Ok(ProducedHistory::Added)
    }

    pub fn copy_gif_image(&self, gif: &[u8]) -> Result<(), ClipboardError> {
        use crate::clipboard::util::{ProducedHistory, produced_image_history};

        if !is_gif(gif) {
            return Err(ClipboardError::new("invalid_input"));
        }
        let png = gif_still_png(gif).map_err(|error| {
            eprintln!("clipboard gif: {error}");
            ClipboardError::new("invalid_input")
        })?;
        self.inner.platform.write_gif(gif, &png)?;
        let (width, height) =
            png_dimensions(&png).ok_or_else(|| ClipboardError::new("invalid_input"))?;
        let raw_len = image_rgba_len(width, height)
            .ok_or_else(|| ClipboardError::new("invalid_input"))? as u64;
        let settings = lock(&self.inner.settings).clone();
        let decision =
            produced_image_history(settings.enabled, settings.allows(ClipKind::Image), raw_len);
        if decision != ProducedHistory::Added {
            return Ok(());
        }
        let png = png.to_vec();
        let inner = Arc::clone(&self.inner);
        thread::Builder::new()
            .name("pixo-gif-history".into())
            .spawn(move || {
                if let Err(error) = inner.persist_produced_png(&png) {
                    eprintln!("gif history: {}", error.code);
                }
            })
            .map_err(|error| {
                eprintln!("gif history thread: {error}");
                ClipboardError::new("storage")
            })?;
        Ok(())
    }

    fn arm(&self, hash: &str) {
        *lock(&self.inner.self_write) = Some(SelfWrite {
            hash: hash.to_string(),
            at: Instant::now(),
        });
    }

    fn disarm(&self) {
        *lock(&self.inner.self_write) = None;
    }
}

impl Inner {
    fn record(&self, capture: Capture, manual: bool) -> Result<Option<Summary>, ClipboardError> {
        self.record_with_image_limit(capture, manual, None, None)
    }

    fn persist_produced_png(&self, png: &[u8]) -> Result<(), ClipboardError> {
        let (width, height, rgba) = decode_png(png).map_err(|error| {
            eprintln!("clipboard png: {error}");
            ClipboardError::new("invalid_input")
        })?;
        let capture = Capture {
            payload: Some(Payload::Image {
                width,
                height,
                rgba,
            }),
            sensitive: false,
            source_app: Some("Pixo".into()),
            types: vec!["image/png".into()],
        };
        self.record_with_image_limit(capture, true, Some(PRODUCED_IMAGE_LIMIT), Some(png))?;
        Ok(())
    }

    fn record_with_image_limit(
        &self,
        capture: Capture,
        manual: bool,
        image_limit: Option<u64>,
        encoded_png: Option<&[u8]>,
    ) -> Result<Option<Summary>, ClipboardError> {
        let settings = lock(&self.settings).clone();
        if !manual && (!settings.enabled || settings.paused) {
            return Ok(None);
        }
        if !manual && type_ignored(&capture.types, &settings.ignored_types) {
            self.consume_ignore_next();
            lock(&self.counters).ignored += 1;
            self.emit_status();
            return Ok(None);
        }
        if capture.sensitive {
            if !manual {
                self.consume_ignore_next();
            }
            if settings.skip_sensitive {
                lock(&self.counters).sensitive += 1;
                self.emit_status();
            }
            return Ok(None);
        }
        let Some(payload) = capture.payload else {
            return Ok(None);
        };
        if !settings.allows(payload.kind()) {
            if manual {
                return Err(ClipboardError::with_params(
                    "invalid_input",
                    serde_json::json!({ "reason": "kind_disabled" }),
                ));
            }
            return Ok(None);
        }
        if app_excluded(capture.source_app.as_deref(), &settings.excluded_apps) {
            if !manual {
                self.consume_ignore_next();
            }
            lock(&self.counters).excluded += 1;
            self.emit_status();
            return Ok(None);
        }
        let payload = match payload {
            Payload::Text(text) => {
                let text = crate::clipboard::rich::fit(
                    crate::clipboard::rich::normalize(text),
                    settings.max_text_bytes,
                );
                Payload::Text(text)
            }
            other => other,
        };
        if payload_is_empty(&payload) {
            return Ok(None);
        }
        if let Some(bytes) = payload_limit_bytes(&payload) {
            let limit = match payload.kind() {
                ClipKind::Image => image_limit.unwrap_or(settings.max_image_bytes),
                kind => limit_for(&settings, kind),
            };
            if bytes > limit {
                if !manual {
                    self.consume_ignore_next();
                }
                lock(&self.counters).too_large += 1;
                self.emit_status();
                log_clipboard("too_large", payload.kind().as_str(), bytes);
                if manual {
                    return Err(ClipboardError::new("too_large"));
                }
                return Ok(None);
            }
        }
        let prepared = prepare_payload(&payload, encoded_png)?;
        let self_write = self.take_self_write(&prepared.hash);
        if !manual && !self_write && self.consume_ignore_next() {
            return Ok(None);
        }
        if let (Some(png), Some(thumb)) = (&prepared.png, &prepared.thumb) {
            store::write_image_files(&self.images_dir, &prepared.hash, png, thumb)?;
        }
        let result = lock(&self.store).upsert(UpsertRequest {
            kind: prepared.kind,
            hash: &prepared.hash,
            text: prepared.text,
            html: prepared.html,
            rtf: prepared.rtf,
            files_json: prepared.files_json,
            image_width: prepared.image_width,
            image_height: prepared.image_height,
            size_bytes: prepared.size_bytes,
            source_app: capture.source_app,
            now: now_millis(),
            self_write,
        })?;
        let (summary, reason) = match result {
            UpsertResult::Added(summary) => (summary, "added"),
            UpsertResult::Updated { summary } => (summary, "updated"),
            UpsertResult::Ignored => return Ok(None),
        };
        if !self_write {
            let removal = lock(&self.store).apply_prune(&settings, now_millis())?;
            remove_images(&self.images_dir, &removal.image_hashes);
        }
        self.emit_change(summary.id, reason);
        Ok(Some(summary))
    }

    fn take_self_write(&self, hash: &str) -> bool {
        let mut slot = lock(&self.self_write);
        let Some(token) = slot.as_ref() else {
            return false;
        };
        if token.at.elapsed() > Duration::from_secs(2) {
            *slot = None;
            return false;
        }
        if token.hash != hash {
            return false;
        }
        *slot = None;
        true
    }

    fn apply_recording(&self, settings: &Settings) {
        let caps = self.platform.capabilities();
        self.platform
            .set_recording(settings.enabled && !settings.paused && caps.can_monitor);
    }

    fn sync_shortcut(&self) -> Result<(), ClipboardError> {
        let settings = lock(&self.settings).clone();
        let caps = self.platform.capabilities();
        let mut registered = lock(&self.registered_shortcut);
        shortcut::sync_shortcut(
            &self.app,
            &settings.shortcut,
            settings.enabled && caps.can_global_shortcut,
            &mut registered,
        )
    }

    fn status(&self) -> Status {
        let settings = lock(&self.settings).clone();
        let counters = *lock(&self.counters);
        let caps = self.platform.capabilities();
        let access = self.platform.access();
        let last_error = *lock(&self.last_error);
        Status {
            state: status_state(
                caps.can_monitor,
                access.as_deref(),
                last_error,
                settings.enabled,
                settings.paused,
            )
            .to_string(),
            platform: caps.platform.to_string(),
            can_monitor: caps.can_monitor,
            can_read_source_app: caps.can_read_source_app,
            can_global_shortcut: caps.can_global_shortcut,
            enabled: settings.enabled,
            paused: settings.paused,
            skipped_sensitive: counters.sensitive,
            skipped_too_large: counters.too_large,
            skipped_excluded: counters.excluded,
            skipped_ignored: counters.ignored,
            ignore_next: self.ignore_next.load(Ordering::SeqCst),
            reason: caps.reason,
            storage_path: self.storage_path.clone(),
            access,
        }
    }

    fn consume_ignore_next(&self) -> bool {
        if self.ignore_next.swap(false, Ordering::SeqCst) {
            self.emit_status();
            true
        } else {
            false
        }
    }

    fn emit_change(&self, id: i64, reason: &'static str) {
        let _ = self.app.emit(
            "clipboard://changed",
            serde_json::json!({ "id": id, "reason": reason }),
        );
    }

    fn emit_status(&self) {
        let _ = self.app.emit("clipboard://status", self.status());
    }
}

struct Prepared {
    kind: ClipKind,
    hash: String,
    text: Option<String>,
    html: Option<String>,
    rtf: Option<Vec<u8>>,
    files_json: Option<String>,
    image_width: Option<u32>,
    image_height: Option<u32>,
    size_bytes: i64,
    png: Option<Vec<u8>>,
    thumb: Option<Vec<u8>>,
}

fn prepare_payload(
    payload: &Payload,
    encoded_png: Option<&[u8]>,
) -> Result<Prepared, ClipboardError> {
    match payload {
        Payload::Text(text) => Ok(Prepared {
            kind: ClipKind::Text,
            hash: crate::clipboard::util::hash_rich(text),
            size_bytes: (text.plain.len()
                + text.html.as_ref().map(String::len).unwrap_or(0)
                + text.rtf.as_ref().map(Vec::len).unwrap_or(0)) as i64,
            text: Some(text.plain.clone()),
            html: text.html.clone(),
            rtf: text.rtf.clone(),
            files_json: None,
            image_width: None,
            image_height: None,
            png: None,
            thumb: None,
        }),
        Payload::Files(files) => Ok(Prepared {
            kind: ClipKind::Files,
            hash: hash_files(files),
            size_bytes: files_size_bytes(files),
            text: None,
            html: None,
            rtf: None,
            files_json: Some(store::files_json(files)?),
            image_width: None,
            image_height: None,
            png: None,
            thumb: None,
        }),
        Payload::Image {
            width,
            height,
            rgba,
        } => {
            if image_rgba_len(*width, *height) != Some(rgba.len()) {
                return Err(ClipboardError::new("invalid_input"));
            }
            let png = if let Some(bytes) = encoded_png {
                if png_dimensions(bytes).is_none() {
                    return Err(ClipboardError::new("invalid_input"));
                }
                bytes.to_vec()
            } else {
                encode_png(*width, *height, rgba).map_err(|error| {
                    eprintln!("clipboard png: {error}");
                    ClipboardError::new("storage")
                })?
            };
            let thumb = thumbnail_png(*width, *height, rgba).map_err(|error| {
                eprintln!("clipboard thumb: {error}");
                ClipboardError::new("storage")
            })?;
            Ok(Prepared {
                kind: ClipKind::Image,
                hash: hash_image(*width, *height, rgba),
                size_bytes: rgba.len() as i64,
                text: None,
                html: None,
                rtf: None,
                files_json: None,
                image_width: Some(*width),
                image_height: Some(*height),
                png: Some(png),
                thumb: Some(thumb),
            })
        }
    }
}

fn payload_is_empty(payload: &Payload) -> bool {
    match payload {
        Payload::Text(text) => text.plain.is_empty(),
        Payload::Files(files) => files.is_empty(),
        Payload::Image { .. } => false,
    }
}

fn payload_limit_bytes(payload: &Payload) -> Option<u64> {
    match payload {
        Payload::Text(text) => Some(text.plain.len() as u64),
        Payload::Image { rgba, .. } => Some(rgba.len() as u64),
        Payload::Files(_) => None,
    }
}

fn limit_for(settings: &Settings, kind: ClipKind) -> u64 {
    match kind {
        ClipKind::Text => settings.max_text_bytes,
        ClipKind::Image => settings.max_image_bytes,
        ClipKind::Files => u64::MAX,
    }
}

fn remove_images(directory: &Path, hashes: &[String]) {
    for hash in hashes {
        let (png, thumb) = store::image_paths(directory, hash);
        let _ = fs::remove_file(png);
        let _ = fs::remove_file(thumb);
    }
}

fn remove_orphan_images(directory: &Path, hashes: &std::collections::HashSet<String>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.ends_with(".tmp") {
            let _ = fs::remove_file(entry.path());
            continue;
        }
        let hash = name
            .strip_suffix(".thumb.png")
            .or_else(|| name.strip_suffix(".png"));
        if let Some(hash) = hash {
            if !hashes.contains(hash) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

pub fn status_state(
    can_monitor: bool,
    access: Option<&str>,
    last_error: Option<&str>,
    enabled: bool,
    paused: bool,
) -> &'static str {
    if !can_monitor {
        return "unsupported";
    }
    if access == Some("deny") || last_error == Some("permission_denied") {
        return "permission_denied";
    }
    if !enabled {
        return "disabled";
    }
    if paused {
        return "paused";
    }
    "running"
}

#[cfg(test)]
mod tests {
    use super::status_state;

    #[test]
    fn status_prefers_support_then_permission_then_switch() {
        assert_eq!(
            status_state(false, Some("deny"), None, true, false),
            "unsupported"
        );
        assert_eq!(
            status_state(true, Some("deny"), None, true, false),
            "permission_denied"
        );
        assert_eq!(
            status_state(true, None, Some("permission_denied"), false, false),
            "permission_denied"
        );
        assert_eq!(
            status_state(true, Some("allow"), None, false, false),
            "disabled"
        );
        assert_eq!(status_state(true, None, None, true, true), "paused");
        assert_eq!(
            status_state(true, Some("ask"), None, true, false),
            "running"
        );
    }
}
