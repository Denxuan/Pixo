use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use super::error::ScreenshotError;

const MIN_WIDTH: u32 = 32;
const MIN_HEIGHT: u32 = 48;

static STOP: AtomicBool = AtomicBool::new(false);
static AUTO: AtomicBool = AtomicBool::new(false);
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn next_generation() -> u64 {
    GENERATION.fetch_add(1, Ordering::Relaxed) + 1
}

pub fn clear_stop() {
    STOP.store(false, Ordering::Release);
}

pub fn request_stop() {
    STOP.store(true, Ordering::Release);
}

pub fn set_auto(enabled: bool) {
    AUTO.store(enabled, Ordering::Release);
}

fn auto_enabled() -> bool {
    AUTO.load(Ordering::Acquire)
}

fn stopped() -> bool {
    STOP.load(Ordering::Acquire)
}

#[derive(Debug)]
enum Motion {
    Unchanged,
    Unaligned,
    Shift(i32),
}

const ROW_SAMPLES: usize = 32;
/// Peaks closer than this are the same alignment, not two ways the page could have moved.
const PEAK_GAP: i32 = 8;

fn find_shift(previous: &[u8], next: &[u8], width: u32, height: u32) -> Motion {
    if previous.len() != next.len() || width < 8 || height < 16 {
        return Motion::Unaligned;
    }
    let previous_rows = row_samples(previous, width, height);
    let next_rows = row_samples(next, width, height);
    let sticky: Vec<bool> = (0..height)
        .map(|y| row_static(&previous_rows[y as usize], &next_rows[y as usize]))
        .collect();
    let weights = row_weights(&previous_rows);
    // The whole fixed band, including a flat toolbar. A one-row flicker in the
    // breadcrumb used to end the band early and copy it into every slice.
    let header = sticky_header(&sticky);
    let unchanged = score_rows(
        &previous_rows,
        &next_rows,
        0,
        0..height,
        &sticky,
        false,
        &weights,
        0,
    );
    let static_rows = sticky.iter().filter(|row| **row).count();
    // A grabbed frame is noisy even when the page has not moved. Blank gaps
    // and a repeating line rhythm pull the average down after a real scroll,
    // so "unchanged" also requires almost every row to still sit in place.
    if unchanged <= 6 && static_rows.saturating_add(8) >= height as usize {
        return Motion::Unchanged;
    }
    // Sticky headers match at shift 0 and drown out a real jump. Blank rows
    // match at every offset, so only rows that are actually distinctive vote.
    let voting: Vec<u32> = (0..height)
        .filter(|&y| !sticky[y as usize] && weights[y as usize] > 0)
        .collect();
    if voting.len() < 8 {
        return Motion::Unaligned;
    }
    let still = score_rows(
        &previous_rows,
        &next_rows,
        0,
        voting.iter().copied(),
        &sticky,
        true,
        &weights,
        header,
    );
    // New rows come from the bottom of the next frame. A shift into the fixed
    // toolbar would paste that bar into the strip and skip the lines between.
    let max_shift = height
        .saturating_sub(header as u32)
        .saturating_sub(8)
        .max(1) as i32;
    let mut best: Option<Alignment> = None;
    let mut rival: Option<Alignment> = None;
    let mut shift = 1i32;
    while shift <= max_shift {
        consider_shift(
            &mut best,
            &mut rival,
            &previous_rows,
            &next_rows,
            shift,
            &voting,
            &sticky,
            &weights,
            header,
        );
        consider_shift(
            &mut best,
            &mut rival,
            &previous_rows,
            &next_rows,
            -shift,
            &voting,
            &sticky,
            &weights,
            header,
        );
        shift += 1;
    }
    let Some(best) = best else {
        return Motion::Unaligned;
    };
    // A rival blocks the match only when it explains the page about as well.
    // A perfect overlap stays: a worse offset used to veto it and drop the scroll.
    if let Some(rival) = rival {
        let margin = if best.score == 0 {
            0
        } else if best.overlap < height / 3 {
            6
        } else if best.score <= 6 {
            3
        } else {
            2
        };
        let similar = rival.overlap.saturating_mul(3) >= best.overlap.saturating_mul(2)
            && best.overlap.saturating_mul(3) >= rival.overlap.saturating_mul(2);
        if rival.score <= best.score.saturating_add(margin) && similar {
            return Motion::Unaligned;
        }
    }
    let moved = best.score == 0 || best.score.saturating_add(2) < still;
    let close = best.score <= 28;
    let obvious = best.score.saturating_mul(2).saturating_add(8) < still;
    if !moved || !(close || obvious) {
        return Motion::Unaligned;
    }
    Motion::Shift(best.shift)
}

struct Alignment {
    shift: i32,
    lo: i32,
    hi: i32,
    score: u32,
    overlap: u32,
}

fn better_alignment(left: &Alignment, right: &Alignment) -> bool {
    // Similar lines can score almost as well as the real scroll. Prefer the
    // offset that covers more of the screen, but never over a perfect match.
    let gap = left.score.abs_diff(right.score);
    if gap <= 4 && left.score > 0 && right.score > 0 {
        let left_longer = left.overlap > right.overlap.saturating_mul(3) / 2;
        let right_longer = right.overlap > left.overlap.saturating_mul(3) / 2;
        if left_longer && !right_longer {
            return true;
        }
        if right_longer && !left_longer {
            return false;
        }
    }
    left.score < right.score || (left.score == right.score && left.overlap > right.overlap)
}

fn same_peak(peak: &Alignment, shift: i32) -> bool {
    (shift - peak.shift).abs() < PEAK_GAP
        || shift == peak.lo.saturating_sub(1)
        || shift == peak.hi.saturating_add(1)
}

fn widen_peak(peak: &mut Alignment, shift: i32, overlap: u32) {
    peak.lo = peak.lo.min(shift);
    peak.hi = peak.hi.max(shift);
    // Text rows stay identical for a few pixels around the real scroll.
    // The middle of that flat run is the offset, not the edge with more overlap.
    peak.shift = peak.lo + (peak.hi - peak.lo) / 2;
    peak.overlap = peak.overlap.max(overlap);
}

fn consider_shift(
    best: &mut Option<Alignment>,
    rival: &mut Option<Alignment>,
    previous: &[[u8; ROW_SAMPLES]],
    next: &[[u8; ROW_SAMPLES]],
    shift: i32,
    voting: &[u32],
    sticky: &[bool],
    weights: &[u32],
    header: i32,
) {
    let score = score_rows(
        previous,
        next,
        shift,
        voting.iter().copied(),
        sticky,
        true,
        weights,
        header,
    );
    if score == u32::MAX {
        return;
    }
    let overlap = overlap_count(voting, shift, sticky, weights, header);
    // A dark editor has only a few rows that are not background. A long flick
    // leaves just those rows in common, so a perfect match of them still counts.
    // A vague match has to cover more of the screen.
    let min_overlap = if score == 0 {
        2
    } else {
        (voting.len() as u32 / 5).max(8)
    };
    if overlap < min_overlap {
        return;
    }
    let found = Alignment {
        shift,
        lo: shift,
        hi: shift,
        score,
        overlap,
    };
    if let Some(current) = best.as_mut()
        && same_peak(current, shift)
    {
        if found.score < current.score {
            *current = found;
        } else if found.score == current.score {
            widen_peak(current, shift, overlap);
        }
        return;
    }
    if best
        .as_ref()
        .is_none_or(|current| better_alignment(&found, current))
    {
        if let Some(current) = best.take() {
            if rival
                .as_ref()
                .is_none_or(|other| better_alignment(&current, other))
            {
                *rival = Some(current);
            }
        }
        *best = Some(found);
        return;
    }
    if let Some(current) = rival.as_mut()
        && same_peak(current, shift)
    {
        if found.score < current.score {
            *current = found;
        } else if found.score == current.score {
            widen_peak(current, shift, overlap);
        }
        return;
    }
    if rival
        .as_ref()
        .is_none_or(|current| better_alignment(&found, current))
    {
        *rival = Some(found);
    }
}

fn sticky_header(sticky: &[bool]) -> i32 {
    let mut last = 0usize;
    let mut misses = 0usize;
    for (index, row) in sticky.iter().enumerate() {
        if *row {
            last = index + 1;
            misses = 0;
        } else {
            misses += 1;
            if misses > 2 {
                break;
            }
        }
    }
    last as i32
}

fn row_static(left: &[u8; ROW_SAMPLES], right: &[u8; ROW_SAMPLES]) -> bool {
    row_sad(left, right) / ROW_SAMPLES as u32 <= 4
}

fn row_sad(left: &[u8; ROW_SAMPLES], right: &[u8; ROW_SAMPLES]) -> u32 {
    let mut total = 0u32;
    for sample in 0..ROW_SAMPLES {
        total = total.saturating_add(u32::from(left[sample].abs_diff(right[sample])));
    }
    total
}

fn row_samples(frame: &[u8], width: u32, height: u32) -> Vec<[u8; ROW_SAMPLES]> {
    // Include the left edge. Line numbers and other gutter marks are what
    // separate two screens of similar code.
    let left = 0u32;
    let right = width.saturating_sub(1).max(left.saturating_add(1));
    let span = right - left;
    let mut rows = vec![[0u8; ROW_SAMPLES]; height as usize];
    for y in 0..height {
        for sample in 0..ROW_SAMPLES {
            let x = left + span.saturating_sub(1) * sample as u32 / (ROW_SAMPLES as u32 - 1);
            let offset = (y as usize * width as usize + x as usize) * 4;
            let luma = (u16::from(frame[offset]) * 3
                + u16::from(frame[offset + 1]) * 6
                + u16::from(frame[offset + 2]))
                / 10;
            rows[y as usize][sample] = luma as u8;
        }
    }
    rows
}

fn row_weights(rows: &[[u8; ROW_SAMPLES]]) -> Vec<u32> {
    let height = rows.len();
    let mut weights = vec![40u32; height];
    for y in 0..height {
        let mut nearest = u32::MAX;
        let mut other = 0usize;
        while other < height {
            if (other as i32 - y as i32).abs() >= 8 {
                nearest = nearest.min(row_sad(&rows[y], &rows[other]) / ROW_SAMPLES as u32);
                if nearest == 0 {
                    break;
                }
            }
            other += 4;
        }
        weights[y] = nearest.min(40);
    }
    weights
}

/// Positive shift means the page moved up: previous row y is next row y - shift.
fn score_rows(
    previous: &[[u8; ROW_SAMPLES]],
    next: &[[u8; ROW_SAMPLES]],
    shift: i32,
    rows: impl Iterator<Item = u32>,
    sticky: &[bool],
    skip_sticky: bool,
    weights: &[u32],
    header: i32,
) -> u32 {
    let height = previous.len() as i32;
    let mut total = 0u64;
    let mut weight = 0u64;
    for y in rows {
        let other = y as i32 - shift;
        if other < 0 || other >= height {
            continue;
        }
        if skip_sticky && (sticky[y as usize] || other < header) {
            continue;
        }
        let row_weight = u64::from(weights[y as usize]);
        if row_weight == 0 {
            continue;
        }
        total = total.saturating_add(
            u64::from(row_sad(&previous[y as usize], &next[other as usize])) * row_weight,
        );
        weight = weight.saturating_add(row_weight);
    }
    if weight == 0 {
        u32::MAX
    } else {
        (total / (weight * ROW_SAMPLES as u64)) as u32
    }
}

fn overlap_count(voting: &[u32], shift: i32, sticky: &[bool], weights: &[u32], header: i32) -> u32 {
    let height = sticky.len() as i32;
    voting
        .iter()
        .filter(|y| {
            weights[**y as usize] > 0 && {
                let other = **y as i32 - shift;
                other >= header && other < height
            }
        })
        .count() as u32
}

enum Step {
    Still,
    Moved { up: bool },
    Full,
}

struct Stitch {
    width: u32,
    height: u32,
    view_top: u32,
    view_height: u32,
    rgba: Vec<u8>,
    /// Last raw grab. Matching uses this, not a slice of the strip, so a fixed
    /// toolbar stays at the top of the long image instead of being stamped
    /// again at every scroll position.
    previous: Vec<u8>,
}

impl Stitch {
    fn new(frame: Vec<u8>, width: u32, height: u32) -> Self {
        let previous = frame.clone();
        Self {
            width,
            height,
            view_top: 0,
            view_height: height,
            rgba: frame,
            previous,
        }
    }

    fn push(&mut self, frame: &[u8], max_height: u32) -> Step {
        let step = match find_shift(&self.previous, frame, self.width, self.view_height) {
            Motion::Unchanged | Motion::Unaligned => return Step::Still,
            Motion::Shift(shift) if shift > 0 => self.grow_down(frame, shift as u32, max_height),
            Motion::Shift(shift) => self.grow_up(frame, shift.unsigned_abs(), max_height),
        };
        if matches!(step, Step::Moved { .. }) {
            self.previous.clear();
            self.previous.extend_from_slice(frame);
        }
        step
    }

    fn view(&self) -> &[u8] {
        &self.previous
    }

    fn grow_down(&mut self, frame: &[u8], shift: u32, max_height: u32) -> Step {
        let new_top = self.view_top.saturating_add(shift);
        let bottom = new_top.saturating_add(self.view_height);
        if bottom <= self.height {
            self.view_top = new_top;
            return Step::Moved { up: false };
        }
        let extra = bottom - self.height;
        let room = max_height.saturating_sub(self.height);
        let source = self.view_height.saturating_sub(extra);
        let take = extra.min(room).min(self.view_height.saturating_sub(source));
        if take == 0 {
            return Step::Full;
        }
        self.append(frame, source, take);
        if take < extra {
            self.view_top = self.view_top.saturating_add(take);
            return Step::Full;
        }
        self.view_top = new_top;
        if self.height >= max_height {
            Step::Full
        } else {
            Step::Moved { up: false }
        }
    }

    fn grow_up(&mut self, frame: &[u8], shift: u32, max_height: u32) -> Step {
        if self.view_top >= shift {
            self.view_top -= shift;
            return Step::Moved { up: true };
        }
        let missing = shift - self.view_top;
        let room = max_height.saturating_sub(self.height);
        let take = missing.min(room);
        if take == 0 {
            return Step::Full;
        }
        self.prepend(frame, take);
        self.view_top = 0;
        if take < missing || self.height >= max_height {
            Step::Full
        } else {
            Step::Moved { up: true }
        }
    }

    fn replace_view(&mut self, frame: &[u8]) {
        let stride = self.width as usize * 4;
        let start = self.view_top as usize * stride;
        let end = start + self.view_height as usize * stride;
        if frame.len() == end - start && end <= self.rgba.len() {
            self.rgba[start..end].copy_from_slice(frame);
        }
        self.previous.clear();
        self.previous.extend_from_slice(frame);
    }

    fn append(&mut self, frame: &[u8], source_y: u32, count: u32) {
        let stride = self.width as usize * 4;
        let start = source_y as usize * stride;
        let end = start + count as usize * stride;
        self.rgba.extend_from_slice(&frame[start..end]);
        self.height = self.height.saturating_add(count);
    }

    fn prepend(&mut self, frame: &[u8], count: u32) {
        let stride = self.width as usize * 4;
        let end = count as usize * stride;
        let mut combined = Vec::with_capacity(end + self.rgba.len());
        combined.extend_from_slice(&frame[..end]);
        combined.extend_from_slice(&self.rgba);
        self.rgba = combined;
        self.height = self.height.saturating_add(count);
    }
}

pub struct Strip {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub struct Cursor {
    pub x: f64,
    pub y: f64,
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

const PREVIEW_WIDTH: u32 = 220;
const PREVIEW_HEIGHT: u32 = 320;

pub fn preview_png(rgba: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let (width, height, pixels) = fit_preview(rgba, width, height, PREVIEW_WIDTH, PREVIEW_HEIGHT)?;
    encode_preview(&pixels, width, height)
}

/// One still of the finished strip, sized for the result window.
pub fn preview_png_within(
    rgba: &[u8],
    width: u32,
    height: u32,
    max_width: u32,
    max_height: u32,
) -> Option<Vec<u8>> {
    if width == 0
        || height == 0
        || max_width == 0
        || max_height == 0
        || rgba.len() != width as usize * height as usize * 4
    {
        return None;
    }
    let owned = image::RgbaImage::from_raw(width, height, rgba.to_vec())?;
    let scale = (f64::from(max_width) / f64::from(width))
        .min(f64::from(max_height) / f64::from(height))
        .min(1.0);
    let out_width = (f64::from(width) * scale)
        .round()
        .clamp(1.0, f64::from(max_width)) as u32;
    let out_height = (f64::from(height) * scale)
        .round()
        .clamp(1.0, f64::from(max_height)) as u32;
    let resized = if out_width == width && out_height == height {
        owned
    } else {
        image::imageops::resize(
            &owned,
            out_width,
            out_height,
            image::imageops::FilterType::Triangle,
        )
    };
    encode_preview(resized.as_raw(), resized.width(), resized.height())
}

fn encode_preview(pixels: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut bytes);
    image::ImageEncoder::write_image(
        encoder,
        pixels,
        width,
        height,
        image::ExtendedColorType::Rgba8,
    )
    .ok()?;
    Some(bytes)
}

pub fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut index = 0;
    while index + 3 <= bytes.len() {
        let value = (u32::from(bytes[index]) << 16)
            | (u32::from(bytes[index + 1]) << 8)
            | u32::from(bytes[index + 2]);
        out.push(TABLE[((value >> 18) & 63) as usize] as char);
        out.push(TABLE[((value >> 12) & 63) as usize] as char);
        out.push(TABLE[((value >> 6) & 63) as usize] as char);
        out.push(TABLE[(value & 63) as usize] as char);
        index += 3;
    }
    let rest = bytes.len() - index;
    if rest == 1 {
        let value = u32::from(bytes[index]) << 16;
        out.push(TABLE[((value >> 18) & 63) as usize] as char);
        out.push(TABLE[((value >> 12) & 63) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rest == 2 {
        let value = (u32::from(bytes[index]) << 16) | (u32::from(bytes[index + 1]) << 8);
        out.push(TABLE[((value >> 18) & 63) as usize] as char);
        out.push(TABLE[((value >> 12) & 63) as usize] as char);
        out.push(TABLE[((value >> 6) & 63) as usize] as char);
        out.push('=');
    }
    out
}

fn fit_preview(
    rgba: &[u8],
    width: u32,
    height: u32,
    max_width: u32,
    max_height: u32,
) -> Option<(u32, u32, Vec<u8>)> {
    if width == 0 || height == 0 || rgba.len() != width as usize * height as usize * 4 {
        return None;
    }
    let scale = (f64::from(max_width) / f64::from(width))
        .min(f64::from(max_height) / f64::from(height))
        .min(1.0);
    let out_width = (f64::from(width) * scale)
        .round()
        .clamp(1.0, f64::from(max_width)) as u32;
    let out_height = (f64::from(height) * scale)
        .round()
        .clamp(1.0, f64::from(max_height)) as u32;
    let mut pixels = vec![0; out_width as usize * out_height as usize * 4];
    for y in 0..out_height {
        let source_y = (u64::from(y) * u64::from(height) / u64::from(out_height)) as u32;
        for x in 0..out_width {
            let source_x = (u64::from(x) * u64::from(width) / u64::from(out_width)) as u32;
            let source = (source_y as usize * width as usize + source_x as usize) * 4;
            let target = (y as usize * out_width as usize + x as usize) * 4;
            pixels[target..target + 4].copy_from_slice(&rgba[source..source + 4]);
        }
    }
    Some((out_width, out_height, pixels))
}

pub fn capture_strip(
    monitor_index: usize,
    origin_x: u32,
    origin_y: u32,
    width: u32,
    height: u32,
    max_height: u32,
    cursor: Cursor,
    mut progress: impl FnMut(&[u8], u32, u32),
) -> Result<Strip, ScreenshotError> {
    if width < MIN_WIDTH
        || height < MIN_HEIGHT
        || origin_x.checked_add(width).is_none()
        || origin_y.checked_add(height).is_none()
    {
        return Err(ScreenshotError::with_params(
            "invalid_input",
            serde_json::json!({ "reason": "scroll" }),
        ));
    }
    let max_height = max_height.max(height);
    let monitor = open_monitor(monitor_index)?;
    thread::sleep(Duration::from_millis(150));
    let first = grab_region(&monitor, origin_x, origin_y, width, height)?;
    let mut stitch = Stitch::new(first, width, height);
    progress(&stitch.rgba, stitch.width, stitch.height);
    let mut grown = false;
    let mut synced = false;
    let mut misses = 0u32;
    let mut hold_auto = Instant::now();
    let mut input_ready = false;
    let mut saw_auto = false;
    let mut next_preview = Instant::now();
    let deadline = Instant::now() + Duration::from_secs(90);
    while !stopped() && stitch.height < max_height && Instant::now() < deadline {
        let enabled = auto_enabled();
        if enabled && !saw_auto {
            input_ready = prepare_input();
        }
        if !enabled {
            input_ready = false;
        }
        saw_auto = enabled;
        let driving = enabled && input_ready && Instant::now() >= hold_auto;
        if driving && !scroll_down(&cursor) {
            input_ready = false;
        }
        // Manual scrolling is a flick, not a 40ms step. Sample again as soon as
        // the previous grab is done so the next frame still overlaps this one.
        thread::sleep(Duration::from_millis(if driving { 160 } else { 8 }));
        if stopped() {
            break;
        }
        let frame = grab_region(&monitor, origin_x, origin_y, width, height)?;
        if frame.len() != stitch.view().len() {
            break;
        }
        match stitch.push(&frame, max_height) {
            Step::Moved { up } => {
                grown = true;
                misses = 0;
                if up {
                    hold_auto = Instant::now() + Duration::from_millis(800);
                }
                // Encoding the live preview on every jump stalls the next grab
                // and turns a fast flick into a gap.
                if Instant::now() >= next_preview {
                    progress(&stitch.rgba, stitch.width, stitch.height);
                    next_preview = Instant::now() + Duration::from_millis(90);
                }
            }
            Step::Full => {
                progress(&stitch.rgba, stitch.width, stitch.height);
                break;
            }
            Step::Still => {
                let motion = find_shift(stitch.view(), &frame, width, height);
                if matches!(motion, Motion::Unchanged) {
                    continue;
                }
                // The first grab can still show the capture overlay. Adopt the
                // live page once, then keep every row that has already been
                // stitched instead of replacing the strip with the latest view.
                if !grown && !synced {
                    misses = misses.saturating_add(1);
                    if misses >= 2 {
                        stitch.replace_view(&frame);
                        synced = true;
                        misses = 0;
                        progress(&stitch.rgba, stitch.width, stitch.height);
                    }
                }
            }
        }
    }
    Ok(Strip {
        width: stitch.width,
        height: stitch.height,
        rgba: stitch.rgba,
    })
}

fn open_monitor(index: usize) -> Result<xcap::Monitor, ScreenshotError> {
    let monitors = xcap::Monitor::all().map_err(|error| {
        eprintln!("screenshot scroll monitors: {error}");
        ScreenshotError::new("capture_failed")
    })?;
    monitors
        .into_iter()
        .nth(index)
        .ok_or_else(|| ScreenshotError::new("capture_failed"))
}

fn grab_region(
    monitor: &xcap::Monitor,
    origin_x: u32,
    origin_y: u32,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, ScreenshotError> {
    let scale = f64::from(monitor.scale_factor().unwrap_or(1.0)).max(1.0);
    let logical = !cfg!(target_os = "windows");
    let (x, y, region_w, region_h) = if logical {
        (
            (f64::from(origin_x) / scale).round().max(0.0) as u32,
            (f64::from(origin_y) / scale).round().max(0.0) as u32,
            ((f64::from(width) / scale).round() as u32).max(1),
            ((f64::from(height) / scale).round() as u32).max(1),
        )
    } else {
        (origin_x, origin_y, width, height)
    };
    if let Ok(image) = monitor.capture_region(x, y, region_w, region_h) {
        if let Some(pixels) = normalize_capture(&image, width, height) {
            return Ok(pixels);
        }
    }
    let image = monitor.capture_image().map_err(|error| {
        eprintln!("screenshot scroll capture: {error}");
        ScreenshotError::new("capture_failed")
    })?;
    crop_image(&image, origin_x, origin_y, width, height)
}

fn normalize_capture(image: &image::RgbaImage, width: u32, height: u32) -> Option<Vec<u8>> {
    if image.width() == width && image.height() == height {
        return Some(image.as_raw().clone());
    }
    if image.width() >= width
        && image.height() >= height
        && image.width() - width <= 4
        && image.height() - height <= 4
    {
        return crop_raw(image.as_raw(), image.width(), 0, 0, width, height);
    }
    None
}

fn crop_image(
    image: &image::RgbaImage,
    origin_x: u32,
    origin_y: u32,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, ScreenshotError> {
    if origin_x.saturating_add(width) > image.width()
        || origin_y.saturating_add(height) > image.height()
    {
        return Err(ScreenshotError::with_params(
            "invalid_input",
            serde_json::json!({ "reason": "scroll" }),
        ));
    }
    crop_raw(
        image.as_raw(),
        image.width(),
        origin_x,
        origin_y,
        width,
        height,
    )
    .ok_or_else(|| ScreenshotError::new("capture_failed"))
}

fn crop_raw(
    pixels: &[u8],
    source_width: u32,
    origin_x: u32,
    origin_y: u32,
    width: u32,
    height: u32,
) -> Option<Vec<u8>> {
    let stride = source_width as usize * 4;
    let row = width as usize * 4;
    if origin_x.saturating_add(width) > source_width {
        return None;
    }
    let mut cropped = Vec::with_capacity(row * height as usize);
    for y in origin_y..origin_y + height {
        let start = y as usize * stride + origin_x as usize * 4;
        let end = start.checked_add(row)?;
        if end > pixels.len() {
            return None;
        }
        cropped.extend_from_slice(&pixels[start..end]);
    }
    Some(cropped)
}

fn prepare_input() -> bool {
    #[cfg(target_os = "macos")]
    {
        if crate::permissions::accessibility_granted() {
            return true;
        }
        crate::permissions::prompt_accessibility();
        return false;
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

fn scroll_down(cursor: &Cursor) -> bool {
    let _ = (cursor.x, cursor.y);
    if !pointer_in_selection(cursor) {
        return true;
    }
    #[cfg(target_os = "macos")]
    {
        return macos_scroll(-4);
    }
    #[cfg(target_os = "windows")]
    {
        return windows_scroll(-240);
    }
    #[cfg(all(target_os = "linux", not(target_os = "android")))]
    {
        return linux_scroll(cursor);
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = cursor;
        false
    }
}

fn point_inside(x: f64, y: f64, area: &Cursor) -> bool {
    x >= area.left && x < area.right && y >= area.top && y < area.bottom
}

fn pointer_in_selection(area: &Cursor) -> bool {
    #[cfg(target_os = "macos")]
    {
        let Some(point) = mouse_point() else {
            return true;
        };
        return point_inside(point.x, point.y, area);
    }
    #[cfg(target_os = "windows")]
    {
        return windows_pointer_inside(area);
    }
    #[cfg(target_os = "linux")]
    {
        return linux_pointer_inside(area);
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = area;
        true
    }
}

#[cfg(target_os = "windows")]
fn windows_pointer_inside(area: &Cursor) -> bool {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut point = POINT::default();
    if unsafe { GetCursorPos(&mut point) }.is_err() {
        return true;
    }
    point_inside(f64::from(point.x), f64::from(point.y), area)
}

#[cfg(target_os = "linux")]
fn linux_pointer_inside(area: &Cursor) -> bool {
    if super::capture::is_wayland() {
        return true;
    }
    let Some((x, y)) = x11_pointer() else {
        return true;
    };
    point_inside(x, y, area)
}

#[cfg(target_os = "linux")]
fn x11_pointer() -> Option<(f64, f64)> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::ConnectionExt;

    let (connection, screen_index) = x11rb::connect(None).ok()?;
    let root = connection.setup().roots.get(screen_index)?.root;
    let reply = connection.query_pointer(root).ok()?.reply().ok()?;
    Some((f64::from(reply.root_x), f64::from(reply.root_y)))
}

#[cfg(target_os = "macos")]
fn mouse_point() -> Option<CgPoint> {
    unsafe {
        let event = CGEventCreate(std::ptr::null_mut());
        if event.is_null() {
            return None;
        }
        let point = CGEventGetLocation(event);
        CFRelease(event);
        Some(point)
    }
}

#[cfg(target_os = "macos")]
fn macos_scroll(lines: i32) -> bool {
    const HID: u32 = 0;
    const LINE: u32 = 1;
    unsafe {
        let event = CGEventCreateScrollWheelEvent(std::ptr::null_mut(), LINE, 1, lines);
        if event.is_null() {
            return false;
        }
        CGEventPost(HID, event);
        CFRelease(event);
    }
    true
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
    fn CGEventCreateScrollWheelEvent(
        source: *mut std::ffi::c_void,
        units: u32,
        wheel_count: u32,
        wheel1: i32,
    ) -> *mut std::ffi::c_void;
    fn CGEventPost(tap: u32, event: *mut std::ffi::c_void);
}

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: *const std::ffi::c_void);
}

#[cfg(target_os = "windows")]
fn windows_scroll(delta: i32) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_WHEEL, MOUSEINPUT, SendInput,
    };
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: delta as u32,
                dwFlags: MOUSEEVENTF_WHEEL,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) > 0 }
}

#[cfg(target_os = "linux")]
fn linux_scroll(cursor: &Cursor) -> bool {
    if super::capture::is_wayland() {
        return false;
    }
    let Ok((connection, screen_index)) = x11rb::connect(None) else {
        return false;
    };
    let Some(screen) = connection.setup().roots.get(screen_index) else {
        return false;
    };
    let root = screen.root;
    let _ = cursor;
    use x11rb::connection::Connection;
    use x11rb::protocol::xtest::ConnectionExt as _;
    for kind in [4u8, 5u8] {
        if connection
            .xtest_fake_input(kind, 5, 0, root, 0, 0, 0)
            .is_err()
        {
            return false;
        }
    }
    connection.flush().is_ok()
}

#[cfg(test)]
mod tests {
    use super::{Cursor, Motion, Stitch, find_shift, point_inside};

    fn rows(width: u32, height: u32, origin: u32) -> Vec<u8> {
        contrast_rows_scaled(width, height, origin, 1)
    }

    fn contrast_rows(width: u32, height: u32, origin: u32) -> Vec<u8> {
        contrast_rows_scaled(width, height, origin, 8)
    }

    fn plain_rows(width: u32, height: u32, origin: u32) -> Vec<u8> {
        let mut rgba = vec![0u8; width as usize * height as usize * 4];
        for y in 0..height {
            let value = origin + y;
            for x in 0..width {
                let offset = (y * width + x) as usize * 4;
                rgba[offset] = value as u8;
                rgba[offset + 1] = value as u8;
                rgba[offset + 2] = value as u8;
                rgba[offset + 3] = 255;
            }
        }
        rgba
    }

    fn contrast_rows_scaled(width: u32, height: u32, origin: u32, scale: u32) -> Vec<u8> {
        let mut rgba = vec![0u8; width as usize * height as usize * 4];
        for y in 0..height {
            let value = (origin.saturating_add(y).saturating_mul(scale)) as u8;
            for x in 0..width {
                let offset = (y * width + x) as usize * 4;
                rgba[offset] = value;
                rgba[offset + 1] = value.wrapping_mul(3);
                rgba[offset + 2] = value.wrapping_mul(7);
                rgba[offset + 3] = 255;
            }
        }
        rgba
    }

    #[test]
    fn auto_scroll_stays_inside_the_selection() {
        let area = Cursor {
            x: 50.0,
            y: 40.0,
            left: 10.0,
            top: 20.0,
            right: 110.0,
            bottom: 80.0,
        };
        assert!(point_inside(10.0, 20.0, &area));
        assert!(!point_inside(110.0, 40.0, &area));
        assert!(!point_inside(9.0, 40.0, &area));
    }

    #[test]
    fn finds_a_two_pixel_scroll() {
        let previous = contrast_rows(16, 24, 0);
        let next = contrast_rows(16, 24, 2);
        assert!(matches!(
            find_shift(&previous, &next, 16, 24),
            Motion::Shift(2)
        ));
    }

    #[test]
    fn accepts_a_noisy_alignment_when_the_page_clearly_moved() {
        let previous = plain_rows(32, 48, 0);
        let mut next = plain_rows(32, 48, 6);
        for (index, pixel) in next.chunks_mut(4).enumerate() {
            let bump = (index % 3) as u8;
            pixel[0] = pixel[0].saturating_add(bump);
            pixel[1] = pixel[1].saturating_add(bump);
            pixel[2] = pixel[2].saturating_add(bump);
        }
        let motion = find_shift(&previous, &next, 32, 48);
        assert!(
            matches!(motion, Motion::Shift(shift) if (5..=7).contains(&shift)),
            "motion = {motion:?}"
        );
    }

    fn lined_rows(width: u32, height: u32, origin: u32) -> Vec<u8> {
        let mut rgba = vec![0u8; width as usize * height as usize * 4];
        for y in 0..height {
            let absolute = origin + y;
            for x in 0..width {
                let offset = (y * width + x) as usize * 4;
                rgba[offset] = absolute as u8;
                rgba[offset + 1] = x.wrapping_mul(3).wrapping_add(absolute) as u8;
                rgba[offset + 2] = ((absolute % 18) * 11) as u8;
                rgba[offset + 3] = 255;
            }
        }
        rgba
    }

    fn periodic_rows(width: u32, height: u32, origin: u32) -> Vec<u8> {
        let mut rgba = vec![0u8; width as usize * height as usize * 4];
        for y in 0..height {
            let value = ((origin + y) % 16) * 12;
            for x in 0..width {
                let offset = (y * width + x) as usize * 4;
                rgba[offset] = value as u8;
                rgba[offset + 1] = value as u8;
                rgba[offset + 2] = value as u8;
                rgba[offset + 3] = 255;
            }
        }
        rgba
    }

    #[test]
    fn finds_a_fast_jump_instead_of_the_repeated_spacing() {
        let previous = lined_rows(48, 120, 0);
        let next = lined_rows(48, 120, 47);
        let motion = find_shift(&previous, &next, 48, 120);
        assert!(
            matches!(motion, Motion::Shift(shift) if (45..=49).contains(&shift)),
            "{motion:?}"
        );
    }

    fn document_rows(width: u32, height: u32, origin: u32) -> Vec<u8> {
        let mut rgba = vec![248u8; width as usize * height as usize * 4];
        for y in 0..height {
            let absolute = origin + y;
            if absolute % 22 > 13 {
                continue;
            }
            for x in 0..width {
                let offset = (y * width + x) as usize * 4;
                let shared = if (x / 3) % 2 == 0 { 36 } else { 8 };
                let ink = ((absolute.wrapping_mul(13) as u16) + (x.wrapping_mul(7) as u16)) as u8;
                let value = 40u8.wrapping_add(shared).wrapping_add(ink);
                rgba[offset] = value;
                rgba[offset + 1] = value.saturating_sub(6);
                rgba[offset + 2] = value.saturating_add(4);
                rgba[offset + 3] = 255;
            }
        }
        rgba
    }

    fn add_grab_noise(rgba: &mut [u8], salt: u8) {
        for (index, pixel) in rgba.chunks_mut(4).enumerate() {
            let bump = ((index as u8).wrapping_mul(3).wrapping_add(salt)) % 7;
            for channel in 0..3 {
                pixel[channel] = pixel[channel].saturating_sub(3).saturating_add(bump);
            }
        }
    }

    #[test]
    fn stitches_a_document_while_the_grab_is_noisy() {
        let mut previous = document_rows(80, 220, 0);
        let mut next = document_rows(80, 220, 36);
        add_grab_noise(&mut previous, 1);
        add_grab_noise(&mut next, 4);
        let motion = find_shift(&previous, &next, 80, 220);
        assert!(
            matches!(motion, Motion::Shift(shift) if (34..=38).contains(&shift)),
            "{motion:?}"
        );
    }

    #[test]
    fn stitches_a_tall_document_without_dropping_the_scroll() {
        let previous = document_rows(240, 900, 0);
        let next = document_rows(240, 900, 70);
        let motion = find_shift(&previous, &next, 240, 900);
        assert!(
            matches!(motion, Motion::Shift(shift) if (68..=72).contains(&shift)),
            "{motion:?}"
        );
    }

    #[test]
    fn keeps_a_fixed_toolbar_once_while_the_code_scrolls() {
        let previous = editor_frame(48, 180, 0);
        let next = editor_frame(48, 180, 40);
        let motion = find_shift(&previous, &next, 48, 180);
        assert!(matches!(motion, Motion::Shift(40)), "{motion:?}");
        let mut stitch = Stitch::new(previous, 48, 180);
        assert!(matches!(
            stitch.push(&next, 2_000),
            super::Step::Moved { up: false }
        ));
        assert_eq!(stitch.height, 220);
        let stride = 48usize * 4;
        for y in 0..stitch.height {
            let offset = y as usize * stride;
            let is_bar = stitch.rgba[offset] == 255
                && stitch.rgba[offset + 1] == 0
                && stitch.rgba[offset + 2] == 128;
            assert_eq!(is_bar, y < 24, "row {y}");
        }
    }

    fn editor_frame(width: u32, height: u32, origin: u32) -> Vec<u8> {
        let mut rgba = vec![0u8; width as usize * height as usize * 4];
        let bar = 24u32.min(height / 3);
        for y in 0..height {
            for x in 0..width {
                let offset = (y * width + x) as usize * 4;
                if y < bar {
                    rgba[offset] = 255;
                    rgba[offset + 1] = 0;
                    rgba[offset + 2] = 128;
                } else {
                    let line = origin + (y - bar);
                    let ink = line.wrapping_mul(17).wrapping_add(x.wrapping_mul(3)) as u8;
                    rgba[offset] = 24u8.wrapping_add(ink);
                    rgba[offset + 1] = 28u8.wrapping_add(ink / 3);
                    rgba[offset + 2] = 36;
                }
                rgba[offset + 3] = 255;
            }
        }
        rgba
    }

    #[test]
    fn finds_a_one_line_document_scroll() {
        let previous = document_rows(64, 160, 14);
        let next = document_rows(64, 160, 36);
        let motion = find_shift(&previous, &next, 64, 160);
        assert!(matches!(motion, Motion::Shift(22)), "{motion:?}");
    }

    #[test]
    fn stitches_each_small_scroll_onto_the_strip() {
        let mut origin = 0u32;
        let mut stitch = Stitch::new(document_rows(64, 160, origin), 64, 160);
        for delta in [14u32, 22, 9, 31] {
            let held = document_rows(64, 160, origin);
            assert_eq!(
                stitch.view(),
                held.as_slice(),
                "view drifted before +{delta}"
            );
            origin += delta;
            let frame = document_rows(64, 160, origin);
            let motion = find_shift(stitch.view(), &frame, 64, 160);
            assert!(
                matches!(stitch.push(&frame, 2_000), super::Step::Moved { up: false }),
                "delta {delta} left height {} motion {motion:?}",
                stitch.height
            );
            assert_eq!(
                stitch.view(),
                frame.as_slice(),
                "view drifted after scrolling to {origin}"
            );
        }
        assert_eq!(stitch.height, 160 + 14 + 22 + 9 + 31);
    }

    fn editor_page(width: u32, height: u32, origin: u32, header: u32) -> Vec<u8> {
        let mut rgba = vec![24u8; width as usize * height as usize * 4];
        for y in 0..height {
            for x in 0..width {
                let offset = (y * width + x) as usize * 4;
                let (red, green, blue) = if y < header {
                    (255, 0, 128)
                } else {
                    let absolute = origin + y;
                    if x < 8 {
                        ((absolute.wrapping_mul(17)) as u8, 80, 120)
                    } else {
                        let ink =
                            ((absolute.wrapping_mul(13) as u16) + (x.wrapping_mul(7) as u16)) as u8;
                        (ink, ink.wrapping_add(12), ink.wrapping_add(24))
                    }
                };
                rgba[offset] = red;
                rgba[offset + 1] = green;
                rgba[offset + 2] = blue;
                rgba[offset + 3] = 255;
            }
        }
        rgba
    }

    #[test]
    fn stitches_a_scrolling_editor_without_repeating_its_header() {
        let first = editor_page(80, 200, 0, 32);
        let jumped = editor_page(80, 200, 48, 32);
        let motion = find_shift(&first, &jumped, 80, 200);
        assert!(
            matches!(motion, Motion::Shift(shift) if (46..=50).contains(&shift)),
            "{motion:?}"
        );
        let mut stitch = Stitch::new(first.clone(), 80, 200);
        assert!(matches!(
            stitch.push(&jumped, 4_000),
            super::Step::Moved { up: false }
        ));
        assert_eq!(stitch.height, 248);
        let stride = 80 * 4;
        assert_eq!(&stitch.rgba[0..4], &[255, 0, 128, 255]);
        for y in 0..stitch.height {
            let offset = y as usize * stride;
            let is_bar = stitch.rgba[offset] == 255
                && stitch.rgba[offset + 1] == 0
                && stitch.rgba[offset + 2] == 128;
            assert_eq!(is_bar, y < 32, "row {y}");
        }
    }

    #[test]
    fn stitches_a_fast_jump_by_the_full_distance() {
        let first = lined_rows(16, 90, 0);
        let mut stitch = Stitch::new(first, 16, 90);
        let jumped = lined_rows(16, 90, 40);
        assert!(matches!(
            stitch.push(&jumped, 500),
            super::Step::Moved { up: false }
        ));
        assert_eq!(stitch.height, 130);
        assert_eq!(stitch.view_top, 40);
    }

    #[test]
    fn skips_a_periodic_page_when_two_shifts_match() {
        let previous = periodic_rows(32, 64, 0);
        let mut next = periodic_rows(32, 64, 16);
        // The new tail keeps the frames from being identical, while a 16px
        // and a 32px alignment still explain the repeated block equally.
        for y in 48..64 {
            for x in 0..32 {
                let offset = (y * 32 + x) as usize * 4;
                next[offset] = 255;
                next[offset + 1] = 0;
                next[offset + 2] = y as u8;
            }
        }
        assert!(
            matches!(find_shift(&previous, &next, 32, 64), Motion::Unaligned),
            "a repeated pattern must not be stitched at a guessed offset"
        );
    }

    #[test]
    fn finds_a_scroll_larger_than_the_middle_band() {
        let previous = plain_rows(32, 80, 0);
        let next = plain_rows(32, 80, 50);
        let motion = find_shift(&previous, &next, 32, 80);
        assert!(matches!(motion, Motion::Shift(50)), "{motion:?}");
    }

    #[test]
    fn finds_a_downward_scroll_past_a_fixed_header() {
        let mut previous = rows(24, 40, 0);
        let mut next = rows(24, 40, 10);
        for image in [&mut previous, &mut next] {
            for y in 0..6 {
                for x in 0..24 {
                    let offset = (y * 24 + x) as usize * 4;
                    image[offset] = 255;
                    image[offset + 1] = 0;
                    image[offset + 2] = 0;
                }
            }
        }
        let motion = find_shift(&previous, &next, 24, 40);
        assert!(matches!(motion, Motion::Shift(10)), "{motion:?}");
    }

    #[test]
    fn finds_an_upward_scroll() {
        let previous = rows(20, 36, 12);
        let next = rows(20, 36, 4);
        assert!(matches!(
            find_shift(&previous, &next, 20, 36),
            Motion::Shift(-8)
        ));
    }

    #[test]
    fn ignores_an_unchanged_frame() {
        let frame = rows(16, 32, 3);
        assert!(matches!(
            find_shift(&frame, &frame, 16, 32),
            Motion::Unchanged
        ));
    }

    #[test]
    fn stitches_down_and_reuses_pixels_when_scrolling_back() {
        let first = rows(12, 30, 0);
        let mut stitch = Stitch::new(first, 12, 30);
        let down = rows(12, 30, 10);
        assert!(matches!(
            stitch.push(&down, 200),
            super::Step::Moved { up: false }
        ));
        assert_eq!(stitch.height, 40);
        assert_eq!(stitch.view_top, 10);
        let back = rows(12, 30, 0);
        assert!(matches!(
            stitch.push(&back, 200),
            super::Step::Moved { up: true }
        ));
        assert_eq!(stitch.height, 40);
        assert_eq!(stitch.view_top, 0);
    }

    #[test]
    fn preview_keeps_the_top_left_pixel_and_fits_the_long_side() {
        let mut rgba = vec![0; 8 * 20 * 4];
        rgba[0] = 12;
        rgba[1] = 34;
        rgba[2] = 56;
        rgba[3] = 255;
        let (width, height, pixels) = super::fit_preview(&rgba, 8, 20, 4, 4).unwrap();
        assert_eq!((width, height), (2, 4));
        assert_eq!(&pixels[0..4], &[12, 34, 56, 255]);
        assert!(
            super::preview_png(&rgba, 8, 20)
                .unwrap()
                .starts_with(b"\x89PNG")
        );
    }

    #[test]
    fn result_preview_shrinks_a_long_image() {
        let rgba = vec![200; 8 * 40 * 4];
        let png = super::preview_png_within(&rgba, 8, 40, 4, 4).unwrap();
        let image = image::load_from_memory(&png).unwrap();
        assert_eq!((image.width(), image.height()), (1, 4));
    }

    #[test]
    fn base64_pads_partial_blocks() {
        assert_eq!(super::encode_base64(b"Man"), "TWFu");
        assert_eq!(super::encode_base64(b"Ma"), "TWE=");
        assert_eq!(super::encode_base64(b"M"), "TQ==");
    }

    #[test]
    fn stitches_a_sparse_editor_when_only_some_rows_are_distinct() {
        for (bar, delta) in [
            (0u32, 22u32),
            (0, 66),
            (0, 200),
            (28, 22),
            (28, 80),
            (28, 220),
        ] {
            let previous = sparse_editor(720, 460, 0, bar);
            let next = sparse_editor(720, 460, delta, bar);
            let motion = find_shift(&previous, &next, 720, 460);
            // A long flick leaves only a couple of text rows in common, so the
            // flat match can sit a few pixels off the true offset.
            let slop = if delta >= 160 { 4 } else { 0 };
            assert!(
                matches!(motion, Motion::Shift(shift) if shift.abs_diff(delta as i32) <= slop),
                "bar {bar} delta {delta} -> {motion:?}"
            );
        }
        let first = sparse_editor(720, 460, 0, 28);
        let next = sparse_editor(720, 460, 80, 28);
        let mut stitch = Stitch::new(first, 720, 460);
        assert!(matches!(
            stitch.push(&next, 4_000),
            super::Step::Moved { up: false }
        ));
        assert_eq!(stitch.height, 540);
        let stride = 720 * 4;
        for y in 0..stitch.height {
            let offset = y as usize * stride;
            let is_bar = stitch.rgba[offset] == 24
                && stitch.rgba[offset + 1] == 24
                && stitch.rgba[offset + 2] == 28;
            assert_eq!(is_bar, y < 28, "row {y}");
        }
    }

    fn sparse_editor(width: u32, height: u32, scroll: u32, bar: u32) -> Vec<u8> {
        let mut rgba = vec![0u8; width as usize * height as usize * 4];
        for pixel in rgba.chunks_mut(4) {
            pixel[0] = 30;
            pixel[1] = 31;
            pixel[2] = 36;
            pixel[3] = 255;
        }
        for y in 0..bar.min(height) {
            for x in 0..width {
                let offset = (y * width + x) as usize * 4;
                rgba[offset] = 24;
                rgba[offset + 1] = 24;
                rgba[offset + 2] = 28;
            }
        }
        let line_h = 22u32;
        let gutter = 56u32;
        let mut doc = scroll / line_h;
        let mut y0 = bar as i32 - (scroll % line_h) as i32;
        while y0 < height as i32 {
            let band_top = y0 + 5;
            let band_bottom = y0 + 16;
            for y in band_top..band_bottom {
                if y < 0 || y >= height as i32 || (y as u32) < bar {
                    continue;
                }
                let y = y as u32;
                if doc % 6 != 4 {
                    for x in 18..34 {
                        if x >= width {
                            break;
                        }
                        let offset = (y * width + x) as usize * 4;
                        let ink = (40 + (doc * 17) % 180) as u8;
                        rgba[offset] = ink;
                        rgba[offset + 1] = ink.wrapping_add(20);
                        rgba[offset + 2] = 140;
                    }
                    let text_x = gutter + 12 + (doc % 9) * 6;
                    if text_x + 10 < width {
                        for x in text_x..text_x + 10 {
                            let offset = (y * width + x) as usize * 4;
                            rgba[offset] = 180;
                            rgba[offset + 1] = 190;
                            rgba[offset + 2] = 200;
                        }
                    }
                }
            }
            if doc == 8 || doc == 9 || doc == 10 {
                for y in
                    (y0.max(bar as i32) as u32)..(y0 + line_h as i32).clamp(0, height as i32) as u32
                {
                    for x in gutter..width.min(gutter + 80) {
                        let offset = (y * width + x) as usize * 4;
                        rgba[offset] = 50;
                        rgba[offset + 1] = 52;
                        rgba[offset + 2] = 64;
                    }
                }
            }
            doc += 1;
            y0 += line_h as i32;
        }
        rgba
    }
}
