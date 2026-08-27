// RayDrone on the GPU — its actual idea: cast N rays over the AUDIO BUFFER and
// let the render emerge from their convergence (Monte Carlo in the time domain).
//
// This is *not* the real-time audio thread (that stays on the CPU, per-sample).
// This is the OFFLINE / batch estimator — exactly what the Convergence Lab does
// today on the CPU — where you want to throw a huge number of rays at the buffer
// and average them. That is embarrassingly parallel, has no real-time deadline,
// and so is the right job for the GPU.
//
// The estimate of a rendered grain of length D, focused at sample f0 with a
// dispersion (aperture) of `a` samples, is:
//
//     est[n] = win[n] · (1/N) · Σ_i  s(f0 + k_i + n)        n = 0..D-1
//
// where each ray i lands at offset k_i = round(tri_inv(u_i) · a), u_i drawn by
// the same low-discrepancy schemes as the engine (random / stratified / golden
// QMC). As N grows, est → the analytic target (triangular-weighted average),
// with error ~ 1/√N — the convergence that *is* RayDrone.
//
// Two backends, identical output, so the GPU path is validated by the CPU one:
//   - renderRays_CPU: reference (runs in Node → unit-tested; browser fallback).
//   - renderRays_GPU: a WGSL compute port — one thread per output sample n,
//     each summing the N shared ray offsets. No atomics, exact match to the CPU.

const GOLDEN = 0.6180339887498949;

function rng(seed) {
    let s = (seed >>> 0) || 1;
    return () => { s ^= s << 13; s ^= s >>> 17; s ^= s << 5; s >>>= 0; return s / 4294967296; };
}

// Inverse-CDF of a symmetric triangular distribution on [-1, 1] (the dispersion).
function triInv(u) {
    return u < 0.5 ? -1 + Math.sqrt(2 * u) : 1 - Math.sqrt(2 * (1 - u));
}

/**
 * The N ray landing offsets (in samples), shared by both backends so the GPU
 * result matches the CPU result bit-for-bit. method: 0 random, 1 stratified,
 * 2 golden-ratio QMC.
 */
export function rayOffsets(aperture, rays, method = 2, seed = 1) {
    const rand = rng(seed);
    const off = new Int32Array(rays);
    const rot = rand();
    for (let i = 0; i < rays; i++) {
        let u;
        if (method === 1) u = (i + rand()) / rays;
        else if (method === 2) u = (rot + i * GOLDEN) % 1;
        else u = rand();
        off[i] = Math.round(triInv(u) * aperture);
    }
    return off;
}

function sampleAt(buf, i) {
    return (i >= 0 && i < buf.length) ? buf[i] : 0;
}

/** Hann window of length D. */
export function hann(D) {
    const w = new Float32Array(D);
    for (let i = 0; i < D; i++) w[i] = 0.5 - 0.5 * Math.cos((2 * Math.PI * i) / (D - 1));
    return w;
}

/** CPU reference: cast `rays` rays over `buffer` around f0, return est[D]. */
export function renderRays_CPU(buffer, f0, aperture, rays, D, win, offsets) {
    const off = offsets || rayOffsets(aperture, rays);
    const est = new Float32Array(D);
    for (let i = 0; i < rays; i++) {
        const base = f0 + off[i];
        for (let n = 0; n < D; n++) est[n] += sampleAt(buffer, base + n);
    }
    const inv = 1 / rays;
    for (let n = 0; n < D; n++) est[n] *= inv * win[n];
    return est;
}

/** Analytic target: the triangular-weighted average the rays converge to. */
export function targetCPU(buffer, f0, aperture, D, win) {
    const a = Math.max(1, Math.round(aperture));
    const tgt = new Float32Array(D);
    const invA2 = 1 / (a * a);
    for (let k = -a; k <= a; k++) {
        const p = (a - Math.abs(k)) * invA2;
        if (p <= 0) continue;
        const base = f0 + k;
        for (let n = 0; n < D; n++) tgt[n] += p * sampleAt(buffer, base + n);
    }
    for (let n = 0; n < D; n++) tgt[n] *= win[n];
    return tgt;
}

/** RMS error between an estimate and the target. */
export function rmsError(est, tgt) {
    let s = 0;
    for (let n = 0; n < est.length; n++) { const d = est[n] - tgt[n]; s += d * d; }
    return Math.sqrt(s / est.length);
}

// ── WebGPU backend ──────────────────────────────────────────────────────────
// One thread per output sample n; each sums the N shared ray offsets. The buffer
// and offsets live in storage; no atomics needed (each n is independent).

const WGSL = /* wgsl */`
struct Params { f0: i32, rays: u32, D: u32, bufLen: u32 };
@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var<storage, read> buf: array<f32>;
@group(0) @binding(2) var<storage, read> offs: array<i32>;
@group(0) @binding(3) var<storage, read> win: array<f32>;
@group(0) @binding(4) var<storage, read_write> est: array<f32>;

fn s(i: i32) -> f32 {
  if (i < 0 || i >= i32(P.bufLen)) { return 0.0; }
  return buf[i];
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let n = gid.x;
  if (n >= P.D) { return; }
  var acc = 0.0;
  let ni = i32(n);
  for (var i: u32 = 0u; i < P.rays; i = i + 1u) {
    acc = acc + s(P.f0 + offs[i] + ni);
  }
  est[n] = acc / f32(P.rays) * win[n];
}
`;

/** WebGPU renderer. Same args as the CPU one; returns Float32Array(D). */
export async function renderRays_GPU(device, buffer, f0, aperture, rays, D, win, offsets) {
    const off = offsets || rayOffsets(aperture, rays);

    const mkStorage = (arr, Ctor) => {
        const b = device.createBuffer({ size: arr.byteLength, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST });
        device.queue.writeBuffer(b, 0, arr);
        return b;
    };
    const bufB = mkStorage(buffer instanceof Float32Array ? buffer : Float32Array.from(buffer));
    const offB = mkStorage(off instanceof Int32Array ? off : Int32Array.from(off));
    const winB = mkStorage(win);
    const estB = device.createBuffer({ size: D * 4, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC });

    const params = new ArrayBuffer(16);
    new Int32Array(params)[0] = f0;
    new Uint32Array(params, 4)[0] = rays;
    new Uint32Array(params, 8)[0] = D;
    new Uint32Array(params, 12)[0] = buffer.length;
    const uni = device.createBuffer({ size: 16, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
    device.queue.writeBuffer(uni, 0, params);

    const pipeline = device.createComputePipeline({ layout: 'auto', compute: { module: device.createShaderModule({ code: WGSL }), entryPoint: 'main' } });
    const bind = device.createBindGroup({
        layout: pipeline.getBindGroupLayout(0),
        entries: [
            { binding: 0, resource: { buffer: uni } },
            { binding: 1, resource: { buffer: bufB } },
            { binding: 2, resource: { buffer: offB } },
            { binding: 3, resource: { buffer: winB } },
            { binding: 4, resource: { buffer: estB } },
        ],
    });

    const enc = device.createCommandEncoder();
    const pass = enc.beginComputePass();
    pass.setPipeline(pipeline); pass.setBindGroup(0, bind);
    pass.dispatchWorkgroups(Math.ceil(D / 64));
    pass.end();
    const read = device.createBuffer({ size: D * 4, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
    enc.copyBufferToBuffer(estB, 0, read, 0, D * 4);
    device.queue.submit([enc.finish()]);

    await read.mapAsync(GPUMapMode.READ);
    const out = new Float32Array(read.getMappedRange().slice(0));
    read.unmap();
    return out;
}

/** Pick the GPU if available, else the CPU reference. Returns { est, backend }. */
export async function renderRays(buffer, f0, aperture, rays, D, win, opts = {}) {
    const offsets = rayOffsets(aperture, rays, opts.method ?? 2, opts.seed ?? 1);
    if (typeof navigator !== 'undefined' && navigator.gpu) {
        try {
            const adapter = await navigator.gpu.requestAdapter();
            if (adapter) {
                const device = await adapter.requestDevice();
                return { est: await renderRays_GPU(device, buffer, f0, aperture, rays, D, win, offsets), backend: 'webgpu' };
            }
        } catch (e) { console.warn('WebGPU render failed, CPU fallback:', e); }
    }
    return { est: renderRays_CPU(buffer, f0, aperture, rays, D, win, offsets), backend: 'cpu' };
}
