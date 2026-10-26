// Optional UI smoke test: requires Playwright and its Chromium browser.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const path = require('node:path');
const fs = require('node:fs');
(async () => {
 const browser=await chromium.launch({headless:true,channel:process.env.BLUECHART_BROWSER||'msedge'});
 const page=await browser.newPage({viewport:{width:1440,height:1000},deviceScaleFactor:1});
 const errors=[];page.on('pageerror',e=>errors.push(e.message));
 const requests=[];page.on('request',r=>requests.push(r.url()));
 const base=process.env.BLUECHART_URL||'http://127.0.0.1:8765';
 async function action(fn){const response=page.waitForResponse(r=>r.url().includes('/api/features')&&r.status()===200);await fn();const result=await response;await result.finished();await page.waitForFunction(()=>document.getElementById('status').textContent!=='Reading chart tiles…');return new URL(result.url()).searchParams.get('bbox')}
 const initialBBox=await action(()=>page.goto(base));
 await page.waitForFunction(()=>document.getElementById('stats').textContent.includes('features'));
 const initial=await page.locator('#detail').innerText();
 const zoomedBBox=await action(()=>page.getByRole('button',{name:'Zoom in',exact:true}).click());
 assert.notEqual(zoomedBBox,initialBBox);
 const box=await page.locator('canvas').boundingBox();
 const pannedBBox=await action(async()=>{await page.mouse.move(box.x+box.width/2,box.y+box.height/2);await page.mouse.down();await page.mouse.move(box.x+box.width/2+90,box.y+box.height/2+30,{steps:8});await page.mouse.up()});
 assert.notEqual(pannedBBox,zoomedBBox);
 const wheelBBox=await action(()=>page.mouse.wheel(0,250));
 assert.notEqual(wheelBBox,pannedBBox);
 await page.locator('canvas').focus();
 const keyBBox=await action(()=>page.keyboard.press('ArrowRight'));
 assert.notEqual(keyBBox,wheelBBox);
 for(const layer of ['depth','symbols','water','text','fishing']){await page.locator(`[data-layer="${layer}"]`).click();await page.locator(`[data-layer="${layer}"]`).click()}
 const resetBBox=await action(()=>page.getByRole('button',{name:'Reset view',exact:true}).click());
 assert.equal(resetBBox,initialBBox);
 await page.mouse.move(1350,80);
 assert.equal(await page.locator('#detail').innerText(),initial);
 assert.equal(await page.locator('#error-count').innerText(),'No decoding errors in this view',await page.locator('#error-list').innerText());
 const colors=await page.locator('canvas').evaluate(c=>{const d=c.getContext('2d').getImageData(0,0,c.width,c.height).data,s=new Set();for(let i=0;i<d.length;i+=80)s.add(`${d[i]},${d[i+1]},${d[i+2]}`);return s.size});
 assert(colors>30,'Canvas should contain actual chart detail');
 assert.deepEqual(errors,[]);
 assert(requests.every(url=>url.startsWith(base)),'No external browser resources');
 const overviewBBox=await action(()=>page.getByRole('button',{name:'Fit chart coverage',exact:true}).click());
 assert.notEqual(overviewBBox,initialBBox);
 assert.equal(await page.locator('#error-count').innerText(),'No decoding errors in this view');
 await action(()=>page.getByRole('button',{name:'Reset view',exact:true}).click());
 const out=path.resolve(__dirname,'../artifacts');fs.mkdirSync(out,{recursive:true});
 await page.screenshot({path:path.join(out,'viewer.png'),fullPage:true});
 await action(()=>page.setViewportSize({width:700,height:750}));
 assert(await page.locator('canvas').isVisible());
 await page.screenshot({path:path.join(out,'viewer-narrow.png'),fullPage:true});
 await action(()=>page.setViewportSize({width:390,height:800}));
 await page.screenshot({path:path.join(out,'viewer-mobile.png'),fullPage:true});
 console.log(JSON.stringify({status:'passed',browserErrors:errors,canvasColors:colors,requests:requests.length,initialDetail:initial},null,2));
 await browser.close();
})().catch(e=>{console.error(e);process.exit(1)});
