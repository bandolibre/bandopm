class BandolibreProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.wasmExports = null;
    this.ctxPtr = 0;
    this.leftPtr = 0;
    this.rightPtr = 0;
    this.pendingEvents = [];

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
          for (const ev of this.pendingEvents) {
            this.wasmExports.synth_handle_midi(this.ctxPtr, ev[0], ev[1], ev[2]);
          }
          this.pendingEvents = [];
          this.port.postMessage({ type: 'ready', sampleRate });
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
    return true;
  }
}

registerProcessor('bandolibre-processor', BandolibreProcessor);
