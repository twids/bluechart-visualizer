/* SPDX-License-Identifier: GPL-3.0-only */
// Deterministic browser integration with synthetic chart/API fixtures; no chart files needed.
'use strict';
const assert=require('node:assert/strict');
const http=require('node:http');
const fs=require('node:fs');
const path=require('node:path');
const {chromium}=require('playwright');
const assets=path.resolve(__dirname,'../web');
const searches=[];
const metadata={coverage_bbox:[10,50,30,70],coverage_bounds:[10,50,30,70],initial_bbox:[17,58,18,59],limitations:[],warnings:[]};
const results=[{chart_id:'base',chart_title:'Synthetic chart',label:'Östergård',kind:'point',type:0x10500,coordinates:[24,64],bounds:[24,64,24,64]},
 {chart_id:'base',chart_title:'Synthetic chart',label:'Östergården',kind:'point',type:0x10500,coordinates:[23,63],bounds:[23,63,23,63]}];
const server=http.createServer((req,res)=>{
 const url=new URL(req.url,'http://localhost');
 function json(data,status=200){res.writeHead(status,{'Content-Type':'application/json'});res.end(JSON.stringify(data))}
 if(url.pathname==='/api/charts')return json({default_chart:'base',charts:[{id:'base',title:'Synthetic chart',role:'base',status:'supported'},{id:'second',title:'Second chart',role:'base',status:'supported'}]});
 if(url.pathname==='/api/metadata')return json(metadata);
 if(url.pathname==='/api/features')return json({features:[{kind:'point',type:0x10500,label:'Synthetic place',coords:[[17.5,58.5]]}],warnings:[],stats:{features:1,subdivisions:1,bytes_read:100,seconds:0.01}});
 if(url.pathname==='/api/search'){
  const q=url.searchParams.get('q');searches.push({q,chart:url.searchParams.get('chart')});
  if(q==='slow')return setTimeout(()=>json({results:[{...results[0],label:'STALE'}],warnings:[]}),700);
  if(q==='error')return json({error:'Synthetic failure'},503);
  if(q==='none')return json({results:[],warnings:[]});
  if(q==='missing')return json({results:[],warnings:['Search index unavailable. Run bluechart build-search.']});
  return json({results,warnings:url.searchParams.get('chart')==='auto'?['Index unavailable for second chart.']:[]});
 }
 const file={'/':'index.html','/app.js':'app.js','/features.js':'features.js','/style.css':'style.css'}[url.pathname];
 if(!file){res.writeHead(404);return res.end()}
 res.writeHead(200,{'Content-Type':file.endsWith('.js')?'text/javascript':file.endsWith('.css')?'text/css':'text/html'});res.end(fs.readFileSync(path.join(assets,file)));
});
(async()=>{
 let browser;
 try{
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  const base=`http://127.0.0.1:${server.address().port}`;
  browser=await chromium.launch({headless:true,channel:process.env.BLUECHART_BROWSER||'msedge'});
  const page=await browser.newPage({viewport:{width:1440,height:1000}});
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.goto(base);const input=page.getByRole('combobox',{name:'Find a place'});
  await input.waitFor({state:'visible'});await page.waitForFunction(()=>!document.getElementById('place-query').disabled);
  await input.fill('o');await page.waitForTimeout(350);assert.equal(searches.length,0);
  await input.fill('os');await input.fill('oste');
  await page.getByRole('option',{name:/Östergård Synthetic/}).waitFor();
  assert.deepEqual(searches,[{q:'oste',chart:'auto'}]);
  await input.press('ArrowDown');assert.equal(await input.getAttribute('aria-activedescendant'),await page.locator('#search-results').getByRole('option').first().getAttribute('id'));
  const featureRequest=page.waitForRequest(r=>r.url().includes('/api/features')&&new URL(r.url()).searchParams.get('bbox').startsWith('23.'));
  await input.press('Enter');await featureRequest;
  assert.equal(await page.locator('#chart-select').inputValue(),'auto');
  await page.locator('#search-marker').waitFor({state:'visible'});assert.equal(await page.locator('#search-marker').innerText(),'Östergård');
  await page.getByRole('button',{name:'Zoom in',exact:true}).click();assert.equal(await page.locator('#search-marker').innerText(),'Östergård');
  await page.getByRole('button',{name:'Clear search'}).click();assert.equal(await input.inputValue(),'');assert.equal(await page.locator('#search-marker').isVisible(),false);
  await page.locator('#chart-select').selectOption('auto');await page.waitForFunction(()=>!document.getElementById('place-query').disabled);
  await input.fill('oste');await page.locator('#search-results').getByRole('option').first().waitFor();assert.equal(searches.at(-1).chart,'auto');
  assert.match(await page.locator('#search-status').innerText(),/Index unavailable/);
  await input.press('ArrowUp');assert.equal(await page.locator('#search-results').getByRole('option').last().getAttribute('aria-selected'),'true');
  await input.press('Enter');assert.equal(await page.locator('#chart-select').inputValue(),'auto');assert.equal(await page.locator('#search-marker').innerText(),'Östergården');
  await input.fill('none');await page.waitForFunction(()=>document.getElementById('search-status').textContent==='No matching places.');
  await input.fill('missing');await page.waitForFunction(()=>document.getElementById('search-status').textContent.includes('build-search'));
  await input.fill('error');await page.waitForFunction(()=>document.getElementById('search-status').textContent.includes('Synthetic failure'));
  await input.fill('slow');await page.waitForRequest(r=>r.url().includes('q=slow'));
  await input.fill('oste');await page.locator('#search-results').getByRole('option').first().waitFor();await page.waitForTimeout(800);
  assert.equal(await page.locator('#search-results').getByRole('option').first().locator('strong').innerText(),'Östergård');
  await input.fill('slow');await page.waitForRequest(r=>r.url().includes('q=slow'));
  await page.locator('#chart-select').selectOption('second');await page.waitForFunction(()=>!document.getElementById('place-query').disabled);await page.waitForTimeout(800);
  assert.equal(await input.inputValue(),'');assert.equal(await page.locator('#search-results').getByRole('option').count(),0);assert.equal(await page.locator('#search-marker').isVisible(),false);
  await input.fill('oste');await page.locator('#search-results').getByRole('option').first().waitFor();await input.press('Escape');
  assert.equal(await input.getAttribute('aria-expanded'),'false');assert.equal(await input.inputValue(),'oste');await input.press('Escape');assert.equal(await input.inputValue(),'');
  await input.fill('oste');await page.locator('#search-results').getByRole('option').first().waitFor();await page.locator('#search-results').getByRole('option').first().click();assert.equal(await page.locator('#search-marker').isVisible(),true);
  await input.fill('oste');await input.press('Enter');
  await page.locator('#full-search-results button').first().waitFor();
  assert.equal(await page.locator('#search-suggestions').isVisible(),false);
  assert.equal(await page.locator('#chart-settings').isVisible(),false);
  await page.locator('#full-search-results button').last().click();
  assert.equal(await page.locator('#search-marker').innerText(),'Östergården');
  assert.equal(await page.locator('#full-search-results button').count(),2);
  await page.getByRole('button',{name:'Back to chart settings'}).click();
  assert.equal(await page.locator('#chart-settings').isVisible(),true);
  await input.fill('none');await input.press('Enter');
  await page.waitForFunction(()=>document.getElementById('results-summary').textContent==='No matching places.');
  await input.fill('error');await page.getByRole('button',{name:'Search',exact:true}).click();
  await page.waitForFunction(()=>document.getElementById('results-summary').textContent.includes('Synthetic failure'));
  await page.getByRole('button',{name:'Clear search'}).click();
  for(const width of [1440,390]){
   await page.setViewportSize({width,height:900});
   await input.fill('oste');await page.locator('#search-results').getByRole('option').first().waitFor();
   const box=await input.boundingBox();const dropdown=await page.locator('#search-suggestions').boundingBox();
   assert.ok(box.y<60);assert.ok(dropdown.y>=box.y+box.height);assert.ok(dropdown.x+dropdown.width<=width);
   await input.press('Enter');await page.locator('#full-search-results button').first().waitFor();
   assert.ok((await page.locator('#chart').boundingBox()).width>150);
   await page.getByRole('button',{name:'Clear search'}).click();
  }
  assert.deepEqual(errors,[]);console.log('Search browser integration passed');
 }finally{await browser?.close();await new Promise(resolve=>server.close(resolve))}
})().catch(e=>{console.error(e);process.exitCode=1});

