// Validates the buffer ray-tracer (RayDrone's own estimator) on the CPU
// reference. The WebGPU path mirrors this exactly (one thread per output sample,
// same shared ray offsets), so the physics check here covers both; the GPU
// itself can't run in this environment.
//
// Run:  node wasm/test_raytrace_buffer.mjs
import { renderRays_CPU, targetCPU, rmsError, rayOffsets, hann } from './raytrace_buffer.js';

let fail = 0;
const check = (name, cond, extra = '') => { if (!cond) fail++; console.log(`  ${cond ? '✓' : '✗ FAIL'}  ${name}${extra ? '  — ' + extra : ''}`); };

// A test buffer: 1 s of a 220 Hz tone with a couple of harmonics @48k.
const SR = 48000, F0 = 220;
const buffer = new Float32Array(SR);
for (let i = 0; i < SR; i++) buffer[i] = 0.6 * Math.sin(2 * Math.PI * F0 * i / SR) + 0.2 * Math.sin(2 * Math.PI * 2 * F0 * i / SR);

const D = 1024, aperture = 1500, focus = 12000;
const win = hann(D);
const tgt = targetCPU(buffer, focus, aperture, D, win);

console.log('RayDrone buffer ray-tracer — CPU reference (the estimator)\n');

// 1) Convergence: more rays → less error (the 1/√N law that *is* RayDrone).
const errs = [16, 64, 256, 1024, 4096].map((N) => {
    const est = renderRays_CPU(buffer, focus, aperture, N, D, win, rayOffsets(aperture, N, 2, 7));
    return { N, e: rmsError(est, tgt) };
});
errs.forEach(({ N, e }) => console.log(`     N=${String(N).padStart(5)}  err=${e.toExponential(3)}`));
check('error decreases monotonically with N', errs.every((x, i) => i === 0 || x.e < errs[i - 1].e));
check('high-N estimate is close to the analytic target', errs[errs.length - 1].e < errs[0].e / 5,
    `${errs[0].e.toExponential(2)} → ${errs[errs.length - 1].e.toExponential(2)}`);

// 2) golden QMC converges faster than plain random at equal N (low-discrepancy).
const N = 256;
const eQMC = rmsError(renderRays_CPU(buffer, focus, aperture, N, D, win, rayOffsets(aperture, N, 2, 3)), tgt);
const eRnd = rmsError(renderRays_CPU(buffer, focus, aperture, N, D, win, rayOffsets(aperture, N, 0, 3)), tgt);
check('golden QMC beats random at equal N', eQMC < eRnd, `QMC=${eQMC.toExponential(2)} < random=${eRnd.toExponential(2)}`);

// 3) Determinism + finiteness.
const a = renderRays_CPU(buffer, focus, aperture, 512, D, win, rayOffsets(aperture, 512, 2, 99));
const b = renderRays_CPU(buffer, focus, aperture, 512, D, win, rayOffsets(aperture, 512, 2, 99));
let same = true, finite = true;
for (let i = 0; i < D; i++) { if (a[i] !== b[i]) same = false; if (!Number.isFinite(a[i])) finite = false; }
check('same seed → identical render (reproducible)', same);
check('render is finite', finite);

console.log(`\n${fail === 0 ? '✅ ALL PASSED' : '❌ ' + fail + ' FAILED'}`);
process.exit(fail === 0 ? 0 : 1);
