//! Minimal zero-dependency WebAssembly C-ABI bindings for WebAudio `AudioWorkletProcessor`.

use synth_core::{BandoneonPreset, KeyboardMode, MidiEvent, SynthEngine, SynthVersion};

pub struct WasmSynthContext {
    engine: SynthEngine,
    presets: [BandoneonPreset; SynthVersion::COUNT],
    left_buf: [f32; 128],
    right_buf: [f32; 128],
}

#[no_mangle]
pub extern "C" fn synth_create(sample_rate: f32) -> *mut WasmSynthContext {
    let presets: [BandoneonPreset; SynthVersion::COUNT] =
        core::array::from_fn(|i| BandoneonPreset::for_version(SynthVersion::from_u8(i as u8)));
    let preset = presets[SynthVersion::V5Vivo as usize].clone();
    let mut engine = SynthEngine::new(sample_rate, preset);
    engine.set_post_enabled(true);
    let ctx = Box::new(WasmSynthContext {
        engine,
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

/// Switches the active synthesis model version (`0..=12`) without heap allocation on the audio thread.
///
/// # Safety
/// `ctx` must be a valid pointer returned by `synth_create`.
#[no_mangle]
pub unsafe extern "C" fn synth_set_version(ctx: *mut WasmSynthContext, version_id: u8) {
    if let Some(c) = ctx.as_mut() {
        let ver = SynthVersion::from_u8(version_id);
        let idx = (ver as usize).min(SynthVersion::COUNT - 1);
        c.engine.apply_preset_ref(&c.presets[idx]);
    }
}

/// Returns the active synthesis model version ID (`0..=12`).
///
/// # Safety
/// `ctx` must be a valid pointer returned by `synth_create`.
#[no_mangle]
pub unsafe extern "C" fn synth_get_version(ctx: *const WasmSynthContext) -> u8 {
    if let Some(c) = ctx.as_ref() {
        c.engine.version().as_u8()
    } else {
        SynthVersion::V5Vivo.as_u8()
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

/// Enables or bypasses the studio post-processing chain (Spatializer + Compressor + Salon Reverb).
///
/// # Safety
/// `ctx` must be a valid pointer returned by `synth_create`.
#[no_mangle]
pub unsafe extern "C" fn synth_set_post_enabled(ctx: *mut WasmSynthContext, enabled: bool) {
    if let Some(c) = ctx.as_mut() {
        c.engine.set_post_enabled(enabled);
    }
}

/// Updates the studio post-processing parameters in `[0.0, 1.0]`.
///
/// # Safety
/// `ctx` must be a valid pointer returned by `synth_create`.
#[no_mangle]
pub unsafe extern "C" fn synth_set_post_params(
    ctx: *mut WasmSynthContext,
    comp_amount: f32,
    reverb_mix: f32,
    reverb_size: f32,
    spatial_amount: f32,
) {
    if let Some(c) = ctx.as_mut() {
        c.engine
            .set_post_params(comp_amount, reverb_mix, reverb_size, spatial_amount);
    }
}

/// Sets the pitch of A4 in Hz (equal temperament, e.g. `440.0` or `442.0`) for notes started afterwards.
///
/// # Safety
/// `ctx` must be a valid pointer returned by `synth_create`.
#[no_mangle]
pub unsafe extern "C" fn synth_set_reference_hz(ctx: *mut WasmSynthContext, hz: f32) {
    if let Some(c) = ctx.as_mut() {
        c.engine.set_reference_hz(hz);
    }
}

/// Sets the reverb pre-delay amount in `[0.0, 1.0]` (`0.5 ms .. 38 ms`).
///
/// # Safety
/// `ctx` must be a valid pointer returned by `synth_create`.
#[no_mangle]
pub unsafe extern "C" fn synth_set_reverb_predelay(ctx: *mut WasmSynthContext, predelay: f32) {
    if let Some(c) = ctx.as_mut() {
        c.engine.set_reverb_predelay(predelay);
    }
}

/// Returns the instantaneous bus compressor gain reduction in dB (`>= 0.0`).
///
/// # Safety
/// `ctx` must be a valid pointer returned by `synth_create`.
#[no_mangle]
pub unsafe extern "C" fn synth_get_post_gr_db(ctx: *const WasmSynthContext) -> f32 {
    if let Some(c) = ctx.as_ref() {
        c.engine.post_gain_reduction_db()
    } else {
        0.0
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_c_abi_all_13_versions_and_bounds() {
        unsafe {
            let ctx = synth_create(48000.0);
            assert!(!ctx.is_null());
            assert_eq!(synth_get_version(ctx), 12); // Default V5Vivo

            synth_handle_midi(ctx, 0xB0, 11, 100);
            synth_handle_midi(ctx, 0x90, 45, 100); // A2 LeftBajos
            synth_handle_midi(ctx, 0x91, 69, 100); // A4 RightCantabile

            for vid in 0..=12u8 {
                synth_set_version(ctx, vid);
                assert_eq!(synth_get_version(ctx), vid);
                synth_render_quantum(ctx);
                let l_ptr = synth_left_buffer_ptr(ctx);
                let r_ptr = synth_right_buffer_ptr(ctx);
                let l_slice = core::slice::from_raw_parts(l_ptr, 128);
                let r_slice = core::slice::from_raw_parts(r_ptr, 128);
                assert!(l_slice.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
                assert!(r_slice.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
            }

            // Out-of-bounds version ID falls back to V5Vivo (12)
            synth_set_version(ctx, 99);
            assert_eq!(synth_get_version(ctx), 12);

            synth_destroy(ctx);
        }
    }
}
