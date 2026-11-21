//! Microphone and speaker capture for MP4 recordings.
//! Both sources are mixed to 48 kHz stereo PCM.

use std::sync::{Arc, Mutex};

use cpal::SizedSample;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

#[cfg(target_os = "macos")]
use screencapturekit::cm::AudioBufferList;
#[cfg(target_os = "macos")]
use screencapturekit::prelude::*;

const RATE: u32 = 48_000;

pub(crate) struct AudioCapture {
    microphone_stream: Option<cpal::Stream>,
    speaker_stream: Option<cpal::Stream>,
    #[cfg(target_os = "macos")]
    speaker_screen: Option<SCStream>,
    microphone_pcm: Arc<Mutex<Vec<f32>>>,
    speaker_pcm: Arc<Mutex<Vec<f32>>>,
    microphone: bool,
    speaker: bool,
}

impl AudioCapture {
    pub(crate) fn start(
        speaker: bool,
        microphone: bool,
        speaker_name: &str,
        microphone_name: &str,
    ) -> Result<Self, String> {
        let microphone_pcm = Arc::new(Mutex::new(Vec::new()));
        let speaker_pcm = Arc::new(Mutex::new(Vec::new()));
        let microphone_stream = if microphone {
            Some(open_microphone(
                microphone_name,
                Arc::clone(&microphone_pcm),
            )?)
        } else {
            None
        };
        #[cfg(target_os = "macos")]
        let speaker_screen = if speaker {
            // ScreenCaptureKit records the computer's playback, not one output device.
            Some(open_speaker_screen(Arc::clone(&speaker_pcm))?)
        } else {
            None
        };
        #[cfg(target_os = "macos")]
        let speaker_stream = None;
        #[cfg(not(target_os = "macos"))]
        let speaker_stream = if speaker {
            Some(open_speaker_cpal(speaker_name, Arc::clone(&speaker_pcm))?)
        } else {
            None
        };
        eprintln!(
            "screenshot audio: speaker={speaker} speaker_device={speaker_name} microphone={microphone} microphone_device={microphone_name}"
        );
        Ok(Self {
            microphone_stream,
            speaker_stream,
            #[cfg(target_os = "macos")]
            speaker_screen,
            microphone_pcm,
            speaker_pcm,
            microphone,
            speaker,
        })
    }

    pub(crate) fn finish(mut self) -> Vec<i16> {
        #[cfg(target_os = "macos")]
        if let Some(stream) = self.speaker_screen.take() {
            if let Err(error) = stream.stop_capture() {
                eprintln!("screenshot audio: {error:?}");
            }
        }
        drop(self.microphone_stream.take());
        drop(self.speaker_stream.take());
        let mut tracks = Vec::new();
        if self.microphone {
            tracks.push(take_pcm(&self.microphone_pcm));
        }
        if self.speaker {
            tracks.push(take_pcm(&self.speaker_pcm));
        }
        mix_to_i16(&tracks)
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        if let Some(stream) = self.speaker_screen.take() {
            let _ = stream.stop_capture();
        }
    }
}

pub(crate) fn list_devices() -> (Vec<String>, Vec<String>) {
    let host = cpal::default_host();
    let speakers = named_list(host.default_output_device(), host.output_devices().ok());
    let microphones = named_list(host.default_input_device(), host.input_devices().ok());
    (speakers, microphones)
}

fn named_list(
    default: Option<cpal::Device>,
    devices: Option<impl Iterator<Item = cpal::Device>>,
) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(device) = default {
        if let Ok(name) = device.name() {
            push_device(&mut names, name);
        }
    }
    if let Some(devices) = devices {
        for device in devices {
            if let Ok(name) = device.name() {
                push_device(&mut names, name);
            }
        }
    }
    names
}

fn push_device(names: &mut Vec<String>, name: String) {
    let name = name.trim();
    if name.is_empty() || names.iter().any(|existing| existing == name) {
        return;
    }
    names.push(name.to_string());
}

fn take_pcm(slot: &Mutex<Vec<f32>>) -> Vec<f32> {
    std::mem::take(&mut slot.lock().unwrap_or_else(|error| error.into_inner()))
}

fn open_microphone(name: &str, target: Arc<Mutex<Vec<f32>>>) -> Result<cpal::Stream, String> {
    let host = cpal::default_host();
    let device = named_input(&host, name)?;
    let supported = device
        .default_input_config()
        .map_err(|error| format!("microphone: {error}"))?;
    build_stream(&device, &supported, target).map_err(|error| format!("microphone: {error}"))
}

fn named_input(host: &cpal::Host, name: &str) -> Result<cpal::Device, String> {
    let wanted = name.trim();
    if wanted.is_empty() {
        return host
            .default_input_device()
            .ok_or_else(|| "microphone: no input device".to_string());
    }
    let devices = host
        .input_devices()
        .map_err(|error| format!("microphone: {error}"))?;
    for device in devices {
        if device_name_is(&device, wanted) {
            return Ok(device);
        }
    }
    Err(format!("microphone: {wanted}"))
}

#[cfg(target_os = "windows")]
fn named_output(host: &cpal::Host, name: &str) -> Result<cpal::Device, String> {
    let wanted = name.trim();
    if wanted.is_empty() {
        return host
            .default_output_device()
            .ok_or_else(|| "speaker: no output device".to_string());
    }
    let devices = host
        .output_devices()
        .map_err(|error| format!("speaker: {error}"))?;
    for device in devices {
        if device_name_is(&device, wanted) {
            return Ok(device);
        }
    }
    Err(format!("speaker: {wanted}"))
}

fn device_name_is(device: &cpal::Device, wanted: &str) -> bool {
    device.name().ok().is_some_and(|name| name.trim() == wanted)
}

#[cfg(target_os = "macos")]
fn open_speaker_screen(target: Arc<Mutex<Vec<f32>>>) -> Result<SCStream, String> {
    let content = SCShareableContent::get().map_err(|error| format!("speaker: {error:?}"))?;
    let displays = content.displays();
    let display = displays
        .first()
        .ok_or_else(|| "speaker: no display".to_string())?;
    let filter = SCContentFilter::create()
        .with_display(display)
        .with_excluding_windows(&[])
        .build()
        .map_err(|error| format!("speaker: {error:?}"))?;
    let interval = CMTime::new(1, 1);
    let config = SCStreamConfiguration::new()
        .with_width(64)
        .with_height(64)
        .with_captures_audio(true)
        .with_sample_rate(i32::try_from(RATE).unwrap_or(48_000))
        .with_channel_count(2)
        .with_minimum_frame_interval(&interval);
    let mut stream =
        SCStream::new(&filter, &config).map_err(|error| format!("speaker: {error:?}"))?;
    stream
        .add_output_handler(
            move |sample: CMSampleBuffer, _kind: SCStreamOutputType| {
                let Ok(list) = sample.audio_buffer_list() else {
                    return;
                };
                let stereo = stereo_from_buffers(&list);
                if stereo.is_empty() {
                    return;
                }
                let mut buffered = target.lock().unwrap_or_else(|error| error.into_inner());
                buffered.extend_from_slice(&stereo);
            },
            SCStreamOutputType::Audio,
        )
        .map_err(|error| format!("speaker: {error:?}"))?;
    stream
        .start_capture()
        .map_err(|error| format!("speaker: {error:?}"))?;
    Ok(stream)
}

#[cfg(target_os = "macos")]
fn stereo_from_buffers(list: &AudioBufferList) -> Vec<f32> {
    let count = list.num_buffers();
    if count == 0 {
        return Vec::new();
    }
    if count == 1 {
        let Some(buffer) = list.get(0) else {
            return Vec::new();
        };
        let channels = usize::try_from(buffer.number_channels.max(1)).unwrap_or(1);
        return to_stereo(&f32_samples(buffer.data()), channels);
    }
    let mut planes = Vec::new();
    for index in 0..count {
        let Some(buffer) = list.get(index) else {
            continue;
        };
        planes.push(f32_samples(buffer.data()));
    }
    let frames = planes.iter().map(Vec::len).max().unwrap_or(0);
    let mut out = Vec::with_capacity(frames * 2);
    for frame in 0..frames {
        let left = planes
            .first()
            .and_then(|plane| plane.get(frame))
            .copied()
            .unwrap_or(0.0);
        let right = planes
            .get(1)
            .and_then(|plane| plane.get(frame))
            .copied()
            .unwrap_or(left);
        out.push(left);
        out.push(right);
    }
    out
}

#[cfg(target_os = "macos")]
fn f32_samples(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

#[cfg(target_os = "macos")]
fn to_stereo(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        let mut out = Vec::with_capacity(samples.len() * 2);
        for sample in samples {
            out.push(*sample);
            out.push(*sample);
        }
        return out;
    }
    let frames = samples.len() / channels;
    let mut out = Vec::with_capacity(frames * 2);
    for frame in 0..frames {
        let base = frame * channels;
        out.push(samples[base]);
        out.push(samples[base + 1]);
    }
    out
}

#[cfg(target_os = "windows")]
fn open_speaker_cpal(name: &str, target: Arc<Mutex<Vec<f32>>>) -> Result<cpal::Stream, String> {
    // WASAPI treats an input stream on an output device as loopback.
    let host = cpal::default_host();
    let device = named_output(&host, name)?;
    let supported = device
        .default_output_config()
        .map_err(|error| format!("speaker: {error}"))?;
    build_stream(&device, &supported, target).map_err(|error| format!("speaker: {error}"))
}

#[cfg(target_os = "linux")]
fn open_speaker_cpal(name: &str, target: Arc<Mutex<Vec<f32>>>) -> Result<cpal::Stream, String> {
    // PulseAudio and PipeWire expose the speaker as an input named "monitor".
    let host = cpal::default_host();
    let device = monitor_device(&host, name)?;
    let supported = device
        .default_input_config()
        .map_err(|error| format!("speaker: {error}"))?;
    build_stream(&device, &supported, target).map_err(|error| format!("speaker: {error}"))
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn open_speaker_cpal(name: &str, target: Arc<Mutex<Vec<f32>>>) -> Result<cpal::Stream, String> {
    let _ = (name, target);
    Err("speaker: unsupported".to_string())
}

#[cfg(any(test, target_os = "linux"))]
fn monitor_matches(source: &str, output: &str) -> bool {
    let source = source.to_ascii_lowercase();
    if !source.contains("monitor") {
        return false;
    }
    let output = output.trim().to_ascii_lowercase();
    output.is_empty() || source.contains(&output)
}

#[cfg(target_os = "linux")]
fn monitor_device(host: &cpal::Host, output: &str) -> Result<cpal::Device, String> {
    let devices = host
        .input_devices()
        .map_err(|error| format!("speaker: {error}"))?;
    for device in devices {
        let name = device.name().unwrap_or_default();
        if monitor_matches(&name, output) {
            return Ok(device);
        }
    }
    Err("speaker: monitor source was not found".to_string())
}

fn build_stream(
    device: &cpal::Device,
    supported: &cpal::SupportedStreamConfig,
    target: Arc<Mutex<Vec<f32>>>,
) -> Result<cpal::Stream, String> {
    let channels = supported.channels();
    let rate = supported.sample_rate().0;
    let config = supported.config();
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => {
            stream_of::<f32>(device, &config, channels, rate, target, |sample| {
                sample.clamp(-1.0, 1.0)
            })
        }
        cpal::SampleFormat::I16 => {
            stream_of::<i16>(device, &config, channels, rate, target, |sample| {
                f32::from(sample) / 32768.0
            })
        }
        cpal::SampleFormat::U16 => {
            stream_of::<u16>(device, &config, channels, rate, target, |sample| {
                (f32::from(sample) - 32768.0) / 32768.0
            })
        }
        cpal::SampleFormat::I32 => {
            stream_of::<i32>(device, &config, channels, rate, target, |sample| {
                sample as f32 / 2147483648.0
            })
        }
        cpal::SampleFormat::F64 => {
            stream_of::<f64>(device, &config, channels, rate, target, |sample| {
                (sample as f32).clamp(-1.0, 1.0)
            })
        }
        other => return Err(format!("format: {other:?}")),
    }?;
    stream.play().map_err(|error| error.to_string())?;
    Ok(stream)
}

fn stream_of<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: u16,
    rate: u32,
    target: Arc<Mutex<Vec<f32>>>,
    convert: fn(T) -> f32,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + Copy + 'static,
{
    let mut resampler = Resampler::new(channels, rate);
    let mut decoded = Vec::<f32>::new();
    let mut stereo = Vec::<f32>::new();
    device
        .build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                decoded.clear();
                decoded.extend(data.iter().copied().map(convert));
                stereo.clear();
                resampler.push(&decoded, &mut stereo);
                if stereo.is_empty() {
                    return;
                }
                let mut buffered = target.lock().unwrap_or_else(|error| error.into_inner());
                buffered.extend_from_slice(&stereo);
            },
            |error| eprintln!("screenshot audio: {error}"),
            None,
        )
        .map_err(|error| error.to_string())
}

struct Resampler {
    channels: usize,
    step: f64,
    pos: f64,
    pending: Vec<f32>,
}

impl Resampler {
    fn new(channels: u16, rate: u32) -> Self {
        let rate = if rate == 0 { RATE } else { rate };
        Self {
            channels: usize::from(channels.max(1)),
            step: f64::from(rate) / f64::from(RATE),
            pos: 0.0,
            pending: Vec::new(),
        }
    }

    fn push(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let channels = self.channels;
        let usable = input.len() - input.len() % channels;
        if usable == 0 {
            return;
        }
        self.pending.extend_from_slice(&input[..usable]);
        let frames = self.pending.len() / channels;
        while self.pos + 1.0 < frames as f64 {
            let frame = self.pos.floor() as usize;
            let frac = (self.pos - frame as f64) as f32;
            let left = blend(&self.pending, channels, frame, 0, frac);
            let right = if channels == 1 {
                left
            } else {
                blend(&self.pending, channels, frame, 1, frac)
            };
            out.push(left);
            out.push(right);
            self.pos += self.step;
        }
        let drop_frames = (self.pos.floor() as usize).min(self.pending.len() / channels);
        if drop_frames > 0 {
            self.pending.drain(..drop_frames * channels);
            self.pos -= drop_frames as f64;
        }
    }
}

fn blend(pending: &[f32], channels: usize, frame: usize, channel: usize, frac: f32) -> f32 {
    let src = if channels == 1 {
        0
    } else {
        channel.min(channels - 1)
    };
    let a = pending[frame * channels + src];
    let b = pending[(frame + 1) * channels + src];
    a + (b - a) * frac
}

fn mix_to_i16(tracks: &[Vec<f32>]) -> Vec<i16> {
    let length = tracks.iter().map(Vec::len).max().unwrap_or(0) / 2 * 2;
    let mut mixed = Vec::with_capacity(length);
    for index in 0..length {
        let mut sample = 0.0f32;
        for track in tracks {
            sample += track.get(index).copied().unwrap_or(0.0);
        }
        let sample = sample.clamp(-1.0, 1.0);
        mixed.push((sample * 32767.0).round() as i16);
    }
    mixed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampler_doubles_a_quiet_ramp() {
        let mut resampler = Resampler::new(1, 24_000);
        let mut out = Vec::new();
        resampler.push(&[0.0, 1.0, 1.0, 0.0], &mut out);
        assert_eq!(
            out,
            vec![0.0, 0.0, 0.5, 0.5, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.5, 0.5]
        );
    }

    #[test]
    fn mix_clamps_the_sum_and_drops_a_dangling_sample() {
        assert_eq!(mix_to_i16(&[vec![1.0, -1.0]]), vec![32767, -32767]);
        assert_eq!(
            mix_to_i16(&[vec![1.0, 0.0], vec![1.0, 0.0]]),
            vec![32767, 0]
        );
        assert_eq!(mix_to_i16(&[vec![0.0, 0.0, 1.0]]), vec![0, 0]);
        assert!(mix_to_i16(&[]).is_empty());
    }

    #[test]
    fn device_list_keeps_the_default_first_and_skips_duplicates() {
        let mut names = Vec::new();
        push_device(&mut names, " Speakers ".to_string());
        push_device(&mut names, "Speakers".to_string());
        push_device(&mut names, "  ".to_string());
        push_device(&mut names, "Headphones".to_string());
        assert_eq!(
            names,
            vec!["Speakers".to_string(), "Headphones".to_string()]
        );
    }

    #[test]
    fn monitor_source_matches_its_output() {
        assert!(monitor_matches("Monitor of Built-in Audio", ""));
        assert!(monitor_matches(
            "Monitor of Built-in Audio",
            "Built-in Audio"
        ));
        assert!(!monitor_matches("Built-in Audio", "Built-in Audio"));
        assert!(!monitor_matches("Monitor of HDMI", "Built-in Audio"));
    }
}
