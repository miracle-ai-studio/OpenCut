import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { ClipboardCopy, Download } from "lucide-react";
import { t, useLocale } from "../../shared/locale";
import { saveResponse } from "../../shared/download";
import { Button } from "../../shared/ui";
export function SnapshotActions({
  prepareCapture,
  name,
  disabled,
  report,
  menu,
  closeMenu,
}: {
  prepareCapture: () => () => Promise<Blob | null>;
  name: string;
  disabled: boolean;
  report: (error: unknown) => void;
  menu: { x: number; y: number } | null;
  closeMenu: () => void;
}) {
  useLocale();
  const [busy, setBusy] = useState(false),
    [notice, setNotice] = useState("");
  const mounted = useRef(true),
    menuRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!menu) return;
    menuRef.current?.querySelector<HTMLButtonElement>("button")?.focus();
    const dismiss = (event: PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) closeMenu();
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        closeMenu();
      }
    };
    document.addEventListener("pointerdown", dismiss);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("pointerdown", dismiss);
      document.removeEventListener("keydown", escape);
    };
  }, [menu, closeMenu]);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  async function perform(copy: boolean) {
    if (busy || disabled) return;
    closeMenu();
    setBusy(true);
    setNotice("");
    try {
      const capture = prepareCapture();
      const frame = async () => {
        const blob = await capture();
        if (!blob) throw new DOMException("Snapshot cancelled", "AbortError");
        if (blob.type !== "image/png")
          throw new Error("Snapshot is not a PNG image.");
        return blob;
      };
      if (copy) {
        if (!navigator.clipboard?.write || typeof ClipboardItem === "undefined")
          throw new Error(
            "Image clipboard is not available in this browser. Save the snapshot instead.",
          );
        // Start the clipboard operation during the click gesture; the native frame can finish later.
        await navigator.clipboard.write([
          new ClipboardItem({ "image/png": frame() }),
        ]);
        if (mounted.current) setNotice(t("Snapshot copied"));
      } else {
        await saveResponse(
          `${name.replace(/[\\/:*?"<>|\u0000-\u001f]/g, "_").slice(0, 100) || "OpenCut"}-snapshot.png`,
          async () =>
            new Response(await frame(), {
              headers: { "content-type": "image/png" },
            }),
        );
        if (mounted.current) setNotice(t("Snapshot saved"));
      }
    } catch (error) {
      if (
        !(error instanceof DOMException && error.name === "AbortError") &&
        mounted.current
      )
        report(error);
    } finally {
      if (mounted.current) setBusy(false);
    }
  }
  return (
    <>
      <Button
        aria-label={t("Save snapshot")}
        disabled={disabled || busy}
        icon={<Download size={15} />}
        onClick={() => void perform(false)}
      />
      <Button
        aria-label={t("Copy snapshot")}
        disabled={disabled || busy}
        icon={<ClipboardCopy size={15} />}
        onClick={() => void perform(true)}
      />
      {menu &&
        createPortal(
          <div
            ref={menuRef}
            className="preview-context-menu"
            role="menu"
            aria-label={t("Preview actions")}
            style={{
              left: Math.max(8, Math.min(menu.x, window.innerWidth - 228)),
              top: Math.max(8, Math.min(menu.y, window.innerHeight - 90)),
            }}
            onKeyDown={(event) => {
              if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key))
                return;
              event.preventDefault();
              const buttons = Array.from(
                  menuRef.current?.querySelectorAll<HTMLButtonElement>(
                    "button:not(:disabled)",
                  ) ?? [],
                ),
                current = buttons.indexOf(
                  document.activeElement as HTMLButtonElement,
                );
              const index =
                event.key === "Home"
                  ? 0
                  : event.key === "End"
                    ? buttons.length - 1
                    : (current +
                        (event.key === "ArrowDown" ? 1 : -1) +
                        buttons.length) %
                      buttons.length;
              buttons[index]?.focus();
            }}
          >
            <button
              role="menuitem"
              disabled={disabled || busy}
              onClick={() => void perform(false)}
            >
              <Download size={15} />
              {t("Save snapshot")}
            </button>
            <button
              role="menuitem"
              disabled={disabled || busy}
              onClick={() => void perform(true)}
            >
              <ClipboardCopy size={15} />
              {t("Copy snapshot")}
            </button>
          </div>,
          document.fullscreenElement ?? document.body,
        )}
      {notice && (
        <span className="snapshot-notice" role="status">
          {notice}
        </span>
      )}
    </>
  );
}
