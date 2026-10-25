import {useEffect,useRef,useState} from "react";
import {createPortal} from "react-dom";
import {MoreHorizontal} from "lucide-react";
import {Button} from "../../../shared/ui";
import {t} from "../../../shared/locale";

export function CardMenu({name,items}:{name:string;items:{label:string;disabled?:boolean;action():void;danger?:boolean}[]}){
 const trigger=useRef<HTMLButtonElement>(null),menu=useRef<HTMLDivElement>(null);
 const [at,setAt]=useState<{left:number;top:number}|null>(null);
 const close=(focus=false)=>{setAt(null);if(focus)trigger.current?.focus();};
 useEffect(()=>{
  if(!at)return;
  menu.current?.querySelector<HTMLElement>('[role="menuitem"]:not(:disabled)')?.focus();
  const outside=(e:PointerEvent)=>{if(!menu.current?.contains(e.target as Node)&&!trigger.current?.contains(e.target as Node))close();};
  const dismiss=()=>close();
  document.addEventListener("pointerdown",outside);window.addEventListener("resize",dismiss);window.addEventListener("scroll",dismiss,true);
  return()=>{document.removeEventListener("pointerdown",outside);window.removeEventListener("resize",dismiss);window.removeEventListener("scroll",dismiss,true);};
 },[at]);
 return <><Button ref={trigger} className="resource-card-more" disabled={items.every(item=>item.disabled)} aria-label={t("More actions for {name}",{name})} title={t("More actions")}
  aria-haspopup="menu" aria-expanded={!!at} icon={<MoreHorizontal size={15}/>}
  onClick={()=>{if(at){close();return;}const r=trigger.current!.getBoundingClientRect();setAt({left:Math.max(8,Math.min(window.innerWidth-200,r.right-184)),top:Math.min(r.bottom+5,window.innerHeight-items.length*34-16)});}}/>
  {at&&createPortal(<div ref={menu} className="resource-card-menu" role="menu" aria-label={t("More actions for {name}",{name})} style={{position:"fixed",...at}}
   onKeyDown={e=>{e.stopPropagation();if(e.key==="Escape"){e.preventDefault();close(true);}else if(["ArrowDown","ArrowUp","Home","End"].includes(e.key)){
    e.preventDefault();const rows=Array.from(menu.current!.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)'));
    const current=rows.indexOf(document.activeElement as HTMLButtonElement),index=e.key==="Home"?0:e.key==="End"?rows.length-1:(current+(e.key==="ArrowDown"?1:-1)+rows.length)%rows.length;rows[index]?.focus();
   }else if(e.key==="Tab")close();}}>
   {items.map(item=><button key={item.label} type="button" role="menuitem" disabled={item.disabled} data-danger={item.danger||undefined}
    onClick={()=>{close(true);item.action();}}>{item.label}</button>)}
  </div>,document.body)}</>;
}
