import test from 'node:test';
import assert from 'node:assert/strict';
import { engine, render } from './helpers.mjs';
import { renderRays_CPU, targetCPU, rayOffsets, hann } from '../raytrace_buffer.js';

test('RMS has no artificial floor and scales with source amplitude', () => {
  const measure = amplitude => {
    const { ex } = engine({ amplitude });
    ex.lab_target(12000, 100); ex.lab_estimate(12000, 100, 1, 0, 123);
    return ex.lab_rms();
  };
  const loud = measure(.5), quiet = measure(1e-6);
  assert.ok(quiet > 0 && quiet <= 2e-6);
  assert.ok(Math.abs(quiet / loud / 2e-6 - 1) < 1e-5);
});

for (const sr of [8000, 44100, 48000, 96000, 192000]) {
  for (const frames of [64, 128, 256]) {
    test(`1ms follower remains finite and audible at ${sr} Hz / ${frames} frames`, () => {
      const e = engine({ sr });
      e.ex.set_modulation(2, 2, .25, 1, .001, .001);
      const r = render(e, Math.ceil(sr * 1.2 / frames), frames);
      assert.ok(Number.isFinite(e.ex.top36_pitch_mean()));
      assert.ok(r.energy > 1e-5);
      assert.ok(r.peak <= .7);
    });
  }
}

const chords = [
  [1, [.5, 1, 2]], [2, [.5, 1, 1.5, 2, 3]], [3, [1, 1.25, 1.5]],
  [4, [1, 1.2, 1.5]], [5, [2/3, 1, 1.5, 2.25]], [6, [1, 1.125, 1.5]],
  [7, [1, 1.125, 1.25, 1.5, 5/3]], [8, [1, 1.125, 1.25, 4/3, 1.5, 5/3, 1.875]],
  [9, [1, 1.125, 1.2, 4/3, 1.5, 1.6, 1.8]],
];
for (const [id, expected] of chords) test(`public chord ${id} has its documented ratios`, () => {
  const e = engine(); e.ex.set_chord(id); e.ex.process(128);
  const actual = [...new Set(e.f32(e.ex.slog_s_ptr(), e.ex.slog_cap()))].sort((a,b)=>a-b);
  assert.equal(actual.length, expected.length);
  actual.forEach((x,i) => assert.ok(Math.abs(x-expected[i]) < 1e-6));
});

test('replacing a source stops direct playback and removes all previous effect history', () => {
  const e = engine();
  e.ex.set_reverb(.7); e.ex.set_effects(.6, .1, .6, .2, .5, .01);
  e.ex.set_advanced_effects(.2,.2,.003,.2,.2,.5,.2,.3,440,.8);
  e.ex.set_direct(1, 0); render(e, 200);
  e.f32(e.ex.sample_ptr(), e.len).fill(0); e.ex.set_sample(e.len, e.sr);
  assert.equal(render(e, 200).peak, 0);
  assert.equal(e.ex.simulated_rays(), 0);
  // New nonzero data must also wait for an explicit transport command.
  e.f32(e.ex.sample_ptr(), e.len).fill(.5); e.ex.set_sample(e.len, e.sr);
  assert.equal(render(e, 40).peak, 0);
});

function response(frequency, cutoff, resonance = 0, sr = 48000) {
  const e = engine({ frequency, amplitude: 1e-4, sr });
  e.ex.set_filter(cutoff, resonance); e.ex.set_direct(1, 0);
  return render(e, Math.ceil(sr / 128)).energy;
}
for (const sr of [44100, 48000, 96000]) test(`filter cutoff and resonance have physical units at ${sr} Hz`, () => {
  const gain = (hz, q=0) => 10*Math.log10(response(hz,1000,q,sr)/response(hz,sr/2,0,sr));
  assert.ok(Math.abs(gain(220)) < .1);
  assert.ok(Math.abs(gain(1000)+3.0103) < .15);
  assert.ok(gain(4000) < -20);
  assert.ok(gain(1000, 1) > 15);
  assert.ok(Math.abs(gain(100,1)-gain(100)) < .3);
});

for (const a of [1,2,8]) for (const method of [0,1,2]) test(`discrete estimator matches its target, A=${a}, method=${method}`, () => {
  const source = new Float32Array(2*a+1); source[a]=1;
  const win=hann(1), rays=65536;
  const target=targetCPU(source,a,a,1,win)[0];
  const actual=renderRays_CPU(source,a,a,rays,1,win,rayOffsets(a,rays,method,123))[0];
  assert.ok(Math.abs(actual-target)<.01, `${actual} vs ${target}`);
});

for (const a of [1,2]) for (const method of [0,1,2,3,4]) test(`WASM lab small aperture A=${a}, method=${method} is unbiased for uniform energy`, () => {
  const e=engine(); const data=e.f32(e.ex.sample_ptr(),e.len);
  for(let i=0;i<data.length;i++) data[i]=(i%2)*2-1;
  e.ex.set_sample(e.len,e.sr); e.ex.lab_target(12000,a); e.ex.lab_imp_build(12000,a);
  e.ex.lab_estimate(12000,a,8192,method,123);
  assert.ok(e.ex.lab_rms()<.025, String(e.ex.lab_rms()));
});
