// Worker del Convergence Lab: instancia su PROPIA copia del motor (raydrone.wasm)
// y mide la convergencia llamando al mismo código Rust que suena en vivo.
// Semillas deterministas por (método, N, tirada) → curvas reproducibles bit a bit.
// Corre fuera del hilo principal: la UI no se congela.

onmessage = (e) => {
    const d = e.data;
    if (d.type !== 'run') return;
    try {
        const mod = new WebAssembly.Module(d.wasmBytes);
        const inst = new WebAssembly.Instance(mod, {});
        const ex = inst.exports;
        const mem = ex.memory;

        // Sample (truncado a la capacidad del motor, como en el worklet)
        const cap = ex.sample_capacity();
        const len = Math.min(d.sample.length, cap);
        new Float32Array(mem.buffer, ex.sample_ptr(), len).set(d.sample.subarray(0, len));
        ex.set_sample(len, d.sampleRate);

        // Ventana Hann del grano del lab (el wasm no tiene cos en no_std)
        const D = ex.lab_grain();
        const win = new Float32Array(mem.buffer, ex.lab_win_ptr(), D);
        for (let n = 0; n < D; n++) win[n] = 0.5 * (1 - Math.cos(2 * Math.PI * n / (D - 1)));

        const A = Math.min(d.A, ex.lab_max_a());
        ex.lab_target(d.f0, A);
        ex.lab_imp_build(d.f0, A);

        const curves = {};
        let done = 0;
        const total = d.methods.length * d.Ns.length;
        for (const m of d.methods) {
            const ys = [];
            for (const N of d.Ns) {
                let acc = 0;
                for (let t = 0; t < d.trials; t++) {
                    // semilla determinista por (método, N, tirada)
                    const sd = ((d.seed ^ Math.imul(N, 2654435761) ^ (m.id << 26)) + Math.imul(t + 1, 1013904223)) >>> 0;
                    ex.lab_estimate(d.f0, A, N, m.id, sd);
                    acc += ex.lab_rms();
                }
                ys.push(acc / d.trials);
                postMessage({ type: 'progress', done: ++done, total });
            }
            curves[m.k] = ys;
        }

        // Objetivo en f32 para el A/B audible en la página
        const target = new Float32Array(D);
        target.set(new Float32Array(mem.buffer, ex.lab_target_ptr(), D));
        postMessage({ type: 'done', curves, target }, [target.buffer]);
    } catch (err) {
        postMessage({ type: 'error', message: String((err && err.message) || err) });
    }
};
