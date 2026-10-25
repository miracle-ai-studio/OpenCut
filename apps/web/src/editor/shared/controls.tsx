import { useEffect, useId, useImperativeHandle, useLayoutEffect, useRef, useState, type Ref, type SelectHTMLAttributes } from "react";
import { Pause, Play, Volume2, VolumeX, Palette } from "lucide-react";
import { t, useLocale } from "./locale";
import { Button } from "./ui";

/** Keep the form value and keyboard focus on the semantic select. Its popup is
 * drawn by OpenCut in the top layer, including inside dialogs and scroll panels. */
export function Select({ children, onKeyDown, onPointerDown, onBlur, className = "", ...props }: SelectHTMLAttributes<HTMLSelectElement>) {
  useLocale();
  const input = useRef<HTMLSelectElement>(null), menu = useRef<HTMLDivElement>(null), id = useId();
  const [open, setOpen] = useState(false), [active, setActive] = useState(0);
  const [items, setItems] = useState<Array<{ value: string; label: string; disabled: boolean; selected: boolean }>>([]);
  const search = useRef({ text: "", time: 0 });
  const blocked = () => !input.current || input.current.matches(":disabled");
  const show = () => { if (!blocked()) { input.current!.focus(); setActive(Math.max(0, input.current!.selectedIndex)); setOpen(true); } };
  const choose = (index: number) => {
    const element = input.current, option = element?.options[index];
    if (!element || blocked() || !option || option.disabled || (option.parentElement instanceof HTMLOptGroupElement && option.parentElement.disabled)) return;
    if (element.multiple) option.selected = !option.selected; else element.value = option.value;
    element.dispatchEvent(new Event("change", { bubbles: true }));
    setOpen(false); element.focus();
  };
  useLayoutEffect(() => {
    if (!open || !input.current || !menu.current) return;
    setItems(Array.from(input.current.options, option => ({ value: option.value, label: option.label,
      selected: option.selected, disabled: option.disabled || (option.parentElement instanceof HTMLOptGroupElement && option.parentElement.disabled) })));
    const popup = menu.current;
    const position = () => {
      if (!input.current) return;
      const rect = input.current.getBoundingClientRect(), below = innerHeight - rect.bottom - 8;
      const height = Math.min(280, Math.max(below, rect.top - 8));
      Object.assign(popup.style, { margin: "0", width: `${Math.min(Math.max(rect.width, 180), innerWidth - 16)}px`,
        left: `${Math.max(8, Math.min(rect.left, innerWidth - Math.max(rect.width, 180) - 8))}px`,
        maxHeight: `${height}px`, top: below >= Math.min(180, height) ? `${rect.bottom + 4}px` : "auto",
        bottom: below >= Math.min(180, height) ? "auto" : `${innerHeight - rect.top + 4}px` });
    };
    position(); popup.showPopover?.();
    const outside = (event: PointerEvent) => { if (!input.current?.contains(event.target as Node) && !popup.contains(event.target as Node)) setOpen(false); };
    document.addEventListener("pointerdown", outside, true);
    window.addEventListener("resize", position); window.addEventListener("scroll", position, true);
    return () => { document.removeEventListener("pointerdown", outside, true); window.removeEventListener("resize", position); window.removeEventListener("scroll", position, true); if (popup.hidePopover && popup.matches(":popover-open")) popup.hidePopover(); };
  }, [open, children]);
  useEffect(() => { menu.current?.querySelector(`[data-active="true"]`)?.scrollIntoView?.({ block: "nearest" }); }, [open, active, items]);
  return <span className="select-control">
    <select {...props} className={className} ref={input} aria-expanded={open} aria-controls={open ? id : undefined}
      aria-activedescendant={open ? `${id}-${active}` : undefined}
      onPointerDown={event => { onPointerDown?.(event); if (!event.defaultPrevented && event.button === 0) { event.preventDefault(); open ? setOpen(false) : show(); } }}
      onMouseDown={event => event.preventDefault()}
      onClick={event => event.preventDefault()}
      onBlur={event => { onBlur?.(event); if (!menu.current?.contains(event.relatedTarget)) setOpen(false); }}
      onKeyDown={event => {
        onKeyDown?.(event); if (event.defaultPrevented || blocked()) return;
        if (event.key === "Tab") { setOpen(false); return; }
        if (event.key === "Escape" && open) { event.preventDefault(); event.stopPropagation(); setOpen(false); return; }
        if (["Enter", " "].includes(event.key)) { event.preventDefault(); open ? choose(active) : show(); return; }
        const options = Array.from(input.current!.options), enabled = options.map((option, index) => ({ option, index })).filter(({ option }) => !option.disabled && !(option.parentElement instanceof HTMLOptGroupElement && option.parentElement.disabled));
        if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
          event.preventDefault();
          if (!open) { show(); return; }
          const current = enabled.findIndex(item => item.index === active), step = event.key === "ArrowUp" ? -1 : 1;
          const next = event.key === "Home" ? 0 : event.key === "End" ? enabled.length - 1 : Math.max(0, Math.min(enabled.length - 1, current + step));
          if (enabled[next]) setActive(enabled[next].index);
        } else if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
          event.preventDefault(); const now = Date.now(); search.current = { text: (now - search.current.time < 700 ? search.current.text : "") + event.key.toLocaleLowerCase(), time: now };
          const match = enabled.find(({ option }) => option.label.toLocaleLowerCase().startsWith(search.current.text));
          if (!open) show(); if (match) setActive(match.index);
        }
      }}>{children}</select>
    {open && <div ref={menu} id={id} role="listbox" popover="manual" className="select-menu" aria-label={props["aria-label"] ?? t("Options")}>
      {items.map((item, index) => <div key={`${index}:${item.value}`} id={`${id}-${index}`} role="option" aria-selected={item.selected}
        aria-disabled={item.disabled || undefined} data-active={index === active} onPointerDown={event => event.preventDefault()}
        onPointerMove={() => { if (!item.disabled) setActive(index); }} onClick={event => {event.preventDefault(); choose(index);}}>
        <span>{item.label}</span><span aria-hidden="true">{item.selected ? "✓" : ""}</span>
      </div>)}
    </div>}
  </span>;
}

export function ColorInput({ value, onValueChange, disabled, "aria-label": label }: {
  value: string; onValueChange(value: string): void; disabled?: boolean; "aria-label"?: string;
}) {
  useLocale(); const [draft, setDraft] = useState(value), [open, setOpen] = useState(false);
  useEffect(() => setDraft(value), [value]);
  const valid = /^#[0-9a-f]{6}$/i.test(draft), colors = ["#ffffff", "#000000", "#ef4444", "#f97316", "#facc15", "#22c55e", "#14b8a6", "#3b82f6", "#8b5cf6", "#ec4899"];
  const update = (next: string) => { setDraft(next); if (/^#[0-9a-f]{6}$/i.test(next)) onValueChange(next); };
  return <span className="color-control">
    <input aria-label={label ?? t("Color code")} value={draft} disabled={disabled} spellCheck={false} maxLength={7} aria-invalid={!valid || undefined}
      onChange={event => update(event.target.value)} onBlur={() => { if (!valid) setDraft(value); }} />
    <Button aria-label={t("Choose color")} title={t("Choose color")} disabled={disabled} aria-expanded={open} onClick={() => setOpen(!open)}>
      <span className="color-swatch" style={{ background: value }} /><Palette size={13} />
    </Button>
    {open && <span className="color-palette" role="group" aria-label={t("Color palette")}>
      {colors.map(color => <Button key={color} aria-label={color} title={color} disabled={disabled} onClick={() => update(color)}><span className="color-swatch" style={{ background: color }} /></Button>)}
      {["Red", "Green", "Blue"].map((name, index) => <span className="color-channel" key={name}>{t(name)}<input aria-label={t(name)} type="range" min="0" max="255" step="1" disabled={disabled}
        value={parseInt(value.slice(1 + index * 2, 3 + index * 2), 16) || 0}
        onChange={event => update(value.slice(0, 1 + index * 2) + Number(event.target.value).toString(16).padStart(2, "0") + value.slice(3 + index * 2))} /></span>)}
      <Button onClick={() => setOpen(false)}>{t("Done")}</Button>
    </span>}
  </span>;
}

const clock = (seconds: number) => `${Math.floor(seconds / 60)}:${String(Math.floor(seconds % 60)).padStart(2, "0")}`;
export type MediaPlayerHandle = {
  position(): number;
  seek(seconds: number): void;
  play(): Promise<void>;
  pause(): void;
  toggle(): Promise<void>;
};
export function MediaPlayer({ src, audioOnly = false, controlRef, beforePlay }: {
  src: string; audioOnly?: boolean; controlRef?: Ref<MediaPlayerHandle>;
  beforePlay?(): boolean | void | Promise<boolean | void>;
}) {
  useLocale(); const ref = useRef<HTMLVideoElement>(null);
  const [playing, setPlaying] = useState(false), [position, setPosition] = useState(0), [duration, setDuration] = useState(0), [volume, setVolume] = useState(1), [error, setError] = useState(false);
  const epoch = useRef(0), starting = useRef(false);
  const pause = () => { epoch.current++; starting.current = false; ref.current?.pause(); setPlaying(false); };
  const seek = (seconds: number) => {
    const media = ref.current;
    if (!media || !Number.isFinite(seconds)) return;
    const end = Number.isFinite(media.duration) ? media.duration : duration;
    media.currentTime = Math.max(0, Math.min(end || seconds, seconds));
    setPosition(media.currentTime);
  };
  const play = async () => {
    const media = ref.current;
    if (!media || starting.current) return;
    const attempt = ++epoch.current;
    starting.current = true;
    try {
      if ((beforePlay && await beforePlay() === false) || attempt !== epoch.current || media !== ref.current || document.hidden) return;
      await media.play();
    } catch (error) {
      if (attempt === epoch.current && (error as Error)?.name !== "AbortError") setError(true);
    } finally { if (attempt === epoch.current) starting.current = false; }
  };
  const toggle = async () => { if (!ref.current) return; if (ref.current.paused) await play(); else pause(); };
  useImperativeHandle(controlRef, () => ({ position: () => ref.current?.currentTime ?? 0, seek, play, pause, toggle }));
  useEffect(() => {
    const element = ref.current;
    epoch.current++; starting.current = false;
    setPlaying(false); setPosition(0); setDuration(0); setError(false);
    const hidden = () => { if (document.hidden) pause(); };
    document.addEventListener("visibilitychange", hidden);
    return () => { epoch.current++; starting.current = false; element?.pause(); document.removeEventListener("visibilitychange", hidden); };
  }, [src]);
  return <div className="media-player">
    <video ref={ref} className={audioOnly ? "audio-source" : "source-preview"} src={src} preload="metadata" playsInline
      onPlay={() => setPlaying(true)} onPause={() => setPlaying(false)} onEnded={() => setPlaying(false)}
      onTimeUpdate={event => setPosition(event.currentTarget.currentTime)} onDurationChange={event => setDuration(Number.isFinite(event.currentTarget.duration) ? event.currentTarget.duration : 0)}
      onError={() => setError(true)} />
    {error ? <div><p role="alert">{t("This media could not be played. Check the file or try another format.")}</p><Button onClick={() => { setError(false); ref.current?.load(); }}>{t("Retry")}</Button></div> : <div className="media-player-controls">
      <Button aria-label={t(playing ? "Pause" : "Play")} title={t(playing ? "Pause" : "Play")} icon={playing ? <Pause size={17} /> : <Play size={17} />}
        onClick={() => void toggle()} />
      <span className="media-clock">{clock(position)} / {clock(duration)}</span>
      <input type="range" aria-label={t("Playback position")} min="0" max={duration || 1} step="0.01" value={position} disabled={!duration}
        onChange={event => seek(Number(event.target.value))} />
      <Button aria-label={t(volume ? "Mute" : "Unmute")} title={t(volume ? "Mute" : "Unmute")} icon={volume ? <Volume2 size={16} /> : <VolumeX size={16} />}
        onClick={() => { const next = volume ? 0 : 1; if (ref.current) ref.current.volume = next; setVolume(next); }} />
      <input className="media-volume" type="range" aria-label={t("Volume")} min="0" max="1" step="0.01" value={volume}
        onChange={event => { const next = Number(event.target.value); if (ref.current) ref.current.volume = next; setVolume(next); }} />
    </div>}
  </div>;
}
