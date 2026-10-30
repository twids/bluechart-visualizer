/* SPDX-License-Identifier: GPL-3.0-only */
'use strict';
// Perspective camera over the chart plane. Coordinates are projected map units;
// no elevation or bathymetry is inferred from chart colours.
const ChartView=(()=>{
 class Camera{
  constructor({width,height,center,scale,pitch=0}){
   Object.assign(this,{width,height,center:[...center],scale,pitch});
   const angle=pitch*Math.PI/180;
   this.cos=Math.cos(angle);this.sin=Math.sin(angle);this.distance=Math.max(1,height)*1.5;
  }
  screen([x,y]){
   const dx=(x-this.center[0])*this.scale,dy=(this.center[1]-y)*this.scale;
   const w=1-dy*this.sin/this.distance;
   if(w<=0)return [NaN,NaN];
   return [this.width/2+dx/w,this.height/2+dy*this.cos/w];
  }
  ground(x,y){
   const dx=x-this.width/2,dy=y-this.height/2;
   const flatY=dy/(this.cos+dy*this.sin/this.distance),w=1-flatY*this.sin/this.distance;
   return [this.center[0]+dx*w/this.scale,this.center[1]-flatY/this.scale];
  }
  bounds(margin=0){
   const corners=[[-margin,-margin],[this.width+margin,-margin],[this.width+margin,this.height+margin],[-margin,this.height+margin]].map(p=>this.ground(...p));
   return [Math.min(...corners.map(p=>p[0])),Math.min(...corners.map(p=>p[1])),Math.max(...corners.map(p=>p[0])),Math.max(...corners.map(p=>p[1]))];
  }
  paths(points,closed=false){
   // Clip on the chart plane before perspective division. Large offscreen
   // polygons can otherwise cross behind the camera and flip onto the map.
   const bounds=this.clipBounds||(this.clipBounds=this.bounds(Math.min(12,this.height*.1)));
   if(closed){
    let out=points;
    for(const [axis,edge,sign] of [[0,bounds[0],1],[0,bounds[2],-1],[1,bounds[1],1],[1,bounds[3],-1]]){
     const input=out;out=[];if(!input.length)break;
     let a=input.at(-1),insideA=(a[axis]-edge)*sign>=0;
     for(const b of input){
      const insideB=(b[axis]-edge)*sign>=0;
      if(insideA!==insideB){const t=(edge-a[axis])/(b[axis]-a[axis]);out.push([a[0]+t*(b[0]-a[0]),a[1]+t*(b[1]-a[1])])}
      if(insideB)out.push(b);a=b;insideA=insideB;
     }
    }
    return out.length>=3?[out.map(p=>this.screen(p))]:[];
   }
   const paths=[];let path=null;
   for(let i=1;i<points.length;i++){
    const a=points[i-1],b=points[i],dx=b[0]-a[0],dy=b[1]-a[1];let low=0,high=1,visible=true;
    for(const [p,q] of [[-dx,a[0]-bounds[0]],[dx,bounds[2]-a[0]],[-dy,a[1]-bounds[1]],[dy,bounds[3]-a[1]]]){
     if(p===0){if(q<0){visible=false;break}continue}
     const t=q/p;if(p<0)low=Math.max(low,t);else high=Math.min(high,t);
     if(low>high){visible=false;break}
    }
    if(!visible){path=null;continue}
    const start=this.screen([a[0]+low*dx,a[1]+low*dy]),end=this.screen([a[0]+high*dx,a[1]+high*dy]);
    if(path&&low===0){path.push(end)}else{path=[start,end];paths.push(path)}
    if(high<1)path=null;
   }
   return paths;
  }
 }
 return {Camera};
})();
if(typeof module!=='undefined')module.exports=ChartView;
