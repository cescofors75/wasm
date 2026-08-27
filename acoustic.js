// Acoustic ray tracing → room impulse response (IR), for RayDrone.
//
// This is the "use the GPU for sound" done where it actually pays off: OFFLINE.
// We trace many rays from a source bouncing around a shoebox room, accumulate
// the energy that reaches a listener into a time histogram → an impulse
// response. That IR is computed ONCE (no real-time deadline, embarrassingly
// parallel → ideal for the GPU) and then convolved in real time on the CPU with
// a Web Audio `ConvolverNode`. So the GPU bakes the *space*; the audio thread
// stays on the CPU where it belongs.
//
// Two backends, identical IR format ({ L, R, sr }):
//   - traceRoomIR_CPU: a reference implementation. Runs in Node (so the physics
//     is unit-tested) and in the browser as the fallback when WebGPU is absent.
//   - traceRoomIR_GPU: a WebGPU compute port of the same algorithm.
//
// The room is an axis-aligned box [0,Lx]×[0,Ly]×[0,Lz] (metres). Rays leave the
// source in random directions, reflect specularly off the 6 walls losing
// `absorption` of their energy per bounce (+ mild air loss over distance), and
// whenever a ray segment passes within `rxRadius` of the listener it deposits
// energy into the IR bin at time = travelled_distance / speed_of_sound.

const SPEED_OF_SOUND = 343; // m/s

export const DEFAULT_ROOM = {
    room: [8, 5, 3.2],   // Lx, Ly, Lz (m)
    src: [2, 1.5, 1.6],  // source position
    lis: [6, 3.5, 1.6],  // listener position
    rays: 20000,
    maxBounce: 120,
    absorption: 0.18,    // 0..1 wall energy loss per bounce (→ RT60)
    airCoef: 0.0006,     // air absorption per metre
    rxRadius: 0.4,       // listener capture sphere (m)
    sr: 44100,
    irSeconds: 1.6,
    seed: 1,
};

// Small deterministic PRNG so CPU and tests are reproducible.
function rng(state) {
    let s = state >>> 0;
    return () => {
        s ^= s << 13; s ^= s >>> 17; s ^= s << 5; s >>>= 0;
        return s / 4294967296;
    };
}

// A random unit vector (uniform on the sphere).
function randDir(rand) {
    const z = rand() * 2 - 1;
    const t = rand() * 2 * Math.PI;
    const r = Math.sqrt(Math.max(0, 1 - z * z));
    return [r * Math.cos(t), r * Math.sin(t), z];
}

/** Reference CPU tracer. Returns { L, R, sr } impulse response. */
export function traceRoomIR_CPU(opts = {}) {
    const p = { ...DEFAULT_ROOM, ...opts };
    const [Lx, Ly, Lz] = p.room;
    const irLen = Math.max(1, Math.floor(p.sr * p.irSeconds));
    const L = new Float32Array(irLen);
    const R = new Float32Array(irLen);
    const rand = rng(p.seed);
    const c = SPEED_OF_SOUND;
    const r2 = p.rxRadius * p.rxRadius;
    const keep = 1 - p.absorption;

    for (let n = 0; n < p.rays; n++) {
        let px = p.src[0], py = p.src[1], pz = p.src[2];
        let [dx, dy, dz] = randDir(rand);
        let energy = 1.0;
        let dist = 0.0;

        for (let b = 0; b < p.maxBounce; b++) {
            // Distance to the nearest wall along the ray, and which axis it is.
            let tWall = Infinity, axis = 0;
            if (dx > 1e-9) { const t = (Lx - px) / dx; if (t < tWall) { tWall = t; axis = 0; } }
            else if (dx < -1e-9) { const t = (0 - px) / dx; if (t < tWall) { tWall = t; axis = 0; } }
            if (dy > 1e-9) { const t = (Ly - py) / dy; if (t < tWall) { tWall = t; axis = 1; } }
            else if (dy < -1e-9) { const t = (0 - py) / dy; if (t < tWall) { tWall = t; axis = 1; } }
            if (dz > 1e-9) { const t = (Lz - pz) / dz; if (t < tWall) { tWall = t; axis = 2; } }
            else if (dz < -1e-9) { const t = (0 - pz) / dz; if (t < tWall) { tWall = t; axis = 2; } }
            if (!isFinite(tWall) || tWall <= 0) break;

            // Closest approach of this segment to the listener.
            const lx = p.lis[0] - px, ly = p.lis[1] - py, lz = p.lis[2] - pz;
            let s = lx * dx + ly * dy + lz * dz;      // dir is unit → projection = distance
            if (s < 0) s = 0; else if (s > tWall) s = tWall;
            const cxp = px + dx * s, cyp = py + dy * s, czp = pz + dz * s;
            const ex = p.lis[0] - cxp, ey = p.lis[1] - cyp, ez = p.lis[2] - czp;
            const d2 = ex * ex + ey * ey + ez * ez;
            if (d2 <= r2) {
                const hitDist = dist + s;
                const bin = Math.round(hitDist / c * p.sr);
                if (bin >= 0 && bin < irLen) {
                    const air = Math.exp(-p.airCoef * hitDist);
                    const e = energy * air;
                    // Stereo: pan by the lateral component of the arrival direction
                    // (incoming = -ray dir), a cheap binaural-ish split.
                    const lateral = -dx; // listener faces +x; ear axis = x
                    const gl = Math.sqrt(0.5 * (1 - lateral));
                    const gr = Math.sqrt(0.5 * (1 + lateral));
                    L[bin] += e * gl;
                    R[bin] += e * gr;
                }
            }

            // Advance to the wall, lose energy, reflect.
            px += dx * tWall; py += dy * tWall; pz += dz * tWall;
            dist += tWall;
            energy *= keep;
            if (axis === 0) dx = -dx; else if (axis === 1) dy = -dy; else dz = -dz;
            if (energy < 1e-4) break;
            if (dist / c > p.irSeconds) break;
        }
    }
    normalizeIR(L, R, p.rays);
    return { L, R, sr: p.sr };
}

function normalizeIR(L, R, rays) {
    const inv = 1 / Math.max(1, rays / 1000); // scale by ray density
    let peak = 0;
    for (let i = 0; i < L.length; i++) { L[i] *= inv; R[i] *= inv; const m = Math.max(Math.abs(L[i]), Math.abs(R[i])); if (m > peak) peak = m; }
    if (peak > 1e-9) { const g = 0.98 / peak; for (let i = 0; i < L.length; i++) { L[i] *= g; R[i] *= g; } }
}

// ── Browser helpers ─────────────────────────────────────────────────────────

/** Build a stereo AudioBuffer from an IR for a ConvolverNode. */
export function irToAudioBuffer(audioCtx, ir) {
    const buf = audioCtx.createBuffer(2, ir.L.length, ir.sr);
    buf.copyToChannel(ir.L, 0);
    buf.copyToChannel(ir.R, 1);
    return buf;
}

/** Make a ConvolverNode loaded with the IR (the real-time "space" effect). */
export function makeConvolver(audioCtx, ir) {
    const conv = audioCtx.createConvolver();
    conv.normalize = false;
    conv.buffer = irToAudioBuffer(audioCtx, ir);
    return conv;
}

// ── WebGPU backend ──────────────────────────────────────────────────────────
// One compute invocation per ray. Energy is accumulated into the IR with atomic
// adds in fixed-point (WGSL atomics are integer-only), then read back, converted
// to float and normalized — same format as the CPU reference.

const FIXED = 1 << 16; // fixed-point scale for atomic accumulation

const WGSL = /* wgsl */`
struct Params {
  room: vec3<f32>, _p0: f32,
  src:  vec3<f32>, _p1: f32,
  lis:  vec3<f32>, rxRadius: f32,
  rays: u32, maxBounce: u32, irLen: u32, seed: u32,
  sr: f32, absorption: f32, airCoef: f32, _p2: f32,
};
@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var<storage, read_write> accumL: array<atomic<i32>>;
@group(0) @binding(2) var<storage, read_write> accumR: array<atomic<i32>>;

fn xorshift(state: ptr<function, u32>) -> f32 {
  var s = *state;
  s = s ^ (s << 13u); s = s ^ (s >> 17u); s = s ^ (s << 5u);
  *state = s;
  return f32(s) / 4294967296.0;
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let idx = gid.x;
  if (idx >= P.rays) { return; }
  var state: u32 = P.seed + idx * 2654435761u + 1u;

  // random unit direction
  let z = xorshift(&state) * 2.0 - 1.0;
  let th = xorshift(&state) * 6.2831853;
  let rr = sqrt(max(0.0, 1.0 - z * z));
  var dir = vec3<f32>(rr * cos(th), rr * sin(th), z);
  var pos = P.src;

  let c = 343.0;
  let r2 = P.rxRadius * P.rxRadius;
  let keep = 1.0 - P.absorption;
  var energy = 1.0;
  var dist = 0.0;

  for (var b: u32 = 0u; b < P.maxBounce; b = b + 1u) {
    var tWall = 1e30; var axis = 0;
    if (dir.x > 1e-9) { let t = (P.room.x - pos.x) / dir.x; if (t < tWall) { tWall = t; axis = 0; } }
    else if (dir.x < -1e-9) { let t = (0.0 - pos.x) / dir.x; if (t < tWall) { tWall = t; axis = 0; } }
    if (dir.y > 1e-9) { let t = (P.room.y - pos.y) / dir.y; if (t < tWall) { tWall = t; axis = 1; } }
    else if (dir.y < -1e-9) { let t = (0.0 - pos.y) / dir.y; if (t < tWall) { tWall = t; axis = 1; } }
    if (dir.z > 1e-9) { let t = (P.room.z - pos.z) / dir.z; if (t < tWall) { tWall = t; axis = 2; } }
    else if (dir.z < -1e-9) { let t = (0.0 - pos.z) / dir.z; if (t < tWall) { tWall = t; axis = 2; } }
    if (tWall >= 1e30 || tWall <= 0.0) { break; }

    let l = P.lis - pos;
    var s = dot(l, dir);
    s = clamp(s, 0.0, tWall);
    let cp = pos + dir * s;
    let e = P.lis - cp;
    if (dot(e, e) <= r2) {
      let hitDist = dist + s;
      let bin = i32(round(hitDist / c * P.sr));
      if (bin >= 0 && bin < i32(P.irLen)) {
        let air = exp(-P.airCoef * hitDist);
        let amp = energy * air;
        let lateral = -dir.x;
        let gl = sqrt(0.5 * (1.0 - lateral));
        let gr = sqrt(0.5 * (1.0 + lateral));
        atomicAdd(&accumL[bin], i32(amp * gl * ${FIXED}.0));
        atomicAdd(&accumR[bin], i32(amp * gr * ${FIXED}.0));
      }
    }

    pos = pos + dir * tWall;
    dist = dist + tWall;
    energy = energy * keep;
    if (axis == 0) { dir.x = -dir.x; } else if (axis == 1) { dir.y = -dir.y; } else { dir.z = -dir.z; }
    if (energy < 1e-4) { break; }
    if (dist / c > f32(P.irLen) / P.sr) { break; }
  }
}
`;

/** WebGPU tracer. Needs a GPUDevice (navigator.gpu). Returns { L, R, sr }. */
export async function traceRoomIR_GPU(device, opts = {}) {
    const p = { ...DEFAULT_ROOM, ...opts };
    const irLen = Math.max(1, Math.floor(p.sr * p.irSeconds));

    const params = new ArrayBuffer(80);
    const f = new Float32Array(params), u = new Uint32Array(params);
    f[0] = p.room[0]; f[1] = p.room[1]; f[2] = p.room[2];           // room (+pad f[3])
    f[4] = p.src[0]; f[5] = p.src[1]; f[6] = p.src[2];              // src  (+pad f[7])
    f[8] = p.lis[0]; f[9] = p.lis[1]; f[10] = p.lis[2]; f[11] = p.rxRadius;
    u[12] = p.rays; u[13] = p.maxBounce; u[14] = irLen; u[15] = p.seed >>> 0;
    f[16] = p.sr; f[17] = p.absorption; f[18] = p.airCoef;          // (+pad f[19])

    const uniform = device.createBuffer({ size: 80, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
    device.queue.writeBuffer(uniform, 0, params);

    const bytes = irLen * 4;
    const accumL = device.createBuffer({ size: bytes, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC });
    const accumR = device.createBuffer({ size: bytes, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC });

    const module = device.createShaderModule({ code: WGSL });
    const pipeline = device.createComputePipeline({ layout: 'auto', compute: { module, entryPoint: 'main' } });
    const bind = device.createBindGroup({
        layout: pipeline.getBindGroupLayout(0),
        entries: [
            { binding: 0, resource: { buffer: uniform } },
            { binding: 1, resource: { buffer: accumL } },
            { binding: 2, resource: { buffer: accumR } },
        ],
    });

    const enc = device.createCommandEncoder();
    const pass = enc.beginComputePass();
    pass.setPipeline(pipeline);
    pass.setBindGroup(0, bind);
    pass.dispatchWorkgroups(Math.ceil(p.rays / 64));
    pass.end();

    const readL = device.createBuffer({ size: bytes, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
    const readR = device.createBuffer({ size: bytes, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
    enc.copyBufferToBuffer(accumL, 0, readL, 0, bytes);
    enc.copyBufferToBuffer(accumR, 0, readR, 0, bytes);
    device.queue.submit([enc.finish()]);

    await readL.mapAsync(GPUMapMode.READ);
    await readR.mapAsync(GPUMapMode.READ);
    const iL = new Int32Array(readL.getMappedRange().slice(0));
    const iR = new Int32Array(readR.getMappedRange().slice(0));
    readL.unmap(); readR.unmap();

    const Lf = new Float32Array(irLen), Rf = new Float32Array(irLen);
    for (let i = 0; i < irLen; i++) { Lf[i] = iL[i] / FIXED; Rf[i] = iR[i] / FIXED; }
    normalizeIR(Lf, Rf, p.rays);
    return { L: Lf, R: Rf, sr: p.sr };
}

/** Convenience: use the GPU if available, else the CPU reference. */
export async function traceRoomIR(opts = {}) {
    if (typeof navigator !== 'undefined' && navigator.gpu) {
        try {
            const adapter = await navigator.gpu.requestAdapter();
            if (adapter) {
                const device = await adapter.requestDevice();
                return { ir: await traceRoomIR_GPU(device, opts), backend: 'webgpu' };
            }
        } catch (e) { console.warn('WebGPU IR failed, falling back to CPU:', e); }
    }
    return { ir: traceRoomIR_CPU(opts), backend: 'cpu' };
}
