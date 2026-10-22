//! Minimal zero-dependency WebAssembly C-ABI bindings for WebAudio `AudioWorkletProcessor`.

use synth_core::{BandoneonPreset, KeyboardMode, MidiEvent, SynthEngine, SynthVersion};

pub struct WasmSynthContext {
    engine: SynthEngine,
    presets: [BandoneonPreset; 6],
    left_buf: [f32; 128],
    right_buf: [f32; 128],
}

#[no_mangle]
pub extern "C" fn synth_create(sample_rate: f32) -> *mut WasmSynthContext {
    let presets = [
        BandoneonPreset::v1_baseline(),
        BandoneonPreset::v2_adaa_baffle(),
        BandoneonPreset::v3_aero_radiation(),
        BandoneonPreset::v3_clamped_bar(),
        BandoneonPreset::v3_transient_entrain(),
        BandoneonPreset::v3_unified(),
    ];
    let preset = presets[5].clone();
    let ctx = Box::new(WasmSynthContext {
        engine: SynthEngine::new(sample_rate, preset),
        presets,
        left_buf: [0.0; 128],
        right_buf: [0.0; 128],
    });
    Box::into_raw(ctx)
}

/// # Safety
/// `ctx` must be a valid pointer returned by `synth_create`.
#[no_mangle]
pub unsafe extern "C" fn synth_destroy(ctx: *mut WasmSynthContext) {
    if !ctx.is_null() {
        drop(Box::from_raw(ctx));
    }
}

/// # Safety
/// `ctx` must be a valid pointer returned by `synth_create`.
#[no_mangle]
pub unsafe extern "C" fn synth_handle_midi(
    ctx: *mut WasmSynthContext,
    status: u8,
    data1: u8,
    data2: u8,
) {
    if let Some(c) = ctx.as_mut() {
        c.engine.handle_raw_midi(status, data1, data2);
    }
}

/// # Safety
/// `ctx` must be a valid pointer returned by `synth_create`.
#[no_mangle]
pub unsafe extern "C" fn synth_set_table_mode(ctx: *mut WasmSynthContext, enabled: bool) {
    if let Some(c) = ctx.as_mut() {
        c.engine.handle_midi_event(MidiEvent::SetTableMode(enabled));
    }
}

/// Switches the active synthesis model version (`0..=5`) without heap allocation on the audio thread.
///
/// # Safety
/// `ctx` must be a valid pointer returned by `synth_create`.
#[no_mangle]
pub unsafe extern "C" fn synth_set_version(ctx: *mut WasmSynthContext, version_id: u8) {
    if let Some(c) = ctx.as_mut() {
        let ver = SynthVersion::from_u8(version_id);
        let idx = (ver as usize).min(5);
        c.engine.apply_preset_ref(&c.presets[idx]);
    }
}

/// Returns the active synthesis model version ID (`0..=5`).
///
/// # Safety
/// `ctx` must be a valid pointer returned by `synth_create`.
#[no_mangle]
pub unsafe extern "C" fn synth_get_version(ctx: *const WasmSynthContext) -> u8 {
    if let Some(c) = ctx.as_ref() {
        c.engine.version().as_u8()
    } else {
        SynthVersion::V3Unified.as_u8()
    }
}

/// Sets the keyboard/channel simulation mode (`0 = DualChannelBandolibre`, `1 = PianistSplit`).
///
/// # Safety
/// `ctx` must be a valid pointer returned by `synth_create`.
#[no_mangle]
pub unsafe extern "C" fn synth_set_keyboard_mode(ctx: *mut WasmSynthContext, mode_id: u8) {
    if let Some(c) = ctx.as_mut() {
        c.engine
            .handle_midi_event(MidiEvent::SetKeyboardMode(KeyboardMode::from_u8(mode_id)));
    }
}

/// Sets the split MIDI note (`36..=96`, default `60 = C4`) used in `PianistSplit` mode.
///
/// # Safety
/// `ctx` must be a valid pointer returned by `synth_create`.
#[no_mangle]
pub unsafe extern "C" fn synth_set_split_note(ctx: *mut WasmSynthContext, split_note: u8) {
    if let Some(c) = ctx.as_mut() {
        c.engine
            .handle_midi_event(MidiEvent::SetSplitNote(split_note));
    }
}

/// Sets the stereo panning separation width in `[0.0, 1.0]` between Left (*Bajos*, Ch 1)
/// and Right (*Cantabile*, Ch 2) keyboards.
///
/// # Safety
/// `ctx` must be a valid pointer returned by `synth_create`.
#[no_mangle]
pub unsafe extern "C" fn synth_set_stereo_width(ctx: *mut WasmSynthContext, width: f32) {
    if let Some(c) = ctx.as_mut() {
        c.engine.handle_midi_event(MidiEvent::SetStereoWidth(width));
    }
}

/// Renders one 128-sample WebAudio quantum and returns pointers to internal left/right buffers.
///
/// # Safety
/// `ctx` must be a valid pointer returned by `synth_create`.
#[no_mangle]
pub unsafe extern "C" fn synth_render_quantum(ctx: *mut WasmSynthContext) {
    if let Some(c) = ctx.as_mut() {
        c.engine.render(&mut c.left_buf, &mut c.right_buf);
    }
}

/// # Safety
/// `ctx` must be a valid pointer returned by `synth_create`.
#[no_mangle]
pub unsafe extern "C" fn synth_left_buffer_ptr(ctx: *const WasmSynthContext) -> *const f32 {
    (*ctx).left_buf.as_ptr()
}

/// # Safety
/// `ctx` must be a valid pointer returned by `synth_create`.
#[no_mangle]
pub unsafe extern "C" fn synth_right_buffer_ptr(ctx: *const WasmSynthContext) -> *const f32 {
    (*ctx).right_buf.as_ptr()
}
