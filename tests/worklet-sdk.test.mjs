import test from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { processor } from './helpers.mjs';
const SDK=createRequire(import.meta.url)('../sdk/raydrone-sdk.js');

test('sample and direct resets cannot overflow spawn telemetry or replay stale rays', () => {
  const p=processor();
  const sample=new Float32Array(48000);
  p.handleMsg({type:'sample',data:sample,sampleRate:48000});
  p.handleMsg({type:'params',focus:.3,aperture:.1,grainMs:150,grainRate:200,gain:.3,master:1});
  const out=[[new Float32Array(128),new Float32Array(128)]];
  for(let i=0;i<16;i++) p.process([],out);
  assert.ok(p.messages.at(-1).perf.spawnsPerSec>0);
  p.handleMsg({type:'direct',on:1,offsetSec:0});
  for(let i=0;i<16;i++) p.process([],out);
  assert.equal(p.messages.at(-1).perf.spawnsPerSec,0);
  assert.equal(p.messages.at(-1).type,'level');
  p.handleMsg({type:'sample',data:sample,sampleRate:48000});
  for(let i=0;i<16;i++) p.process([],out);
  assert.equal(p.messages.at(-1).perf.spawnsPerSec,0);
});

test('recording preserves frames across chunk boundaries and flushes once', () => {
  const p=processor(8000); const left=Float32Array.from({length:8100},(_,i)=>i/8100);
  p.handleMsg({type:'record',on:1}); p.recordBlock(left,left);
  p.handleMsg({type:'record',on:0}); p.handleMsg({type:'record',on:0});
  const chunks=p.messages.filter(x=>x.type==='recdata');
  assert.deepEqual(chunks.map(x=>x.l.length),[8000,100]);
  assert.equal(chunks.filter(x=>x.done).length,1);
  assert.equal(chunks[1].l[99],left[8099]);
});

test('SDK rejects protocol overrides and nonfinite values before sending any message', () => {
  const sent=[], port={postMessage:x=>sent.push(x)};
  for(const params of [{type:'record',on:true},{master:NaN},{gain:Infinity},[]]) {
    assert.throws(()=>SDK.applyScene(port,{material:{base:1},params}));
    assert.equal(sent.length,0);
  }
});
test('partial SDK scenes receive complete audible defaults and bounded effects', () => {
  const sent=[],port={postMessage:x=>sent.push(x)};
  SDK.registerScene('partial-test',{params:{focus:2}}); SDK.applyScene(port,'partial-test');
  assert.deepEqual(sent[0],{type:'params',focus:2,aperture:.1,grainMs:150,grainRate:200,gain:.3,master:1});
  SDK.applyMaterial(port,{base:99,effects:{delayFeedback:1,reverbWet:1}});
  assert.equal(sent[1].kind,5); assert.equal(sent[2].delayFeedback,.68); assert.equal(sent[3].wet,.82);
});
test('registered scene materials are immutable snapshots', () => {
  const material={base:1,amount:.5}; SDK.registerScene('snapshot-test',{material});
  material.base=5; const sent=[]; SDK.applyScene({postMessage:x=>sent.push(x)},'snapshot-test');
  assert.equal(sent[0].kind,1);
});
