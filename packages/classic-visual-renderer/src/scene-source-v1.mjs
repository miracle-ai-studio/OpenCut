// Classic v31 box-like, split and freeform geometry, isolated from editor state.
import { checkAbort, integer, natural, reject, sameJsonFacts } from './validation.mjs';
import { validateSceneTextMaskV1, sceneTextMaskSourceV1, releaseSceneTextMasksV1 } from './scene-text-v1.mjs';
import { exactMaskKeysV1, validateMaskQuadV1 } from './mask-source-v1.mjs';
const prepared = new WeakMap(), issued = new WeakSet();
const number = (v, name) => { if(typeof v!=='number'||!Number.isFinite(v))reject('INVALID_VISUAL_PLAN',`Invalid scene ${name}`); return v; };
export function validateSceneMaskV1(mask) {
  if(!exactMaskKeysV1(mask,['id','enabled','shape','inverted','feather','stroke'])||typeof mask.id!=='string'||!mask.id||typeof mask.enabled!=='boolean'||typeof mask.inverted!=='boolean')reject('INVALID_VISUAL_PLAN','Invalid scene mask');
  if(!mask.enabled)return;
  const shape=mask.shape;
  if(!['split','cinematic_bars','rectangle','ellipse','heart','diamond','star','freeform','text'].includes(shape?.kind))reject('INVALID_VISUAL_PLAN','Unsupported scene mask shape');
  if(!exactMaskKeysV1(mask.stroke,['color','width_px','align'])||number(mask.stroke.width_px,'stroke width')<0||!['inside','center','outside'].includes(mask.stroke.align))reject('INVALID_VISUAL_PLAN','Invalid scene mask stroke');
  const color=mask.stroke.color;
  if(!exactMaskKeysV1(color,['encoding','red','green','blue','alpha'])||color.encoding!=='srgb')reject('INVALID_VISUAL_PLAN','Invalid scene stroke color');
  for(const k of ['red','green','blue','alpha'])if(number(color[k],k)<0||color[k]>1)reject('INVALID_VISUAL_PLAN','Invalid scene stroke channel');
  if(!exactMaskKeysV1(mask.feather,['unit','value'])||mask.feather.unit!=='classic_pixels'||number(mask.feather.value,'feather')<0||!Number.isFinite(Math.fround(mask.feather.value)))reject('INVALID_VISUAL_PLAN','Invalid scene feather');
  const f=Math.fround(mask.feather.value);if(f>0&&Math.fround(f/2)===0)reject('INVALID_VISUAL_PLAN','Scene feather radius underflow');
  if(shape.kind==='text'){if(!exactMaskKeysV1(shape,['kind','mask']))reject('INVALID_VISUAL_PLAN','Invalid text mask fields');validateSceneTextMaskV1(shape.mask);return;}
  const geometry=shape.kind==='split'?shape:shape.kind==='freeform'?shape.path:shape.geometry;
  if(!geometry)reject('INVALID_VISUAL_PLAN','Missing scene mask geometry');
  if(shape.kind==='split') {
    if(!exactMaskKeysV1(shape,['kind','center_x','center_y','rotation_degrees']))reject('INVALID_VISUAL_PLAN','Invalid split fields');
    for(const k of ['center_x','center_y','rotation_degrees'])number(shape[k],k);
  } else if(shape.kind==='freeform') {
    if(!exactMaskKeysV1(shape,['kind','path'])||!exactMaskKeysV1(geometry,['points','closed','center_x','center_y','rotation_degrees','scale'])||!Array.isArray(geometry.points)||geometry.points.length>4096||typeof geometry.closed!=='boolean')reject('INVALID_VISUAL_PLAN','Invalid freeform path');
    for(const k of ['center_x','center_y','rotation_degrees','scale'])number(geometry[k],k);
    if(geometry.scale<0||(geometry.closed&&geometry.points.length<3))reject('INVALID_VISUAL_PLAN','Invalid closed path geometry');
    const ids=new Set();for(const p of geometry.points) {if(!exactMaskKeysV1(p,['id','x','y','in_x','in_y','out_x','out_y'])||typeof p.id!=='string'||!p.id||ids.has(p.id))reject('INVALID_VISUAL_PLAN','Invalid freeform point');ids.add(p.id);for(const k of ['x','y','in_x','in_y','out_x','out_y'])number(p[k],k);}
  } else {
    if(!exactMaskKeysV1(shape,['kind','geometry'])||!exactMaskKeysV1(geometry,['center_x','center_y','width','height','rotation_degrees','scale']))reject('INVALID_VISUAL_PLAN','Invalid scene box fields');
    for(const k of ['center_x','center_y','width','height','rotation_degrees','scale'])number(geometry[k],k);
    if(geometry.width<=0||geometry.height<=0||geometry.scale<0)reject('INVALID_VISUAL_PLAN','Invalid scene mask size');
  }
}
function freezeFacts(v){if(v&&typeof v==='object'){for(const x of Object.values(v))freezeFacts(x);Object.freeze(v);}return v;}
const rotate=(x,y,cx,cy,a)=>[cx+x*Math.cos(a)-y*Math.sin(a),cy+x*Math.sin(a)+y*Math.cos(a)];
export function sceneMaskPathV1(mask,width,height,Path=globalThis.Path2D,stroke=false) {
  const shape=mask.shape,g=shape.kind==='split'?shape:shape.kind==='freeform'?shape.path:shape.geometry;
  const cx=width/2+g.center_x*width,cy=height/2+g.center_y*height,a=g.rotation_degrees*Math.PI/180;
  const path=new Path();
  if(stroke&&shape.kind==='split'){const nx=Math.cos(a),ny=Math.sin(a),hits=[];for(const [x,y,dx,dy] of [[0,0,width,0],[width,0,0,height],[width,height,-width,0],[0,height,0,-height]]){const d=dx*nx+dy*ny;if(Math.abs(d)<1e-10)continue;const t=((cx-x)*nx+(cy-y)*ny)/d;if(t>=-1e-10&&t<=1+1e-10){const hit=[x+t*dx,y+t*dy];if(!hits.some(p=>Math.abs(p[0]-hit[0])<=1e-10&&Math.abs(p[1]-hit[1])<=1e-10))hits.push(hit);}}if(hits.length>=2){path.moveTo(...hits[0]);path.lineTo(...hits[1]);}return path;}
  if(shape.kind==='split') {
    const nx=Math.abs(Math.cos(a))<1e-10?0:Math.cos(a),ny=Math.abs(Math.sin(a))<1e-10?0:Math.sin(a);
    const corners=[[0,0],[width,0],[width,height],[0,height]],output=[];
    const sign=([x,y])=>(x-cx)*nx+(y-cy)*ny;
    for(let i=0;i<4;i++){const p=corners[i],q=corners[(i+1)%4],s=sign(p),t=sign(q);if(s>=0)output.push(p);if((s<0)!==(t<0)){const f=s/(s-t);output.push([p[0]+f*(q[0]-p[0]),p[1]+f*(q[1]-p[1])]);}}
    let area=0;for(let i=0;i<output.length;i++){const p=output[i],q=output[(i+1)%output.length];area+=p[0]*q[1]-q[0]*p[1];}
    if(output.length>=3&&Math.abs(area)/2>=0.5){output.forEach((p,i)=>path[i?'lineTo':'moveTo'](...p));path.closePath();}return path;
  }
  if(shape.kind==='freeform') {
    const point=(p,x,y)=>rotate(p[x]*width*g.scale,p[y]*height*g.scale,cx,cy,a);
    const points=g.points;if(!points.length)return path;path.moveTo(...point(points[0],'x','y'));
    const segment=(p,q)=>path.bezierCurveTo(...point(p,'out_x','out_y'),...point(q,'in_x','in_y'),...point(q,'x','y'));
    for(let i=1;i<points.length;i++)segment(points[i-1],points[i]);if(g.closed&&points.length>1){segment(points.at(-1),points[0]);path.closePath();}return path;
  }
  const offset=stroke?(mask.stroke.align==='inside'?-mask.stroke.width_px/2:mask.stroke.align==='outside'?mask.stroke.width_px/2:0):0;
  const baseHw=(shape.kind==='cinematic_bars'?Math.max(g.width*width,width):Math.max(g.width,0.01)*width)/2,baseHh=Math.max(g.height,0.01)*height/2;
  const hw=stroke?Math.max(1,baseHw+offset):baseHw,hh=stroke?Math.max(1,baseHh+offset):baseHh;
  if(shape.kind==='ellipse'){path.ellipse(cx,cy,hw,hh,a,0,2*Math.PI);return path;}
  const p=(x,y)=>rotate(x,y,cx,cy,a);
  if(shape.kind==='heart'){
    const start=p(0,-hh*.475);path.moveTo(...start);path.bezierCurveTo(...p(hw,-hh*1.225),...p(hw,-hh*.125),...p(0,hh*.725));path.bezierCurveTo(...p(-hw,-hh*.125),...p(-hw,-hh*1.225),...start);path.closePath();return path;
  }
  const points=shape.kind==='star'?Array.from({length:10},(_,i)=>{const r=i%2?.45:1,t=i*Math.PI/5-Math.PI/2;return p(Math.cos(t)*hw*r,Math.sin(t)*hh*r);}):shape.kind==='diamond'?[p(0,-hh),p(hw,0),p(0,hh),p(-hw,0)]:[p(-hw,-hh),p(hw,-hh),p(hw,hh),p(-hw,hh)];
  points.forEach((v,i)=>path[i?'lineTo':'moveTo'](...v));path.closePath();return path;
}
/** Account the complete generated canvas set before preparing any text bitmap. */
export function admitSceneMaskSourcesV1(frame,state,admission,includeText=true){
  const {width,height}=frame.identity;let pixels=0n,rgba=0n;
  for(const layer of frame.scene.layers){validateMaskQuadV1(layer.quad);for(const mask of layer.stack.masks){validateSceneMaskV1(mask);if(!mask.enabled||layer.quad===null||(mask.shape.kind==='freeform'&&!mask.shape.path.closed)||(mask.shape.kind==='text'&&!mask.shape.mask.content.trim()))continue;
    const w=integer(Math.round(layer.quad.width),'local mask width',0,16384),h=integer(Math.round(layer.quad.height),'local mask height',0,16384),local=BigInt(w)*BigInt(h),pair=BigInt(width)*BigInt(height)+local,stroke=mask.stroke.width_px>0?2n:1n;
    pixels+=pair*stroke;rgba+=pair*stroke*4n;
    if(mask.shape.kind==='text'){pixels+=local*stroke;if(includeText)rgba+=local*16n;}
  }}
  if(frame.scene.background.kind==='main_track_blur'&&frame.scene.background.source_element_ids.some(id=>state.sources.has(id))){pixels+=BigInt(width)*BigInt(height);rgba+=BigInt(width)*BigInt(height)*4n;}
  let existing=BigInt(state.retainedDecoded/4);for(const source of new Set(state.sources.values()))if(!source.borrowed)existing+=BigInt(source.width)*BigInt(source.height);
  if(existing+pixels>natural(admission.maxDecodedPixels,'aggregate scene pixels')||state.budget.used+rgba>natural(admission.maxRgbaBytes,'aggregate scene RGBA'))reject('VISUAL_BUDGET_EXCEEDED','Complete scene generated surfaces exceed the original admission');
}
export function prepareSceneSourcesV1(frame,state,admission,signal,createCanvas=(w,h)=>new OffscreenCanvas(w,h),Path=globalThis.Path2D) {
  admitSceneMaskSourcesV1(frame,state,admission,false);
  const masks=new Map(),{width,height}=frame.identity;let pixels=0n;
  const scope=[frame.identity.projectId,frame.identity.revision,frame.identity.sequenceId,frame.identity.profile,width,height];
  for(const layer of frame.scene.layers){
    validateMaskQuadV1(layer.quad);const entries=[];
    for(const raw of layer.stack.masks){validateSceneMaskV1(raw);if(!raw.enabled||layer.quad===null||(raw.shape.kind==='freeform'&&!raw.shape.path.closed)||(raw.shape.kind==='text'&&!raw.shape.mask.content.trim()))continue;
      const quad=structuredClone(layer.quad),mask=structuredClone(raw),localWidth=integer(Math.round(quad.width),'scene local width',0,16384),localHeight=integer(Math.round(quad.height),'scene local height',0,16384);
      const n=BigInt(width)*BigInt(height)+BigInt(localWidth)*BigInt(localHeight);pixels+=n*(raw.stroke.width_px>0?2n:1n);
      const textSource=raw.shape.kind==='text'?sceneTextMaskSourceV1(frame,raw.id):null;
      if(raw.shape.kind==='text'&&!textSource)reject('INVALID_VISUAL_PLAN','Missing fixed text mask raster');
      if(textSource)pixels+=BigInt(textSource.localWidth)*BigInt(textSource.localHeight)*(textSource.stroke?2n:1n);
      entries.push({textSource,sceneSource:true,width,height,localWidth,localHeight,pixels:n,peakRgbaBytes:Number(n*4n*(raw.stroke.width_px>0?2n:1n)),feather:mask.shape.kind==='split'?0:Math.fround(mask.feather.value),inverted:mask.inverted,mask,quad,key:JSON.stringify([scope,mask,quad]),createCanvas,Path});
    }
    if(entries.length)masks.set(layer.stack.elementId,entries);
  }
  let existing=BigInt(state.retainedDecoded/4);for(const s of new Set(state.sources.values()))if(!s.borrowed)existing+=BigInt(s.width)*BigInt(s.height);
  if(existing+pixels>natural(admission.maxDecodedPixels,'scene decoded pixels'))reject('VISUAL_BUDGET_EXCEEDED','Scene mask aggregate exceeds pixel admission');
  for(const entries of masks.values())for(const source of entries){checkAbort(signal);state.releases.push(state.budget.reserve(source.peakRgbaBytes));issued.add(source);freezeFacts(source.mask);freezeFacts(source.quad);Object.freeze(source);}
  let backdrop=null;
  if(frame.scene.background.kind==='main_track_blur'){
    const source=frame.scene.background.source_element_ids.map(id=>state.sources.get(id)).find(Boolean);
    if(source){const n=BigInt(width)*BigInt(height);if(existing+pixels+n>natural(admission.maxDecodedPixels,'scene backdrop pixels'))reject('VISUAL_BUDGET_EXCEEDED','Backdrop exceeds pixel admission');state.releases.push(state.budget.reserve(width*height*4));const canvas=createCanvas(width,height);state.canvases.add(canvas);const ctx=canvas.getContext('2d');if(!ctx)reject('CAPABILITY_UNAVAILABLE','Backdrop Canvas unavailable');const scale=Math.max(width/source.width,height/source.height),w=source.width*scale,h=source.height*scale;ctx.drawImage(source.canvas,(width-w)/2,(height-h)/2,w,h);backdrop={canvas,width,height};}
  }
  prepared.set(frame,{masks,backdrop,scene:structuredClone(frame.scene),identity:structuredClone(frame.identity)});
}
export function sceneFrameSourcesV1(frame){if(frame.scene===undefined)return null;const state=prepared.get(frame);if(!state||!sameJsonFacts(state.scene,frame.scene)||!sameJsonFacts(state.identity,frame.identity))reject('INVALID_VISUAL_PLAN','Scene sources require unchanged final binding');return state;}
export function releaseSceneSourcesV1(frame){prepared.delete(frame);releaseSceneTextMasksV1(frame);}
export function rasterizeSceneMaskV1(source,signal){
  checkAbort(signal);if(!issued.has(source))reject('INVALID_VISUAL_PLAN','Unrecognized scene mask source');
  const {width,height,localWidth,localHeight,mask,quad,createCanvas,Path}=source;let canvas,local,done=false;
  try{canvas=createCanvas(width,height);const ctx=canvas.getContext('2d');if(!ctx)reject('CAPABILITY_UNAVAILABLE','Scene mask Canvas unavailable');
    if(localWidth&&localHeight){local=createCanvas(localWidth,localHeight);const c=local.getContext('2d');if(!c)reject('CAPABILITY_UNAVAILABLE','Local scene mask Canvas unavailable');
      if(mask.shape.kind==='text'){c.drawImage(source.textSource.body,0,0);}
      else if(mask.shape.kind==='split'&&mask.feather.value>0){const g=mask.shape,a=g.rotation_degrees*Math.PI/180,nx=Math.abs(Math.cos(a))<1e-10?0:Math.cos(a),ny=Math.abs(Math.sin(a))<1e-10?0:Math.sin(a),x=quad.width/2+g.center_x*quad.width,y=quad.height/2+g.center_y*quad.height,f=mask.feather.value/2;const gradient=c.createLinearGradient(x-nx*f,y-ny*f,x+nx*f,y+ny*f);gradient.addColorStop(0,'rgba(255,255,255,0)');gradient.addColorStop(1,'white');c.fillStyle=gradient;c.fillRect(0,0,quad.width,quad.height);}
      else{c.fillStyle='white';c.fill(sceneMaskPathV1(mask,quad.width,quad.height,Path));}
      ctx.save();if(quad.rotationRadians!==0||quad.flipX||quad.flipY){ctx.translate(quad.centerX,quad.centerY);ctx.rotate(quad.rotationRadians);ctx.scale(quad.flipX?-1:1,quad.flipY?-1:1);ctx.translate(-quad.centerX,-quad.centerY);}ctx.drawImage(local,quad.centerX-quad.width/2,quad.centerY-quad.height/2,quad.width,quad.height);ctx.restore();}
    checkAbort(signal);done=true;return canvas;
  }finally{if(local){local.width=0;local.height=0;}if(!done&&canvas){canvas.width=0;canvas.height=0;}}
}

// Classic strokes are independent, opaque normal layers after the masked
// foreground. Their width/colour are never softened, inverted or opacity-scaled.
export function rasterizeSceneStrokeV1(source,signal){
  checkAbort(signal);if(!issued.has(source)||source.mask.stroke.width_px<=0)reject('INVALID_VISUAL_PLAN','Unrecognized scene stroke source');
  const {width,height,localWidth,localHeight,mask,quad,createCanvas,Path}=source;let canvas,local,done=false;
  try{canvas=createCanvas(width,height);const ctx=canvas.getContext('2d');if(!ctx)reject('CAPABILITY_UNAVAILABLE','Stroke Canvas unavailable');
    if(localWidth&&localHeight){local=createCanvas(localWidth,localHeight);const c=local.getContext('2d');if(!c)reject('CAPABILITY_UNAVAILABLE','Local stroke Canvas unavailable');
      if(mask.shape.kind==='text'){if(!source.textSource.stroke)reject('INVALID_VISUAL_PLAN','Missing fixed text stroke');c.drawImage(source.textSource.stroke,0,0);}
      else{
      const color=mask.stroke.color;c.strokeStyle=`rgba(${color.red*255},${color.green*255},${color.blue*255},${color.alpha})`;c.lineWidth=mask.stroke.width_px;
      const path=sceneMaskPathV1(mask,quad.width,quad.height,Path,true);
      if(mask.shape.kind==='freeform'){c.lineJoin='round';c.lineCap='round';}
      c.stroke(path);
      if(mask.shape.kind==='freeform'&&mask.stroke.align!=='center'){c.globalCompositeOperation=mask.stroke.align==='inside'?'destination-in':'destination-out';c.fillStyle='white';c.fill(path);}
      }
      ctx.save();ctx.translate(quad.centerX,quad.centerY);ctx.rotate(quad.rotationRadians);ctx.scale(quad.flipX?-1:1,quad.flipY?-1:1);ctx.translate(-quad.centerX,-quad.centerY);ctx.drawImage(local,quad.centerX-quad.width/2,quad.centerY-quad.height/2,quad.width,quad.height);ctx.restore();}
    checkAbort(signal);done=true;return canvas;
  }finally{if(local){local.width=0;local.height=0;}if(!done&&canvas){canvas.width=0;canvas.height=0;}}
}
