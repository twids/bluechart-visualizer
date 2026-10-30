// Runs the actual viewer against synthetic API fixtures; contains no chart data.
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const http=require('node:http');
const {chromium}=require('playwright');
const root=path.resolve(__dirname,'..');
const polygon=(type,coords)=>({kind:'polygon',type,coords,tile:'MTEST'});
const point=(type,coords,extra={})=>({kind:'point',type,coords:[coords],tile:'MTEST',...extra});
const features=[
 polygon(0x10101,[[18,59],[18.05,59],[18.05,59.1],[18,59.1]]),
 polygon(0x10302,[[18.05,59],[18.1,59],[18.1,59.1],[18.05,59.1]]),
 polygon(0x10507,[[18.06,59.01],[18.09,59.01],[18.09,59.035],[18.06,59.035]]),
 {kind:'line',type:0x10402,coords:[[18.01,59.05],[18.09,59.05]],tile:'MTEST'},
 point(0x1040a,[18.02,59.07],{label:'Synthetic rock'}),
 point(0x10402,[18.065,59.08],{depth_m:12.3}),
 point(0x10802,[18.08,59.06],{label:'<img src=x onerror=alert(1)>'}),
 point(0x10600,[18.035,59.03],{label:'Synthetic seabed text'})
];
const bbox=[18,59,18.1,59.1];
let buildState={search:true,overview:true};
const server=http.createServer((req,res)=>{
 const url=new URL(req.url,'http://localhost');let value;
 if(url.pathname==='/api/build-status')value=buildState;
 else if(url.pathname==='/api/charts')value={default_chart:'test',charts:[{id:'test',title:'Synthetic chart',status:'supported',role:'base'}]};
 else if(url.pathname==='/api/metadata')value={initial_bbox:bbox,coverage_bbox:bbox,coverage_bounds:bbox,limitations:[],warnings:[]};
 else if(url.pathname==='/api/features')value={features,warnings:[],chart_title:'Synthetic chart',stats:{features:features.length,subdivisions:1,bytes_read:100,seconds:.01,effective_bits:22}};
 if(value){res.setHeader('Content-Type','application/json');res.end(JSON.stringify(value));return}
 const file={'/':'index.html','/app.js':'app.js','/features.js':'features.js','/view.js':'view.js','/style.css':'style.css'}[url.pathname];
 if(!file){res.writeHead(404);res.end();return}
 res.setHeader('Content-Type',file.endsWith('.js')?'text/javascript':file.endsWith('.css')?'text/css':'text/html');res.end(fs.readFileSync(path.join(root,'web',file)));
});
(async()=>{
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 const browser=await chromium.launch({headless:true,channel:process.env.BLUECHART_BROWSER||'msedge'});
 try{
  const page=await browser.newPage({viewport:{width:1200,height:900},deviceScaleFactor:2,hasTouch:true});
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.goto(`http://127.0.0.1:${server.address().port}`);
  await page.waitForFunction(()=>document.getElementById('status').textContent==='Chart loaded');
  await page.waitForFunction(()=>document.getElementById('build-status-text').textContent.includes('search index and overview'));
  buildState={search:false,overview:true};
  await page.waitForFunction(()=>document.getElementById('build-status-text').textContent==='Building overview…');
  buildState={search:false,overview:false};
  await page.waitForFunction(()=>document.getElementById('build-status').hidden);
  // Let initial ResizeObserver work settle before the first hit test.
  await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
  await page.waitForFunction(()=>document.getElementById('map-loading').hidden);
  const tip=page.locator('#tooltip');
  async function hover(coords){const [x,y]=await page.evaluate(p=>screen(p),coords);const box=await page.locator('canvas').boundingBox();await page.mouse.move(box.x+x,box.y+y);return tip.innerText()}
  assert.match(await hover([18.02,59.07]),/Dangerous rock\nSynthetic rock/);
  assert.match(await tip.innerText(),/Point · Type 0x1040a/);
  assert.match(await hover([18.065,59.08]),/Wreck\nDepth: 12.3 m/);
  assert.match(await hover([18.05,59.05]),/Submarine pipeline/);
  assert.match(await tip.innerText(),/Line · Type 0x10402/);
  const pipelinePosition=await tip.innerText();assert.match(pipelinePosition,/18.05000° E/,'line tooltip uses the selected segment, not the first vertex');
  assert.match(await hover([18.075,59.02]),/Restricted area/);
  assert.match(await hover([18.08,59.06]),/Unknown point feature/);
  assert.equal(await tip.locator('img').count(),0,'chart labels are rendered as text');
  assert.match(await tip.innerText(),/Type 0x10802/);
  assert.match(await hover([18.035,59.03]),/Centered chart label/);
  await page.locator('[data-layer="symbols"]').uncheck();
  assert.doesNotMatch(await hover([18.02,59.07]),/Dangerous rock/,'hidden symbols cannot be selected');
  assert.doesNotMatch(await hover([18.075,59.02]),/Restricted area/,'hidden thematic areas cannot be selected');
  await page.locator('[data-layer="symbols"]').check();
  await hover([18.02,59.07]);
  await page.touchscreen.tap(...await page.evaluate(()=>{const [x,y]=screen([18.02,59.07]),r=document.getElementById('chart').getBoundingClientRect();return [r.left+x,r.top+y]}));
  assert.match(await tip.innerText(),/Dangerous rock/);
  // The same inspector and geographic coordinates must work in perspective.
  async function settled(action){const response=page.waitForResponse(r=>r.url().includes('/api/features')&&r.status()===200);await action();await (await response).finished();await page.waitForFunction(()=>document.getElementById('map-loading').hidden)}
  await settled(()=>page.locator('#view-3d').click());
  assert.equal(await page.locator('#view-3d').getAttribute('aria-pressed'),'true');
  for(const angle of [0,30,60]){
   await settled(()=>page.locator('#pitch').evaluate((el,value)=>{el.value=value;el.dispatchEvent(new Event('input',{bubbles:true}))},String(angle)));
   assert.equal(await page.locator('#pitch-value').innerText(),`${angle}°`);
   assert.match(await hover([18.02,59.07]),/Dangerous rock\nSynthetic rock/);
   assert.match(await hover([18.05,59.05]),/Submarine pipeline/);
   assert.match(await tip.innerText(),/18.05000° E/);
   const roundtrip=await page.evaluate(()=>{
    const p=[width*.22,height*.2],q=screen(coord(...p));return Math.hypot(p[0]-q[0],p[1]-q[1]);
   });assert(roundtrip<1e-6,'cursor coordinates invert the perspective camera');
   const covered=await page.evaluate(()=>{
    const b=viewport();return [[0,0],[width,0],[width,height],[0,height]].every(p=>{const [x,y]=coord(...p);return x>=b[0]-1e-8&&x<=b[2]+1e-8&&y>=b[1]-1e-8&&y<=b[3]+1e-8});
   });assert(covered,'feature requests include all four corners of the tilted view');
  }
  // Keep the grabbed ground location under the pointer, including away from centre.
  const canvasBox=await page.locator('canvas').boundingBox();
  const grab=[canvasBox.width*.4,canvasBox.height*.3],delta=[65,40];
  const anchor=await page.evaluate(p=>coord(...p),grab);
  await settled(async()=>{await page.mouse.move(canvasBox.x+grab[0],canvasBox.y+grab[1]);await page.mouse.down();await page.mouse.move(canvasBox.x+grab[0]+delta[0],canvasBox.y+grab[1]+delta[1],{steps:5});await page.mouse.up()});
  const moved=await page.evaluate(p=>screen(p),anchor);
  assert(Math.hypot(moved[0]-grab[0]-delta[0],moved[1]-grab[1]-delta[1])<.01,'tilted drag is anchored to the chart plane');
  const zoomAnchor=await page.evaluate(()=>coord(width*.35,height*.6));
  const beforeZoom=await page.evaluate(p=>screen(p),zoomAnchor);
  await settled(()=>page.evaluate(p=>zoom(1.3,...p),beforeZoom));
  const afterZoom=await page.evaluate(p=>screen(p),zoomAnchor);
  assert(Math.hypot(afterZoom[0]-beforeZoom[0],afterZoom[1]-beforeZoom[1])<.01,'zoom stays under the cursor');
  await settled(()=>page.locator('#coverage').click());
  assert(await page.evaluate(()=>[[18,59],[18,59.1],[18.1,59.1],[18.1,59]].every(p=>{const [x,y]=screen(p);return x>=0&&x<=width&&y>=0&&y<=height})),'fit includes all corners at maximum tilt');
  await page.locator('[data-layer="symbols"]').uncheck();
  assert.doesNotMatch(await hover([18.02,59.07]),/Dangerous rock/);
  await page.locator('[data-layer="symbols"]').check();
  await page.touchscreen.tap(...await page.evaluate(()=>{const [x,y]=screen([18.02,59.07]),r=document.getElementById('chart').getBoundingClientRect();return [r.left+x,r.top+y]}));
  assert.match(await tip.innerText(),/Dangerous rock/,'touch picking works at maximum tilt');
  await page.locator('canvas').focus();
  await settled(()=>page.keyboard.press('ArrowRight'));
  await settled(()=>page.keyboard.press('Home'));
  fs.mkdirSync(path.join(root,'artifacts'),{recursive:true});await page.mouse.move(1,1);
  await page.screenshot({path:path.join(root,'artifacts','perspective-desktop-synthetic.png')});
  await settled(()=>page.locator('#view-3d').click());
  assert.equal(await page.locator('#view-tilt').isVisible(),false);
  await settled(()=>page.locator('#reset').click());
  const resized=page.waitForResponse(response=>response.url().includes('/api/features')&&response.status()===200);
  await page.setViewportSize({width:700,height:760});await (await resized).finished();
  await page.waitForFunction(()=>document.getElementById('map-loading').hidden);
  await hover([18.08,59.06]);
  const bounds=await tip.boundingBox(),map=await page.locator('#map').boundingBox();
  assert(bounds.x>=map.x&&bounds.x+bounds.width<=map.x+map.width+1);
  assert(bounds.y>=map.y&&bounds.y+bounds.height<=map.y+map.height+1);
  await hover([18.02,59.07]);
  await settled(()=>page.locator('#view-3d').click());
  await settled(()=>page.locator('#coverage').click());
  await hover([18.02,59.07]);
  await page.locator('#tooltip').evaluate(el=>el.hidden=true);
  fs.mkdirSync(path.join(root,'artifacts'),{recursive:true});await page.screenshot({path:path.join(root,'artifacts','feature-inspector-synthetic.png')});
  await settled(()=>page.setViewportSize({width:390,height:740}));
  const controls=await page.locator('.view-controls').boundingBox(),narrowMap=await page.locator('#map').boundingBox();
  assert(controls.x>=narrowMap.x&&controls.x+controls.width<=narrowMap.x+narrowMap.width,'perspective controls fit a narrow mobile map');
  await settled(()=>page.locator('#coverage').click());
  await settled(()=>page.locator('#view-3d').click());
  assert.equal(await page.locator('#view-3d').getAttribute('aria-pressed'),'false');
  assert.deepEqual(errors,[]);
  console.log('Viewer browser checks passed: feature inspector, perspective 0–60°, picking, inverse coordinates, viewport, drag, cursor zoom, fit, layers and responsive layout');
 }finally{await browser.close();server.close()}
})().catch(error=>{console.error(error);server.close();process.exitCode=1});
