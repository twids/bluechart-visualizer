/* SPDX-License-Identifier: GPL-3.0-only */
'use strict';
// Names are geometry-specific. Sources and unverified extensions: THIRD_PARTY.md.
// An absent entry deliberately remains unknown; nearby codes do not imply meaning.
const ChartFeatures = (() => {
 const points={
  0x700:'City (100,000–200,000 inhabitants)',
  0x10100:'Light',0x10101:'Light with unspecified mark',0x10102:'North cardinal light',0x10103:'South cardinal light',0x10104:'East cardinal light',0x10105:'West cardinal light',0x10106:'Isolated danger light',0x10107:'Port-hand light',0x10108:'Starboard-hand light',0x10109:'Special-purpose light',0x1010a:'Safe-water light',
  0x10200:'Buoy',0x10201:'Buoy',0x10202:'North cardinal buoy',0x10203:'South cardinal buoy',0x10204:'East cardinal buoy',0x10205:'West cardinal buoy',0x10206:'Beacon',0x10207:'Spar buoy',0x10208:'Isolated danger buoy',0x10209:'Port-hand buoy',0x1020a:'Starboard-hand buoy',0x1020b:'Special-purpose buoy',0x1020c:'Safe-water buoy',0x1020d:'Offshore platform',0x1020e:'North cardinal beacon',0x1020f:'South cardinal beacon',0x10210:'East cardinal beacon',0x10211:'West cardinal beacon',0x10212:'Isolated danger beacon',0x10213:'Port-hand beacon',0x10214:'Starboard-hand beacon',0x10215:'Special-purpose beacon',0x10216:'Mooring buoy',0x10217:'Fixed point',0x10218:'Pole',
  0x10300:'Depth sounding',0x10301:'Depth sounding',0x10302:'Depth sounding',0x10303:'Spot height',0x10304:'Building',0x10305:'Chimney',0x10306:'Church',0x10307:'Tanks',0x10308:'Tower',0x10309:'Rock (landmark)',0x1030a:'Triangulation point',0x1030b:'Radio mast',
  0x10400:'Isolated danger',0x10401:'Obstruction',0x10402:'Wreck',0x10403:'Exposed wreck',0x10404:'Well',0x10405:'Foul ground',0x10406:'Explosive',0x10407:'Fish haven',0x10408:'Obstruction that covers and uncovers',0x10409:'Marine farm',0x1040a:'Dangerous rock',0x1040b:'No bottom found',0x1040c:'Exposed rock',0x1040d:'Underwater rock of uncertain depth',0x1040e:'Underwater rock (classified non-dangerous)',0x1040f:'Shoal',
  0x10500:'Chart label',0x10600:'Centered chart label',0x10700:'Miscellaneous marine point',0x10701:'Recommended anchorage',0x10702:'Pilot boarding place',0x10703:'Yacht harbour',0x10704:'Pile',0x10705:'Anchoring prohibited',0x10706:'Fishing prohibited',0x10707:'Precautionary area',0x10708:'Radio reporting point',0x10709:'Anchorage berths',0x1070a:'Rescue station',0x1070b:'Fishing harbour',0x1070c:'Airport',0x10801:'Chart information',0x10901:'Bottom conditions',0x10902:'Fishing information',0x10903:'Facility'
 };
 const lines={
  0x10100:'Miscellaneous marine line',0x10101:'Shoreline',0x10102:'Cartographic line',0x10103:'Road',0x10104:'Clearing line',0x10105:'Contour line',0x10106:'Overhead cable',0x10107:'Bridge',0x10108:'Recommended route',0x10109:'Chart border',
  0x10300:'Depth contour',0x10301:'Depth contour',0x10307:'Intertidal zone boundary',
  // These extensions are decoded as contours in the local chart samples.
  0x10308:'Depth contour',0x10309:'Supplementary depth contour',
  0x10400:'Obstruction line',0x10401:'Submarine cable',0x10402:'Submarine pipeline',0x10403:'Pile barrier',0x10404:'Fishing stakes',0x10405:'Supply pipeline area boundary',0x10406:'Submarine cable area boundary',0x10407:'Dumping ground boundary',0x10408:'Explosive dumping ground boundary',0x10409:'Danger area boundary',0x1040a:'Overhead cable',0x1040b:'Submerged construction',0x1040c:'Pier / jetty',
  0x10500:'Restriction boundary',0x10501:'Anchoring prohibited boundary',0x10502:'Fishing prohibited boundary',0x10503:'Prohibited area boundary',0x10504:'Military practice area boundary',0x10505:'Anchoring and fishing prohibited boundary',0x10506:'Nature reserve boundary',0x10507:'Restricted area boundary',0x10508:'Minefield boundary',
  0x10600:'Miscellaneous marine line',0x10601:'Cartographic line',0x10602:'Traffic separation line',0x10603:'International maritime boundary',0x10604:'Territorial sea baseline',0x10605:'Territorial sea limit',0x10606:'Anchorage boundary',0x10607:'Quarantine anchorage boundary',0x10608:'Fishery zone boundary',0x10609:'Swept area boundary',0x1060a:'Traffic separation zone boundary',0x1060b:'Exclusive economic zone limit',0x1060c:'Established traffic direction',0x1060d:'Recommended traffic direction',0x1060e:'Harbour limit',0x1060f:'Inadequately surveyed area boundary',0x10610:'Inshore traffic zone boundary',0x10611:'Traffic lane limit',0x10701:'River channel',0x10702:'Submerged object',0x10706:'Chart boundary',
  0x10903:'Minor depth contour',0x10904:'Intermediate depth contour',0x10905:'Major depth contour'
 };
 const areas={
  0x4b:'Chart background',0x10d01:'Chart background',0x10100:'Land',0x10101:'Land (non-urban)',0x10102:'Land (urban)',0x10103:'Chart exclusion area',0x10104:'Chart background',0x10105:'Bridge',
  0x10300:'Depth area',0x10301:'Intertidal zone',0x10302:'Depth area (blue band 1)',0x10303:'Depth area (blue band 2)',0x10304:'Depth area (blue band 3)',0x10305:'Depth area (blue band 4)',0x10306:'Depth area (blue band 5)',0x10307:'Depth area',0x10308:'Depth area',
  0x10400:'Obstruction area',0x10401:'Submarine cable area',0x10402:'Submarine pipeline area',0x10403:'Pile barrier area',0x10404:'Fishing stakes area',0x10405:'Supply pipeline area',0x10406:'Submarine cable area',0x10407:'Dumping ground',0x10408:'Explosive dumping ground',0x10409:'Danger area',0x1040a:'Overhead cable area',0x1040b:'Submerged construction',0x1040c:'Pier / jetty',
  0x10500:'Restriction area',0x10501:'Anchoring prohibited',0x10502:'Fishing prohibited',0x10503:'Prohibited area',0x10504:'Military practice area',0x10505:'Anchoring and fishing prohibited',0x10506:'Nature reserve',0x10507:'Restricted area',0x10508:'Minefield',
  0x10600:'Miscellaneous marine area',0x10601:'Cartographic area',0x10602:'Traffic separation area',0x10603:'International maritime boundary area',0x10604:'Territorial sea baseline area',0x10605:'Territorial sea limit area',0x10606:'Anchorage area',0x10607:'Quarantine anchorage area',0x10608:'Fishery zone',0x10609:'Swept area',0x1060a:'Traffic separation zone',0x1060b:'Exclusive economic zone limit area',0x1060c:'Established traffic direction area',0x10701:'Fishing area',0x10702:'Restricted area',0x10703:'Anchorage area',0x10704:'Fishing information area'
 };
 const backgrounds=new Set([0x4b,0x10d01,0x10103,0x10104,0x10106]);
 function describe(f){
  const name=({point:points,line:lines,polygon:areas}[f.kind]||{})[f.type];
  const kind={point:'Point',line:'Line',polygon:'Area'}[f.kind]||'Feature';
  return {name:name||`Unknown ${kind.toLowerCase()} feature`,known:!!name,kind,code:`0x${f.type.toString(16)}`};
 }
 function layer(f){
  if(f.kind==='point')return Number.isFinite(f.depth_m)?'depth':[0x10500,0x10600].includes(f.type)?'text':'symbols';
  if(f.kind==='polygon'&&!describe(f).known)return 'unknownAreas';
  if(f.type>=0x10300&&f.type<=0x10310||f.kind==='line'&&f.type>=0x10903&&f.type<=0x10905)return 'water';
  if(f.kind==='line'&&[0x10101,0x10103,0x10104,0x10105,0x10107].includes(f.type)||f.kind==='polygon'&&(f.type>=0x10100&&f.type<=0x10107||backgrounds.has(f.type)))return 'land';
  return 'symbols';
 }
 function lineStyle(f){
  if(layer(f)==='water')return {color:'#73a8bf',width:.7,dash:f.type===0x10307?[3,2]:[]};
  if(f.type===0x10101)return {color:'#766f57',width:.85,dash:[]};
  if([0x10107,0x1040b,0x1040c].includes(f.type))return {color:'#817c72',width:1.4,dash:[]};
  if([0x10106,0x10401,0x10402,0x1040a].includes(f.type))return {color:'#9b668f',width:.9,dash:[4,3]};
  if(f.type===0x10108)return {color:'#925f91',width:1.1,dash:[8,3]};
  return {color:describe(f).known?'#a2779d':'#7c8581',width:.8,dash:[5,4]};
 }
 // A screen-space grid avoids testing every vertex on every pointer movement.
 class HitIndex{
  constructor(){this.clear()}
  clear(){this.cells=new Map();this.large=[];this.order=0}
  add(f,points,filled=false){
   if(!points.length||backgrounds.has(f.type)&&f.kind==='polygon')return;
   let minX=Infinity,minY=Infinity,maxX=-Infinity,maxY=-Infinity;
   for(const [x,y] of points){minX=Math.min(minX,x);maxX=Math.max(maxX,x);minY=Math.min(minY,y);maxY=Math.max(maxY,y)}
   const item={f,points,filled,minX,minY,maxX,maxY,order:this.order++};
   const x0=Math.floor((minX-9)/64),x1=Math.floor((maxX+9)/64),y0=Math.floor((minY-9)/64),y1=Math.floor((maxY+9)/64);
   if((x1-x0+1)*(y1-y0+1)>256){this.large.push(item);return}
   for(let x=x0;x<=x1;x++)for(let y=y0;y<=y1;y++){const key=`${x},${y}`;if(!this.cells.has(key))this.cells.set(key,[]);this.cells.get(key).push(item)}
  }
  pick(x,y){
   let best=null,bestRank=Infinity,bestDistance=Infinity;
   for(const item of [...(this.cells.get(`${Math.floor(x/64)},${Math.floor(y/64)}`)||[]),...this.large]){
    if(x<item.minX-9||x>item.maxX+9||y<item.minY-9||y>item.maxY+9)continue;
    const {f,points}=item;let distance=Infinity,position=[x,y],rank=f.kind==='point'?0:f.kind==='line'?1:2;
    if(f.kind==='point'){position=points[0];distance=Math.hypot(x-position[0],y-position[1]);if(distance>9)continue}
    else{
     for(let i=1;i<points.length+(f.kind==='polygon'?1:0);i++){
      const a=points[i-1],b=points[i%points.length],dx=b[0]-a[0],dy=b[1]-a[1],length=dx*dx+dy*dy;
      const t=length?Math.max(0,Math.min(1,((x-a[0])*dx+(y-a[1])*dy)/length)):0;
      const p=[a[0]+t*dx,a[1]+t*dy],d=Math.hypot(x-p[0],y-p[1]);if(d<distance){distance=d;position=p}
     }
     if(f.kind==='polygon'&&item.filled){
      let inside=false;
      for(let i=0,j=points.length-1;i<points.length;j=i++){
       const a=points[i],b=points[j];if((a[1]>y)!==(b[1]>y)&&x<(b[0]-a[0])*(y-a[1])/(b[1]-a[1])+a[0])inside=!inside;
      }
      if(inside){distance=0;position=[x,y]}
     }
     if(distance>5)continue;
    }
    if(rank<bestRank||rank===bestRank&&(distance<bestDistance||distance===bestDistance&&item.order>best.order)){
     best={f,position,order:item.order};bestRank=rank;bestDistance=distance;
    }
   }
   return best;
  }
 }
 return {describe,layer,lineStyle,backgrounds,HitIndex};
})();
if(typeof module!=='undefined')module.exports=ChartFeatures;
