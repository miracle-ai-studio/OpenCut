// @vitest-environment jsdom
import {useRef} from "react";
import {act,cleanup,fireEvent,render,screen} from "@testing-library/react";
import {afterEach,beforeEach,expect,it,vi} from "vitest";
import {useTimelineScrub} from "./scrub";
import {TimelineScrollNavigator} from "./scroll-navigator";
import type {Time} from "../../../model/types";

beforeEach(()=>{
 vi.useFakeTimers();
 Object.defineProperty(window,"PointerEvent",{configurable:true,value:class extends MouseEvent{pointerId:number;constructor(type:string,init:PointerEventInit){super(type,init);this.pointerId=init.pointerId??1;}}});
 HTMLElement.prototype.setPointerCapture=vi.fn();HTMLElement.prototype.hasPointerCapture=vi.fn(()=>true);HTMLElement.prototype.releasePointerCapture=vi.fn();
 vi.spyOn(HTMLElement.prototype,"getBoundingClientRect").mockReturnValue({left:20,top:0,width:200,height:20,right:220,bottom:20,x:20,y:0,toJSON(){}});
});
afterEach(()=>{cleanup();vi.restoreAllMocks();vi.useRealTimers();});
function Harness({seek,begin}:{seek(time:Time):void;begin():void}){
 const ruler=useRef<HTMLDivElement>(null),line=useRef<HTMLDivElement>(null),scrub=useTimelineScrub({ruler,line,scale:100,end:8,blocked:false,seek,begin});
 return <div><div ref={ruler} role="slider" aria-label="ruler" onPointerDown={e=>scrub.start(e,{ticks:"1",timebase:1})} onPointerMove={scrub.move} onPointerUp={scrub.end} onPointerCancel={scrub.cancel}/><div ref={line} data-testid="line"/></div>;
}
it("follows every pointer immediately, coalesces native seeks, ignores a second pointer and commits the final release",()=>{
 const seek=vi.fn(),begin=vi.fn();render(<Harness seek={seek} begin={begin}/>);const ruler=screen.getByRole("slider");
 fireEvent.pointerDown(ruler,{button:0,pointerId:2,clientX:120});expect(begin).toHaveBeenCalledOnce();expect(seek).toHaveBeenCalledTimes(1);
 fireEvent.pointerMove(ruler,{pointerId:2,clientX:250});fireEvent.pointerMove(ruler,{pointerId:2,clientX:320});
 expect(screen.getByTestId("line").style.transform).toBe("translateX(300px)");expect(seek).toHaveBeenCalledTimes(1);
 fireEvent.pointerMove(ruler,{pointerId:3,clientX:700});expect(screen.getByTestId("line").style.transform).toBe("translateX(300px)");
 act(()=>vi.advanceTimersByTime(80));expect(seek).toHaveBeenCalledTimes(2);
 fireEvent.pointerUp(ruler,{pointerId:2,clientX:420});expect(seek).toHaveBeenCalledTimes(3);const end=seek.mock.lastCall![0];expect(Number(end.ticks)/end.timebase).toBe(4);
 act(()=>vi.advanceTimersByTime(500));expect(seek).toHaveBeenCalledTimes(3);expect(HTMLElement.prototype.releasePointerCapture).toHaveBeenCalledWith(2);
});
it("clamps overshoot and cancellation restores the original position without a delayed seek",()=>{
 const seek=vi.fn();render(<Harness seek={seek} begin={()=>{}}/>);const ruler=screen.getByRole("slider");
 fireEvent.pointerDown(ruler,{button:0,pointerId:1,clientX:2000});expect(screen.getByTestId("line").style.transform).toBe("translateX(800px)");
 fireEvent.pointerMove(ruler,{pointerId:1,clientX:-50});expect(screen.getByTestId("line").style.transform).toBe("translateX(0px)");
 fireEvent.pointerCancel(ruler,{pointerId:1});expect(seek).toHaveBeenLastCalledWith({ticks:"1",timebase:1});const calls=seek.mock.calls.length;
 act(()=>vi.advanceTimersByTime(500));expect(seek).toHaveBeenCalledTimes(calls);
});
it("scroll navigator moves the viewport directly and consumes keyboard navigation before clip shortcuts",()=>{
 const bubbled=vi.fn();function Scroll(){const viewport=useRef<HTMLDivElement>(null);return <div onKeyDown={bubbled}><div ref={viewport} data-testid="viewport" id="timeline"/><TimelineScrollNavigator viewport={viewport} controls="timeline"/></div>;}
 render(<Scroll/>);const view=screen.getByTestId("viewport"),bar=screen.getByRole("scrollbar");
 Object.defineProperties(view,{clientWidth:{configurable:true,value:200},scrollWidth:{configurable:true,value:1000}});Object.defineProperty(bar,"clientWidth",{configurable:true,value:200});
 fireEvent.pointerDown(bar,{button:0,pointerId:4,clientX:120});fireEvent.pointerMove(bar,{pointerId:4,clientX:180});expect(view.scrollLeft).toBe(700);fireEvent.pointerMove(bar,{pointerId:4,clientX:300});expect(view.scrollLeft).toBe(800);fireEvent.pointerUp(bar,{pointerId:4});
 fireEvent.keyDown(bar,{key:"Home"});expect(view.scrollLeft).toBe(0);fireEvent.keyDown(bar,{key:"ArrowRight"});expect(view.scrollLeft).toBe(48);expect(bubbled).not.toHaveBeenCalled();
});
it("releases a drag when the window loses focus so the next gesture still works",()=>{
 const seek=vi.fn(),begin=vi.fn();render(<Harness seek={seek} begin={begin}/>);const ruler=screen.getByRole("slider");
 fireEvent.pointerDown(ruler,{button:0,pointerId:7,clientX:120});fireEvent.pointerMove(ruler,{pointerId:7,clientX:520});fireEvent.blur(window);
 expect(seek).toHaveBeenLastCalledWith({ticks:"1",timebase:1});const calls=seek.mock.calls.length;act(()=>vi.advanceTimersByTime(100));expect(seek).toHaveBeenCalledTimes(calls);
 fireEvent.pointerDown(ruler,{button:0,pointerId:8,clientX:320});expect(begin).toHaveBeenCalledTimes(2);fireEvent.pointerUp(ruler,{pointerId:8,clientX:320});
 expect(Number(seek.mock.lastCall![0].ticks)/seek.mock.lastCall![0].timebase).toBe(3);
});
