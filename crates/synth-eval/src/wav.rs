use anyhow::{Context, Result};
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use std::path::Path;

pub struct WavData {
    pub sample_rate: u32,
    pub channels: u16,
    /// Mono-mixed normalized samples in `[-1.0, 1.0]`.
    pub samples_mono: Vec<f32>,
}

/// Loads a 16/24/32-bit PCM or float WAV file using `hound` and downmixes to normalized mono.
pub fn load_wav_mono(path: &Path, max_frames: Option<usize>) -> Result<WavData> {
    let mut reader =
        WavReader::open(path).with_context(|| format!("Failed to open WAV {:?}", path))?;
    let spec = reader.spec();
    let ch = spec.channels.max(1) as usize;
    let total_frames = (reader.len() as usize) / ch;
    let frames_to_read = max_frames.map_or(total_frames, |m| total_frames.min(m));
    let max_samples = frames_to_read * ch;

    let mut samples_mono = Vec::with_capacity(frames_to_read);
    let inv_ch = 1.0 / (ch as f32);

    match spec.sample_format {
        SampleFormat::Int => {
            let scale = 1.0 / ((1i64 << (spec.bits_per_sample.saturating_sub(1))) as f32);
            let mut iter = reader.samples::<i32>().take(max_samples);
            for _ in 0..frames_to_read {
                let mut sum = 0.0f32;
                for _ in 0..ch {
                    if let Some(s) = iter.next() {
                        sum += (s? as f32) * scale;
                    }
                }
                samples_mono.push(sum * inv_ch);
            }
        }
        SampleFormat::Float => {
            let mut iter = reader.samples::<f32>().take(max_samples);
            for _ in 0..frames_to_read {
                let mut sum = 0.0f32;
                for _ in 0..ch {
                    if let Some(s) = iter.next() {
                        sum += s?;
                    }
                }
                samples_mono.push(sum * inv_ch);
            }
        }
    }

    Ok(WavData {
        sample_rate: spec.sample_rate,
        channels: spec.channels,
        samples_mono,
    })
}

/// Writes a 16-bit mono WAV file to disk using `hound`.
pub fn write_wav_mono_16bit(path: &Path, sample_rate: u32, samples: &[f32]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec)?;
    for &s in samples {
        writer.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16)?;
    }
    writer.finalize()?;
    Ok(())
}
