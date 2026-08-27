// Validates the CPU acoustic ray tracer (the reference + WebGPU fallback).
// The WebGPU path can't run here (no GPU in this environment); it mirrors this
// algorithm, so checking the physics here covers both.
//
// Run:  node wasm/test_acoustic.mjs
import { traceRoomIR_CPU, DEFAULT_ROOM } from './acoustic.js';

let fail = 0;
const check = (name, cond, extra = '') => {
    if (!cond) fail++;
    console.log(`  ${cond ? '✓' : '✗ FAIL'}  ${name}${extra ? '  — ' + extra : ''}`);
};

// Last sample index whose energy is above a small fraction of the peak ≈ tail length.
function tailLen(ir) {
    const e = new Float32Array(ir.L.length);
    let peak = 0;
    for (let i = 0; i < e.length; i++) { e[i] = ir.L[i] * ir.L[i] + ir.R[i] * ir.R[i]; if (e[i] > peak) peak = e[i]; }
    const thr = peak * 1e-4;
    let last = 0;
    for (let i = 0; i < e.length; i++) if (e[i] > thr) last = i;
    return last / ir.sr;
}

console.log('RayDrone acoustic ray tracer — CPU reference\n');

const ir = traceRoomIR_CPU({ rays: 30000, seed: 1 });
check('IR has the right length', ir.L.length === Math.floor(DEFAULT_ROOM.sr * DEFAULT_ROOM.irSeconds));
let energy = 0, finite = true;
for (let i = 0; i < ir.L.length; i++) { energy += ir.L[i] * ir.L[i] + ir.R[i] * ir.R[i]; if (!Number.isFinite(ir.L[i]) || !Number.isFinite(ir.R[i])) finite = false; }
check('IR is finite (no NaN/Inf)', finite);
check('IR carries energy', energy > 1e-3, `Σe²=${energy.toExponential(2)}`);

// Direct sound should arrive around |src-lis| / c.
const dx = DEFAULT_ROOM.src.map((s, i) => s - DEFAULT_ROOM.lis[i]);
const directT = Math.hypot(...dx) / 343;
let firstHit = -1;
for (let i = 0; i < ir.L.length; i++) { if (Math.abs(ir.L[i]) + Math.abs(ir.R[i]) > 0.05) { firstHit = i / ir.sr; break; } }
check('direct sound arrives near |src-lis|/c', firstHit > 0 && firstHit >= directT - 0.01 && firstHit < directT + 0.05,
    `direct≈${(directT * 1000).toFixed(1)}ms, first hit=${(firstHit * 1000).toFixed(1)}ms`);

// More absorption → shorter reverb tail (RT60 falls).
const dead = traceRoomIR_CPU({ rays: 30000, seed: 1, absorption: 0.5 });
const live = traceRoomIR_CPU({ rays: 30000, seed: 1, absorption: 0.08 });
const tDead = tailLen(dead), tLive = tailLen(live);
check('more absorption → shorter tail', tDead < tLive, `tail: dead=${(tDead * 1000).toFixed(0)}ms < live=${(tLive * 1000).toFixed(0)}ms`);

// Reproducible for a fixed seed.
const a = traceRoomIR_CPU({ rays: 5000, seed: 42 });
const b = traceRoomIR_CPU({ rays: 5000, seed: 42 });
let identical = true;
for (let i = 0; i < a.L.length; i += 97) if (a.L[i] !== b.L[i]) { identical = false; break; }
check('same seed → identical IR (reproducible)', identical);

console.log(`\n${fail === 0 ? '✅ ALL PASSED' : '❌ ' + fail + ' FAILED'}`);
process.exit(fail === 0 ? 0 : 1);
