import test from "node:test";
import assert from "node:assert/strict";
import { wrapCaptionText, prepareCaptionText, CAPTION_WRAP_PROFILE } from "../src/caption-layout.mjs";
test("Classic greedy whitespace preserves paragraph breaks and oversized CJK/words",()=>{
  const widths=new Map([["AV To",40],["AV To world",110],["world",70],["",0],["中文编辑连续不拆分",180]]);
  const result=wrapCaptionText("  AV   To world\r\n\n中文编辑连续不拆分  ",80,text=>widths.get(text));
  assert.equal(result.wrappedText,"AV To\nworld\n\n中文编辑连续不拆分");
  assert.deepEqual(result.measurements.map(item=>item.content),["AV To","AV To world","AV To","world","","中文编辑连续不拆分"]);
  assert.equal(wrapCaptionText("averylongword",10,()=>300).wrappedText,"averylongword");
});
test("only fixed finite measurements are admitted",()=>{
  assert.throws(()=>wrapCaptionText("AV To",80,()=>NaN));
  assert.throws(()=>wrapCaptionText("AV To",80,()=>-1));
});
function ctx() {return {font:"",letterSpacing:"",save(){},restore(){},measureText(text){return {width:text.length*10,actualBoundingBoxAscent:7,actualBoundingBoxDescent:3};}};}
function text(align="center") {return {content:"AV To world",fontSizePx:30,lineHeightPx:36,letterSpacingPx:0,align,decoration:"none",color:[1,1,1,1],background:{enabled:false,color:[0,0,0,0],paddingXPx:0,paddingYPx:0,offsetXPx:0,offsetYPx:0,cornerRadiusPercent:0},transform:{centerX:160,centerY:90,scaleX:1,scaleY:1,rotationRadians:0}};}
test("wrap/placement consumes exact font metrics, 80% width and 5% bottom",()=>{
  const original=text(),directive={wrapProfile:CAPTION_WRAP_PROFILE,maxWidthPx:80,bottomMarginPx:9};
  const result=prepareCaptionText(ctx(),original,"fixed",{weight:400,style:"normal"},directive,{width:320,height:180});
  assert.equal(result.text.content,"AV To\nworld");
  assert.equal(result.text.transform.centerX,160); assert.equal(result.text.transform.centerY,135);
  assert.equal(original.content,"AV To world"); assert.equal(original.transform.centerY,90);
  assert.equal(prepareCaptionText(ctx(),text("left"),"fixed",{weight:400,style:"normal"},directive,{width:320,height:180}).text.transform.centerX,-0);
  assert.equal(prepareCaptionText(ctx(),text("right"),"fixed",{weight:400,style:"normal"},directive,{width:320,height:180}).text.transform.centerX,320);
});
test("caption background visual rectangle controls bottom margin",()=>{
  const original=text(); original.background={...original.background,enabled:true,paddingYPx:5,offsetYPx:2};
  const result=prepareCaptionText(ctx(),original,"fixed",{weight:400,style:"normal"},{wrapProfile:CAPTION_WRAP_PROFILE,maxWidthPx:80,bottomMarginPx:9},{width:320,height:180});
  assert.equal(result.text.transform.centerY,128);
});
