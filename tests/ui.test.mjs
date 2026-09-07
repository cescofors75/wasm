import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import { segment, bytes } from './helpers.mjs';

function engineContext(failFirst=false) {
  const calls={modules:0,nodes:0,fetches:0,messages:[]};
  const context={console,Float32Array,Uint8Array,wasmBytes:null,node:null,sampleData:new Float32Array(4),sampleRate:48000,
    ctx:{state:'running',audioWorklet:{addModule:async()=>{calls.modules++;if(failFirst&&calls.modules===1)throw Error('network failure');}}},
    fetchArrayBuffer:async()=>{calls.fetches++;return bytes;},jsGain:{},smoothChk:{checked:true},
    AudioWorkletNode:class {constructor(){calls.nodes++;this.port={postMessage:m=>calls.messages.push(m)};} connect(){}},
  };
  for(const name of ['sendFx','sendInertia','sendSpace','sendPitch','sendReverb','sendMaterial','sendModulation','sendEffects','sendAdvancedEffects','sendSmart','applyTonality','sendFilter','sendFilterLfo','sendAmbient']) context[name]=()=>{};
  vm.createContext(context);
  vm.runInContext(segment('    let engineInitPromise', '    let audioLoadToken'),context);
  return {context,calls};
}
test('engine retries module registration after a transient failure',async()=>{
  const {context,calls}=engineContext(true);
  await assert.rejects(vm.runInContext('ensureEngine()',context),/network failure/);
  await vm.runInContext('ensureEngine()',context);
  assert.equal(calls.modules,2);assert.equal(calls.fetches,1);assert.equal(calls.nodes,1);
});
test('concurrent initialization has one node and existing engine is not reloaded',async()=>{
  const {context,calls}=engineContext();
  await vm.runInContext('Promise.all([ensureEngine(),ensureEngine(),ensureEngine()])',context);
  const count=calls.messages.length;
  await vm.runInContext('ensureEngine()',context);
  assert.equal(calls.nodes,1);assert.equal(calls.modules,1);assert.equal(calls.fetches,1);
  assert.equal(calls.messages.length,count);
});

function loadContext() {
  const calls=[];
  const c={console,Float32Array,audioLoadToken:1,isPlaying:true,mode:'dry',sampleData:null,sampleRate:48000,duration:1,focusSec:.3,
    node:{port:{postMessage:m=>calls.push(m)}},sendParams:active=>calls.push({type:'params',active}),
    playBtn:{},recBtn:{},fileLabel:{},statusEl:{},rayI18n:{t:x=>x},computePeaks(){},invalidateLab:()=>calls.push({type:'invalidate'}),
    ctx:{decodeAudioData:(_,resolve)=>resolve({length:10,duration:.1,numberOfChannels:1,sampleRate:100,getChannelData:()=>new Float32Array(10)})}
  };
  vm.createContext(c);
  vm.runInContext(segment('    function stopDry()', '    async function play()')+segment('    function stop()','    playBtn.addEventListener')+segment('    async function loadAudioBuffer(', '    async function loadDefaultAudio('),c);
  return {c,calls};
}
test('successful file replacement atomically stops direct and resets UI and lab',async()=>{
  const {c,calls}=loadContext();
  assert.equal(await vm.runInContext('loadAudioBuffer(null,"new.wav",1)',c),true);
  assert.equal(c.isPlaying,false);assert.equal(c.playBtn.innerText,'▶ Play');
  assert.equal(calls[0].type,'direct');assert.equal(calls[0].on,0);
  assert.ok(calls.findIndex(x=>x.type==='invalidate')<calls.findIndex(x=>x.type==='sample'));
});
test('stale decode completion cannot replace the newer source',async()=>{
  const {c,calls}=loadContext(); c.audioLoadToken=2;
  assert.equal(await vm.runInContext('loadAudioBuffer(null,"old.wav",1)',c),false);
  assert.equal(calls.length,0);assert.equal(c.isPlaying,true);
});
test('decode error preserves existing playback and does not mutate source state',async()=>{
  const {c,calls}=loadContext(); c.ctx.decodeAudioData=(_,resolve,reject)=>reject(Error('bad codec'));
  await assert.rejects(vm.runInContext('loadAudioBuffer(null,"bad.wav",1)',c),/bad codec/);
  assert.equal(calls.length,0);assert.equal(c.isPlaying,true);
});
test('changing source cancels lab and rejects already queued results',()=>{
  const workers=[];
  const c={console,Math,sampleData:new Float32Array(48000),wasmBytes:bytes,sampleRate:48000,duration:1,focusSec:.3,
    labWorker:null,labTarget:null,labCurves:null,labA:0,labF0:0,
    labRun:{},labReadout:{},charSlider:{value:'0'},LAB_MAX_A:6600,LAB_NS:[1],LAB_TRIALS:1,LAB_SEED:1,LAB_SERIES:[{k:'random',id:0}],
    labTargetBtn:{},labNoisyBtn:{},labCleanBtn:{},labStopBtn:{},labCsvBtn:{},
    clamp:(x,a,b)=>Math.max(a,Math.min(b,x)),rayI18n:{t:x=>x},labStop(){},labDrawPlot(){},labUpdateReadout(){},
    Worker:class {constructor(){workers.push(this);}postMessage(){}terminate(){this.terminated=true;}}
  };
  vm.createContext(c);
  vm.runInContext(segment('    let labGeneration', '    function labGetHann()')+segment('    function labRunTest()', '    // Export CSV'),c);
  vm.runInContext('labRunTest()',c);const old=workers[0];
  vm.runInContext('invalidateLab()',c);
  assert.equal(old.terminated,true);assert.equal(c.labWorker,null);
  old.onmessage({data:{type:'done',target:Float32Array.of(1),curves:{random:[1]}}});
  assert.equal(c.labTarget,null);assert.equal(c.labCurves,null);assert.equal(c.labTargetBtn.disabled,true);
  vm.runInContext('labRunTest()',c);
  workers[1].onmessage({data:{type:'done',target:Float32Array.of(2),curves:{random:[.5]}}});
  assert.equal(c.labTarget[0],2);assert.equal(c.labTargetBtn.disabled,false);
});
