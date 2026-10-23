class BandolibreProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.wasmExports = null;
    this.ctxPtr = 0;
    this.leftPtr = 0;
    this.rightPtr = 0;
    this.pendingEvents = [];
    this.pendingVersionId = null;
    this.quantumCount = 0;

    this.port.onmessage = async (event) => {
      const msg = event.data;
      if (msg.type === 'init-wasm') {
        try {
          const result = await WebAssembly.instantiate(msg.wasmBytes, {});
          this.wasmExports = result.instance.exports;
          this.ctxPtr = this.wasmExports.synth_create(sampleRate);
          this.leftPtr = this.wasmExports.synth_left_buffer_ptr(this.ctxPtr);
          this.rightPtr = this.wasmExports.synth_right_buffer_ptr(this.ctxPtr);
          if (msg.tableMode !== undefined) {
            this.wasmExports.synth_set_table_mode(this.ctxPtr, msg.tableMode ? 1 : 0);
          }
          const initialVer = this.pendingVersionId !== null ? this.pendingVersionId : msg.versionId;
          if (initialVer !== undefined && initialVer !== null && this.wasmExports.synth_set_version) {
            this.wasmExports.synth_set_version(this.ctxPtr, initialVer | 0);
          }
          if (msg.keyboardMode !== undefined && this.wasmExports.synth_set_keyboard_mode) {
            this.wasmExports.synth_set_keyboard_mode(this.ctxPtr, msg.keyboardMode | 0);
          }
          if (msg.splitNote !== undefined && this.wasmExports.synth_set_split_note) {
            this.wasmExports.synth_set_split_note(this.ctxPtr, msg.splitNote | 0);
          }
          if (msg.stereoWidth !== undefined && this.wasmExports.synth_set_stereo_width) {
            this.wasmExports.synth_set_stereo_width(this.ctxPtr, +msg.stereoWidth);
          }
          if (msg.postEnabled !== undefined && this.wasmExports.synth_set_post_enabled) {
            this.wasmExports.synth_set_post_enabled(this.ctxPtr, msg.postEnabled ? 1 : 0);
          }
          if (msg.postParams && this.wasmExports.synth_set_post_params) {
            this.wasmExports.synth_set_post_params(
              this.ctxPtr,
              +msg.postParams.compAmount,
              +msg.postParams.reverbMix,
              +msg.postParams.reverbSize,
              +msg.postParams.spatialAmount
            );
          }
          for (const ev of this.pendingEvents) {
            this.wasmExports.synth_handle_midi(this.ctxPtr, ev[0], ev[1], ev[2]);
          }
          this.pendingEvents = [];
          const activeVersionId = this.wasmExports.synth_get_version
            ? this.wasmExports.synth_get_version(this.ctxPtr)
            : 11;
          this.port.postMessage({ type: 'ready', sampleRate, activeVersionId });
        } catch (err) {
          this.port.postMessage({ type: 'error', message: String(err) });
        }
      } else if (msg.type === 'midi') {
        if (this.wasmExports && this.ctxPtr) {
          this.wasmExports.synth_handle_midi(this.ctxPtr, msg.status, msg.data1, msg.data2);
        } else {
          this.pendingEvents.push([msg.status, msg.data1, msg.data2]);
        }
      } else if (msg.type === 'table-mode') {
        if (this.wasmExports && this.ctxPtr) {
          this.wasmExports.synth_set_table_mode(this.ctxPtr, msg.enabled ? 1 : 0);
        }
      } else if (msg.type === 'set-keyboard-mode') {
        if (this.wasmExports && this.ctxPtr && this.wasmExports.synth_set_keyboard_mode) {
          this.wasmExports.synth_set_keyboard_mode(this.ctxPtr, msg.modeId | 0);
        }
      } else if (msg.type === 'set-split-note') {
        if (this.wasmExports && this.ctxPtr && this.wasmExports.synth_set_split_note) {
          this.wasmExports.synth_set_split_note(this.ctxPtr, msg.splitNote | 0);
        }
      } else if (msg.type === 'set-stereo-width') {
        if (this.wasmExports && this.ctxPtr && this.wasmExports.synth_set_stereo_width) {
          this.wasmExports.synth_set_stereo_width(this.ctxPtr, +msg.stereoWidth);
        }
      } else if (msg.type === 'set-post-enabled') {
        if (this.wasmExports && this.ctxPtr && this.wasmExports.synth_set_post_enabled) {
          this.wasmExports.synth_set_post_enabled(this.ctxPtr, msg.enabled ? 1 : 0);
        }
      } else if (msg.type === 'set-post-params') {
        if (this.wasmExports && this.ctxPtr && this.wasmExports.synth_set_post_params) {
          this.wasmExports.synth_set_post_params(
            this.ctxPtr,
            +msg.compAmount,
            +msg.reverbMix,
            +msg.reverbSize,
            +msg.spatialAmount
          );
        }
      } else if (msg.type === 'set-version') {
        const vid = msg.versionId | 0;
        this.pendingVersionId = vid;
        if (this.wasmExports && this.ctxPtr && this.wasmExports.synth_set_version) {
          this.wasmExports.synth_set_version(this.ctxPtr, vid);
          const activeVersionId = this.wasmExports.synth_get_version
            ? this.wasmExports.synth_get_version(this.ctxPtr)
            : vid;
          this.port.postMessage({ type: 'version-changed', activeVersionId });
        }
      }
    };
  }

  process(_inputs, outputs, _parameters) {
    const output = outputs[0];
    if (!output || output.length === 0) return true;

    const outL = output[0];
    const outR = output.length > 1 ? output[1] : output[0];

    if (!this.wasmExports || !this.ctxPtr) {
      outL.fill(0);
      outR.fill(0);
      return true;
    }

    this.wasmExports.synth_render_quantum(this.ctxPtr);

    const memBuf = this.wasmExports.memory.buffer;
    const wasmL = new Float32Array(memBuf, this.leftPtr, 128);
    const wasmR = new Float32Array(memBuf, this.rightPtr, 128);

    outL.set(wasmL.subarray(0, outL.length));
    outR.set(wasmR.subarray(0, outR.length));

    this.quantumCount = (this.quantumCount + 1) & 15;
    if (this.quantumCount === 0 && this.wasmExports.synth_get_post_gr_db) {
      const grDb = this.wasmExports.synth_get_post_gr_db(this.ctxPtr);
      this.port.postMessage({ type: 'post-telemetry', grDb });
    }
    return true;
  }
}

registerProcessor('bandolibre-processor', BandolibreProcessor);
