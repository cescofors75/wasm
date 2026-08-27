// RayDrone — simplified granular engine (per-instance, std).
//
// A direct, slimmed-down port of `wasm/raydrone.rs`. The core idea is intact:
// each grain is a "ray" cast at a random offset inside the dispersion cone
// around the focus, and the drone *emerges* from the convergence of N rays.
//
// What the simplified VERSION keeps:
//   - Continuous grain cloud, per-sample mixing (no voice cap clicks, no jitter).
//   - Low-discrepancy sampling (golden-ratio quasi-Monte Carlo) for smooth render.
//   - Triangular dispersion around the focus (the "depth of field").
//   - Catmull-Rom interpolation, micro-detune, equal-power stereo spread.
//   - Freeverb-lite stereo reverb + DC blocker + soft clip.
//
// What it drops vs. the WASM engine (to stay "simple"): chromatic aberration,
// Russian-roulette bounces, recursive autoevolution, ambient foci, microtonal
// scales, inverse tracing and the Convergence Lab.

use std::f32::consts::PI;

// Shared DSP kernel — the same primitives the WASM engine links (one source of
// truth). `tri_inv` stays local here because the VST uses the hardware sqrt while
// the WASM build uses a no_std Newton approximation; unifying them would change
// the render.
use raydrone_core::{
    clampf, music::semitone_ratio, sample_at, soft, sqrtf, tri_inv, win_at, DcBlocker, Reverb,
};

#[inline]
fn sample_at_wrapped(sample: &[f32], pos: f32) -> f32 {
    let len = sample.len();
    if len < 4 || !pos.is_finite() {
        return 0.0;
    }
    let floor = pos.floor();
    let base = floor as usize % len;
    let frac = pos - floor;
    let s0 = sample[(base + len - 1) % len];
    let s1 = sample[base];
    let s2 = sample[(base + 1) % len];
    let s3 = sample[(base + 2) % len];
    let a = -0.5 * s0 + 1.5 * s1 - 1.5 * s2 + 0.5 * s3;
    let b = s0 - 2.5 * s1 + 2.0 * s2 - 0.5 * s3;
    let c = -0.5 * s0 + 0.5 * s2;
    ((a * frac + b) * frac + c) * frac + s1
}

const WIN: usize = 2048;
const MAX_VOICES: usize = 512;

const GOLDEN: f32 = 0.618_034;
const STRATA: u32 = 17;

// Ring buffer of recent ray landing positions (normalized 0..1) for the visualizer.
pub const VLOG: usize = 128;

// Ring buffer of recent output samples (post-mix, post-reverb — what actually
// plays), for the GUI's oscilloscope/spectrum view. Power of two: `spectrum()`
// FFTs it directly. Same size the WASM demo's AnalyserNode uses (fftSize).
pub const OUT_RING: usize = 1024;

// Per-grain micro-detune (±0.25%): beats between grains → lush, non-static drone.
const DETUNE: f32 = 0.005;
// Fixed grain shape / level / spread for the simplified build.
const GRAIN_DUR: f32 = 0.15; // seconds
const GRAIN_GAIN: f32 = 0.3;
const WIDTH: f32 = 0.7;

#[derive(Clone, Copy)]
struct Voice {
    pos: f32,
    age: f32,
    inv_dur: f32,
    gain: f32,
    step: f32,
    panl: f32,
    panr: f32,
    depth: u32, // remaining recursive bounces (Russian roulette)
}

impl Default for Voice {
    fn default() -> Self {
        Voice {
            pos: 0.0,
            age: 0.0,
            inv_dur: 0.0,
            gain: 0.0,
            step: 1.0,
            panl: 0.707,
            panr: 0.707,
            depth: 0,
        }
    }
}

pub struct Engine {
    // Source ("the scene") and its native rate.
    sample: Vec<f32>,
    samp_sr: f32,
    // Host output rate (may differ from samp_sr; we keep pitch correct).
    host_sr: f32,
    window: Vec<f32>,

    // Params (set from the plugin each block).
    focus01: f32,     // 0..1 position in the sample
    aperture_ms: f32, // dispersion width
    grain_rate: f32,  // grains per second (density / "N rays")
    master: f32,      // output gain (linear)
    feedback: f32,    // autoevolution amount (recursive feedback)
    octave: f32,      // probability a grain plays an octave up (shimmer)
    bounces: u32,     // recursive ray bounces (Russian-roulette depth)
    refl: f32,        // reflection coefficient: probability each bounce survives
    mode: u32,        // 0 random, 1 QMC (golden), 2 stratified

    // Recursive autoevolution: the output envelope feeds back into focus/aperture,
    // and a slow self-sweep (EVO) makes the render drift on its own.
    env: f32,
    evo: f32,
    evo_dir: f32,

    // Visualization state (read by the GUI thread via the plugin).
    vlog_pos: Vec<f32>,
    vlog_w: usize,
    viz_focus: f32, // effective focus, normalized 0..1
    viz_ap: f32,    // effective aperture as a fraction of the scene span, 0..1
    // Mono mix of the actual output, one ring buffer entry per `tick()`. Cheap
    // (fixed-size array write, no allocation) — the audio thread fills it; the
    // GUI thread reorders + FFTs a snapshot for the oscilloscope/spectrum view.
    out_ring: [f32; OUT_RING],
    out_w: usize,

    // Runtime.
    spawn_acc: f32,
    rng: u32,
    qmc: f32,
    strat_i: u32,
    // Live capture: when on, `sample` is a rolling buffer fed by the host input
    // (the plugin works as an ambient effect on a track instead of an instrument).
    live: bool,
    cap_w: usize,
    // Held-note pitch ratios (MIDI / on-screen piano). Empty = unison drone;
    // otherwise each grain picks one → play the drone as chords.
    key_ratios: Vec<f32>,
    voices: Vec<Voice>,
    active: Vec<usize>,
    free: Vec<usize>,
    nactive: usize,
    nfree: usize,

    // Reverb + DC blocker, both from the shared kernel (same code the WASM engine
    // links). Boxed so the reverb's fixed delay-line buffers live on the heap
    // (like the old Vecs) instead of inline in the plugin struct.
    reverb: Box<Reverb>,
    dc: DcBlocker,
}

impl Engine {
    pub fn new(host_sr: f32) -> Self {
        let host_sr = if host_sr > 1.0 { host_sr } else { 44100.0 };
        let window: Vec<f32> = (0..WIN)
            .map(|i| {
                let x = (i as f32) / ((WIN - 1) as f32);
                0.5 - 0.5 * (2.0 * PI * x).cos() // Hann
            })
            .collect();

        let mut e = Engine {
            sample: Vec::new(),
            samp_sr: 44100.0,
            host_sr,
            window,
            focus01: 0.3,
            aperture_ms: 100.0,
            grain_rate: 200.0,
            master: 0.5,
            feedback: 0.3,
            octave: 0.0,
            bounces: 0,
            refl: 0.5,
            mode: 1,
            env: 0.0,
            evo: 0.5,
            evo_dir: 1.0,
            vlog_pos: vec![0.5; VLOG],
            vlog_w: 0,
            viz_focus: 0.3,
            viz_ap: 0.1,
            out_ring: [0.0; OUT_RING],
            out_w: 0,
            spawn_acc: 0.0,
            rng: 0x1234_5678,
            qmc: 0.5,
            strat_i: 0,
            live: false,
            cap_w: 0,
            key_ratios: Vec::with_capacity(128),
            voices: vec![Voice::default(); MAX_VOICES],
            active: vec![0usize; MAX_VOICES],
            free: (0..MAX_VOICES).map(|i| MAX_VOICES - 1 - i).collect(),
            nactive: 0,
            nfree: MAX_VOICES,
            reverb: Box::new(Reverb::new()),
            dc: DcBlocker::new(),
        };
        e.reverb.set_wet(0.2); // VST default wet mix
        e.update_coeffs();
        e
    }

    pub fn is_empty(&self) -> bool {
        self.sample.len() < 2
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.host_sr = if sr > 1.0 { sr } else { 44100.0 };
        self.update_coeffs();
    }

    /// Replace the scene. Returns the buffer it replaced instead of dropping it
    /// here: this can run on the audio thread (a scene change handed over from
    /// the GUI), and freeing a multi-megabyte `Vec` there is as real-time-unsafe
    /// as allocating one. The caller is responsible for dropping the returned
    /// buffer somewhere that isn't the audio callback.
    #[must_use]
    pub fn load(&mut self, sample: Vec<f32>, sr: f32) -> Vec<f32> {
        self.live = false;
        let old = std::mem::replace(&mut self.sample, sample);
        self.samp_sr = if sr > 1.0 { sr } else { 44100.0 };
        self.reset_runtime(false);
        old
    }

    /// Switch to live-input mode: `sample` becomes a rolling buffer fed by
    /// `push_input`, which the rays render into an ambient cloud. Takes an
    /// already-allocated, zeroed buffer (`buf`) — sized for the desired capture
    /// duration at `sr` — instead of a duration, so this never allocates: building
    /// the buffer is the caller's job, same real-time-safety reasoning as `load`.
    /// Returns the previous sample buffer for the caller to free off-thread.
    #[must_use]
    pub fn begin_live_capture(&mut self, buf: Vec<f32>, sr: f32) -> Vec<f32> {
        let old = std::mem::replace(&mut self.sample, buf);
        self.samp_sr = if sr > 1.0 { sr } else { 44100.0 };
        self.cap_w = 0;
        self.live = true;
        self.reset_runtime(false);
        old
    }

    /// Feed one input sample into the rolling capture buffer (no-op unless live).
    #[inline]
    pub fn push_input(&mut self, x: f32) {
        if self.live && !self.sample.is_empty() {
            self.sample[self.cap_w] = x;
            self.cap_w += 1;
            if self.cap_w >= self.sample.len() {
                self.cap_w = 0;
            }
        }
    }

    pub fn reset(&mut self) {
        self.reset_runtime(true);
    }

    fn reset_runtime(&mut self, clear_effects: bool) {
        self.nactive = 0;
        self.nfree = MAX_VOICES;
        for i in 0..MAX_VOICES {
            self.free[i] = MAX_VOICES - 1 - i;
        }
        self.spawn_acc = 0.0;
        self.env = 0.0;
        self.evo = 0.5;
        self.evo_dir = 1.0;
        for v in self.vlog_pos.iter_mut() {
            *v = 0.5;
        }
        self.vlog_w = 0;
        self.out_ring = [0.0; OUT_RING];
        self.out_w = 0;
        self.dc.reset();
        if clear_effects {
            self.reverb.reset();
        }
    }

    // ── Setters from the plugin ─────────────────────────────────────────────
    pub fn set_density(&mut self, rays_per_s: f32) {
        self.grain_rate = rays_per_s.max(0.0);
    }
    pub fn set_aperture_ms(&mut self, ms: f32) {
        self.aperture_ms = ms.max(0.0);
    }
    pub fn set_focus(&mut self, f01: f32) {
        self.focus01 = clampf(f01, 0.0, 1.0);
    }
    pub fn set_reverb(&mut self, wet: f32) {
        self.reverb.set_wet(wet);
    }
    pub fn set_master(&mut self, gain: f32) {
        self.master = gain.max(0.0);
    }
    pub fn set_feedback(&mut self, amount: f32) {
        self.feedback = clampf(amount, 0.0, 1.0);
    }
    pub fn set_octave(&mut self, p: f32) {
        self.octave = clampf(p, 0.0, 1.0);
    }
    pub fn set_bounce(&mut self, n: u32) {
        self.bounces = n.min(6);
    }
    pub fn set_reflect(&mut self, r: f32) {
        self.refl = clampf(r, 0.0, 1.0);
    }

    /// Set the currently held notes (128-entry mask). `root` is the note that
    /// plays the scene at its natural pitch (unison). Empty mask = unison drone.
    pub fn set_keys(&mut self, mask: &[bool; 128], root: u8) {
        self.key_ratios.clear();
        for (n, &on) in mask.iter().enumerate() {
            if on {
                let semis = n as i32 - root as i32;
                self.key_ratios.push(semitone_ratio(semis));
            }
        }
    }

    // ── Visualization getters (read by the GUI thread) ──────────────────────
    pub fn viz_level(&self) -> f32 {
        self.env
    }
    pub fn viz_focus(&self) -> f32 {
        self.viz_focus
    }
    pub fn viz_aperture(&self) -> f32 {
        self.viz_ap
    }
    pub fn ray_buffer(&self) -> &[f32] {
        &self.vlog_pos
    }
    pub fn ray_write(&self) -> usize {
        self.vlog_w
    }
    /// Ring buffer of the actual output (mono mix, post-reverb/soft-clip), most
    /// recent sample at index `out_write() - 1` (mod the buffer length).
    pub fn out_buffer(&self) -> &[f32] {
        &self.out_ring
    }
    pub fn out_write(&self) -> usize {
        self.out_w
    }

    fn update_coeffs(&mut self) {
        self.dc.set_sample_rate(self.host_sr);
        // Scale the reverb delay lines to the host SR (keeps the tail time) and
        // clear them — mirrors the old "rebuild the buffers" behaviour.
        self.reverb.set_sample_rate(self.host_sr);
        self.reverb.reset();
    }

    #[inline]
    fn rng01(&mut self) -> f32 {
        raydrone_core::rng01(&mut self.rng)
    }

    #[inline]
    fn next_u(&mut self) -> f32 {
        match self.mode {
            1 => {
                self.qmc += GOLDEN;
                if self.qmc >= 1.0 {
                    self.qmc -= 1.0;
                }
                self.qmc
            }
            2 => {
                let u = (self.strat_i as f32 + self.rng01()) / (STRATA as f32);
                self.strat_i = (self.strat_i + 1) % STRATA;
                u
            }
            _ => self.rng01(),
        }
    }

    fn alloc_voice(&mut self) -> usize {
        if self.nfree > 0 {
            self.nfree -= 1;
            let slot = self.free[self.nfree];
            self.active[self.nactive] = slot;
            self.nactive += 1;
            return slot;
        }
        // No free slots: steal the most faded voice (phase closest to 1 → the
        // Hann tail is near 0) so the theft is click-free.
        let mut best = 0usize;
        let mut best_ph = -1.0f32;
        for k in 0..self.nactive {
            let v = self.active[k];
            let ph = self.voices[v].age * self.voices[v].inv_dur;
            if ph > best_ph {
                best_ph = ph;
                best = k;
            }
        }
        self.active[best]
    }

    fn place(&mut self, pos: f32, depth: u32) {
        let dur_samp = GRAIN_DUR * self.host_sr;
        if dur_samp < 1.0 {
            return;
        }
        let detune = 1.0 + (self.rng01() - 0.5) * DETUNE;
        // Read speed: sample-rate ratio keeps pitch correct across host SR.
        // Shimmer: some grains read an octave up (×2) for a bright, airy sheen.
        let oct = if self.rng01() < self.octave { 2.0 } else { 1.0 };
        // Play the drone: when notes are held, each grain takes one note's pitch.
        let key = if self.key_ratios.is_empty() {
            1.0
        } else {
            let idx = (self.rng01() * self.key_ratios.len() as f32) as usize;
            self.key_ratios[idx.min(self.key_ratios.len() - 1)]
        };
        let step = key * oct * (self.samp_sr / self.host_sr) * detune;
        let pan = (self.rng01() * 2.0 - 1.0) * WIDTH;
        let panl = sqrtf((1.0 - pan) * 0.5); // equal-power (shared sqrt → matches WASM)
        let panr = sqrtf((1.0 + pan) * 0.5);
        let slot = self.alloc_voice();
        self.voices[slot] = Voice {
            pos,
            age: 0.0,
            inv_dur: 1.0 / dur_samp,
            gain: GRAIN_GAIN,
            step,
            panl,
            panr,
            depth,
        };
    }

    fn spawn(&mut self) {
        let len = self.sample.len();
        if len < 2 {
            return;
        }
        let span = (len as f32) / self.samp_sr;
        // Recursive autoevolution: EVO slowly sweeps the focus and the output
        // envelope (ENV) widens the aperture — the render feeds back into itself.
        let eff_focus = clampf(
            self.focus01 * span + (self.evo - 0.5) * self.feedback * span * 0.45,
            0.0,
            span,
        );
        let eff_ap = (self.aperture_ms * 0.001) * (1.0 + self.feedback * self.env * 0.8);
        let off_sec = eff_focus + tri_inv(self.next_u()) * eff_ap;
        let maxp = (len - 2) as f32;
        let relative_pos = clampf(off_sec * self.samp_sr, 0.0, maxp);
        let pos = if self.live {
            ((self.cap_w + relative_pos as usize) % len) as f32
        } else {
            relative_pos
        };

        // Log for the visualizer.
        if span > 0.0 {
            self.viz_focus = clampf(eff_focus / span, 0.0, 1.0);
            self.viz_ap = clampf(eff_ap / span, 0.0, 1.0);
        }
        let w = self.vlog_w % VLOG;
        self.vlog_pos[w] = if maxp > 0.0 { pos / maxp } else { 0.5 };
        self.vlog_w = self.vlog_w.wrapping_add(1);

        self.place(pos, self.bounces);
    }

    // One stereo output frame.
    #[inline]
    pub fn tick(&mut self) -> (f32, f32) {
        if self.sample.len() < 2 {
            return (0.0, 0.0);
        }
        let rate_ps = self.grain_rate / self.host_sr;
        self.spawn_acc += rate_ps;
        while self.spawn_acc >= 1.0 {
            self.spawn();
            self.spawn_acc -= 1.0;
        }

        let mut accl = 0.0f32;
        let mut accr = 0.0f32;
        let mut k = 0usize;
        while k < self.nactive {
            let i = self.active[k];
            let ph = self.voices[i].age * self.voices[i].inv_dur;
            if ph >= 1.0 {
                // Capture the dying grain's state before recycling its slot.
                let depth = self.voices[i].depth;
                let endpos = self.voices[i].pos;
                // swap-remove from active list + return slot to the free pool
                self.nactive -= 1;
                self.active[k] = self.active[self.nactive];
                self.free[self.nfree] = i;
                self.nfree += 1;
                // Recursive bounce (Russian roulette): the ray survives with
                // probability = reflection coefficient, relaunching a child grain
                // near where it landed. The decaying chain is the Neumann tail.
                if depth > 0 && self.rng01() < self.refl {
                    let maxp = (self.sample.len().max(2) - 2) as f32;
                    let jitter = (self.rng01() - 0.5) * 0.1 * self.samp_sr;
                    let cpos = if self.live {
                        (endpos + jitter).rem_euclid(self.sample.len() as f32)
                    } else {
                        clampf(endpos + jitter, 0.0, maxp)
                    };
                    self.place(cpos, depth - 1);
                }
                continue; // active[k] is now another voice: don't advance k
            }
            let raw = if self.live {
                sample_at_wrapped(&self.sample, self.voices[i].pos)
            } else {
                sample_at(&self.sample, self.voices[i].pos)
            };
            let s = raw * win_at(&self.window, ph) * self.voices[i].gain;
            accl += s * self.voices[i].panl;
            accr += s * self.voices[i].panr;
            self.voices[i].pos += self.voices[i].step;
            if self.live && self.voices[i].pos >= self.sample.len() as f32 {
                self.voices[i].pos -= self.sample.len() as f32;
            }
            self.voices[i].age += 1.0;
            k += 1;
        }

        let (rl, rr) = self.reverb.process(accl * self.master, accr * self.master);
        let (yl, yr) = self.dc.process(rl, rr);
        let ol = soft(yl);
        let orr = soft(yr);

        // Log the actual output (post-everything) for the GUI's scope/spectrum.
        self.out_ring[self.out_w] = (ol + orr) * 0.5;
        self.out_w += 1;
        if self.out_w >= OUT_RING {
            self.out_w = 0;
        }

        // Recursive autoevolution: follow the output level (ENV) and advance the
        // self-sweep (EVO). ENV widens the aperture; EVO drifts the focus — both
        // gated by `feedback`. Per-sample coefficients ≈ the WASM per-block ones.
        let lvl = (ol.abs() + orr.abs()) * 0.5;
        let rate_scale = 44_100.0 / self.host_sr;
        let env_mix = 1.0 - 0.9996f32.powf(rate_scale);
        self.env += (lvl - self.env) * env_mix;
        if self.feedback > 0.0 {
            let step = self.feedback * (0.0004 + self.env * 0.0018) / 256.0 * rate_scale;
            self.evo += self.evo_dir * step;
            if self.evo >= 1.0 {
                self.evo = 1.0;
                self.evo_dir = -1.0;
            }
            if self.evo <= 0.0 {
                self.evo = 0.0;
                self.evo_dir = 1.0;
            }
        }
        (ol, orr)
    }
}

// The whole DSP kernel — clampf, soft, sqrtf, tri_inv, sample_at, win_at, rng,
// Reverb, DcBlocker — now lives in `raydrone_core`, shared verbatim with the
// WASM engine. Nothing engine-specific left to define here.

// ── Output scope/spectrum (GUI thread only) ─────────────────────────────────
// The audio thread only ever writes `out_ring` (see `tick()`); everything below
// reads a *snapshot* of it and is only ever called from the GUI thread when
// painting the "what you actually hear" panel. None of this is real-time-safe
// (the FFT allocates two scratch Vecs) and none of it needs to be.

/// Reorder a ring buffer (oldest sample first) starting from `write` into
/// `out`. `out.len()` must equal `ring.len()`.
pub fn ring_ordered(ring: &[f32], write: usize, out: &mut [f32]) {
    let n = ring.len();
    debug_assert_eq!(out.len(), n);
    for i in 0..n {
        out[i] = ring[(write + i) % n];
    }
}

/// In-place iterative radix-2 Cooley-Tukey FFT. `re`/`im` must be the same
/// power-of-two length.
fn fft_radix2(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    debug_assert!(n.is_power_of_two() && n == im.len());

    // Bit-reversal permutation.
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j &= !bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }

    // Iterative Cooley-Tukey, butterfly stage by stage.
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * PI / (len as f32);
        let (wr, wi) = (ang.cos(), ang.sin());
        let mut i = 0;
        while i < n {
            let (mut cwr, mut cwi) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (ur, ui) = (re[i + k], im[i + k]);
                let (xr, xi) = (re[i + k + len / 2], im[i + k + len / 2]);
                let vr = xr * cwr - xi * cwi;
                let vi = xr * cwi + xi * cwr;
                re[i + k] = ur + vr;
                im[i + k] = ui + vi;
                re[i + k + len / 2] = ur - vr;
                im[i + k + len / 2] = ui - vi;
                let (ncwr, ncwi) = (cwr * wr - cwi * wi, cwr * wi + cwi * wr);
                cwr = ncwr;
                cwi = ncwi;
            }
            i += len;
        }
        len <<= 1;
    }
}

/// Hann-windowed magnitude spectrum of `samples` (length a power of two,
/// oldest-first — pass it through `ring_ordered` first). Fills `mag_out`
/// (length `samples.len() / 2`) with bins 0..Nyquist.
pub fn magnitude_spectrum(samples: &[f32], mag_out: &mut [f32]) {
    let n = samples.len();
    debug_assert!(n.is_power_of_two());
    debug_assert_eq!(mag_out.len(), n / 2);
    let mut re: Vec<f32> = samples
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            let w = 0.5 - 0.5 * (2.0 * PI * i as f32 / (n - 1) as f32).cos(); // Hann
            s * w
        })
        .collect();
    let mut im = vec![0.0f32; n];
    fft_radix2(&mut re, &mut im);
    for (k, m) in mag_out.iter_mut().enumerate() {
        *m = (re[k] * re[k] + im[k] * im[k]).sqrt();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine_scene(n: usize, freq: f32, sr: f32) -> Vec<f32> {
        (0..n)
            .map(|i| (2.0 * PI * freq * i as f32 / sr).sin())
            .collect()
    }

    #[test]
    fn empty_engine_is_silent() {
        let mut e = Engine::new(44100.0);
        assert!(e.is_empty());
        assert_eq!(e.tick(), (0.0, 0.0));
    }

    #[test]
    fn load_returns_previous_buffer_and_engine_becomes_audible() {
        let mut e = Engine::new(44100.0);
        let first = sine_scene(4096, 220.0, 44100.0);
        let old = e.load(first.clone(), 44100.0);
        assert!(
            old.is_empty(),
            "fresh engine should have had no prior sample"
        );
        assert!(!e.is_empty());

        e.set_density(2000.0); // dense enough to hear quickly
        e.set_master(1.0);
        let mut heard = false;
        for _ in 0..20_000 {
            let (l, r) = e.tick();
            assert!(l.is_finite() && r.is_finite());
            if l.abs() > 1e-4 || r.abs() > 1e-4 {
                heard = true;
            }
        }
        assert!(
            heard,
            "engine produced no audible output after loading a scene"
        );

        let second = sine_scene(2048, 440.0, 44100.0);
        let replaced = e.load(second, 44100.0);
        assert_eq!(
            replaced.len(),
            first.len(),
            "load() must hand back the buffer it replaced"
        );
    }

    #[test]
    fn live_capture_takes_the_callers_buffer_and_renders_it() {
        let mut e = Engine::new(48000.0);
        let mut buf = vec![0.0f32; 48000 * 2]; // 2 s, pre-built by the caller
        for (i, s) in buf.iter_mut().enumerate() {
            *s = (2.0 * PI * 220.0 * i as f32 / 48000.0).sin();
        }
        let old = e.begin_live_capture(buf, 48000.0);
        assert!(old.is_empty());
        assert!(!e.is_empty());

        e.set_density(2000.0);
        e.set_master(1.0);
        let mut heard = false;
        for _ in 0..10_000 {
            let (l, r) = e.tick();
            assert!(l.is_finite() && r.is_finite());
            if l.abs() > 1e-4 || r.abs() > 1e-4 {
                heard = true;
            }
        }
        assert!(heard, "live-captured scene produced no audible output");
    }

    #[test]
    fn voice_pool_handles_oversubscription_without_panicking() {
        // Push density far past MAX_VOICES so alloc_voice() must steal slots.
        let mut e = Engine::new(44100.0);
        let _ = e.load(sine_scene(8192, 110.0, 44100.0), 44100.0);
        e.set_density(50_000.0); // way more than 512 grains can sustain at once
        e.set_master(1.0);
        for _ in 0..10_000 {
            let (l, r) = e.tick();
            assert!(l.is_finite() && r.is_finite());
        }
    }

    #[test]
    fn set_keys_spans_the_midi_range_without_panicking() {
        let mut e = Engine::new(44100.0);
        let _ = e.load(sine_scene(4096, 220.0, 44100.0), 44100.0);
        let mut mask = [false; 128];
        mask[0] = true; // 60 semitones below root
        mask[127] = true; // 67 semitones above root
        e.set_keys(&mask, 60);
        e.set_density(1000.0);
        e.set_master(1.0);
        for _ in 0..2000 {
            let (l, r) = e.tick();
            assert!(l.is_finite() && r.is_finite());
        }
    }

    #[test]
    fn reset_clears_voices_and_silences_the_engine() {
        let mut e = Engine::new(44100.0);
        let _ = e.load(sine_scene(4096, 220.0, 44100.0), 44100.0);
        e.set_density(2000.0);
        e.set_master(1.0);
        for _ in 0..1000 {
            e.tick();
        }
        e.reset();
        // Right after reset no grain has had time to ramp up through the Hann
        // window, so the very next frame must be silent.
        assert_eq!(e.tick(), (0.0, 0.0));
    }

    #[test]
    fn out_ring_fills_and_wraps_with_finite_audio() {
        let mut e = Engine::new(44100.0);
        let _ = e.load(sine_scene(8192, 220.0, 44100.0), 44100.0);
        e.set_density(2000.0);
        e.set_master(1.0);
        for _ in 0..(OUT_RING * 2 + 17) {
            e.tick();
        }
        assert_eq!(
            e.out_write(),
            17,
            "write pointer should have wrapped exactly twice plus 17"
        );
        assert!(e.out_buffer().iter().all(|s| s.is_finite()));
        assert!(
            e.out_buffer().iter().any(|&s| s.abs() > 1e-4),
            "ring never captured audible output"
        );
    }

    #[test]
    fn ring_ordered_reorders_oldest_first() {
        let ring = [4.0, 5.0, 0.0, 1.0, 2.0, 3.0]; // write pointer at 2: oldest..newest = 0..5
        let mut out = [0.0; 6];
        ring_ordered(&ring, 2, &mut out);
        assert_eq!(out, [0.0, 1.0, 2.0, 3.0, 4.0, 5.0]);
    }

    #[test]
    fn magnitude_spectrum_concentrates_energy_at_the_tone_bin() {
        const N: usize = 1024;
        let k0 = 50; // bin the test tone should land in
        let samples: Vec<f32> = (0..N)
            .map(|i| (2.0 * PI * k0 as f32 * i as f32 / N as f32).sin())
            .collect();
        let mut mag = [0.0f32; N / 2];
        magnitude_spectrum(&samples, &mut mag);
        assert!(mag.iter().all(|m| m.is_finite()));
        let (peak_bin, &peak) = mag
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap();
        assert_eq!(peak_bin, k0, "energy should peak at the tone's own bin");
        // Window leakage spreads some energy to neighbours, but the peak bin
        // should still dominate by a wide margin over a distant, empty bin.
        assert!(peak > mag[k0 + 100] * 10.0);
    }

    #[test]
    fn magnitude_spectrum_dc_input_peaks_at_bin_zero() {
        const N: usize = 256;
        let samples = [0.5f32; N];
        let mut mag = [0.0f32; N / 2];
        magnitude_spectrum(&samples, &mut mag);
        let (peak_bin, _) = mag
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap();
        assert_eq!(peak_bin, 0);
    }
}
