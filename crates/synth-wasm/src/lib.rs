//! Minimal zero-dependency WebAssembly C-ABI bindings for WebAudio `AudioWorkletProcessor`.

use synth_core::{BandoneonPreset, MidiEvent, SynthEngine};

pub struct WasmSynthContext {
    engine: SynthEngine,
    left_buf: [f32; 128],
    right_buf: [f32; 128],
}

#[no_mangle]
pub extern "C" fn synth_create(sample_rate: f32) -> *mut WasmSynthContext {
    let preset = BandoneonPreset::default_bandoneon();
    let ctx = Box::new(WasmSynthContext {
        engine: SynthEngine::new(sample_rate, preset),
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
