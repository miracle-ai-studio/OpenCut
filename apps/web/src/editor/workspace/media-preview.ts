import { useEffect, useState } from "react";
import type { Asset, Snapshot } from "../model/types";
import type { OpenCutClient } from "../../lib/editor-sdk";
import { publicJobResult } from "../../lib/editor-sdk";
import { useWorkspace } from "./context";
export type MediaPreview = { image?: string; peaks?: number[] };
const caches = new WeakMap<OpenCutClient, Map<string, Promise<MediaPreview>>>();
let active = 0;
const queue: (() => void)[] = [];
async function limited<T>(task: () => Promise<T>) {
  if (active >= 2) await new Promise<void>((resolve) => queue.push(resolve));
  active++;
  try {
    return await task();
  } finally {
    active--;
    queue.shift()?.();
  }
}
async function read(
  sdk: OpenCutClient,
  snapshot: Snapshot,
  asset: Asset,
): Promise<MediaPreview> {
  if (!asset.uri.startsWith("opencut-media://") || !asset.version) return {};
  if (asset.kind === "audio") {
    const job = await sdk.call("opencut_analysis_waveform", {
      project_id: snapshot.project.id,
      revision: snapshot.revision,
      asset_id: asset.id,
      buckets: 128,
      idempotency_key: `web-waveform-${asset.id}-${asset.version.content_hash}-${snapshot.revision}`,
    });
    const result = publicJobResult(
      await sdk.waitForPublicJob(job.job_id, { timeoutMs: 120000 }),
      "analysis.waveform",
    );
    return { peaks: result.analysis.buckets.map((bucket) => bucket.peak) };
  }
  if (asset.kind !== "video" && asset.kind !== "image") return {};
  const resource = await sdk.call("opencut_project_media_inspect", {
    project_id: snapshot.project.id,
    media_id: asset.uri.slice("opencut-media://".length),
  });
  if (resource.asset.version?.content_hash !== asset.version.content_hash)
    return {};
  const url = sdk.projectMediaUrl(resource);
  // The local application uses an HttpOnly session. Native media requests stream ranges.
  if (new URL(url).origin !== location.origin) return {};
  const media =
    asset.kind === "video" ? document.createElement("video") : new Image();
  try {
    await new Promise<void>((resolve, reject) => {
      const timer = setTimeout(
        () => reject(new Error("Media preview timed out")),
        15000,
      );
      media.addEventListener(
        asset.kind === "video" ? "loadeddata" : "load",
        () => {
          clearTimeout(timer);
          resolve();
        },
        { once: true },
      );
      media.addEventListener(
        "error",
        () => {
          clearTimeout(timer);
          reject(new Error("Preview unavailable"));
        },
        { once: true },
      );
      if (media instanceof HTMLVideoElement) {
        media.preload = "auto";
        media.muted = true;
      }
      media.src = url;
    });
    const canvas = document.createElement("canvas");
    canvas.width = 192;
    canvas.height = 108;
    const context = canvas.getContext("2d")!;
    const w =
      media instanceof HTMLVideoElement ? media.videoWidth : media.naturalWidth;
    const h =
      media instanceof HTMLVideoElement
        ? media.videoHeight
        : media.naturalHeight;
    if (!w || !h) return {};
    const scale = Math.min(canvas.width / w, canvas.height / h);
    context.drawImage(
      media,
      (192 - w * scale) / 2,
      (108 - h * scale) / 2,
      w * scale,
      h * scale,
    );
    return { image: canvas.toDataURL("image/webp", 0.7) };
  } finally {
    media.removeAttribute("src");
    if (media instanceof HTMLVideoElement) media.load();
  }
}
export function useMediaPreview(asset: Asset | undefined) {
  const { api, snapshot } = useWorkspace();
  const [value, setValue] = useState<MediaPreview>({});
  useEffect(() => {
    let live = true;
    setValue({});
    if (!api || !snapshot || !asset) return;
    let cache = caches.get(api.sdk);
    if (!cache) {
      cache = new Map();
      caches.set(api.sdk, cache);
    }
    const key = `${snapshot.project.id}/${asset.id}/${asset.version?.content_hash}`;
    let promise = cache.get(key);
    if (!promise) {
      promise = limited(() => read(api.sdk, snapshot, asset)).catch(() => ({}));
      cache.set(key, promise);
      if (cache.size > 128) cache.delete(cache.keys().next().value!);
    }
    void promise.then((result) => {
      if (live) setValue(result);
    });
    return () => {
      live = false;
    };
  }, [api, snapshot?.project.id, asset?.id, asset?.version?.content_hash]);
  return value;
}
