// Reproducible Top-36 render matrix: node perceptual_test.mjs [seconds]
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { performance } from 'node:perf_hooks';

const SR = 48_000, BLOCK = 128, SEED = 0x5eed1234;
const seconds = Math.max(0.25, Number(process.argv[2] || 2));
const abMode = process.argv[3] === 'ab';
const stressMode = process.argv[3] === 'stress';
const materialMode = process.argv[3] === 'material';
const cases = abMode
  ? [0, 0.25, 0.50].map(inertia => ({
      name: `2k-inertia-${String(Math.round(inertia * 100)).padStart(2, '0')}`,
      population: 2000,
      inertia,
    }))
  : stressMode
    ? [
        { name: '2k-stress-reference', population: 2000, inertia: 0 },
        { name: '8k-stress', population: 8000, inertia: 0 },
      ]
  : materialMode
    ? [
        { name: 'material-empty', population: 2000, inertia: 0.25, material: 0 },
        { name: 'material-crystal', population: 2000, inertia: 0.25, material: 3 },
      ]
  : [128, 512, 2000, 8000].map(population => ({ name: String(population), population, inertia: 0 }));
const wasm = readFileSync(new URL('./raydrone.wasm', import.meta.url));

function wav16(left, right) {
  const n = left.length, out = Buffer.allocUnsafe(44 + n * 4);
  out.write('RIFF', 0); out.writeUInt32LE(36 + n * 4, 4); out.write('WAVEfmt ', 8);
  out.writeUInt32LE(16, 16); out.writeUInt16LE(1, 20); out.writeUInt16LE(2, 22);
  out.writeUInt32LE(SR, 24); out.writeUInt32LE(SR * 4, 28); out.writeUInt16LE(4, 32);
  out.writeUInt16LE(16, 34); out.write('data', 36); out.writeUInt32LE(n * 4, 40);
  for (let i = 0, p = 44; i < n; i++, p += 4) {
    out.writeInt16LE(Math.round(Math.max(-1, Math.min(1, left[i])) * 32767), p);
    out.writeInt16LE(Math.round(Math.max(-1, Math.min(1, right[i])) * 32767), p + 2);
  }
  return out;
}

function render(population, inertia, material = 0) {
  const instance = new WebAssembly.Instance(new WebAssembly.Module(wasm), {});
  const ex = instance.exports, mem = ex.memory;
  ex.set_output_sample_rate(SR); ex.seed(SEED); ex.set_ray_population(population); ex.set_ray_update_hz(100);
  ex.set_top36_inertia(inertia);
  ex.set_material(material, 1);
  const sampleLen = SR * 4;
  const sample = new Float32Array(mem.buffer, ex.sample_ptr(), sampleLen);
  for (let i = 0; i < sampleLen; i++) sample[i] = Math.sin(i * 2 * Math.PI * 110 / SR) * (0.65 + 0.25 * Math.sin(i * 2 * Math.PI * 0.37 / SR));
  ex.set_sample(sampleLen, SR);
  const win = new Float32Array(mem.buffer, ex.window_ptr(), ex.window_capacity());
  for (let i = 0; i < win.length; i++) win[i] = 0.5 - 0.5 * Math.cos(2 * Math.PI * i / (win.length - 1));
  ex.set_params(1.5, 0.8, 2000, 100000, 0.045, 1.0);
  ex.set_mode(1); ex.set_space(0.8, 0); ex.set_fx(0.15, 0, 0, 0);
  const frames = Math.floor(seconds * SR), left = new Float32Array(frames), right = new Float32Array(frames);
  const lp = ex.out_l_ptr(), rp = ex.out_r_ptr();
  let maxMs = 0, sumMs = 0, ticks = 0;
  const blockTimes = [];
  for (let off = 0; off < frames; off += BLOCK) {
    const n = Math.min(BLOCK, frames - off), t0 = performance.now(); ex.process(n);
    const dt = performance.now() - t0; sumMs += dt; maxMs = Math.max(maxMs, dt); ticks++; blockTimes.push(dt);
    left.set(new Float32Array(mem.buffer, lp, n), off); right.set(new Float32Array(mem.buffer, rp, n), off);
  }
  let sumSq = 0, peak = 0;
  let maxSampleJump = 0;
  for (let i = 0; i < frames; i++) {
    sumSq += (left[i] * left[i] + right[i] * right[i]) * 0.5;
    peak = Math.max(peak, Math.abs(left[i]), Math.abs(right[i]));
    if (i) maxSampleJump = Math.max(maxSampleJump, Math.abs(left[i] - left[i - 1]), Math.abs(right[i] - right[i - 1]));
  }
  const rms = Math.sqrt(sumSq / frames);
  const env = [], envWindow = Math.max(1, Math.round(SR * 0.005));
  for (let off = 0; off < frames; off += envWindow) {
    let e = 0, count = Math.min(envWindow, frames - off);
    for (let i = off; i < off + count; i++) e += (left[i] * left[i] + right[i] * right[i]) * 0.5;
    env.push(Math.sqrt(e / count));
  }
  const envMean = env.reduce((a, x) => a + x, 0) / env.length;
  const envelopeCv = Math.sqrt(env.reduce((a, x) => a + (x - envMean) ** 2, 0) / env.length) / Math.max(envMean, 1e-9);
  const dropoutWindowPct = env.filter(x => x < envMean * 0.1).length / env.length * 100;
  if (!Number.isFinite(rms) || rms < 1e-4) throw new Error(`silent render at population ${population}: rms=${rms}`);
  if (dropoutWindowPct > 5) throw new Error(`discontinuous render at population ${population}: ${dropoutWindowPct.toFixed(2)}% dropout windows`);
  blockTimes.sort((a, b) => a - b);
  const percentile = p => blockTimes[Math.min(blockTimes.length - 1, Math.ceil(blockTimes.length * p) - 1)] || 0;
  return {
    left, right, rms, peak, envelopeCv, dropoutWindowPct, maxSampleJump,
    meanMs: sumMs / ticks, maxMs, p95Ms: percentile(0.95), p99Ms: percentile(0.99),
    simulated: ex.simulated_rays(), audible: ex.active_voices(),
    retention: ex.top36_mean_retention(), entries: ex.top36_entries(),
    exits: ex.top36_exits(), transitions: ex.top36_transitions(),
    pitchMean: ex.top36_pitch_mean(), pitchMedian: ex.top36_pitch_median(),
    ageMeanS: ex.top36_age_mean_s(), energyMean: ex.top36_energy_mean(),
    effectiveDurationMeanS: ex.top36_effective_duration_mean_s()
  };
}

mkdirSync(new URL('./renders/', import.meta.url), { recursive: true });
const rows = ['case,population,inertia,mean_audio_block_ms,p95_audio_block_ms,p99_audio_block_ms,max_audio_block_ms,ray_update_hz,simulated_rays,audible_voices,top36_retention,top36_entries,top36_exits,top36_transitions,entries_per_transition,exits_per_transition,top_pitch_mean,top_pitch_median,top_age_mean_s,top_energy_mean,top_effective_duration_mean_s,rms,peak,envelope_cv,dropout_window_pct,max_sample_jump'];
for (const testCase of cases) {
  const { name, population, inertia, material = 0 } = testCase;
  const r = render(population, inertia, material);
  writeFileSync(new URL(`./renders/top36-${name}.wav`, import.meta.url), wav16(r.left, r.right));
  const entriesPerTransition = r.transitions ? r.entries / r.transitions : 0;
  const exitsPerTransition = r.transitions ? r.exits / r.transitions : 0;
  rows.push([name, population, inertia, r.meanMs, r.p95Ms, r.p99Ms, r.maxMs, 100, r.simulated, r.audible, r.retention, r.entries, r.exits, r.transitions, entriesPerTransition, exitsPerTransition, r.pitchMean, r.pitchMedian, r.ageMeanS, r.energyMean, r.effectiveDurationMeanS, r.rms, r.peak, r.envelopeCv, r.dropoutWindowPct, r.maxSampleJump].join(','));
  console.log(`${name}: retention ${(r.retention * 100).toFixed(2)}%, rms ${r.rms.toFixed(4)}, dropouts ${r.dropoutWindowPct.toFixed(2)}%, block ms mean/p95/p99 ${r.meanMs.toFixed(3)}/${r.p95Ms.toFixed(3)}/${r.p99Ms.toFixed(3)}`);
}
const profileName = abMode ? './renders/profile-inertia.csv' : stressMode ? './renders/profile-stress.csv' : materialMode ? './renders/profile-material.csv' : './renders/profile.csv';
writeFileSync(new URL(profileName, import.meta.url), rows.join('\n') + '\n');
