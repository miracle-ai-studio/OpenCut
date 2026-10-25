import { rasterizePreparedClassicText } from "./text-layout.mjs";
import { prepareCaptionText } from "./caption-layout.mjs";
// Shared by caption and graphic frames; the wrapping/measurement algorithm
// remains exclusively in caption-layout.mjs.
export function createCaptionHooks(plan, host) {
  const directives = new Map(plan.captions.map(caption => [caption.elementId, caption]));
  const receipts = [];
  const hooks = {
    rasterize: (text, font, bytes, dimensions, rasterSignal, retained, layer) => rasterizePreparedClassicText(text, font, bytes, dimensions, rasterSignal, retained,
      directives.has(layer.elementId) ? (ctx, original, family, fixedFont) => {
        const prepared = prepareCaptionText(ctx, original, family, fixedFont, directives.get(layer.elementId), dimensions);
        receipts.push({ elementId: layer.elementId, originalTextHash: fixedFont.validatedTextHash, wrappedText: prepared.wrappedText, measurements: prepared.measurements });
        return prepared;
      } : undefined),
    bind: rasters => host.callback("bindCaptionFrameV3", { rasters, captions: structuredClone(receipts) }),
  };
  return { hooks, receipts };
}
