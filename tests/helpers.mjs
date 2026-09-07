import { readFileSync } from 'node:fs';
import vm from 'node:vm';
export const bytes = readFileSync(new URL('../raydrone.wasm', import.meta.url));
export const html = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
export function segment(start, end) {
  const a = html.indexOf(start), b = html.indexOf(end, a + start.length);
  if (a < 0 || b < 0) throw Error(`Missing test boundary: ${start}`);
  return html.slice(a, b);
}
export function engine({ sr = 48000, amplitude = .6, frequency = 220 } = {}) {
  const ex = new WebAssembly.Instance(new WebAssembly.Module(bytes), {}).exports;
  const f32 = (ptr, n) => new Float32Array(ex.memory.buffer, ptr, n);
  const len = Math.round(sr);
  const source = f32(ex.sample_ptr(), len);
  for (let i = 0; i < len; i++) source[i] = amplitude * Math.sin(i * 2 * Math.PI * frequency / sr);
  ex.set_output_sample_rate(sr); ex.set_sample(len, sr);
  for (const [ptr, n] of [[ex.window_ptr(), ex.window_capacity()], [ex.lab_win_ptr(), ex.lab_grain()]]) {
    const win = f32(ptr, n);
    for (let i = 0; i < n; i++) win[i] = .5 - .5 * Math.cos(2 * Math.PI * i / (n - 1));
  }
  ex.set_params(.3, .1, 150, 200, .3, 1);
  return { ex, f32, len, sr };
}
export function render(e, blocks = 200, frames = 128) {
  let energy = 0, peak = 0;
  for (let b = 0; b < blocks; b++) {
    e.ex.process(frames);
    for (const x of e.f32(e.ex.out_l_ptr(), frames)) {
      if (!Number.isFinite(x)) throw Error('Non-finite output');
      if (b >= Math.floor(blocks / 2)) energy += x*x;
      peak = Math.max(peak, Math.abs(x));
    }
  }
  return { energy, peak };
}
export function processor(sr = 48000) {
  let Klass;
  class Base { constructor() { this.messages = []; this.port = { postMessage: m => this.messages.push(structuredClone(m)) }; } }
  vm.runInNewContext(readFileSync(new URL('../processor.js', import.meta.url), 'utf8'), {
    AudioWorkletProcessor: Base, WebAssembly, Float32Array, sampleRate: sr,
    registerProcessor: (_, cls) => { Klass = cls; }
  });
  return new Klass({ processorOptions: { wasmBytes: bytes } });
}
