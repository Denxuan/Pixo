use std::fs;
use std::path::Path;
use std::time::Duration;

use image::RgbaImage;
use openh264::OpenH264API;
use openh264::encoder::{
    BitRate, Encoder, EncoderConfig, FrameRate, FrameType, RateControlMode, UsageType, VuiConfig,
};
use openh264::formats::{RgbaSliceU8, YUVBuffer};

use super::clock::unique_file;

const TIMESCALE: u32 = 90_000;

struct Sample {
    data: Vec<u8>,
    duration: u32,
    sync: bool,
}

pub struct Mp4Stream {
    encoder: Encoder,
    width: u32,
    height: u32,
    pending: Vec<u8>,
    pending_shown: Duration,
    carried: Duration,
    samples: Vec<Sample>,
    sps: Vec<u8>,
    pps: Vec<u8>,
}

impl Mp4Stream {
    pub fn create(image: &RgbaImage) -> Result<Self, String> {
        let (width, height) = even_size(image.width(), image.height());
        if width < 2 || height < 2 || width > 3840 || height > 2160 {
            return Err("size".to_string());
        }
        let encoder = open_encoder(width, height)?;
        Ok(Self {
            encoder,
            width,
            height,
            pending: frame_pixels(image, width, height),
            pending_shown: Duration::ZERO,
            carried: Duration::ZERO,
            samples: Vec::new(),
            sps: Vec::new(),
            pps: Vec::new(),
        })
    }

    pub fn push(&mut self, image: &RgbaImage, elapsed: Duration) -> Result<(), String> {
        self.pending_shown += elapsed;
        let next = frame_pixels(image, self.width, self.height);
        if next == self.pending {
            return Ok(());
        }
        self.emit()?;
        self.pending = next;
        self.pending_shown = Duration::ZERO;
        Ok(())
    }

    pub fn finish_file(
        mut self,
        directory: &Path,
        tail: Duration,
        audio: Option<Vec<i16>>,
    ) -> Result<String, String> {
        self.pending_shown += tail;
        self.emit()?;
        let bytes = self.finish_bytes(audio.as_deref())?;
        fs::create_dir_all(directory).map_err(|error| error.to_string())?;
        let path = unique_file(directory, "mp4");
        if let Err(error) = fs::write(&path, bytes) {
            let _ = fs::remove_file(&path);
            eprintln!("screenshot mp4: {error}");
            return Err("storage".to_string());
        }
        Ok(path.to_string_lossy().into_owned())
    }

    fn finish_bytes(&self, audio: Option<&[i16]>) -> Result<Vec<u8>, String> {
        if self.samples.is_empty() || self.sps.is_empty() || self.pps.is_empty() {
            return Err("empty".to_string());
        }
        mux_avc(
            self.width,
            self.height,
            TIMESCALE,
            &self.sps,
            &self.pps,
            &self.samples,
            audio,
        )
    }

    fn emit(&mut self) -> Result<(), String> {
        let rgba = std::mem::take(&mut self.pending);
        if rgba.is_empty() {
            return Ok(());
        }
        let shown = self.pending_shown + self.carried;
        self.pending_shown = Duration::ZERO;
        self.carried = Duration::ZERO;
        if self.samples.is_empty() {
            self.encoder.force_intra_frame();
        }
        let source = RgbaSliceU8::new(&rgba, (self.width as usize, self.height as usize));
        let yuv = YUVBuffer::from_rgb_source(source);
        let encoded = self
            .encoder
            .encode(&yuv)
            .map_err(|error| error.to_string())?;
        let mut data = Vec::new();
        let mut sps = None;
        let mut pps = None;
        let mut sync = matches!(encoded.frame_type(), FrameType::IDR | FrameType::I);
        for layer_index in 0..encoded.num_layers() {
            let Some(layer) = encoded.layer(layer_index) else {
                continue;
            };
            for nal_index in 0..layer.nal_count() {
                let Some(nal) = layer.nal_unit(nal_index) else {
                    continue;
                };
                let payload = strip_start_code(nal);
                if payload.is_empty() {
                    continue;
                }
                match payload[0] & 0x1f {
                    7 => sps = Some(payload.to_vec()),
                    8 => pps = Some(payload.to_vec()),
                    5 => {
                        sync = true;
                        push_nal(&mut data, payload);
                    }
                    1 | 6 => push_nal(&mut data, payload),
                    _ => {}
                }
            }
        }
        drop(encoded);
        if let Some(sps) = sps {
            self.sps = sps;
        }
        if let Some(pps) = pps {
            self.pps = pps;
        }
        if data.is_empty() {
            if let Some(last) = self.samples.last_mut() {
                last.duration = last.duration.saturating_add(duration_ticks(shown));
            } else {
                self.carried = shown;
            }
            return Ok(());
        }
        self.samples.push(Sample {
            data,
            duration: duration_ticks(shown),
            sync,
        });
        Ok(())
    }
}

fn open_encoder(width: u32, height: u32) -> Result<Encoder, String> {
    let config = EncoderConfig::new()
        .bitrate(BitRate::from_bps(bitrate_for(width, height)))
        .max_frame_rate(FrameRate::from_hz(30.0))
        .usage_type(UsageType::ScreenContentRealTime)
        .rate_control_mode(RateControlMode::Bitrate)
        .skip_frames(false)
        .adaptive_quantization(false)
        .background_detection(false)
        .num_threads(0)
        .vui(VuiConfig::bt709());
    Encoder::with_api_config(OpenH264API::from_source(), config).map_err(|error| error.to_string())
}

fn bitrate_for(width: u32, height: u32) -> u32 {
    let pixels = u64::from(width.max(1)) * u64::from(height.max(1));
    u32::try_from(pixels.saturating_mul(3).clamp(500_000, 8_000_000)).unwrap_or(8_000_000)
}

fn even_size(width: u32, height: u32) -> (u32, u32) {
    (width & !1, height & !1)
}

fn frame_pixels(image: &RgbaImage, width: u32, height: u32) -> Vec<u8> {
    if image.width() == width && image.height() == height {
        return image.as_raw().to_vec();
    }
    if image.width() >= width && image.height() >= height {
        return image::imageops::crop_imm(image, 0, 0, width, height)
            .to_image()
            .into_raw();
    }
    image::imageops::resize(image, width, height, image::imageops::FilterType::Triangle).into_raw()
}

fn strip_start_code(nal: &[u8]) -> &[u8] {
    if nal.starts_with(&[0, 0, 0, 1]) {
        &nal[4..]
    } else if nal.starts_with(&[0, 0, 1]) {
        &nal[3..]
    } else {
        nal
    }
}

fn push_nal(sample: &mut Vec<u8>, payload: &[u8]) {
    let Ok(size) = u32::try_from(payload.len()) else {
        return;
    };
    sample.extend_from_slice(&size.to_be_bytes());
    sample.extend_from_slice(payload);
}

fn duration_ticks(elapsed: Duration) -> u32 {
    let ticks = elapsed.as_nanos().saturating_mul(u128::from(TIMESCALE)) / 1_000_000_000;
    u32::try_from(ticks.max(1).min(u128::from(TIMESCALE) * 30)).unwrap_or(u32::MAX)
}

fn mux_avc(
    width: u32,
    height: u32,
    timescale: u32,
    sps: &[u8],
    pps: &[u8],
    samples: &[Sample],
    audio: Option<&[i16]>,
) -> Result<Vec<u8>, String> {
    let width = u16::try_from(width).map_err(|_| "size".to_string())?;
    let height = u16::try_from(height).map_err(|_| "size".to_string())?;
    if width < 2 || height < 2 || sps.len() < 4 || pps.is_empty() || samples.is_empty() {
        return Err("empty".to_string());
    }
    if sps.len() > usize::from(u16::MAX) || pps.len() > usize::from(u16::MAX) {
        return Err("size".to_string());
    }
    let mut duration = 0u32;
    for sample in samples {
        if sample.data.is_empty() || sample.duration == 0 {
            return Err("empty".to_string());
        }
        duration = duration
            .checked_add(sample.duration)
            .ok_or_else(|| "duration".to_string())?;
    }
    let audio_samples = match audio {
        Some(pcm) => Some(audio_track_samples(pcm, duration)?),
        None => None,
    };
    let mut mdat_payload = Vec::new();
    let mut sizes = Vec::with_capacity(samples.len());
    for sample in samples {
        let size = u32::try_from(sample.data.len()).map_err(|_| "size".to_string())?;
        sizes.push(size);
        mdat_payload.extend_from_slice(&sample.data);
    }
    let video_bytes = u32::try_from(mdat_payload.len()).map_err(|_| "size".to_string())?;
    if let Some(audio_samples) = &audio_samples {
        for sample in audio_samples {
            mdat_payload.extend_from_slice(&sample.data);
        }
    }

    let mut buf = Buf::default();
    write_ftyp(&mut buf)?;
    let mdat_at = buf.begin(b"mdat");
    let chunk_offset = u32::try_from(buf.data.len()).map_err(|_| "size".to_string())?;
    let audio_offset = chunk_offset
        .checked_add(video_bytes)
        .ok_or_else(|| "size".to_string())?;
    buf.bytes(&mdat_payload);
    buf.end(mdat_at)?;
    write_moov(
        &mut buf,
        width,
        height,
        timescale,
        duration,
        sps,
        pps,
        samples,
        &sizes,
        chunk_offset,
        audio_samples.as_deref(),
        audio_offset,
    )?;
    Ok(buf.data)
}

const AUDIO_RATE: u32 = 48_000;
const AUDIO_CHUNK_FRAMES: usize = 1024;

fn audio_track_samples(pcm: &[i16], video_ticks: u32) -> Result<Vec<Sample>, String> {
    let frames = audio_frame_count(video_ticks);
    let mut samples = Vec::new();
    let mut frame = 0usize;
    while frame < frames {
        let count = (frames - frame).min(AUDIO_CHUNK_FRAMES);
        let mut data = vec![0u8; count * 4];
        for index in 0..count {
            let src = (frame + index) * 2;
            let left = pcm.get(src).copied().unwrap_or(0);
            let right = pcm.get(src + 1).copied().unwrap_or(0);
            data[index * 4..index * 4 + 2].copy_from_slice(&left.to_le_bytes());
            data[index * 4 + 2..index * 4 + 4].copy_from_slice(&right.to_le_bytes());
        }
        samples.push(Sample {
            data,
            duration: u32::try_from(count).map_err(|_| "size".to_string())?,
            sync: false,
        });
        frame += count;
    }
    if samples.is_empty() {
        return Err("empty".to_string());
    }
    Ok(samples)
}

fn audio_frame_count(video_ticks: u32) -> usize {
    let ticks = u64::from(video_ticks);
    let scale = u64::from(TIMESCALE);
    let frames = (ticks * u64::from(AUDIO_RATE) + scale / 2) / scale;
    usize::try_from(frames.max(1)).unwrap_or(usize::MAX)
}

fn write_ftyp(buf: &mut Buf) -> Result<(), String> {
    let at = buf.begin(b"ftyp");
    buf.fourcc(b"isom");
    buf.u32(0x200);
    buf.fourcc(b"isom");
    buf.fourcc(b"iso2");
    buf.fourcc(b"avc1");
    buf.fourcc(b"mp41");
    buf.fourcc(b"qt  ");
    buf.end(at)
}

fn write_moov(
    buf: &mut Buf,
    width: u16,
    height: u16,
    timescale: u32,
    duration: u32,
    sps: &[u8],
    pps: &[u8],
    samples: &[Sample],
    sizes: &[u32],
    chunk_offset: u32,
    audio: Option<&[Sample]>,
    audio_offset: u32,
) -> Result<(), String> {
    let at = buf.begin(b"moov");
    write_mvhd(
        buf,
        timescale,
        duration,
        if audio.is_some() { 3 } else { 2 },
    )?;
    write_trak(
        buf,
        width,
        height,
        timescale,
        duration,
        sps,
        pps,
        samples,
        sizes,
        chunk_offset,
    )?;
    if let Some(audio) = audio {
        write_audio_trak(buf, duration, audio, audio_offset)?;
    }
    buf.end(at)
}

fn write_mvhd(buf: &mut Buf, timescale: u32, duration: u32, next_track: u32) -> Result<(), String> {
    let at = buf.begin(b"mvhd");
    buf.full(0, 0);
    buf.u32(0);
    buf.u32(0);
    buf.u32(timescale);
    buf.u32(duration);
    buf.u32(0x0001_0000);
    buf.u16(0x0100);
    buf.u16(0);
    buf.zeroes(8);
    write_matrix(buf);
    buf.zeroes(24);
    buf.u32(next_track);
    buf.end(at)
}

fn write_trak(
    buf: &mut Buf,
    width: u16,
    height: u16,
    timescale: u32,
    duration: u32,
    sps: &[u8],
    pps: &[u8],
    samples: &[Sample],
    sizes: &[u32],
    chunk_offset: u32,
) -> Result<(), String> {
    let at = buf.begin(b"trak");
    write_tkhd(buf, 1, duration, width, height, 0)?;
    write_mdia(
        buf,
        width,
        height,
        timescale,
        duration,
        sps,
        pps,
        samples,
        sizes,
        chunk_offset,
    )?;
    buf.end(at)
}

fn write_tkhd(
    buf: &mut Buf,
    track_id: u32,
    duration: u32,
    width: u16,
    height: u16,
    volume: u16,
) -> Result<(), String> {
    let at = buf.begin(b"tkhd");
    buf.full(0, 0x7);
    buf.u32(0);
    buf.u32(0);
    buf.u32(track_id);
    buf.u32(0);
    buf.u32(duration);
    buf.zeroes(8);
    buf.u16(0);
    buf.u16(0);
    buf.u16(volume);
    buf.u16(0);
    write_matrix(buf);
    buf.u32(u32::from(width) << 16);
    buf.u32(u32::from(height) << 16);
    buf.end(at)
}

fn write_mdia(
    buf: &mut Buf,
    width: u16,
    height: u16,
    timescale: u32,
    duration: u32,
    sps: &[u8],
    pps: &[u8],
    samples: &[Sample],
    sizes: &[u32],
    chunk_offset: u32,
) -> Result<(), String> {
    let at = buf.begin(b"mdia");
    write_mdhd(buf, timescale, duration)?;
    write_hdlr(buf, b"vide", "VideoHandler")?;
    write_minf(buf, width, height, sps, pps, samples, sizes, chunk_offset)?;
    buf.end(at)
}

fn write_mdhd(buf: &mut Buf, timescale: u32, duration: u32) -> Result<(), String> {
    let at = buf.begin(b"mdhd");
    buf.full(0, 0);
    buf.u32(0);
    buf.u32(0);
    buf.u32(timescale);
    buf.u32(duration);
    buf.u16(0x55C4);
    buf.u16(0);
    buf.end(at)
}

fn write_minf(
    buf: &mut Buf,
    width: u16,
    height: u16,
    sps: &[u8],
    pps: &[u8],
    samples: &[Sample],
    sizes: &[u32],
    chunk_offset: u32,
) -> Result<(), String> {
    let at = buf.begin(b"minf");
    write_vmhd(buf)?;
    write_hdlr(buf, b"url ", "DataHandler")?;
    write_dinf(buf)?;
    write_stbl(buf, width, height, sps, pps, samples, sizes, chunk_offset)?;
    buf.end(at)
}

fn write_vmhd(buf: &mut Buf) -> Result<(), String> {
    let at = buf.begin(b"vmhd");
    buf.full(0, 1);
    buf.u16(0);
    buf.u16(0);
    buf.u16(0);
    buf.u16(0);
    buf.end(at)
}

fn write_dinf(buf: &mut Buf) -> Result<(), String> {
    let dinf = buf.begin(b"dinf");
    let dref = buf.begin(b"dref");
    buf.full(0, 0);
    buf.u32(1);
    let url = buf.begin(b"url ");
    buf.full(0, 1);
    buf.end(url)?;
    buf.end(dref)?;
    buf.end(dinf)
}

fn write_stbl(
    buf: &mut Buf,
    width: u16,
    height: u16,
    sps: &[u8],
    pps: &[u8],
    samples: &[Sample],
    sizes: &[u32],
    chunk_offset: u32,
) -> Result<(), String> {
    let at = buf.begin(b"stbl");
    write_stsd(buf, width, height, sps, pps)?;
    write_stts(buf, samples)?;
    write_stss(buf, samples)?;
    write_stsc(buf, samples.len())?;
    write_stsz(buf, sizes)?;
    write_stco(buf, chunk_offset)?;
    buf.end(at)
}

fn write_stsd(
    buf: &mut Buf,
    width: u16,
    height: u16,
    sps: &[u8],
    pps: &[u8],
) -> Result<(), String> {
    let at = buf.begin(b"stsd");
    buf.full(0, 0);
    buf.u32(1);
    write_avc1(buf, width, height, sps, pps)?;
    buf.end(at)
}

fn write_avc1(
    buf: &mut Buf,
    width: u16,
    height: u16,
    sps: &[u8],
    pps: &[u8],
) -> Result<(), String> {
    let at = buf.begin(b"avc1");
    buf.zeroes(6);
    buf.u16(1);
    buf.u16(0);
    buf.u16(0);
    buf.zeroes(12);
    buf.u16(width);
    buf.u16(height);
    buf.u32(0x0048_0000);
    buf.u32(0x0048_0000);
    buf.u32(0);
    buf.u16(1);
    buf.zeroes(32);
    buf.u16(0x0018);
    buf.u16(0xFFFF);
    write_avcc(buf, sps, pps)?;
    buf.end(at)
}

fn write_avcc(buf: &mut Buf, sps: &[u8], pps: &[u8]) -> Result<(), String> {
    let at = buf.begin(b"avcC");
    buf.u8(1);
    buf.u8(sps[1]);
    buf.u8(sps[2]);
    buf.u8(sps[3]);
    buf.u8(0xFF);
    buf.u8(0xE1);
    buf.u16(u16::try_from(sps.len()).map_err(|_| "size".to_string())?);
    buf.bytes(sps);
    buf.u8(1);
    buf.u16(u16::try_from(pps.len()).map_err(|_| "size".to_string())?);
    buf.bytes(pps);
    buf.end(at)
}

fn write_stts(buf: &mut Buf, samples: &[Sample]) -> Result<(), String> {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for sample in samples {
        if let Some(last) = runs.last_mut() {
            if last.1 == sample.duration {
                last.0 = last.0.saturating_add(1);
                continue;
            }
        }
        runs.push((1, sample.duration));
    }
    let at = buf.begin(b"stts");
    buf.full(0, 0);
    buf.u32(u32::try_from(runs.len()).map_err(|_| "size".to_string())?);
    for (count, duration) in runs {
        buf.u32(count);
        buf.u32(duration);
    }
    buf.end(at)
}

fn write_stss(buf: &mut Buf, samples: &[Sample]) -> Result<(), String> {
    let keys: Vec<u32> = samples
        .iter()
        .enumerate()
        .filter(|(_, sample)| sample.sync)
        .map(|(index, _)| u32::try_from(index + 1).unwrap_or(u32::MAX))
        .collect();
    let keys = if keys.is_empty() { vec![1] } else { keys };
    let at = buf.begin(b"stss");
    buf.full(0, 0);
    buf.u32(u32::try_from(keys.len()).map_err(|_| "size".to_string())?);
    for key in keys {
        buf.u32(key);
    }
    buf.end(at)
}

fn write_stsc(buf: &mut Buf, sample_count: usize) -> Result<(), String> {
    let at = buf.begin(b"stsc");
    buf.full(0, 0);
    buf.u32(1);
    buf.u32(1);
    buf.u32(u32::try_from(sample_count).map_err(|_| "size".to_string())?);
    buf.u32(1);
    buf.end(at)
}

fn write_stsz(buf: &mut Buf, sizes: &[u32]) -> Result<(), String> {
    let at = buf.begin(b"stsz");
    buf.full(0, 0);
    buf.u32(0);
    buf.u32(u32::try_from(sizes.len()).map_err(|_| "size".to_string())?);
    for size in sizes {
        buf.u32(*size);
    }
    buf.end(at)
}

fn write_stco(buf: &mut Buf, chunk_offset: u32) -> Result<(), String> {
    let at = buf.begin(b"stco");
    buf.full(0, 0);
    buf.u32(1);
    buf.u32(chunk_offset);
    buf.end(at)
}

fn write_hdlr(buf: &mut Buf, kind: &[u8; 4], name: &str) -> Result<(), String> {
    let at = buf.begin(b"hdlr");
    buf.full(0, 0);
    buf.u32(0);
    buf.fourcc(kind);
    buf.zeroes(12);
    buf.bytes(name.as_bytes());
    buf.u8(0);
    while (buf.data.len() - at) % 4 != 0 {
        buf.u8(0);
    }
    buf.end(at)
}

fn write_audio_trak(
    buf: &mut Buf,
    movie_duration: u32,
    samples: &[Sample],
    chunk_offset: u32,
) -> Result<(), String> {
    let duration = samples.iter().try_fold(0u32, |sum, sample| {
        sum.checked_add(sample.duration)
            .ok_or_else(|| "duration".to_string())
    })?;
    let at = buf.begin(b"trak");
    write_tkhd(buf, 2, movie_duration, 0, 0, 0x0100)?;
    write_audio_mdia(buf, duration, samples, chunk_offset)?;
    buf.end(at)
}

fn write_audio_mdia(
    buf: &mut Buf,
    duration: u32,
    samples: &[Sample],
    chunk_offset: u32,
) -> Result<(), String> {
    let at = buf.begin(b"mdia");
    write_mdhd(buf, AUDIO_RATE, duration)?;
    write_hdlr(buf, b"soun", "SoundHandler")?;
    write_audio_minf(buf, samples, chunk_offset)?;
    buf.end(at)
}

fn write_audio_minf(buf: &mut Buf, samples: &[Sample], chunk_offset: u32) -> Result<(), String> {
    let at = buf.begin(b"minf");
    write_smhd(buf)?;
    write_hdlr(buf, b"url ", "DataHandler")?;
    write_dinf(buf)?;
    write_audio_stbl(buf, samples, chunk_offset)?;
    buf.end(at)
}

fn write_smhd(buf: &mut Buf) -> Result<(), String> {
    let at = buf.begin(b"smhd");
    buf.full(0, 0);
    buf.u16(0);
    buf.u16(0);
    buf.end(at)
}

fn write_audio_stbl(buf: &mut Buf, samples: &[Sample], chunk_offset: u32) -> Result<(), String> {
    let sizes: Vec<u32> = samples
        .iter()
        .map(|sample| u32::try_from(sample.data.len()).unwrap_or(u32::MAX))
        .collect();
    let at = buf.begin(b"stbl");
    write_audio_stsd(buf)?;
    write_stts(buf, samples)?;
    write_stsc(buf, samples.len())?;
    write_stsz(buf, &sizes)?;
    write_stco(buf, chunk_offset)?;
    buf.end(at)
}

fn write_audio_stsd(buf: &mut Buf) -> Result<(), String> {
    let at = buf.begin(b"stsd");
    buf.full(0, 0);
    buf.u32(1);
    write_sowt(buf)?;
    buf.end(at)
}

fn write_sowt(buf: &mut Buf) -> Result<(), String> {
    let at = buf.begin(b"sowt");
    buf.zeroes(6);
    buf.u16(1);
    buf.u16(1);
    buf.u16(0);
    buf.u32(0);
    buf.u16(2);
    buf.u16(16);
    buf.u16(0);
    buf.u16(0);
    buf.u32(AUDIO_RATE << 16);
    buf.u32(1);
    buf.u32(4);
    buf.u32(4);
    buf.u32(2);
    buf.end(at)
}

fn write_matrix(buf: &mut Buf) {
    buf.u32(0x0001_0000);
    buf.u32(0);
    buf.u32(0);
    buf.u32(0);
    buf.u32(0x0001_0000);
    buf.u32(0);
    buf.u32(0);
    buf.u32(0);
    buf.u32(0x4000_0000);
}

#[derive(Default)]
struct Buf {
    data: Vec<u8>,
}

impl Buf {
    fn begin(&mut self, tag: &[u8; 4]) -> usize {
        let at = self.data.len();
        self.u32(0);
        self.fourcc(tag);
        at
    }

    fn end(&mut self, at: usize) -> Result<(), String> {
        let size = u32::try_from(self.data.len() - at).map_err(|_| "box".to_string())?;
        self.data[at..at + 4].copy_from_slice(&size.to_be_bytes());
        Ok(())
    }

    fn full(&mut self, version: u8, flags: u32) {
        self.u8(version);
        self.u8((flags >> 16) as u8);
        self.u8((flags >> 8) as u8);
        self.u8(flags as u8);
    }

    fn fourcc(&mut self, tag: &[u8; 4]) {
        self.data.extend_from_slice(tag);
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.data.extend_from_slice(bytes);
    }

    fn zeroes(&mut self, count: usize) {
        self.data.extend(std::iter::repeat_n(0, count));
    }

    fn u8(&mut self, value: u8) {
        self.data.push(value);
    }

    fn u16(&mut self, value: u16) {
        self.data.extend_from_slice(&value.to_be_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.data.extend_from_slice(&value.to_be_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic() -> Vec<u8> {
        let sps = vec![0x67, 0x42, 0x00, 0x1E, 0xAB];
        let pps = vec![0x68, 0xCE, 0x38, 0x80];
        let payload = [0x65u8, 0x88, 0x84, 0x21];
        let mut data = (payload.len() as u32).to_be_bytes().to_vec();
        data.extend_from_slice(&payload);
        mux_avc(
            32,
            16,
            TIMESCALE,
            &sps,
            &pps,
            &[Sample {
                data,
                duration: 9_000,
                sync: true,
            }],
            None,
        )
        .unwrap()
    }

    fn walk(data: &[u8], tags: &mut Vec<String>) -> Result<(), String> {
        let mut offset = 0usize;
        while offset + 8 <= data.len() {
            let size = u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
            if size < 8 || offset + size > data.len() {
                return Err(format!("bad box at {offset}"));
            }
            let tag = String::from_utf8_lossy(&data[offset + 4..offset + 8]).into_owned();
            tags.push(tag.clone());
            let nested = matches!(
                tag.as_str(),
                "moov" | "trak" | "mdia" | "minf" | "stbl" | "dinf"
            );
            if nested {
                walk(&data[offset + 8..offset + size], tags)?;
            }
            offset += size;
        }
        if offset != data.len() {
            return Err(format!("trailing {}", data.len() - offset));
        }
        Ok(())
    }

    #[test]
    fn mux_boxes_cover_the_file_and_point_at_the_sample() {
        let bytes = synthetic();
        let mut tags = Vec::new();
        walk(&bytes, &mut tags).unwrap();
        for required in ["ftyp", "mdat", "moov"] {
            assert!(tags.iter().any(|tag| tag == required), "{required}");
        }
        for required in ["ftyp", "mdat", "moov", "avc1", "avcC", "stco"] {
            assert!(
                bytes.windows(4).any(|chunk| chunk == required.as_bytes()),
                "{required}"
            );
        }
        let stco = bytes.windows(4).position(|chunk| chunk == b"stco").unwrap();
        let offset = u32::from_be_bytes(bytes[stco + 12..stco + 16].try_into().unwrap()) as usize;
        assert_eq!(&bytes[offset..offset + 4], &4u32.to_be_bytes());
        assert!(bytes.windows(4).any(|chunk| chunk == b"vide"));
        assert!(!bytes.windows(4).any(|chunk| chunk == b"sowt"));
    }

    #[test]
    fn mux_includes_a_pcm_audio_track() {
        let sps = vec![0x67, 0x42, 0x00, 0x1E, 0xAB];
        let pps = vec![0x68, 0xCE, 0x38, 0x80];
        let payload = [0x65u8, 0x88, 0x84, 0x21];
        let mut data = (payload.len() as u32).to_be_bytes().to_vec();
        data.extend_from_slice(&payload);
        let bytes = mux_avc(
            32,
            16,
            TIMESCALE,
            &sps,
            &pps,
            &[Sample {
                data,
                duration: 9_000,
                sync: true,
            }],
            Some(&[0x1234, -2, 0x1234, -2]),
        )
        .unwrap();
        let mut tags = Vec::new();
        walk(&bytes, &mut tags).unwrap();
        assert_eq!(tags.iter().filter(|tag| tag.as_str() == "trak").count(), 2);
        assert!(bytes.windows(4).any(|chunk| chunk == b"sowt"));
        assert!(bytes.windows(4).any(|chunk| chunk == b"soun"));
        let needle = [0x34u8, 0x12, 0xFE, 0xFF, 0x34, 0x12, 0xFE, 0xFF];
        assert!(bytes.windows(8).any(|chunk| chunk == needle));
        assert!(bytes.len() > 4_800 * 4);
        assert!(bytes.len() < 4_800 * 4 + 20_000);
    }

    #[test]
    fn even_size_drops_the_odd_edge() {
        assert_eq!(even_size(1919, 1080), (1918, 1080));
        assert_eq!(even_size(32, 17), (32, 16));
    }

    #[test]
    fn two_frames_encode_as_a_readable_mp4() {
        let first = RgbaImage::from_pixel(32, 32, image::Rgba([20, 90, 200, 255]));
        let mut second = first.clone();
        second.put_pixel(4, 4, image::Rgba([220, 30, 30, 255]));
        let mut stream = Mp4Stream::create(&first).unwrap();
        stream.push(&second, Duration::from_millis(80)).unwrap();
        let root = std::env::temp_dir().join(format!("pixo-mp4-probe-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = stream
            .finish_file(&root, Duration::from_millis(80), None)
            .unwrap();
        let bytes = fs::read(&path).unwrap();
        assert!(bytes.windows(4).any(|chunk| chunk == b"ftyp"));
        assert!(bytes.windows(4).any(|chunk| chunk == b"avcC"));
        #[cfg(target_os = "macos")]
        {
            let output = std::process::Command::new("file")
                .arg("--brief")
                .arg(&path)
                .output()
                .unwrap();
            let text = String::from_utf8_lossy(&output.stdout);
            assert!(output.status.success(), "{text}");
            assert!(text.contains("MP4") || text.contains("ISO Media"), "{text}");
        }
        let _ = fs::remove_dir_all(&root);
    }
}
