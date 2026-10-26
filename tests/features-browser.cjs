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
const server=http.createServer((req,res)=>{
 const url=new URL(req.url,'http://localhost');let value;
 if(url.pathname==='/api/charts')value={default_chart:'test',charts:[{id:'test',title:'Synthetic chart',status:'supported',role:'base'}]};
 else if(url.pathname==='/api/metadata')value={initial_bbox:bbox,coverage_bbox:bbox,coverage_bounds:bbox,limitations:[],warnings:[]};
 else if(url.pathname==='/api/features')value={features,warnings:[],chart_title:'Synthetic chart',stats:{features:features.length,subdivisions:1,bytes_read:100,seconds:.01,effective_bits:22}};
 if(value){res.setHeader('Content-Type','application/json');res.end(JSON.stringify(value));return}
 const file={'/':'index.html','/app.js':'app.js','/features.js':'features.js','/style.css':'style.css'}[url.pathname];
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
  const resized=page.waitForResponse(response=>response.url().includes('/api/features')&&response.status()===200);
  await page.setViewportSize({width:700,height:760});await (await resized).finished();
  await page.waitForFunction(()=>document.getElementById('map-loading').hidden);
  await hover([18.08,59.06]);
  const bounds=await tip.boundingBox(),map=await page.locator('#map').boundingBox();
  assert(bounds.x>=map.x&&bounds.x+bounds.width<=map.x+map.width+1);
  assert(bounds.y>=map.y&&bounds.y+bounds.height<=map.y+map.height+1);
  await hover([18.02,59.07]);
  fs.mkdirSync(path.join(root,'artifacts'),{recursive:true});await page.screenshot({path:path.join(root,'artifacts','feature-inspector-synthetic.png')});
  assert.deepEqual(errors,[]);
  console.log('Viewer browser checks passed: names, labels, depth, points/lines/areas, layers, clicks, coordinates and responsive tooltips');
 }finally{await browser.close();server.close()}
})().catch(error=>{console.error(error);server.close();process.exitCode=1});
