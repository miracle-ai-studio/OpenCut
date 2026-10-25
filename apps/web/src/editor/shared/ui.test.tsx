// @vitest-environment jsdom
import { useState } from "react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeAll, expect, it } from "vitest";
import { Button, Modal } from "./ui";
beforeAll(()=>{HTMLDialogElement.prototype.showModal=function(){this.setAttribute("open","");this.querySelector<HTMLButtonElement>("button")?.focus();};HTMLDialogElement.prototype.close=function(){this.removeAttribute("open");};});
afterEach(cleanup);
function Harness(){const [open,setOpen]=useState(false);return <><Button onClick={()=>setOpen(true)}>Open export</Button>{open&&<Modal title="Export" onClose={()=>setOpen(false)}><Button onClick={()=>setOpen(false)}>Done</Button></Modal>}</>;}
it("returns focus to the connected opener after the dialog is removed",()=>{
 render(<Harness/>);const opener=screen.getByRole("button",{name:"Open export"});opener.focus();fireEvent.click(opener);expect(document.activeElement).not.toBe(opener);fireEvent.click(screen.getByRole("button",{name:"Done"}));expect(document.activeElement).toBe(opener);expect(screen.queryByRole("dialog")).toBeNull();
});
it("also restores focus after a native cancel event",()=>{
 render(<Harness/>);const opener=screen.getByRole("button",{name:"Open export"});opener.focus();fireEvent.click(opener);fireEvent(screen.getByRole("dialog"),new Event("cancel",{cancelable:true}));expect(document.activeElement).toBe(opener);expect(screen.queryByRole("dialog")).toBeNull();
});
