import fs from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { runHost as sharedHost } from "@opencut/classic-audio-renderer/internal-host-runtime";
import { validateBegin, validateTextBegin, validateCaptionBeginV3, validateGraphicBeginV1, validateBlendBeginV1, validateMaskBeginV1, validateSceneBeginV1, validateAdmission } from "./protocol.mjs";
import { browserHooks } from "./browser.mjs";
export const runHost=(input=process.stdin,output=process.stdout)=>sharedHost(input,output,{validateBegin:packet=>packet.type==="beginSceneFrameV1"?validateSceneBeginV1(packet):packet.type==="beginMaskFrameV1"?validateMaskBeginV1(packet):packet.type==="beginBlendFrameV1"?validateBlendBeginV1(packet):packet.type==="beginGraphicFrameV1"?validateGraphicBeginV1(packet):packet.type==="beginCaptionFrameV3"?validateCaptionBeginV3(packet):packet.type==="beginTextFrameV2"?validateTextBegin(packet):validateBegin(packet),validateAdmission,browser:browserHooks,executionName:"visual"});
if(process.argv[1]&&await fs.realpath(process.argv[1]).catch(()=>undefined)===fileURLToPath(import.meta.url))await runHost();
