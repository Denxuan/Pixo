use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
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
/// Mean per-channel difference above this is not the same row.
const ALIGN_LIMIT: u32 = 18;
/// Peaks closer than this are the same alignment, not two ways the page could have moved.
const PEAK_GAP: i32 = 8;

struct Placement {
    motion: Motion,
    header: u32,
    footer: u32,
}

fn find_shift(previous: &[u8], next: &[u8], width: u32, height: u32) -> Motion {
    placement(previous, next, width, height).motion
}

fn placement(previous: &[u8], next: &[u8], width: u32, height: u32) -> Placement {
    let miss = |motion| Placement {
        motion,
        header: 0,
        footer: 0,
    };
    if previous.len() != next.len() || width < 8 || height < 16 {
        return miss(Motion::Unaligned);
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
        u32::MAX,
        u64::MAX,
    );
    let static_rows = sticky.iter().filter(|row| **row).count();
    // A grabbed frame is noisy even when the page has not moved. Blank gaps
    // and a repeating line rhythm pull the average down after a real scroll,
    // so "unchanged" also requires almost every row to still sit in place.
    if unchanged <= 6 && static_rows.saturating_add(8) >= height as usize {
        return miss(Motion::Unchanged);
    }
    // Sticky headers match at shift 0 and drown out a real jump. Blank rows
    // match at every offset, so only rows that are actually distinctive vote.
    let header_px = header.max(0) as u32;
    let footer = footer_from_frames(previous, next, &previous_rows, width, height, header_px);
    let voting: Vec<u32> = (0..height)
        .filter(|&y| {
            !sticky[y as usize] && weights[y as usize] > 0 && y >= header_px && y + footer < height
        })
        .collect();
    if voting.len() < 8 {
        return Placement {
            motion: Motion::Unaligned,
            header: header_px,
            footer,
        };
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
        u32::MAX,
        u64::MAX,
    );
    // New rows come from the bottom of the next frame. A shift into the fixed
    // toolbar would paste that bar into the strip and skip the lines between.
    let max_shift = height.saturating_sub(header_px).saturating_sub(8).max(1) as i32;
    let mut best: Option<Alignment> = None;
    let mut rival: Option<Alignment> = None;
    scan_alignments(
        &mut best,
        &mut rival,
        &previous_rows,
        &next_rows,
        &voting,
        &sticky,
        &weights,
        header,
        max_shift,
        still,
    );
    let Some(best) = best else {
        return Placement {
            motion: Motion::Unaligned,
            header: header_px,
            footer,
        };
    };
    let moved = best.score == 0 || best.score.saturating_add(2) < still;
    let close = best.score <= 28;
    let obvious = best.score.saturating_mul(2).saturating_add(8) < still;
    if !moved || !(close || obvious) {
        return Placement {
            motion: Motion::Unaligned,
            header: header_px,
            footer,
        };
    }
    // Sampled rows treat two similar form lines as the same scroll. Confirm the
    // offset against the pixels, and drop a grab whose top and bottom moved apart.
    let mut candidates = vec![best.lo, best.shift, best.hi];
    if let Some(rival) = rival
        && rival.score <= best.score.saturating_add(10)
    {
        candidates.extend([rival.lo, rival.shift, rival.hi]);
    }
    let motion = match confirm_shift(
        previous,
        next,
        width,
        height,
        header_px,
        footer,
        max_shift,
        best.shift,
        &candidates,
    ) {
        Some(shift) => Motion::Shift(shift),
        None => Motion::Unaligned,
    };
    Placement {
        motion,
        header: header_px,
        footer,
    }
}

fn scan_alignments(
    best: &mut Option<Alignment>,
    rival: &mut Option<Alignment>,
    previous_rows: &[[u8; ROW_SAMPLES]],
    next_rows: &[[u8; ROW_SAMPLES]],
    voting: &[u32],
    sticky: &[bool],
    weights: &[u32],
    header: i32,
    max_shift: i32,
    still: u32,
) {
    // A shift worse than this can never be chosen. Score it only until that
    // is already guaranteed, instead of reading every remaining row. Every
    // voting row still votes: skipping rows made two similar form lines look
    // like the same scroll and pasted one section over the next.
    let obvious = still.saturating_sub(8) / 2;
    let abort_above = still.saturating_sub(3).min(obvious.max(28)).max(6);
    score_offsets(
        best,
        rival,
        previous_rows,
        next_rows,
        voting,
        sticky,
        weights,
        header,
        max_shift,
        abort_above,
    );
}

fn score_offsets(
    best: &mut Option<Alignment>,
    rival: &mut Option<Alignment>,
    previous_rows: &[[u8; ROW_SAMPLES]],
    next_rows: &[[u8; ROW_SAMPLES]],
    voting: &[u32],
    sticky: &[bool],
    weights: &[u32],
    header: i32,
    max_shift: i32,
    abort_above: u32,
) {
    let vote_weight = voting.iter().map(|y| u64::from(weights[*y as usize])).sum();
    let mut shift = 1i32;
    while shift <= max_shift {
        consider_shift(
            best,
            rival,
            previous_rows,
            next_rows,
            shift,
            voting,
            sticky,
            weights,
            header,
            abort_above,
            vote_weight,
        );
        consider_shift(
            best,
            rival,
            previous_rows,
            next_rows,
            -shift,
            voting,
            sticky,
            weights,
            header,
            abort_above,
            vote_weight,
        );
        shift += 1;
    }
}

fn confirm_shift(
    previous: &[u8],
    next: &[u8],
    width: u32,
    height: u32,
    header: u32,
    footer: u32,
    max_shift: i32,
    preferred: i32,
    candidates: &[i32],
) -> Option<i32> {
    let mut ranked: Vec<(i32, u32)> = Vec::new();
    for &coarse in candidates {
        let mut best_shift = 0i32;
        let mut best_sad = u32::MAX;
        for delta in -6..=6 {
            let shift = coarse + delta;
            if shift == 0 || shift.abs() > max_shift {
                continue;
            }
            let Some(sad) = content_sad(previous, next, width, height, shift, header, footer)
            else {
                continue;
            };
            let closer = best_shift != 0 && (shift - coarse).abs() < (best_shift - coarse).abs();
            if sad < best_sad || (sad == best_sad && closer) {
                best_sad = sad;
                best_shift = shift;
            }
        }
        if best_shift != 0 && best_sad <= ALIGN_LIMIT {
            if let Some(existing) = ranked.iter_mut().find(|(shift, _)| *shift == best_shift) {
                existing.1 = existing.1.min(best_sad);
            } else {
                ranked.push((best_shift, best_sad));
            }
        }
    }
    ranked.sort_by(|left, right| {
        left.1
            .cmp(&right.1)
            .then_with(|| (left.0 - preferred).abs().cmp(&(right.0 - preferred).abs()))
    });
    let (shift, sad) = *ranked.first()?;
    // Two far offsets that match equally are a repeated pattern, not a scroll.
    if ranked
        .iter()
        .any(|(other, other_sad)| (*other - shift).abs() > 8 && *other_sad <= sad)
    {
        return None;
    }
    let content_bottom = height.saturating_sub(footer);
    let mid = header + content_bottom.saturating_sub(header) / 2;
    // A torn grab matches one shift on top and another below. Either half
    // disagreeing is enough to drop the frame; a half with too little overlap
    // simply does not vote.
    if content_sad_band(
        previous,
        next,
        width,
        shift,
        header,
        mid,
        header,
        content_bottom,
    )
    .is_some_and(|sad| sad > ALIGN_LIMIT)
        || content_sad_band(
            previous,
            next,
            width,
            shift,
            mid,
            content_bottom,
            header,
            content_bottom,
        )
        .is_some_and(|sad| sad > ALIGN_LIMIT)
    {
        return None;
    }
    Some(shift)
}

fn content_sad(
    previous: &[u8],
    next: &[u8],
    width: u32,
    height: u32,
    shift: i32,
    header: u32,
    footer: u32,
) -> Option<u32> {
    let bottom = height.saturating_sub(footer);
    content_sad_band(previous, next, width, shift, header, bottom, header, bottom)
}

/// Average per-channel difference for `previous` rows in `band_top..band_bottom`.
/// The row they land on may sit anywhere in the scrolling area. Too few
/// overlapping rows means this band cannot confirm or reject the shift.
fn content_sad_band(
    previous: &[u8],
    next: &[u8],
    width: u32,
    shift: i32,
    band_top: u32,
    band_bottom: u32,
    content_top: u32,
    content_bottom: u32,
) -> Option<u32> {
    if band_bottom <= band_top || width < 8 || previous.len() != next.len() {
        return None;
    }
    let stride = width as usize * 4;
    let mut total = 0u64;
    let mut pixels = 0u64;
    let mut rows = 0u32;
    let mut y = band_top;
    while y < band_bottom {
        // A fixed bar stays put. It is not evidence for the scroll distance.
        if row_unmoved(previous, next, width, y) {
            y += 1;
            continue;
        }
        let other = y as i32 - shift;
        if other >= content_top as i32 && other < content_bottom as i32 {
            let left = y as usize * stride;
            let right = other as usize * stride;
            let mut x = 0usize;
            while x < width as usize {
                let a = left + x * 4;
                let b = right + x * 4;
                total += u64::from(previous[a].abs_diff(next[b]));
                total += u64::from(previous[a + 1].abs_diff(next[b + 1]));
                total += u64::from(previous[a + 2].abs_diff(next[b + 2]));
                pixels += 3;
                x += 1;
            }
            rows += 1;
        }
        y += 1;
    }
    if rows < 6 || pixels == 0 {
        None
    } else {
        // Any real difference has to stay visible. Truncating a fraction to 0
        // made a one-channel miss look like a perfect second alignment.
        Some(((total + pixels - 1) / pixels) as u32)
    }
}

fn row_unmoved(previous: &[u8], next: &[u8], width: u32, y: u32) -> bool {
    let stride = width as usize * 4;
    let start = y as usize * stride;
    if start + stride > previous.len() || start + stride > next.len() {
        return false;
    }
    let mut total = 0u32;
    let mut count = 0u32;
    let mut x = 0usize;
    while x < width as usize {
        let index = start + x * 4;
        total += u32::from(previous[index].abs_diff(next[index]));
        total += u32::from(previous[index + 1].abs_diff(next[index + 1]));
        total += u32::from(previous[index + 2].abs_diff(next[index + 2]));
        count += 3;
        x += 2;
    }
    count > 0 && total / count <= 4
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
    abort_above: u32,
    vote_weight: u64,
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
        abort_above,
        vote_weight,
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

/// Header and footer bands that stayed pixel-aligned between two grabs.
/// A flat page background is not a footer: those rows already appear above it.
fn fixed_edges(previous: &[u8], next: &[u8], width: u32, height: u32) -> (u32, u32) {
    if previous.len() != next.len()
        || width < 8
        || height < 16
        || previous.len() != width as usize * height as usize * 4
    {
        return (0, 0);
    }
    let previous_rows = row_samples(previous, width, height);
    let next_rows = row_samples(next, width, height);
    let sticky: Vec<bool> = (0..height)
        .map(|y| row_static(&previous_rows[y as usize], &next_rows[y as usize]))
        .collect();
    let header = sticky_header(&sticky).max(0) as u32;
    let footer = footer_from_frames(previous, next, &previous_rows, width, height, header);
    (header, footer)
}

/// Bottom band that stays put while the page moves, including the dark margin
/// under a button bar. A one-pixel grab shift still counts.
fn footer_from_frames(
    previous: &[u8],
    next: &[u8],
    rows: &[[u8; ROW_SAMPLES]],
    width: u32,
    height: u32,
    header: u32,
) -> u32 {
    if width < 8 || height <= header.saturating_add(24) {
        return 0;
    }
    let max_footer = (height / 3)
        .clamp(64, 320)
        .min(height.saturating_sub(header + 16));
    let floor = height.saturating_sub(max_footer);
    let mut bar_bottom = None;
    let mut y = height;
    while y > floor {
        y -= 1;
        if row_fixed(previous, next, width, height, y)
            && row_is_bar(previous, rows, width, header, floor, y)
        {
            bar_bottom = Some(y);
            break;
        }
    }
    let Some(bar_bottom) = bar_bottom else {
        return 0;
    };
    let mut footer_top = bar_bottom;
    let mut quiet = 0u32;
    while footer_top > floor {
        let above = footer_top - 1;
        if row_is_bar(previous, rows, width, header, floor, above)
            && row_fixed(previous, next, width, height, above)
        {
            quiet = 0;
            footer_top = above;
            continue;
        }
        if quiet < 12
            && row_pixel_contrast(previous, width, above) < 36
            && row_fixed(previous, next, width, height, above)
        {
            quiet += 1;
            footer_top = above;
            continue;
        }
        break;
    }
    let footer = height.saturating_sub(footer_top);
    if footer < 8 { 0 } else { footer }
}

fn row_fixed(previous: &[u8], next: &[u8], width: u32, height: u32, y: u32) -> bool {
    let mut best_sad = u32::MAX;
    let mut best_changed = u32::MAX;
    for delta in -2i32..=2 {
        let other = y as i32 + delta;
        if other < 0 || other >= height as i32 {
            continue;
        }
        let (sad, changed) = row_channel_sad(previous, next, width, y, other as u32);
        if sad < best_sad {
            best_sad = sad;
            best_changed = changed;
        }
    }
    // A line of text on a dark page barely moves the average, but it still
    // changes a run of pixels. The action bar matches across the whole row.
    // Samples are every other pixel, so the budget is about one short word.
    best_sad <= 14 && best_changed <= (width / 80).max(2)
}

fn row_channel_sad(left: &[u8], right: &[u8], width: u32, y_left: u32, y_right: u32) -> (u32, u32) {
    let stride = width as usize * 4;
    let start_left = y_left as usize * stride;
    let start_right = y_right as usize * stride;
    if start_left + stride > left.len() || start_right + stride > right.len() {
        return (u32::MAX, u32::MAX);
    }
    let mut total = 0u64;
    let mut count = 0u64;
    let mut changed = 0u32;
    let mut x = 0usize;
    while x < width as usize {
        let a = start_left + x * 4;
        let b = start_right + x * 4;
        let dr = u32::from(left[a].abs_diff(right[b]));
        let dg = u32::from(left[a + 1].abs_diff(right[b + 1]));
        let db = u32::from(left[a + 2].abs_diff(right[b + 2]));
        total += u64::from(dr + dg + db);
        if dr > 20 || dg > 20 || db > 20 {
            changed += 1;
        }
        count += 3;
        x += 2;
    }
    if count == 0 {
        (u32::MAX, u32::MAX)
    } else {
        (((total + count - 1) / count) as u32, changed)
    }
}

fn row_pixel_contrast(frame: &[u8], width: u32, y: u32) -> u32 {
    let stride = width as usize * 4;
    let start = y as usize * stride;
    if start + stride > frame.len() {
        return 0;
    }
    let mut min_luma = 255u32;
    let mut max_luma = 0u32;
    let mut x = 0usize;
    while x < width as usize {
        let offset = start + x * 4;
        let luma = (u32::from(frame[offset]) * 3
            + u32::from(frame[offset + 1]) * 6
            + u32::from(frame[offset + 2]))
            / 10;
        min_luma = min_luma.min(luma);
        max_luma = max_luma.max(luma);
        x += 2;
    }
    max_luma.saturating_sub(min_luma)
}

fn row_is_bar(
    frame: &[u8],
    rows: &[[u8; ROW_SAMPLES]],
    width: u32,
    header: u32,
    page_end: u32,
    y: u32,
) -> bool {
    row_pixel_contrast(frame, width, y) >= 36
        || !row_matches_page(rows, header as usize, page_end as usize, y as usize)
}

fn row_matches_page(rows: &[[u8; ROW_SAMPLES]], header: usize, page_end: usize, y: usize) -> bool {
    if page_end <= header || y >= rows.len() {
        return false;
    }
    let mut nearest = u32::MAX;
    let mut other = header;
    while other < page_end {
        nearest = nearest.min(row_sad(&rows[y], &rows[other]) / ROW_SAMPLES as u32);
        if nearest <= 4 {
            return true;
        }
        other += 1;
    }
    false
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
    abort_above: u32,
    vote_weight: u64,
) -> u32 {
    let height = previous.len() as i32;
    let mut total = 0u64;
    let mut weight = 0u64;
    let samples = ROW_SAMPLES as u64;
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
        // Even if every row we have not read matches perfectly, this shift
        // cannot win. Stop instead of walking the rest of the page.
        let denom = vote_weight.saturating_mul(samples);
        if denom > 0 && total / denom > u64::from(abort_above) {
            return u32::MAX;
        }
    }
    if weight == 0 {
        u32::MAX
    } else {
        (total / (weight * samples)) as u32
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
        let placed = placement(&self.previous, frame, self.width, self.view_height);
        let step = match placed.motion {
            Motion::Unchanged | Motion::Unaligned => return Step::Still,
            Motion::Shift(shift) if shift > 0 => self.grow_down(
                frame,
                shift as u32,
                max_height,
                placed.header,
                placed.footer,
            ),
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

    fn grow_down(
        &mut self,
        frame: &[u8],
        shift: u32,
        max_height: u32,
        header: u32,
        footer: u32,
    ) -> Step {
        let new_top = self.view_top.saturating_add(shift);
        let bottom = new_top.saturating_add(self.view_height);
        if bottom <= self.height {
            self.view_top = new_top;
            return Step::Moved { up: false };
        }
        let extra = bottom - self.height;
        let room = max_height.saturating_sub(self.height);
        // The new lines sit just above a fixed action bar. Appending the bottom
        // of the grab would stamp that bar into the strip on every scroll.
        if footer > 0 {
            return self
                .grow_past_footer(frame, extra, room, header, footer, new_top, max_height)
                .unwrap_or(if room == 0 { Step::Full } else { Step::Still });
        }
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

    fn grow_past_footer(
        &mut self,
        frame: &[u8],
        extra: u32,
        room: u32,
        header: u32,
        footer: u32,
        new_top: u32,
        max_height: u32,
    ) -> Option<Step> {
        if footer == 0 || extra == 0 || room == 0 {
            return None;
        }
        let content_bottom = self.view_height.saturating_sub(footer);
        let source = content_bottom
            .saturating_sub(extra)
            .max(header.min(content_bottom));
        let take = extra.min(room).min(content_bottom.saturating_sub(source));
        if take == 0 || !self.replace_tail(frame, source, take, footer) {
            return None;
        }
        if take < extra {
            self.view_top = self.view_top.saturating_add(take);
            return Some(Step::Full);
        }
        self.view_top = new_top;
        Some(if self.height >= max_height {
            Step::Full
        } else {
            Step::Moved { up: false }
        })
    }

    /// Drop the previous footer, append the newly revealed rows, then the footer once.
    fn replace_tail(&mut self, frame: &[u8], source_y: u32, count: u32, footer: u32) -> bool {
        let stride = self.width as usize * 4;
        if stride == 0 || footer == 0 || footer > self.view_height || footer > self.height {
            return false;
        }
        let Some(content_end) = source_y.checked_add(count) else {
            return false;
        };
        if content_end > self.view_height - footer {
            return false;
        }
        let footer_bytes = footer as usize * stride;
        let content_at = source_y as usize * stride;
        let footer_at = (self.view_height - footer) as usize * stride;
        if self.rgba.len() < footer_bytes
            || frame.len() < content_at + count as usize * stride
            || frame.len() < footer_at + footer_bytes
        {
            return false;
        }
        self.rgba.truncate(self.rgba.len() - footer_bytes);
        self.height -= footer;
        if count > 0 {
            self.append(frame, source_y, count);
        }
        self.append(frame, self.view_height - footer, footer);
        true
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

/// Grabs kept ahead of the stitch. The capture thread does not wait to join a
/// frame, so a flick stays a sequence of overlapping grabs.
const FRAME_BATCH: usize = 64;
const FRAME_BYTES: usize = 256 * 1024 * 1024;

struct Composer {
    stitch: Stitch,
    grown: bool,
    synced: bool,
    misses: u32,
}

impl Composer {
    fn new(frame: Vec<u8>, width: u32, height: u32) -> Self {
        Self {
            stitch: Stitch::new(frame, width, height),
            grown: false,
            synced: false,
            misses: 0,
        }
    }

    fn absorb(&mut self, frame: &[u8], max_height: u32) -> Step {
        match self.stitch.push(frame, max_height) {
            Step::Moved { up } => {
                self.grown = true;
                self.misses = 0;
                Step::Moved { up }
            }
            Step::Full => Step::Full,
            Step::Still => {
                let motion = find_shift(
                    self.stitch.view(),
                    frame,
                    self.stitch.width,
                    self.stitch.view_height,
                );
                if matches!(motion, Motion::Unchanged) {
                    return Step::Still;
                }
                // The first grab can still show the capture overlay. Adopt the
                // live page once, then keep every row that has already been
                // stitched instead of replacing the strip with the latest view.
                if !self.grown && !self.synced {
                    self.misses = self.misses.saturating_add(1);
                    if self.misses >= 2 {
                        self.stitch.replace_view(frame);
                        self.synced = true;
                        self.misses = 0;
                    }
                }
                Step::Still
            }
        }
    }

    fn into_strip(self) -> Strip {
        Strip {
            width: self.stitch.width,
            height: self.stitch.height,
            rgba: self.stitch.rgba,
        }
    }
}

/// Join grabs in the order they were taken. A frame that does not line up is
/// left out, and the next grab is still compared with the last one that did.
fn compose_frames(frames: &[Vec<u8>], width: u32, height: u32, max_height: u32) -> Strip {
    let mut frames = frames.iter();
    let Some(first) = frames.next() else {
        return Strip {
            width,
            height: 0,
            rgba: Vec::new(),
        };
    };
    let mut composer = Composer::new(first.clone(), width, height);
    for frame in frames {
        if matches!(composer.absorb(frame, max_height), Step::Full) {
            break;
        }
    }
    composer.into_strip()
}

pub fn capture_strip(
    monitor_index: usize,
    origin_x: u32,
    origin_y: u32,
    width: u32,
    height: u32,
    max_height: u32,
    cursor: Cursor,
    mut progress: impl FnMut(&[u8], u32, u32) + Send + 'static,
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
    progress(&first, width, height);
    let frame_bytes = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4)
        .max(1);
    let slots = (FRAME_BYTES / frame_bytes).clamp(8, FRAME_BATCH);
    let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(slots);
    let hold_until = Arc::new(AtomicU64::new(0));
    let hold_for_join = Arc::clone(&hold_until);
    let clock = Instant::now();
    let join_clock = clock.clone();
    let mut anchor = first.clone();
    let mut newest = anchor.clone();
    let mut holding = false;
    let stitcher = thread::spawn(move || {
        let mut composer = Composer::new(first, width, height);
        let mut preview_at = Instant::now() + Duration::from_millis(200);
        while let Ok(frame) = rx.recv() {
            if frame.len() != composer.stitch.view().len() {
                break;
            }
            let step = composer.absorb(&frame, max_height);
            if matches!(step, Step::Moved { up: true }) {
                let until = join_clock.elapsed().as_millis() as u64 + 800;
                hold_for_join.store(until, Ordering::Release);
            }
            if matches!(step, Step::Full) {
                break;
            }
            if matches!(step, Step::Moved { .. }) && Instant::now() >= preview_at {
                progress(
                    &composer.stitch.rgba,
                    composer.stitch.width,
                    composer.stitch.height,
                );
                preview_at = Instant::now() + Duration::from_millis(200);
            }
        }
        let strip = composer.into_strip();
        progress(&strip.rgba, strip.width, strip.height);
        strip
    });
    let mut failure = None;
    let mut input_ready = false;
    let mut saw_auto = false;
    let deadline = Instant::now() + Duration::from_secs(90);
    while !stopped() && Instant::now() < deadline {
        let enabled = auto_enabled();
        if enabled && !saw_auto {
            input_ready = prepare_input();
        }
        if !enabled {
            input_ready = false;
        }
        saw_auto = enabled;
        let driving = enabled
            && input_ready
            && clock.elapsed().as_millis() as u64 >= hold_until.load(Ordering::Acquire);
        if driving && !scroll_down(&cursor) {
            input_ready = false;
        }
        // The wheel needs a moment to move the page. A hand flick does not:
        // grabbing the next frame does not wait for the previous one to be joined.
        if driving {
            thread::sleep(Duration::from_millis(160));
        }
        if stopped() {
            break;
        }
        let frame = match grab_region(&monitor, origin_x, origin_y, width, height) {
            Ok(frame) => frame,
            Err(error) => {
                failure = Some(error);
                break;
            }
        };
        if frame.len() != anchor.len() {
            break;
        }
        if frame == newest {
            if !driving {
                thread::sleep(Duration::from_millis(4));
            }
            continue;
        }
        // A slow scroll moves a few pixels per grab. Keep the latest frame and
        // join it once the page has traveled far enough to be worth a stitch.
        if shift_is_still_short(&anchor, &frame, width, height) {
            newest = frame;
            holding = true;
            continue;
        }
        newest = frame.clone();
        anchor.clone_from(&newest);
        holding = false;
        if tx.send(frame).is_err() {
            break;
        }
    }
    if holding && newest != anchor {
        let _ = tx.send(newest);
    }
    drop(tx);
    let strip = stitcher.join().map_err(|_| {
        eprintln!("screenshot scroll stitch thread stopped");
        ScreenshotError::new("capture_failed")
    })?;
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(strip)
}

/// True when `next` is only a short, clear step away from the last joined frame.
/// A fast flick falls outside this window and is recorded immediately.
fn shift_is_still_short(previous: &[u8], next: &[u8], width: u32, height: u32) -> bool {
    if previous.len() != next.len() || width < 8 || height < 32 {
        return false;
    }
    let previous_rows = row_samples(previous, width, height);
    let next_rows = row_samples(next, width, height);
    // Keep this under one form row. Holding a whole checkbox row made the next
    // join line up with the following similar row and overlap the section.
    let limit = 8i32;
    let mut best_shift = 0i32;
    let mut best_score = u32::MAX;
    let mut still = u32::MAX;
    let bottom = height.saturating_sub(4);
    for shift in -limit..=limit {
        let mut total = 0u64;
        let mut count = 0u64;
        let mut y = 4u32;
        while y < bottom {
            let other = y as i32 - shift;
            if (4..bottom as i32).contains(&other) {
                total += u64::from(row_sad(
                    &previous_rows[y as usize],
                    &next_rows[other as usize],
                ));
                count += 1;
            }
            y += 4;
        }
        if count == 0 {
            continue;
        }
        let score = (total / (count * ROW_SAMPLES as u64)) as u32;
        if shift == 0 {
            still = score;
        }
        if score < best_score || (score == best_score && shift.abs() < best_shift.abs()) {
            best_score = score;
            best_shift = shift;
        }
    }
    best_score <= 8 && best_shift.abs() < limit && (best_shift == 0 || best_score + 3 < still)
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
    use super::{Cursor, Motion, Stitch, compose_frames, find_shift, fixed_edges, point_inside};

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
    fn stitches_similar_section_bands_without_overlapping_them() {
        fn bands(width: u32, height: u32, origin: u32) -> Vec<u8> {
            let mut rgba = vec![22u8; width as usize * height as usize * 4];
            for y in 0..height {
                let absolute = origin + y;
                let into = absolute % 28;
                for x in 0..width {
                    let offset = (y * width + x) as usize * 4;
                    let (red, green, blue) = if into < 8 {
                        (16, 16, 18)
                    } else if x < 18 {
                        let ink = absolute.wrapping_mul(13) as u8;
                        (ink, ink.wrapping_add(20), 90)
                    } else {
                        (32, 34, 40)
                    };
                    rgba[offset] = red;
                    rgba[offset + 1] = green;
                    rgba[offset + 2] = blue;
                    rgba[offset + 3] = 255;
                }
            }
            rgba
        }
        let width = 96u32;
        let height = 220u32;
        let first = bands(width, height, 0);
        let next = bands(width, height, 12);
        assert!(
            matches!(find_shift(&first, &next, width, height), Motion::Shift(12)),
            "{:?}",
            find_shift(&first, &next, width, height)
        );
        let mut stitch = Stitch::new(first, width, height);
        assert!(matches!(
            stitch.push(&next, 4_000),
            super::Step::Moved { up: false }
        ));
        let expected = bands(width, stitch.height, 0);
        assert_eq!(stitch.rgba, expected);
    }

    #[test]
    fn finds_a_two_pixel_scroll_on_a_tall_page() {
        let previous = document_rows(80, 360, 0);
        let next = document_rows(80, 360, 2);
        let motion = find_shift(&previous, &next, 80, 360);
        assert!(matches!(motion, Motion::Shift(2)), "{motion:?}");
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

    fn wizard_page(width: u32, height: u32, origin: u32, header: u32, footer: u32) -> Vec<u8> {
        let mut rgba = vec![18u8; width as usize * height as usize * 4];
        for y in 0..height {
            for x in 0..width {
                let offset = (y * width + x) as usize * 4;
                let (red, green, blue) = if y < header {
                    (255, 0, 128)
                } else if y + footer >= height {
                    let band = y - (height - footer);
                    (20, 40u8.wrapping_add((band.wrapping_mul(17)) as u8), 220)
                } else {
                    let absolute = origin + (y - header);
                    let ink = absolute.wrapping_mul(13).wrapping_add(x.wrapping_mul(3)) as u8;
                    (ink, ink.wrapping_add(20), 48)
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
    fn stitches_a_wizard_without_repeating_its_footer() {
        let header = 28u32;
        let footer = 36u32;
        let width = 64u32;
        let height = 180u32;
        let first = wizard_page(width, height, 0, header, footer);
        let stepped = wizard_page(width, height, 10, header, footer);
        let jumped = wizard_page(width, height, 50, header, footer);
        assert_eq!(
            fixed_edges(&first, &stepped, width, height),
            (header, footer)
        );
        assert!(matches!(
            find_shift(&first, &stepped, width, height),
            Motion::Shift(10)
        ));
        let mut stitch = Stitch::new(first, width, height);
        assert!(matches!(
            stitch.push(&stepped, 4_000),
            super::Step::Moved { up: false }
        ));
        assert_eq!(stitch.height, height + 10);
        assert!(matches!(
            find_shift(stitch.view(), &jumped, width, height),
            Motion::Shift(40)
        ));
        assert!(matches!(
            stitch.push(&jumped, 4_000),
            super::Step::Moved { up: false }
        ));
        assert_eq!(stitch.height, height + 50);
        let stride = width as usize * 4;
        let content_end = stitch.height - footer;
        for y in 0..stitch.height {
            let offset = y as usize * stride;
            let pixel = &stitch.rgba[offset..offset + 4];
            let is_header = pixel == [255, 0, 128, 255];
            let is_footer = pixel[0] == 20 && pixel[2] == 220;
            assert_eq!(is_header, y < header, "header row {y}");
            assert_eq!(is_footer, y >= content_end, "footer row {y}");
            if (header..content_end).contains(&y) {
                let absolute = y - header;
                let ink = absolute.wrapping_mul(13) as u8;
                assert_eq!(
                    pixel,
                    &[ink, ink.wrapping_add(20), 48, 255],
                    "content row {y}"
                );
            }
        }
        let footer_top = content_end as usize * stride;
        assert_eq!(stitch.rgba[footer_top + 1], 40);
    }

    fn form_page(width: u32, height: u32, origin: u32) -> Vec<u8> {
        let mut rgba = vec![22u8; width as usize * height as usize * 4];
        for y in 0..height {
            let absolute = origin + y;
            for x in 0..width {
                let offset = (y * width + x) as usize * 4;
                let (red, green, blue) = if x < 8 || x + 8 >= width {
                    (48, 48, 52)
                } else if absolute % 28 < 18 {
                    let ink = absolute.wrapping_mul(19).wrapping_add(x.wrapping_mul(5)) as u8;
                    (ink, ink.wrapping_add(30), 70)
                } else {
                    (30, 32, 36)
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
    fn stitches_similar_form_rows_without_stacking_them() {
        let first = form_page(96, 180, 0);
        let next = form_page(96, 180, 28);
        assert!(
            matches!(find_shift(&first, &next, 96, 180), Motion::Shift(28)),
            "{:?}",
            find_shift(&first, &next, 96, 180)
        );
        let mut stitch = Stitch::new(first, 96, 180);
        assert!(matches!(
            stitch.push(&next, 4_000),
            super::Step::Moved { up: false }
        ));
        assert_eq!(stitch.height, 208);
        let stride = 96 * 4;
        for y in 0..stitch.height {
            let absolute = y;
            let offset = y as usize * stride + 20 * 4;
            let ink = absolute
                .wrapping_mul(19)
                .wrapping_add(20u32.wrapping_mul(5)) as u8;
            if absolute % 28 < 18 {
                assert_eq!(
                    &stitch.rgba[offset..offset + 3],
                    &[ink, ink.wrapping_add(30), 70],
                    "row {y}"
                );
            }
        }
    }

    #[test]
    fn rejects_a_torn_grab() {
        let previous = rows(48, 120, 0);
        let mut next = rows(48, 120, 20);
        let torn = rows(48, 120, 70);
        let stride = 48 * 4;
        next[60 * stride..].copy_from_slice(&torn[60 * stride..]);
        assert!(
            matches!(find_shift(&previous, &next, 48, 120), Motion::Unaligned),
            "{:?}",
            find_shift(&previous, &next, 48, 120)
        );
        assert!(matches!(
            find_shift(&rows(48, 120, 0), &rows(48, 120, 20), 48, 120),
            Motion::Shift(20)
        ));
    }

    #[test]
    fn footer_is_the_action_bar_not_the_dark_page() {
        let bar = 32u32;
        let mut previous = vec![24u8; 80 * 200 * 4];
        let mut next = previous.clone();
        for (origin, frame) in [(0u32, &mut previous), (36u32, &mut next)] {
            for y in 0..200 {
                for x in 0..80 {
                    let offset = (y * 80 + x) as usize * 4;
                    if y + bar >= 200 {
                        let band = y - (200 - bar);
                        frame[offset] = 12;
                        frame[offset + 1] = 30u8.wrapping_add((band * 13) as u8);
                        frame[offset + 2] = 210;
                    } else if (origin + y) % 26 < 8 {
                        let ink = (origin + y).wrapping_mul(11).wrapping_add(x) as u8;
                        frame[offset] = ink;
                        frame[offset + 1] = ink.wrapping_add(40);
                        frame[offset + 2] = 90;
                    }
                    frame[offset + 3] = 255;
                }
            }
        }
        let (header, footer) = fixed_edges(&previous, &next, 80, 200);
        assert_eq!(header, 0);
        assert!(
            (bar..bar + 13).contains(&footer),
            "footer {footer} swallowed the dark page"
        );
        let mut stitch = Stitch::new(previous, 80, 200);
        assert!(matches!(
            stitch.push(&next, 4_000),
            super::Step::Moved { up: false }
        ));
        let stride = 80 * 4;
        let mut bars = 0u32;
        for y in 0..stitch.height {
            let offset = y as usize * stride;
            if stitch.rgba[offset] == 12 && stitch.rgba[offset + 2] == 210 {
                bars += 1;
            }
        }
        assert_eq!(bars, bar);
    }

    fn dialog_with_padding(width: u32, height: u32, origin: u32, bar: u32, pad: u32) -> Vec<u8> {
        let mut rgba = vec![18u8; width as usize * height as usize * 4];
        let bar_top = height - pad - bar;
        for y in 0..height {
            for x in 0..width {
                let offset = (y * width + x) as usize * 4;
                let (red, green, blue) = if y >= bar_top && y + pad < height {
                    if x % 8 < 4 {
                        (240, 240, 245)
                    } else {
                        (20, 90, 230)
                    }
                } else if (origin + y) % 24 < 10 && y < bar_top {
                    let ink = (origin + y)
                        .wrapping_mul(13)
                        .wrapping_add(x.wrapping_mul(3)) as u8;
                    (ink, ink.wrapping_add(20), 70)
                } else {
                    (18, 18, 22)
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
    fn keeps_one_action_bar_when_dark_padding_sits_under_it() {
        let width = 80u32;
        let height = 220u32;
        let bar = 28u32;
        let pad = 40u32;
        let first = dialog_with_padding(width, height, 0, bar, pad);
        let next = dialog_with_padding(width, height, 80, bar, pad);
        let footer = fixed_edges(&first, &next, width, height).1;
        assert!(
            (bar + pad..bar + pad + 16).contains(&footer),
            "footer {footer} missed the bar above the dark padding"
        );
        let mut stitch = Stitch::new(first, width, height);
        assert!(matches!(
            stitch.push(&next, 4_000),
            super::Step::Moved { up: false }
        ));
        assert_eq!(stitch.height, height + 80);
        let stride = width as usize * 4;
        let mut bars = 0u32;
        for y in 0..stitch.height {
            let offset = y as usize * stride;
            if stitch.rgba[offset] == 240 && stitch.rgba[offset + 2] == 245 {
                bars += 1;
            }
        }
        assert_eq!(bars, bar, "the action bar was stamped again");
        let tail = (stitch.height - pad) as usize * stride;
        assert_eq!(&stitch.rgba[tail..tail + 4], &[18, 18, 22, 255]);
        let bar_row = (stitch.height - pad - bar) as usize * stride;
        assert_eq!(&stitch.rgba[bar_row..bar_row + 4], &[240, 240, 245, 255]);
    }

    #[test]
    fn keeps_an_action_bar_shifted_by_one_pixel() {
        let width = 80u32;
        let height = 200u32;
        let bar = 28u32;
        let pad = 24u32;
        let first = dialog_with_padding(width, height, 0, bar, pad);
        let next = dialog_with_padding(width, height, 50, bar, pad);
        let stride = width as usize * 4;
        let footer_rows = (bar + pad) as usize;
        let mut shifted = next.clone();
        for y in (1..footer_rows).rev() {
            let dest = (height as usize - footer_rows + y) * stride;
            let source = (height as usize - footer_rows + y - 1) * stride;
            shifted[dest..dest + stride].copy_from_slice(&next[source..source + stride]);
        }
        let bar_at = (height as usize - footer_rows) * stride;
        for pixel in shifted[bar_at..bar_at + bar as usize * stride].chunks_mut(4) {
            pixel[0] = pixel[0].saturating_add(2);
        }
        let footer = fixed_edges(&first, &shifted, width, height).1;
        assert!(
            footer + 2 >= bar + pad,
            "footer {footer} dropped the shifted bar"
        );
        let mut stitch = Stitch::new(first, width, height);
        assert!(matches!(
            stitch.push(&shifted, 4_000),
            super::Step::Moved { up: false }
        ));
        let mut bars = 0u32;
        for y in 0..stitch.height {
            let offset = y as usize * stride;
            if stitch.rgba[offset] >= 230 && stitch.rgba[offset + 2] >= 230 {
                bars += 1;
            }
        }
        assert!(bars <= bar + 2, "bars {bars}");
    }

    #[test]
    fn a_flat_page_bottom_is_appended_with_the_new_rows() {
        let mut first = plain_rows(32, 120, 0);
        let mut next = plain_rows(32, 120, 30);
        for frame in [&mut first, &mut next] {
            for y in 96..120 {
                for x in 0..32 {
                    let offset = (y * 32 + x) as usize * 4;
                    frame[offset] = 16;
                    frame[offset + 1] = 16;
                    frame[offset + 2] = 16;
                }
            }
        }
        assert_eq!(fixed_edges(&first, &next, 32, 120).1, 0);
        let mut stitch = Stitch::new(first.clone(), 32, 120);
        assert!(matches!(
            stitch.push(&next, 2_000),
            super::Step::Moved { up: false }
        ));
        assert_eq!(stitch.height, 150);
        let mut expected = first;
        let stride = 32 * 4;
        expected.extend_from_slice(&next[(120 - 30) * stride..]);
        assert_eq!(stitch.rgba, expected);
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

    #[test]
    fn composes_every_frame_of_a_flick() {
        let frames: Vec<_> = (0..6).map(|step| rows(32, 96, step * 20)).collect();
        let strip = compose_frames(&frames, 32, 96, 4_000);
        assert_eq!(strip.height, 96 + 100);
        let stride = 32 * 4;
        for y in 0..strip.height {
            assert_eq!(strip.rgba[y as usize * stride], y as u8, "row {y}");
        }
    }

    #[test]
    fn composes_a_fast_flick_that_one_pair_cannot_span() {
        let height = 140u32;
        let step = 80u32;
        let frames: Vec<_> = (0..4)
            .map(|index| document_rows(48, height, index * step))
            .collect();
        let strip = compose_frames(&frames, 48, height, 4_000);
        let expected = document_rows(48, height + step * 3, 0);
        assert_eq!(strip.height, expected.len() as u32 / (48 * 4));
        assert_eq!(strip.rgba, expected);
    }

    #[test]
    fn composes_the_later_frames_when_one_grab_is_torn() {
        let mut torn = rows(48, 120, 30);
        let decoy = rows(48, 120, 90);
        let stride = 48 * 4;
        torn[70 * stride..].copy_from_slice(&decoy[70 * stride..]);
        let frames = vec![rows(48, 120, 0), rows(48, 120, 18), torn, rows(48, 120, 40)];
        let strip = compose_frames(&frames, 48, 120, 4_000);
        assert_eq!(strip.height, 120 + 40);
        for y in 0..strip.height {
            assert_eq!(strip.rgba[y as usize * stride], y as u8, "row {y}");
        }
    }
}
