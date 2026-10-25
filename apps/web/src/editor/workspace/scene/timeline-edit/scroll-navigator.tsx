import {useLayoutEffect,useRef} from "react";
import type {RefObject} from "react";
import {t} from "../../../shared/locale";

export function TimelineScrollNavigator({viewport,controls}:{viewport:RefObject<HTMLDivElement|null>;controls:string}){
 const root=useRef<HTMLDivElement>(null),thumb=useRef<HTMLDivElement>(null);
 const drag=useRef<{id:number;x:number;scroll:number}|null>(null);
 const metrics=()=>{const view=viewport.current,bar=root.current;if(!view||!bar)return null;const width=bar.clientWidth,max=Math.max(0,view.scrollWidth-view.clientWidth),size=Math.min(width,Math.max(32,width*view.clientWidth/Math.max(1,view.scrollWidth)));return{view,width,max,size,travel:width-size};};
 useLayoutEffect(()=>{
  const view=viewport.current,bar=root.current;if(!view||!bar)return;
  const draw=()=>{const m=metrics();if(!m||!thumb.current)return;thumb.current.style.width=`${m.size}px`;thumb.current.style.transform=`translateX(${m.max?m.view.scrollLeft/m.max*m.travel:0}px)`;bar.setAttribute("aria-valuemax",String(m.max));bar.setAttribute("aria-valuenow",String(m.view.scrollLeft));};
  draw();const resize=typeof ResizeObserver==="function"?new ResizeObserver(draw):null;resize?.observe(view);resize?.observe(bar);if(view.firstElementChild)resize?.observe(view.firstElementChild);
  view.addEventListener("scroll",draw,{passive:true});window.addEventListener("resize",draw);return()=>{resize?.disconnect();view.removeEventListener("scroll",draw);window.removeEventListener("resize",draw);};
 });
 return <div className="editor-timeline-scroll-navigator" ref={root} role="scrollbar" tabIndex={0} aria-label={t("Scroll timeline")} aria-controls={controls} aria-orientation="horizontal" aria-valuemin={0}
  onPointerDown={event=>{if(event.button!==0||drag.current)return;const m=metrics();if(!m)return;event.preventDefault();event.stopPropagation();event.currentTarget.focus({preventScroll:true});event.currentTarget.setPointerCapture?.(event.pointerId);
   if(event.target!==thumb.current)m.view.scrollLeft=m.max?Math.max(0,Math.min(m.max,(event.clientX-event.currentTarget.getBoundingClientRect().left-m.size/2)/m.travel*m.max)):0;
   drag.current={id:event.pointerId,x:event.clientX,scroll:m.view.scrollLeft};}}
  onPointerMove={event=>{const d=drag.current,m=metrics();if(!d||d.id!==event.pointerId||!m)return;event.preventDefault();m.view.scrollLeft=m.travel?Math.max(0,Math.min(m.max,d.scroll+(event.clientX-d.x)/m.travel*m.max)):0;}}
  onPointerUp={event=>{if(drag.current?.id!==event.pointerId)return;drag.current=null;if(event.currentTarget.hasPointerCapture?.(event.pointerId))event.currentTarget.releasePointerCapture(event.pointerId);}}
  onPointerCancel={()=>{drag.current=null;}} onLostPointerCapture={()=>{drag.current=null;}}
  onKeyDown={event=>{const m=metrics();if(!m)return;const change=event.key==="ArrowLeft"?-48:event.key==="ArrowRight"?48:event.key==="PageUp"?-m.view.clientWidth:event.key==="PageDown"?m.view.clientWidth:null;
   if(change!==null){event.preventDefault();event.stopPropagation();m.view.scrollLeft=Math.max(0,Math.min(m.max,m.view.scrollLeft+change));}else if(event.key==="Home"||event.key==="End"){event.preventDefault();event.stopPropagation();m.view.scrollLeft=event.key==="Home"?0:m.max;}}}>
  <div className="editor-timeline-scroll-thumb" ref={thumb}/>
 </div>;
}
