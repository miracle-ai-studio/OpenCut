import test from 'node:test';
import assert from 'node:assert/strict';
import { maskFact } from './mask-fixture.mjs';
import { sceneMaskPath, validateSceneMask, prepareSceneSources, sceneFrameSources, rasterizeSceneStroke, releaseSceneSources } from '../src/scene-source.mjs';
import { byteBudget } from '../src/validation.mjs';
import { sceneEffectPasses } from '../src/scene-effects.mjs';
class Path { ops=[];moveTo(...v){this.ops.push(['M',...v]);}lineTo(...v){this.ops.push(['L',...v]);}closePath(){this.ops.push(['Z']);}ellipse(...v){this.ops.push(['E',...v]);}bezierCurveTo(...v){this.ops.push(['C',...v]);} }
const mask=(kind)=>maskFact(kind,0);
test('Classic normalized shape paths retain rotation, unused scale, star radius and heart Beziers',()=>{
  const diamond=mask('diamond'), g=diamond.shape.geometry;Object.assign(g,{width:1,height:1,rotation_degrees:90,scale:8});
  const path=sceneMaskPath(diamond,100,50,Path);assert.deepEqual(path.ops[0],['M',75,25]);assert.equal(path.ops.length,5);
  const star=mask('star');star.shape.geometry.width=star.shape.geometry.height=1;
  const points=sceneMaskPath(star,100,100,Path).ops;assert.equal(points.length,11);assert.ok(Math.abs(Math.hypot(points[1][1]-50,points[1][2]-50)-22.5)<1e-12);
  const heart=sceneMaskPath(mask('heart'),100,100,Path).ops;assert.equal(heart.filter(p=>p[0]==='C').length,2);assert.equal(heart.at(-1)[0],'Z');
});
test('split body clips a half-plane while stroke crosses only the splitting line',()=>{
  const m=mask('rectangle');m.shape={kind:'split',center_x:0,center_y:0,rotation_degrees:0};
  assert.deepEqual(sceneMaskPath(m,100,50,Path,true).ops,[['M',50,0],['L',50,50]]);
  const fill=sceneMaskPath(m,100,50,Path).ops;assert.ok(fill.filter(p=>p.length===3).every(p=>p[1]>=50));
});
test('freeform uses absolute local handles and closes with the stored final tangent',()=>{
  const m=mask('rectangle');m.shape={kind:'freeform',path:{center_x:0,center_y:0,rotation_degrees:0,scale:1,closed:true,points:[{id:'a',x:-.5,y:0,in_x:-.4,in_y:.1,out_x:-.2,out_y:-.3},{id:'b',x:.5,y:0,in_x:.2,in_y:-.3,out_x:.4,out_y:.1},{id:'c',x:0,y:.25,in_x:0,in_y:.25,out_x:0,out_y:.25}]}};
  const ops=sceneMaskPath(m,100,50,Path).ops;assert.deepEqual(ops,[['M',0,25],['C',30,10,70,10,100,25],['C',90,30,50,37.5,50,37.5],['C',50,37.5,10,30,0,25],['Z']]);
  validateSceneMask(m);m.shape.path.points[1].id='a';assert.throws(()=>validateSceneMask(m),e=>e.code==='INVALID_VISUAL_PLAN');
});
test('stroke is a separate non-inverted normal raster and immutable preparation cannot be forged',()=>{
  const m=mask('ellipse');m.inverted=true;m.stroke.width_px=4;m.stroke.align='outside';
  const quad={centerX:50,centerY:25,width:100,height:50,rotationRadians:0,opacity:.2,flipX:false,flipY:false};
  const frame={identity:{projectId:'p',revision:'r',sequenceId:'s',profile:'classic-v31-gpu-scene-frame-v1',width:100,height:50},scene:{layers:[{stack:{elementId:'e',masks:[m],effects:[]},quad}],steps:[{kind:'layer',element_id:'e'}],background:{kind:'color',color:{encoding:'srgb',red:0,green:0,blue:0,alpha:1}}}};
  const calls=[],canvas=(w,h)=>({width:w,height:h,getContext(){return {stroke(p){calls.push(p.ops);},save(){},restore(){},translate(){},rotate(){},scale(){},drawImage(){}};}});
  const state={sources:new Map(),retainedDecoded:0,releases:[],budget:byteBudget('1000000')};
  prepareSceneSources(frame,state,{maxDecodedPixels:'100000',maxRgbaBytes:'1000000'},null,canvas,Path);
  const source=sceneFrameSources(frame).masks.get('e')[0];assert.ok(Object.isFrozen(source.mask.shape.geometry));rasterizeSceneStroke(source,null);
  assert.deepEqual(calls[0],[['E',50,25,32,17,0,0,2*Math.PI]]);
  assert.throws(()=>rasterizeSceneStroke({...source},null),e=>e.code==='INVALID_VISUAL_PLAN');
  frame.scene.layers[0].quad.opacity=.3;assert.throws(()=>sceneFrameSources(frame),e=>e.code==='INVALID_VISUAL_PLAN');
  releaseSceneSources(frame);state.releases.forEach(fn=>fn());
});
test('admission includes every mask stroke and skips an unfinished freeform like Classic',()=>{
  const m=mask('rectangle');m.shape={kind:'freeform',path:{closed:false,center_x:0,center_y:0,rotation_degrees:0,scale:1,points:[]}};
  validateSceneMask(m);
  for(const mutate of [m=>m.shape.path.closed=true,m=>m.feather.value=NaN,m=>m.stroke.color.alpha=2,m=>m.shape.path.scale=undefined,m=>m.shape.path.surprise=true]){const bad=structuredClone(m);mutate(bad);assert.throws(()=>validateSceneMask(bad),e=>e.code==='INVALID_VISUAL_PLAN');}
});
test('Classic blur splits large sigma into bounded pairs and preserves axis scaling',()=>{
  const e=[{id:'blur',enabled:true,effect:{kind:'classic_blur',intensity:100}}];
  assert.deepEqual(sceneEffectPasses(e,1920,1080),[{sigma:20,step:2,direction:[1,0]},{sigma:20,step:2,direction:[0,1]}]);
  const passes=sceneEffectPasses(e,7680,4320);assert.equal(passes.length,8);assert.equal(passes[0].sigma,40);assert.equal(passes[0].step,4);
  assert.deepEqual(sceneEffectPasses([{...e[0],enabled:false}],1920,1080),[]);assert.deepEqual(sceneEffectPasses([{...e[0],effect:{kind:'classic_blur',intensity:0}}],1920,1080),[]);
});
