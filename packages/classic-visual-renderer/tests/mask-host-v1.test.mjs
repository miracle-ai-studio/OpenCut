import test from 'node:test';
import assert from 'node:assert/strict';
import {maskBeginV1} from './mask-fixture.mjs';
import {graphicObservations} from './blend-fixture.mjs';
import {validateMaskBeginV1,validateBlendBeginV1} from '../host/protocol.mjs';
import {browserHooks} from '../host/browser.mjs';
import {executeMaskVisualPageV1} from '../host/page.mjs';
import {validateVideoBegin} from '../host/video-runner.mjs';
import {executeVideoFrame} from '../host/video-browser.mjs';
test('mask begin preserves complete private handles and cannot enter old blend wire',()=>{
 const p=maskBeginV1();assert.equal(validateMaskBeginV1(p).plan.masks.length,2);
 assert.throws(()=>validateBlendBeginV1({...p,type:'beginBlendFrameV1'}));
 for(const change of [p=>p.resources=[],p=>p.plan.masks.reverse(),p=>p.plan.execution.execution.identity.revision='other',p=>p.fonts=[{fontId:'invented',handle:'private'}]]){const q=maskBeginV1();change(q);assert.throws(()=>validateMaskBeginV1(q));}
 assert.equal(validateVideoBegin({...p,type:'beginMaskVideoV1',frameCount:2}).type,'beginMaskVideoV1');
});
test('mask host blocks pixels and injection until its own final bind succeeds once',async()=>{
 const exposed=new Map(),packets=[];let complete;
 await browserHooks.configurePage({begin:validateMaskBeginV1(maskBeginV1()),signal:new AbortController().signal,
 page:{exposeFunction:async(name,fn)=>exposed.set(name,fn)},broker:{request:async(op)=>op==='bindMaskFrameV1'?new Promise(resolve=>complete=resolve):{}},writer:{send:async p=>packets.push(p)}});
 const call=exposed.get('__opencutVisualCallback'),rgba=exposed.get('__opencutVisualRgba'),packet={offsetBytes:'0',bytesHex:'ff0000ff'};
 assert.throws(()=>call('bindMaskFrameV1',{}));await assert.rejects(rgba(packet));
 await call('bindCaptionFrameV3',{rasters:[],captions:[]});await call('bindGraphicFrameV1',graphicObservations);await call('bindBlendFrameV1',{});
 assert.throws(()=>call('bindMaskFrameV1',{masks:[]}));const pending=call('bindMaskFrameV1',{});
 assert.throws(()=>call('bindMaskFrameV1',{}));await assert.rejects(rgba(packet));complete({});await pending;await rgba(packet);assert.equal(packets.length,1);
});
test('native frame and persistent video dispatch choose only the additive mask renderer',async()=>{
 let selected;await browserHooks.render({begin:maskBeginV1(),page:{evaluate:async fn=>{selected=fn;return{ok:true,receipt:{}}}}});assert.equal(selected,executeMaskVisualPageV1);
 globalThis.__opencutVisualModule={VisualDecoderCache:class{},renderClassicMaskFrameV1:async()=>({masks:[]})};
 try{assert.deepEqual(await executeVideoFrame({plan:{},admission:{},profile:'beginMaskFrameV1'}),{ok:true,receipt:{masks:[]}});}finally{for(const key of ['__opencutVisualModule','__opencutVideoBytes','__opencutVideoFonts','__opencutVideoDecoders'])delete globalThis[key];}
});
