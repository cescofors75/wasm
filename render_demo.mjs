// Renders a "musicality" A/B from the RayDrone WASM engine to WAV, so you can
// hear the difference without a browser. Same source note for both:
//   before — the current default: one continuous pitch, a flat granular wash.
//   after  — tuned pad: core::music POWER voicing (sub·root·fifth·octave) in
//            just intonation + stereo width + octave shimmer + reverb + slow
//            ambient drift. A drone with clear, consonant harmony.
//
// Run:  node wasm/render_demo.mjs   →   raydrone_before.wav, raydrone_after.wav
'use strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const ex = new WebAssembly.Instance(
    new WebAssembly.Module(readFileSync(join(here, 'raydrone.wasm'))), {}
).exports;
const mem = ex.memory;
const f32 = (ptr, len) => new Float32Array(mem.buffer, ptr, len);
const SR = 44100, BLOCK = 128;

// Hann window (without it the engine is silent).
const WIN = 2048, win = new Float32Array(WIN);
for (let i = 0; i < WIN; i++) win[i] = 0.5 - 0.5 * Math.cos((2 * Math.PI * i) / (WIN - 1));

// Source "scene": a sustained D3 (146.83 Hz) with a few harmonics — the kind of
// single note you'd feed RayDrone. Same source for both renders.
const F0 = 146.83;
function makeSource(secs) {
    const n = SR * secs;
    const s = new Float32Array(n);
    const parts = [[1, 1.0], [2, 0.5], [3, 0.28], [4, 0.16], [5, 0.09]];
    for (let i = 0; i < n; i++) {
        let v = 0;
        for (const [h, a] of parts) v += a * Math.sin((2 * Math.PI * F0 * h * i) / SR);
        s[i] = 0.5 * v / 1.9; // normalize-ish
    }
    return s;
}

function loadSource() {
    const src = makeSource(3);
    const cap = ex.sample_capacity();
    const len = Math.min(src.length, cap);
    f32(ex.sample_ptr(), len).set(src.subarray(0, len));
    f32(ex.window_ptr(), WIN).set(win);
    ex.seed(0x12345678);
    ex.set_sample(len, SR);
}

function render(secs, { fadeIn = 0.15, fadeOut = 0.8 } = {}) {
    const total = Math.floor((SR * secs) / BLOCK) * BLOCK;
    const L = new Float32Array(total), R = new Float32Array(total);
    let w = 0;
    while (w < total) {
        ex.process(BLOCK);
        L.set(f32(ex.out_l_ptr(), BLOCK), w);
        R.set(f32(ex.out_r_ptr(), BLOCK), w);
        w += BLOCK;
    }
    const fi = (SR * fadeIn) | 0, fo = (SR * fadeOut) | 0;
    for (let i = 0; i < total; i++) {
        let g = 1;
        if (i < fi) g *= i / fi;
        if (i > total - fo) g *= (total - i) / fo;
        L[i] *= g; R[i] *= g;
    }
    return { L, R };
}

function writeWav(path, L, R) {
    const n = L.length, bytes = 44 + n * 4; // 16-bit stereo
    const b = Buffer.alloc(bytes);
    b.write('RIFF', 0); b.writeUInt32LE(bytes - 8, 4); b.write('WAVE', 8);
    b.write('fmt ', 12); b.writeUInt32LE(16, 16); b.writeUInt16LE(1, 20);
    b.writeUInt16LE(2, 22); b.writeUInt32LE(SR, 24); b.writeUInt32LE(SR * 4, 28);
    b.writeUInt16LE(4, 32); b.writeUInt16LE(16, 34);
    b.write('data', 36); b.writeUInt32LE(n * 4, 40);
    let o = 44;
    const s16 = (x) => Math.max(-32768, Math.min(32767, (Math.max(-1, Math.min(1, x)) * 32767) | 0));
    for (let i = 0; i < n; i++) { b.writeInt16LE(s16(L[i]), o); b.writeInt16LE(s16(R[i]), o + 2); o += 4; }
    writeFileSync(path, b);
    return n / SR;
}

// ── BEFORE: the current "wash" — continuous pitch, narrow, dry. ──────────────
loadSource();
ex.set_chord(0);          // unison (continuous)
ex.set_mode(1);
ex.set_params(0.9, 0.30, 150, 220, 0.30, 1.0);
ex.set_space(0.0, 0.0);   // no width, no shimmer
ex.set_reverb(0.0);       // dry
ex.set_fx(0.0, 0, 0.5, 0.0);
ex.set_ambient(0, 0, 0, 0, 0, 0);
const before = render(6);
const d1 = writeWav(join(here, '..', 'raydrone_before.wav'), before.L, before.R);

// ── AFTER: tuned pad — POWER voicing + width + shimmer + reverb + drift. ─────
loadSource();
ex.set_chord(2);          // core::music CHORD_POWER (sub·root·fifth·octave·oct+fifth)
ex.set_mode(2);           // stratified → every chord tone is covered evenly
ex.set_params(0.9, 0.22, 160, 260, 0.30, 1.0);
ex.set_space(0.75, 0.18); // stereo width + a touch of octave shimmer
ex.set_reverb(0.42);      // lush tail
ex.set_fx(0.12, 1, 0.45, 0.35); // gentle aberration + 1 bounce + slow autoevolution
ex.set_ambient(1, 3, 2, 0.22, 0.28, 0.35); // slow evolving focus constellation
// Resonant filter with a tempo-synced sweep (70 BPM, 2-bar cycle, 2.5 oct) — the
// classic pad "swell" that opens and closes in time with the track.
ex.set_filter(900, 0.35);
ex.set_filter_lfo(70 / (60 * 8), 2.5);
const after = render(8);
const d2 = writeWav(join(here, '..', 'raydrone_after.wav'), after.L, after.R);

console.log(`raydrone_before.wav  (${d1.toFixed(1)}s) — current wash`);
console.log(`raydrone_after.wav   (${d2.toFixed(1)}s) — tuned POWER pad (core::music)`);
