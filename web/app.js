/* SPDX-License-Identifier: GPL-3.0-only */
'use strict';
const $ = id => document.getElementById(id);
const canvas=$('chart'), ctx=canvas.getContext('2d');
const layers=Object.fromEntries([...document.querySelectorAll('[data-layer]')].map(e=>[e.dataset.layer,e.checked]));
const clientId=crypto.randomUUID?.()||Array.from(crypto.getRandomValues(new Uint8Array(16)),b=>b.toString(16).padStart(2,'0')).join('');
const project=([lon,lat])=>[lon,180/Math.PI*Math.log(Math.tan(Math.PI/4+lat*Math.PI/360))];
const unproject=([x,y])=>[x,360/Math.PI*Math.atan(Math.exp(y*Math.PI/180))-90];
let width=canvas.clientWidth||1,height=canvas.clientHeight||1,center=project([0,0]),scale=1,meta=null,features=[],timer,controller,sequence=0,drag=null;
const hits=new ChartFeatures.HitIndex();
let rendered=null,frame=null,viewReady=false;
let selectedChart='',catalog=[],metadataSequence=0,selectedOverlays=new Set();
let searchTimer,searchController,searchSequence=0,searchResults=[],searchActive=-1,searchPlace=null,searchMode="instant",searchMessage="";
const searchInput=$('place-query'),searchList=$('search-results');
const searchMarker=document.createElement('div');searchMarker.id='search-marker';searchMarker.hidden=true;$('map').append(searchMarker);
function positionSearchMarker(){
 if(!searchPlace){searchMarker.hidden=true;return}
 const [x,y]=screen(searchPlace.coordinates);searchMarker.hidden=x<0||x>width||y<0||y>height;
 searchMarker.style.left=`${x}px`;searchMarker.style.top=`${y}px`;
}
function hideSuggestions(){ $('search-suggestions').hidden=true;searchInput.setAttribute('aria-expanded','false');searchInput.removeAttribute('aria-activedescendant');searchActive=-1 }
function closeResults(){ $('search-panel').hidden=true;$('chart-settings').hidden=false;searchMode='instant' }
function dismissResults(){hideSuggestions();searchResults=[];searchActive=-1;searchList.replaceChildren();searchList.hidden=true;searchInput.setAttribute('aria-expanded','false');searchInput.removeAttribute('aria-activedescendant')}
function cancelSearch(){clearTimeout(searchTimer);++searchSequence;searchController?.abort();searchInput.setAttribute('aria-busy','false')}
function clearSearch(){cancelSearch();dismissResults();closeResults();$('full-search-results').replaceChildren();searchInput.value='';searchPlace=null;positionSearchMarker();$('search-status').textContent='Enter at least two characters.'}
function activateResult(index){
 searchActive=index;
 [...searchList.children].forEach((node,i)=>node.setAttribute('aria-selected',String(i===index)));
 const node=searchList.children[index];if(node){searchInput.setAttribute('aria-activedescendant',node.id);node.scrollIntoView({block:'nearest'})}
}
function chooseResult(index){
 const place=searchResults[index];if(!place||!meta)return;
 cancelSearch();searchPlace=place;searchInput.value=place.label;searchMarker.textContent=place.label;if(searchMode==='instant')dismissResults();else{hideSuggestions();[...$('full-search-results').children].forEach((li,i)=>li.querySelector('button').setAttribute('aria-current',String(i===index)))}
 const b=place.bounds,dx=Math.max(.025,b[2]-b[0]),dy=Math.max(.015,b[3]-b[1]);
 const [lon,lat]=place.coordinates;
 fitBounds([Math.max(-180,lon-dx*.6),Math.max(-85,lat-dy*.6),Math.min(180,lon+dx*.6),Math.min(85,lat+dy*.6)]);
 $('search-status').textContent=`Selected ${place.label} · ${place.chart_title}`;
 canvas.focus({preventScroll:true});
}
async function searchPlaces(request,text,chart){
 if(request!==searchSequence)return;
 searchController=new AbortController();searchInput.setAttribute('aria-busy','true');$('search-status').textContent='Searching chart names…';
 try{
  const response=await fetch(`/api/search?chart=${encodeURIComponent(chart)}&q=${encodeURIComponent(text)}`,{signal:searchController.signal});
  const data=await response.json();if(request!==searchSequence)return;if(!response.ok)throw Error(data.error||'Search failed');
  searchResults=data.results;searchMessage=[searchResults.length?`${searchResults.length} results${searchResults.length===20?' (first 20)':''}.`:'No matching places.',...(data.warnings||[])].join(' ');
  renderSearchResults(request,text);

 }catch(e){if(request!==searchSequence||e.name==='AbortError')return;dismissResults();searchMessage=`Could not search: ${e.message}`;$('search-status').textContent=searchMessage;if(searchMode==='full'){$('results-summary').textContent=searchMessage;$('full-search-results').replaceChildren()}}
 finally{if(request===searchSequence)searchInput.setAttribute('aria-busy','false')}
}
function renderSearchResults(request,text){
 searchActive=-1;searchInput.removeAttribute('aria-activedescendant');searchList.replaceChildren();$('full-search-results').replaceChildren();
 searchResults.forEach((place,i)=>{
  const name=document.createElement('strong');name.textContent=place.label;
  const detail=document.createElement('small');detail.textContent=`${place.chart_title} · ${place.coordinates[1].toFixed(5)}°, ${place.coordinates[0].toFixed(5)}°`;
  const li=document.createElement('li');
  if(searchMode==='full'){
   const button=document.createElement('button');button.type='button';button.append(name,detail);button.onclick=()=>chooseResult(i);li.append(button);$('full-search-results').append(li);
  }else{
   li.id=`place-result-${request}-${i}`;li.setAttribute('role','option');li.setAttribute('aria-selected','false');li.append(name,detail);li.onpointerdown=e=>e.preventDefault();li.onclick=()=>chooseResult(i);searchList.append(li);
  }
 });
 const instant=searchMode==='instant'&&document.activeElement===searchInput&&searchResults.length>0;
 searchList.hidden=!instant;$('search-suggestions').hidden=!instant;searchInput.setAttribute('aria-expanded',String(instant));
 $('search-status').textContent=searchMessage;
 if(searchMode==='full'){$('results-heading').textContent=`Results for “${text}”`;$('results-summary').textContent=searchMessage}
}
function submitSearch(){
 const text=searchInput.value.trim();if([...text].length<2){$('search-status').textContent='Enter at least two characters.';searchInput.focus();return}
 cancelSearch();hideSuggestions();searchMode='full';$('search-panel').hidden=false;$('chart-settings').hidden=true;
 $('results-heading').textContent=`Results for “${text}”`;$('results-summary').textContent='Searching chart names…';$('full-search-results').replaceChildren();
 searchPlaces(searchSequence,text,selectedChart);
}
searchInput.oninput=()=>{
 cancelSearch();dismissResults();closeResults();const text=searchInput.value.trim();
 if(!text){searchPlace=null;positionSearchMarker()}
 if([...text].length<2){$('search-status').textContent='Enter at least two characters.';return}
 $('search-status').textContent='Waiting to search…';const request=searchSequence,chart=selectedChart;
 searchTimer=setTimeout(()=>searchPlaces(request,text,chart),250);
};
searchInput.onkeydown=e=>{
 if(e.key==='Escape'){e.preventDefault();if(!$('search-suggestions').hidden)hideSuggestions();else clearSearch();return}
 if(e.key==='Enter'){e.preventDefault();if(searchActive>=0)chooseResult(searchActive);else submitSearch();return}
 if(!searchResults.length||searchMode!=='instant')return;
 if(e.key==='ArrowDown'||e.key==='ArrowUp'){e.preventDefault();$('search-suggestions').hidden=false;searchList.hidden=false;searchInput.setAttribute('aria-expanded','true');activateResult((searchActive+(e.key==='ArrowDown'?1:searchActive<0?0:-1)+searchResults.length)%searchResults.length)}
};
searchInput.onfocus=()=>{if(searchMode==='instant'&&searchResults.length)renderSearchResults(searchSequence,searchInput.value.trim())};
searchInput.onblur=()=>hideSuggestions();
$('show-all-results').onpointerdown=e=>e.preventDefault();
$('show-all-results').onclick=submitSearch;
document.querySelector('.place-search').onsubmit=e=>{e.preventDefault();submitSearch()};
$('close-results').onclick=()=>{cancelSearch();dismissResults();closeResults();searchInput.focus()};
$('search-clear').onclick=()=>{clearSearch();searchInput.focus()};
const hiddenFeatureTypes=new Set([0x10614]);
const polygonColors={0x10101:'#e6dfbd',0x10102:'#cfba8e',0x10301:'#b7ca9e',0x10302:'#95c9dc',0x10303:'#a8d2e1',0x10304:'#bddce8',0x10305:'#d1e5ec',0x10306:'#e1eef0',0x10307:'#eff4f2',0x10308:'#f2f6f3',0x10905:'#ded9c4'};
const polygonOrder=[0x4b,0x10106,0x10104,0x10101,0x10102,0x10301,0x10302,0x10303,0x10304,0x10305,0x10306,0x10307,0x10308];
const number=n=>n.toLocaleString('en-US');
function screen(p){const [x,y]=project(p);return [(x-center[0])*scale+width/2,(center[1]-y)*scale+height/2]}
function coord(x,y){return unproject([center[0]+(x-width/2)/scale,center[1]-(y-height/2)/scale])}
function viewport(){const sw=coord(0,height),ne=coord(width,0);return [Math.max(-180,sw[0]),Math.max(-85,sw[1]),Math.min(180,ne[0]),Math.min(85,ne[1])]}
function detailBits(){return Math.max(11,Math.min(24,Math.round(Math.log2(scale)+10)))}
function fitBounds(b){if(!meta)return;const rect=canvas.getBoundingClientRect();width=rect.width;height=rect.height;const a=project(b.slice(0,2)),z=project(b.slice(2));center=[(a[0]+z[0])/2,(a[1]+z[1])/2];scale=Math.min(width/(z[0]-a[0]),height/(z[1]-a[1]))*.90;viewReady=true;draw();schedule(0)}
function fit(){if(meta)fitBounds(meta.initial_bbox)}
function fitCoverage(){if(meta)fitBounds(meta.coverage_bbox)}
function path(f){const points=f.projected.map(p=>[(p[0]-center[0])*scale+width/2,(center[1]-p[1])*scale+height/2]);ctx.beginPath();points.forEach(([x,y],i)=>{if(i)ctx.lineTo(x,y);else ctx.moveTo(x,y)});return points}
function label(text,x,y,color='#33565e',size=11){ctx.font=`${size}px Segoe UI, sans-serif`;ctx.textAlign='center';ctx.lineWidth=3;ctx.strokeStyle='#f5f7ed';ctx.strokeText(text,x,y);ctx.fillStyle=color;ctx.fillText(text,x,y)}
function symbol(f,x,y){
 ctx.save();ctx.translate(x,y);ctx.lineWidth=1.2;ctx.strokeStyle='#4f6260';ctx.fillStyle='#fffdf0';ctx.beginPath();
 if(f.type>=0x10200&&f.type<0x10300){
  if(f.type===0x10207){ctx.moveTo(-2,4);ctx.lineTo(2,-5);ctx.moveTo(-4,5);ctx.lineTo(4,5)}
  else if(f.type>=0x1020e&&f.type<=0x10215||f.type===0x10206){ctx.moveTo(-4,4);ctx.lineTo(0,-5);ctx.lineTo(4,4);ctx.closePath();ctx.fill()}
  else{ctx.moveTo(0,-5);ctx.lineTo(3,0);ctx.lineTo(0,5);ctx.lineTo(-3,0);ctx.closePath();ctx.fill()}
  ctx.stroke();
 }else if(f.type>=0x10100&&f.type<0x10200){
  ctx.strokeStyle='#955987';ctx.fillStyle='#c273aa';ctx.arc(0,0,3,0,Math.PI*2);ctx.fill();ctx.moveTo(-6,0);ctx.lineTo(6,0);ctx.moveTo(0,-6);ctx.lineTo(0,6);ctx.stroke();
 }else if([0x10402,0x10403].includes(f.type)){
  ctx.ellipse(0,0,5,2.5,0,0,Math.PI*2);ctx.moveTo(-6,-4);ctx.lineTo(6,4);ctx.moveTo(-6,4);ctx.lineTo(6,-4);ctx.stroke();
 }else if(f.type>=0x10400&&f.type<=0x1040f){
  ctx.moveTo(-3,-3);ctx.lineTo(3,3);ctx.moveTo(3,-3);ctx.lineTo(-3,3);ctx.stroke();
  if(f.type!==0x1040c){ctx.beginPath();ctx.setLineDash([1,2]);ctx.arc(0,0,5,0,Math.PI*2);ctx.stroke()}
 }else if(f.type===0x10304||f.type===0x10307){
  if(f.type===0x10307)ctx.arc(0,0,3,0,Math.PI*2);else ctx.rect(-3,-3,6,6);ctx.fillStyle='#9b9788';ctx.fill();ctx.stroke();
 }else if([0x10305,0x10306,0x10308,0x1030b].includes(f.type)){
  ctx.moveTo(-3,4);ctx.lineTo(0,-5);ctx.lineTo(3,4);ctx.moveTo(-4,-2);ctx.lineTo(4,-2);ctx.stroke();
 }else{
  ctx.arc(0,0,ChartFeatures.describe(f).known?2.5:3.5,0,Math.PI*2);ctx.fill();ctx.stroke();
 }
 ctx.restore();
}
function draw(full=true){
 positionSearchMarker();
 if(!full&&rendered){const ratio=window.devicePixelRatio||1,k=scale/rendered.scale;ctx.setTransform(ratio,0,0,ratio,0,0);ctx.fillStyle='#e5eff0';ctx.fillRect(0,0,width,height);const x=width/2-k*rendered.width/2+(rendered.center[0]-center[0])*scale,y=height/2-k*rendered.height/2+(center[1]-rendered.center[1])*scale;ctx.drawImage(rendered.canvas,x,y,rendered.width*k,rendered.height*k);$('detail').textContent=`Detail level ${detailBits()} / 24`;return}
 const ratio=window.devicePixelRatio||1;ctx.setTransform(ratio,0,0,ratio,0,0);ctx.clearRect(0,0,width,height);ctx.fillStyle='#e5eff0';ctx.fillRect(0,0,width,height);hits.clear();$('tooltip').hidden=true;
 const polygons=features.filter(f=>f.kind==='polygon').sort((a,b)=>{
  const ai=polygonOrder.indexOf(a.type),bi=polygonOrder.indexOf(b.type);return (ai<0?50:ai)-(bi<0?50:bi)
 });
 for(const f of polygons){
  if(hiddenFeatureTypes.has(f.type)||!layers[ChartFeatures.layer(f)])continue;
  // Backgrounds are container geometry, not selectable chart objects.
  if(ChartFeatures.backgrounds.has(f.type))continue;
  const color=polygonColors[f.type],points=path(f);ctx.closePath();
  if(color){ctx.fillStyle=color;ctx.fill();hits.add(f,points,true)}
  else{const known=ChartFeatures.describe(f).known;ctx.save();if(known){ctx.fillStyle='#a2779d0d';ctx.fill()}ctx.strokeStyle=known?'#a2779d':'#7c8581';ctx.lineWidth=.8;ctx.setLineDash([5,4]);ctx.stroke();ctx.restore();hits.add(f,points,known)}
 }
 for(const f of features){if(f.kind!=='line')continue;
  if(hiddenFeatureTypes.has(f.type)||f.tile?.startsWith('F')&&!layers.fishing||!layers[ChartFeatures.layer(f)])continue;
  const points=path(f),style=ChartFeatures.lineStyle(f);ctx.lineWidth=style.width;ctx.strokeStyle=style.color;ctx.setLineDash(style.dash);ctx.stroke();ctx.setLineDash([]);hits.add(f,points);
 }
 const occupied=new Set();
 function textFree(text,x,y,color,size,cell=60){const key=`${Math.floor(x/cell)},${Math.floor(y/18)}`;if(occupied.has(key))return false;occupied.add(key);label(text,x,y,color,size);return true}
 for(const f of features){if(f.kind!=='point'||hiddenFeatureTypes.has(f.type))continue;const [x,y]=screen(f.coords[0]);if(x<0||x>width||y<0||y>height)continue;
  const depth=Number.isFinite(f.depth_m),sounding=f.type>=0x10300&&f.type<=0x10303,text=[0x10500,0x10600].includes(f.type);
  if(sounding){if(!layers.depth)continue;if(depth){if(!textFree(String(f.depth_m),x,y,'#295d7a',10,36))continue}else symbol(f,x,y)}
  else if(text){if(!layers.text||!f.label||!textFree(f.label,x,y,'#5d6257',11))continue}
  else{if(!layers.symbols&&!(depth&&layers.depth))continue;
   if(layers.symbols)symbol(f,x,y);
   if(depth&&layers.depth)textFree(String(f.depth_m),x+9,y+12,'#295d7a',10,36);
  }
  hits.add(f,[[x,y]]);
 }
 if(meta){const b=meta.coverage_bounds,a=screen([b[0],b[3]]),z=screen([b[2],b[1]]);ctx.strokeStyle='#537f8060';ctx.lineWidth=1;ctx.setLineDash([6,6]);ctx.strokeRect(a[0],a[1],z[0]-a[0],z[1]-a[1]);ctx.setLineDash([])}
 const lat=unproject(center)[1],metersPerPixel=111320*Math.cos(lat*Math.PI/180)/scale;
 const target=metersPerPixel*100,power=10**Math.floor(Math.log10(target)),m=[1,2,5,10].map(n=>n*power).reduce((a,b)=>Math.abs(a-target)<Math.abs(b-target)?a:b);
 $('scale').style.width=`${m/metersPerPixel}px`;$('scale').textContent=m>=1000?`${m/1000} km`:`${m} m`;
 $('detail').textContent=`Detail level ${detailBits()} / 24`;
 const bitmap=document.createElement('canvas');bitmap.width=canvas.width;bitmap.height=canvas.height;bitmap.getContext('2d').drawImage(canvas,0,0);rendered={canvas:bitmap,center:[...center],scale,width,height};
}
function preview(){if(frame===null)frame=requestAnimationFrame(()=>{frame=null;draw(false)})}
function setLoading(active){$('map-loading').hidden=!active;canvas.setAttribute('aria-busy',String(active))}
function pending(){++sequence;if(controller)controller.abort();hits.clear();$('tooltip').hidden=true;setLoading(true)}
async function load(){
 if(!meta)return;const request=++sequence;if(controller)controller.abort();controller=new AbortController();setLoading(true);$('status').textContent='Reading chart tiles…';
 try{const response=await fetch(`/api/features?chart=${encodeURIComponent(selectedChart)}&overlays=${encodeURIComponent([...selectedOverlays].join(','))}&bbox=${viewport().join(',')}&bits=${detailBits()}&fishing=${layers.fishing?1:0}&compact=1&client=${clientId}&generation=${request}`,{signal:controller.signal});const data=await response.json();if(request!==sequence)return;if(!response.ok)throw Error(data.error);
  $('active-chart').textContent=data.chart_title?`Active: ${data.chart_title}`:'';
  features=data.features;for(const f of features)f.projected=f.coords.map(project);draw();$('empty').hidden=features.length>0;
  const s=data.stats;if(s.effective_bits!==undefined)$('detail').textContent=`Detail level ${s.effective_bits} / 24`;$('status').textContent=data.warnings.length?'Showing with limitations':'Chart loaded';
  $('stats').textContent=s.features===undefined?'Outside chart coverage.':`${number(s.features)} features · ${s.overview_cache?'local overview cache':`${number(s.subdivisions)} tiles`}\n${number(Math.round(s.bytes_read/1024))} kB read · ${s.seconds.toFixed(2)} s`;
  $('stats').style.whiteSpace='pre-line';$('error-count').textContent=data.warnings.length?`${data.warnings.length} decoding warnings`:'No decoding errors in this view';$('error-list').textContent=data.warnings.join('\n');
 }catch(e){if(e.name==='AbortError'||request!==sequence)return;$('status').textContent='Could not load chart';$('error-count').textContent='Error';$('error-list').textContent=e.message;features=[];draw()}
 finally{if(request===sequence&&!(drag&&drag.moved))setLoading(false)}
}
function schedule(delay=180){pending();clearTimeout(timer);timer=setTimeout(load,delay)}
function zoom(factor,x=width/2,y=height/2){const before=coord(x,y);scale=Math.max(3,Math.min(1400000,scale*factor));const after=coord(x,y),a=project(before),b=project(after);center[0]+=a[0]-b[0];center[1]+=a[1]-b[1];preview();schedule()}
canvas.addEventListener('wheel',e=>{e.preventDefault();const r=canvas.getBoundingClientRect();zoom(Math.exp(-e.deltaY*.0015),e.clientX-r.left,e.clientY-r.top)},{passive:false});
canvas.addEventListener('dblclick',e=>{const r=canvas.getBoundingClientRect();zoom(3,e.clientX-r.left,e.clientY-r.top)});
canvas.addEventListener('pointerdown',e=>{if(e.button!==0||e.isPrimary===false||drag)return;e.preventDefault();canvas.focus({preventScroll:true});drag={pointerId:e.pointerId,moved:false,x:e.clientX,y:e.clientY,center:[...center]};canvas.setPointerCapture(e.pointerId);$('tooltip').hidden=true});
function inspectAt(x,y){
 const best=hits.pick(x,y),tip=$('tooltip');tip.hidden=!best;if(!best)return;
 const f=best.f,info=ChartFeatures.describe(f),p=f.kind==='point'?f.coords[0]:coord(...best.position);
 const rows=[info.name];if(f.label)rows.push(f.label);
 if(Number.isFinite(f.depth_m))rows.push(`${f.kind==='point'&&f.type===0x10303?'Height':'Depth'}: ${f.depth_m} m`);
 if(!info.known)rows.push('Meaning not yet identified');
 rows.push(`${info.kind} · Type ${info.code}`);
 rows.push(`${Math.abs(p[1]).toFixed(5)}° ${p[1]<0?'S':'N'}, ${Math.abs(p[0]).toFixed(5)}° ${p[0]<0?'W':'E'}`);
 tip.replaceChildren();rows.forEach((text,i)=>{const row=document.createElement(i===0?'strong':'div');row.textContent=text;tip.append(row)});
 tip.style.maxWidth=`${Math.max(80,Math.min(280,width-24))}px`;
 tip.style.left=`${Math.max(8,Math.min(width-tip.offsetWidth-8,x+14))}px`;tip.style.top=`${Math.max(8,Math.min(height-tip.offsetHeight-8,y+14))}px`;
}
canvas.addEventListener('pointermove',e=>{const r=canvas.getBoundingClientRect(),x=e.clientX-r.left,y=e.clientY-r.top;const p=coord(x,y);$('coords').textContent=`${p[1].toFixed(5)}° N  ${p[0].toFixed(5)}° E`;
 if(drag){if(e.pointerId!==drag.pointerId)return;const dx=e.clientX-drag.x,dy=e.clientY-drag.y;if(!drag.moved&&Math.hypot(dx,dy)<6)return;e.preventDefault();drag.moved=true;pending();clearTimeout(timer);center=[drag.center[0]-dx/scale,drag.center[1]+dy/scale];center[1]=Math.max(-175,Math.min(175,center[1]));preview();return}
 inspectAt(x,y);
});
function endDrag(e){if(!drag||e.pointerId!==drag.pointerId)return;const moved=drag.moved;drag=null;if(moved)schedule(0);else if(e.type==='pointerup'){const r=canvas.getBoundingClientRect();inspectAt(e.clientX-r.left,e.clientY-r.top)}}
canvas.addEventListener('pointerup',endDrag);canvas.addEventListener('pointercancel',endDrag);canvas.addEventListener('lostpointercapture',endDrag);canvas.addEventListener('pointerleave',()=>{$('tooltip').hidden=true});
canvas.addEventListener('keydown',e=>{const steps={ArrowLeft:[-1,0],ArrowRight:[1,0],ArrowUp:[0,1],ArrowDown:[0,-1]};if(steps[e.key]){e.preventDefault();center[0]+=steps[e.key][0]*width/scale*.15;center[1]+=steps[e.key][1]*height/scale*.15;draw();schedule()}else if(e.key==='+'||e.key==='=')zoom(1.5);else if(e.key==='-')zoom(1/1.5);else if(e.key==='Home')fit()});
$('zoom-in').onclick=()=>zoom(1.5);$('zoom-out').onclick=()=>zoom(1/1.5);$('reset').onclick=fit;$('coverage').onclick=fitCoverage;$('return').onclick=fitCoverage;
document.querySelectorAll('[data-layer]').forEach(e=>e.onchange=()=>{layers[e.dataset.layer]=e.checked;if(e.dataset.layer==='fishing'){schedule(0)}else draw()});
new ResizeObserver(()=>{const r=canvas.getBoundingClientRect();width=r.width;height=r.height;const d=window.devicePixelRatio||1;canvas.width=Math.round(width*d);canvas.height=Math.round(height*d);draw();schedule()}).observe(canvas);
async function selectChart(id,initial=false){
 clearSearch();searchInput.disabled=true;
 const generation=++metadataSequence;pending();clearTimeout(timer);selectedChart=id;selectedOverlays.clear();features=[];hits.clear();rendered=null;$('tooltip').hidden=true;$('empty').hidden=true;$('active-chart').textContent='';
 const previous=initial||!viewReady?null:viewport();meta=null;draw();$('source').textContent='Loading selected chart…';$('chart-overlays').replaceChildren();
 try{const response=await fetch(`/api/metadata?chart=${encodeURIComponent(id)}`);const data=await response.json();if(generation!==metadataSequence)return;if(!response.ok)throw Error(data.error);meta=data;$('source').textContent='User-supplied chart';
  $('limitations').replaceChildren();for(const text of [...meta.limitations,...meta.warnings]){const li=document.createElement('li');li.textContent=text;$('limitations').append(li)}
  const entry=catalog.find(c=>c.id===id);$('chart-support').textContent=id==='auto'?'Automatically selects one supported base chart for this view.':entry?.status==='partial'?'Partial support — see limitations.':'2D chart support';
  for(const overlay of catalog.filter(c=>c.role==='overlay'&&c.companion_of===id&&c.status!=='unreadable')){const label=document.createElement('label'),input=document.createElement('input');input.type='checkbox';input.dataset.overlay=overlay.id;input.onchange=()=>{if(input.checked)selectedOverlays.add(overlay.id);else selectedOverlays.delete(overlay.id);schedule(0)};label.append(input,document.createTextNode(overlay.title));$('chart-overlays').append(label)}
  searchInput.disabled=false;
  const b=meta.coverage_bbox,overlaps=previous&&previous[0]<=b[2]&&previous[2]>=b[0]&&previous[1]<=b[3]&&previous[3]>=b[1];if(!initial&&overlaps){draw();schedule(0)}else fit();
 }catch(e){if(generation!==metadataSequence)return;setLoading(false);$('status').textContent='Could not select chart';$('source').textContent=e.message;}
}
$('chart-select').onchange=e=>selectChart(e.target.value);
(async()=>{try{const response=await fetch('/api/charts');if(!response.ok)throw Error('Could not read chart catalog');const data=await response.json();catalog=data.charts;const select=$('chart-select');select.replaceChildren();const auto=document.createElement('option');auto.value='auto';auto.textContent='All / Auto';select.append(auto);for(const chart of catalog){const option=document.createElement('option');option.value=chart.id;option.textContent=chart.title+(chart.role==='overlay'?' · supplementary layer':'')+(chart.status==='unreadable'?' · unavailable':'');option.disabled=chart.status==='unreadable'||chart.role==='overlay';select.append(option)}select.value='auto';await selectChart('auto',true)}catch(e){setLoading(false);$('status').textContent='Startup error';$('source').textContent=e.message}})();
