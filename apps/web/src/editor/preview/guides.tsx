import { useEntrance } from "../shared/motion";
import { useEffect, useRef, useState, useId } from "react";
import { createPortal } from "react-dom";
import { Grid2x2, Plus, X } from "lucide-react";
import type { PreviewGuides } from "../shared/view-preferences";
import { Button } from "../shared/ui";
import { t, useLocale } from "../shared/locale";
import tiktokLayout from "./tiktok-layout.svg";
export function GuideControls({
  value,
  onChange,
}: {
  value: PreviewGuides;
  onChange: (value: PreviewGuides) => void;
}) {
  useLocale();
  const [position, setPosition] = useState<{
    left: number;
    bottom: number;
  } | null>(null);
  const anchor = useRef<HTMLButtonElement | null>(null);
  const menu = useEntrance<HTMLDivElement>(!!position);
  const id = useId();
  useEffect(() => {
    if (!position) return;
    menu.current?.querySelector("select")?.focus();
    const outside = (event: PointerEvent) => {
      if (
        !menu.current?.contains(event.target as Node) &&
        !anchor.current?.contains(event.target as Node)
      )
        setPosition(null);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        setPosition(null);
        anchor.current?.focus();
      }
    };
    const resize = () => setPosition(null);
    document.addEventListener("pointerdown", outside);
    document.addEventListener("keydown", escape);
    window.addEventListener("resize", resize);
    return () => {
      document.removeEventListener("pointerdown", outside);
      document.removeEventListener("keydown", escape);
      window.removeEventListener("resize", resize);
    };
  }, [position]);
  const patch = (next: Partial<PreviewGuides>) =>
    onChange({ ...value, ...next });
  return (
    <div className="preview-guide-menu">
      <Button
        aria-label={t("Preview guides")}
        aria-haspopup="dialog"
        aria-expanded={!!position}
        aria-controls={position ? id : undefined}
        icon={<Grid2x2 size={13} />}
        onClick={(event) => {
          anchor.current = event.currentTarget;
          if (position) {
            setPosition(null);
            return;
          }
          const rect = event.currentTarget.getBoundingClientRect();
          setPosition({
            left: Math.max(8, Math.min(innerWidth - 278, rect.left)),
            bottom: innerHeight - rect.top + 6,
          });
        }}
      >
        {t("Guides")}
      </Button>
      {position &&
        createPortal(
          <div
            ref={menu}
            id={id}
            role="dialog"
            aria-label={t("Preview guides")}
            className="preview-guide-options"
            style={{
              left: position.left,
              bottom: position.bottom,
              maxHeight: Math.max(
                80,
                Math.min(350, innerHeight - position.bottom - 12),
              ),
            }}
          >
            <label>
              {t("Guide type")}
              <select
                aria-label={t("Guide type")}
                value={value.kind}
                onChange={(event) =>
                  patch({ kind: event.target.value as PreviewGuides["kind"] })
                }
              >
                <option value="none">{t("None")}</option>
                <option value="grid">{t("Grid")}</option>
                <option value="tiktok">TikTok</option>
                <option value="custom">{t("Custom")}</option>
              </select>
            </label>
            {value.kind === "grid" && (
              <div className="guide-grid-fields">
                {(["rows", "cols"] as const).map((key) => (
                  <label key={key}>
                    {t(key === "rows" ? "Rows" : "Columns")}
                    <input
                      type="number"
                      min={1}
                      max={24}
                      value={value[key]}
                      onChange={(event) => {
                        const n = Number(event.target.value);
                        if (Number.isFinite(n))
                          patch({
                            [key]: Math.max(1, Math.min(24, Math.round(n))),
                          });
                      }}
                    />
                  </label>
                ))}
              </div>
            )}
            {value.kind === "custom" && (
              <>
                {value.lines.map((line, index) => (
                  <div className="guide-line-fields" key={index}>
                    <span>
                      {t(line.axis === "x" ? "Vertical" : "Horizontal")}
                    </span>
                    <input
                      aria-label={t("Guide position {number}", {
                        number: index + 1,
                      })}
                      type="number"
                      min={0}
                      max={100}
                      value={line.position}
                      onChange={(event) => {
                        const n = Number(event.target.value);
                        if (Number.isFinite(n))
                          patch({
                            lines: value.lines.map((prior, i) =>
                              i === index
                                ? {
                                    ...prior,
                                    position: Math.max(0, Math.min(100, n)),
                                  }
                                : prior,
                            ),
                          });
                      }}
                    />
                    <span>%</span>
                    <Button
                      aria-label={t("Remove guide {number}", {
                        number: index + 1,
                      })}
                      icon={<X size={12} />}
                      onClick={() =>
                        patch({
                          lines: value.lines.filter((_, i) => i !== index),
                        })
                      }
                    />
                  </div>
                ))}
                <div className="toolbar">
                  {(["x", "y"] as const).map((axis) => (
                    <Button
                      key={axis}
                      disabled={value.lines.length >= 32}
                      icon={<Plus size={12} />}
                      onClick={() =>
                        patch({
                          lines: [...value.lines, { axis, position: 50 }],
                        })
                      }
                    >
                      {t(
                        axis === "x"
                          ? "Add vertical guide"
                          : "Add horizontal guide",
                      )}
                    </Button>
                  ))}
                </div>
              </>
            )}
            <p className="muted">
              {t(
                "Guides help you compose. They are not included in snapshots or exports.",
              )}
            </p>
          </div>,
          document.body,
        )}
    </div>
  );
}
export function GuideOverlay({ value }: { value: PreviewGuides }) {
  useLocale();
  if (value.kind === "none") return null;
  const lines =
    value.kind === "grid"
      ? [
          ...Array.from({ length: value.cols - 1 }, (_, index) => ({
            axis: "x" as const,
            position: ((index + 1) / value.cols) * 100,
          })),
          ...Array.from({ length: value.rows - 1 }, (_, index) => ({
            axis: "y" as const,
            position: ((index + 1) / value.rows) * 100,
          })),
        ]
      : value.lines;
  return (
    <div className="preview-guides" aria-label={t("Composition guides")}>
      {value.kind === "tiktok" ? (
        <img src={tiktokLayout} alt={t("TikTok layout")} />
      ) : (
        lines.map((line, index) => (
          <div
            key={index}
            className={`preview-guide-line guide-${line.axis}`}
            style={
              line.axis === "x"
                ? { left: `${line.position}%` }
                : { top: `${line.position}%` }
            }
          />
        ))
      )}
    </div>
  );
}
