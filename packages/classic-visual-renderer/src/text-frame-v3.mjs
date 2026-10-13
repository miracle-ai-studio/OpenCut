import { renderFrame } from "./index.mjs";
import { composeTextFrame } from "./text-frame.mjs";
import { rasterizePreparedClassicText } from "./text-layout.mjs";
import { prepareCaptionTextV3 } from "./caption-layout-v3.mjs";
import { checkAbort } from "./validation.mjs";
import { CAPTION_FRAME_PROFILE_V3, validateCaptionFramePlanV3 } from "./caption-plan-v3.mjs";
export { CAPTION_FRAME_PROFILE_V3, validateCaptionFramePlanV3 } from "./caption-plan-v3.mjs";
/** Shared decode/font-resource/budget/GPU orchestration is unchanged. V3 adds
 * only caption raster preparation and a separately versioned bind message. */
export async function renderClassicCaptionFrameV3(fixedPlan, host, realm, signal, cache, decoders, fontCache) {
  const plan = structuredClone(fixedPlan); validateCaptionFramePlanV3(plan, host.admission);
  const directives = new Map(plan.captions.map(caption => [caption.elementId, caption]));
  const receipts = [];
  const hooks = {
    rasterize: (text, font, bytes, dimensions, rasterSignal, retained, layer) => rasterizePreparedClassicText(text, font, bytes, dimensions, rasterSignal, retained,
      directives.has(layer.elementId) ? (ctx, original, family, fixedFont) => {
        const prepared = prepareCaptionTextV3(ctx, original, family, fixedFont, directives.get(layer.elementId), dimensions);
        receipts.push({ elementId: layer.elementId, originalTextHash: fixedFont.validatedTextHash, wrappedText: prepared.wrappedText, measurements: prepared.measurements });
        return prepared;
      } : undefined),
    bind: rasters => host.callback("bindCaptionFrameV3", { rasters, captions: structuredClone(receipts) }),
  };
  const receipt = await renderFrame(plan.execution.media, host, realm, signal, cache, decoders,
    state => composeTextFrame(plan.execution, host, state, signal, hooks), fontCache);
  checkAbort(signal);
  return { identity: plan.identity, engineIdentity: CAPTION_FRAME_PROFILE_V3, execution: receipt, captions: receipts };
}
