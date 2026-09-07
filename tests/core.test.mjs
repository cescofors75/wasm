import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const ex=new WebAssembly.Instance(new WebAssembly.Module(readFileSync(new URL('../.build/core-tests.wasm',import.meta.url))),{}).exports;
test('sqrt accuracy spans subnormal through maximum finite f32',()=>{
  const values=[0,1e-45,1e-38,1e-30,1e-12,1e-6,.1,1,2,100,1e20,3.4028234663852886e38];
  for(const value of values){const x=Math.fround(value), want=Math.sqrt(x),actual=ex.sqrt(x);
    assert.ok(Math.abs(actual-want)<=Math.max(1e-30,want*2e-7),`${x}: ${actual} vs ${want}`);}
});
test('octave exponent matches 2^x throughout modulation range',()=>{
  for(let x=-12;x<=12;x+=.125)assert.ok(Math.abs(ex.exp2(x)/2**x-1)<2e-6, String(x));
});
test('two-octave filter modulation spans cutoff/4 through cutoff*4',()=>{
  ex.filter_init(48000,1000,2);
  assert.ok(Math.abs(ex.filter_advance(0)-250)<.01);
  assert.ok(Math.abs(ex.filter_advance(24000)-4000)<10);
});
for(const sr of [44100,48000,96000]) for(const scenario of [0,1,2,3,4]) test(`VST engine scenario ${scenario}, ${sr} Hz (WASM execution)`,()=>{
  const result=ex.vst_engine_check(scenario,sr);
  if(scenario===0||scenario===4)assert.equal(result,0);
  else assert.ok(result>1e-5&&Number.isFinite(result));
});
