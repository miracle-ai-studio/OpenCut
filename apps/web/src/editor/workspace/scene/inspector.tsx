import { SceneStackInspector } from "./scene-stack";
import { GainKeyframeLine } from "./gain-keyframes";
import { VolumeControls } from "./property-edit/volume-controls";
import { VisualPropertyControls } from "./property-edit/visual-controls";
import { GainFadeControls } from "./gain-keyframes/fade-controls";
import { ElementPropertiesEditor } from "./property-edit/editor";
import { KeyframeEditor } from "./keyframes/editor";
import { MaskInspector } from "./mask/workspace";
import { SourceAudioControls } from "./source-audio-controls";
import { useState, type ReactNode } from "react";
import { useEditingWorkspace as useWorkspace } from "../context";
import { Button } from "../../shared/ui";
import { formatTime } from "../../model/time";
import { t } from "../../shared/locale";
import type { Document, Element, Sequence } from "./types";
import { GraphicEditButton } from "./graphic/workspace";
import { TextEditButton } from "./text/workspace";
import { TextTransformEditor } from "./text/transform-editor";
import { BlendEditor } from "./blend-editor";
import { CaptionTextEditor } from "./captions/edit-text";

function TextTransformControl({
  trackId,
  elementId,
  locked,
}: {
  trackId: string;
  elementId: string;
  locked: boolean;
}) {
  const { scene, busy } = useWorkspace();
  const [editing, setEditing] = useState(false);
  return editing ? (
    <TextTransformEditor
      trackId={trackId}
      elementId={elementId}
      onClose={() => setEditing(false)}
    />
  ) : (
    <Button
      disabled={busy || locked || !!scene?.historical}
      onClick={() => setEditing(true)}
    >
      {t("Edit text position and scale")}
    </Button>
  );
}
const label = (kind: string) =>
  t(
    (
      {
        video: "Video",
        audio: "Audio",
        image: "Image",
        text: "Text",
        sticker: "Sticker",
        graphic: "Graphic",
        effect_layer: "Effect layer",
        caption: "Caption",
        effect: "Effect",
        unknown: "Unknown",
        absent: "No audio",
        present: "Has audio",
        normal: "Normal",
        bold: "Bold",
        italic: "Italic",
        classic_trimmed: "Trimmed",
        legacy_crossfade_v3: "Crossfade",
      } as Record<string, string>
    )[kind] ?? kind.replaceAll("_", " "),
  );
const yes = (value: boolean) => t(value ? "Yes" : "No");
function Properties({ rows }: { rows: [string, ReactNode][] }) {
  return (
    <dl className="editor-properties">
      {rows.map(([name, value]) => (
        <div key={name}>
          <dt>{t(name)}</dt>
          <dd>{value}</dd>
        </div>
      ))}
    </dl>
  );
}
export function Inspector({
  project,
  sequence,
  selectedId,
}: {
  project: Document;
  sequence: Sequence;
  selectedId: string | null;
}) {
  const { api, scene } = useWorkspace();
  const element = sequence.tracks
    .flatMap((t) => t.elements)
    .find((e) => e.id === selectedId);
  const elementTrack = sequence.tracks.find((track) =>
    track.elements.some((item) => item.id === selectedId),
  );
  const track = sequence.tracks.find((t) => t.id === selectedId);
  const caption = sequence.captions.find((c) => c.id === selectedId);
  const settings =
    project.settings.profile === "classic_global"
      ? project.settings.settings
      : project.settings.formats.find((f) => f.sequence_id === sequence.id)!;
  const assetId =
    element && "asset_id" in element.content ? element.content.asset_id : null;
  const resource = project.assets.find((a) => a.id === assetId);
  const content = element?.content;
  const audio =
    content?.kind === "video" || content?.kind === "audio"
      ? content.audio
      : null;
  const gain = (value: Sequence["tracks"][number]["gain"]) =>
    `${value.value} ${value.unit === "decibels" ? "dB" : t("linear")}`;
  const font = (binding: NonNullable<typeof caption>["style"]["font"]) =>
    binding.kind === "locked"
      ? (project.fonts.find((f) => f.id === binding.font_id)?.family ??
        t("Unavailable font"))
      : binding.family;
  const elementRows = (e: Element): [string, ReactNode][] => [
    ["Type", label(e.content.kind)],
    ["Start", formatTime(e.timeline_range.start)],
    ["Duration", formatTime(e.timeline_range.duration)],
    ["Enabled", yes(e.enabled)],
    ["Animation channels", e.channels.length],
  ];
  const selected = scene?.timelineEdit.selection ?? [];
  if (selected.length > 1) return <>
    <h3>{t("Selected clips")} · {selected.length}</h3>
    <VisualPropertyControls elementIds={selected.map(target => target.element_id)} />
    <VolumeControls elementIds={selected.map(target => target.element_id)} />
  </>;
  return (
    <>
      {element && (
        <>
          <h3>{element.name}</h3>
          <p className="workbench-clip-meta">{label(element.content.kind)} · {formatTime(element.timeline_range.duration)}</p>
          <VisualPropertyControls elementIds={[element.id]} />
          {audio && <><VolumeControls elementIds={[element.id]} /><GainFadeControls elementId={element.id} /></>}
          {element.visual ? <details className="workbench-advanced-properties"><summary>{t("Advanced clip properties")}</summary>
            <ElementPropertiesEditor elementId={element.id} locked={elementTrack?.locked ?? false} showAudio={false} />
          </details> : <ElementPropertiesEditor elementId={element.id} locked={elementTrack?.locked ?? false} showAudio={false} />}
          <SceneStackInspector elementId={element.id} locked={elementTrack?.locked ?? false} />
          <details className="workbench-animation-section"><summary>{t("Animation")}</summary><KeyframeEditor elementId={element.id} locked={elementTrack?.locked ?? false} /></details>
          <GainKeyframeLine elementId={element.id} />
          <details className="workbench-source-section"><summary>{t("Clip information")}</summary>
          <Properties rows={elementRows(element)} />
          {resource && (
            <Properties
              rows={[
                ["Media", resource.name],
                ["Type", label(resource.kind)],
                [
                  "Source duration",
                  resource.duration ? formatTime(resource.duration) : "—",
                ],
                ["Audio", label(resource.metadata.audio.status)],
                [
                  "Canvas",
                  resource.metadata.intrinsic_size
                    ? `${resource.metadata.intrinsic_size.width} × ${resource.metadata.intrinsic_size.height}`
                    : "—",
                ],
              ]}
            />
          )}
          {element.source_span.profile === "classic_trimmed" ? (
            <Properties
              rows={[
                [
                  "Source trim start",
                  formatTime(element.source_span.trim_start),
                ],
                ["Source trim end", formatTime(element.source_span.trim_end)],
              ]}
            />
          ) : (
            <Properties
              rows={[
                ["Source start", formatTime(element.source_span.range.start)],
                [
                  "Source duration",
                  formatTime(element.source_span.range.duration),
                ],
              ]}
            />
          )}
          {audio && (
            <Properties
              rows={[
                ["Volume", gain(audio.gain)],
                ["Muted", yes(audio.muted)],
                ...(content?.kind === "video"
                  ? [
                      [
                        "Source audio enabled",
                        yes(content.source_audio_enabled),
                      ] as [string, ReactNode],
                    ]
                  : []),
              ]}
            />
          )}
          {(content?.kind === "video" || content?.kind === "audio") && (
            <Properties
              rows={[
                [
                  "Playback rate",
                  `${Number((content.retime.rate.numerator / content.retime.rate.denominator).toFixed(4))}×`,
                ],
                ["Maintain pitch", yes(content.retime.maintain_pitch)],
              ]}
            />
          )}
          </details>
          {content?.kind === "text" && (
            <>
              <TextEditButton
                elementId={element.id}
                locked={elementTrack?.locked ?? true}
              />
              {element.visual && elementTrack && (
                <TextTransformControl
                  key={`${project.id}/${sequence.id}/${element.id}`}
                  trackId={elementTrack.id}
                  elementId={element.id}
                  locked={elementTrack.locked}
                />
              )}
              <p className="editor-note-text">{content.text.content}</p>
              <Properties
                rows={[
                  ["Font", font(content.text.font)],
                  ["Font size", content.text.font_size.value],
                  ["Font style", label(content.text.font_style)],
                  ["Font weight", label(content.text.font_weight)],
                  ["Line height", content.text.line_height_multiplier],
                ]}
              />
            </>
          )}
          {content?.kind === "graphic" && (
            <>
              {element.visual &&
                !["legacy_rectangle", "legacy_ellipse"].includes(
                  content.graphic.geometry.kind,
                ) && (
                  <GraphicEditButton
                    elementId={element.id}
                    locked={elementTrack?.locked ?? true}
                  />
                )}
              <Properties
                rows={[
                  ["Shape", label(content.graphic.geometry.kind)],
                  ["Stroke width", content.graphic.stroke.width_px],
                ]}
              />
            </>
          )}
          {content?.kind === "effect_layer" && (
            <Properties
              rows={[
                ["Effect", label(content.effect.effect.kind)],
                ["Enabled", yes(content.effect.enabled)],
              ]}
            />
          )}
          <MaskInspector elementId={element.id} locked={elementTrack?.locked} />
          {element.visual && (
            <>
              {elementTrack && (
                <BlendEditor
                  key={`${api?.connectionIdentity}/${project.id}/${scene?.head.revision}/${scene?.historical?.revision}/${sequence.id}/${element.id}`}
                  trackId={elementTrack.id}
                  elementId={element.id}
                />
              )}
              <Properties
                rows={[
                  ["Position X", element.visual.transform.position_x_px],
                  ["Position Y", element.visual.transform.position_y_px],
                  ["Rotation", element.visual.transform.rotation_degrees],
                  ["Scale X", element.visual.transform.scale_x],
                  ["Scale Y", element.visual.transform.scale_y],
                  ["Opacity", element.visual.opacity],
                  ["Effects", element.visual.effects.length],
                  ["Masks", element.visual.masks.length],
                ]}
              />
            </>
          )}
        </>
      )}
      <SourceAudioControls />
      {track && (
        <>
          <h3>{track.name}</h3>
          <Properties
            rows={[
              ["Type", label(track.kind)],
              ["Enabled", yes(track.enabled)],
              ["Locked", yes(track.locked)],
              ["Muted", yes(track.muted)],
              ["Volume", gain(track.gain)],
              ["Opacity", track.opacity],
              ["Elements", track.elements.length],
              ["Animation channels", track.channels.length],
            ]}
          />
        </>
      )}
      {caption && (
        <>
          <h3>{t("Caption")}</h3>
          <p className="editor-note-text">{caption.text}</p>
          <CaptionTextEditor key={`${project.id}/${sequence.id}/${caption.id}`} captionId={caption.id} />
          <Properties
            rows={[
              ["Start", formatTime(caption.timeline_range.start)],
              ["Duration", formatTime(caption.timeline_range.duration)],
              ["Font", font(caption.style.font)],
              ["Font size", caption.style.font_size.value],
              [
                "Track",
                sequence.tracks.find((t) => t.id === caption.track_id)?.name ??
                  "—",
              ],
            ]}
          />
        </>
      )}
      <details>
        <summary>{t("Project settings and resources")}</summary>
        <Properties
          rows={[
            [
              "Canvas",
              `${settings.canvas_size.width} × ${settings.canvas_size.height}`,
            ],
            [
              "Frame rate",
              `${settings.fps.numerator}/${settings.fps.denominator}`,
            ],
            ["Background", label(settings.background.kind)],
            ["Media", project.assets.length],
            ["Fonts", project.fonts.length],
          ]}
        />
        <ul>
          {project.assets.map((a) => (
            <li key={a.id}>
              {a.name} · {label(a.kind)} ·{" "}
              {a.duration ? formatTime(a.duration) : "—"}
              {a.license?.attribution && <p>{a.license.attribution}</p>}
            </li>
          ))}
        </ul>
        <ul>
          {project.fonts.map((f) => (
            <li key={f.id}>
              {f.family} · {f.weight} · {label(f.style)}
              {f.license?.attribution && <p>{f.license.attribution}</p>}
            </li>
          ))}
        </ul>
      </details>
      <details>
        <summary>{t("Scene transitions")}</summary>
        <ul>
          {sequence.transitions.map((tr) => (
            <li key={tr.id}>
              {label(tr.transition)} · {formatTime(tr.duration)} ·{" "}
              {sequence.tracks
                .flatMap((t) => t.elements)
                .find((e) => e.id === tr.outgoing_element_id)?.name ?? "—"}{" "}
              →{" "}
              {sequence.tracks
                .flatMap((t) => t.elements)
                .find((e) => e.id === tr.incoming_element_id)?.name ?? "—"}
            </li>
          ))}
        </ul>
        {!sequence.transitions.length && <p>{t("No transitions")}</p>}
      </details>
    </>
  );
}
