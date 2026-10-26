/* SPDX-License-Identifier: GPL-3.0-only */
'use strict';
const $ = id => document.getElementById(id);
const canvas=$('chart'), ctx=canvas.getContext('2d');
const layers=Object.fromEntries([...document.querySelectorAll('[data-layer]')].map(e=>[e.dataset.layer,e.checked]));
const clientId=crypto.randomUUID();
const project=([lon,lat])=>[lon,180/Math.PI*Math.log(Math.tan(Math.PI/4+lat*Math.PI/360))];
const unproject=([x,y])=>[x,360/Math.PI*Math.atan(Math.exp(y*Math.PI/180))-90];
let width=canvas.clientWidth||1,height=canvas.clientHeight||1,center=project([18.745,59.265]),scale=1,meta=null,features=[],hitPoints=[],timer,controller,sequence=0,drag=null;
let rendered=null,frame=null;
const polygonColors={0x10101:'#e6dfbd',0x10102:'#cfba8e',0x10301:'#b7ca9e',0x10302:'#95c9dc',0x10303:'#a8d2e1',0x10304:'#bddce8',0x10305:'#d1e5ec',0x10306:'#e1eef0',0x10307:'#eff4f2',0x10308:'#f2f6f3',0x10905:'#ded9c4',0x10614:'#f3f6f2'};
const polygonOrder=[0x4b,0x10106,0x10104,0x10614,0x10101,0x10102,0x10301,0x10302,0x10303,0x10304,0x10305,0x10306,0x10307,0x10308];
const number=n=>n.toLocaleString('sv-SE');
function screen(p){const [x,y]=project(p);return [(x-center[0])*scale+width/2,(center[1]-y)*scale+height/2]}
function coord(x,y){return unproject([center[0]+(x-width/2)/scale,center[1]-(y-height/2)/scale])}
function viewport(){const sw=coord(0,height),ne=coord(width,0);return [...sw,...ne]}
function detailBits(){return Math.max(18,Math.min(24,Math.round(Math.log2(scale)+10)))}
function fit(){if(!meta)return;const rect=canvas.getBoundingClientRect();width=rect.width;height=rect.height;const b=meta.initial_bbox,a=project(b.slice(0,2)),z=project(b.slice(2));center=[(a[0]+z[0])/2,(a[1]+z[1])/2];scale=Math.min(width/(z[0]-a[0]),height/(z[1]-a[1]))*.90;draw();schedule(0)}
function path(f){ctx.beginPath();f.projected.forEach((p,i)=>{const x=(p[0]-center[0])*scale+width/2,y=(center[1]-p[1])*scale+height/2;if(i)ctx.lineTo(x,y);else ctx.moveTo(x,y)})}
function label(text,x,y,color='#33565e',size=11){ctx.font=`${size}px Segoe UI, sans-serif`;ctx.textAlign='center';ctx.lineWidth=3;ctx.strokeStyle='#f5f7ed';ctx.strokeText(text,x,y);ctx.fillStyle=color;ctx.fillText(text,x,y)}
function draw(full=true){
 if(!full&&rendered){const ratio=window.devicePixelRatio||1,k=scale/rendered.scale;ctx.setTransform(ratio,0,0,ratio,0,0);ctx.fillStyle='#e5eff0';ctx.fillRect(0,0,width,height);const x=width/2-k*rendered.width/2+(rendered.center[0]-center[0])*scale,y=height/2-k*rendered.height/2+(center[1]-rendered.center[1])*scale;ctx.drawImage(rendered.canvas,x,y,rendered.width*k,rendered.height*k);$('detail').textContent=`Detaljnivå ${detailBits()} / 24`;return}
 const ratio=window.devicePixelRatio||1;ctx.setTransform(ratio,0,0,ratio,0,0);ctx.clearRect(0,0,width,height);ctx.fillStyle='#e5eff0';ctx.fillRect(0,0,width,height);hitPoints=[];
 const polygons=features.filter(f=>f.kind==='polygon').sort((a,b)=>{
  const ai=polygonOrder.indexOf(a.type),bi=polygonOrder.indexOf(b.type);return (ai<0?50:ai)-(bi<0?50:bi)
 });
 for(const f of polygons){
  const water=f.type>=0x10301&&f.type<=0x10308;
  if((water&&!layers.water)||(!water&&!layers.land))continue;
  const color=polygonColors[f.type];if(!color)continue;
  path(f);ctx.closePath();ctx.fillStyle=color;ctx.fill();
 }
 for(const f of features){if(f.kind!=='line')continue;
  const fishing=f.tile.startsWith('F'),depth=f.type>=0x10903&&f.type<=0x10905||f.type===0x10308||f.type===0x10301;
  if(fishing&&!layers.fishing||depth&&!layers.water||f.type===0x10101&&!layers.land)continue;
  path(f);ctx.lineWidth=f.type===0x10101?.85:.65;ctx.strokeStyle=depth?'#73a8bf':f.type===0x10101?'#766f57':f.type===0x1040c?'#ada79b':'#ad91af';ctx.setLineDash([0x10401,0x10402].includes(f.type)?[3,4]:[]);ctx.stroke();ctx.setLineDash([]);
 }
 const occupied=new Set();
 function textFree(text,x,y,color,size,cell=60){const key=`${Math.floor(x/cell)},${Math.floor(y/18)}`;if(occupied.has(key))return;occupied.add(key);label(text,x,y,color,size)}
 for(const f of features){if(f.kind!=='point')continue;const [x,y]=screen(f.coords[0]);if(x<0||x>width||y<0||y>height)continue;
  const depth=Number.isFinite(f.depth_m),buoy=f.type>=0x10200&&f.type<0x10300,light=f.type>=0x10100&&f.type<0x10200,text=f.type===0x10500;
  if(depth){if(!layers.depth)continue;textFree(String(f.depth_m),x,y,'#295d7a',10,36)}
  else if(text){if(layers.text&&f.label)textFree(f.label,x,y,'#5d6257',11);else continue}
  else{if(!layers.symbols)continue;
   if(buoy){ctx.beginPath();ctx.moveTo(x,y-5);ctx.lineTo(x+3,y);ctx.lineTo(x,y+5);ctx.lineTo(x-3,y);ctx.closePath();ctx.fillStyle='#faf6df';ctx.strokeStyle='#346955';ctx.lineWidth=1.3;ctx.fill();ctx.stroke()}
   else if(light){ctx.beginPath();ctx.arc(x,y,3,0,Math.PI*2);ctx.fillStyle='#c273aa';ctx.fill();ctx.beginPath();ctx.moveTo(x-5,y);ctx.lineTo(x+5,y);ctx.moveTo(x,y-5);ctx.lineTo(x,y+5);ctx.strokeStyle='#955987';ctx.lineWidth=1;ctx.stroke()}
   else if([0x10408,0x1040a,0x1040c].includes(f.type)){ctx.beginPath();ctx.moveTo(x-2,y-2);ctx.lineTo(x+2,y+2);ctx.moveTo(x+2,y-2);ctx.lineTo(x-2,y+2);ctx.strokeStyle='#5b6667';ctx.lineWidth=.9;ctx.stroke()}
   else if(f.type===0x10304){ctx.fillStyle='#8f8c80';ctx.fillRect(x-1.3,y-1.3,2.6,2.6)}
   else{ctx.beginPath();ctx.arc(x,y,1.6,0,Math.PI*2);ctx.fillStyle='#737c75';ctx.fill()}
  }
  hitPoints.push({x,y,f});
 }
 if(meta){const b=meta.test_bounds,a=screen([b[0],b[3]]),z=screen([b[2],b[1]]);ctx.strokeStyle='#537f8060';ctx.lineWidth=1;ctx.setLineDash([6,6]);ctx.strokeRect(a[0],a[1],z[0]-a[0],z[1]-a[1]);ctx.setLineDash([])}
 const lat=unproject(center)[1],metersPerPixel=111320*Math.cos(lat*Math.PI/180)/scale;
 const target=metersPerPixel*100,power=10**Math.floor(Math.log10(target)),m=[1,2,5,10].map(n=>n*power).reduce((a,b)=>Math.abs(a-target)<Math.abs(b-target)?a:b);
 $('scale').style.width=`${m/metersPerPixel}px`;$('scale').textContent=m>=1000?`${m/1000} km`:`${m} m`;
 $('detail').textContent=`Detaljnivå ${detailBits()} / 24`;
 const bitmap=document.createElement('canvas');bitmap.width=canvas.width;bitmap.height=canvas.height;bitmap.getContext('2d').drawImage(canvas,0,0);rendered={canvas:bitmap,center:[...center],scale,width,height};
}
function preview(){if(frame===null)frame=requestAnimationFrame(()=>{frame=null;draw(false)})}
async function load(){
 if(!meta)return;const request=++sequence;if(controller)controller.abort();controller=new AbortController();$('status').textContent='Läser kartdelar…';
 try{const response=await fetch(`/api/features?bbox=${viewport().join(',')}&bits=${detailBits()}&fishing=${layers.fishing?1:0}&compact=1&client=${clientId}&generation=${request}`,{signal:controller.signal});const data=await response.json();if(request!==sequence)return;if(!response.ok)throw Error(data.error);
  features=data.features;for(const f of features)f.projected=f.coords.map(project);draw();$('empty').hidden=features.length>0;
  const s=data.stats;$('status').textContent=data.warnings.length?'Visas med begränsningar':'Kartdata inläst';
  $('stats').textContent=s.features===undefined?'Utanför testområdet.':`${number(s.features)} objekt · ${number(s.subdivisions)} kartdelar\n${number(Math.round(s.bytes_read/1024))} kB läst · ${s.seconds.toFixed(2)} s`;
  $('stats').style.whiteSpace='pre-line';$('error-count').textContent=data.warnings.length?`${data.warnings.length} avkodningsanmärkningar`:'Inga avkodningsfel i utsnittet';$('error-list').textContent=data.warnings.join('\n');
 }catch(e){if(e.name==='AbortError'||request!==sequence)return;$('status').textContent='Kunde inte läsa kartan';$('error-count').textContent='Fel';$('error-list').textContent=e.message;features=[];draw()}
}
function schedule(delay=180){clearTimeout(timer);timer=setTimeout(load,delay)}
function zoom(factor,x=width/2,y=height/2){const before=coord(x,y);scale=Math.max(900,Math.min(1400000,scale*factor));const after=coord(x,y),a=project(before),b=project(after);center[0]+=a[0]-b[0];center[1]+=a[1]-b[1];preview();schedule()}
canvas.addEventListener('wheel',e=>{e.preventDefault();const r=canvas.getBoundingClientRect();zoom(Math.exp(-e.deltaY*.0015),e.clientX-r.left,e.clientY-r.top)},{passive:false});
canvas.addEventListener('pointerdown',e=>{if(e.button!==0)return;drag={x:e.clientX,y:e.clientY,center:[...center]};canvas.setPointerCapture(e.pointerId);$('tooltip').hidden=true});
canvas.addEventListener('pointermove',e=>{const r=canvas.getBoundingClientRect(),x=e.clientX-r.left,y=e.clientY-r.top;const p=coord(x,y);$('coords').textContent=`${p[1].toFixed(5)}° N  ${p[0].toFixed(5)}° E`;
 if(drag){center=[drag.center[0]-(e.clientX-drag.x)/scale,drag.center[1]+(e.clientY-drag.y)/scale];center[1]=Math.max(-175,Math.min(175,center[1]));preview();return}
 let best=null,distance=9;for(const hit of hitPoints){const d=Math.hypot(x-hit.x,y-hit.y);if(d<distance){best=hit;distance=d}}
 const tip=$('tooltip');tip.hidden=!best;if(!best)return;const f=best.f;
 const title=f.depth_m!==undefined?`Djup: ${f.depth_m} m`:f.label||({'buoy':'Sjömärke','light':'Fyr / ljus'}[f.symbol_detail])||'Kartobjekt';
 tip.replaceChildren();const strong=document.createElement('strong');strong.textContent=title;tip.append(strong,document.createTextNode(`Typ 0x${f.type.toString(16)} · ${f.coords[0][1].toFixed(5)}° N, ${f.coords[0][0].toFixed(5)}° E`));tip.style.left=`${Math.max(8,Math.min(width-260,x+14))}px`;tip.style.top=`${Math.max(8,Math.min(height-90,y+14))}px`;
});
function endDrag(){if(drag){drag=null;schedule(0)}}
canvas.addEventListener('pointerup',endDrag);canvas.addEventListener('pointercancel',endDrag);canvas.addEventListener('lostpointercapture',endDrag);canvas.addEventListener('pointerleave',()=>{$('tooltip').hidden=true});
canvas.addEventListener('keydown',e=>{const steps={ArrowLeft:[-1,0],ArrowRight:[1,0],ArrowUp:[0,1],ArrowDown:[0,-1]};if(steps[e.key]){e.preventDefault();center[0]+=steps[e.key][0]*width/scale*.15;center[1]+=steps[e.key][1]*height/scale*.15;draw();schedule()}else if(e.key==='+'||e.key==='=')zoom(1.5);else if(e.key==='-')zoom(1/1.5);else if(e.key==='Home')fit()});
$('zoom-in').onclick=()=>zoom(1.5);$('zoom-out').onclick=()=>zoom(1/1.5);$('reset').onclick=fit;$('return').onclick=fit;
document.querySelectorAll('[data-layer]').forEach(e=>e.onchange=()=>{layers[e.dataset.layer]=e.checked;if(e.dataset.layer==='fishing'){schedule(0)}else draw()});
new ResizeObserver(()=>{const r=canvas.getBoundingClientRect();width=r.width;height=r.height;const d=window.devicePixelRatio||1;canvas.width=Math.round(width*d);canvas.height=Math.round(height*d);draw();schedule()}).observe(canvas);
(async()=>{try{const response=await fetch('/api/metadata');if(!response.ok)throw Error('Metadata kunde inte läsas');meta=await response.json();$('source').textContent=meta.title;for(const text of meta.limitations){const li=document.createElement('li');li.textContent=text;$('limitations').append(li)}fit()}catch(e){$('status').textContent='Startfel';$('source').textContent=e.message}})();
