// Runs the actual shared core and VST Engine under WASM when native SDKs are
// unavailable. This does not replace cargo test / DAW integration checks.
#![allow(static_mut_refs, dead_code)]
#[path = "../vst/src/engine.rs"]
mod engine;
use raydrone_core::filter::Filter;
static mut FILTER: Filter = Filter::new();

#[no_mangle]
pub extern "C" fn sqrt(value: f32) -> f32 {
    raydrone_core::sqrtf(value)
}
#[no_mangle]
pub extern "C" fn exp2(value: f32) -> f32 {
    raydrone_core::exp2f(value)
}
#[no_mangle]
pub extern "C" fn filter_init(sr: f32, cutoff: f32, depth: f32) {
    unsafe {
        FILTER = Filter::new();
        FILTER.set_sample_rate(sr);
        FILTER.set(cutoff, 0.0);
        FILTER.set_lfo(1.0, depth);
    }
}
#[no_mangle]
pub extern "C" fn filter_advance(frames: u32) -> f32 {
    unsafe {
        for _ in 0..frames {
            FILTER.process(0.0, 0.0);
        }
        FILTER.effective_cutoff()
    }
}
#[no_mangle]
pub extern "C" fn vst_engine_check(case: u32, sr: f32) -> f32 {
    let mut e = engine::Engine::new(sr);
    if case == 0 {
        let (l, r) = e.tick();
        return l.abs() + r.abs();
    }
    let sample: Vec<f32> = (0..(sr as usize))
        .map(|i| (i as f32 * 2.0 * core::f32::consts::PI * 220.0 / sr).sin() * 0.5)
        .collect();
    if case == 2 {
        let _ = e.begin_live_capture(sample, sr);
    } else {
        let _ = e.load(sample, sr);
    }
    e.set_density(2000.0);
    e.set_master(0.5);
    if case == 3 {
        let mut keys = [false; 128];
        keys[0] = true;
        keys[127] = true;
        e.set_keys(&keys, 60);
        e.set_bounce(6);
        e.set_reflect(0.9);
    }
    let mut energy = 0.0;
    for _ in 0..24000 {
        let (l, r) = e.tick();
        if !l.is_finite() || !r.is_finite() {
            return -1.0;
        }
        energy += l * l + r * r;
    }
    if case == 4 {
        e.reset();
        let (l, r) = e.tick();
        return l.abs() + r.abs();
    }
    energy
}
