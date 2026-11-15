use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;

use super::bytes::decode_base64;
use super::error::ScreenshotError;

const PNG_LIMIT: usize = 32 * 1024 * 1024;
const LONGEST_EDGE: u32 = 1600;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrText {
    pub text: String,
}

struct Flight {
    stop: AtomicBool,
}

static FLIGHT: Mutex<Option<Arc<Flight>>> = Mutex::new(None);
static DROP_NEXT: AtomicBool = AtomicBool::new(false);

fn flight_slot() -> std::sync::MutexGuard<'static, Option<Arc<Flight>>> {
    FLIGHT.lock().unwrap_or_else(|error| error.into_inner())
}

struct Job {
    flight: Arc<Flight>,
}

impl Job {
    fn begin() -> Result<Self, ScreenshotError> {
        let mut slot = flight_slot();
        if DROP_NEXT.swap(false, Ordering::AcqRel) {
            return Err(ScreenshotError::new("cancelled"));
        }
        let flight = Arc::new(Flight {
            stop: AtomicBool::new(false),
        });
        *slot = Some(Arc::clone(&flight));
        Ok(Self { flight })
    }

    fn stopped(&self) -> bool {
        self.flight.stop.load(Ordering::Acquire)
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        let mut slot = flight_slot();
        if slot
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &self.flight))
        {
            *slot = None;
        }
    }
}

pub fn cancel_recognition() {
    let slot = flight_slot();
    if let Some(flight) = slot.as_ref() {
        flight.stop.store(true, Ordering::Release);
    } else {
        DROP_NEXT.store(true, Ordering::Release);
    }
    drop(slot);
    #[cfg(target_os = "macos")]
    cancel_vision_request();
}

/// Drops a cancel that arrived while nothing was recognizing, so the next capture can read text.
pub fn clear_pending_cancel() {
    DROP_NEXT.store(false, Ordering::Release);
}

pub fn copy_text(text: &str) -> Result<(), ScreenshotError> {
    let text = text.trim();
    if text.is_empty() || text.len() > 1024 * 1024 {
        return Err(ScreenshotError::new("invalid_input"));
    }
    crate::clipboard::copy_plain_text(text).map_err(|error| {
        eprintln!("screenshot ocr clipboard: {error}");
        ScreenshotError::new(match error.code {
            "busy" => "busy",
            "unsupported" => "unsupported",
            _ => "storage",
        })
    })
}

pub fn available() -> bool {
    #[cfg(target_os = "macos")]
    {
        true
    }
    #[cfg(target_os = "windows")]
    {
        windows_ocr_available()
    }
    #[cfg(target_os = "linux")]
    {
        tesseract_installed()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        false
    }
}

pub fn recognize(png_base64: &str) -> Result<OcrText, ScreenshotError> {
    let job = Job::begin()?;
    if job.stopped() {
        return Err(ScreenshotError::new("cancelled"));
    }
    if png_base64.len() > PNG_LIMIT.saturating_mul(2) {
        return Err(ScreenshotError::new("invalid_input"));
    }
    let bytes = decode_base64(png_base64)?;
    if job.stopped() {
        return Err(ScreenshotError::new("cancelled"));
    }
    if bytes.len() > PNG_LIMIT || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(ScreenshotError::new("invalid_input"));
    }
    let fitted = fit_png(&bytes)?;
    if job.stopped() {
        return Err(ScreenshotError::new("cancelled"));
    }
    let text = read_text(&job, &fitted)?;
    if job.stopped() {
        return Err(ScreenshotError::new("cancelled"));
    }
    let text = text.trim();
    if text.is_empty() {
        return Err(ScreenshotError::with_params(
            "not_found",
            serde_json::json!({ "reason": "ocr" }),
        ));
    }
    if job.stopped() {
        return Err(ScreenshotError::new("cancelled"));
    }
    Ok(OcrText {
        text: text.to_string(),
    })
}

fn fit_png(bytes: &[u8]) -> Result<Cow<'_, [u8]>, ScreenshotError> {
    let image = image::load_from_memory(bytes).map_err(|error| {
        eprintln!("screenshot ocr image: {error}");
        ScreenshotError::new("invalid_input")
    })?;
    let width = image.width();
    let height = image.height();
    let longest = width.max(height);
    if longest <= LONGEST_EDGE || longest == 0 {
        return Ok(Cow::Borrowed(bytes));
    }
    let scale = f64::from(LONGEST_EDGE) / f64::from(longest);
    let target_width = ((f64::from(width) * scale).round() as u32).max(1);
    let target_height = ((f64::from(height) * scale).round() as u32).max(1);
    let resized = image.resize(
        target_width,
        target_height,
        image::imageops::FilterType::Triangle,
    );
    let mut encoded = Vec::new();
    resized
        .write_to(
            &mut std::io::Cursor::new(&mut encoded),
            image::ImageFormat::Png,
        )
        .map_err(|error| {
            eprintln!("screenshot ocr scale: {error}");
            ScreenshotError::new("capture_failed")
        })?;
    Ok(Cow::Owned(encoded))
}

#[cfg(not(target_os = "macos"))]
fn unsupported_ocr() -> ScreenshotError {
    ScreenshotError::with_params("unsupported", serde_json::json!({ "reason": "ocr" }))
}

#[cfg(target_os = "windows")]
fn read_text(job: &Job, bytes: &[u8]) -> Result<String, ScreenshotError> {
    windows_read_text(job, bytes)
}

#[cfg(target_os = "linux")]
fn read_text(job: &Job, bytes: &[u8]) -> Result<String, ScreenshotError> {
    linux_read_text(job, bytes)
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn read_text(job: &Job, _bytes: &[u8]) -> Result<String, ScreenshotError> {
    if job.stopped() {
        return Err(ScreenshotError::new("cancelled"));
    }
    Err(unsupported_ocr())
}

#[cfg(target_os = "windows")]
fn windows_ocr_available() -> bool {
    static READY: OnceLock<bool> = OnceLock::new();
    *READY.get_or_init(|| windows_engine().is_ok())
}

#[cfg(target_os = "windows")]
fn windows_engine() -> Result<windows::Media::Ocr::OcrEngine, ScreenshotError> {
    windows::Media::Ocr::OcrEngine::TryCreateFromUserProfileLanguages().map_err(|error| {
        eprintln!("screenshot ocr engine: {error}");
        unsupported_ocr()
    })
}

#[cfg(target_os = "windows")]
fn windows_read_text(job: &Job, bytes: &[u8]) -> Result<String, ScreenshotError> {
    use windows::Graphics::Imaging::{
        BitmapAlphaMode, BitmapDecoder, BitmapPixelFormat, SoftwareBitmap,
    };
    use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

    if job.stopped() {
        return Err(ScreenshotError::new("cancelled"));
    }
    let stream = InMemoryRandomAccessStream::new().map_err(ocr_failed)?;
    let writer = DataWriter::CreateDataWriter(&stream).map_err(ocr_failed)?;
    writer.WriteBytes(bytes).map_err(ocr_failed)?;
    writer
        .StoreAsync()
        .map_err(ocr_failed)?
        .join()
        .map_err(ocr_failed)?;
    writer.DetachStream().map_err(ocr_failed)?;
    stream.Seek(0).map_err(ocr_failed)?;
    if job.stopped() {
        return Err(ScreenshotError::new("cancelled"));
    }
    let decoder = BitmapDecoder::CreateAsync(&stream)
        .map_err(ocr_failed)?
        .join()
        .map_err(ocr_failed)?;
    let bitmap = decoder
        .GetSoftwareBitmapAsync()
        .map_err(ocr_failed)?
        .join()
        .map_err(ocr_failed)?;
    let bitmap = SoftwareBitmap::ConvertWithAlpha(
        &bitmap,
        BitmapPixelFormat::Bgra8,
        BitmapAlphaMode::Ignore,
    )
    .map_err(ocr_failed)?;
    let width = f64::from(bitmap.PixelWidth().map_err(ocr_failed)?.max(1));
    let height = f64::from(bitmap.PixelHeight().map_err(ocr_failed)?.max(1));
    let engine = windows_engine()?;
    if job.stopped() {
        return Err(ScreenshotError::new("cancelled"));
    }
    let result = engine
        .RecognizeAsync(&bitmap)
        .map_err(ocr_failed)?
        .join()
        .map_err(ocr_failed)?;
    if job.stopped() {
        return Err(ScreenshotError::new("cancelled"));
    }
    let lines = result.Lines().map_err(ocr_failed)?;
    let mut pieces = Vec::new();
    let line_count = lines.Size().map_err(ocr_failed)?;
    for index in 0..line_count {
        let line = lines.GetAt(index).map_err(ocr_failed)?;
        let words = line.Words().map_err(ocr_failed)?;
        let word_count = words.Size().map_err(ocr_failed)?;
        for word_index in 0..word_count {
            let word = words.GetAt(word_index).map_err(ocr_failed)?;
            let text = hstring_text(&word.Text().map_err(ocr_failed)?);
            if text.trim().is_empty() {
                continue;
            }
            let bounds = word.BoundingRect().map_err(ocr_failed)?;
            let word_height = f64::from(bounds.Height).max(1.0);
            pieces.push(Piece {
                text,
                x: f64::from(bounds.X) / width,
                mid: (f64::from(bounds.Y) + word_height / 2.0) / height,
                height: word_height / height,
            });
        }
    }
    let _ = bitmap.Close();
    Ok(arrange_text(pieces))
}

#[cfg(target_os = "windows")]
fn hstring_text(value: &windows::core::HSTRING) -> String {
    value.to_string_lossy()
}

#[cfg(target_os = "windows")]
fn ocr_failed(error: impl std::fmt::Display) -> ScreenshotError {
    eprintln!("screenshot ocr: {error}");
    ScreenshotError::new("capture_failed")
}

#[cfg(target_os = "linux")]
fn png_size(bytes: &[u8]) -> (u32, u32) {
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" || &bytes[12..16] != b"IHDR" {
        return (1, 1);
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap_or([0, 0, 0, 1]));
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap_or([0, 0, 0, 1]));
    if width == 0 || height == 0 {
        (1, 1)
    } else {
        (width, height)
    }
}

#[cfg(target_os = "linux")]
fn tesseract_installed() -> bool {
    static READY: OnceLock<bool> = OnceLock::new();
    *READY.get_or_init(|| {
        std::process::Command::new("tesseract")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    })
}

#[cfg(target_os = "linux")]
fn linux_read_text(job: &Job, bytes: &[u8]) -> Result<String, ScreenshotError> {
    if job.stopped() {
        return Err(ScreenshotError::new("cancelled"));
    }
    if !tesseract_installed() {
        return Err(unsupported_ocr());
    }
    let languages = tesseract_languages();
    let (width, height) = png_size(bytes);
    let table = run_tesseract(bytes, &languages, true)?;
    if job.stopped() {
        return Err(ScreenshotError::new("cancelled"));
    }
    let arranged = words_from_tsv(&table, width, height);
    if !arranged.trim().is_empty() {
        return Ok(arranged);
    }
    let plain = run_tesseract(bytes, &languages, false)?;
    if job.stopped() {
        return Err(ScreenshotError::new("cancelled"));
    }
    Ok(plain)
}

#[cfg(target_os = "linux")]
fn tesseract_languages() -> String {
    let output = std::process::Command::new("tesseract")
        .arg("--list-langs")
        .output();
    let Ok(output) = output else {
        return "eng".to_string();
    };
    let mut listed = String::from_utf8_lossy(&output.stdout).into_owned();
    listed.push('\n');
    listed.push_str(&String::from_utf8_lossy(&output.stderr));
    let have: Vec<String> = listed
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.contains(' ') && !line.contains(':'))
        .map(str::to_string)
        .collect();
    let wanted = preferred_tesseract_languages();
    let chosen: Vec<&str> = wanted
        .iter()
        .copied()
        .filter(|language| have.iter().any(|item| item == language))
        .collect();
    if chosen.is_empty() {
        "eng".to_string()
    } else {
        chosen.join("+")
    }
}

#[cfg(target_os = "linux")]
fn preferred_tesseract_languages() -> Vec<&'static str> {
    let locale = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_default()
        .to_ascii_lowercase();
    let primary = if locale.starts_with("zh") {
        if locale.contains("tw") || locale.contains("hk") || locale.contains("hant") {
            "chi_tra"
        } else {
            "chi_sim"
        }
    } else if locale.starts_with("ja") {
        "jpn"
    } else if locale.starts_with("ko") {
        "kor"
    } else {
        "eng"
    };
    if primary == "eng" {
        vec!["eng"]
    } else {
        vec![primary, "eng"]
    }
}

#[cfg(target_os = "linux")]
fn run_tesseract(bytes: &[u8], languages: &str, table: bool) -> Result<String, ScreenshotError> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut command = Command::new("tesseract");
    command.args(["stdin", "stdout", "-l", languages, "--psm", "6"]);
    if table {
        command.arg("tsv");
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            eprintln!("screenshot ocr tesseract: {error}");
            unsupported_ocr()
        })?;
    if let Some(mut stdin) = child.stdin.take() {
        if stdin.write_all(bytes).is_err() {
            let _ = child.kill();
            return Err(ScreenshotError::new("capture_failed"));
        }
    }
    let output = child.wait_with_output().map_err(|error| {
        eprintln!("screenshot ocr tesseract: {error}");
        ScreenshotError::new("capture_failed")
    })?;
    if !output.status.success() {
        eprintln!("screenshot ocr tesseract status: {}", output.status);
        return Err(ScreenshotError::new("capture_failed"));
    }
    String::from_utf8(output.stdout).map_err(|_| ScreenshotError::new("capture_failed"))
}

#[cfg(any(test, target_os = "linux"))]
fn words_from_tsv(tsv: &str, width: u32, height: u32) -> String {
    if width == 0 || height == 0 {
        return String::new();
    }
    let width = f64::from(width);
    let height = f64::from(height);
    let mut pieces = Vec::new();
    for line in tsv.lines().skip(1) {
        let columns: Vec<&str> = line.split('\t').collect();
        if columns.len() < 12 || columns[0] != "5" {
            continue;
        }
        let text = columns[11..].join(" ").trim().to_string();
        if text.is_empty() {
            continue;
        }
        let Ok(left) = columns[6].parse::<f64>() else {
            continue;
        };
        let Ok(top) = columns[7].parse::<f64>() else {
            continue;
        };
        let Ok(word_height) = columns[9].parse::<f64>() else {
            continue;
        };
        let word_height = word_height.max(1.0);
        pieces.push(Piece {
            text,
            x: left / width,
            mid: (top + word_height / 2.0) / height,
            height: word_height / height,
        });
    }
    arrange_text(pieces)
}

#[cfg(target_os = "macos")]
#[derive(Clone)]
struct SharedRequest(objc2::rc::Retained<objc2_vision::VNRecognizeTextRequest>);

// VNRequest::cancel is safe to call from any thread. The retain keeps the request alive there.
#[cfg(target_os = "macos")]
unsafe impl Send for SharedRequest {}

#[cfg(target_os = "macos")]
static VISION_REQUEST: Mutex<Option<SharedRequest>> = Mutex::new(None);

#[cfg(target_os = "macos")]
fn bind_request(request: &objc2::rc::Retained<objc2_vision::VNRecognizeTextRequest>) {
    let mut slot = VISION_REQUEST
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    *slot = Some(SharedRequest(request.clone()));
}

#[cfg(target_os = "macos")]
fn unbind_request() {
    let mut slot = VISION_REQUEST
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    *slot = None;
}

#[cfg(target_os = "macos")]
fn cancel_vision_request() {
    use objc2::ClassType;
    use objc2_vision::{VNRecognizeTextRequest, VNRequest};

    let request = {
        let slot = VISION_REQUEST
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        (*slot).clone()
    };
    let Some(SharedRequest(request)) = request else {
        return;
    };
    let text_request: &VNRecognizeTextRequest = &request;
    let as_request: &VNRequest = ClassType::as_super(ClassType::as_super(text_request));
    unsafe {
        as_request.cancel();
    }
}

#[cfg(target_os = "macos")]
fn read_text(job: &Job, bytes: &[u8]) -> Result<String, ScreenshotError> {
    use objc2::{AnyThread, ClassType};
    use objc2_foundation::{NSArray, NSData, NSDictionary};
    use objc2_vision::{VNImageRequestHandler, VNRecognizeTextRequest, VNRequest};

    if job.stopped() {
        return Err(ScreenshotError::new("cancelled"));
    }
    let data = unsafe { NSData::dataWithBytes_length(bytes.as_ptr().cast(), bytes.len()) };
    let request = VNRecognizeTextRequest::new();
    configure_request(&request);
    bind_request(&request);
    if job.stopped() {
        unbind_request();
        return Err(ScreenshotError::new("cancelled"));
    }
    let options = NSDictionary::dictionary();
    let handler = VNImageRequestHandler::initWithData_options(
        VNImageRequestHandler::alloc(),
        &data,
        &options,
    );
    let text_request: &VNRecognizeTextRequest = &request;
    let as_request: &VNRequest = ClassType::as_super(ClassType::as_super(text_request));
    let requests = NSArray::from_slice(&[as_request]);
    let performed = handler.performRequests_error(&requests);
    unbind_request();
    if job.stopped() {
        return Err(ScreenshotError::new("cancelled"));
    }
    if let Err(error) = performed {
        eprintln!("screenshot ocr: {}", error.localizedDescription());
        return Err(ScreenshotError::new("capture_failed"));
    }
    let Some(results) = request.results() else {
        return Ok(String::new());
    };
    let mut pieces = Vec::new();
    for observation in results.iter() {
        let Some(best) = observation.topCandidates(1).iter().next() else {
            continue;
        };
        let text = best.string().to_string();
        if text.trim().is_empty() {
            continue;
        }
        let bounds = unsafe { observation.boundingBox() };
        let height = f64::from(bounds.size.height);
        let top = 1.0 - (f64::from(bounds.origin.y) + height);
        pieces.push(Piece {
            text,
            x: f64::from(bounds.origin.x),
            mid: top + height / 2.0,
            height,
        });
    }
    Ok(arrange_text(pieces))
}

#[cfg(target_os = "macos")]
struct RecognitionPlan {
    fast: bool,
    languages: Vec<String>,
}

#[cfg(target_os = "macos")]
fn recognition_plan() -> &'static RecognitionPlan {
    static PLAN: OnceLock<RecognitionPlan> = OnceLock::new();
    PLAN.get_or_init(|| {
        let wanted = preferred_languages();
        let fast = supported_languages(objc2_vision::VNRequestTextRecognitionLevel::Fast);
        let system_is_fast = wanted
            .first()
            .is_some_and(|language| match_language(language, &fast).is_some());
        if system_is_fast {
            RecognitionPlan {
                fast: true,
                languages: choose_languages(&wanted, &fast),
            }
        } else {
            let accurate =
                supported_languages(objc2_vision::VNRequestTextRecognitionLevel::Accurate);
            RecognitionPlan {
                fast: false,
                languages: choose_languages(&wanted, &accurate),
            }
        }
    })
}

#[cfg(target_os = "macos")]
fn configure_request(request: &objc2_vision::VNRecognizeTextRequest) {
    use objc2::rc::Retained;
    use objc2_foundation::{NSArray, NSString};
    use objc2_vision::VNRequestTextRecognitionLevel;

    let plan = recognition_plan();
    request.setRecognitionLevel(if plan.fast {
        VNRequestTextRecognitionLevel::Fast
    } else {
        VNRequestTextRecognitionLevel::Accurate
    });
    request.setUsesLanguageCorrection(false);
    if plan.languages.is_empty() {
        return;
    }
    let owned: Vec<Retained<NSString>> = plan
        .languages
        .iter()
        .map(|language| NSString::from_str(language))
        .collect();
    let refs: Vec<&NSString> = owned.iter().map(|language| language.as_ref()).collect();
    request.setRecognitionLanguages(&NSArray::from_slice(&refs));
}

#[cfg(target_os = "macos")]
fn supported_languages(level: objc2_vision::VNRequestTextRecognitionLevel) -> Vec<String> {
    let request = objc2_vision::VNRecognizeTextRequest::new();
    request.setRecognitionLevel(level);
    request.setUsesLanguageCorrection(false);
    let supported = match unsafe { request.supportedRecognitionLanguagesAndReturnError() } {
        Ok(languages) => languages,
        Err(error) => {
            eprintln!("screenshot ocr languages: {}", error.localizedDescription());
            return Vec::new();
        }
    };
    supported
        .iter()
        .map(|language| language.to_string())
        .collect()
}

#[cfg(target_os = "macos")]
fn preferred_languages() -> Vec<String> {
    let Some(system) = system_language() else {
        return vec!["zh-Hans".to_string(), "en-US".to_string()];
    };
    let second = if system.to_ascii_lowercase().starts_with("en") {
        "zh-Hans"
    } else {
        "en-US"
    };
    if system.eq_ignore_ascii_case(second) {
        vec![system]
    } else {
        vec![system, second.to_string()]
    }
}

fn choose_languages(wanted: &[String], available: &[String]) -> Vec<String> {
    let mut chosen = Vec::new();
    for want in wanted {
        let Some(found) = match_language(want, available) else {
            continue;
        };
        if !chosen.iter().any(|have: &String| have == &found) {
            chosen.push(found);
        }
        if chosen.len() == 2 {
            break;
        }
    }
    chosen
}

#[cfg(target_os = "macos")]
fn system_language() -> Option<String> {
    use objc2::rc::Retained;
    use objc2_foundation::{NSArray, NSGlobalDomain, NSString, NSUserDefaults};

    let defaults = NSUserDefaults::standardUserDefaults();
    let global = defaults.persistentDomainForName(unsafe { NSGlobalDomain })?;
    let languages = global.objectForKey(&NSString::from_str("AppleLanguages"))?;
    let languages = languages.downcast::<NSArray>().ok()?;
    let first = languages.objectAtIndex(0);
    let first = Retained::downcast::<NSString>(first).ok()?;
    Some(vision_language(&first.to_string()))
}

fn vision_language(tag: &str) -> String {
    let mut parts = tag.split(['-', '_']);
    let language = parts.next().unwrap_or(tag);
    match parts.next() {
        Some(script)
            if script.len() == 4
                && script
                    .chars()
                    .next()
                    .is_some_and(|char| char.is_uppercase()) =>
        {
            format!("{language}-{script}")
        }
        Some(region)
            if region.len() == 2 && region.chars().all(|char| char.is_ascii_uppercase()) =>
        {
            format!("{language}-{region}")
        }
        _ => language.to_string(),
    }
}

fn match_language(want: &str, available: &[String]) -> Option<String> {
    let want_lower = want.to_ascii_lowercase();
    available
        .iter()
        .find(|item| item.eq_ignore_ascii_case(want))
        .cloned()
        .or_else(|| {
            available
                .iter()
                .find(|item| {
                    item.to_ascii_lowercase()
                        .starts_with(&format!("{want_lower}-"))
                })
                .cloned()
        })
}

struct Piece {
    text: String,
    x: f64,
    mid: f64,
    height: f64,
}

fn arrange_text(mut pieces: Vec<Piece>) -> String {
    pieces.sort_by(|left, right| {
        left.mid
            .partial_cmp(&right.mid)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                left.x
                    .partial_cmp(&right.x)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    let mut rows: Vec<Vec<Piece>> = Vec::new();
    for piece in pieces {
        let same_row = rows.last().is_some_and(|row| {
            let anchor = &row[0];
            let limit = (anchor.height.max(piece.height) * 0.6).max(0.012);
            (piece.mid - anchor.mid).abs() <= limit
        });
        if same_row {
            if let Some(row) = rows.last_mut() {
                row.push(piece);
            }
        } else {
            rows.push(vec![piece]);
        }
    }
    rows.into_iter()
        .map(|mut row| {
            row.sort_by(|left, right| {
                left.x
                    .partial_cmp(&right.x)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            join_line(row.iter().map(|piece| piece.text.as_str()))
        })
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn join_line<'a>(parts: impl IntoIterator<Item = &'a str>) -> String {
    let mut line = String::new();
    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if !line.is_empty() {
            let left = line.chars().next_back();
            let right = part.chars().next();
            if !matches!((left, right), (Some(left), Some(right)) if cjk(left) && cjk(right)) {
                line.push(' ');
            }
        }
        line.push_str(part);
    }
    line
}

fn cjk(char: char) -> bool {
    matches!(
        char,
        '\u{3400}'..='\u{9FFF}' | '\u{3040}'..='\u{30FF}' | '\u{AC00}'..='\u{D7AF}'
    )
}

#[cfg(test)]
mod tests {
    use super::{Piece, arrange_text, vision_language, words_from_tsv};

    #[test]
    fn keeps_chinese_characters_together_and_separates_lines() {
        let text = arrange_text(vec![
            Piece {
                text: "你好".into(),
                x: 0.1,
                mid: 0.2,
                height: 0.05,
            },
            Piece {
                text: "世界".into(),
                x: 0.4,
                mid: 0.21,
                height: 0.05,
            },
            Piece {
                text: "Hello".into(),
                x: 0.1,
                mid: 0.6,
                height: 0.05,
            },
            Piece {
                text: "world".into(),
                x: 0.4,
                mid: 0.61,
                height: 0.05,
            },
        ]);
        assert_eq!(text, "你好世界\nHello world");
    }

    #[test]
    fn maps_system_language_tags_to_vision_identifiers() {
        assert_eq!(vision_language("zh-Hans-CN"), "zh-Hans");
        assert_eq!(vision_language("en-US"), "en-US");
        assert_eq!(vision_language("ja-JP"), "ja-JP");
    }

    #[test]
    fn tsv_words_keep_their_rows() {
        let tsv = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n5\t1\t1\t1\t1\t1\t10\t20\t40\t16\t90\t你好\n5\t1\t1\t1\t1\t2\t60\t20\t40\t16\t90\t世界\n5\t1\t1\t1\t2\t1\t10\t80\t40\t16\t90\tHello\n";
        assert_eq!(words_from_tsv(tsv, 200, 200), "你好世界\nHello");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn blank_png_has_no_text() {
        let image = image::RgbImage::from_pixel(24, 24, image::Rgb([255, 255, 255]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgb8(image)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        let _guard = ocr_test_lock();
        super::clear_pending_cancel();
        let job = super::Job::begin().unwrap();
        assert!(super::read_text(&job, &bytes).unwrap().trim().is_empty());
    }

    #[test]
    fn shrinks_a_wide_image_before_recognition() {
        let image = image::RgbaImage::from_pixel(3200, 800, image::Rgba([240, 240, 240, 255]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(image)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        let fitted = super::fit_png(&bytes).unwrap();
        let decoded = image::load_from_memory(&fitted).unwrap();
        assert!(decoded.width() <= 1600);
        assert!(decoded.height() <= 1600);
        assert!(decoded.width() >= 1500);
    }

    #[test]
    fn leaves_a_small_image_unchanged() {
        let image = image::RgbaImage::from_pixel(32, 16, image::Rgba([10, 20, 30, 255]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(image)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        let fitted = super::fit_png(&bytes).unwrap();
        assert_eq!(&fitted[..], &bytes[..]);
    }

    #[test]
    fn rejects_blank_text_before_copying() {
        let error = super::copy_text("  \n").unwrap_err();
        assert_eq!(error.code, "invalid_input");
    }

    #[test]
    fn cancel_stops_the_active_job_without_poisoning_the_next_one() {
        let _guard = ocr_test_lock();
        super::clear_pending_cancel();

        let job = super::Job::begin().unwrap();
        super::cancel_recognition();
        assert!(job.stopped());
        drop(job);

        let error = super::recognize("").unwrap_err();
        assert_eq!(error.code, "invalid_input");

        super::cancel_recognition();
        let error = super::recognize("").unwrap_err();
        assert_eq!(error.code, "cancelled");

        let error = super::recognize("").unwrap_err();
        assert_eq!(error.code, "invalid_input");
    }

    fn ocr_test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|error| error.into_inner())
    }
}
