import {useEffect,useRef} from "react";
import type {RefObject,PointerEvent as ReactPointerEvent} from "react";
import type {Time} from "../../../model/types";
import {pointerTime} from "./geometry";

/** Pointer feedback is immediate. Native seeks are coalesced, always ending
 * at the final pointer position; no inertia or animation on a frame selector. */
export function useTimelineScrub(options:{ruler:RefObject<HTMLDivElement|null>;line:RefObject<HTMLDivElement|null>;scale:number;end:number;blocked:boolean;seek(time:Time):void;begin?():void}){
 const latest=useRef(options);latest.current=options;
 const drag=useRef<{id:number;node:HTMLElement;seconds:number;committed:number;original:Time;timer:ReturnType<typeof setTimeout>|null}|null>(null);
 const commit=()=>{const d=drag.current;if(!d)return;if(d.timer)clearTimeout(d.timer);d.timer=null;d.committed=d.seconds;latest.current.seek(pointerTime(d.seconds));};
 const place=(clientX:number)=>{
  const d=drag.current,r=latest.current.ruler.current;if(!d||!r)return;
  d.seconds=Math.max(0,Math.min(latest.current.end,(clientX-r.getBoundingClientRect().left)/latest.current.scale));
  const line=latest.current.line.current;if(line)line.style.transform=`translateX(${d.seconds*latest.current.scale}px)`;
  r.setAttribute("aria-valuenow",String(d.seconds));
  if(!d.timer)d.timer=setTimeout(commit,80);
 };
 const finish=(cancel=false)=>{
  const d=drag.current;if(!d)return;
  if(d.timer)clearTimeout(d.timer);
  if(cancel)latest.current.seek(d.original);else if(d.committed!==d.seconds)latest.current.seek(pointerTime(d.seconds));
  drag.current=null;if(d.node.hasPointerCapture?.(d.id))d.node.releasePointerCapture(d.id);
 };
 useEffect(()=>{const blur=()=>finish(true);window.addEventListener("blur",blur);return()=>{window.removeEventListener("blur",blur);const d=drag.current;if(d?.timer)clearTimeout(d.timer);drag.current=null;};},[]);
 return {
  active:()=>!!drag.current,
  start:(event:ReactPointerEvent<HTMLElement>,original:Time)=>{
   event.stopPropagation();
   if(event.button!==0||latest.current.blocked||drag.current)return;
   event.preventDefault();event.stopPropagation();
   latest.current.begin?.();
   const node=event.currentTarget;node.focus({preventScroll:true});node.setPointerCapture?.(event.pointerId);
   drag.current={id:event.pointerId,node,seconds:0,committed:-Infinity,original,timer:null};
   place(event.clientX);commit();
  },
  move:(event:ReactPointerEvent<HTMLElement>)=>{if(drag.current?.id!==event.pointerId)return;event.preventDefault();event.stopPropagation();place(event.clientX);},
  end:(event:ReactPointerEvent<HTMLElement>)=>{if(drag.current?.id!==event.pointerId)return;event.preventDefault();event.stopPropagation();place(event.clientX);finish();},
  cancel:()=>finish(true),
 };
}
