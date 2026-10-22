mod alsa_ffi;

use alsa_ffi::{AlsaSeqMidiListener, LinuxAudioStream};
use anyhow::Result;
use clap::Parser;
use hound::{SampleFormat, WavSpec, WavWriter};
use rtrb::RingBuffer;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use synth_core::{BandoneonPreset, Hand, MidiEvent, SynthEngine, SynthVersion};

#[derive(Parser, Debug)]
#[command(
    name = "synth-standalone",
    about = "Real-time polyphonic Bandolibre physical modeling synthesizer for Linux (ALSA / PipeWire)"
)]
struct Cli {
    /// Substring or [client:port] of the MIDI input device to bind to (e.g. "Bandolibre", "Through", "20:0")
    #[arg(short = 'm', long, default_value = "Bandolibre")]
    midi_device: String,

    /// List all detected ALSA Sequencer MIDI input devices and exit
    #[arg(short = 'l', long)]
    list_devices: bool,

    /// Audio output device ("default", "pulse", "hw:0,0", etc.)
    #[arg(short = 'a', long, default_value = "default")]
    audio_device: String,

    /// Sample rate in Hz
    #[arg(short = 'r', long, default_value_t = 48000)]
    sample_rate: u32,

    /// Render quantum size in frames (e.g. 128 = 2.67ms at 48kHz)
    #[arg(short = 'b', long, default_value_t = 128)]
    buffer_frames: usize,

    /// Path to calibrated bandoneon preset JSON
    #[arg(
        short = 'p',
        long,
        default_value = "assets/presets/default_bandoneon.json"
    )]
    preset: PathBuf,

    /// Optional synthesis version override (`v1`, `v2`, `v3_aero`, `v3_beam`, `v3_transient`, `v3`)
    #[arg(long)]
    version: Option<String>,

    /// Enable Table Mode (allows playing notes using Note On velocity without moving the bellows / CC#11)
    #[arg(long)]
    table_mode: bool,

    /// Run a self-test polyphonic tango chord + bellows swell and write to the specified WAV path
    #[arg(long)]
    demo_wav: Option<PathBuf>,
}

fn load_preset(path: &PathBuf, version_override: Option<&str>) -> Result<BandoneonPreset> {
    if let Some(v_str) = version_override {
        let ver = SynthVersion::from_slug(v_str).ok_or_else(|| {
            anyhow::anyhow!(
                "Unknown synthesis version '{}'. Expected one of: v1, v2, v3_aero, v3_beam, v3_transient, v3",
                v_str
            )
        })?;
        return Ok(BandoneonPreset::for_version(ver));
    }
    Ok(std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(BandoneonPreset::default_bandoneon))
}

fn render_demo_wav(path: &PathBuf, sample_rate: u32, preset: BandoneonPreset) -> Result<()> {
    let mut engine = SynthEngine::new(sample_rate as f32, preset);
    // Play an A-minor tango chord across Left Hand (Bajos: A2, E3) and Right Hand (Cantabile: A4, C5, E5)
    for (note, velocity, hand) in [
        (45, 92, Hand::LeftBajos),
        (52, 88, Hand::LeftBajos),
        (69, 96, Hand::RightCantabile),
        (72, 90, Hand::RightCantabile),
        (76, 94, Hand::RightCantabile),
    ] {
        engine.handle_midi_event(MidiEvent::NoteOn {
            note,
            velocity,
            hand,
        });
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let spec = WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec)?;

    let dur_sec = 2.5f32;
    let total_frames = (sample_rate as f32 * dur_sec) as usize;
    let block = 128usize;
    let mut left = vec![0.0f32; block];
    let mut right = vec![0.0f32; block];

    for frame in (0..total_frames).step_by(block) {
        let t = (frame as f32) / (sample_rate as f32);
        let expr = if t < 0.8 {
            0.15 + 0.80 * (t / 0.8)
        } else if t < 1.8 {
            0.95 - 0.55 * ((t - 0.8) / 1.0)
        } else {
            0.40 * (1.0 - ((t - 1.8) / 0.7).min(1.0))
        };
        engine.handle_midi_event(MidiEvent::ExpressionNormalized(expr));
        if frame == (sample_rate as usize * 2) {
            engine.handle_midi_event(MidiEvent::AllNotesOff);
        }

        let n = block.min(total_frames - frame);
        engine.render(&mut left[..n], &mut right[..n]);
        for i in 0..n {
            writer.write_sample((left[i].clamp(-1.0, 1.0) * 32767.0) as i16)?;
            writer.write_sample((right[i].clamp(-1.0, 1.0) * 32767.0) as i16)?;
        }
    }

    writer.finalize()?;
    println!(
        "[synth-standalone] Wrote 5-voice polyphonic tango chord demo WAV ({:.1}s) to {:?}",
        dur_sec, path
    );
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let preset = load_preset(&cli.preset, cli.version.as_deref())?;

    if let Some(ref wav_path) = cli.demo_wav {
        return render_demo_wav(wav_path, cli.sample_rate, preset);
    }

    if cli.list_devices {
        println!("Scanning ALSA Sequencer MIDI ports...");
        match AlsaSeqMidiListener::list_ports() {
            Ok(ports) if ports.is_empty() => {
                println!("  (No hardware/virtual MIDI ports currently registered)")
            }
            Ok(ports) => {
                for p in ports {
                    println!("  {}", p.display_name());
                }
            }
            Err(e) => println!("  Warning: {}", e),
        }
        return Ok(());
    }

    println!(
        "[synth-standalone] Version: {} | Preset: '{}' | Sample Rate: {} Hz | Quantum: {} frames",
        preset.version.slug(),
        preset.name,
        cli.sample_rate,
        cli.buffer_frames
    );

    let (mut midi_prod, mut midi_cons) = RingBuffer::<(u8, u8, u8)>::new(1024);
    let running = Arc::new(AtomicBool::new(true));

    // 1. Open and bind ALSA Sequencer MIDI listener by device name
    match AlsaSeqMidiListener::open_and_bind(&cli.midi_device) {
        Ok((mut listener, matched_port)) => {
            if let Some(port) = matched_port {
                println!(
                    "[synth-standalone] Bound to MIDI device: {}",
                    port.display_name()
                );
            } else {
                println!(
                    "[synth-standalone] No MIDI port matched '{}'. Created virtual ALSA Sequencer port 'Bandolibre-Synth:MIDI-In' waiting for connections.",
                    cli.midi_device
                );
            }

            let run_flag = Arc::clone(&running);
            std::thread::Builder::new()
                .name("midi-input".to_string())
                .spawn(move || {
                    let mut pkt = [0u8; 16];
                    while run_flag.load(Ordering::Relaxed) {
                        if let Some(n) = listener.recv_midi_packet(&mut pkt) {
                            if n >= 3 {
                                let _ = midi_prod.push((pkt[0], pkt[1], pkt[2]));
                            } else if n == 2 {
                                let _ = midi_prod.push((pkt[0], pkt[1], 0));
                            }
                        }
                    }
                })?;
        }
        Err(e) => {
            eprintln!("[synth-standalone] MIDI open warning: {}", e);
        }
    }

    // 2. Open low-latency Linux audio output stream (ALSA PCM or PulseAudio/PipeWire)
    let latency_us =
        ((cli.buffer_frames as u64 * 4 * 1_000_000) / (cli.sample_rate as u64)).max(6000) as u32;
    let mut audio_out = LinuxAudioStream::open(&cli.audio_device, cli.sample_rate, latency_us)?;
    println!(
        "[synth-standalone] Audio stream active on '{}' (target latency: {:.1} ms). Press Ctrl+C to stop.",
        cli.audio_device,
        latency_us as f32 / 1000.0
    );

    // 3. Real-time polyphonic synthesis loop (zero allocations)
    let mut engine = SynthEngine::new(cli.sample_rate as f32, preset);
    if cli.table_mode {
        engine.handle_midi_event(MidiEvent::SetTableMode(true));
    }

    let n_frames = cli.buffer_frames.max(32);
    let mut left = vec![0.0f32; n_frames];
    let mut right = vec![0.0f32; n_frames];
    let mut interleaved = vec![0.0f32; n_frames * 2];

    while running.load(Ordering::Relaxed) {
        while let Ok((status, d1, d2)) = midi_cons.pop() {
            engine.handle_raw_midi(status, d1, d2);
        }
        engine.render(&mut left, &mut right);
        for i in 0..n_frames {
            interleaved[i * 2] = left[i];
            interleaved[i * 2 + 1] = right[i];
        }
        audio_out.write_interleaved(&interleaved)?;
    }

    Ok(())
}
