/** Library artwork is a representative source thumbnail, not a composition render.
 * Media is read through the authenticated, fixed-version resource URL. No jobs. */
export function sourceThumbnail(url: string, kind: "image" | "video", signal: AbortSignal, time = 0): Promise<Blob> {
  signal.throwIfAborted();
  if (!Number.isFinite(time) || time < 0) return Promise.reject(Error("Source thumbnail time is invalid"));
  return new Promise((resolve, reject) => {
    const media = kind === "image" ? new Image() : document.createElement("video");
    let finished = false;
    const cleanup = () => { clearTimeout(timer); signal.removeEventListener("abort", abort); media.onload = null; media.onerror = null; if (media instanceof HTMLVideoElement) { media.onloadeddata = null; media.onseeked = null; media.pause(); media.removeAttribute("src"); media.load(); } else media.removeAttribute("src"); };
    const finish = (blob: Blob | null, error?: unknown) => { if (finished) return; finished = true; cleanup(); if (blob) resolve(blob); else reject(error ?? Error("Source thumbnail unavailable")); };
    const abort = () => finish(null, signal.reason);
    const timer = setTimeout(() => finish(null, Error("Source thumbnail timed out")), 10000);
    signal.addEventListener("abort", abort, { once: true });
    const draw = () => {
      if (finished) return;
      try {
        signal.throwIfAborted(); const width = media instanceof HTMLImageElement ? media.naturalWidth : media.videoWidth, height = media instanceof HTMLImageElement ? media.naturalHeight : media.videoHeight;
        if (!width || !height) throw Error("Source thumbnail has no image");
        const scale = Math.min(320 / width, 180 / height, 1), canvas = document.createElement("canvas"); canvas.width = Math.max(1, Math.round(width * scale)); canvas.height = Math.max(1, Math.round(height * scale));
        const context = canvas.getContext("2d"); if (!context) throw Error("Source thumbnail unavailable"); context.drawImage(media, 0, 0, canvas.width, canvas.height); canvas.toBlob(blob => finish(blob), "image/png");
      } catch(error) { finish(null, error); }
    };
    media.onerror = () => finish(null, Error("Source thumbnail unavailable"));
    if (media instanceof HTMLVideoElement) {
      media.muted = true; media.preload = "auto";
      media.onloadeddata = () => {
        if (time === 0) { draw(); return; }
        if (time >= media.duration) { finish(null, Error("Source thumbnail time is outside the video")); return; }
        media.onloadeddata = null;
        media.onseeked = draw;
        try { media.currentTime = time; } catch (error) { finish(null, error); }
      };
    } else media.onload = draw;
    media.src = url;
  });
}
