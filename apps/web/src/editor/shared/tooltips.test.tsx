// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { Tooltips } from "./tooltips";
import { Button } from "./ui";
afterEach(()=>{cleanup();vi.useRealTimers();});
it("keeps managed icon buttons named and suppresses competing native titles",()=>{
 vi.useFakeTimers();render(<Tooltips><Button title="收起属性面板" icon={<svg/>}/></Tooltips>);
 const button=screen.getByRole("button",{name:"收起属性面板"});expect(button.hasAttribute("title")).toBe(false);
 fireEvent.pointerOver(button);act(()=>{vi.advanceTimersByTime(1000);});expect(screen.getByRole("tooltip").textContent).toBe("收起属性面板");
 fireEvent.click(button);expect(screen.queryByRole("tooltip")).toBeNull();expect(button.hasAttribute("title")).toBe(false);
});
it("shows disabled-control explanations only after one second, outside the clipped pane",()=>{
  vi.useFakeTimers();
  const {container}=render(<Tooltips><button disabled title="文字轨道没有声音">轨道静音</button></Tooltips>);
  const button=screen.getByRole("button");fireEvent.pointerOver(button);
  act(()=>{vi.advanceTimersByTime(999);});expect(screen.queryByRole("tooltip")).toBeNull();
  expect(button.hasAttribute("title")).toBe(false);
  act(()=>{vi.advanceTimersByTime(1);});const tip=screen.getByRole("tooltip");
  expect(tip.textContent).toBe("文字轨道没有声音");expect(container.contains(tip)).toBe(false);
  fireEvent.pointerOut(button,{relatedTarget:document.body});expect(screen.queryByRole("tooltip")).toBeNull();
  expect(button.title).toBe("文字轨道没有声音");
});
it("cancels a short hover, clears on scrolling, and supports keyboard focus",()=>{
  vi.useFakeTimers();render(<Tooltips><button title="收起属性面板">收起</button></Tooltips>);
  const button=screen.getByRole("button");fireEvent.pointerOver(button);fireEvent.pointerOut(button);
  act(()=>{vi.advanceTimersByTime(2200);});expect(screen.queryByRole("tooltip")).toBeNull();
  fireEvent.focusIn(button);expect(screen.getByRole("tooltip").textContent).toBe("收起属性面板");
  fireEvent.scroll(window);expect(screen.queryByRole("tooltip")).toBeNull();expect(button.hasAttribute("aria-describedby")).toBe(false);
});
