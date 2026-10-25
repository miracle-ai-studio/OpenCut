import { describe, expect, it, vi } from "vitest";
import { initialSnapshot } from "./fixture";
import { actorLabel, changeSummary, historyChanges, readHistoryPair } from "./history-changes";
import type { Document } from "./types";

describe("readable fixed-version history differences", () => {
  it("matches a moved clip by identity and names its actual before/after properties", () => {
    const before = initialSnapshot().project, after = structuredClone(before);
    const source = after.sequences[0].tracks[0], clip = source.elements[0];
    source.elements = [];
    after.sequences[0].tracks.push({ ...structuredClone(source), id: "target-track-id", name: "Titles", elements: [clip] });
    clip.name = "Sunset";
    clip.timeline_range.start = { ticks: "5", timebase: 2 };
    clip.timeline_range.duration = { ticks: "10", timebase: 1 };
    if (clip.source_span.profile === "classic_trimmed") clip.source_span.trim_start = { ticks: "2", timebase: 1 };
    const changes = historyChanges(before, after, "en"), moved = changes.filter(row => row.object === "Clip “Sunset”");
    expect(moved).toContainEqual(expect.objectContaining({ field: "Track / scene", before: "Main Scene · Main Track", after: "Main Scene · Titles" }));
    expect(moved).toContainEqual(expect.objectContaining({ field: "Name", before: "Whole image", after: "Sunset" }));
    expect(moved).toContainEqual(expect.objectContaining({ field: "Timeline · Start time", before: "0 s", after: "2.5 s" }));
    expect(moved).toContainEqual(expect.objectContaining({ field: "Source trim · Trim from start", before: "0 s", after: "2 s" }));
    expect(moved.every(row => row.kind === "changed")).toBe(true);
    expect(JSON.stringify(changes)).not.toContain("target-track-id");
  });

  it("displays canvas, volume, speed and order without duplicate clip order rows", () => {
    const before = initialSnapshot().project;
    const track = before.sequences[0].tracks[0];
    track.elements.push({ ...structuredClone(track.elements[0]), id: "other", name: "Other" });
    track.elements[0].content = { kind: "video", asset_id: "asset", video_stream: 0, source_audio_enabled: true, audio: { stream: 0, gain: { unit: "decibels", value: 0 }, muted: false }, retime: { rate: { numerator: 1, denominator: 1 }, maintain_pitch: true, mapping: {kind:"exact_rational"} } };
    const after = structuredClone(before), target = after.sequences[0].tracks[0];
    target.gain.value = -6;
    const content = target.elements[0].content;
    if (content.kind === "video") {
      content.retime.rate = { numerator: 2, denominator: 1 };
      content.audio.gain.value = -12;
    }
    target.elements[0].visual!.transform.position_x_px = 24;
    target.elements.reverse();
    if (after.settings.profile === "classic_global") after.settings.settings.canvas_size = { width: 1920, height: 1080 };
    const changes = historyChanges(before, after, "en");
    expect(changes).toContainEqual(expect.objectContaining({ field: "Volume", before: "0 dB", after: "-6 dB" }));
    expect(changes).toContainEqual(expect.objectContaining({ field: "Playback · Speed", before: "1×", after: "2×" }));
    expect(changes).toContainEqual(expect.objectContaining({ object: "Clip “Whole image”", field: "Volume", before: "0 dB", after: "-12 dB" }));
    expect(changes).toContainEqual(expect.objectContaining({ field: "Horizontal position", before: "17 px", after: "24 px" }));
    expect(changes).toContainEqual(expect.objectContaining({ field: "Size", before: "640 × 360 px", after: "1920 × 1080 px" }));
    expect(changes.filter(row => row.object === "Clip “Whole image”" && row.field === "Order")).toHaveLength(1);
  });

  it("shows added/removed subtitle text, font/style, masks, effects, media replacement and animation values", () => {
    const before = initialSnapshot().project, white = { encoding: "srgb" as const, red: 1, green: 1, blue: 1, alpha: 1 };
    before.sequences[0].captions.push({ id: "caption-id", track_id: "track", text: "Before words", timeline_range: { start: { ticks: "0", timebase: 1 }, duration: { ticks: "1", timebase: 1 } }, source_anchor: null, style: { font: { kind: "unresolved_family", family: "Arial" }, font_size: { unit: "canvas_pixels", value: 22 }, color: white, background_color: null, max_width_px: null, line_height_px: null, text_align: "center", layout: "pinned_caption_v4" } } as Document["sequences"][number]["captions"][number]);
    const after = structuredClone(before), caption = after.sequences[0].captions[0];
    caption.text = "After words"; caption.style.font_size = { unit: "canvas_pixels", value: 30 };
    caption.style.font = { kind: "unresolved_family", family: "Noto Sans" } as typeof caption.style.font;
    const visual = after.sequences[0].tracks[0].elements[0].visual!;
    visual.effects.push({ id: "effect-id", enabled: true, effect: { kind: "classic_blur", intensity: 8 } });
    visual.masks.push({ id: "mask-id", enabled: true, inverted: false, feather: { unit: "classic_pixels", value: 4 }, stroke: { color: white, width_px: 1, align: "center" }, shape: { kind: "rectangle", geometry: { center_x: 0.5, center_y: 0.5, width: 0.6, height: 0.8, rotation_degrees: 0, scale: 1 } } });
    after.assets[0].version = { ...after.assets[0].version, id: "secret-version-id", content_hash: "b".repeat(64) };
    after.sequences[0].tracks[0].elements[0].channels.push({ kind: "scalar", id: "channel-id", target: { kind: "element", element_id: "image" }, binding: { kind: "property", parameter: "opacity" }, keys: [{ id: "key-id", time: { ticks: "0", timebase: 1 }, value: 0.5, left_handle: null, right_handle: null, segment_to_next: { kind: "linear" }, tangent_mode: "auto" }], extrapolation: { before: "hold", after: "hold" } });
    const changes = historyChanges(before, after, "en"), output = JSON.stringify(changes);
    expect(changes).toContainEqual(expect.objectContaining({ field: "Text", before: "Before words", after: "After words" }));
    expect(changes).toContainEqual(expect.objectContaining({ field: "Style · Font size", before: "22 px", after: "30 px" }));
    expect(changes).toContainEqual(expect.objectContaining({ field: "Style · Font", before: "Arial", after: "Noto Sans" }));
    expect(changes).toContainEqual(expect.objectContaining({ field: "Effect · Blur amount", after: "8" }));
    expect(changes).toContainEqual(expect.objectContaining({ field: "Feather", after: "4 px" }));
    expect(changes).toContainEqual(expect.objectContaining({ field: "Value", after: "0.5" }));
    expect(changes).toContainEqual(expect.objectContaining({ field: "Media version", after: "Content replaced" }));
    for (const hidden of ["secret-version-id", "b".repeat(64), "mask-id", "channel-id", "element_id", "canvas_pixels"]) expect(output).not.toContain(hidden);
    const deleted = structuredClone(after); deleted.sequences[0].captions = [];
    expect(historyChanges(after, deleted, "zh")).toContainEqual(expect.objectContaining({ object: "字幕 “After words”", kind: "removed", after: "不存在" }));
    expect(changeSummary(historyChanges(after, deleted, "zh"), "zh")).toContain("删除了 字幕 “After words”");
  });

  it("ignores JSON key order but distinguishes creation from a failed parent lookup", async () => {
    const snapshot = initialSnapshot(), after = structuredClone(snapshot.project);
    after.sequences[0].tracks[0].gain = { value: 0, unit: "decibels" };
    expect(historyChanges(snapshot.project, after, "en")).toEqual([]);
    const inspect = vi.fn().mockResolvedValue(snapshot);
    expect((await readHistoryPair(inspect, snapshot.project.id, null, "r0")).before).toBeNull();
    expect(inspect).toHaveBeenCalledTimes(1);
    await expect(readHistoryPair(inspect, snapshot.project.id, "missing-parent", "r0")).rejects.toThrow("identity");
    await expect(readHistoryPair(inspect, "different-project", null, "r0")).rejects.toThrow("identity");
  });

  it("attributes recorded accounts without inferring a human from the HTTP transport", () => {
    expect(actorLabel({ principal_id: "Travel Agent", source: "http" }, "en")).toBe("Account: Travel Agent · Connected app");
    expect(actorLabel({ principal_id: "local-owner", source: "cli" }, "zh")).toBe("本机账户 · 本机工具");
    expect(actorLabel(null, "zh")).toBe("操作者未记录");
    expect(actorLabel(undefined, "zh")).toBe("操作者信息暂不可用");
  });

  it("does not hide layout, font fallback, timing, freeform geometry or protected source changes", () => {
    const before = initialSnapshot().project;
    const clip = before.sequences[0].tracks[0].elements[0];
    clip.content = { kind:"video",asset_id:"asset",video_stream:0,source_audio_enabled:false,audio:{stream:null,gain:{unit:"decibels",value:0},muted:false},retime:{rate:{numerator:1,denominator:1},maintain_pitch:true,mapping:{kind:"exact_rational"}} };
    clip.visual!.masks.push({id:"custom-mask",enabled:true,inverted:false,feather:{unit:"classic_pixels",value:0},stroke:{color:{encoding:"srgb",red:1,green:1,blue:1,alpha:1},width_px:0,align:"inside"},shape:{kind:"freeform",path:{points:[{id:"point",x:0,y:0,in_x:0,in_y:0,out_x:0,out_y:0}],closed:false,center_x:0,center_y:0,rotation_degrees:0,scale:1}}});
    before.sequences[0].captions.push({id:"subtitle",track_id:"track",text:"Hello",timeline_range:clip.timeline_range,source_anchor:null,style:{font:{kind:"unresolved_family",family:"Arial",policy:{mode:"strict"}},font_size:{unit:"canvas_pixels",value:20},color:{encoding:"srgb",red:1,green:1,blue:1,alpha:1},background_color:null,max_width_px:null,line_height_px:null,text_align:"left",layout:"legacy_caption_v2"}});
    before.assets[0].origin={kind:"online",provider:"Media library",source_url:"https://media.example/private/before.mov"};
    const after=structuredClone(before), edited=after.sequences[0].tracks[0].elements[0];
    if(edited.content.kind==="video") edited.content.retime.mapping={kind:"classic_rounded_ticks",timebase:1000};
    const shape=edited.visual!.masks[0].shape;
    if(shape.kind==="freeform") shape.path.points[0].x=0.4;
    after.sequences[0].captions[0].style.layout="pinned_caption_v4";
    after.sequences[0].captions[0].style.font={kind:"unresolved_family",family:"Arial",policy:{mode:"explicit_fallback",families:["Noto Sans","Inter"]}};
    after.assets[0].origin={kind:"online",provider:"Media library",source_url:"https://media.example/private/after.mov"};
    const changes=historyChanges(before,after,"en");
    expect(changes).toContainEqual(expect.objectContaining({field:"Style · Layout mode",before:"Original subtitle layout",after:"Consistent subtitle layout"}));
    expect(changes).toContainEqual(expect.objectContaining({field:"Style · Font · Font matching · Matching mode",before:"Use the exact font",after:"Use a listed alternative if needed"}));
    expect(changes).toContainEqual(expect.objectContaining({field:"Style · Font · Font matching · Alternative fonts",after:"Noto Sans, Inter"}));
    expect(changes).toContainEqual(expect.objectContaining({field:"Playback · Timing mode · Type",before:"Exact speed timing",after:"Rounded speed timing"}));
    expect(changes).toContainEqual(expect.objectContaining({after:"0.4",field:expect.stringContaining("Horizontal position")}));
    expect(changes).toContainEqual(expect.objectContaining({object:"Media “Image”",field:"Source"}));
    expect(JSON.stringify(changes)).not.toContain("https:");
  });
});
