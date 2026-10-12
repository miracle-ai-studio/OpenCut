import { SourceAudioControls } from "./source-audio-controls";
import type { ReactNode } from "react";
import { formatTime } from "../../model/time";
import { t } from "../../shared/locale";
import type { V4Document, V4Element, V4Sequence } from "./types";
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
    <dl className="v4-properties">
      {rows.map(([name, value]) => (
        <div key={name}>
          <dt>{t(name)}</dt>
          <dd>{value}</dd>
        </div>
      ))}
    </dl>
  );
}
export function V4Inspector({
  project,
  sequence,
  selectedId,
}: {
  project: V4Document;
  sequence: V4Sequence;
  selectedId: string | null;
}) {
  const element = sequence.tracks
    .flatMap((t) => t.elements)
    .find((e) => e.id === selectedId);
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
  const gain = (value: V4Sequence["tracks"][number]["gain"]) =>
    `${value.value} ${value.unit === "decibels" ? "dB" : t("linear")}`;
  const font = (binding: NonNullable<typeof caption>["style"]["font"]) =>
    binding.kind === "locked"
      ? (project.fonts.find((f) => f.id === binding.font_id)?.family ??
        t("Unavailable font"))
      : binding.family;
  const elementRows = (e: V4Element): [string, ReactNode][] => [
    ["Type", label(e.content.kind)],
    ["Start", formatTime(e.timeline_range.start)],
    ["Duration", formatTime(e.timeline_range.duration)],
    ["Enabled", yes(e.enabled)],
    ["Animation channels", e.channels.length],
  ];
  return (
    <>
      <SourceAudioControls />
      {element && (
        <>
          <h3>{element.name}</h3>
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
                  `${content.retime.rate.numerator}/${content.retime.rate.denominator}`,
                ],
                ["Maintain pitch", yes(content.retime.maintain_pitch)],
              ]}
            />
          )}
          {content?.kind === "text" && (
            <>
              <p className="v4-note-text">{content.text.content}</p>
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
            <Properties
              rows={[
                ["Shape", label(content.graphic.geometry.kind)],
                ["Stroke width", content.graphic.stroke.width_px],
              ]}
            />
          )}
          {content?.kind === "effect_layer" && (
            <Properties
              rows={[
                ["Effect", label(content.effect.effect.kind)],
                ["Enabled", yes(content.effect.enabled)],
              ]}
            />
          )}
          {element.visual && (
            <Properties
              rows={[
                ["Position X", element.visual.transform.position_x_px],
                ["Position Y", element.visual.transform.position_y_px],
                ["Rotation", element.visual.transform.rotation_degrees],
                ["Scale X", element.visual.transform.scale_x],
                ["Scale Y", element.visual.transform.scale_y],
                ["Opacity", element.visual.opacity],
                ["Blend mode", label(element.visual.blend_mode)],
                ["Effects", element.visual.effects.length],
                ["Masks", element.visual.masks.length],
              ]}
            />
          )}
          <p className="muted">
            {t(
              "Other element controls and animation editing are not available yet. All saved properties are preserved.",
            )}
          </p>
        </>
      )}
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
          <p className="v4-note-text">{caption.text}</p>
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
