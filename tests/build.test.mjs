import test from 'node:test';
import assert from 'node:assert/strict';
import { cpSync, mkdtempSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { resolve, dirname, basename, join } from 'node:path';
import { spawnSync } from 'node:child_process';
test('WASM builds from an isolated source checkout with no external core',()=>{
  const build=resolve('.build');mkdirSync(build,{recursive:true});
  const dir=mkdtempSync(join(build,'clean-checkout-'));
  try {
    for(const file of ['core','raydrone.rs','build.ps1','build.sh'])cpSync(file,join(dir,file),{recursive:true});
    const command=process.platform==='win32'?'powershell.exe':'bash';
    const args=process.platform==='win32'?['-NoProfile','-ExecutionPolicy','Bypass','-File','build.ps1']:['build.sh'];
    const result=spawnSync(command,args,{cwd:dir,encoding:'utf8',timeout:120000});
    assert.equal(result.status,0,String(result.error||result.stderr));
    const ex=new WebAssembly.Instance(new WebAssembly.Module(readFileSync(join(dir,'raydrone.wasm'))),{}).exports;
    assert.equal(ex.ray_population(),2000);assert.equal(ex.block_capacity(),256);
  } finally {
    if(dirname(dir)!==build||!basename(dir).startsWith('clean-checkout-'))throw Error('unsafe temporary path');
    rmSync(dir,{recursive:true,force:true});
  }
});
