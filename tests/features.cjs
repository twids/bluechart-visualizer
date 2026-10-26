// Synthetic taxonomy and picking regressions; contains no chart content.
const assert=require('node:assert/strict');
const {describe,layer,HitIndex}=require('../web/features.js');
const feature=(kind,type)=>({kind,type});
assert.equal(describe(feature('point',0x10409)).name,'Marine farm');
assert.equal(describe(feature('polygon',0x10409)).name,'Danger area');
assert.equal(describe(feature('line',0x10402)).name,'Submarine pipeline');
assert.equal(describe(feature('point',0x10402)).name,'Wreck');
assert.equal(describe(feature('polygon',0x10301)).name,'Intertidal zone');
for(const [kind,type] of [['point',0x10802],['point',0x11200],['polygon',0x10310],['polygon',0x10614],['line',0x10616]]){
 const info=describe(feature(kind,type));assert.equal(info.known,false);assert.equal(info.code,`0x${type.toString(16)}`);
}
assert.equal(layer(feature('polygon',0x10507)),'symbols');
assert.equal(layer(feature('line',0x10309)),'water');
assert.equal(layer(feature('point',0x10600)),'text');
assert.equal(layer(feature('line',0x10106)),'symbols','overhead cables are not hidden with land');
assert.equal(layer(feature('polygon',0x10301)),'water');
const index=new HitIndex(),area=feature('polygon',0x10507),line=feature('line',0x10402),point=feature('point',0x1040a);
index.add(area,[[0,0],[200,0],[200,200],[0,200]],true);
assert.equal(index.pick(100,100).f,area);
index.add(line,[[0,100],[200,100]]);
assert.equal(index.pick(100,104).f,line);
assert.deepEqual(index.pick(100,104).position,[100,100]);
index.add(point,[[100,100]]);
assert.equal(index.pick(104,104).f,point,'points win over lines and areas');
assert.equal(index.pick(250,100),null);
index.clear();assert.equal(index.pick(100,100),null,'clearing removes stale geometry');
index.add(area,[[0,0],[200,0],[200,200],[0,200]],false);
assert.equal(index.pick(100,100),null,'outline-only areas do not claim their interior');
assert.equal(index.pick(100,2).f,area);
index.clear();index.add(area,[[-100000,-100000],[100000,-100000],[100000,100000],[-100000,100000]],true);
assert.equal(index.pick(100,100).f,area,'large geometry uses the bounded grid fallback');
index.clear();index.add(feature('polygon',0x4b),[[0,0],[200,0],[200,200],[0,200]],true);
assert.equal(index.pick(100,100),null,'chart container backgrounds are not selectable');
console.log('Feature taxonomy and geometry picking checks passed');
