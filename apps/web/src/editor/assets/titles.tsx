import { t, useLocale } from "../shared/locale";
import { useState } from "react";
import { stickers, stickerFile } from "./stickers";
import { uploadMedia } from "./upload";
import { placeAsset } from "../model/edit-requests";
import { Type, Circle, Square } from "lucide-react";
import { useWorkspace } from "../workspace/context";
import { activeSequence } from "../model/types";
import { fromSeconds } from "../model/time";
import { Button, Field, Panel } from "../shared/ui";
export function TitlesPanel({ mode = "text" }: { mode?: "text" | "stickers" }) {
  useLocale();
  const { controller, api, snapshot, historical, sequenceId, playhead, busy } =
      useWorkspace(),
    sequence = activeSequence(snapshot, sequenceId);
  const [text, setText] = useState(t("Your story starts here")),
    [font, setFont] = useState("Arial"),
    [size, setSize] = useState(48),
    [duration, setDuration] = useState(3);
  const [stickerColor, setStickerColor] = useState("#ffffff"),
    [importing, setImporting] = useState(false);
  const addSticker = async (name: keyof typeof stickers) => {
    if (!api || !snapshot || !sequence || historical) return;
    setImporting(true);
    try {
      const file = await stickerFile(name, stickerColor);
      const result = await uploadMedia(
        api.sdk,
        snapshot.project.id,
        file,
        () => {},
        new AbortController().signal,
      );
      if (
        controller.getSnapshot().api !== api ||
        controller.getSnapshot().snapshot?.project.id !== snapshot.project.id
      )
        return;
      const operations = placeAsset(result.asset, sequence, playhead);
      const insert = operations.find(
        (operation) => operation.type === "clip.insert",
      );
      if (insert?.type === "clip.insert") {
        insert.clip.params = { scale_x: 0.3, scale_y: 0.3 };
        insert.clip.timeline_range.duration = fromSeconds(duration);
      }
      await controller.mutate("Add sticker", [
        { type: "asset.bind", asset: result.asset },
        ...operations,
      ]);
    } catch (error) {
      controller.report(error);
    } finally {
      setImporting(false);
    }
  };
  const addText = () => {
    if (sequence)
      void controller
        .mutate("Add caption", [
          {
            type: "caption.insert",
            sequence_id: sequence.id,
            cue: {
              id: crypto.randomUUID(),
              timeline_range: {
                start: playhead,
                duration: fromSeconds(duration),
              },
              text,
              source_anchor: null,
              style: {
                font_family: font,
                font_size: size,
                color: "#ffffff",
                background_color: null,
                text_align: "center",
              },
            },
          },
        ])
        .catch((error) => controller.report(error));
  };
  const shape = (kind: "rectangle" | "ellipse") => {
    if (!sequence || !snapshot) return;
    const id = crypto.randomUUID(),
      track = sequence.tracks.find(
        (track) => track.kind === "graphic" && !track.locked,
      ),
      trackId = track?.id ?? crypto.randomUUID();
    void controller
      .mutate(
        "Add shape",
        track
          ? []
          : [
              {
                type: "track.create",
                sequence_id: sequence.id,
                track: {
                  id: trackId,
                  name: "Graphics",
                  kind: "graphic",
                  enabled: true,
                  muted: false,
                  locked: false,
                  clips: [],
                  params: {},
                },
              },
            ],
        [
          {
            type: "shape.put",
            sequence_id: sequence.id,
            shape: {
              id,
              track_id: trackId,
              timeline_range: {
                start: playhead,
                duration: fromSeconds(duration),
              },
              primitive:
                kind === "rectangle"
                  ? { kind, width_px: 400, height_px: 240 }
                  : { kind, radius_x_px: 160, radius_y_px: 160 },
              fill: {
                red: parseInt(stickerColor.slice(1, 3), 16),
                green: parseInt(stickerColor.slice(3, 5), 16),
                blue: parseInt(stickerColor.slice(5, 7), 16),
                alpha: 255,
              },
              position_x: 0,
              position_y: 0,
              rotation_degrees: 0,
              opacity: 1,
            },
          },
        ],
      )
      .then(() => controller.select({ kind: "shape", id }))
      .catch((error) => controller.report(error));
  };
  return (
    <Panel title={t(mode === "text" ? "Text" : "Stickers")}>
      <div className="scroll padded">
        <fieldset disabled={busy || importing || !!historical || !sequence}>
          {mode === "text" && (
            <>
              <legend>{t("Text")}</legend>
              <Field label={t("Caption text")}>
                <textarea
                  value={text}
                  onChange={(event) => setText(event.target.value)}
                />
              </Field>
              <Field label={t("Font family")}>
                <input
                  value={font}
                  onChange={(event) => setFont(event.target.value)}
                />
              </Field>
              <div className="field-pair">
                <Field label={t("Font size")}>
                  <input
                    type="number"
                    min="1"
                    value={size}
                    onChange={(event) => setSize(Number(event.target.value))}
                  />
                </Field>
                <Field label={t("Duration")}>
                  <input
                    type="number"
                    min=".1"
                    step=".1"
                    value={duration}
                    onChange={(event) =>
                      setDuration(Number(event.target.value))
                    }
                  />
                </Field>
              </div>
              <Button icon={<Type size={16} />} onClick={addText}>
                {t("Add text at playhead")}
              </Button>
            </>
          )}
          {mode === "stickers" && (
            <>
              <legend>{t("Shapes")}</legend>
              <Field label={t("Fill")}>
                <input
                  type="color"
                  value={stickerColor}
                  onChange={(event) => setStickerColor(event.target.value)}
                />
              </Field>
              <div className="shape-presets">
                <Button
                  disabled={snapshot?.format !== "visual"}
                  icon={<Square size={28} />}
                  onClick={() => shape("rectangle")}
                >
                  {t("Rectangle")}
                </Button>
                <Button
                  disabled={snapshot?.format !== "visual"}
                  icon={<Circle size={28} />}
                  onClick={() => shape("ellipse")}
                >
                  {t("Ellipse")}
                </Button>
                {(Object.keys(stickers) as (keyof typeof stickers)[]).map(
                  (name) => (
                    <Button key={name} onClick={() => void addSticker(name)}>
                      <span
                        className="sticker-icon"
                        style={{ color: stickerColor }}
                        dangerouslySetInnerHTML={{ __html: stickers[name] }}
                      />
                      {t(name)}
                    </Button>
                  ),
                )}
              </div>
            </>
          )}
        </fieldset>
      </div>
    </Panel>
  );
}
