import test from 'node:test';
import assert from 'node:assert/strict';
import { renderRays_GPU } from '../raytrace_buffer.js';
import { traceRoomIR_GPU } from '../acoustic.js';
globalThis.GPUBufferUsage={UNIFORM:1,COPY_DST:2,STORAGE:4,COPY_SRC:8,MAP_READ:16};
globalThis.GPUMapMode={READ:1};
function device(failure) {
  const buffers=[];
  const pass={setPipeline(){},setBindGroup(){},dispatchWorkgroups(){},end(){}};
  return {buffers,queue:{writeBuffer(){},submit(){}},
    createBuffer({size}){const buffer={destroyed:0,destroy(){this.destroyed++;},async mapAsync(){if(failure==='map')throw Error('map failed');},getMappedRange(){return new ArrayBuffer(size);},unmap(){}};buffers.push(buffer);return buffer;},
    createShaderModule(){return{};},createComputePipeline(){if(failure==='pipeline')throw Error('pipeline failed');return{getBindGroupLayout(){return{};}};},
    createBindGroup(){return{};},createCommandEncoder(){return{beginComputePass(){return pass;},copyBufferToBuffer(){},finish(){return{};}};}
  };
}
for(const kind of ['rays','room'])for(const failure of [null,'pipeline','map'])test(`${kind} destroys every GPU buffer on ${failure||'success'}`,async()=>{
  const d=device(failure);
  const run=()=>kind==='rays'?renderRays_GPU(d,Float32Array.of(0,1,0),1,1,8,1,Float32Array.of(1)):traceRoomIR_GPU(d,{sr:8000,irSeconds:.01,rays:8});
  if(failure)await assert.rejects(run(),new RegExp(failure));else await run();
  assert.ok(d.buffers.length>0); assert.ok(d.buffers.every(b=>b.destroyed===1));
});
