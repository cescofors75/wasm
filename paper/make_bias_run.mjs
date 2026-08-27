#!/usr/bin/env node
// Genera paper/data/run5-bias-ap91-foc2.0.csv — el run que evidencia el SESGO
// del trazado inverso (paper §5.3 / Fig. 4), usando el motor Rust→wasm actual
// del Convergence Lab.
//
// Los runs 1–4 son pilotos históricos de una implementación anterior. Este run,
// reproducible con el motor actual, construye el caso donde el sesgo
// DOMINA: una fuente sintética con energía fuertemente no uniforme dentro de
// la apertura (mitad tono fuerte, mitad casi-silencio, foco en la frontera).
// Aquí el muestreo exacto q ∝ energía sin reponderar estima un objetivo distinto, y su
// curva de error se APLANA mientras random sigue cayendo ~1/√N e importance
// (reponderado, insesgado) cae mucho más rápido.
//
// Reproducir:   bash wasm/build.sh && node paper/make_bias_run.mjs
// Determinista: fuente sintética + semillas fijas por (método, N, tirada).
'use strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const wasmPath = join(here, '..', 'raydrone.wasm');
const outPath = join(here, 'data', 'run5-bias-ap91-foc2.0.csv');

const bytes = readFileSync(wasmPath);
const mod = new WebAssembly.Module(bytes);
const inst = new WebAssembly.Instance(mod, {});
const ex = inst.exports;
const mem = ex.memory;
const f32 = (ptr, len) => new Float32Array(mem.buffer, ptr, len);

const SR = 44100;

// Fuente: 4 s. Primeros 2 s = tono 220 Hz a 0.8; últimos 2 s = el mismo tono
// a 0.001 (casi silencio). Foco exactamente en la frontera → la apertura
// cubre mitad fuerte / mitad silenciosa: máxima asimetría de energía.
const len = SR * 4;
const s = f32(ex.sample_ptr(), len);
for (let i = 0; i < len; i++) {
    const amp = i < SR * 2 ? 0.8 : 0.001;
    s[i] = amp * Math.sin((2 * Math.PI * 220 * i) / SR);
}
ex.set_sample(len, SR);
ex.set_output_sample_rate(SR);

// Ventana Hann del grano del Lab (mismo setup que lab-worker.js).
const D = ex.lab_grain();
const win = f32(ex.lab_win_ptr(), D);
for (let n = 0; n < D; n++) win[n] = 0.5 * (1 - Math.cos((2 * Math.PI * n) / (D - 1)));

const F0 = SR * 2; // foco en la frontera fuerte/silencio (2.000 s)
const A = 4000;    // ≈ 90.7 ms de apertura
ex.lab_target(F0, A);
ex.lab_imp_build(F0, A);

// Mismo orden de columnas que los runs 1–4 (export CSV del Lab).
const METHODS = [
    ['random', 0],
    ['stratified', 1],
    ['qmc', 2],
    ['importance', 3],
    ['reverse', 4],
];
// Hasta 32768 rayos: el codo/meseta de reverse aparece claramente por encima
// de ~4096, fuera del rango de los runs 1–4 — por eso allí no se veía.
const NS = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768];
const TRIALS = 8;
const SEED = 0x9e3779b9;

function meanRms(methodId, N) {
    let acc = 0;
    for (let t = 0; t < TRIALS; t++) {
        // Semilla determinista por (método, N, tirada), como lab-worker.js.
        const sd = ((SEED ^ Math.imul(N, 2654435761) ^ (methodId << 26)) + Math.imul(t + 1, 1013904223)) >>> 0;
        ex.lab_estimate(F0, A, N, methodId, sd);
        acc += ex.lab_rms();
    }
    return acc / TRIALS;
}

function fitSlope(Ns, ys) {
    // Ajuste por mínimos cuadrados de log10(y) vs log10(N).
    const pts = Ns.map((N, i) => [Math.log10(N), Math.log10(ys[i])]).filter(([, y]) => Number.isFinite(y));
    const n = pts.length;
    const sx = pts.reduce((a, [x]) => a + x, 0);
    const sy = pts.reduce((a, [, y]) => a + y, 0);
    const sxx = pts.reduce((a, [x]) => a + x * x, 0);
    const sxy = pts.reduce((a, [x, y]) => a + x * y, 0);
    return (n * sxy - sx * sy) / (n * sxx - sx * sx);
}

const curves = {};
for (const [name, id] of METHODS) {
    curves[name] = NS.map((N) => meanRms(id, N));
    console.log(`${name.padEnd(11)} listo (último punto: ${curves[name].at(-1).toExponential(3)})`);
}

const slopes = METHODS.map(([name]) => `${name}=${fitSlope(NS, curves[name]).toFixed(4)}`).join(', ');
const apMs = ((2 * A + 1) / SR * 1000 / 2).toFixed(1); // semiancho en ms, como los runs 1-4

const lines = [
    '# RayDrone Convergence Lab',
    `# aperture_ms=${apMs}, grain=${D}, trials=${TRIALS}, focus_s=${(F0 / SR).toFixed(3)}, sr=${SR}`,
    '# source: synthetic tone|near-silence boundary (bias demo, paper §5.3 / Fig. 4) — node paper/make_bias_run.mjs',
    `# slopes: ${slopes}`,
    'N,' + METHODS.map(([n]) => n).join(','),
    ...NS.map((N, i) => `${N},` + METHODS.map(([n]) => curves[n][i].toExponential(6)).join(',')),
    '',
];
writeFileSync(outPath, lines.join('\n'));
console.log(`\nEscrito ${outPath}`);
console.log(`Pendientes: ${slopes}`);
