import { spawnSync } from 'node:child_process';
import { readdirSync, mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
process.chdir(fileURLToPath(new URL('.',import.meta.url)));
function run(command,args){const r=spawnSync(command,args,{stdio:'inherit'});if(r.error)throw r.error;if(r.status!==0)process.exit(r.status||1);}
mkdirSync('.build',{recursive:true});
const flags=['--edition','2021','--target','wasm32-unknown-unknown','-O','-C','panic=abort','-C','lto=fat'];
run('rustc',[...flags,'--crate-name','raydrone_core','--crate-type=lib','core/src/lib.rs','-o','.build/libraydrone_core.rlib']);
run('rustc',[...flags,'--extern','raydrone_core=.build/libraydrone_core.rlib','--crate-type=cdylib','raydrone.rs','-o','raydrone.wasm']);
run('rustc',[...flags,'--extern','raydrone_core=.build/libraydrone_core.rlib','--crate-type=cdylib','tests/core_bridge.rs','-o','.build/core-tests.wasm']);
for(const file of ['test_engine.mjs','test_acoustic.mjs','test_raytrace_buffer.mjs'])run(process.execPath,[file]);
run(process.execPath,['--test',...readdirSync('tests').filter(f=>f.endsWith('.test.mjs')).sort().map(f=>'tests/'+f)]);
