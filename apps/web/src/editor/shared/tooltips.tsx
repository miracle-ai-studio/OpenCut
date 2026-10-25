import { createContext, useEffect, useId, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { useLocale } from "./locale";
export const ManagedTooltips=createContext(false);

/** One overlay escapes pane clipping. Native titles are suppressed only while
 * this scope owns the pointer/focus; disabled controls retain their explanations. */
export function Tooltips({ children }: { children: ReactNode }) {
  const id = useId();
  const {language}=useLocale();
  const root = useRef<HTMLDivElement>(null);
  const [tip, setTip] = useState<{ text: string; x: number; y: number; below: boolean; container:Element } | null>(null);
  useEffect(() => {
    const scope = root.current!;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let active: HTMLElement | null = null, originalTitle: string | null = null;
    let description: string | null = null;
    const clear = () => {
      clearTimeout(timer);
      if (active) {
        if (originalTitle !== null && !active.hasAttribute("title")) active.setAttribute("title", originalTitle);
        active.removeAttribute("data-workbench-tooltip");
        if (description === null) active.removeAttribute("aria-describedby");
        else active.setAttribute("aria-describedby", description);
      }
      active = null;setTip(null);
    };
    const show = (event: Event) => {
      const target = event.target instanceof Element ? event.target.closest<HTMLElement>("[title], [data-tooltip], [data-workbench-tooltip]") : null;
      if (!target || !scope.contains(target) || target === active) return;
      clear();
      const text = target.getAttribute("data-tooltip") ?? target.getAttribute("title");
      if (!text?.trim()) return;
      active = target;originalTitle = target.getAttribute("title");description = target.getAttribute("aria-describedby");
      target.removeAttribute("title");target.setAttribute("data-workbench-tooltip", text);
      const reveal = () => {
        if (active !== target || !target.isConnected) return;
        const rect = target.getBoundingClientRect(), below = rect.top < 58;
        target.setAttribute("aria-describedby", [description, id].filter(Boolean).join(" "));
        setTip({ text, x: Math.max(156, Math.min(window.innerWidth - 156, rect.left + rect.width / 2)), y: below ? rect.bottom + 8 : rect.top - 8, below, container:target.closest("dialog[open]")??document.body });
      };
      if (event.type === "focusin") reveal();else timer = setTimeout(reveal, 2000);
    };
    const leave = (event: Event) => {
      const related = (event as FocusEvent).relatedTarget;
      if (active && related instanceof Node && active.contains(related)) return;
      clear();
    };
    const escape = (event: KeyboardEvent) => { if (event.key === "Escape") clear(); };
    scope.addEventListener("pointerover", show);scope.addEventListener("pointerout", leave);
    scope.addEventListener("focusin", show);scope.addEventListener("focusout", leave);
    scope.addEventListener("pointerdown", clear);
    scope.addEventListener("click", clear);
    window.addEventListener("scroll", clear, true);window.addEventListener("resize", clear);window.addEventListener("keydown", escape);
    return () => {
      clear();scope.removeEventListener("pointerover", show);scope.removeEventListener("pointerout", leave);
      scope.removeEventListener("focusin", show);scope.removeEventListener("focusout", leave);scope.removeEventListener("pointerdown", clear);scope.removeEventListener("click", clear);
      window.removeEventListener("scroll", clear, true);window.removeEventListener("resize", clear);window.removeEventListener("keydown", escape);
    };
  }, [id,language]);
  return <ManagedTooltips.Provider value={true}><div ref={root} className="workbench-tooltip-scope">{children}{tip && createPortal(
    <div id={id} role="tooltip" className="workbench-tooltip" style={{ left: tip.x, top: tip.y, transform: `translate(-50%, ${tip.below ? "0" : "-100%"})` }}>{tip.text}</div>, tip.container,
  )}</div></ManagedTooltips.Provider>;
}
