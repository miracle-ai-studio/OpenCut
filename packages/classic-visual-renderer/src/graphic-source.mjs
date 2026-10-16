import { checkAbort, finite, integer, natural, reject } from './validation.mjs';

// Classic cf5e79e graphics/definitions and graphics/stroke.ts (MIT).
// Receives sampled values: no animation evaluation, parameter defaults or assets.
const clamp = (value, min, max) => Math.max(min, Math.min(max, value));
const rgba = value => {
  if (!Array.isArray(value) || value.length !== 4 || value.some(v => finite(v, 'graphic color') < 0 || v > 1)) reject('INVALID_VISUAL_PLAN', 'Invalid graphic RGBA');
  return `rgba(${value[0]*255}, ${value[1]*255}, ${value[2]*255}, ${value[3]})`;
};
function validateGraphic(plan) {
  if(!plan?.geometry || !plan.stroke) reject('INVALID_VISUAL_PLAN','Missing sampled graphic');
  integer(plan.sourceWidth,'graphic width',1,16384);integer(plan.sourceHeight,'graphic height',1,16384);
  if(!['inside','center','outside'].includes(plan.stroke.align) || finite(plan.stroke.widthPx,'graphic stroke width')<0 || !Number.isFinite(plan.stroke.widthPx*2)) reject('INVALID_VISUAL_PLAN','Invalid graphic stroke');
  rgba(plan.fill);rgba(plan.stroke.color);
  const g=plan.geometry;
  if(g.kind==='rectangle') finite(g.cornerRadiusPercent,'rectangle radius');
  else if(g.kind==='polygon') {finite(g.sides,'polygon sides');finite(g.cornerRadiusPercent,'polygon radius');}
  else if(g.kind==='star') {finite(g.points,'star points');finite(g.depthPercent,'star depth');}
  else if(g.kind!=='ellipse') reject('INVALID_VISUAL_PLAN','Unsupported graphic geometry');
}
export function graphicPathV1(plan) {
  validateGraphic(plan);const width=plan.sourceWidth,height=plan.sourceHeight,stroke=plan.stroke;
  const inset=stroke.align==='center'?stroke.widthPx/2:0,g=plan.geometry;
  if (!g) reject('INVALID_VISUAL_PLAN','Missing graphic geometry');
  if (g.kind==='rectangle') {
    const w=Math.max(1,width-inset*2),h=Math.max(1,height-inset*2);
    return [['roundRect',inset,inset,w,h,Math.min(w,h)/2*clamp(finite(g.cornerRadiusPercent,'rectangle radius'),0,50)/50]];
  }
  if (g.kind==='ellipse') return [['ellipse',width/2,height/2,Math.max(1,width/2-inset),Math.max(1,height/2-inset),0,0,Math.PI*2]];
  const radius=Math.max(1,Math.min(width,height)/2-inset),commands=[];
  if (g.kind==='star') {
    const points=clamp(Math.round(finite(g.points,'star points')),3,12),depth=clamp(finite(g.depthPercent,'star depth'),1,99)/100;
    for(let n=0;n<points*2;n++) {
      const r=n%2===0?radius:radius*depth,angle=-Math.PI/2+n*Math.PI/points;
      commands.push([n===0?'moveTo':'lineTo',width/2+Math.cos(angle)*r,height/2+Math.sin(angle)*r]);
    }
  } else if(g.kind==='polygon') {
    const sides=clamp(Math.round(finite(g.sides,'polygon sides')),3,12),corner=radius*Math.sin(Math.PI/sides)*clamp(finite(g.cornerRadiusPercent,'polygon radius'),0,50)/50;
    const vertices=Array.from({length:sides},(_,n)=>{const angle=-Math.PI/2+n*2*Math.PI/sides;return [width/2+Math.cos(angle)*radius,height/2+Math.sin(angle)*radius];});
    if(corner<=0) vertices.forEach((v,n)=>commands.push([n===0?'moveTo':'lineTo',...v]));
    else vertices.forEach((current,n)=>{
      const previous=vertices[(n+sides-1)%sides],next=vertices[(n+1)%sides];
      const previousLength=Math.hypot(previous[0]-current[0],previous[1]-current[1]),nextLength=Math.hypot(next[0]-current[0],next[1]-current[1]);
      const a=[(previous[0]-current[0])/(previousLength||1),(previous[1]-current[1])/(previousLength||1)],b=[(next[0]-current[0])/(nextLength||1),(next[1]-current[1])/(nextLength||1)];
      const angle=Math.acos(clamp(a[0]*b[0]+a[1]*b[1],-1,1)),maxOffset=Math.min(previousLength,nextLength)/2,offset=Math.min(corner/Math.tan(angle/2),maxOffset);
      commands.push([n===0?'moveTo':'lineTo',current[0]+a[0]*offset,current[1]+a[1]*offset],['arcTo',...current,current[0]+b[0]*offset,current[1]+b[1]*offset,Math.min(corner,maxOffset)]);
    });
  } else reject('INVALID_VISUAL_PLAN','Unsupported graphic geometry');
  commands.push(['closePath']);return commands;
}

// New GraphicFrameV1 fixes Classic's source-edge clipping for centered/outside
// miter strokes. Conservative padding matches half-width * miterLimit (10).
// Logical coordinates remain unchanged; symmetric padding keeps their center.
function sourceBounds(plan) {
  const multiplier=plan.stroke.align==='outside'?10:plan.stroke.align==='center'?5:0;
  const offset=Math.ceil(plan.stroke.widthPx*multiplier);
  integer(offset,'graphic outside padding',0,16384);
  return {width:integer(plan.sourceWidth+offset*2,'graphic raster width',1,16384),height:integer(plan.sourceHeight+offset*2,'graphic raster height',1,16384),originX:offset,originY:offset};
}
export function graphicSourceLayoutV1(plan, admission) {
  validateGraphic(plan);
  const layout=sourceBounds(plan),pixels=BigInt(layout.width)*BigInt(layout.height);
  // Main source plus simultaneous outside-stroke scratch. GPU textures are
  // independently accounted by the existing GPU allocator.
  const peakRgbaBytes=pixels*4n*(plan.stroke.align==='outside'&&plan.stroke.widthPx>0?2n:1n);
  if(pixels>natural(admission.maxPixels,'graphic source pixels') || peakRgbaBytes>natural(admission.maxRasterBytes,'graphic raster bytes')) reject('VISUAL_BUDGET_EXCEEDED','Graphic raster exceeds explicit admission');
  return {...layout,peakRgbaBytes:Number(peakRgbaBytes)};
}

// Called only after the complete layout admission above. Canvas creation is
// injected so the frame owner can track and release every admitted allocation.
export function paintGraphicSourceV1(ctx, plan, createCanvas, signal, Path=globalThis.Path2D) {
  checkAbort(signal);
  const commands=graphicPathV1(plan),layout=sourceBounds(plan);
  if(typeof Path!=='function') reject('CAPABILITY_UNAVAILABLE','Graphic Path2D unavailable');
  if(ctx.canvas.width!==layout.width || ctx.canvas.height!==layout.height) reject('INVALID_VISUAL_PLAN','Graphic canvas size differs from admission');
  const path=new Path();for(const [name,...args] of commands) path[name](...args);
  ctx.clearRect(0,0,layout.width,layout.height);ctx.save();
  try {
    ctx.translate(layout.originX,layout.originY);ctx.globalAlpha=1;ctx.globalCompositeOperation='source-over';ctx.fillStyle=rgba(plan.fill);ctx.fill(path);
    const stroke=plan.stroke;
    if(stroke.widthPx>0) {
      const apply=(target,width)=>{target.strokeStyle=rgba(stroke.color);target.lineWidth=width;target.lineJoin='miter';target.miterLimit=10;target.stroke(path);};
      if(stroke.align==='inside') {ctx.save();try {ctx.clip(path);apply(ctx,stroke.widthPx*2);} finally {ctx.restore();}}
      else if(stroke.align==='outside') {
        checkAbort(signal);const canvas=createCanvas(layout.width,layout.height);
        try {
          const scratch=canvas.getContext('2d');if(!scratch) reject('CAPABILITY_UNAVAILABLE','Graphic stroke canvas unavailable');
          scratch.translate(layout.originX,layout.originY);apply(scratch,stroke.widthPx*2);scratch.globalCompositeOperation='destination-out';scratch.fill(path);checkAbort(signal);ctx.drawImage(canvas,-layout.originX,-layout.originY);
        } finally {canvas.width=0;canvas.height=0;}
      } else apply(ctx,stroke.widthPx);
    }
    checkAbort(signal);return ctx.canvas;
  } finally {ctx.restore();}
}
export function rasterizeGraphicSourceV1(plan, admission, createCanvas, signal, Path=globalThis.Path2D) {
  checkAbort(signal);const {peakRgbaBytes,...layout}=graphicSourceLayoutV1(plan,admission);
  const canvas=createCanvas(layout.width,layout.height);let complete=false;
  try {
    const ctx=canvas.getContext('2d');if(!ctx) reject('CAPABILITY_UNAVAILABLE','Graphic source canvas unavailable');
    paintGraphicSourceV1(ctx,plan,createCanvas,signal,Path);complete=true;
    return {canvas,...layout};
  } finally {if(!complete){canvas.width=0;canvas.height=0;}}
}

// Classic GraphicNode retains the last raster for unchanged resolved params.
// Keep that identity here without retaining any frame/quad/authorization facts.
// Only objects issued by this module can opt into GPU source-texture reuse;
// decoded video canvases may mutate and must never use that path.
const retainedGraphicSources = new WeakSet();
const graphicSourceCaches = new WeakMap();
export const isRetainedGraphicSourceV1 = source => retainedGraphicSources.has(source);
export function clearGraphicSourceCacheV1(realm) {
  const cache = realm && graphicSourceCaches.get(realm);
  if (!cache) return;
  for (const { source } of cache.entries.values()) {
    retainedGraphicSources.delete(source);
    source.canvas.width = 0; source.canvas.height = 0;
  }
  graphicSourceCaches.delete(realm);
}

// Called before any frame source allocation. Admission includes every current
// raster, hit or miss; obsolete entries are destroyed before that reservation.
export function prepareGraphicSourcesV1(realm, plan, admission) {
  const limits = { maxPixels: admission.maxDecodedPixels, maxRasterBytes: admission.maxRgbaBytes };
  const layouts = plan.graphics.map(layer => graphicSourceLayoutV1(layer.graphic, limits));
  const keys = new Map(plan.graphics.map(layer => [layer.elementId, JSON.stringify(layer.graphic)]));
  let cache;
  if (realm?.device && !realm.closed) {
    const scope = JSON.stringify([plan.identity.projectId, plan.identity.revision, plan.identity.sequenceId]);
    if (graphicSourceCaches.get(realm)?.scope !== scope) clearGraphicSourceCacheV1(realm);
    if (!graphicSourceCaches.has(realm)) graphicSourceCaches.set(realm, { scope, entries: new Map() });
    cache = graphicSourceCaches.get(realm).entries;
    for (const [id, entry] of cache) {
      if (keys.get(id) !== entry.key || entry.source.canvas.width !== entry.source.width || entry.source.canvas.height !== entry.source.height) {
        retainedGraphicSources.delete(entry.source);
        entry.source.canvas.width = 0; entry.source.canvas.height = 0;
        cache.delete(id);
      }
    }
  }
  return { layouts, keys, cache };
}
export function resolveGraphicSourceV1(prepared, layer, admission, createCanvas, signal, Path) {
  checkAbort(signal);
  const previous = prepared.cache?.get(layer.elementId);
  if (previous) return previous.source;
  const source = rasterizeGraphicSourceV1(layer.graphic, admission, createCanvas, signal, Path);
  if (prepared.cache) {
    Object.freeze(source);
    retainedGraphicSources.add(source);
    prepared.cache.set(layer.elementId, { key: prepared.keys.get(layer.elementId), source });
  }
  return source;
}
