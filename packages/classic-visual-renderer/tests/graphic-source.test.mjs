import assert from 'node:assert/strict';
import test from 'node:test';
import { graphicPath, paintGraphicSource, graphicSourceLayout, rasterizeGraphicSource, prepareGraphicSources, resolveGraphicSource, clearGraphicSourceCache, isRetainedGraphicSource } from '../src/graphic-source.mjs';
const plan = (geometry,align='center',widthPx=10) => ({sourceWidth:100,sourceHeight:80,geometry,fill:[1,0,0,0],stroke:{color:[0,1,0,.5],widthPx,align}});
const near=(actual,expected)=>assert.ok(Math.abs(actual-expected)<1e-10,`${actual} != ${expected}`);
class Path {
  commands=[];
  constructor(){ for(const method of ['roundRect','ellipse','moveTo','lineTo','arcTo','closePath'])this[method]=(...args)=>this.commands.push([method,...args]); }
}
function context(width=100,height=80) {
  const calls=[];const ctx={canvas:{width,height},calls};
  for(const method of ['clearRect','fill','stroke','save','restore','clip','drawImage','translate'])ctx[method]=(...args)=>calls.push([method,...args,ctx.lineWidth,ctx.globalCompositeOperation]);
  return ctx;
}
test('rectangle/ellipse center inset and percentage radius use source dimensions',()=>{
  assert.deepEqual(graphicPath(plan({kind:'rectangle',cornerRadiusPercent:50})),[['roundRect',5,5,90,70,35]]);
  assert.deepEqual(graphicPath(plan({kind:'rectangle',cornerRadiusPercent:25},'inside')), [['roundRect',0,0,100,80,20]]);
  assert.deepEqual(graphicPath(plan({kind:'ellipse'})),[['ellipse',50,40,45,35,0,0,Math.PI*2]]);
});
test('polygon counts round at evaluation; first vertex faces up and rounded tangents match geometry',()=>{
  const triangle=graphicPath(plan({kind:'polygon',sides:3.49,cornerRadiusPercent:0},'inside',0));
  assert.equal(triangle.length,4);near(triangle[0][1],50);near(triangle[0][2],0);near(triangle[1][1],50+20*Math.sqrt(3));near(triangle[1][2],60);
  const square=graphicPath(plan({kind:'polygon',sides:3.5,cornerRadiusPercent:50},'inside',0));
  assert.equal(square.filter(c=>c[0]==='arcTo').length,4);near(square[0][1],30);near(square[0][2],20);near(square[1][5],20*Math.sqrt(2));
});
test('star alternates exact depth-scaled radii without treating depth as inverse percentage',()=>{
  const star=graphicPath(plan({kind:'star',points:3,depthPercent:50},'inside',0));
  assert.equal(star.length,7);
  for(let n=0;n<6;n++)near(Math.hypot(star[n][1]-50,star[n][2]-40),n%2===0?40:20);
  near(star[0][2],0);
});
test('inside clips double stroke and restores state; center uses one stroke',()=>{
  for(const align of ['inside','center']) {
    const layout=graphicSourceLayout(plan({kind:'ellipse'},align),{maxPixels:'1000000',maxRasterBytes:'8000000'});const ctx=context(layout.width,layout.height);paintGraphicSource(ctx,plan({kind:'ellipse'},align),()=>{throw Error('unexpected scratch')},null,Path);
    const names=ctx.calls.map(c=>c[0]);assert.equal(names.includes('clip'),align==='inside');
    assert.equal(ctx.calls.find(c=>c[0]==='stroke').at(-2),align==='inside'?20:10);
    assert.equal(ctx.lineJoin,'miter');assert.equal(ctx.miterLimit,10);
    assert.equal(names.filter(n=>n==='restore').length,align==='inside'?2:1);
  }
});
test('outside erases inner stroke even with transparent fill and releases scratch',()=>{
  const ctx=context(300,280),scratch=context(300,280);let creates=0;
  scratch.canvas.getContext=()=>scratch;
  paintGraphicSource(ctx,plan({kind:'ellipse'},'outside'),(w,h)=>{creates++;assert.deepEqual([w,h],[300,280]);return scratch.canvas;},null,Path);
  assert.equal(creates,1);assert.equal(scratch.calls.find(c=>c[0]==='stroke').at(-2),20);
  assert.equal(scratch.calls.find(c=>c[0]==='fill').at(-1),'destination-out');
  assert.deepEqual(ctx.calls.find(c=>c[0]==='drawImage').slice(2,4),[-100,-100]);assert.equal(scratch.canvas.width,0);assert.equal(scratch.canvas.height,0);
});
test('invalid/unknown sampled geometry and cancellation fail before scratch allocation',()=>{
  for(const geometry of [{kind:'star',points:Infinity,depthPercent:45},{kind:'polygon',sides:5,cornerRadiusPercent:NaN},{kind:'legacy_rectangle'}]) assert.throws(()=>graphicPath(plan(geometry)),e=>e.code==='INVALID_VISUAL_PLAN');
  const huge=plan({kind:'ellipse'},'inside');huge.stroke.widthPx=1e308;assert.throws(()=>graphicPath(huge),e=>e.code==='INVALID_VISUAL_PLAN');
  const reason=Error('cancelled');assert.throws(()=>paintGraphicSource(context(),plan({kind:'ellipse'}),()=>{throw Error('allocation')},{aborted:true,reason},Path),e=>e===reason);
});

test('expanded source admission counts simultaneous scratch and rejects before any allocation',()=>{
  const p=plan({kind:'rectangle',cornerRadiusPercent:0},'outside');
  const admitted=graphicSourceLayout(p,{maxPixels:'84000',maxRasterBytes:'672000'});
  assert.deepEqual(admitted,{width:300,height:280,originX:100,originY:100,peakRgbaBytes:672000});
  for(const limits of [{maxPixels:'83999',maxRasterBytes:'672000'},{maxPixels:'84000',maxRasterBytes:'671999'}]) {
    let creates=0;assert.throws(()=>rasterizeGraphicSource(p,limits,()=>{creates++;},null,Path),e=>e.code==='VISUAL_BUDGET_EXCEEDED');assert.equal(creates,0);
  }
});
test('raster returns full source dimensions and origin; failed source canvas is released',()=>{
  const p=plan({kind:'ellipse'},'outside');const canvases=[];
  const result=rasterizeGraphicSource(p,{maxPixels:'84000',maxRasterBytes:'672000'},(w,h)=>{
    const ctx=context(w,h);ctx.canvas.getContext=()=>ctx;canvases.push(ctx.canvas);return ctx.canvas;
  },null,Path);
  assert.deepEqual([result.width,result.height,result.originX,result.originY],[300,280,100,100]);
  assert.equal(result.canvas.width,300);assert.equal(canvases[1].width,0);
  const missing={width:100,height:80,getContext:()=>null};
  assert.throws(()=>rasterizeGraphicSource(plan({kind:'ellipse'},'inside'),{maxPixels:'8000',maxRasterBytes:'32000'},()=>missing,null,Path),e=>e.code==='CAPABILITY_UNAVAILABLE');assert.equal(missing.width,0);
});

test('miter padding is conservative for centered and outside acute corners, zero/inside need none',()=>{
  const limits={maxPixels:'1000000',maxRasterBytes:'8000000'};
  const center=graphicSourceLayout(plan({kind:'polygon',sides:3,cornerRadiusPercent:0},'center'),limits);
  assert.deepEqual([center.width,center.height,center.originX,center.originY],[200,180,50,50]);
  const inside=graphicSourceLayout(plan({kind:'star',points:5,depthPercent:1},'inside'),limits);
  assert.deepEqual([inside.width,inside.height,inside.originX],[100,80,0]);
  const zero=graphicSourceLayout(plan({kind:'ellipse'},'outside',0),limits);
  assert.deepEqual([zero.width,zero.height,zero.originX],[100,80,0]);
});

test('source cache keys every raster parameter, evicts inactive elements and isolates revision/realm',()=>{
 const realm={device:{}},admission={maxDecodedPixels:'1000000',maxRgbaBytes:'8000000'},limits={maxPixels:'1000000',maxRasterBytes:'8000000'};
 const frame={identity:{projectId:'p',revision:'r',sequenceId:'s'},graphics:[{elementId:'g',graphic:plan({kind:'ellipse'},'inside',0)}]};
 let allocations=0;
 const make=(w,h)=>{allocations++;const ctx=context(w,h);ctx.canvas.getContext=()=>ctx;return ctx.canvas;};
 const render=(r=realm,p=frame)=>resolveGraphicSource(prepareGraphicSources(r,p,admission),p.graphics[0],limits,make,null,Path);
 let source=render();assert.equal(render(),source);assert.equal(allocations,1);assert.equal(isRetainedGraphicSource(source),true);
 for(const change of [g=>g.fill[0]=.25,g=>g.stroke.color[0]=.5,g=>g.stroke.widthPx=1,g=>g.stroke.align='center',g=>g.geometry={kind:'star',points:5,depthPercent:45},g=>g.geometry.depthPercent=30,g=>g.sourceWidth=110,g=>g.sourceHeight=90]){
  change(frame.graphics[0].graphic);const next=render();assert.notEqual(next,source);assert.equal(source.canvas.width,0);assert.equal(isRetainedGraphicSource(source),false);source=next;
 }
 const other={device:{}};assert.notEqual(render(other),source);clearGraphicSourceCache(other);
 frame.identity.revision='next';const revision=render();assert.equal(source.canvas.width,0);assert.notEqual(revision,source);
 prepareGraphicSources(realm,{...frame,graphics:[]},admission);assert.equal(revision.canvas.width,0);
 clearGraphicSourceCache(realm);clearGraphicSourceCache(realm);
});
