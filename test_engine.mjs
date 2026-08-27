// Headless functional test for the RayDrone WASM engine (the "web rust" build).
// Drives raydrone.wasm exactly like wasm/processor.js does — load a sample, set
// the Hann window + params, run process() — and checks it renders real, finite,
// non-silent stereo audio while exercising reverb / ambient / scale / smart paths.
//
// Run:  node wasm/test_engine.mjs
'use strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const bytes = readFileSync(join(here, 'raydrone.wasm'));
const mod = new WebAssembly.Module(bytes);
const inst = new WebAssembly.Instance(mod, {});
const ex = inst.exports;
const mem = ex.memory;
const f32 = (ptr, len) => new Float32Array(mem.buffer, ptr, len);

const SR = 48000;
const BLOCK = 128;
let failures = 0;
const check = (name, cond, extra = '') => {
    const ok = !!cond;
    if (!ok) failures++;
    console.log(`  ${ok ? '✓' : '✗ FAIL'}  ${name}${extra ? '  — ' + extra : ''}`);
};

// 1) Hann window (2048) — without it win_at() returns 0 and the engine is silent.
const WIN = 2048;
check('exports the fixed window capacity', ex.window_capacity() === WIN, `${ex.window_capacity()} samples`);
check('exports a block capacity >= AudioWorklet quantum', ex.block_capacity() >= BLOCK, `${ex.block_capacity()} samples`);
check('exports configurable ray population', typeof ex.set_ray_population === 'function' && typeof ex.simulated_rays === 'function');
check('uses 2K as the default musical population', ex.ray_population() === 2000, `${ex.ray_population()} rays`);
check('uses 25% as the default musical inertia', Math.abs(ex.top36_inertia() - 0.25) < 1e-5, `${ex.top36_inertia()}`);
ex.set_ray_population(8000);
check('accepts the 8K research population', ex.ray_population() === 8000, `${ex.ray_population()} rays`);
ex.set_ray_population(512);
ex.set_top36_inertia(0.35);
check('accepts bounded Top-36 inertia', Math.abs(ex.top36_inertia() - 0.35) < 1e-5, `${ex.top36_inertia()}`);
ex.set_top36_inertia(0);
const win = new Float32Array(WIN);
for (let i = 0; i < WIN; i++) win[i] = 0.5 - 0.5 * Math.cos((2 * Math.PI * i) / (WIN - 1));
f32(ex.window_ptr(), WIN).set(win);

// 2) A test "scene": 2 s of a 220 Hz sine (the rays will granulate this).
const cap = ex.sample_capacity();
const len = Math.min(SR * 2, cap);
const sample = f32(ex.sample_ptr(), len);
for (let i = 0; i < len; i++) sample[i] = 0.6 * Math.sin((2 * Math.PI * 220 * i) / SR);
ex.seed(0x9e3779b9);
ex.set_output_sample_rate(SR);
ex.set_sample(len, SR);

// 3) Params: a continuous drone. set_params(focus_s, aperture_s, grain_ms, grain_rate, gain, master)
ex.set_mode(1); // golden-ratio QMC
ex.set_params(1.0, 0.25, 150, 220, 0.3, 1.0);
ex.set_space(0.6, 0.0); // stereo width, no octave shimmer
ex.set_reverb(0.0); // dry first

// Helper: run N blocks, return peak/RMS over the whole run + final-block stats.
function run(blocks) {
    let peak = 0, sumSq = 0, n = 0, nonFinite = 0, lastBlockEnergy = 0;
    for (let b = 0; b < blocks; b++) {
        ex.process(BLOCK);
        const L = f32(ex.out_l_ptr(), BLOCK);
        const R = f32(ex.out_r_ptr(), BLOCK);
        let be = 0;
        for (let i = 0; i < BLOCK; i++) {
            const l = L[i], r = R[i];
            if (!Number.isFinite(l) || !Number.isFinite(r)) nonFinite++;
            const a = Math.abs(l), c = Math.abs(r);
            if (a > peak) peak = a;
            if (c > peak) peak = c;
            sumSq += l * l + r * r;
            be += a + c;
            n += 2;
        }
        lastBlockEnergy = be / (2 * BLOCK);
    }
    return { peak, rms: Math.sqrt(sumSq / n), nonFinite, lastBlockEnergy };
}

console.log('RayDrone WASM engine — headless functional test\n');

console.log('[dry drone]');
const dry = run(200); // ~0.53 s of audio
check('output is finite (no NaN/Inf)', dry.nonFinite === 0, `${dry.nonFinite} bad samples`);
check('output is audible (rms > 1e-4)', dry.rms > 1e-4, `rms=${dry.rms.toExponential(2)}`);
check('output within soft-clip bounds (peak <= 0.7)', dry.peak <= 0.7 + 1e-6, `peak=${dry.peak.toFixed(4)}`);
check('rays are spawning', (ex.spawn_count() >>> 0) > 0, `${ex.spawn_count() >>> 0} grains`);
check('voices are active', (ex.active_voices() >>> 0) > 0, `${ex.active_voices() >>> 0} voices`);
check('granular output is hard-limited to Top-36', (ex.active_voices() >>> 0) <= 36, `${ex.active_voices() >>> 0} voices`);
check('simulation population is reported separately', (ex.simulated_rays() >>> 0) >= (ex.active_voices() >>> 0), `${ex.simulated_rays()} simulated / ${ex.active_voices()} audible`);
check('simulation fills its configured population independently of audible voices', (ex.simulated_rays() >>> 0) === (ex.ray_population() >>> 0), `${ex.simulated_rays()} / ${ex.ray_population()} rays`);
check('Top-36 retention is finite and bounded', Number.isFinite(ex.top36_mean_retention()) && ex.top36_mean_retention() >= 0 && ex.top36_mean_retention() <= 1, `${(ex.top36_mean_retention() * 100).toFixed(2)}%`);
check('Top-36 entry/exit counters are exported', typeof ex.top36_entries === 'function' && typeof ex.top36_exits === 'function');
check('Top-36 physical descriptors are finite', [ex.top36_pitch_mean(), ex.top36_pitch_median(), ex.top36_age_mean_s(), ex.top36_energy_mean(), ex.top36_effective_duration_mean_s()].every(Number.isFinite));

console.log('\n[reverb tail]');
ex.set_reverb(0.6);
run(100); // let the tail build
ex.set_params(1.0, 0.25, 150, 0, 0.3, 1.0); // grain_rate=0 → stop spawning new rays
run(60); // let existing grains die out; only the reverb tail should remain
const tail = run(40);
check('reverb leaves a decaying tail after rays stop', tail.rms > 1e-5 && tail.rms < dry.rms, `tail rms=${tail.rms.toExponential(2)}`);
check('tail stays finite', tail.nonFinite === 0);

console.log('\n[microtonal scale]');
// A just-intonation-ish set of ratios within an octave.
const ratios = Float32Array.from([1.0, 9 / 8, 5 / 4, 4 / 3, 3 / 2, 5 / 3, 15 / 8]);
const sc = Math.min(ratios.length, ex.scale_capacity());
f32(ex.scale_ptr(), sc).set(ratios.subarray(0, sc));
ex.set_scale(sc);
ex.set_params(1.0, 0.25, 150, 220, 0.3, 1.0);
const scaled = run(120);
check('renders cleanly with a microtonal scale', scaled.nonFinite === 0 && scaled.rms > 1e-4, `rms=${scaled.rms.toExponential(2)}`);

console.log('\n[held keys — computer-keyboard/mouse piano, chords]');
// Held keys must render cleanly on their own...
const chord = Float32Array.from([1.0, 5 / 4, 3 / 2]); // C major triad ratios
const kc = Math.min(chord.length, ex.keys_capacity());
f32(ex.keys_ptr(), kc).set(chord.subarray(0, kc));
ex.set_keys(kc);
ex.set_params(1.0, 0.25, 150, 220, 0.3, 1.0);
const withKeysHeld = run(120);
check('renders cleanly with keys held', withKeysHeld.nonFinite === 0 && withKeysHeld.rms > 1e-4, `rms=${withKeysHeld.rms.toExponential(2)}`);
ex.set_scale(0); // back to continuous pitch
ex.set_keys(0); // release all keys

// ...and must take priority over an active microtonal scale. A true A/B needs
// two fresh, identically-seeded instances (this file's shared `ex` has already
// spawned grains and consumed RNG draws by this point, so reseeding it alone
// wouldn't isolate the one thing that should differ: whether keys are held).
function freshInstance() {
    const i2 = new WebAssembly.Instance(mod, {});
    const e2 = i2.exports;
    const m2 = e2.memory;
    const g32 = (ptr, n) => new Float32Array(m2.buffer, ptr, n);
    e2.set_output_sample_rate(SR);
    g32(e2.window_ptr(), WIN).set(win);
    const s2 = g32(e2.sample_ptr(), len);
    for (let i = 0; i < len; i++) s2[i] = 0.6 * Math.sin((2 * Math.PI * 220 * i) / SR);
    e2.set_sample(len, SR);
    e2.set_mode(1);
    e2.set_params(1.0, 0.25, 150, 220, 0.3, 1.0);
    const sc2 = g32(e2.scale_ptr(), sc);
    sc2.set(ratios.subarray(0, sc));
    e2.set_scale(sc);
    return { ex: e2, f32: g32 };
}

function rmsFrom(instance, blocks) {
    let sumSq = 0, count = 0;
    for (let block = 0; block < blocks; block++) {
        instance.ex.process(BLOCK);
        const left = instance.f32(instance.ex.out_l_ptr(), BLOCK);
        const right = instance.f32(instance.ex.out_r_ptr(), BLOCK);
        for (let frame = 0; frame < BLOCK; frame++) {
            sumSq += left[frame] * left[frame] + right[frame] * right[frame];
            count += 2;
        }
    }
    return Math.sqrt(sumSq / count);
}

console.log('\n[source endpoint]');
const endpoint = freshInstance();
const endpointSource = endpoint.f32(endpoint.ex.sample_ptr(), len);
endpointSource.fill(0);
const tailStart = len - 12_000;
for (let frame = tailStart; frame < len - 2; frame++) {
    endpointSource[frame] = 0.6 * Math.sin((2 * Math.PI * 220 * (frame - tailStart)) / SR);
}
endpoint.ex.set_sample(len, SR);
endpoint.ex.set_params((len - 2) / SR, 0, 150, 220, 0.3, 1.0);
const endpointRms = rmsFrom(endpoint, 30);
check('focus at the source endpoint renders the final audible region', endpointRms > 1e-4, `rms=${endpointRms.toExponential(2)}`);

const a = freshInstance();
a.ex.seed(0x9e37_79b9);
let sumA = 0, nA = 0;
for (let b = 0; b < 120; b++) {
    a.ex.process(BLOCK);
    const L = a.f32(a.ex.out_l_ptr(), BLOCK), R = a.f32(a.ex.out_r_ptr(), BLOCK);
    for (let i = 0; i < BLOCK; i++) { sumA += L[i] + R[i]; nA++; }
}

const b = freshInstance();
const kc2 = Math.min(chord.length, b.ex.keys_capacity());
b.f32(b.ex.keys_ptr(), kc2).set(chord.subarray(0, kc2));
b.ex.set_keys(kc2); // only difference from `a`: keys are held
b.ex.seed(0x9e37_79b9);
let sumB = 0, nB = 0;
for (let bl = 0; bl < 120; bl++) {
    b.ex.process(BLOCK);
    const L = b.f32(b.ex.out_l_ptr(), BLOCK), R = b.f32(b.ex.out_r_ptr(), BLOCK);
    for (let i = 0; i < BLOCK; i++) { sumB += L[i] + R[i]; nB++; }
}
check(
    'held keys override the microtonal scale (bit-for-bit different render)',
    sumA !== sumB,
    `scale-only sum=${sumA.toExponential(3)} vs keys-held sum=${sumB.toExponential(3)}`
);

console.log('\n[ambient constellation + smart rays + bounces]');
ex.set_ambient(1, 3, 2, 0.25, 0.3, 0.4);
ex.set_smart(1);
ex.set_fx(0.3, 2, 0.5, 0.4); // aberration, 2 bounces, reflect, feedback
const amb = run(300);
check('ambient/smart/bounces render finite audio', amb.nonFinite === 0, `${amb.nonFinite} bad`);
check('ambient is audible', amb.rms > 1e-4, `rms=${amb.rms.toExponential(2)}`);
let fociAlive = 0;
const fcap = ex.foci_cap();
const fw = f32(ex.foci_w_ptr(), fcap);
for (let i = 0; i < fcap; i++) if (fw[i] > 0.004) fociAlive++;
check('focus constellation is populated', fociAlive > 0, `${fociAlive} foci alive`);

console.log('\n[resonant filter — set_filter through the real process()]');
ex.set_ambient(0, 0, 0, 0, 0, 0);
ex.set_smart(0);
ex.set_fx(0, 0, 0.5, 0);
ex.set_scale(0);
ex.set_space(0, 0);
ex.set_reverb(0);
ex.set_filter_lfo(0, 0);
ex.set_material(0, 0);
ex.set_modulation(0, 0, 0.25, 0, 0.05, 0.5);
ex.set_effects(0, 0.38, 0.35, 0, 0.25, 0.008);
function rmsOut(blocks) {
    let sum = 0, n = 0;
    for (let b = 0; b < blocks; b++) {
        ex.process(BLOCK);
        const L = f32(ex.out_l_ptr(), BLOCK), R = f32(ex.out_r_ptr(), BLOCK);
        for (let i = 0; i < BLOCK; i++) { sum += L[i] * L[i] + R[i] * R[i]; n += 2; }
    }
    return Math.sqrt(sum / n);
}
ex.set_params(1.0, 0.25, 150, 260, 0.3, 1.0);
ex.set_filter(22050, 0); // open
const rmsOpen = rmsOut(120);
ex.set_filter(90, 0.1); // cutoff well below the 220 Hz source → should gut it
const rmsLow = rmsOut(120);
check('low cutoff attenuates the signal (filter is in the path)', rmsLow < rmsOpen * 0.5, `rms open=${rmsOpen.toExponential(2)} → low=${rmsLow.toExponential(2)}`);
ex.set_filter(22050, 0); // reopen

console.log('\n[materials, modulation and spatial effects]');
ex.set_material(3, 0.85); // crystal
ex.set_modulation(1, 1, 0.35, 0.4, 0.02, 0.4); // LFO → aperture
ex.set_effects(0.2, 0.11, 0.3, 0.18, 0.25, 0.008);
ex.set_advanced_effects(0.35, 0.18, 0.003, 0.3, 0.22, 0.6, 0.35, 0.28, 330, 0.55);
ex.set_reverb(0.2);
const designed = run(180);
check('material/modulation/effects chain stays finite', designed.nonFinite === 0, `${designed.nonFinite} bad samples`);
check('material/modulation/effects chain remains audible', designed.rms > 1e-4, `rms=${designed.rms.toExponential(2)}`);

console.log('\n[original ray direct — same WASM acoustic path]');
ex.set_direct(1, 0.25);
ex.set_material(1, 0.8); // metal
ex.set_modulation(1, 0, 0.6, 0.35, 0.02, 0.3);
ex.set_effects(0.18, 0.12, 0.25, 0.12, 0.3, 0.008);
ex.set_reverb(0.22);
const directRay = run(160);
check('exports and renders Original as a direct ray through WASM', directRay.nonFinite === 0 && directRay.rms > 1e-4, `rms=${directRay.rms.toExponential(2)}`);
ex.set_direct(0, 0);

// No basta con que la cadena no se caiga: este A/B obliga a que material y
// efectos alteren muestras reales con la misma fuente, semilla y parámetros.
function renderSignature(material, effects, reverb) {
    const q = freshInstance();
    q.ex.set_reverb(reverb);
    q.ex.set_material(material, 1);
    q.ex.set_modulation(1, 1, 0.42, 0.65, 0.02, 0.3);
    q.ex.set_effects(...effects);
    q.ex.seed(0x7f4a_7c15);
    const out = [];
    for (let b = 0; b < 520; b++) { // > delay más largo de la prueba
        q.ex.process(BLOCK);
        const L = q.f32(q.ex.out_l_ptr(), BLOCK);
        for (let i = 0; i < BLOCK; i++) out.push(L[i]);
    }
    return out;
}
const plainSig = renderSignature(0, [0, 0.12, 0, 0, 0.25, 0.008], 0);
const shapedSig = renderSignature(3, [0.45, 0.12, 0.32, 0.35, 0.3, 0.012], 0.45);
let meanDifference = 0;
for (let i = 0; i < plainSig.length; i++) meanDifference += Math.abs(plainSig[i] - shapedSig[i]);
meanDifference /= plainSig.length;
check('material + movement + FX audibly alter the rendered signal', meanDifference > 1e-3, `mean abs difference=${meanDifference.toExponential(2)}`);

// Regresión: Delay compartía memoria con Chorus/Flanger, pero su feedback se
// aplicaba incluso con Delay wet=0. Eso coloreaba Chorus y podía auto-resonar.
function renderChorusWithHiddenDelayFeedback(feedback) {
    const q = freshInstance();
    q.ex.set_direct(1, 0);
    q.ex.set_effects(0, 0.12, feedback, 0.35, 0.3, 0.008);
    const out = [];
    for (let b = 0; b < 180; b++) {
        q.ex.process(BLOCK);
        const L = q.f32(q.ex.out_l_ptr(), BLOCK);
        for (let i = 0; i < BLOCK; i++) out.push(L[i]);
    }
    return out;
}
const chorusNoFeedback = renderChorusWithHiddenDelayFeedback(0);
const chorusMaxFeedback = renderChorusWithHiddenDelayFeedback(0.68);
let hiddenFeedbackDifference = 0;
for (let i = 0; i < chorusNoFeedback.length; i++) hiddenFeedbackDifference += Math.abs(chorusNoFeedback[i] - chorusMaxFeedback[i]);
hiddenFeedbackDifference /= chorusNoFeedback.length;
check('Delay feedback is inaudible when Delay wet is zero', hiddenFeedbackDifference < 1e-8, `mean abs difference=${hiddenFeedbackDifference.toExponential(2)}`);

console.log('\n[convergence lab — the offline estimator]');
// The same estimator, measured offline: more rays → lower error vs the target.
f32(ex.lab_win_ptr(), ex.lab_grain()).fill(1.0); // flat window for the measurement
ex.lab_target(50000, 4000);
ex.lab_estimate(50000, 4000, 64, 2, 1);
const errLow = ex.lab_rms();
ex.lab_estimate(50000, 4000, 4096, 2, 1);
const errHigh = ex.lab_rms();
check('estimator converges (more rays → less error)', errHigh < errLow, `err: ${errLow.toExponential(2)} (64) → ${errHigh.toExponential(2)} (4096)`);

// Regression: the importance estimator (method 3) used to NaN at large N — the
// f32 stratified u could round to 1.0 (or land past the f32 cumulative-sum CDF
// maximum), the binary search then returned the aperture's edge bin where the
// triangular kernel is exactly 0, and PM/QM = 0/0 = NaN poisoned the whole
// estimate. Must stay finite for any seed at the largest N the paper needs.
ex.lab_imp_build(50000, 4000);
let impFiniteOk = true;
let impErr = 0;
for (const sd of [1, 0x9e3779b9, 0xdeadbeef]) {
    ex.lab_estimate(50000, 4000, 65536, 3, sd);
    const e = ex.lab_rms();
    if (!Number.isFinite(e)) impFiniteOk = false;
    impErr = e;
}
check('importance stays finite at N=65536 (edge-bin 0/0 regression)', impFiniteOk, `err=${impErr.toExponential(2)}`);

console.log('\n[ABI guards]');
ex.process(0);
check('empty process block preserves a finite meter', Number.isFinite(ex.out_level()), `level=${ex.out_level()}`);
ex.set_params(1, 0.25, 150, Infinity, 0.3, 1);
ex.process(BLOCK);
check('non-finite grain rate is rejected without hanging', Number.isFinite(ex.out_level()));
ex.set_pitch(Infinity);
ex.set_params(1, 0.25, 150, 220, 0.3, 1);
const guarded = run(4);
check('non-finite pitch falls back to finite audio', guarded.nonFinite === 0);
f32(ex.scale_ptr(), 2).set([NaN, Infinity]);
f32(ex.keys_ptr(), 2).set([-Infinity, 0]);
ex.set_scale(2);
ex.set_keys(2);
ex.set_reverb(NaN);
ex.set_filter(NaN, Infinity);
ex.set_filter_lfo(Infinity, NaN);
ex.set_material(99, NaN);
ex.set_modulation(99, 99, Infinity, NaN, Infinity, NaN);
ex.set_effects(NaN, Infinity, NaN, Infinity, NaN, Infinity);
ex.set_advanced_effects(NaN, Infinity, NaN, Infinity, NaN, Infinity, NaN, Infinity, NaN, Infinity);
ex.set_fx(NaN, 99, Infinity, NaN);
ex.set_space(NaN, Infinity);
ex.set_ambient(1, 3, 2, NaN, Infinity, NaN);
const allGuarded = run(8);
check('all floating-point ABI setters reject non-finite values', allGuarded.nonFinite === 0);

console.log(`\n${failures === 0 ? '✅ ALL PASSED' : '❌ ' + failures + ' CHECK(S) FAILED'}`);
process.exit(failures === 0 ? 0 : 1);
