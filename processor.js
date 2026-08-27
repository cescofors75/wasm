// AudioWorkletProcessor que ejecuta el motor RayDrone (Rust→wasm). Estéreo.
// Incluye diagnóstico: CPU del hilo de audio, voces (rayos) activas y granos/seg.

class RayDroneProcessor extends AudioWorkletProcessor {
    constructor(options) {
        super();
        // Compilar aquí dentro: Safari no permite clonar un WebAssembly.Module
        // del hilo principal al worklet, así que llegan los bytes crudos.
        const mod = new WebAssembly.Module(options.processorOptions.wasmBytes);
        this.inst = new WebAssembly.Instance(mod, {});
        this.ex = this.inst.exports;
        this.mem = this.ex.memory;
        this.outL = this.ex.out_l_ptr();
        this.outR = this.ex.out_r_ptr();
        this.blockCapacity = this.ex.block_capacity();
        this.outViewBuffer = null;
        this.outViewCount = 0;
        this.outLView = null;
        this.outRView = null;
        this.ex.set_output_sample_rate(sampleRate);
        this.ready = false;
        this.ex.seed(0x9e3779b9);
        this.lastW = 0;
        this.rayOff = [];
        this.rayBand = [];
        this.rayRatio = [];
        this.foci = [];
        this.blockCount = 0;
        // Diagnóstico de rendimiento
        this.now = (typeof performance !== 'undefined' && performance.now) ? () => performance.now() : null;
        this.cpuAcc = 0;
        this.cpuBlocks = 0;
        this.lastSpawn = 0;
        this.spawnRateSmooth = 0;
        // Grabación de la salida del motor (export WAV); tope de seguridad 5 min.
        // Dos buffers reutilizables evitan slice()/arrays por quantum en el hilo de audio.
        this.recOn = false;
        this.recCapacity = Math.ceil(sampleRate);
        this.recL = new Float32Array(this.recCapacity);
        this.recR = new Float32Array(this.recCapacity);
        this.recWrite = 0;
        this.recFrames = 0;
        this.port.onmessage = (e) => this.onMsg(e.data);
    }

    onMsg(d) {
        try {
            this.handleMsg(d);
        } catch (err) {
            // Lo típico: raydrone.wasm está desactualizado respecto al JS (p.ej.
            // tras un `git pull` sin recompilar — el .wasm NO está en git, hay
            // que correr wasm/build.sh) y falta un export nuevo. Sin este
            // try/catch, la excepción moría en silencio aquí dentro del
            // worklet y el control simplemente "no hacía nada".
            this.port.postMessage({
                type: 'enginemismatch',
                messageType: d.type,
                error: String((err && err.message) || err),
            });
        }
    }

    handleMsg(d) {
        const ex = this.ex;
        if (d.type === 'sample') {
            const cap = ex.sample_capacity();
            const len = Math.min(d.data.length, cap);
            new Float32Array(this.mem.buffer, ex.sample_ptr(), len).set(d.data.subarray(0, len));
            ex.set_sample(len, d.sampleRate);
            this.ready = true;
            // Avisar a la UI si el sample no cabe entero (truncado silencioso, no más)
            this.port.postMessage({ type: 'sampleinfo', used: len, total: d.data.length, truncated: d.data.length > cap });
        } else if (d.type === 'record') {
            if (d.on) {
                this.recOn = true;
                this.recWrite = 0;
                this.recFrames = 0;
            } else if (this.recOn) {
                this.recOn = false;
                this.flushRec(true, false);
            }
        } else if (d.type === 'window') {
            const cap = ex.window_capacity();
            const len = Math.min(d.data.length, cap);
            new Float32Array(this.mem.buffer, ex.window_ptr(), len).set(d.data.subarray(0, len));
            if (d.data.length !== cap) {
                this.port.postMessage({ type: 'windowinfo', used: len, total: d.data.length, expected: cap });
            }
        } else if (d.type === 'params') {
            ex.set_params(d.focus, d.aperture, d.grainMs, d.grainRate, d.gain, d.master);
        } else if (d.type === 'direct') {
            ex.set_direct(d.on >>> 0, d.offsetSec);
        } else if (d.type === 'mode') {
            ex.set_mode(d.value >>> 0);
        } else if (d.type === 'fx') {
            ex.set_fx(d.aber, d.bounces >>> 0, d.refl, d.feedback);
        } else if (d.type === 'space') {
            ex.set_space(d.width, d.oct);
        } else if (d.type === 'pitch') {
            ex.set_pitch(d.mult);
        } else if (d.type === 'scale') {
            // Tabla de ratios microtonales (vacía = pitch continuo)
            const len = Math.min(d.data.length, ex.scale_capacity());
            if (len) new Float32Array(this.mem.buffer, ex.scale_ptr(), len).set(d.data.subarray(0, len));
            ex.set_scale(len);
        } else if (d.type === 'keys') {
            // Ratios de las notas sostenidas (piano por teclado/ratón/táctil);
            // vacía = ninguna tecla pulsada, se cae a Microtonal/Voicing o pitch continuo.
            const len = Math.min(d.data.length, ex.keys_capacity());
            if (len) new Float32Array(this.mem.buffer, ex.keys_ptr(), len).set(d.data.subarray(0, len));
            ex.set_keys(len);
        } else if (d.type === 'chord') {
            // Voicing afinado de core::music (0 = unísono/continuo).
            ex.set_chord(d.preset >>> 0);
        } else if (d.type === 'filter') {
            ex.set_filter(d.cutoff, d.res);
        } else if (d.type === 'filterlfo') {
            ex.set_filter_lfo(d.rate, d.depth);
        } else if (d.type === 'reverb') {
            ex.set_reverb(d.wet);
        } else if (d.type === 'material') {
            ex.set_material(d.kind >>> 0, d.amount);
        } else if (d.type === 'modulation') {
            ex.set_modulation(d.mode >>> 0, d.target >>> 0, d.rate, d.depth, d.attack, d.release);
        } else if (d.type === 'effects') {
            ex.set_effects(d.delayWet, d.delayTime, d.delayFeedback, d.chorusWet, d.chorusRate, d.chorusDepth);
        } else if (d.type === 'advancedfx') {
            ex.set_advanced_effects(d.flangerWet, d.flangerRate, d.flangerDepth, d.phaserWet, d.phaserRate, d.phaserDepth, d.drive, d.resonatorWet, d.resonatorHz, d.resonatorDecay);
        } else if (d.type === 'smart') {
            ex.set_smart(d.on >>> 0);
        } else if (d.type === 'ambient') {
            ex.set_ambient(d.on >>> 0, d.seeds >>> 0, d.depth >>> 0, d.spread, d.drift, d.rate);
            this.ambOn = (d.on >>> 0) === 1;
        } else if (d.type === 'population') {
            ex.set_ray_population(d.value >>> 0);
        } else if (d.type === 'top36inertia') {
            ex.set_top36_inertia(d.value);
        }
    }

    // Copiar una vez por segundo y transferir el chunk; no asigna por quantum.
    flushRec(done = false, limited = false) {
        if (this.recWrite > 0) {
            const L = this.recL.slice(0, this.recWrite);
            const R = this.recR.slice(0, this.recWrite);
            this.port.postMessage({ type: 'recdata', l: L, r: R, sr: sampleRate, done, limited }, [L.buffer, R.buffer]);
        } else if (done) {
            this.port.postMessage({ type: 'recdata', l: new Float32Array(0), r: new Float32Array(0), sr: sampleRate, done, limited });
        }
        this.recWrite = 0;
    }

    recordBlock(left, right) {
        let sourceOffset = 0;
        while (sourceOffset < left.length) {
            const count = Math.min(left.length - sourceOffset, this.recCapacity - this.recWrite);
            this.recL.set(left.subarray(sourceOffset, sourceOffset + count), this.recWrite);
            this.recR.set(right.subarray(sourceOffset, sourceOffset + count), this.recWrite);
            this.recWrite += count;
            this.recFrames += count;
            sourceOffset += count;
            if (this.recWrite === this.recCapacity) this.flushRec(false, false);
        }
    }

    process(inputs, outputs) {
        const out = outputs[0];
        const frames = out[0].length;
        if (this.ready) {
            // Cronometrar el render del motor (carga real del hilo de audio).
            const t0 = this.now ? this.now() : 0;
            for (let offset = 0; offset < frames; offset += this.blockCapacity) {
                const count = Math.min(this.blockCapacity, frames - offset);
                this.ex.process(count);
                // La memoria WASM es estable en ejecución normal. Reutilizar
                // las vistas elimina dos objetos por quantum (~750/seg a 48 kHz).
                if (this.outViewBuffer !== this.mem.buffer || this.outViewCount !== count) {
                    this.outViewBuffer = this.mem.buffer;
                    this.outViewCount = count;
                    this.outLView = new Float32Array(this.mem.buffer, this.outL, count);
                    this.outRView = new Float32Array(this.mem.buffer, this.outR, count);
                }
                out[0].set(this.outLView, offset);
                if (out[1]) out[1].set(this.outRView, offset);
            }
            if (this.now) { this.cpuAcc += this.now() - t0; this.cpuBlocks++; }

            if (this.recOn) {
                this.recordBlock(out[0], out[1] || out[0]);
                if (this.recFrames >= sampleRate * 300) { // tope 5 min
                    this.recOn = false;
                    this.flushRec(true, true);
                }
            }

            // Recoger rayos (offset + banda) para la visualización.
            const w = this.ex.slog_w() >>> 0;
            if (w !== this.lastW) {
                const cap = this.ex.slog_cap();
                const off = new Float32Array(this.mem.buffer, this.ex.slog_ptr(), cap);
                const bnd = new Float32Array(this.mem.buffer, this.ex.slog_b_ptr(), cap);
                const rat = new Float32Array(this.mem.buffer, this.ex.slog_s_ptr(), cap);
                let count = (w - this.lastW) >>> 0;
                if (count > cap) count = cap;
                for (let k = 0; k < count; k++) {
                    const idx = (this.lastW + k) % cap;
                    this.rayOff.push(off[idx]);
                    this.rayBand.push(bnd[idx]);
                    this.rayRatio.push(rat[idx]);
                }
                this.lastW = w;
            }

            if (++this.blockCount >= 16) {
                const blocks = this.blockCount;
                this.blockCount = 0;
                const level = this.ex.out_level();
                // ── Diagnóstico ──
                const blockMs = frames / sampleRate * 1000;
                const cpu = (this.now && this.cpuBlocks > 0) ? (this.cpuAcc / this.cpuBlocks) / blockMs * 100 : -1;
                const voices = this.ex.active_voices();
                const population = this.ex.ray_population();
                const simulatedRays = this.ex.simulated_rays();
                const sc = this.ex.spawn_count() >>> 0;
                const spawnsDelta = (sc - this.lastSpawn) >>> 0;
                this.lastSpawn = sc;
                const spawnRateInstant = spawnsDelta / (blocks * frames / sampleRate);
                this.spawnRateSmooth += (spawnRateInstant - this.spawnRateSmooth) * 0.2;
                const spawnsPerSec = this.spawnRateSmooth;
                this.cpuAcc = 0; this.cpuBlocks = 0;
                const perf = {
                    cpu, voices, simulatedRays, population, spawnsPerSec,
                    top36Retention: this.ex.top36_retention(),
                    top36MeanRetention: this.ex.top36_mean_retention(),
                    top36Entries: this.ex.top36_entries() >>> 0,
                    top36Exits: this.ex.top36_exits() >>> 0,
                    top36Transitions: this.ex.top36_transitions() >>> 0,
                    top36Inertia: this.ex.top36_inertia(),
                    top36PitchMean: this.ex.top36_pitch_mean(),
                    top36PitchMedian: this.ex.top36_pitch_median(),
                    top36AgeMeanS: this.ex.top36_age_mean_s(),
                    top36EnergyMean: this.ex.top36_energy_mean(),
                    top36EffectiveDurationMeanS: this.ex.top36_effective_duration_mean_s(),
                };

                // Constelación de focos (solo en ambient): posición + peso de cada foco vivo.
                let foci = null;
                if (this.ambOn) {
                    const cap = this.ex.foci_cap();
                    const fp = new Float32Array(this.mem.buffer, this.ex.foci_ptr(), cap);
                    const fw = new Float32Array(this.mem.buffer, this.ex.foci_w_ptr(), cap);
                    foci = this.foci;
                    foci.length = 0;
                    for (let i = 0; i < cap; i++) if (fw[i] > 0.004) foci.push(fp[i], fw[i]);
                }

                if (this.rayOff.length) {
                    this.port.postMessage({ type: 'rays', offsets: this.rayOff, bands: this.rayBand, ratios: this.rayRatio, foci, level, perf });
                    // structured clone ya ha capturado el mensaje al volver de
                    // postMessage: conservar la capacidad evita tres arrays y
                    // su posterior GC unas 20 veces por segundo en audio real.
                    this.rayOff.length = 0;
                    this.rayBand.length = 0;
                    this.rayRatio.length = 0;
                } else {
                    this.port.postMessage({ type: 'level', foci, level, perf });
                }
            }
        } else {
            out[0].fill(0);
            if (out[1]) out[1].fill(0);
        }
        return true;
    }
}

registerProcessor('raydrone', RayDroneProcessor);
