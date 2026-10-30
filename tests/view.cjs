/* SPDX-License-Identifier: GPL-3.0-only */
'use strict';
const assert=require('node:assert/strict');
const {Camera}=require('../web/view.js');
const close=(a,b)=>assert(Math.hypot(a[0]-b[0],a[1]-b[1])<1e-7,`${a} != ${b}`);
for(const [width,height] of [[1200,800],[250,700],[1600,300],[120,20]])for(const pitch of [0,15,45,60]){
 const c=new Camera({width,height,center:[18,74],scale:1000,pitch});
 for(const p of [[0,0],[width,0],[width,height],[0,height],[width*.2,height*.7]])close(c.screen(c.ground(...p)),p);
 const bounds=c.bounds();
 for(const p of [[0,0],[width,0],[width,height],[0,height]]){
  const q=c.ground(...p);assert(q[0]>=bounds[0]&&q[0]<=bounds[2]&&q[1]>=bounds[1]&&q[1]<=bounds[3]);
 }
 // A polygon extending through the camera must still cover the visible map.
 const polygon=c.paths([[-1e6,-1e6],[1e6,-1e6],[1e6,1e6],[-1e6,1e6]],true);
 assert.equal(polygon.length,1);assert(polygon[0].flat().every(Number.isFinite));
 const minX=Math.min(...polygon[0].map(p=>p[0])),maxX=Math.max(...polygon[0].map(p=>p[0]));assert(minX<0&&maxX>width);
 assert.equal(c.paths([[1e7,1e7],[2e7,2e7]]).length,0,'offscreen line omitted');
 const line=c.paths([c.ground(-100,height/2),c.ground(width+100,height/2)]);
 assert.equal(line.length,1);assert(line[0].flat().every(Number.isFinite));
 // Leaving and re-entering the clip box must not invent a connecting segment.
 const clip=c.bounds(Math.min(12,height*.1)),outsideX=clip[2]+(clip[2]-clip[0]),outsideY=clip[1]-(clip[3]-clip[1]);
 const separated=c.paths([c.center,[outsideX,c.center[1]],[outsideX,outsideY],c.center]);
 assert.equal(separated.length,2);
 if(pitch===0)close(c.screen([18.1,74.1]),[width/2+100,height/2-100]);
}
console.log('Perspective camera checks passed: inversion, portrait/landscape, bounds, camera-plane clipping and disconnected lines');
