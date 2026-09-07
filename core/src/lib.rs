#![no_std]

const TWO_PI: f32 = 6.283_185_5;

#[inline]
fn absf(value: f32) -> f32 {
    if value < 0.0 {
        -value
    } else {
        value
    }
}

#[inline]
pub fn clampf(value: f32, minimum: f32, maximum: f32) -> f32 {
    if !value.is_finite() || value < minimum {
        minimum
    } else if value > maximum {
        maximum
    } else {
        value
    }
}

#[inline]
pub fn sqrtf(value: f32) -> f32 {
    if !value.is_finite() || value <= 0.0 {
        return 0.0;
    }

    // Normalize subnormals before constructing an exponent-based estimate.
    let tiny = value < f32::MIN_POSITIVE;
    let x = if tiny { value * 16_777_216.0 } else { value };
    let mut estimate = f32::from_bits((x.to_bits() >> 1) + 0x1fc0_0000);
    for _ in 0..4 {
        estimate = 0.5 * (estimate + x / estimate);
    }
    if tiny {
        estimate / 4096.0
    } else {
        estimate
    }
}

/// Inverse CDF of p(k) = (a - |k|)/a², with zero-mass endpoints excluded.
/// Used by offline estimators; the live engine keeps continuous offsets.
pub fn discrete_tri_offset(unit: f64, aperture: i32) -> i32 {
    let a = aperture.max(1).min(1_000_000);
    let u = if unit.is_finite() {
        unit.max(0.0).min(1.0)
    } else {
        0.0
    };
    let mut lo = 1 - a;
    let mut hi = a - 1;
    let denom = 2.0 * (a as f64) * (a as f64);
    while lo < hi {
        let k = lo + (hi - lo) / 2;
        let cdf = if k < 0 {
            let j = (a + k) as f64;
            j * (j + 1.0) / denom
        } else {
            let j = (a - k) as f64;
            1.0 - j * (j - 1.0) / denom
        };
        if u < cdf {
            hi = k;
        } else {
            lo = k + 1;
        }
    }
    lo
}

/// 2^x for the bounded octave modulation range, without a libm dependency.
pub fn exp2f(x: f32) -> f32 {
    let x = clampf(x, -24.0, 24.0);
    let mut integer = x as i32;
    if x < integer as f32 {
        integer -= 1;
    }
    let y = (x - integer as f32) * core::f32::consts::LN_2;
    let mut sum = 1.0;
    let mut term = 1.0;
    for n in 1..=8 {
        term *= y / n as f32;
        sum += term;
    }
    sum * f32::from_bits(((integer + 127) as u32) << 23)
}

#[inline]
pub fn soft(value: f32) -> f32 {
    if !value.is_finite() {
        return 0.0;
    }
    0.7 * value / (1.0 + absf(value))
}

#[inline]
pub fn tri_inv(value: f32) -> f32 {
    let unit = clampf(value, 0.0, 1.0);
    if unit < 0.5 {
        sqrtf(2.0 * unit) - 1.0
    } else {
        1.0 - sqrtf(2.0 * (1.0 - unit))
    }
}

#[inline]
pub fn rng01(state: &mut u32) -> f32 {
    *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    (*state as f32) * (1.0 / 4_294_967_296.0)
}

#[inline]
pub fn sample_at(sample: &[f32], position: f32) -> f32 {
    if sample.len() < 4 || !position.is_finite() {
        return 0.0;
    }

    let maximum_position = (sample.len() - 2) as f32;
    let bounded_position = clampf(position, 0.0, maximum_position);
    let base = bounded_position as usize;
    let fraction = bounded_position - base as f32;
    let previous = sample[base.saturating_sub(1)];
    let current = sample[base];
    let next = sample[(base + 1).min(sample.len() - 1)];
    let following = sample[(base + 2).min(sample.len() - 1)];
    let a = -0.5 * previous + 1.5 * current - 1.5 * next + 0.5 * following;
    let b = previous - 2.5 * current + 2.0 * next - 0.5 * following;
    let c = -0.5 * previous + 0.5 * next;
    ((a * fraction + b) * fraction + c) * fraction + current
}

#[inline]
pub fn win_at(window: &[f32], phase: f32) -> f32 {
    if window.len() < 2 || !phase.is_finite() {
        return 0.0;
    }

    let position = clampf(phase, 0.0, 1.0) * (window.len() - 1) as f32;
    let index = position as usize;
    let fraction = position - index as f32;
    let next = (index + 1).min(window.len() - 1);
    window[index] + (window[next] - window[index]) * fraction
}

const REVERB_CAPACITY: usize = 4_096;

pub struct Reverb {
    wet: f32,
    feedback: f32,
    delay_length: usize,
    write_index: usize,
    left: [f32; REVERB_CAPACITY],
    right: [f32; REVERB_CAPACITY],
}

impl Reverb {
    pub const fn new() -> Self {
        Self {
            wet: 0.0,
            feedback: 0.72,
            delay_length: 3_528,
            write_index: 0,
            left: [0.0; REVERB_CAPACITY],
            right: [0.0; REVERB_CAPACITY],
        }
    }

    pub fn reset(&mut self) {
        for value in &mut self.left {
            *value = 0.0;
        }
        for value in &mut self.right {
            *value = 0.0;
        }
        self.write_index = 0;
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        let rate = if sample_rate.is_finite() && sample_rate > 1.0 {
            sample_rate
        } else {
            44_100.0
        };
        let length = (rate * 0.073_5) as usize;
        self.delay_length = length.clamp(256, REVERB_CAPACITY);
        self.write_index %= self.delay_length;
    }

    #[inline]
    pub fn set_wet(&mut self, wet: f32) {
        self.wet = clampf(wet, 0.0, 1.0);
    }

    #[inline]
    pub fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
        let delayed_left = self.left[self.write_index];
        let delayed_right = self.right[self.write_index];
        self.left[self.write_index] =
            left + (delayed_left * 0.23 + delayed_right * 0.77) * self.feedback;
        self.right[self.write_index] =
            right + (delayed_right * 0.23 + delayed_left * 0.77) * self.feedback;
        self.write_index += 1;
        if self.write_index >= self.delay_length {
            self.write_index = 0;
        }

        let dry = 1.0 - self.wet;
        (
            left * dry + delayed_left * self.wet,
            right * dry + delayed_right * self.wet,
        )
    }
}

pub struct DcBlocker {
    previous_left: f32,
    previous_right: f32,
    output_left: f32,
    output_right: f32,
    coefficient: f32,
}

impl DcBlocker {
    pub const fn new() -> Self {
        Self {
            previous_left: 0.0,
            previous_right: 0.0,
            output_left: 0.0,
            output_right: 0.0,
            coefficient: 0.998_5,
        }
    }

    pub fn reset(&mut self) {
        self.previous_left = 0.0;
        self.previous_right = 0.0;
        self.output_left = 0.0;
        self.output_right = 0.0;
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        let rate = if sample_rate.is_finite() && sample_rate > 1.0 {
            sample_rate
        } else {
            44_100.0
        };
        self.coefficient = clampf(1.0 - TWO_PI * 10.0 / rate, 0.9, 0.999_9);
    }

    #[inline]
    pub fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
        self.output_left = left - self.previous_left + self.coefficient * self.output_left;
        self.output_right = right - self.previous_right + self.coefficient * self.output_right;
        self.previous_left = left;
        self.previous_right = right;
        (self.output_left, self.output_right)
    }
}

pub mod filter {
    use crate::{clampf, exp2f};

    // Topology-preserving state-variable lowpass. Its integrator states remain
    // well behaved as cutoff moves; Q, Hz and octave depth are physical units.
    pub struct Filter {
        sample_rate: f32,
        cutoff: f32,
        resonance: f32,
        lfo_rate: f32,
        lfo_depth: f32,
        lfo_phase: f32,
        state: [[f32; 2]; 2],
        a1: f32,
        a2: f32,
        a3: f32,
        k: f32,
        dirty: bool,
        coefficient_tick: u32,
    }
    impl Filter {
        pub const fn new() -> Self {
            Self {
                sample_rate: 44100.0,
                cutoff: 44100.0,
                resonance: 0.0,
                lfo_rate: 0.0,
                lfo_depth: 0.0,
                lfo_phase: 0.0,
                state: [[0.0; 2]; 2],
                a1: 1.0,
                a2: 0.0,
                a3: 0.0,
                k: 1.4142135,
                dirty: true,
                coefficient_tick: 0,
            }
        }
        pub fn reset(&mut self) {
            self.state = [[0.0; 2]; 2];
            self.lfo_phase = 0.0;
            self.dirty = true;
            self.coefficient_tick = 0;
        }
        pub fn set_sample_rate(&mut self, sr: f32) {
            self.sample_rate = if sr.is_finite() {
                sr.max(8000.0).min(384000.0)
            } else {
                44100.0
            };
            self.dirty = true;
        }
        pub fn set(&mut self, cutoff: f32, res: f32) {
            let cutoff = clampf(cutoff, 10.0, self.sample_rate * 0.5);
            let res = clampf(res, 0.0, 1.0);
            self.dirty |= cutoff != self.cutoff || res != self.resonance;
            self.cutoff = cutoff;
            self.resonance = res;
        }
        pub fn set_lfo(&mut self, rate: f32, depth: f32) {
            self.lfo_rate = clampf(rate, 0.0, 100.0);
            self.lfo_depth = clampf(depth, 0.0, 12.0);
            self.dirty = true;
        }
        pub fn effective_cutoff(&self) -> f32 {
            let triangle = 1.0 - (2.0 * self.lfo_phase - 1.0).abs();
            clampf(
                self.cutoff * exp2f((triangle * 2.0 - 1.0) * self.lfo_depth),
                10.0,
                self.sample_rate * 0.49,
            )
        }
        fn coefficients(&mut self) {
            let x = core::f32::consts::PI * self.effective_cutoff() / self.sample_rate;
            let x2 = x * x;
            // sin/cos Taylor on [0, 0.49*pi], used at control rate, not libm.
            let sin = x
                * (1.0
                    + x2 * (-1.0 / 6.0
                        + x2 * (1.0 / 120.0
                            + x2 * (-1.0 / 5040.0 + x2 * (1.0 / 362880.0 - x2 / 39916800.0)))));
            let cos = 1.0
                + x2 * (-1.0 / 2.0
                    + x2 * (1.0 / 24.0
                        + x2 * (-1.0 / 720.0
                            + x2 * (1.0 / 40320.0 + x2 * (-1.0 / 3628800.0 + x2 / 479001600.0)))));
            let g = sin / cos;
            let q = 0.70710677 + self.resonance * 11.292893;
            self.k = 1.0 / q;
            self.a1 = 1.0 / (1.0 + g * (g + self.k));
            self.a2 = g * self.a1;
            self.a3 = g * self.a2;
            self.dirty = false;
        }
        #[inline]
        pub fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
            if self.cutoff >= self.sample_rate * 0.49 && self.lfo_depth == 0.0 {
                self.state = [[0.0; 2]; 2];
                return (left, right);
            }
            if self.dirty || (self.lfo_depth > 0.0 && self.coefficient_tick == 0) {
                self.coefficients();
            }
            self.coefficient_tick = (self.coefficient_tick + 1) % 16;
            self.lfo_phase += self.lfo_rate / self.sample_rate;
            if self.lfo_phase >= 1.0 {
                self.lfo_phase -= 1.0;
            }
            let mut out = [0.0; 2];
            for (ch, input) in [left, right].iter().enumerate() {
                let v3 = input - self.state[ch][1];
                let v1 = self.a1 * self.state[ch][0] + self.a2 * v3;
                let v2 = self.state[ch][1] + self.a2 * self.state[ch][0] + self.a3 * v3;
                self.state[ch][0] = 2.0 * v1 - self.state[ch][0];
                self.state[ch][1] = 2.0 * v2 - self.state[ch][1];
                if v2.is_finite() {
                    out[ch] = v2;
                } else {
                    self.state[ch] = [0.0; 2];
                }
            }
            (out[0], out[1])
        }
    }
}

pub mod music {
    pub const CHORD_UNISON: u32 = 0;

    // Stable catalog v1: IDs match the web UI and saved scenes.
    const UNISON: [f32; 1] = [1.0];
    const OCTAVES: [f32; 3] = [0.5, 1.0, 2.0];
    const POWER: [f32; 5] = [0.5, 1.0, 1.5, 2.0, 3.0];
    const MAJOR: [f32; 3] = [1.0, 1.25, 1.5];
    const MINOR: [f32; 3] = [1.0, 1.2, 1.5];
    const FIFTHS: [f32; 4] = [0.6666667, 1.0, 1.5, 2.25];
    const SUS2: [f32; 3] = [1.0, 1.125, 1.5];
    const PENTATONIC: [f32; 5] = [1.0, 1.125, 1.25, 1.5, 1.6666666];
    const MAJOR_SCALE: [f32; 7] = [1.0, 1.125, 1.25, 1.3333334, 1.5, 1.6666666, 1.875];
    const MINOR_SCALE: [f32; 7] = [1.0, 1.125, 1.2, 1.3333334, 1.5, 1.6, 1.8];
    pub fn chord_ratios(preset: u32) -> &'static [f32] {
        match preset {
            1 => &OCTAVES,
            2 => &POWER,
            3 => &MAJOR,
            4 => &MINOR,
            5 => &FIFTHS,
            6 => &SUS2,
            7 => &PENTATONIC,
            8 => &MAJOR_SCALE,
            9 => &MINOR_SCALE,
            _ => &UNISON,
        }
    }

    pub fn semitone_ratio(semitones: i32) -> f32 {
        const SEMITONE: f32 = 1.059_463_1;
        let mut ratio = 1.0;
        if semitones >= 0 {
            for _ in 0..semitones {
                ratio *= SEMITONE;
            }
        } else {
            for _ in semitones..0 {
                ratio /= SEMITONE;
            }
        }
        ratio
    }
}
